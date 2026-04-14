use core::ptr::{read_volatile, write_volatile};

use log::{debug, warn};
use redox_driver_sys::memory::{CacheType, MmioProt, MmioRegion};

use crate::acpi::{parse_ivrs, Bdf, IommuUnitInfo, IvrsError};
use crate::command_buffer::{CommandBuffer, CommandEntry, EventLog, EventLogEntry};
use crate::device_table::{DeviceTable, DeviceTableEntry, DEVICE_TABLE_ENTRIES};
use crate::interrupt::InterruptRemapTable;
use crate::mmio::{control, ext_feature, status, AmdViMmio, AMD_VI_MMIO_BYTES};
use crate::page_table::DomainPageTables;

const CMD_BUF_LEN_ENCODING: u64 = 0x09;
const EVT_LOG_LEN_ENCODING: u64 = 0x09;
const DEV_TABLE_SIZE_ENCODING: u64 = 0x0F;
const DEFAULT_CMD_ENTRIES: usize = 512;
const DEFAULT_EVT_ENTRIES: usize = 512;
const DEFAULT_IRT_ENTRIES: usize = 4096;
const COMPLETION_TOKEN: u32 = 0xA11D_F00D;

struct MmioMapping {
    region: MmioRegion,
    base: *mut AmdViMmio,
}

pub struct AmdViUnit {
    info: IommuUnitInfo,
    mmio: Option<MmioMapping>,
    device_table: Option<DeviceTable>,
    command_buffer: Option<CommandBuffer>,
    event_log: Option<EventLog>,
    interrupt_table: Option<InterruptRemapTable>,
    completion_store: Option<redox_driver_sys::dma::DmaBuffer>,
    command_tail: usize,
    event_head: usize,
    initialized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmdViEvent {
    pub unit_id: u8,
    pub event_code: u16,
    pub event_flags: u16,
    pub device_id: Bdf,
    pub address: u64,
}

impl AmdViUnit {
    pub fn detect(ivrs: &[u8]) -> Result<Vec<Self>, IvrsError> {
        let parsed = parse_ivrs(ivrs)?;
        Ok(parsed.units.into_iter().map(Self::from_info).collect())
    }

    pub fn from_info(info: IommuUnitInfo) -> Self {
        Self {
            info,
            mmio: None,
            device_table: None,
            command_buffer: None,
            event_log: None,
            interrupt_table: None,
            completion_store: None,
            command_tail: 0,
            event_head: 0,
            initialized: false,
        }
    }

    pub fn info(&self) -> &IommuUnitInfo {
        &self.info
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    pub fn handles_device(&self, bdf: Bdf) -> bool {
        self.info.handles_device(bdf)
    }

    pub fn init(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }

        let region = MmioRegion::map(
            self.info.mmio_base,
            AMD_VI_MMIO_BYTES,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )
        .map_err(|err| {
            format!(
                "failed to map AMD-Vi MMIO {:#x}: {err}",
                self.info.mmio_base
            )
        })?;
        let base = region.as_ptr() as *mut AmdViMmio;
        self.mmio = Some(MmioMapping { region, base });

        self.disable_unit()?;

        let device_table = DeviceTable::new().map_err(|err| err.to_string())?;
        let command_buffer =
            CommandBuffer::new(DEFAULT_CMD_ENTRIES).map_err(|err| err.to_string())?;
        let event_log = EventLog::new(DEFAULT_EVT_ENTRIES).map_err(|err| err.to_string())?;
        let interrupt_table =
            InterruptRemapTable::new(DEFAULT_IRT_ENTRIES).map_err(|err| err.to_string())?;

        self.program_bars(&device_table, &command_buffer, &event_log)?;
        self.reset_ring_pointers()?;

        self.device_table = Some(device_table);
        self.command_buffer = Some(command_buffer);
        self.event_log = Some(event_log);
        self.interrupt_table = Some(interrupt_table);

        let ext = self.mmio_read_extended_feature()?;
        let mut control_value = control::EVENT_LOG_EN | control::CMD_BUF_EN;
        if ext & ext_feature::XT_SUP != 0 {
            control_value |= control::XT_EN;
        }
        if ext & ext_feature::NX_SUP != 0 {
            control_value |= control::NX_EN;
        }
        unsafe {
            AmdViMmio::write_control(self.mmio_base()?, control_value);
        }

        self.flush_configuration()?;

        unsafe {
            AmdViMmio::write_control(self.mmio_base()?, control_value | control::IOMMU_ENABLE);
        }
        self.wait_for_running(true)?;
        self.initialized = true;
        Ok(())
    }

    pub fn assign_device(&mut self, bdf: Bdf, domain: &DomainPageTables) -> Result<(), String> {
        if !self.initialized {
            return Err("AMD-Vi unit is not initialized".to_string());
        }
        if !self.handles_device(bdf) {
            return Err(format!(
                "AMD-Vi unit {} does not cover device {bdf}",
                self.info.unit_id()
            ));
        }

        let interrupt_table = self
            .interrupt_table
            .as_ref()
            .ok_or_else(|| "interrupt remap table not initialized".to_string())?;
        let device_table = self
            .device_table
            .as_mut()
            .ok_or_else(|| "device table not initialized".to_string())?;

        let mut entry = DeviceTableEntry::new();
        entry.set_valid(true);
        entry.set_translation_valid(true);
        entry.set_read_permission(true);
        entry.set_write_permission(true);
        entry.set_mode(domain.levels());
        entry.set_page_table_root(domain.root_address());
        entry.set_interrupt_remap(true);
        entry.set_interrupt_write(true);
        entry.set_interrupt_control(0x02);
        entry.set_int_table_len(interrupt_table.len_encoding());
        entry.set_int_remap_table_ptr(interrupt_table.physical_address() as u64);

        device_table.set_entry(bdf.raw(), &entry);
        self.submit_command(CommandEntry::invalidate_devtab_entry(bdf.raw()))?;
        self.submit_command(CommandEntry::invalidate_interrupt_table(bdf.raw()))?;
        self.wait_for_completion()?;
        Ok(())
    }

    pub fn drain_events(&mut self) -> Result<Vec<AmdViEvent>, String> {
        let mut drained = Vec::new();
        if !self.initialized {
            return Ok(drained);
        }

        let base = self.mmio_base()?;
        let event_log = self
            .event_log
            .as_ref()
            .ok_or_else(|| "event log not initialized".to_string())?;
        let tail = unsafe { AmdViMmio::read_evt_log_tail(base) as usize % event_log.capacity() };

        while self.event_head != tail {
            let event = event_log.read_entry(self.event_head);
            drained.push(self.decode_event(event));
            self.event_head = (self.event_head + 1) % event_log.capacity();
        }

        unsafe {
            AmdViMmio::write_evt_log_head(base, self.event_head as u64);
        }
        Ok(drained)
    }

    fn decode_event(&self, event: EventLogEntry) -> AmdViEvent {
        AmdViEvent {
            unit_id: self.info.unit_id(),
            event_code: event.event_type() as u16,
            event_flags: event.event_flags(),
            device_id: Bdf(event.device_id()),
            address: event.virtual_address(),
        }
    }

    fn disable_unit(&mut self) -> Result<(), String> {
        let base = self.mmio_base()?;
        unsafe {
            AmdViMmio::write_control(base, 0);
        }
        self.wait_for_running(false)
    }

    fn wait_for_running(&self, expected: bool) -> Result<(), String> {
        let base = self.mmio_base()?;
        for _ in 0..100_000 {
            let running = unsafe { AmdViMmio::read_status(base) } & status::IOMMU_RUNNING != 0;
            if running == expected {
                return Ok(());
            }
            std::hint::spin_loop();
        }

        Err(format!(
            "timed out waiting for AMD-Vi unit {} running={expected}",
            self.info.unit_id()
        ))
    }

    fn program_bars(
        &mut self,
        device_table: &DeviceTable,
        command_buffer: &CommandBuffer,
        event_log: &EventLog,
    ) -> Result<(), String> {
        let base = self.mmio_base()?;
        unsafe {
            AmdViMmio::write_dev_table_bar(
                base,
                (device_table.physical_address() as u64 & !0xFFF) | DEV_TABLE_SIZE_ENCODING,
            );
            AmdViMmio::write_cmd_buf_bar(
                base,
                (command_buffer.physical_address() as u64 & !0xFFF) | CMD_BUF_LEN_ENCODING,
            );
            AmdViMmio::write_evt_log_bar(
                base,
                (event_log.physical_address() as u64 & !0xFFF) | EVT_LOG_LEN_ENCODING,
            );
            AmdViMmio::write_exclusion_base(base, 0);
            AmdViMmio::write_exclusion_limit(base, 0);
        }
        Ok(())
    }

    fn reset_ring_pointers(&mut self) -> Result<(), String> {
        let base = self.mmio_base()?;
        unsafe {
            AmdViMmio::write_cmd_buf_head(base, 0);
            AmdViMmio::write_cmd_buf_tail(base, 0);
            AmdViMmio::write_evt_log_head(base, 0);
        }
        self.command_tail = 0;
        self.event_head = 0;
        Ok(())
    }

    fn flush_configuration(&mut self) -> Result<(), String> {
        let ext = self.mmio_read_extended_feature()?;
        if ext & ext_feature::IA_SUP != 0 {
            self.submit_command(CommandEntry::invalidate_all())?;
        } else if let Some(table) = self.device_table.as_ref() {
            let mut pending_invalidations = Vec::new();
            for device_id in 0..DEVICE_TABLE_ENTRIES {
                let entry = table.get_entry(device_id as u16);
                if entry.valid() {
                    pending_invalidations.push(device_id as u16);
                }
            }
            for device_id in pending_invalidations {
                self.submit_command(CommandEntry::invalidate_devtab_entry(device_id))?;
            }
        } else {
            warn!("amd-vi: device table not yet allocated while flushing configuration");
        }
        self.wait_for_completion()
    }

    fn submit_command(&mut self, command: CommandEntry) -> Result<(), String> {
        let base = self.mmio_base()?;
        let command_buffer = self
            .command_buffer
            .as_mut()
            .ok_or_else(|| "command buffer not initialized".to_string())?;

        let head =
            unsafe { AmdViMmio::read_cmd_buf_head(base) as usize % command_buffer.capacity() };
        let next_tail = (self.command_tail + 1) % command_buffer.capacity();
        if next_tail == head {
            return Err("AMD-Vi command buffer is full".to_string());
        }

        command_buffer.write_command(self.command_tail, &command);
        self.command_tail = next_tail;
        unsafe {
            AmdViMmio::write_cmd_buf_tail(base, self.command_tail as u64);
        }
        Ok(())
    }

    fn wait_for_completion(&mut self) -> Result<(), String> {
        let completion_store = match self.completion_store.take() {
            Some(buffer) => buffer,
            None => redox_driver_sys::dma::DmaBuffer::allocate(8, 8)
                .map_err(|err| format!("failed to allocate completion wait store: {err}"))?,
        };

        let completion_ptr = completion_store.as_ptr() as *const u32;
        let completion_mut = completion_store.as_ptr() as *mut u32;
        unsafe {
            write_volatile(completion_mut, 0);
        }
        let completion_phys = completion_store.physical_address() as u64;
        self.submit_command(CommandEntry::completion_wait(
            completion_phys,
            COMPLETION_TOKEN,
        ))?;

        for _ in 0..100_000 {
            if unsafe { read_volatile(completion_ptr) } == COMPLETION_TOKEN {
                self.completion_store = Some(completion_store);
                return Ok(());
            }
            std::hint::spin_loop();
        }

        self.completion_store = Some(completion_store);
        Err("timed out waiting for AMD-Vi command completion".to_string())
    }

    fn mmio_read_extended_feature(&self) -> Result<u64, String> {
        let base = self.mmio_base()?;
        Ok(unsafe { AmdViMmio::read_extended_feature(base) })
    }

    fn mmio_base(&self) -> Result<*mut AmdViMmio, String> {
        self.mmio
            .as_ref()
            .map(|mapping| mapping.base)
            .ok_or_else(|| "AMD-Vi MMIO is not mapped".to_string())
    }
}

impl Drop for AmdViUnit {
    fn drop(&mut self) {
        if let Some(mapping) = &self.mmio {
            debug!(
                "amd-vi: dropping unit {} mapped at {:#x} ({:#x} bytes)",
                self.info.unit_id(),
                self.info.mmio_base,
                mapping.region.size()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::acpi::Bdf;

    use super::AmdViUnit;

    fn build_ivrs_with_unit() -> Vec<u8> {
        let mut table = vec![0u8; 40 + 28];
        table[0..4].copy_from_slice(b"IVRS");
        table[4..8].copy_from_slice(&(68u32).to_le_bytes());
        table[8] = 3;
        table[10..16].copy_from_slice(b"RDBEAR");
        table[16..24].copy_from_slice(b"AMDVI   ");

        let offset = 40;
        table[offset] = 0x11;
        table[offset + 1] = 0x20;
        table[offset + 2..offset + 4].copy_from_slice(&(28u16).to_le_bytes());
        table[offset + 4..offset + 6].copy_from_slice(&Bdf::new(0, 0x18, 2).raw().to_le_bytes());
        table[offset + 6..offset + 8].copy_from_slice(&0x40u16.to_le_bytes());
        table[offset + 8..offset + 16].copy_from_slice(&0xfee0_0000u64.to_le_bytes());
        table[offset + 16..offset + 18].copy_from_slice(&0u16.to_le_bytes());
        table[offset + 18..offset + 20].copy_from_slice(&0x0081u16.to_le_bytes());
        table[offset + 20..offset + 24].copy_from_slice(&0u32.to_le_bytes());
        table[offset + 24..offset + 28].copy_from_slice(&[0x00, 0, 0, 0]);

        let checksum =
            (!table.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))).wrapping_add(1);
        table[9] = checksum;
        table
    }

    #[test]
    fn detect_builds_units_from_ivrs() {
        let units = AmdViUnit::detect(&build_ivrs_with_unit())
            .unwrap_or_else(|err| panic!("amd-vi detect failed: {err}"));
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].info().mmio_base, 0xfee0_0000);
        assert!(units[0].handles_device(Bdf::new(0x80, 0x1f, 7)));
    }
}
