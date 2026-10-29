//! Implements SC-16 paged address translation and physical-frame management.
//! Defines page permissions and faults, validates page tables, and manages
//! address-space allocation, mapping, permission changes, and teardown.

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_MASK: u32 = (PAGE_SIZE as u32) - 1;
pub const PAGE_DIRECTORY_ENTRIES: usize = 512;
pub const PAGE_TABLE_ENTRIES: usize = 512;
pub const PAGE_TABLE_BYTES: usize = PAGE_TABLE_ENTRIES * 4;
pub use crate::machine::ADDRESS_SPACE_SIZE;
pub const PTE_PRESENT: u32 = 1;
pub const PTE_WRITABLE: u32 = 1 << 1;
pub const PTE_USER: u32 = 1 << 2;
pub const PTE_EXECUTABLE: u32 = 1 << 3;
const PTE_FRAME_MASK: u32 = 0x3ffff << 12;
const PTE_RESERVED_MASK: u32 = 0xc000_0ff0;
const PTE_DIRECTORY_FLAGS: PagePermissions = PagePermissions {
    writable: true,
    user: true,
    executable: true,
};

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
pub enum PageTableError {
    InvalidAddressSpace,
    InvalidVirtualAddress,
    InvalidPhysicalFrame,
    AlreadyMapped,
    NotMapped,
    OutOfFrames,
    MalformedEntry,
    OutOfMemoryRange,
}

#[derive(Clone, Debug, Default)]
struct AddressSpaceMappings {
    pages: HashMap<u32, Option<u32>>,
    table_frames: HashSet<u32>,
    owns_directory: bool,
}

#[derive(Clone, Debug)]
pub struct PageMemoryManager {
    allocator: FrameAllocator,
    address_spaces: HashMap<u32, AddressSpaceMappings>,
}

impl PageMemoryManager {
    pub fn new(memory_size: usize, reserved_bytes: usize) -> Result<Self, PageTableError> {
        Ok(Self {
            allocator: FrameAllocator::new(memory_size, reserved_bytes)?,
            address_spaces: HashMap::new(),
        })
    }

    pub fn create_address_space(&mut self, memory: &mut [u8]) -> Result<u32, PageTableError> {
        let frame = self.allocator.allocate()?;
        let start = frame as usize * PAGE_SIZE;
        let Some(end) = start.checked_add(PAGE_TABLE_BYTES) else {
            self.allocator.free(frame)?;
            return Err(PageTableError::OutOfMemoryRange);
        };
        if end > crate::machine::usable_ram_size(memory.len()) {
            self.allocator.free(frame)?;
            return Err(PageTableError::OutOfMemoryRange);
        }
        memory[start..start + PAGE_SIZE].fill(0);
        let directory = start as u32;
        self.address_spaces.insert(
            directory,
            AddressSpaceMappings {
                owns_directory: true,
                ..AddressSpaceMappings::default()
            },
        );
        Ok(directory)
    }

    pub fn adopt_address_space(
        &mut self,
        memory: &[u8],
        directory: u32,
    ) -> Result<(), PageTableError> {
        if self.address_spaces.contains_key(&directory) {
            return Ok(());
        }
        for frame in address_space_physical_frames(memory, directory)? {
            self.allocator
                .reserve_range(frame as usize * PAGE_SIZE, PAGE_SIZE)?;
        }
        let mut mappings = AddressSpaceMappings {
            owns_directory: false,
            ..AddressSpaceMappings::default()
        };
        for directory_index in 0..PAGE_DIRECTORY_ENTRIES {
            let directory_entry = read_entry(memory, directory, directory_index)
                .ok_or(PageTableError::InvalidAddressSpace)?;
            if directory_entry & PTE_PRESENT == 0 {
                continue;
            }
            let table_frame = (directory_entry & PTE_FRAME_MASK) >> 12;
            let table = table_frame * PAGE_SIZE as u32;
            for table_index in 0..PAGE_TABLE_ENTRIES {
                let table_entry = read_entry(memory, table, table_index)
                    .ok_or(PageTableError::InvalidAddressSpace)?;
                if table_entry & PTE_PRESENT != 0 {
                    let page = (directory_index as u32 * PAGE_DIRECTORY_ENTRIES as u32)
                        + table_index as u32;
                    mappings.pages.insert(page, None);
                }
            }
        }
        self.address_spaces.insert(directory, mappings);
        Ok(())
    }

    pub fn reserve_physical_range(
        &mut self,
        start: usize,
        length: usize,
    ) -> Result<(), PageTableError> {
        self.allocator.reserve_range(start, length)
    }

    pub fn map_borrowed_page(
        &mut self,
        memory: &mut [u8],
        directory: u32,
        virtual_address: u32,
        physical_frame: u32,
        permissions: PagePermissions,
    ) -> Result<(), PageTableError> {
        let mappings = self
            .address_spaces
            .get_mut(&directory)
            .ok_or(PageTableError::InvalidAddressSpace)?;
        let allocated_table = map_page(
            memory,
            directory,
            virtual_address,
            physical_frame,
            permissions,
            &mut self.allocator,
        )?;
        if let Some(frame) = allocated_table {
            mappings.table_frames.insert(frame);
        }
        mappings
            .pages
            .insert(virtual_address / PAGE_SIZE as u32, None);
        Ok(())
    }

    pub fn allocate_and_map_page(
        &mut self,
        memory: &mut [u8],
        directory: u32,
        virtual_address: u32,
        permissions: PagePermissions,
    ) -> Result<u32, PageTableError> {
        let frame = self.allocator.allocate()?;
        let start = frame as usize * PAGE_SIZE;
        let Some(end) = start.checked_add(PAGE_SIZE) else {
            self.allocator.free(frame)?;
            return Err(PageTableError::OutOfMemoryRange);
        };
        if end > crate::machine::usable_ram_size(memory.len()) {
            self.allocator.free(frame)?;
            return Err(PageTableError::OutOfMemoryRange);
        }
        memory[start..end].fill(0);
        match self.map_borrowed_page(memory, directory, virtual_address, frame, permissions) {
            Ok(()) => {
                self.address_spaces
                    .get_mut(&directory)
                    .ok_or(PageTableError::InvalidAddressSpace)?
                    .pages
                    .insert(virtual_address / PAGE_SIZE as u32, Some(frame));
                Ok(frame)
            }
            Err(error) => {
                self.allocator.free(frame)?;
                Err(error)
            }
        }
    }

    pub fn set_permissions(
        &mut self,
        memory: &mut [u8],
        directory: u32,
        virtual_address: u32,
        permissions: PagePermissions,
    ) -> Result<(), PageTableError> {
        if !self.address_spaces.contains_key(&directory) {
            return Err(PageTableError::InvalidAddressSpace);
        }
        set_page_permissions(memory, directory, virtual_address, permissions)
    }

    pub fn unmap_page(
        &mut self,
        memory: &mut [u8],
        directory: u32,
        virtual_address: u32,
    ) -> Result<(), PageTableError> {
        let page = virtual_address / PAGE_SIZE as u32;
        let mappings = self
            .address_spaces
            .get_mut(&directory)
            .ok_or(PageTableError::InvalidAddressSpace)?;
        let owned_frame = *mappings.pages.get(&page).ok_or(PageTableError::NotMapped)?;
        let released_table = unmap_page(memory, directory, virtual_address)?;
        mappings.pages.remove(&page);
        if let Some(frame) = owned_frame {
            self.allocator.free(frame)?;
        }
        if let Some(frame) = released_table {
            if mappings.table_frames.remove(&frame) {
                self.allocator.free(frame)?;
            }
        }
        Ok(())
    }

    pub fn destroy_address_space(
        &mut self,
        memory: &mut [u8],
        directory: u32,
    ) -> Result<(), PageTableError> {
        let mappings = self
            .address_spaces
            .remove(&directory)
            .ok_or(PageTableError::InvalidAddressSpace)?;
        if !mappings.owns_directory {
            self.address_spaces.insert(directory, mappings);
            return Err(PageTableError::InvalidAddressSpace);
        }
        for (page, owned_frame) in mappings.pages {
            let virtual_address = page * PAGE_SIZE as u32;
            unmap_page(memory, directory, virtual_address)?;
            if let Some(frame) = owned_frame {
                self.allocator.free(frame)?;
            }
        }
        for frame in mappings.table_frames {
            self.allocator.free(frame)?;
        }
        self.allocator.free(directory / PAGE_SIZE as u32)?;
        Ok(())
    }

    pub fn identity_map_range(
        &mut self,
        memory: &mut [u8],
        directory: u32,
        start: u32,
        length: u32,
        permissions: PagePermissions,
    ) -> Result<(), PageTableError> {
        if length == 0 {
            return Ok(());
        }
        let end = start
            .checked_add(length)
            .filter(|end| *end <= ADDRESS_SPACE_SIZE)
            .ok_or(PageTableError::InvalidVirtualAddress)?;
        if end as usize > crate::machine::usable_ram_size(memory.len()) {
            return Err(PageTableError::OutOfMemoryRange);
        }
        let first_page = start & !PAGE_MASK;
        let end_page = end
            .checked_add(PAGE_MASK)
            .ok_or(PageTableError::InvalidVirtualAddress)?
            & !PAGE_MASK;
        let mut address = first_page;
        while address < end_page {
            let frame = address / PAGE_SIZE as u32;
            self.map_borrowed_page(memory, directory, address, frame, permissions)?;
            address += PAGE_SIZE as u32;
        }
        Ok(())
    }

    pub fn used_frames(&self) -> usize {
        self.allocator.used_frames()
    }

    pub fn free_frames(&self) -> usize {
        self.allocator.total_frames() - self.allocator.used_frames()
    }

    pub fn total_frames(&self) -> usize {
        self.allocator.total_frames()
    }
}

#[derive(Clone, Debug)]
pub struct FrameAllocator {
    allocated: Vec<bool>,
    reserved: Vec<bool>,
    next_frame: usize,
}

impl FrameAllocator {
    pub fn new(memory_size: usize, reserved_bytes: usize) -> Result<Self, PageTableError> {
        if memory_size == 0 || memory_size > ADDRESS_SPACE_SIZE as usize {
            return Err(PageTableError::InvalidPhysicalFrame);
        }
        let frame_count = crate::machine::usable_ram_size(memory_size).div_ceil(PAGE_SIZE);
        let reserved_frames = reserved_bytes.div_ceil(PAGE_SIZE).min(frame_count);
        let mut allocated = vec![false; frame_count];
        allocated[..reserved_frames].fill(true);
        let mut reserved = vec![false; frame_count];
        reserved[..reserved_frames].fill(true);
        Ok(Self {
            allocated,
            reserved,
            next_frame: reserved_frames,
        })
    }

    pub fn allocate(&mut self) -> Result<u32, PageTableError> {
        let frame_count = self.allocated.len();
        for offset in 0..frame_count {
            let frame = (self.next_frame + offset) % frame_count;
            if !self.allocated[frame] {
                self.allocated[frame] = true;
                self.next_frame = (frame + 1) % frame_count;
                return u32::try_from(frame).map_err(|_| PageTableError::InvalidPhysicalFrame);
            }
        }
        Err(PageTableError::OutOfFrames)
    }

    pub fn free(&mut self, frame: u32) -> Result<(), PageTableError> {
        let index = usize::try_from(frame).map_err(|_| PageTableError::InvalidPhysicalFrame)?;
        let Some(allocated) = self.allocated.get_mut(index) else {
            return Err(PageTableError::InvalidPhysicalFrame);
        };
        if self.reserved[index] || !*allocated {
            return Err(PageTableError::InvalidPhysicalFrame);
        }
        *allocated = false;
        self.next_frame = self.next_frame.min(index);
        Ok(())
    }

    pub fn reserve_range(&mut self, start: usize, length: usize) -> Result<(), PageTableError> {
        let end = start
            .checked_add(length)
            .filter(|end| *end <= self.allocated.len() * PAGE_SIZE)
            .ok_or(PageTableError::InvalidPhysicalFrame)?;
        if length == 0 {
            return Ok(());
        }
        let first = start / PAGE_SIZE;
        let last = end.div_ceil(PAGE_SIZE);
        for frame in first..last {
            if self.allocated[frame] && !self.reserved[frame] {
                return Err(PageTableError::InvalidPhysicalFrame);
            }
        }
        self.allocated[first..last].fill(true);
        self.reserved[first..last].fill(true);
        Ok(())
    }

    pub fn used_frames(&self) -> usize {
        self.allocated
            .iter()
            .filter(|allocated| **allocated)
            .count()
    }

    pub fn total_frames(&self) -> usize {
        self.allocated.len()
    }
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

    let physical_address =
        (((table_entry & PTE_FRAME_MASK) >> 12) * PAGE_SIZE as u32) | (virtual_address & PAGE_MASK);
    let device = crate::machine::is_implemented_device_page(physical_address);
    if device && user_mode {
        return Err(fail(PageFaultReason::SupervisorOnly));
    }
    if device && access == MemoryAccess::Fetch {
        return Err(fail(PageFaultReason::NotExecutable));
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

    if device {
        return Ok(physical_address);
    }
    if physical_address as usize >= crate::machine::usable_ram_size(memory.len()) {
        return Err(fail(PageFaultReason::PhysicalOutOfRange));
    }
    Ok(physical_address)
}

pub fn address_space_physical_frames(
    memory: &[u8],
    directory: u32,
) -> Result<Vec<u32>, PageTableError> {
    if directory & 0x7ff != 0
        || directory
            .checked_add(PAGE_TABLE_BYTES as u32)
            .is_none_or(|end| end as usize > crate::machine::usable_ram_size(memory.len()))
    {
        return Err(PageTableError::InvalidAddressSpace);
    }
    let mut frames = HashSet::new();
    frames.insert(directory / PAGE_SIZE as u32);
    for directory_index in 0..PAGE_DIRECTORY_ENTRIES {
        let entry = read_entry(memory, directory, directory_index)
            .ok_or(PageTableError::InvalidAddressSpace)?;
        validate_entry(entry, &|_| PageFault {
            virtual_address: 0,
            access: MemoryAccess::Read,
            reason: PageFaultReason::MalformedEntry,
        })
        .map_err(|_| PageTableError::MalformedEntry)?;
        if entry & PTE_PRESENT == 0 {
            continue;
        }
        let table_frame = (entry & PTE_FRAME_MASK) >> 12;
        frames.insert(table_frame);
        let table = table_frame * PAGE_SIZE as u32;
        for table_index in 0..PAGE_TABLE_ENTRIES {
            let table_entry = read_entry(memory, table, table_index)
                .ok_or(PageTableError::InvalidAddressSpace)?;
            validate_entry(table_entry, &|_| PageFault {
                virtual_address: 0,
                access: MemoryAccess::Read,
                reason: PageFaultReason::MalformedEntry,
            })
            .map_err(|_| PageTableError::MalformedEntry)?;
            if table_entry & PTE_PRESENT != 0 {
                let frame = (table_entry & PTE_FRAME_MASK) >> 12;
                let permissions = PagePermissions {
                    writable: table_entry & PTE_WRITABLE != 0,
                    user: table_entry & PTE_USER != 0,
                    executable: table_entry & PTE_EXECUTABLE != 0,
                };
                if !valid_leaf_frame(memory.len(), frame, permissions) {
                    return Err(PageTableError::InvalidPhysicalFrame);
                }
                if !crate::machine::is_implemented_device_page(frame * PAGE_SIZE as u32) {
                    frames.insert(frame);
                }
            }
        }
    }
    let mut frames: Vec<_> = frames.into_iter().collect();
    frames.sort_unstable();
    Ok(frames)
}

pub fn map_page(
    memory: &mut [u8],
    directory: u32,
    virtual_address: u32,
    physical_frame: u32,
    permissions: PagePermissions,
    allocator: &mut FrameAllocator,
) -> Result<Option<u32>, PageTableError> {
    if directory & 0x7ff != 0
        || directory
            .checked_add(PAGE_TABLE_BYTES as u32)
            .is_none_or(|end| end as usize > crate::machine::usable_ram_size(memory.len()))
    {
        return Err(PageTableError::InvalidAddressSpace);
    }
    if virtual_address >= ADDRESS_SPACE_SIZE || virtual_address & PAGE_MASK != 0 {
        return Err(PageTableError::InvalidVirtualAddress);
    }
    if !valid_leaf_frame(memory.len(), physical_frame, permissions) {
        return Err(PageTableError::InvalidPhysicalFrame);
    }
    let directory_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let table_index = ((virtual_address >> 12) & 0x1ff) as usize;
    let directory_entry = read_entry(memory, directory, directory_index)
        .ok_or(PageTableError::InvalidAddressSpace)?;
    if directory_entry & PTE_RESERVED_MASK != 0 {
        return Err(PageTableError::MalformedEntry);
    }
    let (table_frame, allocated_table) = if directory_entry & PTE_PRESENT != 0 {
        (((directory_entry & PTE_FRAME_MASK) >> 12), None)
    } else {
        let frame = allocator.allocate()?;
        let table_start = frame as usize * PAGE_SIZE;
        let Some(table_end) = table_start.checked_add(PAGE_TABLE_BYTES) else {
            allocator.free(frame)?;
            return Err(PageTableError::InvalidAddressSpace);
        };
        if table_end > crate::machine::usable_ram_size(memory.len()) {
            allocator.free(frame)?;
            return Err(PageTableError::InvalidAddressSpace);
        }
        memory[table_start..table_end].fill(0);
        let entry =
            encode_entry(frame, PTE_DIRECTORY_FLAGS).ok_or(PageTableError::InvalidPhysicalFrame)?;
        write_entry(memory, directory, directory_index, entry)?;
        (frame, Some(frame))
    };
    let table_base = table_frame * PAGE_SIZE as u32;
    let entry =
        read_entry(memory, table_base, table_index).ok_or(PageTableError::InvalidAddressSpace)?;
    if entry & PTE_RESERVED_MASK != 0 {
        if let Some(frame) = allocated_table {
            write_entry(memory, directory, directory_index, 0)?;
            allocator.free(frame)?;
        }
        return Err(PageTableError::MalformedEntry);
    }
    if entry & PTE_PRESENT != 0 {
        if let Some(frame) = allocated_table {
            write_entry(memory, directory, directory_index, 0)?;
            allocator.free(frame)?;
        }
        return Err(PageTableError::AlreadyMapped);
    }
    let page_entry =
        encode_entry(physical_frame, permissions).ok_or(PageTableError::InvalidPhysicalFrame)?;
    write_entry(memory, table_base, table_index, page_entry)?;
    Ok(allocated_table)
}

pub fn set_page_permissions(
    memory: &mut [u8],
    directory: u32,
    virtual_address: u32,
    permissions: PagePermissions,
) -> Result<(), PageTableError> {
    let (directory_index, table_index) = indices(directory, virtual_address)?;
    let directory_entry = read_entry(memory, directory, directory_index)
        .ok_or(PageTableError::InvalidAddressSpace)?;
    if directory_entry & PTE_RESERVED_MASK != 0 {
        return Err(PageTableError::MalformedEntry);
    }
    if directory_entry & PTE_PRESENT == 0 {
        return Err(PageTableError::NotMapped);
    }
    let table_base = ((directory_entry & PTE_FRAME_MASK) >> 12) * PAGE_SIZE as u32;
    let page_entry =
        read_entry(memory, table_base, table_index).ok_or(PageTableError::InvalidAddressSpace)?;
    if page_entry & PTE_RESERVED_MASK != 0 {
        return Err(PageTableError::MalformedEntry);
    }
    if page_entry & PTE_PRESENT == 0 {
        return Err(PageTableError::NotMapped);
    }
    let frame = (page_entry & PTE_FRAME_MASK) >> 12;
    if !valid_leaf_frame(memory.len(), frame, permissions) {
        return Err(PageTableError::InvalidPhysicalFrame);
    }
    let updated = encode_entry(frame, permissions).ok_or(PageTableError::InvalidPhysicalFrame)?;
    write_entry(memory, table_base, table_index, updated)
}

fn valid_leaf_frame(memory_size: usize, frame: u32, permissions: PagePermissions) -> bool {
    let Some(address) = frame.checked_mul(PAGE_SIZE as u32) else {
        return false;
    };
    if crate::machine::is_implemented_device_page(address) {
        return !permissions.user && !permissions.executable;
    }
    frame < crate::machine::usable_ram_size(memory_size).div_ceil(PAGE_SIZE) as u32
}

pub fn unmap_page(
    memory: &mut [u8],
    directory: u32,
    virtual_address: u32,
) -> Result<Option<u32>, PageTableError> {
    let (directory_index, table_index) = indices(directory, virtual_address)?;
    let directory_entry = read_entry(memory, directory, directory_index)
        .ok_or(PageTableError::InvalidAddressSpace)?;
    if directory_entry & PTE_RESERVED_MASK != 0 {
        return Err(PageTableError::MalformedEntry);
    }
    if directory_entry & PTE_PRESENT == 0 {
        return Err(PageTableError::NotMapped);
    }
    let table_frame = (directory_entry & PTE_FRAME_MASK) >> 12;
    let table_base = table_frame * PAGE_SIZE as u32;
    let page_entry =
        read_entry(memory, table_base, table_index).ok_or(PageTableError::InvalidAddressSpace)?;
    if page_entry & PTE_RESERVED_MASK != 0 {
        return Err(PageTableError::MalformedEntry);
    }
    if page_entry & PTE_PRESENT == 0 {
        return Err(PageTableError::NotMapped);
    }
    write_entry(memory, table_base, table_index, 0)?;
    let empty = (0..PAGE_TABLE_ENTRIES)
        .all(|index| read_entry(memory, table_base, index).is_some_and(|entry| entry == 0));
    if empty {
        write_entry(memory, directory, directory_index, 0)?;
        Ok(Some(table_frame))
    } else {
        Ok(None)
    }
}

fn indices(directory: u32, virtual_address: u32) -> Result<(usize, usize), PageTableError> {
    if directory & 0x7ff != 0 {
        return Err(PageTableError::InvalidAddressSpace);
    }
    if virtual_address >= ADDRESS_SPACE_SIZE || virtual_address & PAGE_MASK != 0 {
        return Err(PageTableError::InvalidVirtualAddress);
    }
    Ok((
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ))
}

fn read_entry(memory: &[u8], base: u32, index: usize) -> Option<u32> {
    let address = usize::try_from(base)
        .ok()?
        .checked_add(index.checked_mul(4)?)?;
    let end = address.checked_add(4)?;
    if end > crate::machine::usable_ram_size(memory.len()) {
        return None;
    }
    let bytes = memory.get(address..end)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn write_entry(
    memory: &mut [u8],
    base: u32,
    index: usize,
    value: u32,
) -> Result<(), PageTableError> {
    let address = usize::try_from(base)
        .ok()
        .and_then(|base| {
            index
                .checked_mul(4)
                .and_then(|offset| base.checked_add(offset))
        })
        .ok_or(PageTableError::InvalidAddressSpace)?;
    let end = address
        .checked_add(4)
        .filter(|end| *end <= crate::machine::usable_ram_size(memory.len()))
        .ok_or(PageTableError::InvalidAddressSpace)?;
    memory[address..end].copy_from_slice(&value.to_be_bytes());
    Ok(())
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

    #[test]
    fn frame_allocator_never_allocates_the_high_device_aperture() {
        let mut allocator = FrameAllocator::new(ADDRESS_SPACE_SIZE as usize, 0).unwrap();
        let ram_frames = crate::machine::DEVICE_APERTURE_START as usize / PAGE_SIZE;
        assert_eq!(allocator.total_frames(), ram_frames);
        for expected in 0..ram_frames {
            assert_eq!(allocator.allocate().unwrap(), expected as u32);
        }
        assert_eq!(allocator.allocate(), Err(PageTableError::OutOfFrames));
        allocator.free(ram_frames as u32 - 1).unwrap();
        assert_eq!(allocator.allocate().unwrap(), ram_frames as u32 - 1);
        assert_eq!(
            allocator.free(ram_frames as u32),
            Err(PageTableError::InvalidPhysicalFrame)
        );
    }

    #[test]
    fn high_aperture_is_never_page_table_storage_or_ram_mapping() {
        let mut memory = memmap2::MmapMut::map_anon(ADDRESS_SPACE_SIZE as usize).unwrap();
        let aperture = crate::machine::DEVICE_APERTURE_START;
        let mut manager = PageMemoryManager::new(memory.len(), 0x4000).unwrap();
        let root = manager.create_address_space(&mut memory).unwrap();
        assert_eq!(
            manager.map_borrowed_page(
                &mut memory,
                root,
                0,
                aperture / PAGE_SIZE as u32,
                PagePermissions::USER_DATA
            ),
            Err(PageTableError::InvalidPhysicalFrame)
        );
        assert_eq!(
            manager.adopt_address_space(&memory, aperture),
            Err(PageTableError::InvalidAddressSpace)
        );
        assert_eq!(
            manager.identity_map_range(
                &mut memory,
                root,
                aperture - PAGE_SIZE as u32,
                PAGE_SIZE as u32 * 2,
                PagePermissions::KERNEL_DATA
            ),
            Err(PageTableError::OutOfMemoryRange)
        );
        assert_eq!(
            translate(
                &memory,
                root,
                aperture - PAGE_SIZE as u32,
                MemoryAccess::Read,
                false
            )
            .unwrap_err()
            .reason,
            PageFaultReason::NotPresent
        );

        write_entry(
            &mut memory,
            0,
            encode_entry(2, PagePermissions::USER_CODE).unwrap(),
        );
        write_entry(
            &mut memory,
            0x2000,
            encode_entry(
                crate::machine::GRAPHICS_BASE / PAGE_SIZE as u32,
                PagePermissions::USER_CODE,
            )
            .unwrap(),
        );
        for user in [false, true] {
            for access in [MemoryAccess::Read, MemoryAccess::Fetch] {
                assert_eq!(
                    translate(&memory, 0, 0, access, user).unwrap_err().reason,
                    PageFaultReason::PhysicalOutOfRange
                );
            }
        }
        assert_eq!(
            address_space_physical_frames(&memory, 0),
            Err(PageTableError::InvalidPhysicalFrame)
        );
        assert_eq!(
            set_page_permissions(&mut memory, 0, 0, PagePermissions::USER_DATA),
            Err(PageTableError::InvalidPhysicalFrame)
        );
        write_entry(
            &mut memory,
            0,
            encode_entry(aperture / PAGE_SIZE as u32, PagePermissions::USER_CODE).unwrap(),
        );
        assert_eq!(
            translate(&memory, 0, 0, MemoryAccess::Read, false)
                .unwrap_err()
                .reason,
            PageFaultReason::PhysicalOutOfRange
        );
        assert_eq!(
            translate(&memory, aperture, 0, MemoryAccess::Read, false)
                .unwrap_err()
                .reason,
            PageFaultReason::PhysicalOutOfRange
        );
        assert_eq!(
            set_page_permissions(&mut memory, aperture, 0, PagePermissions::KERNEL_DATA),
            Err(PageTableError::InvalidAddressSpace)
        );
        assert_eq!(
            unmap_page(&mut memory, aperture, 0),
            Err(PageTableError::InvalidAddressSpace)
        );
    }

    fn write_entry(memory: &mut [u8], address: usize, value: u32) {
        memory[address..address + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn supervisor_device_mappings_do_not_own_or_reserve_device_frames() {
        for base in [
            crate::machine::SYSTEM_BASE,
            crate::machine::INTERRUPT_BASE,
            crate::machine::FAULT_BASE,
            crate::machine::CLOCK_BASE,
            crate::machine::INPUT_BASE,
            crate::machine::CONSOLE_BASE,
            crate::machine::FIRMWARE_TEXT_BASE,
            crate::machine::AUDIO_BASE,
            crate::machine::NETWORK_BASE,
            crate::machine::FIRMWARE_GATEWAY_BASE,
        ] {
            let mut memory = vec![0; 1 << 16];
            let mut manager = PageMemoryManager::new(memory.len(), 0x2000).unwrap();
            let root = manager.create_address_space(&mut memory).unwrap();
            let used = manager.used_frames();
            let device = base / PAGE_SIZE as u32;
            for permissions in [PagePermissions::USER_DATA, PagePermissions::KERNEL_CODE] {
                assert_eq!(
                    manager.map_borrowed_page(&mut memory, root, 0x9000, device, permissions),
                    Err(PageTableError::InvalidPhysicalFrame)
                );
                assert_eq!(manager.used_frames(), used);
            }
            manager
                .map_borrowed_page(
                    &mut memory,
                    root,
                    0x9000,
                    device,
                    PagePermissions::KERNEL_DATA,
                )
                .unwrap();
            assert_eq!(manager.used_frames(), used + 1);
            assert_eq!(
                translate(&memory, root, 0x9008, MemoryAccess::Read, false).unwrap(),
                base + 8
            );
            assert_eq!(
                translate(&memory, root, 0x9008, MemoryAccess::Read, true)
                    .unwrap_err()
                    .reason,
                PageFaultReason::SupervisorOnly
            );
            assert!(
                !address_space_physical_frames(&memory, root)
                    .unwrap()
                    .contains(&device)
            );
            for permissions in [PagePermissions::USER_DATA, PagePermissions::KERNEL_CODE] {
                assert_eq!(
                    manager.set_permissions(&mut memory, root, 0x9000, permissions),
                    Err(PageTableError::InvalidPhysicalFrame)
                );
            }
            manager
                .set_permissions(
                    &mut memory,
                    root,
                    0x9000,
                    PagePermissions {
                        writable: false,
                        user: false,
                        executable: false,
                    },
                )
                .unwrap();
            let mut adopted = PageMemoryManager::new(memory.len(), 0x2000).unwrap();
            adopted.adopt_address_space(&memory, root).unwrap();
            assert_eq!(adopted.used_frames(), manager.used_frames());
            manager.unmap_page(&mut memory, root, 0x9000).unwrap();
            assert_eq!(manager.used_frames(), used);
            manager
                .map_borrowed_page(
                    &mut memory,
                    root,
                    0x9000,
                    device,
                    PagePermissions::KERNEL_DATA,
                )
                .unwrap();
            manager.destroy_address_space(&mut memory, root).unwrap();
            assert_eq!(manager.used_frames(), 2);
        }
    }

    #[test]
    fn forged_user_executable_device_entries_cannot_override_bus_protection() {
        for base in [
            crate::machine::SYSTEM_BASE,
            crate::machine::INTERRUPT_BASE,
            crate::machine::FAULT_BASE,
            crate::machine::CLOCK_BASE,
            crate::machine::INPUT_BASE,
            crate::machine::CONSOLE_BASE,
            crate::machine::FIRMWARE_TEXT_BASE,
            crate::machine::AUDIO_BASE,
            crate::machine::NETWORK_BASE,
            crate::machine::FIRMWARE_GATEWAY_BASE,
        ] {
            let mut memory = mapped_memory(PagePermissions::USER_CODE, PagePermissions::USER_CODE);
            write_entry(&mut memory, 0x2000, base | 0xf);
            assert_eq!(
                translate(&memory, 0, 0, MemoryAccess::Read, false).unwrap(),
                base
            );
            for (user, access, reason) in [
                (true, MemoryAccess::Read, PageFaultReason::SupervisorOnly),
                (true, MemoryAccess::Write, PageFaultReason::SupervisorOnly),
                (true, MemoryAccess::Fetch, PageFaultReason::SupervisorOnly),
                (false, MemoryAccess::Fetch, PageFaultReason::NotExecutable),
            ] {
                assert_eq!(
                    translate(&memory, 0, 0, access, user).unwrap_err().reason,
                    reason
                );
            }
            assert_eq!(
                address_space_physical_frames(&memory, 0),
                Err(PageTableError::InvalidPhysicalFrame)
            );
        }
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
            PageFaultReason::AddressOutOfRange
        );
        assert!(encode_entry(1 << 18, PagePermissions::USER_DATA).is_none());
    }

    #[test]
    fn allocator_reuses_frames_and_page_helpers_manage_tables_and_permissions() {
        let mut memory = vec![0; 0x1_0000];
        let mut allocator = FrameAllocator::new(memory.len(), 0x5000).unwrap();
        assert_eq!(allocator.allocate(), Ok(5));
        allocator.free(5).unwrap();
        let allocated_table = map_page(
            &mut memory,
            0x2000,
            0x0040_0000,
            10,
            PagePermissions::USER_DATA,
            &mut allocator,
        )
        .unwrap();
        assert_eq!(allocated_table, Some(5));
        assert_eq!(allocator.used_frames(), 6);
        assert_eq!(
            translate(&memory, 0x2000, 0x0040_0034, MemoryAccess::Read, true),
            Ok(0xa034)
        );

        set_page_permissions(
            &mut memory,
            0x2000,
            0x0040_0000,
            PagePermissions {
                writable: false,
                user: true,
                executable: false,
            },
        )
        .unwrap();
        assert_eq!(
            translate(&memory, 0x2000, 0x0040_0000, MemoryAccess::Write, true)
                .unwrap_err()
                .reason,
            PageFaultReason::ReadOnly
        );

        assert_eq!(unmap_page(&mut memory, 0x2000, 0x0040_0000), Ok(Some(5)));
        allocator.free(5).unwrap();
        assert_eq!(allocator.used_frames(), 5);
        assert_eq!(allocator.allocate(), Ok(5));
    }

    #[test]
    fn memory_manager_isolates_address_spaces_and_reuses_zeroed_frames() {
        let mut memory = vec![0; 0x2_0000];
        let mut manager = PageMemoryManager::new(memory.len(), 0x4000).unwrap();
        let first = manager.create_address_space(&mut memory).unwrap();
        let second = manager.create_address_space(&mut memory).unwrap();
        let virtual_address = 0x0040_0000;
        let first_frame = manager
            .allocate_and_map_page(
                &mut memory,
                first,
                virtual_address,
                PagePermissions::USER_DATA,
            )
            .unwrap();
        let second_frame = manager
            .allocate_and_map_page(
                &mut memory,
                second,
                virtual_address,
                PagePermissions::USER_DATA,
            )
            .unwrap();
        assert_ne!(first_frame, second_frame);
        memory[first_frame as usize * PAGE_SIZE] = 0xa5;
        memory[second_frame as usize * PAGE_SIZE] = 0x5a;
        assert_eq!(
            translate(&memory, first, virtual_address, MemoryAccess::Read, true),
            Ok(first_frame * PAGE_SIZE as u32)
        );
        assert_eq!(
            translate(&memory, second, virtual_address, MemoryAccess::Read, true),
            Ok(second_frame * PAGE_SIZE as u32)
        );
        assert_eq!(
            translate(
                &memory,
                first,
                virtual_address + PAGE_SIZE as u32,
                MemoryAccess::Read,
                true
            )
            .unwrap_err()
            .reason,
            PageFaultReason::NotPresent
        );

        manager
            .unmap_page(&mut memory, first, virtual_address)
            .unwrap();
        let reused_frame = manager
            .allocate_and_map_page(
                &mut memory,
                first,
                virtual_address,
                PagePermissions::USER_DATA,
            )
            .unwrap();
        assert_eq!(reused_frame, first_frame);
        assert_eq!(memory[reused_frame as usize * PAGE_SIZE], 0);
        manager.destroy_address_space(&mut memory, first).unwrap();
        manager.destroy_address_space(&mut memory, second).unwrap();
        assert_eq!(manager.used_frames(), 4);
    }

    #[test]
    fn identity_mapping_is_supervisor_only_and_rejects_ranges_outside_ram() {
        let mut memory = vec![0; 0x1_0000];
        let mut manager = PageMemoryManager::new(memory.len(), 0x4000).unwrap();
        let directory = manager.create_address_space(&mut memory).unwrap();
        manager
            .identity_map_range(
                &mut memory,
                directory,
                0x8001,
                0x1fff,
                PagePermissions::KERNEL_DATA,
            )
            .unwrap();
        assert_eq!(
            translate(&memory, directory, 0x9000, MemoryAccess::Read, false),
            Ok(0x9000)
        );
        assert_eq!(
            translate(&memory, directory, 0x9000, MemoryAccess::Read, true)
                .unwrap_err()
                .reason,
            PageFaultReason::SupervisorOnly
        );
        manager.unmap_page(&mut memory, directory, 0x8000).unwrap();
        let virtual_frame = manager
            .allocate_and_map_page(&mut memory, directory, 0x1000, PagePermissions::USER_DATA)
            .unwrap();
        assert_eq!(
            translate(&memory, directory, 0x1000, MemoryAccess::Read, true),
            Ok(virtual_frame * PAGE_SIZE as u32)
        );
        assert_ne!(virtual_frame * PAGE_SIZE as u32, 0x1000);
        assert_eq!(
            manager.identity_map_range(
                &mut memory,
                directory,
                0xf000,
                0x2000,
                PagePermissions::KERNEL_DATA,
            ),
            Err(PageTableError::OutOfMemoryRange)
        );
    }

    #[test]
    fn page_helpers_reject_unaligned_virtual_pages_and_malformed_absent_entries() {
        let mut memory = vec![0; 0x8000];
        let mut allocator = FrameAllocator::new(memory.len(), 0x5000).unwrap();
        assert_eq!(
            map_page(
                &mut memory,
                0x2000,
                1,
                5,
                PagePermissions::USER_DATA,
                &mut allocator
            ),
            Err(PageTableError::InvalidVirtualAddress)
        );

        write_entry(&mut memory, 0x2000, 1 << 4);
        assert_eq!(
            map_page(
                &mut memory,
                0x2000,
                0,
                5,
                PagePermissions::USER_DATA,
                &mut allocator
            ),
            Err(PageTableError::MalformedEntry)
        );

        write_entry(
            &mut memory,
            0x2000,
            encode_entry(3, PTE_DIRECTORY_FLAGS).unwrap(),
        );
        write_entry(&mut memory, 0x3000, 1 << 4);
        assert_eq!(
            unmap_page(&mut memory, 0x2000, 0),
            Err(PageTableError::MalformedEntry)
        );
        assert_eq!(
            set_page_permissions(&mut memory, 0x2000, 0, PagePermissions::USER_DATA),
            Err(PageTableError::MalformedEntry)
        );
    }
}
use std::collections::{HashMap, HashSet};
