#include "redoxfs.h"

static inline grub_uint64_t
seahash_diffuse (grub_uint64_t x)
{
  grub_uint64_t a;
  grub_uint64_t b;

  x *= 0x6eed0e9da4d94a4fULL;
  a = x >> 32;
  b = x >> 60;
  x ^= a >> b;
  x *= 0x6eed0e9da4d94a4fULL;
  return x;
}

static inline grub_uint64_t
seahash_read_le64 (const grub_uint8_t *buf, grub_size_t len)
{
  grub_uint64_t x = 0;
  grub_size_t i;

  for (i = 0; i < len; i++)
    x |= (grub_uint64_t) buf[i] << (8 * i);

  return x;
}

grub_uint64_t
grub_redoxfs_seahash (const void *data, grub_size_t size)
{
  const grub_uint8_t *buf = (const grub_uint8_t *) data;
  grub_uint64_t a = 0x16f11fe89b0d677cULL;
  grub_uint64_t b = 0xb480a793d8e6c86cULL;
  grub_uint64_t c = 0x6fe2e5aaf078ebc9ULL;
  grub_uint64_t d = 0x14f994a4c5259381ULL;
  grub_size_t i = 0;

  for (; i + 8 <= size; i += 8)
    {
      grub_uint64_t n = seahash_read_le64 (buf + i, 8);
      grub_uint64_t new_a = seahash_diffuse (a ^ n);

      a = b;
      b = c;
      c = d;
      d = new_a;
    }

  if (i < size)
    {
      grub_uint64_t n = seahash_read_le64 (buf + i, size - i);
      grub_uint64_t new_a = seahash_diffuse (a ^ n);

      a = b;
      b = c;
      c = d;
      d = new_a;
    }

  return seahash_diffuse (a ^ b ^ c ^ d ^ (grub_uint64_t) size);
}
