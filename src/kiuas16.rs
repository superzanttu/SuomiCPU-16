use std::collections::VecDeque;

pub const PHYSICAL_ADDRESS_BITS: u32 = 56;
pub const PHYSICAL_ADDRESS_LIMIT: u64 = 1 << PHYSICAL_ADDRESS_BITS;

pub const BOOT_ROM: AddressRange = AddressRange::new(0x0000_1000, 0x0001_0000);
pub const PLIC: AddressRange = AddressRange::new(0x0c00_0000, 0x1000_0000);
pub const UART: AddressRange = AddressRange::new(0x1000_0000, 0x1000_1000);
pub const ROOT_BLOCK: AddressRange = AddressRange::new(0x1000_1000, 0x1000_2000);
pub const RAM: AddressRange = AddressRange::new(0x8000_0000, 0x8800_0000);
pub const BOOT_ROM_BYTES: usize = (BOOT_ROM.end - BOOT_ROM.start) as usize;

pub const RESET_PC: u64 = BOOT_ROM.start;
pub const XV6_HART_COUNT: u32 = 1;
pub const XV6_RAM_BYTES: u64 = RAM.end - RAM.start;
pub const TIMEBASE_FREQUENCY: u64 = 10_000_000;
pub const RESET_STIMECMP: u64 = u64::MAX;
pub const PLIC_SOURCE_COUNT: u32 = 31;
pub const PLIC_CONTEXT_COUNT: usize = 2;
pub const UART_PLIC_SOURCE: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressRange {
    pub start: u64,
    pub end: u64,
}

impl AddressRange {
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub const fn contains(self, address: u64) -> bool {
        self.start <= address && address < self.end
    }

    pub const fn contains_range(self, start: u64, end: u64) -> bool {
        self.start <= start && start < end && end <= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalRegion {
    BootRom,
    Plic,
    Uart,
    RootBlock,
    Ram,
}

impl PhysicalRegion {
    const fn address_range(self) -> AddressRange {
        match self {
            Self::BootRom => BOOT_ROM,
            Self::Plic => PLIC,
            Self::Uart => UART,
            Self::RootBlock => ROOT_BLOCK,
            Self::Ram => RAM,
        }
    }

    const fn from_address(address: u64) -> Option<Self> {
        if BOOT_ROM.contains(address) {
            Some(Self::BootRom)
        } else if PLIC.contains(address) {
            Some(Self::Plic)
        } else if UART.contains(address) {
            Some(Self::Uart)
        } else if ROOT_BLOCK.contains(address) {
            Some(Self::RootBlock)
        } else if RAM.contains(address) {
            Some(Self::Ram)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalAccess {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalAccessError {
    EmptyAccess,
    AddressOverflow,
    AddressOutOfRange,
    Unmapped,
    RegionBoundary,
    PermissionDenied,
    DmaOutsideRam,
    InvalidConfiguration,
    InvalidDeviceAccess,
    DeviceNotImplemented,
}

pub fn check_physical_access(
    address: u64,
    length: u64,
    access: PhysicalAccess,
) -> Result<PhysicalRegion, PhysicalAccessError> {
    let end = checked_end(address, length)?;
    let region = PhysicalRegion::from_address(address).ok_or(PhysicalAccessError::Unmapped)?;
    let last_region =
        PhysicalRegion::from_address(end - 1).ok_or(PhysicalAccessError::RegionBoundary)?;
    if region != last_region || !region.address_range().contains_range(address, end) {
        return Err(PhysicalAccessError::RegionBoundary);
    }
    if !matches!(
        (region, access),
        (
            PhysicalRegion::BootRom,
            PhysicalAccess::Read | PhysicalAccess::Execute
        ) | (
            PhysicalRegion::Plic,
            PhysicalAccess::Read | PhysicalAccess::Write
        ) | (
            PhysicalRegion::Uart,
            PhysicalAccess::Read | PhysicalAccess::Write
        ) | (
            PhysicalRegion::RootBlock,
            PhysicalAccess::Read | PhysicalAccess::Write
        ) | (PhysicalRegion::Ram, _)
    ) {
        return Err(PhysicalAccessError::PermissionDenied);
    }
    Ok(region)
}

pub fn check_dma_range(address: u64, length: u64) -> Result<(), PhysicalAccessError> {
    let end = checked_end(address, length)?;
    if !RAM.contains_range(address, end) {
        return Err(PhysicalAccessError::DmaOutsideRam);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VihtaPlic {
    priority: [u8; PLIC_SOURCE_COUNT as usize + 1],
    pending: u32,
    enabled: [u32; PLIC_CONTEXT_COUNT],
    threshold: [u8; PLIC_CONTEXT_COUNT],
    source_levels: u32,
    in_service: [u32; PLIC_CONTEXT_COUNT],
}

impl VihtaPlic {
    const PENDING_BASE: u64 = 0x1000;
    const ENABLE_BASE: u64 = 0x2000;
    const CONTEXT_BASE: u64 = 0x20_0000;
    const ENABLE_STRIDE: u64 = 0x80;
    const CONTEXT_STRIDE: u64 = 0x1000;
    const MAX_PRIORITY: u8 = 7;

    fn new() -> Self {
        Self {
            priority: [0; PLIC_SOURCE_COUNT as usize + 1],
            pending: 0,
            enabled: [0; PLIC_CONTEXT_COUNT],
            threshold: [0; PLIC_CONTEXT_COUNT],
            source_levels: 0,
            in_service: [0; PLIC_CONTEXT_COUNT],
        }
    }

    fn read_word(&mut self, offset: u64) -> Result<u32, PhysicalAccessError> {
        if offset < Self::PENDING_BASE {
            let source = offset / 4;
            if offset % 4 != 0 || source > u64::from(PLIC_SOURCE_COUNT) {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            return Ok(u32::from(self.priority[source as usize]));
        }
        if (Self::PENDING_BASE..Self::ENABLE_BASE).contains(&offset) {
            if offset != Self::PENDING_BASE {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            return Ok(self.pending);
        }
        if (Self::ENABLE_BASE..Self::CONTEXT_BASE).contains(&offset) {
            let relative = offset - Self::ENABLE_BASE;
            let context = (relative / Self::ENABLE_STRIDE) as usize;
            if context >= PLIC_CONTEXT_COUNT || relative % Self::ENABLE_STRIDE != 0 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            return Ok(self.enabled[context]);
        }
        let relative = offset
            .checked_sub(Self::CONTEXT_BASE)
            .ok_or(PhysicalAccessError::InvalidDeviceAccess)?;
        let context = (relative / Self::CONTEXT_STRIDE) as usize;
        let register = relative % Self::CONTEXT_STRIDE;
        if context >= PLIC_CONTEXT_COUNT || !matches!(register, 0 | 4) {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        if register == 0 {
            return Ok(u32::from(self.threshold[context]));
        }
        let claim = self.best_interrupt(context);
        if claim != 0 {
            let bit = 1u32 << claim;
            self.pending &= !bit;
            self.in_service[context] |= bit;
        }
        Ok(claim)
    }

    fn write_word(&mut self, offset: u64, value: u32) -> Result<(), PhysicalAccessError> {
        if offset < Self::PENDING_BASE {
            let source = offset / 4;
            if offset % 4 != 0 || source > u64::from(PLIC_SOURCE_COUNT) {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            if source != 0 {
                self.priority[source as usize] = (value as u8).min(Self::MAX_PRIORITY);
            }
            return Ok(());
        }
        if (Self::ENABLE_BASE..Self::CONTEXT_BASE).contains(&offset) {
            let relative = offset - Self::ENABLE_BASE;
            let context = (relative / Self::ENABLE_STRIDE) as usize;
            if context >= PLIC_CONTEXT_COUNT || relative % Self::ENABLE_STRIDE != 0 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            self.enabled[context] = value & !1;
            return Ok(());
        }
        let relative = offset
            .checked_sub(Self::CONTEXT_BASE)
            .ok_or(PhysicalAccessError::InvalidDeviceAccess)?;
        let context = (relative / Self::CONTEXT_STRIDE) as usize;
        let register = relative % Self::CONTEXT_STRIDE;
        if context >= PLIC_CONTEXT_COUNT || !matches!(register, 0 | 4) {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        if register == 0 {
            self.threshold[context] = (value as u8).min(Self::MAX_PRIORITY);
        } else if value != 0 && value <= PLIC_SOURCE_COUNT {
            let bit = 1u32 << value;
            if self.in_service[context] & bit != 0 {
                self.in_service[context] &= !bit;
                if self.source_levels & bit != 0 {
                    self.pending |= bit;
                }
            }
        }
        Ok(())
    }

    fn set_source_level(&mut self, source: u32, asserted: bool) -> Result<(), PhysicalAccessError> {
        if source == 0 || source > PLIC_SOURCE_COUNT {
            return Err(PhysicalAccessError::InvalidDeviceAccess);
        }
        let bit = 1u32 << source;
        if asserted {
            self.source_levels |= bit;
            if self.in_service.iter().all(|claimed| claimed & bit == 0) {
                self.pending |= bit;
            }
        } else {
            self.source_levels &= !bit;
        }
        Ok(())
    }

    fn interrupt_pending(&self, context: usize) -> bool {
        context < PLIC_CONTEXT_COUNT && self.best_interrupt(context) != 0
    }

    fn best_interrupt(&self, context: usize) -> u32 {
        if context >= PLIC_CONTEXT_COUNT {
            return 0;
        }
        let eligible = self.pending & self.enabled[context];
        (1..=PLIC_SOURCE_COUNT)
            .filter(|source| eligible & (1u32 << source) != 0)
            .filter(|source| self.priority[*source as usize] > self.threshold[context])
            .max_by_key(|source| (self.priority[*source as usize], std::cmp::Reverse(*source)))
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Uart16550 {
    receive_fifo: VecDeque<u8>,
    transmit_output: Vec<u8>,
    interrupt_enable: u8,
    fifo_control: u8,
    line_control: u8,
    modem_control: u8,
    modem_status: u8,
    scratch: u8,
    divisor_latch: u16,
    line_status_errors: u8,
    thre_interrupt_pending: bool,
}

impl Uart16550 {
    const FIFO_CAPACITY: usize = 16;

    fn new() -> Self {
        Self {
            receive_fifo: VecDeque::new(),
            transmit_output: Vec::new(),
            interrupt_enable: 0,
            fifo_control: 0,
            line_control: 0,
            modem_control: 0,
            modem_status: 0,
            scratch: 0,
            divisor_latch: 0,
            line_status_errors: 0,
            thre_interrupt_pending: true,
        }
    }

    fn read_register(&mut self, offset: u64) -> Result<u8, PhysicalAccessError> {
        let value = match offset {
            0 if self.line_control & 0x80 != 0 => self.divisor_latch as u8,
            0 => self.receive_fifo.pop_front().unwrap_or(0),
            1 if self.line_control & 0x80 != 0 => (self.divisor_latch >> 8) as u8,
            1 => self.interrupt_enable,
            2 => {
                let iir = self.interrupt_identification();
                if iir & 0x0f == 0x02 {
                    self.thre_interrupt_pending = false;
                }
                iir
            }
            3 => self.line_control,
            4 => self.modem_control,
            5 => {
                let status = self.line_status();
                self.line_status_errors = 0;
                status
            }
            6 => {
                let status = self.modem_status;
                self.modem_status &= 0xf0;
                status
            }
            7 => self.scratch,
            _ => return Err(PhysicalAccessError::InvalidDeviceAccess),
        };
        Ok(value)
    }

    fn write_register(&mut self, offset: u64, value: u8) -> Result<(), PhysicalAccessError> {
        match offset {
            0 if self.line_control & 0x80 != 0 => {
                self.divisor_latch = (self.divisor_latch & 0xff00) | u16::from(value);
            }
            0 => {
                self.thre_interrupt_pending = true;
                if self.modem_control & 0x10 != 0 {
                    self.receive(&[value]);
                } else {
                    self.transmit_output.push(value);
                }
            }
            1 if self.line_control & 0x80 != 0 => {
                self.divisor_latch = (self.divisor_latch & 0x00ff) | (u16::from(value) << 8);
            }
            1 => {
                let previous = self.interrupt_enable;
                self.interrupt_enable = value & 0x0f;
                if previous & 0x02 == 0 && self.interrupt_enable & 0x02 != 0 {
                    self.thre_interrupt_pending = true;
                }
            }
            2 => {
                self.fifo_control = value & 0xc1;
                if value & 0x02 != 0 {
                    self.receive_fifo.clear();
                    self.line_status_errors &= !0x02;
                }
                if value & 0x04 != 0 {
                    self.thre_interrupt_pending = true;
                }
            }
            3 => self.line_control = value,
            4 => {
                self.modem_control = value & 0x1f;
                self.update_loopback_modem_status();
            }
            7 => self.scratch = value,
            _ => return Err(PhysicalAccessError::InvalidDeviceAccess),
        }
        Ok(())
    }

    fn receive(&mut self, input: &[u8]) -> usize {
        let capacity = if self.fifo_control & 1 != 0 {
            Self::FIFO_CAPACITY
        } else {
            1
        };
        let mut accepted = 0;
        for &byte in input {
            if self.receive_fifo.len() >= capacity {
                self.line_status_errors |= 0x02;
            } else {
                self.receive_fifo.push_back(byte);
                accepted += 1;
            }
        }
        accepted
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transmit_output)
    }

    fn reset(&mut self) {
        let output = self.take_output();
        *self = Self::new();
        self.transmit_output = output;
    }

    fn update_loopback_modem_status(&mut self) {
        let previous = (self.modem_status >> 4) & 0x0f;
        let current = if self.modem_control & 0x10 != 0 {
            ((self.modem_control & 0x02) << 3)
                | ((self.modem_control & 0x01) << 5)
                | ((self.modem_control & 0x04) << 4)
                | ((self.modem_control & 0x08) << 4)
        } else {
            0
        };
        let current = (current >> 4) & 0x0f;
        let changed = previous ^ current;
        let mut delta = self.modem_status & 0x0f;
        if changed & 0x01 != 0 {
            delta |= 0x01;
        }
        if changed & 0x02 != 0 {
            delta |= 0x02;
        }
        if previous & 0x04 != 0 && current & 0x04 == 0 {
            delta |= 0x04;
        }
        if changed & 0x08 != 0 {
            delta |= 0x08;
        }
        self.modem_status = (current << 4) | delta;
    }

    fn line_status(&self) -> u8 {
        let receive_ready = u8::from(!self.receive_fifo.is_empty());
        receive_ready | self.line_status_errors | 0x60
    }

    fn interrupt_identification(&self) -> u8 {
        let receive_trigger = if self.fifo_control & 1 == 0 {
            1
        } else {
            match (self.fifo_control >> 6) & 3 {
                0 => 1,
                1 => 4,
                2 => 8,
                _ => 14,
            }
        };
        let cause = if self.interrupt_enable & 0x04 != 0 && self.line_status_errors != 0 {
            0x06
        } else if self.interrupt_enable & 0x01 != 0 && self.receive_fifo.len() >= receive_trigger {
            0x04
        } else if self.interrupt_enable & 0x02 != 0 && self.thre_interrupt_pending {
            0x02
        } else if self.interrupt_enable & 0x08 != 0 && self.modem_status & 0x0f != 0 {
            0x00
        } else {
            0x01
        };
        cause | if self.fifo_control & 1 != 0 { 0xc0 } else { 0 }
    }

    fn interrupt_asserted(&self) -> bool {
        self.interrupt_identification() & 1 == 0
    }
}

fn checked_end(address: u64, length: u64) -> Result<u64, PhysicalAccessError> {
    if length == 0 {
        return Err(PhysicalAccessError::EmptyAccess);
    }
    let end = address
        .checked_add(length)
        .ok_or(PhysicalAccessError::AddressOverflow)?;
    if address >= PHYSICAL_ADDRESS_LIMIT || end > PHYSICAL_ADDRESS_LIMIT {
        return Err(PhysicalAccessError::AddressOutOfRange);
    }
    Ok(end)
}

pub struct Kiuas16Bus {
    boot_rom: Vec<u8>,
    ram: Vec<u8>,
    ram_end: u64,
    write_epoch: u64,
    plic: VihtaPlic,
    uart: Uart16550,
}

impl Kiuas16Bus {
    pub fn new(boot_rom: Vec<u8>) -> Result<Self, PhysicalAccessError> {
        Self::new_with_ram_size(boot_rom, XV6_RAM_BYTES as usize)
    }

    pub fn new_with_ram_size(
        boot_rom: Vec<u8>,
        ram_bytes: usize,
    ) -> Result<Self, PhysicalAccessError> {
        if boot_rom.len() != BOOT_ROM_BYTES || ram_bytes == 0 || ram_bytes as u64 > XV6_RAM_BYTES {
            return Err(PhysicalAccessError::InvalidConfiguration);
        }
        let ram_end = RAM
            .start
            .checked_add(ram_bytes as u64)
            .ok_or(PhysicalAccessError::InvalidConfiguration)?;
        Ok(Self {
            boot_rom,
            ram: vec![0; ram_bytes],
            ram_end,
            write_epoch: 0,
            plic: VihtaPlic::new(),
            uart: Uart16550::new(),
        })
    }

    pub fn read(&mut self, address: u64, output: &mut [u8]) -> Result<(), PhysicalAccessError> {
        self.transfer(address, output, PhysicalAccess::Read)
    }

    pub fn fetch(&self, address: u64, output: &mut [u8]) -> Result<(), PhysicalAccessError> {
        let length =
            u64::try_from(output.len()).map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let region = self.check_access(address, length, PhysicalAccess::Execute)?;
        self.transfer_backed(address, output, region)
    }

    pub fn write(&mut self, address: u64, data: &[u8]) -> Result<(), PhysicalAccessError> {
        let length =
            u64::try_from(data.len()).map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let region = self.check_access(address, length, PhysicalAccess::Write)?;
        if region == PhysicalRegion::Plic {
            if length != 4 || address & 3 != 0 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            let value = u32::from_le_bytes(
                data.try_into()
                    .map_err(|_| PhysicalAccessError::InvalidDeviceAccess)?,
            );
            return self.plic.write_word(address - PLIC.start, value);
        }
        if region == PhysicalRegion::Uart {
            if length != 1 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            self.uart.write_register(address - UART.start, data[0])?;
            return self
                .plic
                .set_source_level(UART_PLIC_SOURCE, self.uart.interrupt_asserted());
        }
        if region != PhysicalRegion::Ram {
            return Err(PhysicalAccessError::DeviceNotImplemented);
        }
        let start = usize::try_from(address - RAM.start)
            .map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let end = start
            .checked_add(data.len())
            .ok_or(PhysicalAccessError::AddressOverflow)?;
        self.ram[start..end].copy_from_slice(data);
        self.write_epoch = self.write_epoch.wrapping_add(1);
        Ok(())
    }

    fn transfer(
        &mut self,
        address: u64,
        output: &mut [u8],
        access: PhysicalAccess,
    ) -> Result<(), PhysicalAccessError> {
        let length =
            u64::try_from(output.len()).map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let region = self.check_access(address, length, access)?;
        if region == PhysicalRegion::Plic {
            if length != 4 || address & 3 != 0 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            let value = self.plic.read_word(address - PLIC.start)?;
            output.copy_from_slice(&value.to_le_bytes());
            return Ok(());
        }
        if region == PhysicalRegion::Uart {
            if length != 1 {
                return Err(PhysicalAccessError::InvalidDeviceAccess);
            }
            output[0] = self.uart.read_register(address - UART.start)?;
            return self
                .plic
                .set_source_level(UART_PLIC_SOURCE, self.uart.interrupt_asserted());
        }
        self.transfer_backed(address, output, region)
    }

    fn transfer_backed(
        &self,
        address: u64,
        output: &mut [u8],
        region: PhysicalRegion,
    ) -> Result<(), PhysicalAccessError> {
        let region_range = region.address_range();
        let start = usize::try_from(address - region_range.start)
            .map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let end = start
            .checked_add(output.len())
            .ok_or(PhysicalAccessError::AddressOverflow)?;
        let backing = match region {
            PhysicalRegion::BootRom => &self.boot_rom,
            PhysicalRegion::Ram => &self.ram,
            PhysicalRegion::Plic | PhysicalRegion::Uart | PhysicalRegion::RootBlock => {
                return Err(PhysicalAccessError::DeviceNotImplemented);
            }
        };
        output.copy_from_slice(&backing[start..end]);
        Ok(())
    }

    pub fn set_plic_source_level(
        &mut self,
        source: u32,
        asserted: bool,
    ) -> Result<(), PhysicalAccessError> {
        self.plic.set_source_level(source, asserted)
    }

    pub fn plic_interrupt_pending(&self, context: usize) -> bool {
        self.plic.interrupt_pending(context)
    }

    pub fn receive_uart_input(&mut self, input: &[u8]) -> Result<usize, PhysicalAccessError> {
        let accepted = self.uart.receive(input);
        self.plic
            .set_source_level(UART_PLIC_SOURCE, self.uart.interrupt_asserted())?;
        Ok(accepted)
    }

    pub fn take_uart_output(&mut self) -> Vec<u8> {
        self.uart.take_output()
    }

    pub fn reset_uart(&mut self) -> Result<(), PhysicalAccessError> {
        self.uart.reset();
        self.plic.set_source_level(UART_PLIC_SOURCE, false)
    }

    fn reset_volatile_devices(&mut self) {
        self.plic = VihtaPlic::new();
        self.uart = Uart16550::new();
    }

    fn check_access(
        &self,
        address: u64,
        length: u64,
        access: PhysicalAccess,
    ) -> Result<PhysicalRegion, PhysicalAccessError> {
        let region = check_physical_access(address, length, access)?;
        let end = address
            .checked_add(length)
            .ok_or(PhysicalAccessError::AddressOverflow)?;
        if region == PhysicalRegion::Ram && end > self.ram_end {
            return Err(PhysicalAccessError::Unmapped);
        }
        Ok(region)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Extension {
    I,
    M,
    A,
    F,
    D,
    C,
    Zicsr,
    Zifencei,
    Hypervisor,
    Vector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedFeature {
    Sv39,
    Pmp,
    Sstc,
    Adue,
}

pub const SUPPORTED_EXTENSIONS: &[Extension] = &[
    Extension::I,
    Extension::M,
    Extension::A,
    Extension::C,
    Extension::Zicsr,
    Extension::Zifencei,
];

pub const UNSUPPORTED_EXTENSIONS: &[Extension] = &[
    Extension::F,
    Extension::D,
    Extension::Hypervisor,
    Extension::Vector,
];

pub const REQUIRED_PRIVILEGED_FEATURES: &[PrivilegedFeature] = &[
    PrivilegedFeature::Sv39,
    PrivilegedFeature::Pmp,
    PrivilegedFeature::Sstc,
    PrivilegedFeature::Adue,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineProfile {
    pub hart_count: u32,
    pub ram: AddressRange,
    pub physical_address_bits: u32,
    pub timebase_frequency: u64,
    pub supported_extensions: &'static [Extension],
    pub unsupported_extensions: &'static [Extension],
    pub required_privileged_features: &'static [PrivilegedFeature],
}

pub const XV6_PROFILE: MachineProfile = MachineProfile {
    hart_count: XV6_HART_COUNT,
    ram: RAM,
    physical_address_bits: PHYSICAL_ADDRESS_BITS,
    timebase_frequency: TIMEBASE_FREQUENCY,
    supported_extensions: SUPPORTED_EXTENSIONS,
    unsupported_extensions: UNSUPPORTED_EXTENSIONS,
    required_privileged_features: REQUIRED_PRIVILEGED_FEATURES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegeMode {
    Machine,
    Supervisor,
    User,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColdResetState {
    pub pc: u64,
    pub privilege: PrivilegeMode,
    pub hart_id: u64,
    pub integer_registers: [u64; 32],
    pub floating_registers: [u64; 32],
    pub fcsr: u32,
    pub mstatus: u64,
    pub mie: u64,
    pub medeleg: u64,
    pub mideleg: u64,
    pub mtvec: u64,
    pub mepc: u64,
    pub mcause: u64,
    pub mtval: u64,
    pub stvec: u64,
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
    pub satp: u64,
    pub pmpcfg0: u64,
    pub pmpaddr0: u64,
    pub mip: u64,
    pub stimecmp: u64,
    pub menvcfg: u64,
    pub mcounteren: u64,
    pub mcycle: u64,
    pub minstret: u64,
    pub time: u64,
}

pub const COLD_RESET_STATE: ColdResetState = ColdResetState {
    pc: RESET_PC,
    privilege: PrivilegeMode::Machine,
    hart_id: 0,
    integer_registers: [0; 32],
    floating_registers: [0; 32],
    fcsr: 0,
    mstatus: 0,
    mie: 0,
    medeleg: 0,
    mideleg: 0,
    mtvec: 0,
    mepc: 0,
    mcause: 0,
    mtval: 0,
    stvec: 0,
    sepc: 0,
    scause: 0,
    stval: 0,
    satp: 0,
    pmpcfg0: 0,
    pmpaddr0: 0,
    mip: 0,
    stimecmp: RESET_STIMECMP,
    menvcfg: 0,
    mcounteren: 0,
    mcycle: 0,
    minstret: 0,
    time: 0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreTrapCause {
    InstructionAddressMisaligned,
    InstructionAccessFault,
    InstructionPageFault,
    IllegalInstruction,
    Breakpoint,
    EnvironmentCallFromUser,
    EnvironmentCallFromSupervisor,
    EnvironmentCallFromMachine,
    LoadAddressMisaligned,
    LoadAccessFault,
    LoadPageFault,
    StoreAddressMisaligned,
    StoreAccessFault,
    StorePageFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreInterruptCause {
    SupervisorSoftware,
    MachineSoftware,
    SupervisorTimer,
    MachineTimer,
    SupervisorExternal,
    MachineExternal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreTrap {
    pub pc: u64,
    pub cause: CoreTrapCause,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepOutcome {
    Retired,
    TrapDelivered(CoreTrap),
    InterruptDelivered(CoreInterruptCause),
    WaitingForInterrupt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rv64Core {
    pub pc: u64,
    integer_registers: [u64; 32],
    privilege: PrivilegeMode,
    mstatus: u64,
    mie: u64,
    medeleg: u64,
    mideleg: u64,
    mtvec: u64,
    scounteren: u64,
    mscratch: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
    mip: u64,
    mcounteren: u64,
    menvcfg: u64,
    stimecmp: u64,
    time: u64,
    supervisor_timer_source_pending: bool,
    injected_external_interrupts: u64,
    plic_external_interrupts: u64,
    pmpcfg0: u64,
    pmpaddr0: u64,
    stvec: u64,
    sscratch: u64,
    sepc: u64,
    scause: u64,
    stval: u64,
    satp: u64,
    mcycle: u64,
    minstret: u64,
    waiting_for_interrupt: bool,
    reservation: Option<(u64, u8, u64)>,
}

pub struct Kiuas16Machine {
    core: Rv64Core,
    bus: Kiuas16Bus,
}

impl Kiuas16Machine {
    pub fn new(boot_rom: Vec<u8>) -> Result<Self, PhysicalAccessError> {
        Self::new_with_ram_size(boot_rom, XV6_RAM_BYTES as usize)
    }

    pub fn new_with_ram_size(
        boot_rom: Vec<u8>,
        ram_bytes: usize,
    ) -> Result<Self, PhysicalAccessError> {
        Ok(Self {
            core: Rv64Core::cold_reset(),
            bus: Kiuas16Bus::new_with_ram_size(boot_rom, ram_bytes)?,
        })
    }

    pub fn core(&self) -> &Rv64Core {
        &self.core
    }

    pub fn bus(&self) -> &Kiuas16Bus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut Kiuas16Bus {
        &mut self.bus
    }

    pub fn profile(&self) -> MachineProfile {
        MachineProfile {
            ram: AddressRange::new(RAM.start, self.bus.ram_end),
            ..XV6_PROFILE
        }
    }

    pub fn virtual_time(&self) -> u64 {
        self.core.virtual_time()
    }

    pub fn reset(&mut self) {
        self.core = Rv64Core::cold_reset();
        self.bus.reset_volatile_devices();
    }

    pub fn step(&mut self) -> Result<StepOutcome, CoreTrap> {
        self.core.step_with_trap_delivery(&mut self.bus)
    }

    pub fn receive_uart_input(&mut self, input: &[u8]) -> Result<usize, PhysicalAccessError> {
        self.bus.receive_uart_input(input)
    }

    pub fn take_uart_output(&mut self) -> Vec<u8> {
        self.bus.take_uart_output()
    }
}

const MSTATUS_SIE: u64 = 1 << 1;
const MSTATUS_MIE: u64 = 1 << 3;
const MSTATUS_SPIE: u64 = 1 << 5;
const MSTATUS_MPIE: u64 = 1 << 7;
const MSTATUS_SPP: u64 = 1 << 8;
const MSTATUS_MPP_MASK: u64 = 0b11 << 11;
const MSTATUS_MPRV: u64 = 1 << 17;
const MSTATUS_TVM: u64 = 1 << 20;
const MSTATUS_SUM: u64 = 1 << 18;
const MSTATUS_MXR: u64 = 1 << 19;
const MSTATUS_SSTATUS_MASK: u64 =
    MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP | MSTATUS_SUM | MSTATUS_MXR;
const SSTATUS_MASK: u64 = MSTATUS_SSTATUS_MASK;
const MSTATUS_WRITABLE_MASK: u64 =
    MSTATUS_SSTATUS_MASK | MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK | MSTATUS_MPRV;
const MIE_MASK: u64 = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
const MIP_SOFTWARE: u64 = 1 << 1;
const MIP_MASK: u64 = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
const MIDELEG_MASK: u64 = (1 << 1) | (1 << 5) | (1 << 9);
const MEDELEG_MASK: u64 = (1 << 16) - 1;
const SCOUNTEREN_MASK: u64 = 0b111;
const MENVCFG_MASK: u64 = (1 << 61) | (1 << 63);
const PMP_R: u8 = 1 << 0;
const PMP_W: u8 = 1 << 1;
const PMP_X: u8 = 1 << 2;
const PMP_A_MASK: u8 = 0b11 << 3;
const PMP_L: u8 = 1 << 7;

impl Rv64Core {
    fn read_csr(&self, csr: u16) -> Result<u64, ()> {
        if !self.csr_access_allowed(csr) {
            return Err(());
        }
        let value = match csr {
            0x100 => self.mstatus & SSTATUS_MASK,
            0x104 => self.mie & self.mideleg,
            0x105 => self.stvec,
            0x106 => self.scounteren,
            0x140 => self.sscratch,
            0x141 => self.sepc,
            0x142 => self.scause,
            0x143 => self.stval,
            0x144 => self.mip & self.mideleg,
            0x180 => self.satp,
            0x300 => self.mstatus,
            0x301 => (2u64 << 62) | (1 << 0) | (1 << 2) | (1 << 8) | (1 << 12),
            0x302 => self.medeleg,
            0x303 => self.mideleg,
            0x304 => self.mie,
            0x305 => self.mtvec,
            0x306 => self.mcounteren,
            0x30a => self.menvcfg,
            0x14d => self.stimecmp,
            0x3a0 => self.pmpcfg0,
            0x340 => self.mscratch,
            0x341 => self.mepc,
            0x342 => self.mcause,
            0x343 => self.mtval,
            0x344 => self.mip,
            0x3b0 => self.pmpaddr0,
            0xb00 | 0xc00 => self.mcycle,
            0xb02 | 0xc02 => self.minstret,
            0xc01 => self.time,
            0xf14 => 0,
            _ => return Err(()),
        };
        Ok(value)
    }

    fn write_csr(&mut self, csr: u16, value: u64) -> Result<(), ()> {
        if !self.csr_access_allowed(csr) || (csr >> 10) & 0x3 == 0x3 {
            return Err(());
        }
        match csr {
            0x100 => {
                self.mstatus = (self.mstatus & !SSTATUS_MASK) | (value & SSTATUS_MASK);
            }
            0x104 => self.mie = (self.mie & !self.mideleg) | (value & self.mideleg),
            0x105 => self.stvec = normalize_tvec(value),
            0x106 => self.scounteren = value & SCOUNTEREN_MASK,
            0x140 => self.sscratch = value,
            0x141 => self.sepc = value & !1,
            0x142 => self.scause = value,
            0x143 => self.stval = value,
            0x144 => self.mip = (self.mip & !self.mideleg) | (value & self.mideleg & MIP_SOFTWARE),
            0x180 => {
                let mode = value >> 60;
                self.satp = if mode == 0 || mode == 8 { value } else { 0 };
            }
            0x300 => {
                let mpp = (value & MSTATUS_MPP_MASK) >> 11;
                let value = if mpp == 2 {
                    value & !MSTATUS_MPP_MASK
                } else {
                    value
                };
                self.mstatus =
                    (self.mstatus & !MSTATUS_WRITABLE_MASK) | (value & MSTATUS_WRITABLE_MASK);
            }
            0x302 => self.medeleg = value & MEDELEG_MASK,
            0x303 => self.mideleg = value & MIDELEG_MASK,
            0x304 => self.mie = value & MIE_MASK,
            0x305 => self.mtvec = normalize_tvec(value),
            0x306 => self.mcounteren = value & SCOUNTEREN_MASK,
            0x30a => self.menvcfg = value & MENVCFG_MASK,
            0x14d => {
                self.stimecmp = value;
                self.update_supervisor_timer_pending();
            }
            0x3a0 => {
                let old = self.pmpcfg0 as u8;
                let requested = value as u8;
                let mut config = requested & (PMP_R | PMP_W | PMP_X | PMP_A_MASK | PMP_L);
                if config & PMP_W != 0 && config & PMP_R == 0 {
                    config &= !PMP_W;
                }
                self.pmpcfg0 = u64::from(if old & PMP_L != 0 { old } else { config });
            }
            0x3b0 => {
                if self.pmpcfg0 as u8 & PMP_L == 0 {
                    self.pmpaddr0 = value & ((1u64 << (PHYSICAL_ADDRESS_BITS - 2)) - 1);
                }
            }
            0x340 => self.mscratch = value,
            0x341 => self.mepc = value & !1,
            0x342 => self.mcause = value,
            0x343 => self.mtval = value,
            0x344 => self.mip = (self.mip & !MIP_SOFTWARE) | (value & MIP_SOFTWARE),
            0xb00 => self.mcycle = value,
            0xb02 => self.minstret = value,
            _ => return Err(()),
        }
        Ok(())
    }

    fn csr_access_allowed(&self, csr: u16) -> bool {
        let required_privilege = (csr >> 8) & 0x3;
        privilege_encoding(self.privilege) >= u64::from(required_privilege)
            && !(self.privilege == PrivilegeMode::Supervisor
                && csr == 0x180
                && self.mstatus & MSTATUS_TVM != 0)
            && match csr {
                0xc00 if self.privilege == PrivilegeMode::Supervisor => self.mcounteren & 1 != 0,
                0xc01 if self.privilege == PrivilegeMode::Supervisor => {
                    self.mcounteren & (1 << 1) != 0
                }
                0xc02 if self.privilege == PrivilegeMode::Supervisor => self.mcounteren & 4 != 0,
                0xc00 if self.privilege == PrivilegeMode::User => {
                    self.mcounteren & 1 != 0 && self.scounteren & 1 != 0
                }
                0xc01 if self.privilege == PrivilegeMode::User => {
                    self.mcounteren & (1 << 1) != 0 && self.scounteren & (1 << 1) != 0
                }
                0xc02 if self.privilege == PrivilegeMode::User => {
                    self.mcounteren & 4 != 0 && self.scounteren & 4 != 0
                }
                0x14d if self.privilege == PrivilegeMode::Supervisor => {
                    self.menvcfg & (1 << 63) != 0
                }
                _ => true,
            }
    }

    fn pmp_allows(&self, address: u64, length: u64, access: PhysicalAccess) -> bool {
        self.pmp_allows_as(self.effective_privilege(access), address, length, access)
    }

    fn effective_privilege(&self, access: PhysicalAccess) -> PrivilegeMode {
        if access != PhysicalAccess::Execute
            && self.privilege == PrivilegeMode::Machine
            && self.mstatus & MSTATUS_MPRV != 0
        {
            match (self.mstatus & MSTATUS_MPP_MASK) >> 11 {
                0 => PrivilegeMode::User,
                1 => PrivilegeMode::Supervisor,
                _ => PrivilegeMode::Machine,
            }
        } else {
            self.privilege
        }
    }

    fn pmp_allows_as(
        &self,
        privilege: PrivilegeMode,
        address: u64,
        length: u64,
        access: PhysicalAccess,
    ) -> bool {
        let config = self.pmpcfg0 as u8;
        let address_mode = (config & PMP_A_MASK) >> 3;
        let end = match address.checked_add(length) {
            Some(end) if length != 0 => end as u128,
            _ => return false,
        };
        let start = address as u128;
        let (region_start, region_end) = match address_mode {
            0 => return privilege == PrivilegeMode::Machine,
            1 => {
                let top = (self.pmpaddr0 as u128) << 2;
                (0, top)
            }
            2 => {
                let base = (self.pmpaddr0 as u128) << 2;
                (base, base + 4)
            }
            3 => {
                let trailing_ones = self.pmpaddr0.trailing_ones();
                let size = 1u128 << (trailing_ones + 3);
                let mask = if trailing_ones == 0 {
                    0
                } else {
                    (1u64 << trailing_ones) - 1
                };
                let base = ((self.pmpaddr0 & !mask) as u128) << 2;
                (base, base + size)
            }
            _ => return false,
        };
        if end <= region_start || start >= region_end {
            return privilege == PrivilegeMode::Machine && config & PMP_L == 0;
        }
        if start < region_start || end > region_end {
            return false;
        }
        if privilege == PrivilegeMode::Machine && config & PMP_L == 0 {
            return true;
        }
        let required = match access {
            PhysicalAccess::Read => PMP_R,
            PhysicalAccess::Write => PMP_W,
            PhysicalAccess::Execute => PMP_X,
        };
        config & required != 0
    }

    fn translate_address(
        &self,
        bus: &mut Kiuas16Bus,
        pc: u64,
        virtual_address: u64,
        access: PhysicalAccess,
    ) -> Result<u64, CoreTrap> {
        let privilege = self.effective_privilege(access);
        let page_fault = match access {
            PhysicalAccess::Execute => CoreTrapCause::InstructionPageFault,
            PhysicalAccess::Read => CoreTrapCause::LoadPageFault,
            PhysicalAccess::Write => CoreTrapCause::StorePageFault,
        };
        let fault = || CoreTrap {
            pc,
            cause: page_fault,
            value: virtual_address,
        };
        if privilege == PrivilegeMode::Machine || self.satp >> 60 == 0 {
            return Ok(virtual_address);
        }
        if self.satp >> 60 != 8
            || (((virtual_address << 25) as i64 >> 25) as u64) != virtual_address
        {
            return Err(fault());
        }

        let vpn = [
            (virtual_address >> 12) & 0x1ff,
            (virtual_address >> 21) & 0x1ff,
            (virtual_address >> 30) & 0x1ff,
        ];
        let mut table = (self.satp & ((1u64 << 44) - 1)) << 12;
        for level in (0..=2).rev() {
            let pte_address = table + vpn[level] * 8;
            if !self.pmp_allows_as(privilege, pte_address, 8, PhysicalAccess::Read) {
                return Err(CoreTrap {
                    pc,
                    cause: match access {
                        PhysicalAccess::Execute => CoreTrapCause::InstructionAccessFault,
                        PhysicalAccess::Read => CoreTrapCause::LoadAccessFault,
                        PhysicalAccess::Write => CoreTrapCause::StoreAccessFault,
                    },
                    value: virtual_address,
                });
            }
            let mut pte_bytes = [0; 8];
            bus.read(pte_address, &mut pte_bytes)
                .map_err(|_| CoreTrap {
                    pc,
                    cause: match access {
                        PhysicalAccess::Execute => CoreTrapCause::InstructionAccessFault,
                        PhysicalAccess::Read => CoreTrapCause::LoadAccessFault,
                        PhysicalAccess::Write => CoreTrapCause::StoreAccessFault,
                    },
                    value: virtual_address,
                })?;
            let mut pte = u64::from_le_bytes(pte_bytes);
            let valid = pte & 1 != 0;
            let readable = pte & 2 != 0;
            let writable = pte & 4 != 0;
            let executable = pte & 8 != 0;
            if !valid || (writable && !readable) || pte >> 54 != 0 {
                return Err(fault());
            }
            if readable || executable {
                let user_page = pte & (1 << 4) != 0;
                let permitted = match (privilege, user_page, access) {
                    (PrivilegeMode::User, true, PhysicalAccess::Read) => {
                        readable || (self.mstatus & MSTATUS_MXR != 0 && executable)
                    }
                    (PrivilegeMode::User, true, PhysicalAccess::Write) => writable,
                    (PrivilegeMode::User, true, PhysicalAccess::Execute) => executable,
                    (PrivilegeMode::Supervisor, false, PhysicalAccess::Read) => {
                        readable || (self.mstatus & MSTATUS_MXR != 0 && executable)
                    }
                    (PrivilegeMode::Supervisor, false, PhysicalAccess::Write) => writable,
                    (PrivilegeMode::Supervisor, false, PhysicalAccess::Execute) => executable,
                    (PrivilegeMode::Supervisor, true, PhysicalAccess::Read) => {
                        self.mstatus & MSTATUS_SUM != 0
                            && (readable || (self.mstatus & MSTATUS_MXR != 0 && executable))
                    }
                    (PrivilegeMode::Supervisor, true, PhysicalAccess::Write) => {
                        self.mstatus & MSTATUS_SUM != 0 && writable
                    }
                    _ => false,
                };
                let lower_ppn_bits = level * 9;
                let ppn = (pte >> 10) & ((1u64 << 44) - 1);
                if !permitted || (lower_ppn_bits != 0 && ppn & ((1u64 << lower_ppn_bits) - 1) != 0)
                {
                    return Err(fault());
                }
                let required_ad = (1 << 6)
                    | if access == PhysicalAccess::Write {
                        1 << 7
                    } else {
                        0
                    };
                if pte & required_ad != required_ad {
                    if self.menvcfg & (1 << 61) == 0 {
                        return Err(fault());
                    }
                    pte |= required_ad;
                    if !self.pmp_allows_as(privilege, pte_address, 8, PhysicalAccess::Write) {
                        return Err(CoreTrap {
                            pc,
                            cause: match access {
                                PhysicalAccess::Execute => CoreTrapCause::InstructionAccessFault,
                                PhysicalAccess::Read => CoreTrapCause::LoadAccessFault,
                                PhysicalAccess::Write => CoreTrapCause::StoreAccessFault,
                            },
                            value: virtual_address,
                        });
                    }
                    bus.write(pte_address, &pte.to_le_bytes())
                        .map_err(|_| CoreTrap {
                            pc,
                            cause: match access {
                                PhysicalAccess::Execute => CoreTrapCause::InstructionAccessFault,
                                PhysicalAccess::Read => CoreTrapCause::LoadAccessFault,
                                PhysicalAccess::Write => CoreTrapCause::StoreAccessFault,
                            },
                            value: virtual_address,
                        })?;
                }
                let page_offset = virtual_address & 0xfff;
                let vpn_mask = if lower_ppn_bits == 0 {
                    0
                } else {
                    (1u64 << lower_ppn_bits) - 1
                };
                let physical_ppn = (ppn & !vpn_mask) | ((virtual_address >> 12) & vpn_mask);
                return Ok((physical_ppn << 12) | page_offset);
            }
            if level == 0 || pte & ((1 << 4) | (1 << 6) | (1 << 7)) != 0 {
                return Err(fault());
            }
            table = ((pte >> 10) & ((1u64 << 44) - 1)) << 12;
        }
        Err(fault())
    }

    pub const fn cold_reset() -> Self {
        Self {
            pc: COLD_RESET_STATE.pc,
            integer_registers: COLD_RESET_STATE.integer_registers,
            privilege: COLD_RESET_STATE.privilege,
            mstatus: COLD_RESET_STATE.mstatus,
            mie: COLD_RESET_STATE.mie,
            medeleg: COLD_RESET_STATE.medeleg,
            mideleg: COLD_RESET_STATE.mideleg,
            mtvec: COLD_RESET_STATE.mtvec,
            scounteren: 0,
            mscratch: 0,
            mepc: COLD_RESET_STATE.mepc,
            mcause: COLD_RESET_STATE.mcause,
            mtval: COLD_RESET_STATE.mtval,
            mip: COLD_RESET_STATE.mip,
            mcounteren: COLD_RESET_STATE.mcounteren,
            menvcfg: COLD_RESET_STATE.menvcfg,
            stimecmp: COLD_RESET_STATE.stimecmp,
            time: COLD_RESET_STATE.time,
            supervisor_timer_source_pending: false,
            injected_external_interrupts: 0,
            plic_external_interrupts: 0,
            pmpcfg0: COLD_RESET_STATE.pmpcfg0,
            pmpaddr0: COLD_RESET_STATE.pmpaddr0,
            stvec: COLD_RESET_STATE.stvec,
            sscratch: 0,
            sepc: COLD_RESET_STATE.sepc,
            scause: COLD_RESET_STATE.scause,
            stval: COLD_RESET_STATE.stval,
            satp: COLD_RESET_STATE.satp,
            mcycle: COLD_RESET_STATE.mcycle,
            minstret: COLD_RESET_STATE.minstret,
            waiting_for_interrupt: false,
            reservation: None,
        }
    }

    pub fn privilege(&self) -> PrivilegeMode {
        self.privilege
    }

    pub fn virtual_time(&self) -> u64 {
        self.time
    }

    pub fn register(&self, index: usize) -> Option<u64> {
        self.integer_registers.get(index).copied()
    }

    pub fn set_interrupt_pending(&mut self, cause: CoreInterruptCause, pending: bool) {
        let bit = interrupt_bit(cause);
        if cause == CoreInterruptCause::SupervisorTimer {
            self.supervisor_timer_source_pending = pending;
            self.update_supervisor_timer_pending();
            return;
        }
        if matches!(
            cause,
            CoreInterruptCause::MachineExternal | CoreInterruptCause::SupervisorExternal
        ) {
            if pending {
                self.injected_external_interrupts |= bit;
            } else {
                self.injected_external_interrupts &= !bit;
            }
            self.update_external_interrupt_pending();
            return;
        }
        if pending {
            self.mip |= bit;
        } else {
            self.mip &= !bit;
        }
    }

    pub fn deliver_trap(&mut self, trap: CoreTrap) -> Result<(), CoreTrap> {
        let cause = exception_code(trap.cause).ok_or(trap)?;
        let delegated =
            self.privilege != PrivilegeMode::Machine && self.medeleg & (1 << cause) != 0;
        if delegated {
            let previous_interrupt_enable = self.mstatus & MSTATUS_SIE != 0;
            self.mstatus = (self.mstatus & !(MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP))
                | if previous_interrupt_enable {
                    MSTATUS_SPIE
                } else {
                    0
                }
                | if self.privilege == PrivilegeMode::Supervisor {
                    MSTATUS_SPP
                } else {
                    0
                };
            self.sepc = trap.pc & !1;
            self.scause = cause;
            self.stval = trap.value;
            self.privilege = PrivilegeMode::Supervisor;
            self.pc = self.stvec & !3;
        } else {
            let previous_interrupt_enable = self.mstatus & MSTATUS_MIE != 0;
            let previous_privilege = privilege_encoding(self.privilege);
            self.mstatus = (self.mstatus & !(MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK))
                | if previous_interrupt_enable {
                    MSTATUS_MPIE
                } else {
                    0
                }
                | (previous_privilege << 11);
            self.mepc = trap.pc & !1;
            self.mcause = cause;
            self.mtval = trap.value;
            self.privilege = PrivilegeMode::Machine;
            self.pc = self.mtvec & !3;
        }
        self.reservation = None;
        Ok(())
    }

    pub fn step(&mut self, bus: &mut Kiuas16Bus) -> Result<(), CoreTrap> {
        self.advance_cycle();
        let result = self.step_inner(bus);
        if result.is_ok() {
            self.minstret = self.minstret.wrapping_add(1);
        }
        result
    }

    pub fn step_with_trap_delivery(
        &mut self,
        bus: &mut Kiuas16Bus,
    ) -> Result<StepOutcome, CoreTrap> {
        self.plic_external_interrupts = (if bus.plic_interrupt_pending(0) {
            interrupt_bit(CoreInterruptCause::MachineExternal)
        } else {
            0
        }) | (if bus.plic_interrupt_pending(1) {
            interrupt_bit(CoreInterruptCause::SupervisorExternal)
        } else {
            0
        });
        self.update_external_interrupt_pending();
        self.update_supervisor_timer_pending();
        if let Some(cause) = self.pending_interrupt() {
            self.advance_cycle();
            self.deliver_interrupt(cause);
            return Ok(StepOutcome::InterruptDelivered(cause));
        }
        if self.waiting_for_interrupt {
            if self.mip & MIP_MASK == 0 {
                self.advance_cycle();
                return Ok(StepOutcome::WaitingForInterrupt);
            }
            self.waiting_for_interrupt = false;
        }
        match self.step(bus) {
            Ok(()) => Ok(StepOutcome::Retired),
            Err(trap) => {
                self.deliver_trap(trap)?;
                Ok(StepOutcome::TrapDelivered(trap))
            }
        }
    }

    fn advance_cycle(&mut self) {
        self.mcycle = self.mcycle.wrapping_add(1);
        self.time = self.time.wrapping_add(1);
        self.update_supervisor_timer_pending();
    }

    fn update_supervisor_timer_pending(&mut self) {
        let bit = interrupt_bit(CoreInterruptCause::SupervisorTimer);
        if self.supervisor_timer_source_pending || self.time >= self.stimecmp {
            self.mip |= bit;
        } else {
            self.mip &= !bit;
        }
    }

    fn update_external_interrupt_pending(&mut self) {
        let mask = interrupt_bit(CoreInterruptCause::MachineExternal)
            | interrupt_bit(CoreInterruptCause::SupervisorExternal);
        self.mip = (self.mip & !mask)
            | ((self.injected_external_interrupts | self.plic_external_interrupts) & mask);
    }

    fn pending_interrupt(&self) -> Option<CoreInterruptCause> {
        const PRIORITY: [CoreInterruptCause; 6] = [
            CoreInterruptCause::MachineExternal,
            CoreInterruptCause::MachineSoftware,
            CoreInterruptCause::MachineTimer,
            CoreInterruptCause::SupervisorExternal,
            CoreInterruptCause::SupervisorSoftware,
            CoreInterruptCause::SupervisorTimer,
        ];
        PRIORITY.into_iter().find(|cause| {
            let bit = interrupt_bit(*cause);
            if self.mip & self.mie & bit == 0 {
                return false;
            }
            let delegated = self.mideleg & bit != 0;
            if self.privilege == PrivilegeMode::Machine && delegated {
                return false;
            }
            let target = if delegated {
                PrivilegeMode::Supervisor
            } else {
                PrivilegeMode::Machine
            };
            privilege_encoding(self.privilege) < privilege_encoding(target)
                || (self.privilege == target
                    && self.mstatus
                        & (if target == PrivilegeMode::Machine {
                            MSTATUS_MIE
                        } else {
                            MSTATUS_SIE
                        })
                        != 0)
        })
    }

    fn deliver_interrupt(&mut self, cause: CoreInterruptCause) {
        let code = interrupt_code(cause);
        let delegated =
            self.privilege != PrivilegeMode::Machine && self.mideleg & interrupt_bit(cause) != 0;
        if delegated {
            let previous_interrupt_enable = self.mstatus & MSTATUS_SIE != 0;
            self.mstatus = (self.mstatus & !(MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP))
                | if previous_interrupt_enable {
                    MSTATUS_SPIE
                } else {
                    0
                }
                | if self.privilege == PrivilegeMode::Supervisor {
                    MSTATUS_SPP
                } else {
                    0
                };
            self.sepc = self.pc & !1;
            self.scause = (1 << 63) | code;
            self.stval = 0;
            self.privilege = PrivilegeMode::Supervisor;
            self.pc = trap_vector(self.stvec, code, true);
        } else {
            let previous_interrupt_enable = self.mstatus & MSTATUS_MIE != 0;
            let previous_privilege = privilege_encoding(self.privilege);
            self.mstatus = (self.mstatus & !(MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK))
                | if previous_interrupt_enable {
                    MSTATUS_MPIE
                } else {
                    0
                }
                | (previous_privilege << 11);
            self.mepc = self.pc & !1;
            self.mcause = (1 << 63) | code;
            self.mtval = 0;
            self.privilege = PrivilegeMode::Machine;
            self.pc = trap_vector(self.mtvec, code, true);
        }
        self.waiting_for_interrupt = false;
        self.reservation = None;
    }

    fn step_inner(&mut self, bus: &mut Kiuas16Bus) -> Result<(), CoreTrap> {
        let pc = self.pc;
        if pc & 1 != 0 {
            return Err(CoreTrap {
                pc,
                cause: CoreTrapCause::InstructionAddressMisaligned,
                value: pc,
            });
        }

        let physical_pc = self.translate_address(bus, pc, pc, PhysicalAccess::Execute)?;
        let mut halfword = [0; 2];
        if !self.pmp_allows(physical_pc, 2, PhysicalAccess::Execute) {
            return Err(CoreTrap {
                pc,
                cause: CoreTrapCause::InstructionAccessFault,
                value: pc,
            });
        }
        bus.fetch(physical_pc, &mut halfword)
            .map_err(|_| CoreTrap {
                pc,
                cause: CoreTrapCause::InstructionAccessFault,
                value: pc,
            })?;
        let halfword = u16::from_le_bytes(halfword);
        let (instruction, instruction_length) = if halfword & 0b11 == 0b11 {
            let second_pc = pc.checked_add(2).ok_or(CoreTrap {
                pc,
                cause: CoreTrapCause::InstructionAccessFault,
                value: pc,
            })?;
            let physical_second_pc =
                self.translate_address(bus, pc, second_pc, PhysicalAccess::Execute)?;
            if !self.pmp_allows(physical_second_pc, 2, PhysicalAccess::Execute) {
                return Err(CoreTrap {
                    pc,
                    cause: CoreTrapCause::InstructionAccessFault,
                    value: second_pc,
                });
            }
            let mut second_halfword = [0; 2];
            bus.fetch(physical_second_pc, &mut second_halfword)
                .map_err(|_| CoreTrap {
                    pc,
                    cause: CoreTrapCause::InstructionAccessFault,
                    value: second_pc,
                })?;
            (
                u32::from_le_bytes([
                    halfword as u8,
                    (halfword >> 8) as u8,
                    second_halfword[0],
                    second_halfword[1],
                ]),
                4,
            )
        } else {
            let instruction = decompress_rv64c(halfword).ok_or(CoreTrap {
                pc,
                cause: CoreTrapCause::IllegalInstruction,
                value: halfword as u64,
            })?;
            (instruction, 2)
        };
        self.execute_instruction(bus, pc, instruction, instruction_length)
    }

    fn execute_instruction(
        &mut self,
        bus: &mut Kiuas16Bus,
        pc: u64,
        instruction: u32,
        instruction_length: u64,
    ) -> Result<(), CoreTrap> {
        let opcode = instruction & 0x7f;
        let rd = ((instruction >> 7) & 0x1f) as usize;
        let funct3 = (instruction >> 12) & 0x7;
        let rs1 = ((instruction >> 15) & 0x1f) as usize;
        let rs2 = ((instruction >> 20) & 0x1f) as usize;
        let left = self.integer_registers[rs1];
        let right = self.integer_registers[rs2];
        let mut next_pc = pc.wrapping_add(instruction_length);
        let mut result = None;

        match opcode {
            0x73 if instruction == 0x0010_0073 => {
                return Err(CoreTrap {
                    pc,
                    cause: CoreTrapCause::Breakpoint,
                    value: 0,
                });
            }
            0x73 if instruction == 0x0000_0073 => {
                let cause = match self.privilege {
                    PrivilegeMode::User => CoreTrapCause::EnvironmentCallFromUser,
                    PrivilegeMode::Supervisor => CoreTrapCause::EnvironmentCallFromSupervisor,
                    PrivilegeMode::Machine => CoreTrapCause::EnvironmentCallFromMachine,
                };
                return Err(CoreTrap {
                    pc,
                    cause,
                    value: 0,
                });
            }
            0x73 if instruction == 0x3020_0073 => {
                if self.privilege != PrivilegeMode::Machine {
                    return Err(illegal_instruction(pc, instruction));
                }
                let target_privilege = match (self.mstatus & MSTATUS_MPP_MASK) >> 11 {
                    0 => PrivilegeMode::User,
                    1 => PrivilegeMode::Supervisor,
                    _ => PrivilegeMode::Machine,
                };
                self.mstatus = (self.mstatus & !(MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK))
                    | if self.mstatus & MSTATUS_MPIE != 0 {
                        MSTATUS_MIE
                    } else {
                        0
                    }
                    | MSTATUS_MPIE;
                if target_privilege != PrivilegeMode::Machine {
                    self.mstatus &= !MSTATUS_MPRV;
                }
                self.privilege = target_privilege;
                next_pc = self.mepc;
            }
            0x73 if instruction == 0x1020_0073 => {
                if self.privilege == PrivilegeMode::User {
                    return Err(illegal_instruction(pc, instruction));
                }
                let target_privilege = if self.mstatus & MSTATUS_SPP != 0 {
                    PrivilegeMode::Supervisor
                } else {
                    PrivilegeMode::User
                };
                self.mstatus = (self.mstatus & !(MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP))
                    | if self.mstatus & MSTATUS_SPIE != 0 {
                        MSTATUS_SIE
                    } else {
                        0
                    }
                    | MSTATUS_SPIE;
                self.privilege = target_privilege;
                next_pc = self.sepc;
            }
            0x73 if instruction & 0xfe00_7fff == 0x1200_0073 => {
                if self.privilege == PrivilegeMode::User
                    || (self.privilege == PrivilegeMode::Supervisor
                        && self.mstatus & MSTATUS_TVM != 0)
                {
                    return Err(illegal_instruction(pc, instruction));
                }
            }
            0x73 if instruction == 0x1050_0073 => {
                self.waiting_for_interrupt = true;
            }
            0x73 if funct3 != 0 => {
                let csr = (instruction >> 20) as u16;
                let old = self
                    .read_csr(csr)
                    .map_err(|_| illegal_instruction(pc, instruction))?;
                let immediate_form = funct3 >= 5;
                let source = if immediate_form { rs1 as u64 } else { left };
                let write_value = match funct3 {
                    1 | 5 => Some(source),
                    2 | 6 if rs1 != 0 => Some(old | source),
                    3 | 7 if rs1 != 0 => Some(old & !source),
                    2 | 3 | 6 | 7 => None,
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                if let Some(value) = write_value {
                    self.write_csr(csr, value)
                        .map_err(|_| illegal_instruction(pc, instruction))?;
                }
                result = Some(old);
            }
            0x03 => {
                let address = left.wrapping_add(sign_extend(instruction >> 20, 12) as u64);
                let (length, signed) = match funct3 {
                    0 => (1, true),
                    1 => (2, true),
                    2 => (4, true),
                    3 => (8, true),
                    4 => (1, false),
                    5 => (2, false),
                    6 => (4, false),
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                if address & (length - 1) != 0 {
                    return Err(CoreTrap {
                        pc,
                        cause: CoreTrapCause::LoadAddressMisaligned,
                        value: address,
                    });
                }
                let physical_address =
                    self.translate_address(bus, pc, address, PhysicalAccess::Read)?;
                if !self.pmp_allows(physical_address, length, PhysicalAccess::Read) {
                    return Err(CoreTrap {
                        pc,
                        cause: CoreTrapCause::LoadAccessFault,
                        value: address,
                    });
                }
                let mut data = [0; 8];
                bus.read(physical_address, &mut data[..length as usize])
                    .map_err(|_| CoreTrap {
                        pc,
                        cause: CoreTrapCause::LoadAccessFault,
                        value: address,
                    })?;
                let value = u64::from_le_bytes(data);
                result = Some(if signed {
                    if length == 8 {
                        value
                    } else {
                        sign_extend(value as u32, (length * 8) as u32) as u64
                    }
                } else {
                    value
                });
            }
            0x23 => {
                let immediate = ((instruction >> 7) & 0x1f) | (((instruction >> 25) & 0x7f) << 5);
                let address = left.wrapping_add(sign_extend(immediate, 12) as u64);
                let length = match funct3 {
                    0 => 1,
                    1 => 2,
                    2 => 4,
                    3 => 8,
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                if address & (length - 1) != 0 {
                    return Err(CoreTrap {
                        pc,
                        cause: CoreTrapCause::StoreAddressMisaligned,
                        value: address,
                    });
                }
                let physical_address =
                    self.translate_address(bus, pc, address, PhysicalAccess::Write)?;
                if !self.pmp_allows(physical_address, length, PhysicalAccess::Write) {
                    return Err(CoreTrap {
                        pc,
                        cause: CoreTrapCause::StoreAccessFault,
                        value: address,
                    });
                }
                bus.write(physical_address, &right.to_le_bytes()[..length as usize])
                    .map_err(|_| CoreTrap {
                        pc,
                        cause: CoreTrapCause::StoreAccessFault,
                        value: address,
                    })?;
                self.reservation = None;
            }
            0x2f => {
                let length = match funct3 {
                    2 => 4,
                    3 => 8,
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                let address = left;
                if address & (length - 1) != 0 {
                    return Err(CoreTrap {
                        pc,
                        cause: if (instruction >> 27) & 0x1f == 2 {
                            CoreTrapCause::LoadAddressMisaligned
                        } else {
                            CoreTrapCause::StoreAddressMisaligned
                        },
                        value: address,
                    });
                }

                let operation = (instruction >> 27) & 0x1f;
                let access = if operation == 0b00010 && rs2 == 0 {
                    PhysicalAccess::Read
                } else {
                    PhysicalAccess::Write
                };
                let physical_address = self.translate_address(bus, pc, address, access)?;
                if !self.pmp_allows(physical_address, length, access)
                    || (access == PhysicalAccess::Write
                        && operation != 0b00011
                        && !self.pmp_allows(physical_address, length, PhysicalAccess::Read))
                {
                    return Err(CoreTrap {
                        pc,
                        cause: if access == PhysicalAccess::Read {
                            CoreTrapCause::LoadAccessFault
                        } else {
                            CoreTrapCause::StoreAccessFault
                        },
                        value: address,
                    });
                }
                let mut data = [0; 8];
                match operation {
                    0b00010 if rs2 == 0 => {
                        let region = bus
                            .check_access(physical_address, length, PhysicalAccess::Read)
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::LoadAccessFault,
                                value: address,
                            })?;
                        if region != PhysicalRegion::Ram {
                            return Err(CoreTrap {
                                pc,
                                cause: CoreTrapCause::LoadAccessFault,
                                value: address,
                            });
                        }
                        bus.read(physical_address, &mut data[..length as usize])
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::LoadAccessFault,
                                value: address,
                            })?;
                        self.reservation = Some((physical_address, length as u8, bus.write_epoch));
                        let old = u64::from_le_bytes(data);
                        result = Some(if length == 4 {
                            old as u32 as i32 as i64 as u64
                        } else {
                            old
                        });
                    }
                    0b00011 => {
                        let region = bus
                            .check_access(physical_address, length, PhysicalAccess::Write)
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::StoreAccessFault,
                                value: address,
                            })?;
                        if region != PhysicalRegion::Ram {
                            return Err(CoreTrap {
                                pc,
                                cause: CoreTrapCause::StoreAccessFault,
                                value: address,
                            });
                        }
                        let reserved = self.reservation.take();
                        let succeeded =
                            reserved == Some((physical_address, length as u8, bus.write_epoch));
                        if succeeded {
                            bus.write(physical_address, &right.to_le_bytes()[..length as usize])
                                .map_err(|_| CoreTrap {
                                    pc,
                                    cause: CoreTrapCause::StoreAccessFault,
                                    value: address,
                                })?;
                        }
                        result = Some(u64::from(!succeeded));
                    }
                    0b00001 | 0b00000 | 0b00100 | 0b01100 | 0b01000 | 0b10000 | 0b10100
                    | 0b11000 | 0b11100 => {
                        let region = bus
                            .check_access(physical_address, length, PhysicalAccess::Write)
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::StoreAccessFault,
                                value: address,
                            })?;
                        if region != PhysicalRegion::Ram {
                            return Err(CoreTrap {
                                pc,
                                cause: CoreTrapCause::StoreAccessFault,
                                value: address,
                            });
                        }
                        bus.read(physical_address, &mut data[..length as usize])
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::LoadAccessFault,
                                value: address,
                            })?;
                        let old = u64::from_le_bytes(data);
                        let mask = if length == 4 {
                            u32::MAX as u64
                        } else {
                            u64::MAX
                        };
                        let old_width = old & mask;
                        let operand = right & mask;
                        let new = match operation {
                            0b00001 => operand,
                            0b00000 => old_width.wrapping_add(operand) & mask,
                            0b00100 => old_width ^ operand,
                            0b01100 => old_width & operand,
                            0b01000 => old_width | operand,
                            0b10000 if length == 4 => {
                                if (old_width as i32) < (operand as i32) {
                                    old_width
                                } else {
                                    operand
                                }
                            }
                            0b10000 => {
                                if (old_width as i64) < (operand as i64) {
                                    old_width
                                } else {
                                    operand
                                }
                            }
                            0b10100 if length == 4 => {
                                if (old_width as i32) > (operand as i32) {
                                    old_width
                                } else {
                                    operand
                                }
                            }
                            0b10100 => {
                                if (old_width as i64) > (operand as i64) {
                                    old_width
                                } else {
                                    operand
                                }
                            }
                            0b11000 => old_width.min(operand),
                            0b11100 => old_width.max(operand),
                            _ => return Err(illegal_instruction(pc, instruction)),
                        };
                        bus.write(physical_address, &new.to_le_bytes()[..length as usize])
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::StoreAccessFault,
                                value: address,
                            })?;
                        self.reservation = None;
                        result = Some(if length == 4 {
                            old_width as u32 as i32 as i64 as u64
                        } else {
                            old_width
                        });
                    }
                    _ => return Err(illegal_instruction(pc, instruction)),
                }
            }
            0x13 => {
                let immediate = sign_extend(instruction >> 20, 12) as u64;
                result = Some(match funct3 {
                    0 => left.wrapping_add(immediate),
                    2 => u64::from((left as i64) < (immediate as i64)),
                    3 => u64::from(left < immediate),
                    4 => left ^ immediate,
                    6 => left | immediate,
                    7 => left & immediate,
                    1 if instruction >> 26 == 0 => left.wrapping_shl((instruction >> 20) & 0x3f),
                    5 if instruction >> 26 == 0 => left.wrapping_shr((instruction >> 20) & 0x3f),
                    5 if instruction >> 26 == 0x10 => {
                        ((left as i64) >> ((instruction >> 20) & 0x3f)) as u64
                    }
                    _ => return Err(illegal_instruction(pc, instruction)),
                });
            }
            0x1b => {
                let immediate = sign_extend(instruction >> 20, 12) as u64;
                let word = match funct3 {
                    0 => left.wrapping_add(immediate) as u32,
                    1 if instruction >> 25 == 0 => {
                        (left as u32).wrapping_shl((instruction >> 20) & 0x1f)
                    }
                    5 if instruction >> 25 == 0 => {
                        (left as u32).wrapping_shr((instruction >> 20) & 0x1f)
                    }
                    5 if instruction >> 25 == 0x20 => {
                        ((left as i32) >> ((instruction >> 20) & 0x1f)) as u32
                    }
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                result = Some(word as i32 as i64 as u64);
            }
            0x33 => {
                let shift = (right & 0x3f) as u32;
                let funct7 = instruction >> 25;
                result = Some(match (funct3, funct7) {
                    (0, 0) => left.wrapping_add(right),
                    (0, 0x20) => left.wrapping_sub(right),
                    (1, 0) => left.wrapping_shl(shift),
                    (2, 0) => u64::from((left as i64) < (right as i64)),
                    (3, 0) => u64::from(left < right),
                    (4, 0) => left ^ right,
                    (5, 0) => left.wrapping_shr(shift),
                    (5, 0x20) => ((left as i64) >> shift) as u64,
                    (6, 0) => left | right,
                    (7, 0) => left & right,
                    (0, 1) => left.wrapping_mul(right),
                    (1, 1) => ((left as i64 as i128 * right as i64 as i128) >> 64) as u64,
                    (2, 1) => multiply_high_signed_unsigned(left, right),
                    (3, 1) => ((left as u128 * right as u128) >> 64) as u64,
                    (4, 1) if right == 0 => u64::MAX,
                    (4, 1) if left == 1 << 63 && right == u64::MAX => left,
                    (4, 1) => ((left as i64) / (right as i64)) as u64,
                    (5, 1) if right == 0 => u64::MAX,
                    (5, 1) => left / right,
                    (6, 1) if right == 0 => left,
                    (6, 1) if left == 1 << 63 && right == u64::MAX => 0,
                    (6, 1) => ((left as i64) % (right as i64)) as u64,
                    (7, 1) if right == 0 => left,
                    (7, 1) => left % right,
                    _ => return Err(illegal_instruction(pc, instruction)),
                });
            }
            0x3b => {
                let shift = (right & 0x1f) as u32;
                let funct7 = instruction >> 25;
                let word = match (funct3, funct7) {
                    (0, 0) => (left as u32).wrapping_add(right as u32),
                    (0, 0x20) => (left as u32).wrapping_sub(right as u32),
                    (1, 0) => (left as u32).wrapping_shl(shift),
                    (5, 0) => (left as u32).wrapping_shr(shift),
                    (5, 0x20) => ((left as i32) >> shift) as u32,
                    (0, 1) => (left as u32).wrapping_mul(right as u32),
                    (4, 1) if right as u32 == 0 => u32::MAX,
                    (4, 1) if left as u32 == 1 << 31 && right as u32 == u32::MAX => left as u32,
                    (4, 1) => ((left as i32) / (right as i32)) as u32,
                    (5, 1) if right as u32 == 0 => u32::MAX,
                    (5, 1) => (left as u32) / (right as u32),
                    (6, 1) if right as u32 == 0 => left as u32,
                    (6, 1) if left as u32 == 1 << 31 && right as u32 == u32::MAX => 0,
                    (6, 1) => ((left as i32) % (right as i32)) as u32,
                    (7, 1) if right as u32 == 0 => left as u32,
                    (7, 1) => (left as u32) % (right as u32),
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                result = Some(word as i32 as i64 as u64);
            }
            0x0f if funct3 == 0 && rd == 0 && rs1 == 0 && instruction >> 28 == 0 => {}
            0x0f if funct3 == 1 && rd == 0 && rs1 == 0 && instruction >> 20 == 0 => {}
            0x37 => {
                result = Some(sign_extend(instruction & 0xffff_f000, 32) as u64);
            }
            0x17 => {
                result = Some(pc.wrapping_add(sign_extend(instruction & 0xffff_f000, 32) as u64));
            }
            0x63 => {
                let immediate = ((instruction >> 31) << 12)
                    | (((instruction >> 7) & 1) << 11)
                    | (((instruction >> 25) & 0x3f) << 5)
                    | (((instruction >> 8) & 0xf) << 1);
                let take = match funct3 {
                    0 => left == right,
                    1 => left != right,
                    4 => (left as i64) < (right as i64),
                    5 => (left as i64) >= (right as i64),
                    6 => left < right,
                    7 => left >= right,
                    _ => return Err(illegal_instruction(pc, instruction)),
                };
                if take {
                    next_pc = pc.wrapping_add(sign_extend(immediate, 13) as u64);
                }
            }
            0x6f => {
                let immediate = ((instruction >> 31) << 20)
                    | (((instruction >> 12) & 0xff) << 12)
                    | (((instruction >> 20) & 1) << 11)
                    | (((instruction >> 21) & 0x3ff) << 1);
                result = Some(next_pc);
                next_pc = pc.wrapping_add(sign_extend(immediate, 21) as u64);
            }
            0x67 if funct3 == 0 => {
                let target = left.wrapping_add(sign_extend(instruction >> 20, 12) as u64) & !1;
                result = Some(next_pc);
                next_pc = target;
            }
            _ => return Err(illegal_instruction(pc, instruction)),
        }

        if let Some(value) = result {
            if rd != 0 {
                self.integer_registers[rd] = value;
            }
        }
        self.integer_registers[0] = 0;
        self.pc = next_pc;
        Ok(())
    }
}

fn decompress_rv64c(instruction: u16) -> Option<u32> {
    let quadrant = instruction & 0b11;
    let funct3 = instruction >> 13;
    let rd = ((instruction >> 7) & 0x1f) as u32;
    let rs2 = ((instruction >> 2) & 0x1f) as u32;
    let rd_prime = ((instruction >> 2) & 0x7) as u32 + 8;
    let rs1_prime = ((instruction >> 7) & 0x7) as u32 + 8;
    let rs2_prime = rd_prime;
    let ci_imm = sign_extend(
        (((instruction >> 12) as u32 & 1) << 5) | ((instruction >> 2) as u32 & 0x1f),
        6,
    );
    let encode_i = |imm: i64, rs1: u32, funct3: u32, rd: u32, opcode: u32| {
        (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
    };
    let encode_s = |imm: u32, rs1: u32, rs2: u32, funct3: u32| {
        ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | ((imm & 0x1f) << 7) | 0x23
    };
    let encode_b = |imm: i64, rs1: u32, rs2: u32, funct3: u32| {
        let imm = imm as u32;
        (((imm >> 12) & 1) << 31)
            | (((imm >> 5) & 0x3f) << 25)
            | (rs2 << 20)
            | (rs1 << 15)
            | (funct3 << 12)
            | (((imm >> 1) & 0xf) << 8)
            | (((imm >> 11) & 1) << 7)
            | 0x63
    };
    let encode_j = |imm: i64, rd: u32| {
        let imm = imm as u32;
        (((imm >> 20) & 1) << 31)
            | (((imm >> 1) & 0x3ff) << 21)
            | (((imm >> 11) & 1) << 20)
            | (((imm >> 12) & 0xff) << 12)
            | (rd << 7)
            | 0x6f
    };

    match (quadrant, funct3) {
        (0b00, 0b000) => {
            let immediate = (((instruction as u32 >> 11) & 0x3) << 4)
                | (((instruction as u32 >> 7) & 0xf) << 6)
                | (((instruction as u32 >> 6) & 0x1) << 2)
                | (((instruction as u32 >> 5) & 0x1) << 3);
            (immediate != 0).then(|| encode_i(immediate as i64, 2, 0, rd_prime, 0x13))
        }
        (0b00, 0b010 | 0b110) => {
            let offset = (((instruction as u32 >> 10) & 0x7) << 3)
                | (((instruction as u32 >> 6) & 0x1) << 2)
                | (((instruction as u32 >> 5) & 0x1) << 6);
            let store = funct3 == 0b110;
            Some(if store {
                encode_s(offset, rs1_prime, rs2_prime, 0b010)
            } else {
                encode_i(offset as i64, rs1_prime, 0b010, rd_prime, 0x03)
            })
        }
        (0b00, 0b011 | 0b111) => {
            let offset = (((instruction as u32 >> 10) & 0x7) << 3)
                | (((instruction as u32 >> 5) & 0x3) << 6);
            let store = funct3 == 0b111;
            Some(if store {
                encode_s(offset, rs1_prime, rs2_prime, 0b011)
            } else {
                encode_i(offset as i64, rs1_prime, 0b011, rd_prime, 0x03)
            })
        }
        (0b01, 0b000) => Some(encode_i(ci_imm, rd, 0, rd, 0x13)),
        (0b01, 0b001) if rd != 0 => Some(encode_i(ci_imm, rd, 0, rd, 0x1b)),
        (0b01, 0b010) => Some(encode_i(ci_imm, 0, 0, rd, 0x13)),
        (0b01, 0b011) if rd == 2 => {
            let immediate = sign_extend(
                (((instruction as u32 >> 12) & 1) << 9)
                    | (((instruction as u32 >> 6) & 1) << 4)
                    | (((instruction as u32 >> 5) & 1) << 6)
                    | (((instruction as u32 >> 3) & 0x3) << 7)
                    | (((instruction as u32 >> 2) & 1) << 5),
                10,
            );
            (immediate != 0).then(|| encode_i(immediate, 2, 0, 2, 0x13))
        }
        (0b01, 0b011) if rd != 0 && rd != 2 && ci_imm != 0 => {
            Some(((ci_imm as u32 & 0xfffff) << 12) | (rd << 7) | 0x37)
        }
        (0b01, 0b100) => {
            let operation = (instruction >> 10) & 0x3;
            let shift =
                (((instruction >> 12) as u32 & 1) << 5) | ((instruction as u32 >> 2) & 0x1f);
            match operation {
                0b00 => Some(encode_i(shift as i64, rs1_prime, 0b101, rs1_prime, 0x13)),
                0b01 => Some(encode_i(
                    (0x400 | shift) as i64,
                    rs1_prime,
                    0b101,
                    rs1_prime,
                    0x13,
                )),
                0b10 => Some(encode_i(ci_imm, rs1_prime, 0b111, rs1_prime, 0x13)),
                0b11 => {
                    let selector = (instruction >> 5) & 0x3;
                    let word_operation = instruction & 0x1000 != 0;
                    let (funct3, funct7, opcode) = match (word_operation, selector) {
                        (false, 0b00) => (0b000, 0b0100000, 0x33),
                        (false, 0b01) => (0b100, 0, 0x33),
                        (false, 0b10) => (0b110, 0, 0x33),
                        (false, 0b11) => (0b111, 0, 0x33),
                        (true, 0b00) => (0b000, 0b0100000, 0x3b),
                        (true, 0b01) => (0b000, 0, 0x3b),
                        _ => return None,
                    };
                    Some(
                        (funct7 << 25)
                            | (rs2_prime << 20)
                            | (rs1_prime << 15)
                            | (funct3 << 12)
                            | (rs1_prime << 7)
                            | opcode,
                    )
                }
                _ => None,
            }
        }
        (0b01, 0b101) => {
            let immediate = sign_extend(
                (((instruction as u32 >> 12) & 1) << 11)
                    | (((instruction as u32 >> 11) & 1) << 4)
                    | (((instruction as u32 >> 9) & 0x3) << 8)
                    | (((instruction as u32 >> 8) & 1) << 10)
                    | (((instruction as u32 >> 7) & 1) << 6)
                    | (((instruction as u32 >> 6) & 1) << 7)
                    | (((instruction as u32 >> 3) & 0x7) << 1)
                    | (((instruction as u32 >> 2) & 1) << 5),
                12,
            );
            Some(encode_j(immediate, 0))
        }
        (0b01, 0b110 | 0b111) => {
            let immediate = sign_extend(
                (((instruction as u32 >> 12) & 1) << 8)
                    | (((instruction as u32 >> 10) & 0x3) << 3)
                    | (((instruction as u32 >> 5) & 0x3) << 6)
                    | (((instruction as u32 >> 3) & 0x3) << 1)
                    | (((instruction as u32 >> 2) & 1) << 5),
                9,
            );
            Some(encode_b(
                immediate,
                rs1_prime,
                0,
                if funct3 == 0b110 { 0 } else { 1 },
            ))
        }
        (0b10, 0b000) => {
            let shift =
                (((instruction >> 12) as u32 & 1) << 5) | ((instruction as u32 >> 2) & 0x1f);
            Some(encode_i(shift as i64, rd, 0b001, rd, 0x13))
        }
        (0b10, 0b010) if rd != 0 => {
            let offset = (((instruction as u32 >> 12) & 1) << 5)
                | (((instruction as u32 >> 4) & 0x7) << 2)
                | (((instruction as u32 >> 2) & 0x3) << 6);
            Some(encode_i(offset as i64, 2, 0b010, rd, 0x03))
        }
        (0b10, 0b011) if rd != 0 => {
            let offset = (((instruction as u32 >> 12) & 1) << 5)
                | (((instruction as u32 >> 5) & 0x3) << 3)
                | (((instruction as u32 >> 2) & 0x7) << 6);
            Some(encode_i(offset as i64, 2, 0b011, rd, 0x03))
        }
        (0b10, 0b100) => {
            let high_bit = instruction & 0x1000 != 0;
            if !high_bit && rs2 == 0 {
                (rd != 0).then(|| encode_i(0, rd, 0, 0, 0x67))
            } else if !high_bit {
                Some(if rd == 0 {
                    encode_i(0, 0, 0, 0, 0x13)
                } else {
                    (rs2 << 20) | (rd << 15) | (rd << 7) | 0x33
                })
            } else if rd == 0 && rs2 == 0 {
                Some(0x0010_0073)
            } else if rs2 == 0 {
                Some(encode_i(0, rd, 0, 1, 0x67))
            } else {
                Some(if rd == 0 {
                    encode_i(0, 0, 0, 0, 0x13)
                } else {
                    (rs2 << 20) | (rd << 15) | (rd << 7) | 0x33
                })
            }
        }
        (0b10, 0b110) => {
            let offset =
                (((instruction as u32 >> 9) & 0xf) << 2) | (((instruction as u32 >> 7) & 0x3) << 6);
            Some(encode_s(offset, 2, rs2, 0b010))
        }
        (0b10, 0b111) => {
            let offset = (((instruction as u32 >> 10) & 0x7) << 3)
                | (((instruction as u32 >> 7) & 0x7) << 6);
            Some(encode_s(offset, 2, rs2, 0b011))
        }
        _ => None,
    }
}

fn illegal_instruction(pc: u64, instruction: u32) -> CoreTrap {
    CoreTrap {
        pc,
        cause: CoreTrapCause::IllegalInstruction,
        value: instruction as u64,
    }
}

fn privilege_encoding(privilege: PrivilegeMode) -> u64 {
    match privilege {
        PrivilegeMode::User => 0,
        PrivilegeMode::Supervisor => 1,
        PrivilegeMode::Machine => 3,
    }
}

fn interrupt_bit(cause: CoreInterruptCause) -> u64 {
    1 << interrupt_code(cause)
}

fn interrupt_code(cause: CoreInterruptCause) -> u64 {
    match cause {
        CoreInterruptCause::SupervisorSoftware => 1,
        CoreInterruptCause::MachineSoftware => 3,
        CoreInterruptCause::SupervisorTimer => 5,
        CoreInterruptCause::MachineTimer => 7,
        CoreInterruptCause::SupervisorExternal => 9,
        CoreInterruptCause::MachineExternal => 11,
    }
}

fn trap_vector(vector: u64, cause: u64, interrupt: bool) -> u64 {
    let base = vector & !3;
    if interrupt && vector & 3 == 1 {
        base.wrapping_add(cause.wrapping_mul(4))
    } else {
        base
    }
}

fn normalize_tvec(value: u64) -> u64 {
    let mode = value & 0x3;
    (value & !0x3) | u64::from(mode <= 1) * mode
}

fn exception_code(cause: CoreTrapCause) -> Option<u64> {
    Some(match cause {
        CoreTrapCause::InstructionAddressMisaligned => 0,
        CoreTrapCause::InstructionAccessFault => 1,
        CoreTrapCause::InstructionPageFault => 12,
        CoreTrapCause::IllegalInstruction => 2,
        CoreTrapCause::Breakpoint => 3,
        CoreTrapCause::LoadAddressMisaligned => 4,
        CoreTrapCause::LoadAccessFault => 5,
        CoreTrapCause::LoadPageFault => 13,
        CoreTrapCause::StoreAddressMisaligned => 6,
        CoreTrapCause::StoreAccessFault => 7,
        CoreTrapCause::StorePageFault => 15,
        CoreTrapCause::EnvironmentCallFromUser => 8,
        CoreTrapCause::EnvironmentCallFromSupervisor => 9,
        CoreTrapCause::EnvironmentCallFromMachine => 11,
    })
}

fn sign_extend(value: u32, bits: u32) -> i64 {
    ((value << (32 - bits)) as i32 >> (32 - bits)) as i64
}

fn multiply_high_signed_unsigned(left: u64, right: u64) -> u64 {
    let high = ((left as u128 * right as u128) >> 64) as u64;
    if left >> 63 == 0 {
        high
    } else {
        high.wrapping_sub(right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xv6_profile_reports_initial_board_and_extensions() {
        assert_eq!(XV6_PROFILE.hart_count, 1);
        assert_eq!(XV6_PROFILE.ram, AddressRange::new(0x8000_0000, 0x8800_0000));
        assert_eq!(
            XV6_PROFILE.ram.end - XV6_PROFILE.ram.start,
            128 * 1024 * 1024
        );
        assert_eq!(XV6_PROFILE.physical_address_bits, 56);
        assert_eq!(XV6_PROFILE.timebase_frequency, 10_000_000);
        for extension in [
            Extension::I,
            Extension::M,
            Extension::A,
            Extension::C,
            Extension::Zicsr,
            Extension::Zifencei,
        ] {
            assert!(XV6_PROFILE.supported_extensions.contains(&extension));
        }
        assert_eq!(
            XV6_PROFILE.unsupported_extensions,
            &[
                Extension::F,
                Extension::D,
                Extension::Hypervisor,
                Extension::Vector
            ]
        );
        assert_eq!(
            XV6_PROFILE.required_privileged_features,
            &[
                PrivilegedFeature::Sv39,
                PrivilegedFeature::Pmp,
                PrivilegedFeature::Sstc,
                PrivilegedFeature::Adue,
            ]
        );
    }

    fn bus_with_program(instructions: &[u32]) -> Kiuas16Bus {
        bus_with_program_and_ram_size(instructions, 16)
    }

    fn bus_with_program_and_ram_size(instructions: &[u32], ram_bytes: usize) -> Kiuas16Bus {
        let mut rom = vec![0; BOOT_ROM_BYTES];
        for (index, instruction) in instructions.iter().enumerate() {
            let start = index * 4;
            rom[start..start + 4].copy_from_slice(&instruction.to_le_bytes());
        }
        Kiuas16Bus::new_with_ram_size(rom, ram_bytes).unwrap()
    }

    fn bus_with_compressed_program(instructions: &[u16]) -> Kiuas16Bus {
        let mut rom = vec![0; BOOT_ROM_BYTES];
        for (index, instruction) in instructions.iter().enumerate() {
            let start = index * 2;
            rom[start..start + 2].copy_from_slice(&instruction.to_le_bytes());
        }
        Kiuas16Bus::new_with_ram_size(rom, 32).unwrap()
    }

    fn encode_r(funct7: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
    }

    fn encode_atomic(funct5: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32) -> u32 {
        (funct5 << 27) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x2f
    }

    fn encode_csr(csr: u32, rs1: u32, funct3: u32, rd: u32) -> u32 {
        (csr << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x73
    }

    fn allow_full_physical_range(core: &mut Rv64Core) {
        core.pmpaddr0 = (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1;
        core.pmpcfg0 = 0x1f;
    }

    fn install_sv39_mapping(
        bus: &mut Kiuas16Bus,
        root: u64,
        virtual_address: u64,
        physical_address: u64,
        flags: u64,
    ) {
        let vpn = [
            (virtual_address >> 12) & 0x1ff,
            (virtual_address >> 21) & 0x1ff,
            (virtual_address >> 30) & 0x1ff,
        ];
        let level1 = root + 0x1000;
        let level0 = root + 0x2000;
        for (address, ppn) in [
            (root + vpn[2] * 8, level1 >> 12),
            (level1 + vpn[1] * 8, level0 >> 12),
            (level0 + vpn[0] * 8, physical_address >> 12),
        ] {
            let pte = (ppn << 10)
                | if address == level0 + vpn[0] * 8 {
                    flags
                } else {
                    1
                };
            bus.write(address, &pte.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn rv64_core_executes_integer_word_and_memory_operations() {
        let mut bus = bus_with_program(&[
            0x7fff_f317,
            0x0070_0093,
            0xffe0_8113,
            0x0020_81b3,
            0xfff0_021b,
            0x02a0_0393,
            0x0073_2023,
            0x0003_2403,
        ]);
        let mut core = Rv64Core::cold_reset();
        for _ in 0..8 {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(0), Some(0));
        assert_eq!(core.register(1), Some(7));
        assert_eq!(core.register(2), Some(5));
        assert_eq!(core.register(3), Some(12));
        assert_eq!(core.register(4), Some(u64::MAX));
        assert_eq!(core.register(8), Some(42));
        let mut stored = [0; 4];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 42);
    }

    #[test]
    fn rv64_core_branches_and_traps_on_unsupported_instructions() {
        let mut bus = bus_with_program(&[0x0010_8493, 0x0010_8463, 0x0630_0493, 0x0010_8493]);
        let mut core = Rv64Core::cold_reset();
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 12);
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(9), Some(1));

        let invalid_atomic = encode_atomic(0b11111, 0, 0, 2, 3);
        let mut unsupported = bus_with_program(&[invalid_atomic]);
        core.pc = BOOT_ROM.start;
        let trap = core.step(&mut unsupported).unwrap_err();
        assert_eq!(
            trap,
            CoreTrap {
                pc: BOOT_ROM.start,
                cause: CoreTrapCause::IllegalInstruction,
                value: invalid_atomic as u64,
            }
        );

        core.pc = BOOT_ROM.start;
        let mut compressed = bus_with_program(&[0x0000_0000]);
        let trap = core.step(&mut compressed).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
        assert_eq!(trap.value, 0);
    }

    #[test]
    fn rv64c_executes_compressed_integer_and_memory_instructions() {
        let instructions = [
            0x4415, 0x040d, 0x44a1, 0x94a2, 0x0486, 0xc144, 0x414c, 0x367d,
        ];
        let mut bus = bus_with_compressed_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[10] = RAM.start;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.pc, BOOT_ROM.start + instructions.len() as u64 * 2);
        assert_eq!(core.register(8), Some(8));
        assert_eq!(core.register(9), Some(32));
        assert_eq!(core.register(11), Some(32));
        assert_eq!(core.register(12), Some(u64::MAX));
        let mut stored = [0; 4];
        bus.read(RAM.start + 4, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 32);
    }

    #[test]
    fn rv64c_branches_jumps_and_mixed_instruction_lengths_update_pc() {
        let compressed = [0xc011u16, 0x0405, 0x4485, 0xa011, 0x0485];
        let word = 0x0010_0513u32;
        let mut rom = vec![0; BOOT_ROM_BYTES];
        for (index, instruction) in compressed.iter().enumerate() {
            let start = index * 2;
            rom[start..start + 2].copy_from_slice(&instruction.to_le_bytes());
        }
        rom[10..14].copy_from_slice(&word.to_le_bytes());
        let mut bus = Kiuas16Bus::new_with_ram_size(rom, 16).unwrap();
        let mut core = Rv64Core::cold_reset();
        for _ in 0..3 {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.pc, BOOT_ROM.start + 10);
        core.step(&mut bus).unwrap();

        assert_eq!(core.register(8), Some(0));
        assert_eq!(core.register(9), Some(1));
        assert_eq!(core.register(10), Some(1));
        assert_eq!(core.pc, BOOT_ROM.start + 14);
    }

    #[test]
    fn rv64c_executes_stack_pointer_and_doubleword_memory_forms() {
        let instructions = [0x0800, 0xe500, 0x6504, 0xe426, 0x65a2, 0xc226, 0x4612];
        let mut bus = bus_with_compressed_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[2] = RAM.start;
        core.integer_registers[10] = RAM.start;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(8), Some(RAM.start + 16));
        assert_eq!(core.register(9), Some(RAM.start + 16));
        assert_eq!(core.register(11), Some(RAM.start + 16));
        assert_eq!(core.register(12), Some(0xffff_ffff_8000_0010));
        let mut stored = [0; 8];
        bus.read(RAM.start + 8, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), RAM.start + 16);
        let mut stored_word = [0; 4];
        bus.read(RAM.start + 4, &mut stored_word).unwrap();
        assert_eq!(u32::from_le_bytes(stored_word), 0x8000_0010);
    }

    #[test]
    fn rv64c_executes_shift_alu_and_nonzero_branch_forms() {
        let instructions = [0x443d, 0x881d, 0x8005, 0x8405, 0x4489, 0x9c05, 0x9c25];
        let mut bus = bus_with_compressed_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.register(8), Some(1));

        let instructions = [0x4405, 0xe011, 0x0405, 0x4485];
        let mut bus = bus_with_compressed_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        for _ in 0..2 {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.pc, BOOT_ROM.start + 6);
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(8), Some(1));
        assert_eq!(core.register(9), Some(1));
    }

    #[test]
    fn rv64c_executes_stack_adjustment_lui_and_indirect_jumps() {
        let instructions = [0x717d, 0x6405, 0x74fd];
        let mut bus = bus_with_compressed_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[2] = RAM.start + 16;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.register(2), Some(RAM.start));
        assert_eq!(core.register(8), Some(0x1000));
        assert_eq!(core.register(9), Some(0xffff_ffff_ffff_f000));

        let mut bus = bus_with_compressed_program(&[0x8282, 0x0001, 0x4405]);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[5] = BOOT_ROM.start + 4;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 4);
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(8), Some(1));

        let mut bus = bus_with_compressed_program(&[0x9282, 0x0001]);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[5] = BOOT_ROM.start + 4;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 4);
        assert_eq!(core.register(1), Some(BOOT_ROM.start + 2));
    }

    #[test]
    fn rv64c_handles_legal_hints_and_reports_reserved_encodings() {
        let mut bus = bus_with_compressed_program(&[0x0001, 0x0005, 0x8006, 0x9006, 0x2000]);
        let mut core = Rv64Core::cold_reset();
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 8);

        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(
            trap,
            CoreTrap {
                pc: BOOT_ROM.start + 8,
                cause: CoreTrapCause::IllegalInstruction,
                value: 0x2000,
            }
        );
    }

    #[test]
    fn rv64c_ebreak_and_fetch_at_rom_end_have_architectural_results() {
        let mut bus = bus_with_compressed_program(&[0x9002]);
        let mut core = Rv64Core::cold_reset();
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(
            trap,
            CoreTrap {
                pc: BOOT_ROM.start,
                cause: CoreTrapCause::Breakpoint,
                value: 0,
            }
        );

        let mut rom = vec![0; BOOT_ROM_BYTES];
        let last_instruction = (BOOT_ROM_BYTES - 2) as u64;
        rom[BOOT_ROM_BYTES - 2..].copy_from_slice(&0x0001u16.to_le_bytes());
        let mut bus = Kiuas16Bus::new_with_ram_size(rom, 16).unwrap();
        core.pc = BOOT_ROM.start + last_instruction;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.end);
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::InstructionAccessFault);
    }

    #[test]
    fn rv64_core_executes_machine_and_supervisor_csr_operations() {
        let instructions = [
            encode_csr(0x305, 2, 1, 1),
            encode_csr(0x305, 0, 2, 3),
            encode_csr(0x100, 3, 5, 5),
            encode_csr(0x100, 0, 2, 6),
            encode_csr(0x301, 0, 2, 7),
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[2] = BOOT_ROM.start + 0x101;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(1), Some(0));
        assert_eq!(core.register(3), Some(BOOT_ROM.start + 0x101));
        assert_eq!(core.register(5), Some(0));
        assert_eq!(core.register(6), Some(MSTATUS_SIE));
        assert_eq!(
            core.register(7),
            Some((2u64 << 62) | (1 << 0) | (1 << 2) | (1 << 8) | (1 << 12))
        );
        assert_eq!(core.mtvec, BOOT_ROM.start + 0x101);
    }

    #[test]
    fn rv64_core_rejects_privilege_violations_and_read_only_csr_writes() {
        let instruction = encode_csr(0x300, 1, 1, 2);
        let mut bus = bus_with_program(&[instruction]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
        assert_eq!(trap.value, instruction as u64);

        let instruction = encode_csr(0x301, 1, 1, 2);
        let mut bus = bus_with_program(&[instruction]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
        assert_eq!(trap.value, instruction as u64);
    }

    #[test]
    fn rv64_core_counts_retired_instructions_and_gates_supervisor_counters() {
        let instructions = [
            encode_csr(0xc00, 0, 2, 1),
            encode_csr(0xc00, 0, 2, 2),
            encode_csr(0xc02, 0, 2, 3),
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.register(1), Some(1));
        assert_eq!(core.register(2), Some(2));
        assert_eq!(core.register(3), Some(2));
        assert_eq!(core.mcycle, 3);
        assert_eq!(core.minstret, 3);

        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.pc = BOOT_ROM.start;
        let mut bus = bus_with_program(&[encode_csr(0xc02, 0, 2, 4)]);
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);

        core.mcounteren = 1 << 2;
        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(4), Some(3));
    }

    #[test]
    fn rv64_core_applies_csr_warl_masks_and_user_counter_gates() {
        let instructions = [
            encode_csr(0x300, 1, 1, 2),
            encode_csr(0x302, 3, 1, 4),
            encode_csr(0x305, 5, 1, 6),
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = 2 << 11;
        core.integer_registers[3] = u64::MAX;
        core.integer_registers[5] = BOOT_ROM.start + 0x203;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }
        assert_eq!(core.mstatus & MSTATUS_MPP_MASK, 0);
        assert_eq!(core.medeleg, MEDELEG_MASK);
        assert_eq!(core.mtvec, BOOT_ROM.start + 0x200);

        let mut bus = bus_with_program(&[encode_csr(0xc00, 0, 2, 7)]);
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.pc = BOOT_ROM.start;
        core.mcounteren = 1;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);

        core.scounteren = 1;
        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(7), Some(core.mcycle));
    }

    #[test]
    fn rv64_pmp_csr_programming_allows_contract_tor_range_for_supervisor() {
        let instructions = [
            encode_csr(0x3b0, 1, 1, 0),
            encode_csr(0x3a0, 2, 1, 0),
            0x0002_a303,
            0x0062_a023,
        ];
        let mut bus = bus_with_program(&instructions);
        bus.write(RAM.start, &0x1234_5678u32.to_le_bytes()).unwrap();
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1;
        core.integer_registers[2] = 0x0f;
        core.integer_registers[5] = RAM.start;

        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        assert_eq!(core.pmpaddr0, (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1);
        assert_eq!(core.pmpcfg0, 0x0f);

        core.privilege = PrivilegeMode::Supervisor;
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        assert_eq!(core.register(6), Some(0x1234_5678));
        let mut stored = [0; 4];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 0x1234_5678);
    }

    #[test]
    fn rv64_pmp_enforces_permissions_and_rejects_partial_matches() {
        let instructions = [
            encode_csr(0x3b0, 1, 1, 0),
            encode_csr(0x3a0, 2, 1, 0),
            0x0002_a303,
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1;
        core.integer_registers[2] = 0x0c;
        core.integer_registers[5] = RAM.start;
        core.step(&mut bus).unwrap();
        core.step(&mut bus).unwrap();
        core.privilege = PrivilegeMode::Supervisor;
        core.pc = BOOT_ROM.start + 8;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::LoadAccessFault);
        assert_eq!(trap.value, RAM.start);

        core.pmpcfg0 = 0x1f;
        core.pmpaddr0 = 0x2000_0000 >> 2;
        core.pc = BOOT_ROM.start;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::InstructionAccessFault);
        assert_eq!(trap.value, BOOT_ROM.start);

        core.pmpcfg0 = 0x13;
        core.pmpaddr0 = RAM.start >> 2;
        assert!(core.pmp_allows(RAM.start, 4, PhysicalAccess::Read));
        assert!(!core.pmp_allows(RAM.start, 8, PhysicalAccess::Read));
    }

    #[test]
    fn rv64_pmp_locks_entries_and_preserves_locked_configuration() {
        let instructions = [
            encode_csr(0x3b0, 1, 1, 0),
            encode_csr(0x3a0, 2, 1, 0),
            encode_csr(0x3b0, 3, 1, 0),
            encode_csr(0x3a0, 4, 1, 0),
            encode_csr(0x3b0, 0, 2, 5),
            encode_csr(0x3a0, 0, 2, 6),
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1;
        core.integer_registers[2] = 0x8f;
        core.integer_registers[3] = 1;
        core.integer_registers[4] = 0;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.pmpaddr0, (1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1);
        assert_eq!(core.pmpcfg0, 0x8f);
        assert_eq!(
            core.register(5),
            Some((1 << (PHYSICAL_ADDRESS_BITS - 2)) - 1)
        );
        assert_eq!(core.register(6), Some(0x8f));
    }

    #[test]
    fn rv64_sv39_executes_mapped_code_and_updates_accessed_dirty_bits() {
        let mut bus = bus_with_program_and_ram_size(&[], 0x5000);
        let root = RAM.start;
        let code = RAM.start + 0x3000;
        let va = 0x4000;
        let instructions = [0x1002_b303u32, 0x1062_b423];
        bus.write(code, &instructions[0].to_le_bytes()).unwrap();
        bus.write(code + 4, &instructions[1].to_le_bytes()).unwrap();
        bus.write(code + 0x100, &0x1234_5678_9abc_def0u64.to_le_bytes())
            .unwrap();
        install_sv39_mapping(&mut bus, root, va, code, 0b1_1111);

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.pc = va;
        core.integer_registers[5] = va;
        core.satp = (8 << 60) | (root >> 12);
        core.menvcfg = 1 << 61;

        core.step(&mut bus).unwrap();
        assert_eq!(core.register(6), Some(0x1234_5678_9abc_def0));
        core.step(&mut bus).unwrap();
        let pte_address = root + 0x2000 + ((va >> 12) & 0x1ff) * 8;
        let mut pte = [0; 8];
        bus.read(pte_address, &mut pte).unwrap();
        assert_eq!(u64::from_le_bytes(pte) & (1 << 6), 1 << 6);
        assert_eq!(u64::from_le_bytes(pte) & (1 << 7), 1 << 7);
        let mut stored = [0; 8];
        bus.read(code + 0x108, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), 0x1234_5678_9abc_def0);
    }

    #[test]
    fn rv64_sv39_reports_invalid_and_noncanonical_addresses_as_page_faults() {
        let mut bus = bus_with_program_and_ram_size(&[], 0x4000);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.satp = (8 << 60) | (RAM.start >> 12);
        core.pc = 0x4000;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::InstructionPageFault);
        assert_eq!(trap.value, 0x4000);

        let noncanonical = 1 << 39;
        assert_eq!(
            core.translate_address(&mut bus, core.pc, noncanonical, PhysicalAccess::Read)
                .unwrap_err()
                .cause,
            CoreTrapCause::LoadPageFault
        );
    }

    #[test]
    fn rv64_sv39_enforces_store_permissions_and_supervisor_user_access_rules() {
        let mut bus = bus_with_program_and_ram_size(&[], 0x5000);
        let root = RAM.start;
        let va = 0x4000;
        let pa = RAM.start + 0x3000;
        install_sv39_mapping(&mut bus, root, va, pa, 0b1_1011);

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.satp = (8 << 60) | (root >> 12);
        core.menvcfg = 1 << 61;
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap_err()
                .cause,
            CoreTrapCause::LoadPageFault
        );
        core.mstatus |= MSTATUS_SUM;
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap(),
            pa
        );
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Write)
                .unwrap_err()
                .cause,
            CoreTrapCause::StorePageFault
        );
        core.privilege = PrivilegeMode::Machine;
        core.mstatus = MSTATUS_MPRV | MSTATUS_SUM | (1 << 11);
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap(),
            pa
        );
        core.mstatus = 0;
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap(),
            va
        );
    }

    #[test]
    fn rv64_sv39_supports_aligned_superpages_and_rejects_misaligned_ones() {
        let mut bus = bus_with_program_and_ram_size(&[], 0x5000);
        let root = RAM.start;
        let level1 = root + 0x1000;
        let va = 0x4000;
        let root_pte = ((level1 >> 12) << 10) | 1;
        let superpage_pte = ((RAM.start >> 12) << 10) | 0b1100_1011;
        bus.write(root, &root_pte.to_le_bytes()).unwrap();
        bus.write(level1, &superpage_pte.to_le_bytes()).unwrap();

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.satp = (8 << 60) | (root >> 12);
        core.menvcfg = 1 << 61;
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap(),
            RAM.start + va
        );

        let misaligned_superpage_pte = superpage_pte | (1 << 10);
        bus.write(level1, &misaligned_superpage_pte.to_le_bytes())
            .unwrap();
        assert_eq!(
            core.translate_address(&mut bus, 0, va, PhysicalAccess::Read)
                .unwrap_err()
                .cause,
            CoreTrapCause::LoadPageFault
        );
    }

    #[test]
    fn rv64_sv39_fetches_across_pages_and_faults_on_an_unmapped_second_half() {
        let mut bus = bus_with_program_and_ram_size(&[], 0x6000);
        let root = RAM.start;
        let first_page = RAM.start + 0x3000;
        let second_page = RAM.start + 0x4000;
        let first_va = 0x4000;
        let second_va = 0x5000;
        let crossing_pc = second_va - 2;
        install_sv39_mapping(&mut bus, root, first_va, first_page, 0b1_1001);
        install_sv39_mapping(&mut bus, root, second_va, second_page, 0b1_1001);
        bus.write(first_page + 0xffe, &0x0013u16.to_le_bytes())
            .unwrap();
        bus.write(second_page, &0u16.to_le_bytes()).unwrap();

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.pc = crossing_pc;
        core.satp = (8 << 60) | (root >> 12);
        core.menvcfg = 1 << 61;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, crossing_pc + 4);

        let second_leaf = root + 0x2000 + ((second_va >> 12) & 0x1ff) * 8;
        bus.write(second_leaf, &0u64.to_le_bytes()).unwrap();
        core.pc = crossing_pc;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::InstructionPageFault);
        assert_eq!(trap.value, second_va);
    }

    #[test]
    fn rv64_core_delivers_machine_traps_and_executes_mret() {
        let mut bus = bus_with_program(&[0x3020_0073]);
        let mut core = Rv64Core::cold_reset();
        core.pc = RAM.start + 0x124;
        core.mtvec = BOOT_ROM.start + 0x200;
        core.mstatus = MSTATUS_MIE;
        let trap = CoreTrap {
            pc: core.pc,
            cause: CoreTrapCause::LoadAccessFault,
            value: 0xffff_8000_1234_5678,
        };
        core.deliver_trap(trap).unwrap();

        assert_eq!(core.privilege(), PrivilegeMode::Machine);
        assert_eq!(core.pc, BOOT_ROM.start + 0x200);
        assert_eq!(core.mepc, RAM.start + 0x124);
        assert_eq!(core.mcause, 5);
        assert_eq!(core.mtval, 0xffff_8000_1234_5678);
        assert_eq!(core.mstatus & MSTATUS_MIE, 0);
        assert_ne!(core.mstatus & MSTATUS_MPIE, 0);
        assert_eq!((core.mstatus & MSTATUS_MPP_MASK) >> 11, 3);

        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.privilege(), PrivilegeMode::Machine);
        assert_eq!(core.pc, RAM.start + 0x124);
        assert_ne!(core.mstatus & MSTATUS_MIE, 0);
        assert_ne!(core.mstatus & MSTATUS_MPIE, 0);
        assert_eq!(core.mstatus & MSTATUS_MPP_MASK, 0);

        core.mstatus = (core.mstatus & !MSTATUS_MPP_MASK) | (1 << 11) | MSTATUS_MPRV;
        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(core.mstatus & MSTATUS_MPRV, 0);
    }

    #[test]
    fn rv64_core_delegates_supervisor_traps_and_executes_sret_to_user() {
        let mut bus = bus_with_program(&[0x1020_0073]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.pc = RAM.start + 0x20;
        core.medeleg = 1 << 2;
        core.stvec = BOOT_ROM.start + 0x300;
        core.mstatus = MSTATUS_SIE;
        core.deliver_trap(CoreTrap {
            pc: core.pc,
            cause: CoreTrapCause::IllegalInstruction,
            value: 0xdead_beef,
        })
        .unwrap();

        assert_eq!(core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(core.pc, BOOT_ROM.start + 0x300);
        assert_eq!(core.sepc, RAM.start + 0x20);
        assert_eq!(core.scause, 2);
        assert_eq!(core.stval, 0xdead_beef);
        assert_eq!(core.mstatus & MSTATUS_SIE, 0);
        assert_ne!(core.mstatus & MSTATUS_SPIE, 0);
        assert_eq!(core.mstatus & MSTATUS_SPP, 0);

        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.privilege(), PrivilegeMode::User);
        assert_eq!(core.pc, RAM.start + 0x20);
        assert_ne!(core.mstatus & MSTATUS_SIE, 0);
        assert_eq!(core.mstatus & MSTATUS_SPP, 0);
    }

    #[test]
    fn rv64_step_with_trap_delivery_enters_handler_and_does_not_retire_faulting_instruction() {
        let mut rom = vec![0; BOOT_ROM_BYTES];
        rom[0..4].copy_from_slice(&0x0000_0073u32.to_le_bytes());
        rom[4..8].copy_from_slice(&0x0020_0593u32.to_le_bytes());
        rom[0x100..0x104].copy_from_slice(&0x0010_0513u32.to_le_bytes());
        rom[0x104..0x108].copy_from_slice(&0x1020_0073u32.to_le_bytes());
        let mut bus = Kiuas16Bus::new_with_ram_size(rom, 16).unwrap();
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.pc = BOOT_ROM.start;
        core.medeleg = 1 << 8;
        core.stvec = BOOT_ROM.start + 0x100;

        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::TrapDelivered(CoreTrap {
                pc: BOOT_ROM.start,
                cause: CoreTrapCause::EnvironmentCallFromUser,
                value: 0,
            })
        );
        assert_eq!(core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(core.pc, BOOT_ROM.start + 0x100);
        assert_eq!(core.sepc, BOOT_ROM.start);
        assert_eq!(core.scause, 8);
        assert_eq!(core.mcycle, 1);
        assert_eq!(core.minstret, 0);

        core.sepc = BOOT_ROM.start + 4;
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::Retired
        );
        assert_eq!(core.register(10), Some(1));
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::Retired
        );
        assert_eq!(core.privilege(), PrivilegeMode::User);
        assert_eq!(core.pc, BOOT_ROM.start + 4);
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::Retired
        );
        assert_eq!(core.register(11), Some(2));
        assert_eq!(core.mcycle, 4);
        assert_eq!(core.minstret, 3);
    }

    #[test]
    fn rv64_core_ecall_reports_current_privilege_and_diagnostic_wfi_retires() {
        let mut bus = bus_with_program(&[0x0000_0073, 0x1050_0073]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::EnvironmentCallFromSupervisor);
        assert_eq!(trap.pc, BOOT_ROM.start);
        assert_eq!(exception_code(trap.cause), Some(9));
        core.pc += 4;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 8);
    }

    #[test]
    fn rv64_step_delivers_delegated_interrupts_with_vectored_entry_and_no_retirement() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::User;
        core.mideleg = 1 << 5;
        core.mie = 1 << 5;
        core.mstatus = MSTATUS_SIE;
        core.stvec = BOOT_ROM.start + 0x100 | 1;
        core.set_interrupt_pending(CoreInterruptCause::SupervisorTimer, true);

        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorTimer)
        );
        assert_eq!(core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(core.pc, BOOT_ROM.start + 0x100 + 5 * 4);
        assert_eq!(core.sepc, BOOT_ROM.start);
        assert_eq!(core.scause, (1 << 63) | 5);
        assert_eq!(core.stval, 0);
        assert_eq!(core.mstatus & MSTATUS_SIE, 0);
        assert_ne!(core.mstatus & MSTATUS_SPIE, 0);
        assert_eq!(core.mstatus & MSTATUS_SPP, 0);
        assert_eq!(core.mcycle, 1);
        assert_eq!(core.minstret, 0);
    }

    #[test]
    fn rv64_step_respects_global_interrupt_mask_and_source_priority() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.mie = (1 << 3) | (1 << 7);
        core.mtvec = BOOT_ROM.start + 0x100;
        core.set_interrupt_pending(CoreInterruptCause::MachineTimer, true);
        core.set_interrupt_pending(CoreInterruptCause::MachineSoftware, true);

        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::Retired
        );
        assert_eq!(core.register(10), Some(1));
        assert_eq!(core.mcycle, 1);
        assert_eq!(core.minstret, 1);

        core.mstatus |= MSTATUS_MIE;
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::MachineSoftware)
        );
        assert_eq!(core.pc, BOOT_ROM.start + 0x100);
        assert_eq!(core.mepc, BOOT_ROM.start + 4);
        assert_eq!(core.mcause, (1 << 63) | 3);
        assert_eq!(core.mstatus & MSTATUS_MIE, 0);
        assert_ne!(core.mstatus & MSTATUS_MPIE, 0);
        assert_eq!(core.mcycle, 2);
        assert_eq!(core.minstret, 1);
    }

    #[test]
    fn kiuas16_machine_executes_guest_uart_io_and_advances_virtual_time() {
        let instructions = [
            (0x1_0000u32 << 12) | (1 << 7) | 0x37,
            (1 << 15) | (0b100 << 12) | (2 << 7) | 0x03,
            (2 << 20) | (1 << 15) | 0x23,
        ];
        let mut rom = vec![0; BOOT_ROM_BYTES];
        for (index, instruction) in instructions.iter().enumerate() {
            let start = index * 4;
            rom[start..start + 4].copy_from_slice(&instruction.to_le_bytes());
        }
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 16).unwrap();

        assert_eq!(machine.virtual_time(), 0);
        assert_eq!(machine.receive_uart_input(b"x").unwrap(), 1);
        for tick in 1..=3 {
            assert_eq!(machine.step().unwrap(), StepOutcome::Retired);
            assert_eq!(machine.virtual_time(), tick);
        }
        assert_eq!(machine.core().register(2), Some(u64::from(b'x')));
        assert_eq!(machine.take_uart_output(), b"x");
        assert!(machine.take_uart_output().is_empty());
    }

    #[test]
    fn kiuas16_machine_delivers_plic_interrupts_to_guest_trap_state() {
        let mut machine = Kiuas16Machine::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 16).unwrap();
        machine.core.privilege = PrivilegeMode::User;
        machine.core.mideleg = 1 << 9;
        machine.core.mie = 1 << 9;
        machine.core.mstatus = MSTATUS_SIE;
        machine.core.stvec = BOOT_ROM.start + 0x100 | 1;
        machine
            .bus
            .write(
                PLIC.start + u64::from(UART_PLIC_SOURCE) * 4,
                &1u32.to_le_bytes(),
            )
            .unwrap();
        machine
            .bus
            .write(
                PLIC.start + 0x2080,
                &(1u32 << UART_PLIC_SOURCE).to_le_bytes(),
            )
            .unwrap();
        machine
            .bus
            .write(PLIC.start + 0x20_1000, &0u32.to_le_bytes())
            .unwrap();
        machine
            .bus
            .set_plic_source_level(UART_PLIC_SOURCE, true)
            .unwrap();

        assert_eq!(
            machine.step().unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorExternal)
        );
        assert_eq!(machine.core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(machine.core.pc, BOOT_ROM.start + 0x100 + 9 * 4);
        assert_eq!(machine.core.scause, (1 << 63) | 9);
        assert_eq!(machine.core.mcycle, 1);
        assert_eq!(machine.core.minstret, 0);
        assert_eq!(machine.virtual_time(), 1);
    }

    #[test]
    fn kiuas16_machine_reset_preserves_memory_and_clears_volatile_state() {
        let mut rom = vec![0; BOOT_ROM_BYTES];
        rom[..4].copy_from_slice(&0x0070_0093u32.to_le_bytes());
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 32).unwrap();
        machine
            .bus
            .write(RAM.start, &[0x12, 0x34, 0x56, 0x78])
            .unwrap();
        machine.step().unwrap();
        assert_eq!(machine.core.register(1), Some(7));
        assert_eq!(machine.virtual_time(), 1);

        machine.bus.write(UART.start + 1, &[1]).unwrap();
        machine.receive_uart_input(b"r").unwrap();
        machine.bus.write(UART.start, b"t").unwrap();
        machine
            .bus
            .write(
                PLIC.start + u64::from(UART_PLIC_SOURCE) * 4,
                &1u32.to_le_bytes(),
            )
            .unwrap();
        machine
            .bus
            .write(
                PLIC.start + 0x2080,
                &(1u32 << UART_PLIC_SOURCE).to_le_bytes(),
            )
            .unwrap();
        assert!(machine.bus.plic_interrupt_pending(1));
        assert_eq!(machine.take_uart_output(), b"t");
        machine.bus.write(UART.start, b"q").unwrap();

        let profile = machine.profile();
        assert_eq!(profile.ram, AddressRange::new(RAM.start, RAM.start + 32));
        assert_eq!(profile.hart_count, 1);
        assert!(profile.supported_extensions.contains(&Extension::I));
        assert!(profile.unsupported_extensions.contains(&Extension::F));

        machine.reset();

        assert_eq!(machine.core.pc, RESET_PC);
        assert_eq!(machine.core.privilege(), PrivilegeMode::Machine);
        assert_eq!(machine.core.register(1), Some(0));
        assert_eq!(machine.virtual_time(), 0);
        assert!(!machine.bus.plic_interrupt_pending(1));
        assert!(machine.take_uart_output().is_empty());
        let mut line_status = [0];
        machine.bus.read(UART.start + 5, &mut line_status).unwrap();
        assert_eq!(line_status[0] & 0x03, 0);
        let mut ram = [0; 4];
        machine.bus.read(RAM.start, &mut ram).unwrap();
        assert_eq!(ram, [0x12, 0x34, 0x56, 0x78]);
        let mut instruction = [0; 4];
        machine.bus.fetch(BOOT_ROM.start, &mut instruction).unwrap();
        assert_eq!(instruction, 0x0070_0093u32.to_le_bytes());
    }

    #[test]
    fn hoyry_uart_implements_divisor_aliases_transmit_and_byte_access_rules() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let register = |offset| UART.start + offset;
        bus.write(register(3), &[0x80]).unwrap();
        bus.write(register(0), &[0x34]).unwrap();
        bus.write(register(1), &[0x12]).unwrap();
        let mut value = [0];
        bus.read(register(0), &mut value).unwrap();
        assert_eq!(value[0], 0x34);
        bus.read(register(1), &mut value).unwrap();
        assert_eq!(value[0], 0x12);

        bus.write(register(3), &[0x03]).unwrap();
        bus.write(register(1), &[0x02]).unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x02);
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x01);
        bus.write(register(0), b"X").unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x02);
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x01);
        assert_eq!(bus.take_uart_output(), b"X");
        bus.read(register(5), &mut value).unwrap();
        assert_eq!(value[0] & 0x60, 0x60);

        assert_eq!(
            bus.read(register(0), &mut [0; 2]),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(
            bus.write(register(0), b"xy"),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(
            bus.read(register(8), &mut value),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
    }

    #[test]
    fn hoyry_uart_bounds_receive_fifos_and_reports_status_and_interrupt_causes() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let register = |offset| UART.start + offset;
        let mut value = [0];
        assert_eq!(bus.receive_uart_input(b"ab").unwrap(), 1);
        bus.write(register(1), &[0x01]).unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0], 0x04);
        bus.read(register(5), &mut value).unwrap();
        assert_eq!(value[0] & 0x03, 0x03);
        bus.read(register(0), &mut value).unwrap();
        assert_eq!(value[0], b'a');
        bus.read(register(5), &mut value).unwrap();
        assert_eq!(value[0] & 0x03, 0);

        bus.write(register(2), &[0x41]).unwrap();
        assert_eq!(bus.receive_uart_input(&[1, 2, 3]).unwrap(), 3);
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0], 0xc1);
        assert_eq!(bus.receive_uart_input(&[4]).unwrap(), 1);
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0], 0xc4);

        bus.write(register(2), &[0x43]).unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0], 0xc1);
        assert_eq!(bus.receive_uart_input(&[0; 17]).unwrap(), 16);
        bus.read(register(5), &mut value).unwrap();
        assert_ne!(value[0] & 0x02, 0);
        bus.write(register(2), &[0]).unwrap();
        assert_eq!(bus.receive_uart_input(b"z").unwrap(), 0);
        bus.read(register(5), &mut value).unwrap();
        assert_ne!(value[0] & 0x02, 0);
    }

    #[test]
    fn hoyry_uart_loopback_modem_interrupts_and_device_reset_work() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let register = |offset| UART.start + offset;
        let mut value = [0];
        bus.write(register(1), &[0x08]).unwrap();
        bus.write(register(4), &[0x17]).unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x00);
        bus.read(register(6), &mut value).unwrap();
        assert_eq!(value[0], 0x73);
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x01);

        bus.write(register(1), &[0x01]).unwrap();
        bus.write(register(0), b"L").unwrap();
        bus.read(register(2), &mut value).unwrap();
        assert_eq!(value[0] & 0x0f, 0x04);
        bus.read(register(0), &mut value).unwrap();
        assert_eq!(value[0], b'L');
        assert!(bus.take_uart_output().is_empty());

        let priority = PLIC.start + u64::from(UART_PLIC_SOURCE) * 4;
        let enable = PLIC.start + VihtaPlic::ENABLE_BASE;
        bus.write(priority, &1u32.to_le_bytes()).unwrap();
        bus.write(enable, &(1u32 << UART_PLIC_SOURCE).to_le_bytes())
            .unwrap();
        bus.receive_uart_input(b"r").unwrap();
        assert!(bus.plic_interrupt_pending(0));
        bus.reset_uart().unwrap();
        assert!(bus.plic_interrupt_pending(0));
        let claim = PLIC.start + VihtaPlic::CONTEXT_BASE + 4;
        let mut claimed = [0; 4];
        bus.read(claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), UART_PLIC_SOURCE);
        bus.write(claim, &UART_PLIC_SOURCE.to_le_bytes()).unwrap();
        assert!(!bus.plic_interrupt_pending(0));
        bus.read(register(1), &mut value).unwrap();
        assert_eq!(value[0], 0);
        bus.read(register(5), &mut value).unwrap();
        assert_eq!(value[0] & 0x03, 0);
    }

    #[test]
    fn hoyry_uart_rx_interrupt_reaches_supervisor_through_vihta() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let plic_priority = PLIC.start + u64::from(UART_PLIC_SOURCE) * 4;
        let plic_enable = PLIC.start + VihtaPlic::ENABLE_BASE + VihtaPlic::ENABLE_STRIDE;
        let plic_claim = PLIC.start + VihtaPlic::CONTEXT_BASE + VihtaPlic::CONTEXT_STRIDE + 4;
        bus.write(plic_priority, &1u32.to_le_bytes()).unwrap();
        bus.write(plic_enable, &(1u32 << UART_PLIC_SOURCE).to_le_bytes())
            .unwrap();
        bus.write(UART.start + 1, &[0x01]).unwrap();
        assert_eq!(bus.receive_uart_input(b"x").unwrap(), 1);
        assert!(bus.plic_interrupt_pending(1));

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.mideleg = 1 << 9;
        core.mie = 1 << 9;
        core.mstatus = MSTATUS_SIE;
        core.stvec = BOOT_ROM.start + 0x100;
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorExternal)
        );

        let mut claimed = [0; 4];
        bus.read(plic_claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), UART_PLIC_SOURCE);
        let mut received = [0];
        bus.read(UART.start, &mut received).unwrap();
        assert_eq!(received[0], b'x');
        bus.write(plic_claim, &UART_PLIC_SOURCE.to_le_bytes())
            .unwrap();
        assert!(!bus.plic_interrupt_pending(1));
    }

    #[test]
    fn vihta_plic_prioritizes_enabled_sources_and_claims_pending_interrupts() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let priority = |source: u64| PLIC.start + source * 4;
        let enable = PLIC.start + VihtaPlic::ENABLE_BASE + VihtaPlic::ENABLE_STRIDE;
        let threshold = PLIC.start + VihtaPlic::CONTEXT_BASE + VihtaPlic::CONTEXT_STRIDE;
        let claim = threshold + 4;
        bus.write(priority(1), &3u32.to_le_bytes()).unwrap();
        bus.write(priority(2), &6u32.to_le_bytes()).unwrap();
        bus.write(enable, &((1u32 << 1) | (1u32 << 2)).to_le_bytes())
            .unwrap();
        bus.write(threshold, &2u32.to_le_bytes()).unwrap();
        bus.set_plic_source_level(1, true).unwrap();
        bus.set_plic_source_level(2, true).unwrap();
        assert!(bus.plic_interrupt_pending(1));
        bus.write(threshold, &6u32.to_le_bytes()).unwrap();
        assert!(!bus.plic_interrupt_pending(1));
        bus.write(threshold, &2u32.to_le_bytes()).unwrap();

        let mut claimed = [0; 4];
        bus.read(claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), 2);
        assert!(bus.plic_interrupt_pending(1));
        bus.set_plic_source_level(2, false).unwrap();
        bus.write(claim, &2u32.to_le_bytes()).unwrap();
        assert!(bus.plic_interrupt_pending(1));
        bus.read(claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), 1);
        bus.set_plic_source_level(1, false).unwrap();
        bus.write(claim, &1u32.to_le_bytes()).unwrap();
        assert!(!bus.plic_interrupt_pending(1));
    }

    #[test]
    fn vihta_plic_reasserts_level_sources_after_completion_and_rejects_bad_accesses() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let priority = PLIC.start + 4 * 3;
        let enable = PLIC.start + VihtaPlic::ENABLE_BASE;
        let claim = PLIC.start + VihtaPlic::CONTEXT_BASE + 4;
        bus.write(priority, &1u32.to_le_bytes()).unwrap();
        bus.write(enable, &(1u32 << 3).to_le_bytes()).unwrap();
        bus.set_plic_source_level(3, true).unwrap();
        let mut claimed = [0; 4];
        bus.read(claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), 3);
        bus.set_plic_source_level(3, true).unwrap();
        assert!(!bus.plic_interrupt_pending(0));
        bus.write(claim, &3u32.to_le_bytes()).unwrap();
        assert!(bus.plic_interrupt_pending(0));
        bus.set_plic_source_level(3, false).unwrap();
        bus.read(claim, &mut claimed).unwrap();
        assert_eq!(u32::from_le_bytes(claimed), 3);
        bus.write(claim, &3u32.to_le_bytes()).unwrap();
        assert!(!bus.plic_interrupt_pending(0));

        assert_eq!(
            bus.read(PLIC.start, &mut [0; 2]),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(
            bus.read(PLIC.start + 1, &mut [0; 4]),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(
            bus.write(PLIC.start + VihtaPlic::PENDING_BASE, &1u32.to_le_bytes()),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
        assert_eq!(
            bus.set_plic_source_level(PLIC_SOURCE_COUNT + 1, true),
            Err(PhysicalAccessError::InvalidDeviceAccess)
        );
    }

    #[test]
    fn vihta_plic_drives_the_supervisor_external_interrupt_line() {
        let mut bus = bus_with_program(&[0x0010_0513]);
        let priority = PLIC.start + 4 * 5;
        let enable = PLIC.start + VihtaPlic::ENABLE_BASE + VihtaPlic::ENABLE_STRIDE;
        bus.write(priority, &1u32.to_le_bytes()).unwrap();
        bus.write(enable, &(1u32 << 5).to_le_bytes()).unwrap();
        bus.set_plic_source_level(5, true).unwrap();

        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.mideleg = 1 << 9;
        core.mie = 1 << 9;
        core.mstatus = MSTATUS_SIE;
        core.stvec = BOOT_ROM.start + 0x100;
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorExternal)
        );
        assert_eq!(core.scause, (1 << 63) | 9);
    }

    #[test]
    fn rv64_time_csr_permissions_and_sstc_compare_access_follow_controls() {
        let mut core = Rv64Core::cold_reset();
        assert_eq!(core.read_csr(0xc01), Ok(0));
        assert_eq!(core.read_csr(0x14d), Ok(u64::MAX));
        assert_eq!(core.write_csr(0x14d, 7), Ok(()));
        assert_eq!(core.write_csr(0xc01, 7), Err(()));

        core.privilege = PrivilegeMode::Supervisor;
        assert_eq!(core.read_csr(0xc01), Err(()));
        assert_eq!(core.read_csr(0x14d), Err(()));
        core.mcounteren = 1 << 1;
        assert_eq!(core.read_csr(0xc01), Ok(0));
        core.menvcfg |= 1 << 63;
        assert_eq!(core.read_csr(0x14d), Ok(7));
        assert_eq!(core.write_csr(0x14d, 9), Ok(()));

        core.privilege = PrivilegeMode::User;
        assert_eq!(core.read_csr(0xc01), Err(()));
        core.scounteren = 1 << 1;
        assert_eq!(core.read_csr(0xc01), Ok(0));
        assert_eq!(core.read_csr(0x14d), Err(()));
    }

    #[test]
    fn rv64_virtual_time_drives_stimer_and_wraps_monotonically() {
        let mut bus = bus_with_program(&[0x0010_0513; 4]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.stimecmp = 2;

        core.step(&mut bus).unwrap();
        assert_eq!(core.time, 1);
        assert_eq!(core.mip & (1 << 5), 0);
        core.step(&mut bus).unwrap();
        assert_eq!(core.time, 2);
        assert_ne!(core.mip & (1 << 5), 0);

        core.time = u64::MAX - 1;
        core.stimecmp = u64::MAX;
        core.step(&mut bus).unwrap();
        assert_eq!(core.time, u64::MAX);
        assert_ne!(core.mip & (1 << 5), 0);
        core.step(&mut bus).unwrap();
        assert_eq!(core.time, 0);
        assert_eq!(core.mip & (1 << 5), 0);
        assert_eq!(core.mcycle, 4);

        core.privilege = PrivilegeMode::Supervisor;
        core.mideleg = 1 << 5;
        core.mie = 1 << 5;
        core.mstatus = MSTATUS_SIE;
        core.stvec = BOOT_ROM.start + 0x100;
        core.stimecmp = 0;
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorTimer)
        );
        assert_eq!(core.scause, (1 << 63) | 5);
        assert_eq!(core.mcycle, 5);
        assert_eq!(core.minstret, 4);
    }

    #[test]
    fn rv64_wfi_waits_and_wakes_for_a_pending_interrupt() {
        let mut bus = bus_with_program(&[0x1050_0073, 0x0010_0513]);
        let mut core = Rv64Core::cold_reset();
        allow_full_physical_range(&mut core);
        core.privilege = PrivilegeMode::Supervisor;
        core.mideleg = 1 << 1;
        core.mie = 1 << 1;
        core.mstatus = MSTATUS_SIE;
        core.stvec = BOOT_ROM.start + 0x100;

        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::Retired
        );
        assert_eq!(core.pc, BOOT_ROM.start + 4);
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::WaitingForInterrupt
        );
        assert_eq!(core.mcycle, 2);
        assert_eq!(core.minstret, 1);

        core.set_interrupt_pending(CoreInterruptCause::SupervisorSoftware, true);
        assert_eq!(
            core.step_with_trap_delivery(&mut bus).unwrap(),
            StepOutcome::InterruptDelivered(CoreInterruptCause::SupervisorSoftware)
        );
        assert_eq!(core.privilege(), PrivilegeMode::Supervisor);
        assert_eq!(core.pc, BOOT_ROM.start + 0x100);
        assert_eq!(core.sepc, BOOT_ROM.start + 4);
        assert_eq!(core.scause, (1 << 63) | 1);
        assert_eq!(core.mcycle, 3);
        assert_eq!(core.minstret, 1);

        core.set_interrupt_pending(CoreInterruptCause::SupervisorSoftware, false);
        core.sepc = BOOT_ROM.start + 4;
        core.pc = BOOT_ROM.start;
        core.step(&mut bus).unwrap();
        assert_eq!(core.pc, BOOT_ROM.start + 4);
    }

    #[test]
    fn rv64_m_extension_handles_division_edges_and_word_sign_extension() {
        let instructions = [
            0x0070_0093,
            0x0030_0113,
            encode_r(1, 2, 1, 0, 3, 0x33),
            encode_r(1, 2, 1, 4, 4, 0x33),
            encode_r(1, 2, 1, 6, 5, 0x33),
            encode_r(1, 0, 1, 4, 6, 0x33),
            encode_r(1, 0, 1, 6, 7, 0x33),
            encode_r(1, 2, 1, 4, 8, 0x3b),
            encode_r(1, 2, 1, 6, 9, 0x3b),
            0x8000_00b7,
            0x0200_9093,
            0xfff0_0113,
            encode_r(1, 2, 1, 4, 10, 0x33),
            encode_r(1, 2, 1, 6, 11, 0x33),
            encode_r(1, 2, 1, 1, 12, 0x33),
            encode_r(1, 2, 1, 2, 13, 0x33),
            encode_r(1, 2, 1, 3, 14, 0x33),
            0x8000_07b7,
            0xfff0_0813,
            encode_r(1, 16, 15, 4, 17, 0x3b),
            encode_r(1, 16, 15, 6, 18, 0x3b),
        ];
        let mut bus = bus_with_program(&instructions);
        let mut core = Rv64Core::cold_reset();
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(3), Some(21));
        assert_eq!(core.register(4), Some(2));
        assert_eq!(core.register(5), Some(1));
        assert_eq!(core.register(6), Some(u64::MAX));
        assert_eq!(core.register(7), Some(7));
        assert_eq!(core.register(8), Some(2));
        assert_eq!(core.register(9), Some(1));
        assert_eq!(core.register(10), Some(1 << 63));
        assert_eq!(core.register(11), Some(0));
        assert_eq!(core.register(12), Some(0));
        assert_eq!(core.register(13), Some(1 << 63));
        assert_eq!(core.register(14), Some(0x7fff_ffff_ffff_ffff));
        assert_eq!(core.register(17), Some(0xffff_ffff_8000_0000));
        assert_eq!(core.register(18), Some(0));
    }

    #[test]
    fn rv64_a_extension_executes_word_amos_with_sign_extended_results() {
        let instructions = [
            0xffd0_0113,
            encode_atomic(0b00000, 2, 1, 2, 3),
            encode_atomic(0b00001, 2, 1, 2, 4),
            encode_atomic(0b00100, 2, 1, 2, 5),
            encode_atomic(0b01000, 2, 1, 2, 6),
            encode_atomic(0b01100, 2, 1, 2, 7),
            encode_atomic(0b10000, 2, 1, 2, 8),
            encode_atomic(0b10100, 2, 1, 2, 9),
            encode_atomic(0b11000, 2, 1, 2, 10),
            encode_atomic(0b11100, 2, 1, 2, 11),
        ];
        let mut bus = bus_with_program(&instructions);
        bus.write(RAM.start, &5i32.to_le_bytes()).unwrap();
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = RAM.start;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(3), Some(5));
        assert_eq!(core.register(4), Some(2));
        assert_eq!(core.register(5), Some(0xffff_ffff_ffff_fffd));
        assert_eq!(core.register(6), Some(0));
        assert_eq!(core.register(7), Some(0xffff_ffff_ffff_fffd));
        assert_eq!(core.register(8), Some(0xffff_ffff_ffff_fffd));
        assert_eq!(core.register(9), Some(0xffff_ffff_ffff_fffd));
        assert_eq!(core.register(10), Some(0xffff_ffff_ffff_fffd));
        assert_eq!(core.register(11), Some(0xffff_ffff_ffff_fffd));
        let mut stored = [0; 4];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(i32::from_le_bytes(stored), -3);
    }

    #[test]
    fn rv64_a_extension_distinguishes_signed_and_unsigned_min_max() {
        let instructions = [
            encode_atomic(0b10000, 5, 1, 2, 6),
            encode_atomic(0b10100, 5, 2, 2, 7),
            encode_atomic(0b11000, 5, 3, 2, 8),
            encode_atomic(0b11100, 5, 4, 2, 9),
        ];
        let mut bus = bus_with_program_and_ram_size(&instructions, 32);
        for offset in [0, 8, 16, 24] {
            bus.write(RAM.start + offset, &0x8000_0000u32.to_le_bytes())
                .unwrap();
        }
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = RAM.start;
        core.integer_registers[2] = RAM.start + 8;
        core.integer_registers[3] = RAM.start + 16;
        core.integer_registers[4] = RAM.start + 24;
        core.integer_registers[5] = 0x7fff_ffff;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(6), Some(0xffff_ffff_8000_0000));
        assert_eq!(core.register(7), Some(0xffff_ffff_8000_0000));
        assert_eq!(core.register(8), Some(0xffff_ffff_8000_0000));
        assert_eq!(core.register(9), Some(0xffff_ffff_8000_0000));
        let mut signed_min = [0; 4];
        let mut signed_max = [0; 4];
        let mut unsigned_min = [0; 4];
        let mut unsigned_max = [0; 4];
        bus.read(RAM.start, &mut signed_min).unwrap();
        bus.read(RAM.start + 8, &mut signed_max).unwrap();
        bus.read(RAM.start + 16, &mut unsigned_min).unwrap();
        bus.read(RAM.start + 24, &mut unsigned_max).unwrap();
        assert_eq!(u32::from_le_bytes(signed_min), 0x8000_0000);
        assert_eq!(u32::from_le_bytes(signed_max), 0x7fff_ffff);
        assert_eq!(u32::from_le_bytes(unsigned_min), 0x7fff_ffff);
        assert_eq!(u32::from_le_bytes(unsigned_max), 0x8000_0000);
    }

    #[test]
    fn rv64_a_extension_tracks_lr_sc_address_width_and_intervening_stores() {
        let instructions = [
            0x0070_0113,
            encode_atomic(0b00010, 0, 1, 2, 3),
            encode_atomic(0b00011, 2, 1, 2, 4),
            encode_atomic(0b00010, 0, 1, 2, 5),
            0x0020_a023,
            encode_atomic(0b00011, 2, 1, 2, 6),
            encode_atomic(0b00010, 0, 1, 2, 7),
            encode_atomic(0b00011, 2, 1, 3, 8),
        ];
        let mut bus = bus_with_program(&instructions);
        bus.write(RAM.start, &0x8000_0000u32.to_le_bytes()).unwrap();
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = RAM.start;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(3), Some(0xffff_ffff_8000_0000));
        assert_eq!(core.register(4), Some(0));
        assert_eq!(core.register(5), Some(7));
        assert_eq!(core.register(6), Some(1));
        assert_eq!(core.register(7), Some(7));
        assert_eq!(core.register(8), Some(1));
        let mut stored = [0; 4];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 7);
    }

    #[test]
    fn rv64_a_extension_executes_doubleword_lr_sc_and_amo() {
        let instructions = [
            0x0010_0113,
            encode_atomic(0b00010, 0, 1, 3, 3),
            encode_atomic(0b00011, 2, 1, 3, 4),
            encode_atomic(0b00000, 2, 1, 3, 5),
        ];
        let mut bus = bus_with_program(&instructions);
        bus.write(RAM.start, &0x8000_0000_0000_0000u64.to_le_bytes())
            .unwrap();
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = RAM.start;
        for _ in 0..instructions.len() {
            core.step(&mut bus).unwrap();
        }

        assert_eq!(core.register(3), Some(0x8000_0000_0000_0000));
        assert_eq!(core.register(4), Some(0));
        assert_eq!(core.register(5), Some(1));
        let mut stored = [0; 8];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), 2);
    }

    #[test]
    fn rv64_a_extension_external_bus_write_invalidates_lr_reservation() {
        let instructions = [
            encode_atomic(0b00010, 0, 1, 2, 3),
            encode_atomic(0b00011, 2, 1, 2, 4),
        ];
        let mut bus = bus_with_program(&instructions);
        bus.write(RAM.start, &5u32.to_le_bytes()).unwrap();
        let mut core = Rv64Core::cold_reset();
        core.integer_registers[1] = RAM.start;
        core.integer_registers[2] = 9;
        core.step(&mut bus).unwrap();
        bus.write(RAM.start + 8, &1u32.to_le_bytes()).unwrap();
        core.step(&mut bus).unwrap();

        assert_eq!(core.register(3), Some(5));
        assert_eq!(core.register(4), Some(1));
        let mut stored = [0; 4];
        bus.read(RAM.start, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 5);
    }

    #[test]
    fn rv64_core_reports_alignment_and_memory_access_faults() {
        let mut bus = bus_with_program(&[0x0010_2083]);
        let mut core = Rv64Core::cold_reset();
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::LoadAddressMisaligned);
        assert_eq!(trap.value, 1);

        core.pc = PHYSICAL_ADDRESS_LIMIT;
        let trap = core.step(&mut bus).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::InstructionAccessFault);
        assert_eq!(trap.value, PHYSICAL_ADDRESS_LIMIT);
    }

    #[test]
    fn physical_map_accepts_rom_and_device_apertures() {
        assert_eq!(
            check_physical_access(BOOT_ROM.start, 1, PhysicalAccess::Read),
            Ok(PhysicalRegion::BootRom)
        );
        assert_eq!(
            check_physical_access(BOOT_ROM.start, 1, PhysicalAccess::Execute),
            Ok(PhysicalRegion::BootRom)
        );
        assert_eq!(
            check_physical_access(PLIC.start, 1, PhysicalAccess::Read),
            Ok(PhysicalRegion::Plic)
        );
        assert_eq!(
            check_physical_access(UART.start, 1, PhysicalAccess::Read),
            Ok(PhysicalRegion::Uart)
        );
        assert_eq!(
            check_physical_access(ROOT_BLOCK.start, 1, PhysicalAccess::Write),
            Ok(PhysicalRegion::RootBlock)
        );
    }

    #[test]
    fn physical_map_accepts_ram_edges_and_full_width_addresses() {
        assert_eq!(
            check_physical_access(RAM.start, 1, PhysicalAccess::Read),
            Ok(PhysicalRegion::Ram)
        );
        assert_eq!(
            check_physical_access(RAM.end - 1, 1, PhysicalAccess::Write),
            Ok(PhysicalRegion::Ram)
        );
        assert_eq!(
            check_physical_access(0x1_0000_0000, 1, PhysicalAccess::Read),
            Err(PhysicalAccessError::Unmapped)
        );
        assert_eq!(
            check_physical_access(PHYSICAL_ADDRESS_LIMIT - 1, 1, PhysicalAccess::Read),
            Err(PhysicalAccessError::Unmapped)
        );
        assert_eq!(
            check_physical_access(PHYSICAL_ADDRESS_LIMIT, 1, PhysicalAccess::Read),
            Err(PhysicalAccessError::AddressOutOfRange)
        );
    }

    #[test]
    fn physical_access_rejects_holes_permission_violations_and_partial_crossings() {
        for address in [0, BOOT_ROM.end, ROOT_BLOCK.end, RAM.end] {
            assert_eq!(
                check_physical_access(address, 1, PhysicalAccess::Read),
                Err(PhysicalAccessError::Unmapped)
            );
        }
        assert_eq!(
            check_physical_access(RAM.end - 1, 2, PhysicalAccess::Write),
            Err(PhysicalAccessError::RegionBoundary)
        );
        assert_eq!(
            check_physical_access(BOOT_ROM.start, 1, PhysicalAccess::Write),
            Err(PhysicalAccessError::PermissionDenied)
        );
        assert_eq!(
            check_physical_access(BOOT_ROM.end - 1, 2, PhysicalAccess::Read),
            Err(PhysicalAccessError::RegionBoundary)
        );
        assert_eq!(
            check_physical_access(u64::MAX, 2, PhysicalAccess::Read),
            Err(PhysicalAccessError::AddressOverflow)
        );
        assert_eq!(
            check_physical_access(RAM.start, 0, PhysicalAccess::Read),
            Err(PhysicalAccessError::EmptyAccess)
        );
    }

    #[test]
    fn dma_is_limited_to_complete_ranges_inside_ram() {
        assert_eq!(check_dma_range(RAM.start, 1), Ok(()));
        assert_eq!(check_dma_range(RAM.end - 4, 4), Ok(()));
        assert_eq!(
            check_dma_range(RAM.end - 1, 2),
            Err(PhysicalAccessError::DmaOutsideRam)
        );
        assert_eq!(
            check_dma_range(UART.start, 1),
            Err(PhysicalAccessError::DmaOutsideRam)
        );
        assert_eq!(
            check_dma_range(u64::MAX, 2),
            Err(PhysicalAccessError::AddressOverflow)
        );
    }

    #[test]
    fn bus_reads_rom_and_fetches_without_allowing_rom_mutation() {
        let mut rom = vec![0; BOOT_ROM_BYTES];
        rom[0] = 0x13;
        let mut bus = Kiuas16Bus::new_with_ram_size(rom, 16).unwrap();
        let mut byte = [0];
        assert_eq!(bus.read(BOOT_ROM.start, &mut byte), Ok(()));
        assert_eq!(byte, [0x13]);
        assert_eq!(bus.fetch(BOOT_ROM.start, &mut byte), Ok(()));
        assert_eq!(
            bus.write(BOOT_ROM.start, &[0]),
            Err(PhysicalAccessError::PermissionDenied)
        );
        assert_eq!(
            bus.read(BOOT_ROM.start, &mut []),
            Err(PhysicalAccessError::EmptyAccess)
        );
    }

    #[test]
    fn bus_ram_operations_obey_configured_bounds_atomically() {
        let mut bus = Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 8).unwrap();
        assert_eq!(bus.write(RAM.start, &[1, 2, 3, 4]), Ok(()));
        let mut bytes = [0; 4];
        assert_eq!(bus.read(RAM.start, &mut bytes), Ok(()));
        assert_eq!(bytes, [1, 2, 3, 4]);
        assert_eq!(
            bus.write(RAM.start + 7, &[9, 8]),
            Err(PhysicalAccessError::Unmapped)
        );
        assert_eq!(
            bus.read(RAM.start + 8, &mut [0]),
            Err(PhysicalAccessError::Unmapped)
        );
        let mut unchanged = [0; 4];
        assert_eq!(bus.read(RAM.start, &mut unchanged), Ok(()));
        assert_eq!(unchanged, [1, 2, 3, 4]);
    }

    #[test]
    fn bus_exposes_uart_without_faking_unimplemented_mmio() {
        let mut bus = Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 8).unwrap();
        let mut line_status = [0];
        assert_eq!(bus.read(UART.start + 5, &mut line_status), Ok(()));
        assert_eq!(line_status[0] & 0x60, 0x60);
        assert_eq!(
            bus.read(ROOT_BLOCK.start, &mut [0]),
            Err(PhysicalAccessError::DeviceNotImplemented)
        );
    }

    #[test]
    fn bus_rejects_invalid_backing_configuration() {
        assert_eq!(
            Kiuas16Bus::new_with_ram_size(vec![], 8).err(),
            Some(PhysicalAccessError::InvalidConfiguration)
        );
        assert_eq!(
            Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 0).err(),
            Some(PhysicalAccessError::InvalidConfiguration)
        );
        assert_eq!(
            Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], XV6_RAM_BYTES as usize + 1)
                .err(),
            Some(PhysicalAccessError::InvalidConfiguration)
        );
    }

    #[test]
    fn cold_reset_matches_contract_values() {
        assert_eq!(COLD_RESET_STATE.pc, 0x1000);
        assert_eq!(COLD_RESET_STATE.privilege, PrivilegeMode::Machine);
        assert_eq!(COLD_RESET_STATE.hart_id, 0);
        assert_eq!(COLD_RESET_STATE.integer_registers, [0; 32]);
        assert_eq!(COLD_RESET_STATE.floating_registers, [0; 32]);
        assert_eq!(COLD_RESET_STATE.fcsr, 0);
        assert_eq!(COLD_RESET_STATE.mip, 0);
        assert_eq!(COLD_RESET_STATE.stimecmp, u64::MAX);
        assert_eq!(COLD_RESET_STATE.menvcfg, 0);
        assert_eq!(COLD_RESET_STATE.mcounteren, 0);
        assert_eq!(COLD_RESET_STATE.time, 0);
        assert_eq!(COLD_RESET_STATE.mcycle, 0);
        assert_eq!(COLD_RESET_STATE.minstret, 0);
        assert_eq!(COLD_RESET_STATE.mstatus, 0);
        assert_eq!(COLD_RESET_STATE.mie, 0);
        assert_eq!(COLD_RESET_STATE.medeleg, 0);
        assert_eq!(COLD_RESET_STATE.mideleg, 0);
        assert_eq!(COLD_RESET_STATE.mtvec, 0);
        assert_eq!(COLD_RESET_STATE.mepc, 0);
        assert_eq!(COLD_RESET_STATE.mcause, 0);
        assert_eq!(COLD_RESET_STATE.mtval, 0);
        assert_eq!(COLD_RESET_STATE.stvec, 0);
        assert_eq!(COLD_RESET_STATE.sepc, 0);
        assert_eq!(COLD_RESET_STATE.scause, 0);
        assert_eq!(COLD_RESET_STATE.stval, 0);
        assert_eq!(COLD_RESET_STATE.satp, 0);
        assert_eq!(COLD_RESET_STATE.pmpcfg0, 0);
        assert_eq!(COLD_RESET_STATE.pmpaddr0, 0);
    }
}
