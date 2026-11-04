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
        })
    }

    pub fn read(&self, address: u64, output: &mut [u8]) -> Result<(), PhysicalAccessError> {
        self.transfer(address, output, PhysicalAccess::Read)
    }

    pub fn fetch(&self, address: u64, output: &mut [u8]) -> Result<(), PhysicalAccessError> {
        self.transfer(address, output, PhysicalAccess::Execute)
    }

    pub fn write(&mut self, address: u64, data: &[u8]) -> Result<(), PhysicalAccessError> {
        let length =
            u64::try_from(data.len()).map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let region = self.check_access(address, length, PhysicalAccess::Write)?;
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
        &self,
        address: u64,
        output: &mut [u8],
        access: PhysicalAccess,
    ) -> Result<(), PhysicalAccessError> {
        let length =
            u64::try_from(output.len()).map_err(|_| PhysicalAccessError::AddressOutOfRange)?;
        let region = self.check_access(address, length, access)?;
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
    Extension::F,
    Extension::D,
    Extension::C,
    Extension::Zicsr,
    Extension::Zifencei,
];

pub const UNSUPPORTED_EXTENSIONS: &[Extension] = &[Extension::Hypervisor, Extension::Vector];

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
    IllegalInstruction,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAddressMisaligned,
    StoreAccessFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreTrap {
    pub pc: u64,
    pub cause: CoreTrapCause,
    pub value: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rv64Core {
    pub pc: u64,
    integer_registers: [u64; 32],
    reservation: Option<(u64, u8, u64)>,
}

impl Rv64Core {
    pub const fn cold_reset() -> Self {
        Self {
            pc: COLD_RESET_STATE.pc,
            integer_registers: COLD_RESET_STATE.integer_registers,
            reservation: None,
        }
    }

    pub fn register(&self, index: usize) -> Option<u64> {
        self.integer_registers.get(index).copied()
    }

    pub fn step(&mut self, bus: &mut Kiuas16Bus) -> Result<(), CoreTrap> {
        let pc = self.pc;
        if pc & 1 != 0 {
            return Err(CoreTrap {
                pc,
                cause: CoreTrapCause::InstructionAddressMisaligned,
                value: pc,
            });
        }

        let mut halfword = [0; 2];
        bus.fetch(pc, &mut halfword).map_err(|_| CoreTrap {
            pc,
            cause: CoreTrapCause::InstructionAccessFault,
            value: pc,
        })?;
        let halfword = u16::from_le_bytes(halfword);
        if halfword & 0b11 != 0b11 {
            return Err(CoreTrap {
                pc,
                cause: CoreTrapCause::IllegalInstruction,
                value: halfword as u64,
            });
        }

        let mut bytes = [0; 4];
        bus.fetch(pc, &mut bytes).map_err(|_| CoreTrap {
            pc,
            cause: CoreTrapCause::InstructionAccessFault,
            value: pc,
        })?;
        let instruction = u32::from_le_bytes(bytes);
        let opcode = instruction & 0x7f;
        let rd = ((instruction >> 7) & 0x1f) as usize;
        let funct3 = (instruction >> 12) & 0x7;
        let rs1 = ((instruction >> 15) & 0x1f) as usize;
        let rs2 = ((instruction >> 20) & 0x1f) as usize;
        let left = self.integer_registers[rs1];
        let right = self.integer_registers[rs2];
        let mut next_pc = pc.wrapping_add(4);
        let mut result = None;

        match opcode {
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
                let mut data = [0; 8];
                bus.read(address, &mut data[..length as usize])
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
                bus.write(address, &right.to_le_bytes()[..length as usize])
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
                let mut data = [0; 8];
                match operation {
                    0b00010 if rs2 == 0 => {
                        let region = bus
                            .check_access(address, length, PhysicalAccess::Read)
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
                        bus.read(address, &mut data[..length as usize])
                            .map_err(|_| CoreTrap {
                                pc,
                                cause: CoreTrapCause::LoadAccessFault,
                                value: address,
                            })?;
                        self.reservation = Some((address, length as u8, bus.write_epoch));
                        let old = u64::from_le_bytes(data);
                        result = Some(if length == 4 {
                            old as u32 as i32 as i64 as u64
                        } else {
                            old
                        });
                    }
                    0b00011 => {
                        let region = bus
                            .check_access(address, length, PhysicalAccess::Write)
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
                        let succeeded = reserved == Some((address, length as u8, bus.write_epoch));
                        if succeeded {
                            bus.write(address, &right.to_le_bytes()[..length as usize])
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
                            .check_access(address, length, PhysicalAccess::Write)
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
                        bus.read(address, &mut data[..length as usize])
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
                        bus.write(address, &new.to_le_bytes()[..length as usize])
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

fn illegal_instruction(pc: u64, instruction: u32) -> CoreTrap {
    CoreTrap {
        pc,
        cause: CoreTrapCause::IllegalInstruction,
        value: instruction as u64,
    }
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
            Extension::F,
            Extension::D,
            Extension::C,
            Extension::Zicsr,
            Extension::Zifencei,
        ] {
            assert!(XV6_PROFILE.supported_extensions.contains(&extension));
        }
        assert_eq!(
            XV6_PROFILE.unsupported_extensions,
            &[Extension::Hypervisor, Extension::Vector]
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

    fn encode_r(funct7: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
    }

    fn encode_atomic(funct5: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32) -> u32 {
        (funct5 << 27) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x2f
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
        let mut compressed = bus_with_program(&[0x0000_0001]);
        let trap = core.step(&mut compressed).unwrap_err();
        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
        assert_eq!(trap.value, 1);
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
    fn bus_does_not_fake_unimplemented_mmio() {
        let bus = Kiuas16Bus::new_with_ram_size(vec![0; BOOT_ROM_BYTES], 8).unwrap();
        assert_eq!(
            bus.read(UART.start, &mut [0]),
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
