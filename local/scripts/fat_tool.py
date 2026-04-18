#!/usr/bin/env python3
"""Minimal FAT32 read/write tool for modifying ESP partitions in disk images.

Supports: ls, mkdir, cp-in, cp-out
No external dependencies — uses only struct/os standard library.

Usage:
    fat_tool.py ls <image> <offset> [path]
    fat_tool.py mkdir <image> <offset> <path>
    fat_tool.py cp-in <image> <offset> <host_path> <fat_path>
    fat_tool.py cp-out <image> <offset> <fat_path> <host_path>
"""

import os
import struct
import sys
from datetime import datetime


ATTR_VOLUME_ID = 0x08
ATTR_DIRECTORY = 0x10
ATTR_ARCHIVE = 0x20
ATTR_LFN = 0x0F
END_OF_CHAIN = 0x0FFFFFF8
SHORT_NAME_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789$%'-_@~`!(){}^#&"
LFN_CHAR_OFFSETS = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]


def read_le16(data, off):
    return struct.unpack_from("<H", data, off)[0]


def read_le32(data, off):
    return struct.unpack_from("<I", data, off)[0]


def write_le16(data, off, val):
    struct.pack_into("<H", data, off, val)


def write_le32(data, off, val):
    struct.pack_into("<I", data, off, val)


class Fat32:
    def __init__(self, image_path, offset):
        self.f = open(image_path, "r+b")
        self.image_size = os.fstat(self.f.fileno()).st_size
        self.offset = offset
        try:
            self._read_bpb()
            self._load_fat()
        except Exception:
            self.f.close()
            raise

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
        return False

    def _read_bpb(self):
        self.f.seek(self.offset)
        bpb = self.f.read(512)

        self.bytes_per_sector = read_le16(bpb, 11)
        self.sectors_per_cluster = bpb[13]
        self.reserved_sectors = read_le16(bpb, 14)
        self.num_fats = bpb[16]
        self.root_entry_count = read_le16(bpb, 17)
        self.total_sectors_16 = read_le16(bpb, 19)
        self.media = bpb[21]
        self.fat_size_16 = read_le16(bpb, 22)
        self.total_sectors_32 = read_le32(bpb, 32)
        self.fat_size_32 = read_le32(bpb, 36)
        self.root_cluster = read_le32(bpb, 44)

        self.total_sectors = self.total_sectors_32 or self.total_sectors_16
        self.fat_size = self.fat_size_32 or self.fat_size_16
        if self.bytes_per_sector == 0 or self.sectors_per_cluster == 0:
            raise RuntimeError("FAT32: invalid BPB geometry")
        if self.num_fats == 0 or self.total_sectors == 0 or self.fat_size == 0:
            raise RuntimeError("FAT32: invalid BPB layout")

        self.first_data_sector = self.reserved_sectors + self.num_fats * self.fat_size
        self.data_sectors = self.total_sectors - self.first_data_sector
        if self.data_sectors <= 0:
            raise RuntimeError("FAT32: invalid data region")

        self.cluster_count = self.data_sectors // self.sectors_per_cluster
        self.max_cluster = self.cluster_count + 1
        self.cluster_size = self.bytes_per_sector * self.sectors_per_cluster
        self.fat_start = self.offset + self.reserved_sectors * self.bytes_per_sector
        self.data_start = self.fat_start + self.num_fats * self.fat_size * self.bytes_per_sector

        if self.cluster_count == 0 or not 2 <= self.root_cluster <= self.max_cluster:
            raise RuntimeError("FAT32: invalid root cluster")

        fat_bytes = self.fat_size * self.bytes_per_sector
        if (self.max_cluster + 1) * 4 > fat_bytes:
            raise RuntimeError(
                f"FAT32: FAT too small ({fat_bytes} bytes) for {self.max_cluster} clusters"
            )

        data_end = self.data_start + self.cluster_count * self.cluster_size
        if self.data_start > self.image_size or data_end > self.image_size:
            raise RuntimeError("FAT32: filesystem exceeds backing image")

    def _load_fat(self):
        self.f.seek(self.fat_start)
        fat_bytes = self.fat_size * self.bytes_per_sector
        self.fat = bytearray(self.f.read(fat_bytes))

    def _flush_fat(self):
        for i in range(self.num_fats):
            self.f.seek(self.fat_start + i * self.fat_size * self.bytes_per_sector)
            self.f.write(self.fat)

    def sync(self):
        self.f.flush()
        os.fsync(self.f.fileno())

    def _cluster_offset(self, cluster):
        if not 2 <= cluster <= self.max_cluster:
            raise RuntimeError(f"FAT32: invalid cluster {cluster}")
        return self.data_start + (cluster - 2) * self.cluster_size

    def _read_cluster(self, cluster):
        self.f.seek(self._cluster_offset(cluster))
        return bytearray(self.f.read(self.cluster_size))

    def _write_cluster(self, cluster, data):
        if len(data) > self.cluster_size:
            raise RuntimeError(f"_write_cluster: data size {len(data)} exceeds cluster size {self.cluster_size}")
        self.f.seek(self._cluster_offset(cluster))
        self.f.write(data[: self.cluster_size])

    def _next_cluster(self, cluster):
        if not 2 <= cluster <= self.max_cluster:
            raise RuntimeError(f"FAT32: invalid cluster {cluster}")
        idx = cluster * 4
        val = read_le32(self.fat, idx) & 0x0FFFFFFF
        if val == 0x0FFFFFF7:
            raise RuntimeError(f"FAT32: bad cluster marker at {cluster}")
        return val

    def _set_fat(self, cluster, value):
        write_le32(self.fat, cluster * 4, value & 0x0FFFFFFF)

    def _alloc_cluster(self):
        fat_entries = len(self.fat) // 4
        limit = min(self.max_cluster + 1, fat_entries)
        for i in range(2, limit):
            if read_le32(self.fat, i * 4) == 0:
                self._set_fat(i, END_OF_CHAIN)
                self._flush_fat()
                self.f.seek(self._cluster_offset(i))
                self.f.write(b"\x00" * self.cluster_size)
                self.f.flush()
                return i
        raise RuntimeError("FAT32: no free clusters")

    def _cluster_chain(self, start):
        if start < 2:
            return []

        chain = []
        seen = set()
        cluster = start
        while 2 <= cluster <= self.max_cluster and cluster < END_OF_CHAIN and cluster not in seen:
            chain.append(cluster)
            seen.add(cluster)
            cluster = self._next_cluster(cluster)
        return chain

    def _split_name(self, name):
        if "." in name:
            return name.rsplit(".", 1)
        return name, ""

    def _normalize_name(self, name):
        return name.upper()

    def _sanitize_short_component(self, text, max_len):
        chars = []
        for ch in text.upper():
            if ch in SHORT_NAME_CHARS:
                chars.append(ch)
            elif ch in " .":
                continue
            else:
                chars.append("_")
            if len(chars) == max_len:
                break
        return "".join(chars)

    def _short_name_bytes(self, base, ext):
        return (base.ljust(8) + ext.ljust(3)).encode("ascii")

    def _decode_short_name(self, name_bytes):
        base = name_bytes[:8].decode("ascii", errors="replace").rstrip()
        ext = name_bytes[8:11].decode("ascii", errors="replace").rstrip()
        return f"{base}.{ext}" if ext else base

    def _short_name_checksum(self, short_bytes):
        checksum = 0
        for byte in short_bytes:
            checksum = ((checksum >> 1) + ((checksum & 1) << 7) + byte) & 0xFF
        return checksum

    def _needs_lfn(self, name):
        if name in (".", ".."):
            return False

        base, ext = self._split_name(name)
        if not base or len(base) > 8 or len(ext) > 3:
            return True
        if name != name.upper():
            return True

        return (
            self._sanitize_short_component(base, 8) != base
            or self._sanitize_short_component(ext, 3) != ext
        )

    def _make_short_name(self, parent_cluster, name):
        existing = set()
        for entry in self._dir_entries(parent_cluster):
            existing.add(self._normalize_name(entry["short_name"]))

        base, ext = self._split_name(name)
        clean_base = self._sanitize_short_component(base, 8) or "_"
        clean_ext = self._sanitize_short_component(ext, 3)
        needs_lfn = self._needs_lfn(name)

        if not needs_lfn:
            short = self._short_name_bytes(clean_base, clean_ext)
            if self._normalize_name(self._decode_short_name(short)) not in existing:
                return short, False
            needs_lfn = True

        stem_source = self._sanitize_short_component(base, 8) or "_"
        for index in range(1, 1000000):
            suffix = f"~{index}"
            stem = (stem_source[: max(1, 8 - len(suffix))] + suffix)[:8]
            short = self._short_name_bytes(stem, clean_ext)
            if self._normalize_name(self._decode_short_name(short)) not in existing:
                return short, needs_lfn

        raise RuntimeError(f"cannot generate unique short name for '{name}'")

    def _read_lfn_fragment(self, entry):
        chars = []
        for offset in LFN_CHAR_OFFSETS:
            code_unit = read_le16(entry, offset)
            if code_unit == 0x0000:
                break
            if code_unit != 0xFFFF:
                chars.append(chr(code_unit))
        return "".join(chars)

    def _assemble_lfn(self, parts):
        if not parts:
            return None

        name = []
        for index in range(1, max(parts) + 1):
            part = parts.get(index)
            if part is None:
                return None
            name.append(part)
        return "".join(name)

    def _lfn_chunks(self, name):
        encoded = name.encode("utf-16-le")
        code_units = []
        for offset in range(0, len(encoded), 2):
            code_units.append(read_le16(encoded, offset))
        code_units.append(0x0000)

        chunks = []
        for offset in range(0, len(code_units), 13):
            chunk = code_units[offset : offset + 13]
            while len(chunk) < 13:
                chunk.append(0xFFFF)
            chunks.append(chunk)
        return chunks

    def _dir_entries(self, cluster):
        lfn_parts = {}
        lfn_offsets = []
        lfn_checksum = None

        for current_cluster in self._cluster_chain(cluster):
            data = self._read_cluster(current_cluster)
            base = self._cluster_offset(current_cluster)
            for i in range(0, self.cluster_size, 32):
                entry = data[i : i + 32]
                entry_offset = base + i
                first_byte = entry[0]

                if first_byte == 0x00:
                    return

                if first_byte == 0xE5:
                    lfn_parts = {}
                    lfn_offsets = []
                    lfn_checksum = None
                    continue

                attr = entry[11]
                if attr == ATTR_LFN:
                    seq = entry[0]
                    seq_num = seq & 0x1F
                    if seq & 0x40:
                        lfn_parts = {}
                        lfn_offsets = []
                        lfn_checksum = entry[13]
                    if seq_num == 0:
                        lfn_parts = {}
                        lfn_offsets = []
                        lfn_checksum = None
                        continue
                    lfn_parts[seq_num] = self._read_lfn_fragment(entry)
                    lfn_offsets.append(entry_offset)
                    continue

                if attr & ATTR_VOLUME_ID:
                    lfn_parts = {}
                    lfn_offsets = []
                    lfn_checksum = None
                    continue

                short_bytes = bytes(entry[0:11])
                short_name = self._decode_short_name(short_bytes)
                name = short_name
                is_lfn = False

                if lfn_parts and lfn_checksum == self._short_name_checksum(short_bytes):
                    full_name = self._assemble_lfn(lfn_parts)
                    if full_name:
                        name = full_name
                        is_lfn = True

                first_cluster = (read_le16(entry, 20) << 16) | read_le16(entry, 26)
                size = read_le32(entry, 28)
                is_dir = bool(attr & ATTR_DIRECTORY)

                yield {
                    "name": name,
                    "short_name": short_name,
                    "short_bytes": short_bytes,
                    "entry_offset": entry_offset,
                    "slots": list(lfn_offsets) + [entry_offset],
                    "first_cluster": first_cluster,
                    "size": size,
                    "is_dir": is_dir,
                    "is_lfn": is_lfn,
                }

                lfn_parts = {}
                lfn_offsets = []
                lfn_checksum = None

    def _find_in_dir(self, parent_cluster, name):
        target = self._normalize_name(name)
        for entry in self._dir_entries(parent_cluster):
            if self._normalize_name(entry["name"]) == target:
                return entry
            if self._normalize_name(entry["short_name"]) == target:
                return entry
        return None

    def _resolve_path(self, path):
        """Return (parent_cluster, entry_or_None) for a path like 'EFI/BOOT/BOOTX64.EFI'."""
        parts = [part for part in path.replace("\\", "/").split("/") if part]
        current = self.root_cluster
        if not parts:
            return current, None

        for index, part in enumerate(parts):
            found = self._find_in_dir(current, part)
            if found is None:
                return current, None
            if index == len(parts) - 1:
                return current, found
            if not found["is_dir"]:
                raise RuntimeError(f"'{part}' is not a directory")
            current = found["first_cluster"]

        return current, None

    def _timestamp_values(self):
        now = datetime.now()
        year = min(max(now.year, 1980), 2107)
        date_val = ((year - 1980) << 9) | (now.month << 5) | now.day
        time_val = (now.hour << 11) | (now.minute << 5) | (now.second // 2)
        return date_val, time_val

    def _build_short_entry(self, short_name, first_cluster, is_dir, size):
        entry = bytearray(32)
        entry[0:11] = short_name
        entry[11] = ATTR_DIRECTORY if is_dir else ATTR_ARCHIVE
        date_val, time_val = self._timestamp_values()
        write_le16(entry, 14, time_val)
        write_le16(entry, 16, date_val)
        write_le16(entry, 18, date_val)
        write_le16(entry, 20, (first_cluster >> 16) & 0xFFFF)
        write_le16(entry, 22, time_val)
        write_le16(entry, 24, date_val)
        write_le16(entry, 26, first_cluster & 0xFFFF)
        write_le32(entry, 28, size)
        return entry

    def _find_free_dir_slots(self, parent_cluster, entries_needed):
        run = []
        seen_end = False
        chain = self._cluster_chain(parent_cluster)

        for current_cluster in chain:
            data = self._read_cluster(current_cluster)
            base = self._cluster_offset(current_cluster)
            for i in range(0, self.cluster_size, 32):
                marker = data[i]
                entry_offset = base + i

                if seen_end or marker in (0x00, 0xE5):
                    run.append(entry_offset)
                    if marker == 0x00:
                        seen_end = True
                    if len(run) == entries_needed:
                        return run
                else:
                    run = []

        last_cluster = chain[-1]
        while len(run) < entries_needed:
            new_cluster = self._alloc_cluster()
            self._set_fat(last_cluster, new_cluster)
            self._set_fat(new_cluster, END_OF_CHAIN)
            self._flush_fat()

            for i in range(0, self.cluster_size, 32):
                run.append(self._cluster_offset(new_cluster) + i)
                if len(run) == entries_needed:
                    return run

            last_cluster = new_cluster

        return run

    def _add_dir_entry(self, parent_cluster, name, first_cluster, is_dir, size=0):
        short_name, needs_lfn = self._make_short_name(parent_cluster, name)
        lfn_chunks = self._lfn_chunks(name) if needs_lfn else []
        slots = self._find_free_dir_slots(parent_cluster, len(lfn_chunks) + 1)
        checksum = self._short_name_checksum(short_name)

        for slot_index, seq in enumerate(range(len(lfn_chunks), 0, -1)):
            entry = bytearray(32)
            entry[0] = seq | (0x40 if seq == len(lfn_chunks) else 0)
            entry[11] = ATTR_LFN
            entry[13] = checksum

            chunk = lfn_chunks[seq - 1]
            for char_offset, code_unit in zip(LFN_CHAR_OFFSETS, chunk):
                write_le16(entry, char_offset, code_unit)

            self.f.seek(slots[slot_index])
            self.f.write(entry)

        self.f.seek(slots[len(lfn_chunks)])
        self.f.write(self._build_short_entry(short_name, first_cluster, is_dir, size))
        self.f.flush()

    def _initialize_directory(self, cluster, parent_cluster):
        data = bytearray(self.cluster_size)
        data[0:32] = self._build_short_entry(b".          ", cluster, True, 0)
        data[32:64] = self._build_short_entry(b"..         ", parent_cluster, True, 0)
        self._write_cluster(cluster, data)
        self.f.flush()

    def _free_cluster_chain(self, start_cluster):
        for cluster in self._cluster_chain(start_cluster):
            self._set_fat(cluster, 0)
        self._flush_fat()

    def _delete_entry(self, entry):
        for slot in entry["slots"]:
            self.f.seek(slot)
            self.f.write(b"\xE5")
        if entry["first_cluster"] >= 2:
            self._free_cluster_chain(entry["first_cluster"])
        self.f.flush()

    # Public API

    def ls(self, path="/"):
        normalized_path = path if path != "/" else ""
        parent_cluster, found = self._resolve_path(normalized_path)

        if normalized_path and found is None:
            raise RuntimeError(f"ls: '{path}' not found")

        if found is not None and not found["is_dir"]:
            print(f"- {found['size']:>10} {found['name']}")
            return

        cluster = found["first_cluster"] if found is not None else self.root_cluster
        for entry in self._dir_entries(cluster):
            if entry["name"] in (".", ".."):
                continue
            prefix = "d" if entry["is_dir"] else "-"
            print(f"{prefix} {entry['size']:>10} {entry['name']}")

    def mkdir(self, path):
        parts = [part for part in path.replace("\\", "/").split("/") if part]
        if not parts:
            raise RuntimeError("mkdir: empty path")

        current_cluster = self.root_cluster
        for part in parts:
            found = self._find_in_dir(current_cluster, part)
            if found is not None:
                if not found["is_dir"]:
                    raise RuntimeError(f"mkdir: '{part}' already exists and is not a directory")
                current_cluster = found["first_cluster"]
                continue

            new_cluster = self._alloc_cluster()
            try:
                self._initialize_directory(new_cluster, current_cluster)
                self._add_dir_entry(current_cluster, part, new_cluster, True)
            except Exception:
                self._free_cluster_chain(new_cluster)
                raise
            current_cluster = new_cluster

    def cp_in(self, host_path, fat_path):
        with open(host_path, "rb") as host_file:
            data = host_file.read()

        parts = [part for part in fat_path.replace("\\", "/").split("/") if part]
        if not parts:
            raise RuntimeError("cp-in: need destination path")

        parent_path = "/".join(parts[:-1])
        file_name = parts[-1]

        if parent_path:
            _, parent_entry = self._resolve_path(parent_path)
            if parent_entry is None or not parent_entry["is_dir"]:
                raise RuntimeError(f"cp-in: directory '{parent_path}' not found")
            parent_cluster = parent_entry["first_cluster"]
            if parent_cluster < 2:
                raise RuntimeError(f"cp-in: parent cluster {parent_cluster} is invalid")
        else:
            parent_cluster = self.root_cluster

        existing = self._find_in_dir(parent_cluster, file_name)
        if existing is not None:
            if existing["is_dir"]:
                raise RuntimeError(f"cp-in: '{file_name}' is a directory")

        cluster_count = (len(data) + self.cluster_size - 1) // self.cluster_size
        clusters = []
        try:
            for _ in range(cluster_count):
                clusters.append(self._alloc_cluster())

            if clusters:
                for i in range(len(clusters) - 1):
                    self._set_fat(clusters[i], clusters[i + 1])
                self._set_fat(clusters[-1], END_OF_CHAIN)
                self._flush_fat()

                for index, cluster in enumerate(clusters):
                    chunk = data[index * self.cluster_size : (index + 1) * self.cluster_size]
                    buffer = bytearray(self.cluster_size)
                    buffer[: len(chunk)] = chunk
                    self._write_cluster(cluster, buffer)

            first_cluster = clusters[0] if clusters else 0
            if existing is not None:
                replacement = self._build_short_entry(
                    existing["short_bytes"], first_cluster, False, len(data)
                )
                self.f.seek(existing["entry_offset"])
                self.f.write(replacement)
                if existing["first_cluster"] >= 2:
                    self._free_cluster_chain(existing["first_cluster"])
            else:
                self._add_dir_entry(parent_cluster, file_name, first_cluster, False, len(data))
            self.f.flush()
            os.fsync(self.f.fileno())
        except Exception:
            if clusters:
                self._free_cluster_chain(clusters[0])
            raise

    def cp_out(self, fat_path, host_path):
        _, found = self._resolve_path(fat_path)
        if found is None:
            raise RuntimeError(f"cp-out: '{fat_path}' not found")
        if found["is_dir"]:
            raise RuntimeError(f"cp-out: '{fat_path}' is a directory")

        data = bytearray()
        for cluster in self._cluster_chain(found["first_cluster"]):
            data.extend(self._read_cluster(cluster))

        with open(host_path, "wb") as host_file:
            host_file.write(data[: found["size"]])

    def close(self):
        self.f.close()


def main():
    if len(sys.argv) < 4:
        print(__doc__)
        sys.exit(1)

    cmd = sys.argv[1]
    image = sys.argv[2]
    offset = int(sys.argv[3])

    fat = Fat32(image, offset)
    try:
        if cmd == "ls":
            path = sys.argv[4] if len(sys.argv) > 4 else "/"
            fat.ls(path)
        elif cmd == "mkdir":
            if len(sys.argv) != 5:
                print(__doc__)
                sys.exit(1)
            fat.mkdir(sys.argv[4])
        elif cmd == "cp-in":
            if len(sys.argv) != 6:
                print(__doc__)
                sys.exit(1)
            fat.cp_in(sys.argv[4], sys.argv[5])
        elif cmd == "cp-out":
            if len(sys.argv) != 6:
                print(__doc__)
                sys.exit(1)
            fat.cp_out(sys.argv[4], sys.argv[5])
        else:
            print(f"Unknown command: {cmd}")
            sys.exit(1)
    finally:
        fat.close()


if __name__ == "__main__":
    main()
