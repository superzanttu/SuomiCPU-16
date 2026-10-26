pub const PAGE_SIZE: usize = 4096;
pub const PAGE_MASK: u32 = (PAGE_SIZE as u32) - 1;
pub const PAGE_DIRECTORY_ENTRIES: usize = 512;
pub const PAGE_TABLE_ENTRIES: usize = 512;
pub const PAGE_TABLE_BYTES: usize = PAGE_TABLE_ENTRIES * 4;
pub const ADDRESS_SPACE_SIZE: u32 = 1 << 30;
pub const PTE_PRESENT: u32 = 1;
pub const PTE_WRITABLE: u32 = 1 << 1;
pub const PTE_USER: u32 = 1 << 2;
pub const PTE_EXECUTABLE: u32 = 1 << 3;
const PTE_FRAME_MASK: u32 = 0x3ffff << 12;
const PTE_RESERVED_MASK: u32 = 0xc000_0ff0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryAccess {
    Fetch,
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageFaultReason {
    AddressOutOfRange,
    NotPresent,
    ReadOnly,
    SupervisorOnly,
    NotExecutable,
    MalformedEntry,
    PhysicalOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFault {
    pub virtual_address: u32,
    pub access: MemoryAccess,
    pub reason: PageFaultReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagePermissions {
    pub writable: bool,
    pub user: bool,
    pub executable: bool,
}

impl PagePermissions {
    pub const KERNEL_CODE: Self = Self {
        writable: false,
        user: false,
        executable: true,
    };

    pub const KERNEL_DATA: Self = Self {
        writable: true,
        user: false,
        executable: false,
    };

    pub const USER_CODE: Self = Self {
        writable: false,
        user: true,
        executable: true,
    };

    pub const USER_DATA: Self = Self {
        writable: true,
        user: true,
        executable: false,
    };
}

pub fn encode_entry(frame: u32, permissions: PagePermissions) -> Option<u32> {
    if frame >= 1 << 18 {
        return None;
    }
    Some(
        (frame << 12)
            | PTE_PRESENT
            | (u32::from(permissions.writable) * PTE_WRITABLE)
            | (u32::from(permissions.user) * PTE_USER)
            | (u32::from(permissions.executable) * PTE_EXECUTABLE),
    )
}

pub fn translate(
    memory: &[u8],
    directory: u32,
    virtual_address: u32,
    access: MemoryAccess,
    user_mode: bool,
) -> Result<u32, PageFault> {
    let fail = |reason| PageFault {
        virtual_address,
        access,
        reason,
    };
    if virtual_address >= ADDRESS_SPACE_SIZE {
        return Err(fail(PageFaultReason::AddressOutOfRange));
    }
    if directory & 0x7ff != 0 {
        return Err(fail(PageFaultReason::MalformedEntry));
    }
    let directory_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let table_index = ((virtual_address >> 12) & 0x1ff) as usize;
    let directory_entry = read_entry(memory, directory, directory_index)
        .ok_or_else(|| fail(PageFaultReason::PhysicalOutOfRange))?;
    validate_entry(directory_entry, &fail)?;
    if directory_entry & PTE_PRESENT == 0 {
        return Err(fail(PageFaultReason::NotPresent));
    }

    let table = (directory_entry & PTE_FRAME_MASK) >> 12;
    let table_entry = read_entry(memory, table * PAGE_SIZE as u32, table_index)
        .ok_or_else(|| fail(PageFaultReason::PhysicalOutOfRange))?;
    validate_entry(table_entry, &fail)?;
    if table_entry & PTE_PRESENT == 0 {
        return Err(fail(PageFaultReason::NotPresent));
    }

    let writable = directory_entry & PTE_WRITABLE != 0 && table_entry & PTE_WRITABLE != 0;
    let user = directory_entry & PTE_USER != 0 && table_entry & PTE_USER != 0;
    let executable = directory_entry & PTE_EXECUTABLE != 0 && table_entry & PTE_EXECUTABLE != 0;
    if user_mode && !user {
        return Err(fail(PageFaultReason::SupervisorOnly));
    }
    if access == MemoryAccess::Write && !writable {
        return Err(fail(PageFaultReason::ReadOnly));
    }
    if access == MemoryAccess::Fetch && !executable {
        return Err(fail(PageFaultReason::NotExecutable));
    }

    let physical_address =
        (((table_entry & PTE_FRAME_MASK) >> 12) * PAGE_SIZE as u32) | (virtual_address & PAGE_MASK);
    if physical_address as usize >= memory.len() {
        return Err(fail(PageFaultReason::PhysicalOutOfRange));
    }
    Ok(physical_address)
}

fn read_entry(memory: &[u8], base: u32, index: usize) -> Option<u32> {
    let address = usize::try_from(base)
        .ok()?
        .checked_add(index.checked_mul(4)?)?;
    let bytes = memory.get(address..address.checked_add(4)?)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn validate_entry(
    entry: u32,
    fail: &impl Fn(PageFaultReason) -> PageFault,
) -> Result<(), PageFault> {
    if entry & PTE_RESERVED_MASK != 0 {
        Err(fail(PageFaultReason::MalformedEntry))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_entry(memory: &mut [u8], address: usize, value: u32) {
        memory[address..address + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn mapped_memory(
        directory_permissions: PagePermissions,
        table_permissions: PagePermissions,
    ) -> Vec<u8> {
        let mut memory = vec![0; 0x8000];
        write_entry(
            &mut memory,
            0x0000,
            encode_entry(2, directory_permissions).unwrap(),
        );
        write_entry(
            &mut memory,
            0x2000,
            encode_entry(3, table_permissions).unwrap(),
        );
        memory[0x3000] = 0x5a;
        memory
    }

    #[test]
    fn translates_address_and_intersects_permissions() {
        let memory = mapped_memory(PagePermissions::USER_DATA, PagePermissions::USER_DATA);

        assert_eq!(
            translate(&memory, 0, 0x0000_0123, MemoryAccess::Read, true),
            Ok(0x3123)
        );
        assert_eq!(
            translate(&memory, 0, 0x0000_0123, MemoryAccess::Write, true),
            Ok(0x3123)
        );
        assert_eq!(
            translate(&memory, 0, 0x0000_0123, MemoryAccess::Fetch, true),
            Err(PageFault {
                virtual_address: 0x123,
                access: MemoryAccess::Fetch,
                reason: PageFaultReason::NotExecutable,
            })
        );
    }

    #[test]
    fn supervisor_mappings_reject_user_access_and_writes_to_read_only_pages() {
        let memory = mapped_memory(PagePermissions::KERNEL_CODE, PagePermissions::KERNEL_CODE);

        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Fetch, true)
                .unwrap_err()
                .reason,
            PageFaultReason::SupervisorOnly
        );
        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Write, false)
                .unwrap_err()
                .reason,
            PageFaultReason::ReadOnly
        );
        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Fetch, false),
            Ok(0x3000)
        );
    }

    #[test]
    fn rejects_missing_malformed_and_out_of_range_entries() {
        let mut memory = vec![0; 0x8000];
        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Read, false)
                .unwrap_err()
                .reason,
            PageFaultReason::NotPresent
        );
        write_entry(&mut memory, 0, PTE_PRESENT | (1 << 4));
        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Read, false)
                .unwrap_err()
                .reason,
            PageFaultReason::MalformedEntry
        );
        assert_eq!(
            translate(&memory, 0, ADDRESS_SPACE_SIZE, MemoryAccess::Read, false)
                .unwrap_err()
                .reason,
            PageFaultReason::PhysicalOutOfRange
        );
        assert!(encode_entry(1 << 18, PagePermissions::USER_DATA).is_none());
    }
}
