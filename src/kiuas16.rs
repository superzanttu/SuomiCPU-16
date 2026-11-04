pub const PHYSICAL_ADDRESS_BITS: u32 = 56;
pub const PHYSICAL_ADDRESS_LIMIT: u64 = 1 << PHYSICAL_ADDRESS_BITS;

pub const BOOT_ROM: AddressRange = AddressRange::new(0x0000_1000, 0x0001_0000);
pub const PLIC: AddressRange = AddressRange::new(0x0c00_0000, 0x1000_0000);
pub const UART: AddressRange = AddressRange::new(0x1000_0000, 0x1000_1000);
pub const ROOT_BLOCK: AddressRange = AddressRange::new(0x1000_1000, 0x1000_2000);
pub const RAM: AddressRange = AddressRange::new(0x8000_0000, 0x8800_0000);

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
