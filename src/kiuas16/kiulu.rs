use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::{
    Kiuas16Bus, PhysicalAccessError, VIRTIO_BLOCK_DEVICE_ID, VIRTIO_MMIO_MAGIC,
    VIRTIO_MMIO_VENDOR_ID, VIRTIO_MMIO_VERSION, VIRTIO_SECTOR_BYTES,
};

pub const QUEUE_MAX: u32 = 256;
pub const MMIO_MAGIC_VALUE: u64 = 0x000;
pub const MMIO_VERSION: u64 = 0x004;
pub const MMIO_DEVICE_ID: u64 = 0x008;
pub const MMIO_VENDOR_ID: u64 = 0x00c;
pub const MMIO_DEVICE_FEATURES: u64 = 0x010;
pub const MMIO_DEVICE_FEATURES_SEL: u64 = 0x014;
pub const MMIO_DRIVER_FEATURES: u64 = 0x020;
pub const MMIO_DRIVER_FEATURES_SEL: u64 = 0x024;
pub const MMIO_QUEUE_SEL: u64 = 0x030;
pub const MMIO_QUEUE_NUM_MAX: u64 = 0x034;
pub const MMIO_QUEUE_NUM: u64 = 0x038;
pub const MMIO_QUEUE_READY: u64 = 0x044;
pub const MMIO_QUEUE_NOTIFY: u64 = 0x050;
pub const MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const MMIO_INTERRUPT_ACK: u64 = 0x064;
pub const MMIO_STATUS: u64 = 0x070;
pub const MMIO_QUEUE_DESC_LOW: u64 = 0x080;
pub const MMIO_QUEUE_DESC_HIGH: u64 = 0x084;
pub const MMIO_QUEUE_DRIVER_LOW: u64 = 0x090;
pub const MMIO_QUEUE_DRIVER_HIGH: u64 = 0x094;
pub const MMIO_QUEUE_DEVICE_LOW: u64 = 0x0a0;
pub const MMIO_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
pub const MMIO_CONFIG_GENERATION: u64 = 0x0fc;
pub const MMIO_CAPACITY_LOW: u64 = 0x100;
pub const MMIO_CAPACITY_HIGH: u64 = 0x104;
const FEATURE_RO: u64 = 1 << 5;
const FEATURE_FLUSH: u64 = 1 << 9;
const FEATURE_VERSION_1: u64 = 1 << 32;
const ACKNOWLEDGE: u32 = 1;
const DRIVER: u32 = 2;
const DRIVER_OK: u32 = 4;
const FEATURES_OK: u32 = 8;
const DEVICE_NEEDS_RESET: u32 = 64;
const FAILED: u32 = 128;

pub trait SectorMedia: Send {
    fn sector_count(&self) -> u64;
    fn read_only(&self) -> bool;
    fn read_sectors(&mut self, sector: u64, output: &mut [u8]) -> io::Result<()>;
    fn write_sectors(&mut self, sector: u64, data: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

pub struct FileSectorMedia {
    file: File,
    sectors: u64,
    read_only: bool,
}

impl FileSectorMedia {
    pub fn open(path: impl AsRef<Path>, read_only: bool) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(!read_only).open(path)?;
        let metadata = file.metadata()?;
        let bytes = metadata.len();
        if !metadata.is_file() || bytes == 0 || bytes % u64::from(VIRTIO_SECTOR_BYTES) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sector image must have a nonzero, sector-aligned length",
            ));
        }
        Ok(Self {
            file,
            sectors: bytes / u64::from(VIRTIO_SECTOR_BYTES),
            read_only,
        })
    }

    fn offset(&self, sector: u64, length: usize) -> io::Result<u64> {
        let length =
            u64::try_from(length).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let offset = sector
            .checked_mul(u64::from(VIRTIO_SECTOR_BYTES))
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        if length % u64::from(VIRTIO_SECTOR_BYTES) != 0
            || end > self.sectors * u64::from(VIRTIO_SECTOR_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sector transfer exceeds image bounds or is not sector aligned",
            ));
        }
        Ok(offset)
    }
}

impl SectorMedia for FileSectorMedia {
    fn sector_count(&self) -> u64 {
        self.sectors
    }

    fn read_only(&self) -> bool {
        self.read_only
    }

    fn read_sectors(&mut self, sector: u64, output: &mut [u8]) -> io::Result<()> {
        let offset = self.offset(sector, output.len())?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(output)
    }

    fn write_sectors(&mut self, sector: u64, data: &[u8]) -> io::Result<()> {
        if self.read_only {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let offset = self.offset(sector, data.len())?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        self.file.sync_all()
    }
}

pub(super) struct Kiulu {
    media: Box<dyn SectorMedia>,
    capacity: u64,
    features: u64,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: u64,
    queue_sel: u32,
    queue_size: u32,
    queue_ready: bool,
    descriptor_table: u64,
    available_ring: u64,
    used_ring: u64,
    last_available: u16,
    used_index: u16,
    interrupt_status: u32,
    status: u32,
    io_error: Option<io::Error>,
}

impl Kiulu {
    pub(super) fn new(media: Box<dyn SectorMedia>) -> Result<Self, PhysicalAccessError> {
        let capacity = media.sector_count();
        if capacity == 0
            || capacity
                .checked_mul(u64::from(VIRTIO_SECTOR_BYTES))
                .is_none()
        {
            return Err(PhysicalAccessError::InvalidConfiguration);
        }
        let features =
            FEATURE_VERSION_1 | FEATURE_FLUSH | if media.read_only() { FEATURE_RO } else { 0 };
        Ok(Self {
            media,
            capacity,
            features,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: 0,
            queue_sel: 0,
            queue_size: 0,
            queue_ready: false,
            descriptor_table: 0,
            available_ring: 0,
            used_ring: 0,
            last_available: 0,
            used_index: 0,
            interrupt_status: 0,
            status: 0,
            io_error: None,
        })
    }

    pub(super) fn reset(&mut self) {
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = 0;
        self.queue_sel = 0;
        self.queue_size = 0;
        self.queue_ready = false;
        self.descriptor_table = 0;
        self.available_ring = 0;
        self.used_ring = 0;
        self.last_available = 0;
        self.used_index = 0;
        self.interrupt_status = 0;
        self.status = 0;
    }

    pub(super) fn interrupt_asserted(&self) -> bool {
        self.interrupt_status != 0
    }

    pub(super) fn take_io_error(&mut self) -> Option<io::Error> {
        self.io_error.take()
    }

    pub(super) fn read_word(&self, offset: u64) -> Result<u32, PhysicalAccessError> {
        let value = match offset {
            MMIO_MAGIC_VALUE => VIRTIO_MMIO_MAGIC,
            MMIO_VERSION => VIRTIO_MMIO_VERSION,
            MMIO_DEVICE_ID => VIRTIO_BLOCK_DEVICE_ID,
            MMIO_VENDOR_ID => VIRTIO_MMIO_VENDOR_ID,
            MMIO_DEVICE_FEATURES => match self.device_features_sel {
                0 => self.features as u32,
                1 => (self.features >> 32) as u32,
                _ => 0,
            },
            MMIO_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    QUEUE_MAX
                } else {
                    0
                }
            }
            MMIO_QUEUE_READY => u32::from(self.queue_sel == 0 && self.queue_ready),
            MMIO_INTERRUPT_STATUS => self.interrupt_status,
            MMIO_STATUS => self.status,
            MMIO_CONFIG_GENERATION => 0,
            MMIO_CAPACITY_LOW => self.capacity as u32,
            MMIO_CAPACITY_HIGH => (self.capacity >> 32) as u32,
            _ => return Err(PhysicalAccessError::InvalidDeviceAccess),
        };
        Ok(value)
    }

    pub(super) fn write_word(
        &mut self,
        offset: u64,
        value: u32,
        bus: &mut Kiuas16Bus,
    ) -> Result<(), PhysicalAccessError> {
        match offset {
            MMIO_DEVICE_FEATURES_SEL => self.device_features_sel = value,
            MMIO_DRIVER_FEATURES
                if self.status & (FEATURES_OK | DRIVER_OK) == 0 && self.status & DRIVER != 0 =>
            {
                match self.driver_features_sel {
                    0 => {
                        self.driver_features =
                            (self.driver_features & !0xffff_ffff) | u64::from(value)
                    }
                    1 => {
                        self.driver_features =
                            (self.driver_features & 0xffff_ffff) | (u64::from(value) << 32)
                    }
                    _ if value == 0 => {}
                    _ => return Err(PhysicalAccessError::InvalidDeviceAccess),
                }
            }
            MMIO_DRIVER_FEATURES_SEL => self.driver_features_sel = value,
            MMIO_QUEUE_SEL => self.queue_sel = value,
            MMIO_QUEUE_NUM if self.queue_configurable() => {
                if value == 0 || value > QUEUE_MAX || !value.is_power_of_two() {
                    return Err(PhysicalAccessError::InvalidDeviceAccess);
                }
                self.queue_size = value;
            }
            MMIO_QUEUE_READY if self.queue_configurable() => {
                if value != 1 {
                    return Err(PhysicalAccessError::InvalidDeviceAccess);
                }
                self.validate_queue(bus)?;
                self.queue_ready = true;
            }
            MMIO_QUEUE_NOTIFY if value == 0 => {
                if self.status & (DRIVER_OK | DEVICE_NEEDS_RESET | FAILED) != DRIVER_OK
                    || !self.queue_ready
                {
                    return Err(PhysicalAccessError::InvalidDeviceAccess);
                }
                if let Err(error) = self.notify(bus) {
                    self.status |= DEVICE_NEEDS_RESET;
                    self.interrupt_status |= 2;
                    return Err(error);
                }
            }
            MMIO_INTERRUPT_ACK if value & !3 == 0 => self.interrupt_status &= !value,
            MMIO_STATUS => self.write_status(value)?,
            MMIO_QUEUE_DESC_LOW
            | MMIO_QUEUE_DESC_HIGH
            | MMIO_QUEUE_DRIVER_LOW
            | MMIO_QUEUE_DRIVER_HIGH
            | MMIO_QUEUE_DEVICE_LOW
            | MMIO_QUEUE_DEVICE_HIGH
                if self.queue_configurable() =>
            {
                let address = match offset {
                    MMIO_QUEUE_DESC_LOW | MMIO_QUEUE_DESC_HIGH => &mut self.descriptor_table,
                    MMIO_QUEUE_DRIVER_LOW | MMIO_QUEUE_DRIVER_HIGH => &mut self.available_ring,
                    _ => &mut self.used_ring,
                };
                if offset & 4 == 0 {
                    *address = (*address & !0xffff_ffff) | u64::from(value);
                } else {
                    *address = (*address & 0xffff_ffff) | (u64::from(value) << 32);
                }
            }
            _ => return Err(PhysicalAccessError::InvalidDeviceAccess),
        }
        Ok(())
    }

    fn queue_configurable(&self) -> bool {
        self.queue_sel == 0
            && !self.queue_ready
            && self.status & FEATURES_OK != 0
            && self.status & (DRIVER_OK | DEVICE_NEEDS_RESET | FAILED) == 0
    }

    fn write_status(&mut self, value: u32) -> Result<(), PhysicalAccessError> {
        if value == 0 {
            self.reset();
            return Ok(());
        }
        let writable = ACKNOWLEDGE | DRIVER | DRIVER_OK | FEATURES_OK | FAILED;
        if value & !(writable | (self.status & DEVICE_NEEDS_RESET)) != 0
            || value & self.status != self.status
            || value & ACKNOWLEDGE == 0
            || (value & (FEATURES_OK | DRIVER_OK) != 0 && value & DRIVER == 0)
            || (value & DRIVER_OK != 0 && self.status & FEATURES_OK == 0)
        {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let mut accepted = value;
        if value & FEATURES_OK != 0
            && (self.driver_features & FEATURE_VERSION_1 == 0
                || self.driver_features & !self.features != 0)
        {
            accepted &= !FEATURES_OK;
        }
        self.status = accepted;
        Ok(())
    }

    fn validate_queue(&self, bus: &Kiuas16Bus) -> Result<(), PhysicalAccessError> {
        if self.queue_size == 0
            || self.queue_size > QUEUE_MAX
            || !self.queue_size.is_power_of_two()
            || self.descriptor_table & 15 != 0
            || self.available_ring & 1 != 0
            || self.used_ring & 3 != 0
        {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let size = u64::from(self.queue_size);
        let ranges = [
            (self.descriptor_table, size * 16),
            (self.available_ring, 4 + size * 2),
            (self.used_ring, 4 + size * 8),
        ];
        for (index, &(start, length)) in ranges.iter().enumerate() {
            bus.validate_dma_range(start, length)?;
            for &(other, other_length) in &ranges[..index] {
                if start < other + other_length && other < start + length {
                    return Err(PhysicalAccessError::InvalidDeviceAccess);
                }
            }
        }
        Ok(())
    }

    fn notify(&mut self, bus: &mut Kiuas16Bus) -> Result<(), PhysicalAccessError> {
        self.validate_queue(bus)?;
        let flags = read_u16(bus, self.available_ring)?;
        if flags & !1 != 0 {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let available = read_u16(bus, self.available_ring + 2)?;
        let pending = available.wrapping_sub(self.last_available);
        if u32::from(pending) > self.queue_size {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        for _ in 0..pending {
            let slot = u32::from(self.last_available) % self.queue_size;
            let head = read_u16(bus, self.available_ring + 4 + u64::from(slot) * 2)?;
            let written = self.execute_request(bus, head)?;
            let used_slot = u32::from(self.used_index) % self.queue_size;
            let used = self.used_ring + 4 + u64::from(used_slot) * 8;
            bus.write(used, &u32::from(head).to_le_bytes())?;
            bus.write(used + 4, &written.to_le_bytes())?;
            self.used_index = self.used_index.wrapping_add(1);
            bus.write(self.used_ring + 2, &self.used_index.to_le_bytes())?;
            self.last_available = self.last_available.wrapping_add(1);
            if flags & 1 == 0 {
                self.interrupt_status |= 1;
            }
        }
        Ok(())
    }

    fn execute_request(
        &mut self,
        bus: &mut Kiuas16Bus,
        head: u16,
    ) -> Result<u32, PhysicalAccessError> {
        let chain =
            bus.validate_virtqueue_chain(self.descriptor_table, self.queue_size as u16, head)?;
        if chain.len() < 2 {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let header = &chain[0];
        let status = &chain[chain.len() - 1];
        if header.device_writable
            || header.length < 16
            || !status.device_writable
            || status.length < 1
        {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let mut request = [0; 16];
        bus.read(header.address, &mut request)?;
        let kind = u32::from_le_bytes(request[..4].try_into().unwrap());
        let sector = u64::from_le_bytes(request[8..].try_into().unwrap());
        let payload = &chain[1..chain.len() - 1];
        let mut total = 0usize;
        for descriptor in payload {
            total = total
                .checked_add(descriptor.length as usize)
                .filter(|&length| length <= bus.ram.len())
                .ok_or(PhysicalAccessError::InvalidDeviceAccess)?;
            if matches!(kind, 0 | 1) && descriptor.device_writable != (kind == 0) {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
        }
        let mut written = 1;
        let result = match kind {
            0 | 1 => {
                let sectors = total as u64 / u64::from(VIRTIO_SECTOR_BYTES);
                let in_range = total as u64 % u64::from(VIRTIO_SECTOR_BYTES) == 0
                    && sector
                        .checked_add(sectors)
                        .is_some_and(|end| end <= self.capacity);
                if !in_range || (kind == 1 && self.media.read_only()) {
                    1
                } else {
                    let mut data = vec![0; total];
                    if kind == 1 {
                        let mut offset = 0;
                        for descriptor in payload {
                            let end = offset + descriptor.length as usize;
                            bus.read(descriptor.address, &mut data[offset..end])?;
                            offset = end;
                        }
                    }
                    let io_result = if kind == 0 {
                        self.media.read_sectors(sector, &mut data)
                    } else {
                        self.media.write_sectors(sector, &data).and_then(|()| {
                            if self.driver_features & FEATURE_FLUSH == 0 {
                                self.media.flush()
                            } else {
                                Ok(())
                            }
                        })
                    };
                    match io_result {
                        Ok(()) => {
                            if kind == 0 {
                                let mut offset = 0;
                                for descriptor in payload {
                                    let end = offset + descriptor.length as usize;
                                    if end != offset {
                                        bus.write(descriptor.address, &data[offset..end])?;
                                    }
                                    offset = end;
                                }
                                written += total as u32;
                            }
                            0
                        }
                        Err(error) => {
                            self.io_error = Some(error);
                            1
                        }
                    }
                }
            }
            4 if self.driver_features & FEATURE_FLUSH != 0 => {
                if !payload.is_empty() {
                    return Err(PhysicalAccessError::InvalidDeviceAccess);
                }
                match self.media.flush() {
                    Ok(()) => 0,
                    Err(error) => {
                        self.io_error = Some(error);
                        1
                    }
                }
            }
            _ => 2,
        };
        bus.write(status.address, &[result])?;
        Ok(written)
    }
}

fn read_u16(bus: &mut Kiuas16Bus, address: u64) -> Result<u16, PhysicalAccessError> {
    let mut bytes = [0; 2];
    bus.read(address, &mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::kiuas16::{
        BOOT_ROM_BYTES, Kiuas16Machine, PLIC, RAM, ROOT_BLOCK, ROOT_BLOCK_PLIC_SOURCE, Rv64Core,
    };

    const DESC: u64 = RAM.start;
    const AVAIL: u64 = RAM.start + 0x100;
    const USED: u64 = RAM.start + 0x200;
    const HEADER: u64 = RAM.start + 0x300;
    const DATA: u64 = RAM.start + 0x400;
    const STATUS: u64 = RAM.start + 0x900;

    struct MediaState {
        bytes: Vec<u8>,
        read_only: bool,
        fail_read: bool,
        fail_write: bool,
        fail_flush: bool,
        writes: usize,
        flushes: usize,
    }

    struct TestMedia(Arc<Mutex<MediaState>>);

    impl SectorMedia for TestMedia {
        fn sector_count(&self) -> u64 {
            self.0.lock().unwrap().bytes.len() as u64 / 512
        }

        fn read_only(&self) -> bool {
            self.0.lock().unwrap().read_only
        }

        fn read_sectors(&mut self, sector: u64, output: &mut [u8]) -> io::Result<()> {
            let state = self.0.lock().unwrap();
            if state.fail_read {
                output.fill(0xee);
                return Err(io::Error::other("injected read failure"));
            }
            let start = sector as usize * 512;
            output.copy_from_slice(&state.bytes[start..start + output.len()]);
            Ok(())
        }

        fn write_sectors(&mut self, sector: u64, data: &[u8]) -> io::Result<()> {
            let mut state = self.0.lock().unwrap();
            if state.fail_write {
                return Err(io::Error::other("injected write failure"));
            }
            let start = sector as usize * 512;
            state.bytes[start..start + data.len()].copy_from_slice(data);
            state.writes += 1;
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            let mut state = self.0.lock().unwrap();
            if state.fail_flush {
                return Err(io::Error::other("injected flush failure"));
            }
            state.flushes += 1;
            Ok(())
        }
    }

    fn fixture(read_only: bool) -> (Kiuas16Bus, Arc<Mutex<MediaState>>) {
        let state = Arc::new(Mutex::new(MediaState {
            bytes: vec![0x5a; 4096],
            read_only,
            fail_read: false,
            fail_write: false,
            fail_flush: false,
            writes: 0,
            flushes: 0,
        }));
        let mut bus = Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 0x2000).unwrap();
        bus.attach_root_block(Box::new(TestMedia(state.clone())))
            .unwrap();
        (bus, state)
    }

    fn write_register(bus: &mut Kiuas16Bus, offset: u64, value: u32) {
        bus.write(ROOT_BLOCK.start + offset, &value.to_le_bytes())
            .unwrap();
    }

    fn read_register(bus: &mut Kiuas16Bus, offset: u64) -> u32 {
        let mut bytes = [0; 4];
        bus.read(ROOT_BLOCK.start + offset, &mut bytes).unwrap();
        u32::from_le_bytes(bytes)
    }

    fn start(bus: &mut Kiuas16Bus, flush: bool) {
        write_register(bus, 0x070, ACKNOWLEDGE);
        write_register(bus, 0x070, ACKNOWLEDGE | DRIVER);
        write_register(bus, 0x024, 1);
        write_register(bus, 0x020, 1);
        write_register(bus, 0x024, 0);
        write_register(bus, 0x020, if flush { FEATURE_FLUSH as u32 } else { 0 });
        write_register(bus, 0x070, ACKNOWLEDGE | DRIVER | FEATURES_OK);
        assert_eq!(read_register(bus, 0x070), 11);
        write_register(bus, 0x030, 0);
        write_register(bus, 0x038, 8);
        for (offset, address) in [(0x080, DESC), (0x090, AVAIL), (0x0a0, USED)] {
            write_register(bus, offset, address as u32);
            write_register(bus, offset + 4, (address >> 32) as u32);
        }
        write_register(bus, 0x044, 1);
        write_register(bus, 0x070, ACKNOWLEDGE | DRIVER | FEATURES_OK | DRIVER_OK);
    }

    fn descriptor(
        bus: &mut Kiuas16Bus,
        index: u16,
        address: u64,
        length: u32,
        flags: u16,
        next: u16,
    ) {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&address.to_le_bytes());
        bytes[8..12].copy_from_slice(&length.to_le_bytes());
        bytes[12..14].copy_from_slice(&flags.to_le_bytes());
        bytes[14..].copy_from_slice(&next.to_le_bytes());
        bus.write(DESC + u64::from(index) * 16, &bytes).unwrap();
    }

    fn request(bus: &mut Kiuas16Bus, kind: u32, sector: u64) {
        let mut header = [0; 16];
        header[..4].copy_from_slice(&kind.to_le_bytes());
        header[8..].copy_from_slice(&sector.to_le_bytes());
        bus.write(HEADER, &header).unwrap();
        bus.write(STATUS, &[0xff]).unwrap();
        descriptor(bus, 0, HEADER, 16, 1, 1);
        if kind == 4 {
            descriptor(bus, 1, STATUS, 1, 2, 0);
        } else {
            descriptor(bus, 1, DATA, 512, if kind == 0 { 3 } else { 1 }, 2);
            descriptor(bus, 2, STATUS, 1, 2, 0);
        }
    }

    fn publish(bus: &mut Kiuas16Bus, index: u16, head: u16) {
        bus.write(
            AVAIL + 4 + u64::from(index.wrapping_sub(1) % 8) * 2,
            &head.to_le_bytes(),
        )
        .unwrap();
        bus.write(AVAIL + 2, &index.to_le_bytes()).unwrap();
    }

    fn bytes(bus: &mut Kiuas16Bus, address: u64, length: usize) -> Vec<u8> {
        let mut output = vec![0; length];
        bus.read(address, &mut output).unwrap();
        output
    }

    #[test]
    fn identity_features_capacity_and_invalid_mmio() {
        let (mut bus, _) = fixture(true);
        for (offset, expected) in [
            (0, VIRTIO_MMIO_MAGIC),
            (4, 2),
            (8, 2),
            (12, VIRTIO_MMIO_VENDOR_ID),
            (0x100, 8),
            (0x104, 0),
            (0xfc, 0),
        ] {
            assert_eq!(read_register(&mut bus, offset), expected);
        }
        assert_eq!(
            read_register(&mut bus, 0x10),
            (FEATURE_FLUSH | FEATURE_RO) as u32
        );
        write_register(&mut bus, 0x14, 1);
        assert_eq!(read_register(&mut bus, 0x10), 1);
        write_register(&mut bus, 0x14, 9);
        assert_eq!(read_register(&mut bus, 0x10), 0);
        write_register(&mut bus, 0x30, 1);
        assert_eq!(read_register(&mut bus, 0x34), 0);
        assert_eq!(read_register(&mut bus, 0x44), 0);
        for (address, length) in [
            (ROOT_BLOCK.start, 1),
            (ROOT_BLOCK.start + 1, 4),
            (ROOT_BLOCK.start + 0x108, 4),
        ] {
            assert_eq!(
                bus.read(address, &mut vec![0; length]),
                Err(PhysicalAccessError::InvalidDeviceAccess)
            );
            assert_eq!(
                bus.write(address, &vec![0; length]),
                Err(PhysicalAccessError::InvalidDeviceAccess)
            );
        }
        assert_eq!(
            bus.write(ROOT_BLOCK.start, &0u32.to_le_bytes()),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
    }

    #[test]
    fn register_constants_match_guest_header() {
        let expected = [
            ("KIULU_QUEUE_MAX", u64::from(QUEUE_MAX)),
            ("KIULU_MMIO_MAGIC_VALUE", MMIO_MAGIC_VALUE),
            ("KIULU_MMIO_VERSION", MMIO_VERSION),
            ("KIULU_MMIO_DEVICE_ID", MMIO_DEVICE_ID),
            ("KIULU_MMIO_VENDOR_ID", MMIO_VENDOR_ID),
            ("KIULU_MMIO_DEVICE_FEATURES", MMIO_DEVICE_FEATURES),
            ("KIULU_MMIO_DEVICE_FEATURES_SEL", MMIO_DEVICE_FEATURES_SEL),
            ("KIULU_MMIO_DRIVER_FEATURES", MMIO_DRIVER_FEATURES),
            ("KIULU_MMIO_DRIVER_FEATURES_SEL", MMIO_DRIVER_FEATURES_SEL),
            ("KIULU_MMIO_QUEUE_SEL", MMIO_QUEUE_SEL),
            ("KIULU_MMIO_QUEUE_NUM_MAX", MMIO_QUEUE_NUM_MAX),
            ("KIULU_MMIO_QUEUE_NUM", MMIO_QUEUE_NUM),
            ("KIULU_MMIO_QUEUE_READY", MMIO_QUEUE_READY),
            ("KIULU_MMIO_QUEUE_NOTIFY", MMIO_QUEUE_NOTIFY),
            ("KIULU_MMIO_INTERRUPT_STATUS", MMIO_INTERRUPT_STATUS),
            ("KIULU_MMIO_INTERRUPT_ACK", MMIO_INTERRUPT_ACK),
            ("KIULU_MMIO_STATUS", MMIO_STATUS),
            ("KIULU_MMIO_QUEUE_DESC_LOW", MMIO_QUEUE_DESC_LOW),
            ("KIULU_MMIO_QUEUE_DESC_HIGH", MMIO_QUEUE_DESC_HIGH),
            ("KIULU_MMIO_QUEUE_DRIVER_LOW", MMIO_QUEUE_DRIVER_LOW),
            ("KIULU_MMIO_QUEUE_DRIVER_HIGH", MMIO_QUEUE_DRIVER_HIGH),
            ("KIULU_MMIO_QUEUE_DEVICE_LOW", MMIO_QUEUE_DEVICE_LOW),
            ("KIULU_MMIO_QUEUE_DEVICE_HIGH", MMIO_QUEUE_DEVICE_HIGH),
            ("KIULU_MMIO_CONFIG_GENERATION", MMIO_CONFIG_GENERATION),
            ("KIULU_MMIO_CAPACITY_LOW", MMIO_CAPACITY_LOW),
            ("KIULU_MMIO_CAPACITY_HIGH", MMIO_CAPACITY_HIGH),
        ];
        crate::kiuas16::tests::assert_header_values_match(
            include_str!("../../toolchain/include/kiuas16_machine.h"),
            &expected,
        );
    }

    #[test]
    fn negotiation_rejects_missing_version_and_unsupported_features() {
        for features in [0, FEATURE_VERSION_1 | (1 << 28)] {
            let (mut bus, _) = fixture(false);
            write_register(&mut bus, 0x70, 3);
            write_register(&mut bus, 0x20, features as u32);
            write_register(&mut bus, 0x24, 1);
            write_register(&mut bus, 0x20, (features >> 32) as u32);
            write_register(&mut bus, 0x70, 11);
            assert_eq!(read_register(&mut bus, 0x70), 3);
            assert_eq!(
                bus.write(ROOT_BLOCK.start + 0x70, &15u32.to_le_bytes()),
                Err(PhysicalAccessError::InvalidDeviceAccess)
            );
            assert_eq!(
                bus.write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes()),
                Err(PhysicalAccessError::InvalidDeviceAccess)
            );
        }
        let (mut bus, _) = fixture(false);
        start(&mut bus, true);
        for (offset, value) in [(0x20, 0), (0x38, 4), (0x80, 0), (0x44, 0), (0x70, 3)] {
            assert_eq!(
                bus.write(ROOT_BLOCK.start + offset, &u32::to_le_bytes(value)),
                Err(PhysicalAccessError::InvalidDeviceAccess)
            );
        }
    }

    #[test]
    fn read_write_and_flush_complete_with_correct_used_lengths() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 0, 1);
        publish(&mut bus, 1, 0);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(bytes(&mut bus, DATA, 512), vec![0x5a; 512]);
        assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
        assert_eq!(bytes(&mut bus, USED + 2, 2), 1u16.to_le_bytes());
        assert_eq!(bytes(&mut bus, USED + 4, 8), [0, 0, 0, 0, 1, 2, 0, 0]);
        request(&mut bus, 1, 2);
        bus.write(DATA, &[0x37; 512]).unwrap();
        publish(&mut bus, 2, 0);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(&state.lock().unwrap().bytes[1024..1536], &[0x37; 512]);
        assert_eq!(bytes(&mut bus, USED + 16, 4), 1u32.to_le_bytes());
        request(&mut bus, 4, 0);
        publish(&mut bus, 3, 0);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
        assert_eq!(state.lock().unwrap().flushes, 1);
    }

    #[test]
    fn unnegotiated_flush_requires_writethrough_and_reports_sync_errors() {
        for fail_flush in [false, true] {
            let (mut bus, state) = fixture(false);
            state.lock().unwrap().fail_flush = fail_flush;
            start(&mut bus, false);
            request(&mut bus, 1, 0);
            publish(&mut bus, 1, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [u8::from(fail_flush)]);
            assert_eq!(state.lock().unwrap().writes, 1);
            if fail_flush {
                assert!(bus.take_root_block_io_error().is_some());
            } else {
                assert_eq!(state.lock().unwrap().flushes, 1);
            }
        }
    }

    #[test]
    fn one_notification_drains_multiple_available_requests() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 4, 0);
        descriptor(&mut bus, 2, HEADER, 16, 1, 3);
        descriptor(&mut bus, 3, STATUS + 1, 1, 2, 0);
        bus.write(STATUS + 1, &[0xff]).unwrap();
        publish(&mut bus, 1, 0);
        publish(&mut bus, 2, 2);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(bytes(&mut bus, STATUS, 2), [0, 0]);
        assert_eq!(bytes(&mut bus, USED + 2, 2), [2, 0]);
        assert_eq!(bytes(&mut bus, USED + 4, 4), 0u32.to_le_bytes());
        assert_eq!(bytes(&mut bus, USED + 12, 4), 2u32.to_le_bytes());
        assert_eq!(state.lock().unwrap().flushes, 2);
    }

    #[test]
    fn sector_alignment_and_total_dma_budget_are_enforced() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 1, 0);
        descriptor(&mut bus, 1, DATA, 511, 1, 2);
        publish(&mut bus, 1, 0);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(bytes(&mut bus, STATUS, 1), [1]);
        assert_eq!(state.lock().unwrap().writes, 0);
        request(&mut bus, 1, 0);
        descriptor(&mut bus, 1, RAM.start, 0x2000, 1, 2);
        descriptor(&mut bus, 2, DATA, 512, 1, 3);
        descriptor(&mut bus, 3, STATUS, 1, 2, 0);
        publish(&mut bus, 2, 0);
        assert_eq!(
            bus.write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes()),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(state.lock().unwrap().writes, 0);
        assert_eq!(bytes(&mut bus, STATUS, 1), [0xff]);
        assert_eq!(bytes(&mut bus, USED + 2, 2), [1, 0]);
    }

    #[test]
    fn pinned_xv6_low_feature_page_alone_is_not_modern_negotiation() {
        let (mut bus, _) = fixture(false);
        write_register(&mut bus, 0x70, 0);
        write_register(&mut bus, 0x70, 1);
        write_register(&mut bus, 0x70, 3);
        let features = read_register(&mut bus, 0x10) & !(FEATURE_RO | FEATURE_FLUSH) as u32;
        write_register(&mut bus, 0x20, features);
        write_register(&mut bus, 0x70, 11);
        assert_eq!(read_register(&mut bus, 0x70), 3);
    }

    #[test]
    fn scatter_gather_reads_and_writes() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        for (index, kind) in [(1, 0), (2, 1)] {
            request(&mut bus, kind, 0);
            descriptor(&mut bus, 1, DATA, 128, if kind == 0 { 3 } else { 1 }, 2);
            descriptor(
                &mut bus,
                2,
                DATA + 0x200,
                384,
                if kind == 0 { 3 } else { 1 },
                3,
            );
            descriptor(&mut bus, 3, STATUS, 1, 2, 0);
            if kind == 1 {
                bus.write(DATA, &[0x11; 128]).unwrap();
                bus.write(DATA + 0x200, &[0x22; 384]).unwrap();
            }
            publish(&mut bus, index, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
            if kind == 0 {
                assert_eq!(bytes(&mut bus, DATA, 128), [0x5a; 128]);
                assert_eq!(bytes(&mut bus, DATA + 0x200, 384), [0x5a; 384]);
            }
        }
        let state = state.lock().unwrap();
        assert_eq!(&state.bytes[..128], &[0x11; 128]);
        assert_eq!(&state.bytes[128..512], &[0x22; 384]);
    }

    #[test]
    fn completion_irq_acknowledgement_and_suppression() {
        let (mut bus, _) = fixture(false);
        start(&mut bus, true);
        bus.write(PLIC.start + 4, &1u32.to_le_bytes()).unwrap();
        bus.write(
            PLIC.start + 0x2080,
            &(1u32 << ROOT_BLOCK_PLIC_SOURCE).to_le_bytes(),
        )
        .unwrap();
        request(&mut bus, 0, 0);
        publish(&mut bus, 1, 0);
        write_register(&mut bus, 0x50, 0);
        assert!(bus.plic_interrupt_pending(1));
        let mut claim = [0; 4];
        bus.read(PLIC.start + 0x201004, &mut claim).unwrap();
        assert_eq!(u32::from_le_bytes(claim), ROOT_BLOCK_PLIC_SOURCE);
        write_register(&mut bus, 0x64, 1);
        bus.write(PLIC.start + 0x201004, &claim).unwrap();
        assert!(!bus.plic_interrupt_pending(1));
        bus.write(AVAIL, &1u16.to_le_bytes()).unwrap();
        publish(&mut bus, 2, 0);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(bytes(&mut bus, USED + 2, 2), 2u16.to_le_bytes());
        assert_eq!(read_register(&mut bus, 0x60), 0);
        assert!(!bus.plic_interrupt_pending(1));
    }

    #[test]
    fn ring_slots_and_u16_indices_wrap_without_replaying_requests() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 4, 0);
        for sequence in 1u32..=65_538 {
            publish(&mut bus, sequence as u16, 0);
            write_register(&mut bus, 0x50, 0);
        }
        assert_eq!(bytes(&mut bus, USED + 2, 2), 2u16.to_le_bytes());
        assert_eq!(state.lock().unwrap().flushes, 65_538);
        write_register(&mut bus, 0x50, 0);
        assert_eq!(state.lock().unwrap().flushes, 65_538);
    }

    #[test]
    fn malformed_chains_fail_before_dma_or_media_writes() {
        for invalid in 0..8 {
            let (mut bus, state) = fixture(false);
            start(&mut bus, true);
            request(&mut bus, 1, 0);
            match invalid {
                0 => descriptor(&mut bus, 1, DATA, 512, 1, 0),
                1 => descriptor(&mut bus, 1, DATA, 512, 1, 8),
                2 => descriptor(&mut bus, 1, DATA, 512, 5, 2),
                3 => descriptor(&mut bus, 1, DATA, 512, 3, 2),
                4 => descriptor(&mut bus, 2, STATUS, 1, 0, 0),
                5 => descriptor(&mut bus, 1, ROOT_BLOCK.start, 512, 1, 2),
                6 => descriptor(&mut bus, 1, RAM.start + 0x1fff, 512, 1, 2),
                _ => descriptor(&mut bus, 0, HEADER, 15, 1, 1),
            }
            publish(&mut bus, 1, 0);
            let error = bus
                .write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes())
                .unwrap_err();
            assert!(matches!(
                error,
                PhysicalAccessError::InvalidDeviceAccess | PhysicalAccessError::DmaOutsideRam
            ));
            assert_eq!(state.lock().unwrap().writes, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [0xff]);
            assert_eq!(bytes(&mut bus, USED + 2, 2), [0, 0]);
            assert_eq!(
                read_register(&mut bus, 0x70) & DEVICE_NEEDS_RESET,
                DEVICE_NEEDS_RESET
            );
            assert_eq!(read_register(&mut bus, 0x60), 2);
            assert!(
                bus.write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes())
                    .is_err()
            );
        }
    }

    #[test]
    fn rejects_invalid_queue_extents_alignment_and_available_overrun() {
        for (offset, value, expected) in [
            (
                0x80,
                (DESC + 1) as u32,
                PhysicalAccessError::InvalidDeviceAccess,
            ),
            (
                0x90,
                (AVAIL + 1) as u32,
                PhysicalAccessError::InvalidDeviceAccess,
            ),
            (
                0xa0,
                (USED + 2) as u32,
                PhysicalAccessError::InvalidDeviceAccess,
            ),
            (0x84, 1, PhysicalAccessError::DmaOutsideRam),
            (0x90, DESC as u32, PhysicalAccessError::InvalidDeviceAccess),
            (
                0xa0,
                (RAM.start + 0x1ffc) as u32,
                PhysicalAccessError::DmaOutsideRam,
            ),
        ] {
            let (mut bus, _) = fixture(false);
            start(&mut bus, true);
            write_register(&mut bus, 0x70, 0);
            write_register(&mut bus, 0x70, 3);
            write_register(&mut bus, 0x24, 1);
            write_register(&mut bus, 0x20, 1);
            write_register(&mut bus, 0x70, 11);
            write_register(&mut bus, 0x38, 8);
            write_register(&mut bus, 0x80, DESC as u32);
            write_register(&mut bus, 0x90, AVAIL as u32);
            write_register(&mut bus, 0xa0, USED as u32);
            write_register(&mut bus, offset, value);
            assert_eq!(
                bus.write(ROOT_BLOCK.start + 0x44, &1u32.to_le_bytes()),
                Err(expected)
            );
            assert_eq!(read_register(&mut bus, 0x44), 0);
        }
        let (mut bus, _) = fixture(false);
        start(&mut bus, true);
        publish(&mut bus, 9, 0);
        assert_eq!(
            bus.write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes()),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(bytes(&mut bus, USED + 2, 2), [0, 0]);
    }

    #[test]
    fn request_errors_complete_instead_of_reporting_success() {
        for (read_only, kind, sector, expected) in [
            (true, 1, 0, 1),
            (false, 0, 8, 1),
            (false, 1, u64::MAX, 1),
            (false, 7, 0, 2),
            (false, 4, 0, 2),
        ] {
            let (mut bus, state) = fixture(read_only);
            start(&mut bus, false);
            request(&mut bus, kind, sector);
            publish(&mut bus, 1, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [expected]);
            assert_eq!(bytes(&mut bus, USED + 2, 2), [1, 0]);
            assert_eq!(bytes(&mut bus, USED + 8, 4), 1u32.to_le_bytes());
            assert_eq!(state.lock().unwrap().writes, 0);
            assert_eq!(read_register(&mut bus, 0x70), 15);
        }
    }

    #[test]
    fn failed_host_io_reports_ioerr_and_preserves_read_destinations() {
        for kind in [0, 1, 4] {
            let (mut bus, state) = fixture(false);
            {
                let mut state = state.lock().unwrap();
                state.fail_read = true;
                state.fail_write = true;
                state.fail_flush = true;
            }
            start(&mut bus, true);
            request(&mut bus, kind, 0);
            bus.write(DATA, &[0x77; 512]).unwrap();
            publish(&mut bus, 1, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [1]);
            assert_eq!(bytes(&mut bus, DATA, 512), [0x77; 512]);
            assert_eq!(bytes(&mut bus, USED + 8, 4), 1u32.to_le_bytes());
            assert_eq!(
                bus.take_root_block_io_error().unwrap().kind(),
                io::ErrorKind::Other
            );
            assert!(bus.take_root_block_io_error().is_none());
            assert_eq!(read_register(&mut bus, 0x70), 15);
        }
    }

    #[test]
    fn reset_discards_unnotified_work_and_preserves_media_and_ram() {
        let (mut bus, state) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 1, 0);
        bus.write(DATA, &[0x77; 512]).unwrap();
        publish(&mut bus, 1, 0);
        write_register(&mut bus, 0x70, 0);
        assert_eq!(read_register(&mut bus, 0x70), 0);
        assert_eq!(read_register(&mut bus, 0x44), 0);
        assert_eq!(read_register(&mut bus, 0x60), 0);
        assert!(
            bus.write(ROOT_BLOCK.start + 0x50, &0u32.to_le_bytes())
                .is_err()
        );
        assert_eq!(state.lock().unwrap().writes, 0);
        assert_eq!(bytes(&mut bus, DATA, 512), [0x77; 512]);
        bus.write(AVAIL + 2, &0u16.to_le_bytes()).unwrap();
        start(&mut bus, true);
        publish(&mut bus, 1, 0);
        write_register(&mut bus, 0x50, 0);
        let mut machine = Kiuas16Machine {
            core: Rv64Core::cold_reset(),
            bus,
        };
        machine.reset();
        assert_eq!(read_register(machine.bus_mut(), 0x70), 0);
        assert_eq!(read_register(machine.bus_mut(), 0x60), 0);
        assert_eq!(&state.lock().unwrap().bytes[..512], &[0x77; 512]);
        assert_eq!(bytes(machine.bus_mut(), DATA, 512), [0x77; 512]);
    }

    #[test]
    fn rv64_store_notifies_device_and_load_observes_dma_completion() {
        let (mut bus, _) = fixture(false);
        start(&mut bus, true);
        request(&mut bus, 0, 0);
        publish(&mut bus, 1, 0);
        bus.boot_rom[..4].copy_from_slice(&0x0020_a023u32.to_le_bytes());
        bus.boot_rom[4..8].copy_from_slice(&0x0002_4183u32.to_le_bytes());
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = ROOT_BLOCK.start + 0x50;
        core.integer_registers[4] = DATA;
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(3), Some(0x5a));
        assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
        assert_eq!(bytes(&mut bus, USED + 2, 2), [1, 0]);
    }

    struct ImageFile(std::path::PathBuf);

    impl ImageFile {
        fn new(length: u64) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("kiulu-{}-{id}.img", std::process::id()));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.set_len(length).unwrap();
            Self(path)
        }
    }

    impl Drop for ImageFile {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).unwrap();
        }
    }

    #[test]
    fn file_media_bounds_read_only_flush_and_persistence() {
        let image = ImageFile::new(1024);
        {
            let mut media = FileSectorMedia::open(&image.0, false).unwrap();
            assert_eq!(media.sector_count(), 2);
            media.write_sectors(1, &[0x42; 512]).unwrap();
            media.flush().unwrap();
            for (sector, length) in [(2, 512), (u64::MAX, 512), (0, 1)] {
                assert_eq!(
                    media
                        .write_sectors(sector, &vec![0; length])
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::InvalidInput
                );
                assert_eq!(
                    media
                        .read_sectors(sector, &mut vec![0; length])
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::InvalidInput
                );
            }
        }
        {
            let mut media = FileSectorMedia::open(&image.0, true).unwrap();
            assert!(media.read_only());
            let mut data = [0; 512];
            media.read_sectors(1, &mut data).unwrap();
            assert_eq!(data, [0x42; 512]);
            assert_eq!(
                media.write_sectors(0, &[0; 512]).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
            media.flush().unwrap();
        }
        assert_eq!(std::fs::metadata(&image.0).unwrap().len(), 1024);
        for length in [0, 513] {
            let invalid = ImageFile::new(length);
            assert_eq!(
                FileSectorMedia::open(&invalid.0, false)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn file_backed_guest_write_flush_and_reopen() {
        let image = ImageFile::new(4096);
        {
            let mut bus = Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 0x2000).unwrap();
            bus.attach_root_block(Box::new(FileSectorMedia::open(&image.0, false).unwrap()))
                .unwrap();
            start(&mut bus, true);
            request(&mut bus, 1, 7);
            bus.write(DATA, &[0x9c; 512]).unwrap();
            publish(&mut bus, 1, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
            request(&mut bus, 4, 0);
            publish(&mut bus, 2, 0);
            write_register(&mut bus, 0x50, 0);
            assert_eq!(bytes(&mut bus, STATUS, 1), [0]);
        }
        let mut media = FileSectorMedia::open(&image.0, true).unwrap();
        let mut output = [0; 512];
        media.read_sectors(7, &mut output).unwrap();
        assert_eq!(output, [0x9c; 512]);
    }
}
