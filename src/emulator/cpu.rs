//! Implements SC-16 CPU execution and its connected device gateways.
//! Contains machine state, instruction decoding, memory access, privilege transitions,
//! interrupts, task/syscall integration, graphics operations, and behavioral tests.

use memmap2::MmapMut;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::bios_settings::{BiosSettingValue, BiosSettings, StartupBehavior};
use crate::console::ConsoleDevice;
use crate::disk::{FolderBootMode, FolderDisk};
use crate::guest_text::GuestTextDisplay;
use crate::net::{DEFAULT_PORT, NetNode};
use crate::sc_net::ScNetNode;
use crate::scheduler::{Scheduler, SchedulerError, TaskContext, TaskId, TaskState};
use crate::syscall::{KernelState, OpenFile, SyscallNumber};

pub const MEMORY_SIZE: usize = crate::machine::ADDRESS_SPACE_SIZE as usize;
pub const ADDRESS_MASK: u32 = (MEMORY_SIZE as u32) - 1;

const EXCEPTION_ILLEGAL_INSTRUCTION: u16 = 0;
const EXCEPTION_PRIVILEGE_VIOLATION: u16 = 1;
const EXCEPTION_PAGE_FAULT: u16 = 2;
const EXCEPTION_ENTRY_BYTES: u32 = 16;
pub const BIOS_FIRMWARE_MARKER: u32 = 0x4fffe;
pub const BIOS_AUTOBOOT_WAIT_MARKER: u32 = 0x4ffff;
const FOLDER_FS_LIST: u8 = 0x41;
const FOLDER_FS_READ: u8 = 0x42;
const FOLDER_FS_WRITE: u8 = 0x43;
const FOLDER_FS_CREATE_DIR: u8 = 0x44;
const FOLDER_FS_DELETE: u8 = 0x45;
const FOLDER_FS_RENAME: u8 = 0x46;
const FOLDER_FS_STAT: u8 = 0x47;
const VRAM_BYTES: usize = 640 * 480 / 2;
#[cfg(test)]
const FORMER_FRAMEBUFFER_START: u32 = 0x50000;
const FONT_ADDR: usize = 0x4400;
static FONT_DATA: OnceLock<Vec<u8>> = OnceLock::new();
const OP_HALT: u16 = 0x00;
const OP_LDI: u16 = 0x01;
const OP_LD: u16 = 0x02;
const OP_ST: u16 = 0x03;
const OP_MOV: u16 = 0x04;
const OP_ADD: u16 = 0x05;
const OP_SUB: u16 = 0x06;
const OP_AND: u16 = 0x07;
const OP_OR: u16 = 0x08;
const OP_XOR: u16 = 0x09;
const OP_NOT: u16 = 0x0a;
const OP_JMP: u16 = 0x0b;
const OP_JZ: u16 = 0x0c;
const OP_INC: u16 = 0x0d;
const OP_DEC: u16 = 0x0e;
const OP_CMP: u16 = 0x0f;
const OP_LDI_H: u16 = 0x10;
const OP_EI: u16 = 0x11;
const OP_DI: u16 = 0x12;
const OP_RTI: u16 = 0x13;
const OP_PUSH: u16 = 0x14;
const OP_POP: u16 = 0x15;
const OP_CALL: u16 = 0x16;
const OP_RET: u16 = 0x17;
const OP_BZ: u16 = 0x18;
const OP_LDW: u16 = 0x19;
const OP_STW: u16 = 0x1a;
const OP_LDWS: u16 = 0x1b;
const OP_STWS: u16 = 0x1c;
const OP_ADJSP: u16 = 0x1d;
const OP_BC: u16 = 0x1e;
const OP_SHR: u16 = 0x1f;
const FAR_CONTROL_MASK: u16 = 0xf83f;
const FAR_CONTROL_PREFIX: u16 = 0xf83e;
const FAR_BRANCH_MASK: u16 = 0xf80f;
const FAR_BRANCH_PREFIX: u16 = 0xf80d;
const FLOAT_MASK: u16 = 0xf81f;
const FLOAT_PREFIX: u16 = 0xf81b;
const MULDIV_MASK: u16 = 0xf8ff;
const MULDIV_PREFIX: u16 = 0xf81e;
const WIDE_MASK: u16 = 0xf81f;
const WIDE_PREFIX: u16 = 0xf81a;
const ALU_MASK: u16 = 0xf8ff;
const ALU_PREFIX: u16 = 0xf81f;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub zero: bool,
    pub carry: bool,
    pub negative: bool,
    pub overflow: bool,
}

impl Cpu {
    fn high_graphics_command(&mut self, command: u8) {
        use crate::machine::*;

        if command != 1 {
            self.graphics_device.fail(GRAPHICS_STATUS_INVALID_COMMAND);
            return;
        }

        let staged = self.graphics_device.request();
        let data = *staged.data;
        let request = crate::machine::GraphicsRequest {
            operation: staged.operation,
            color: staged.color,
            x: staged.x,
            y: staged.y,
            width: staged.width,
            height: staged.height,
            length: staged.length,
            mode: staged.mode,
            data: &data,
        };
        match self.execute_graphics_request(request) {
            Ok(result) => self.graphics_device.complete(result),
            Err(()) => self.graphics_device.fail(GRAPHICS_STATUS_INVALID_REQUEST),
        }
    }

    fn execute_graphics_request(
        &mut self,
        request: crate::machine::GraphicsRequest<'_>,
    ) -> Result<u16, ()> {
        use crate::machine::*;

        let GraphicsRequest {
            operation,
            color,
            x: raw_x,
            y: raw_y,
            width: raw_width,
            height: raw_height,
            length,
            mode,
            data,
        } = request;
        let (screen_width, screen_height) = self.screen_size();
        let x = i32::from(raw_x);
        let y = i32::from(raw_y);
        let width = i32::from(raw_width);
        let height = i32::from(raw_height);
        let pixel_count = usize::from(raw_width).checked_mul(usize::from(raw_height));
        let result = match operation {
            GRAPHICS_OP_CLEAR if length == 0 => {
                let pixel_count = (screen_width * screen_height) as usize;
                if screen_width == 320 {
                    self.vram[..pixel_count].fill(color);
                } else {
                    let fill = ((color & 0x0f) << 4) | (color & 0x0f);
                    self.vram[..pixel_count / 2].fill(fill);
                }
                Some(0)
            }
            GRAPHICS_OP_PIXEL if length == 0 && x < screen_width && y < screen_height => {
                self.graphics_pixel(x, y, color);
                Some(1)
            }
            GRAPHICS_OP_RECT
                if length == 0
                    && width > 0
                    && height > 0
                    && x.checked_add(width).is_some_and(|end| end <= screen_width)
                    && y.checked_add(height)
                        .is_some_and(|end| end <= screen_height) =>
            {
                for row in y..y + height {
                    for column in x..x + width {
                        self.graphics_pixel(column, row, color);
                    }
                }
                Some(u16::try_from(width * height).unwrap_or(u16::MAX))
            }
            GRAPHICS_OP_LINE
                if length == 0
                    && x < screen_width
                    && y < screen_height
                    && width < screen_width
                    && height < screen_height =>
            {
                let (mut x0, mut y0) = (x, y);
                let dx = (width - x0).abs();
                let sx = if x0 < width { 1 } else { -1 };
                let dy = -(height - y0).abs();
                let sy = if y0 < height { 1 } else { -1 };
                let mut error = dx + dy;
                let mut points = 0u16;
                loop {
                    self.graphics_pixel(x0, y0, color);
                    points = points.saturating_add(1);
                    if x0 == width && y0 == height {
                        break;
                    }
                    let twice = 2 * error;
                    if twice >= dy {
                        error += dy;
                        x0 += sx;
                    }
                    if twice <= dx {
                        error += dx;
                        y0 += sy;
                    }
                }
                Some(points)
            }
            GRAPHICS_OP_PRESENT if length == 0 => {
                self.frame_yield = true;
                self.frame_presented = true;
                Some(self.frame_count)
            }
            GRAPHICS_OP_BLIT
                if width > 0
                    && height > 0
                    && pixel_count == Some(usize::from(length))
                    && usize::from(length) <= GRAPHICS_DATA_SIZE
                    && x.checked_add(width).is_some_and(|end| end <= screen_width)
                    && y.checked_add(height)
                        .is_some_and(|end| end <= screen_height) =>
            {
                for row in 0..height {
                    for column in 0..width {
                        let source = (row * width + column) as usize;
                        self.graphics_pixel(x + column, y + row, data[source]);
                    }
                }
                Some(length)
            }
            GRAPHICS_OP_PALETTE if length == 3 => {
                self.palette[color as usize] = [data[0], data[1], data[2]];
                Some(0)
            }
            GRAPHICS_OP_MODE if length == 0 && mode <= 1 => {
                self.set_graphics_mode(mode).ok().map(|()| 0)
            }
            _ => None,
        };
        result.ok_or(())
    }

    fn high_filesystem_command(&mut self, command: u8) {
        use crate::machine::*;

        if command != 1 {
            self.filesystem_device.fail(1);
            return;
        }
        let operation = self.filesystem_device.operation;
        if !(FOLDER_FS_LIST..=FOLDER_FS_STAT).contains(&operation) {
            self.filesystem_device.fail(1);
            return;
        }
        if matches!(operation, FOLDER_FS_READ | FOLDER_FS_WRITE)
            && usize::from(self.filesystem_device.length) > FILESYSTEM_DATA_SIZE
        {
            self.filesystem_device.fail(2);
            return;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            self.filesystem_device.fail(3);
            return;
        };
        let path = match Self::decode_filesystem_path(&self.filesystem_device.path) {
            Ok(path) => path,
            Err(error) => {
                self.filesystem_device.fail(2);
                self.debug_console.log_error("filesystem", &error);
                return;
            }
        };
        let output = match operation {
            FOLDER_FS_LIST => {
                let directory = if path.is_empty() {
                    None
                } else {
                    Some(path.as_str())
                };
                disk.list_dir(directory).and_then(|entries| {
                    let result = u16::try_from(entries.len()).unwrap_or(u16::MAX);
                    let entry = entries.get(usize::from(self.filesystem_device.index));
                    let encoded = if let Some(entry) = entry {
                        let size = u32::try_from(entry.size_bytes)
                            .map_err(|_| "filesystem entry exceeds 32-bit size".to_owned())?;
                        let mut bytes = entry.name.as_bytes().to_vec();
                        bytes.push(0);
                        bytes.push(u8::from(entry.is_directory));
                        bytes.extend_from_slice(&size.to_be_bytes());
                        bytes
                    } else {
                        vec![0]
                    };
                    Ok((result, 0, encoded))
                })
            }
            FOLDER_FS_READ => {
                if path.is_empty() {
                    Err("filesystem path is empty".to_owned())
                } else {
                    disk.read_file(
                        &path,
                        u64::from(self.filesystem_device.offset),
                        usize::from(self.filesystem_device.length),
                    )
                    .and_then(|(data, file_size)| {
                        let size = u32::try_from(file_size)
                            .map_err(|_| "filesystem file exceeds 32-bit size".to_owned())?;
                        let result = u16::try_from(data.len()).unwrap_or(u16::MAX);
                        Ok((result, size, data))
                    })
                }
            }
            FOLDER_FS_WRITE => {
                let length = usize::from(self.filesystem_device.length);
                if path.is_empty() {
                    Err("filesystem path is empty".to_owned())
                } else {
                    Self::validate_guest_mutation_path(&path)
                        .and_then(|()| {
                            disk.write_file(&path, &self.filesystem_device.data[..length])
                        })
                        .map(|()| (length as u16, length as u32, Vec::new()))
                }
            }
            FOLDER_FS_CREATE_DIR => {
                if path.is_empty() {
                    Err("filesystem path is empty".to_owned())
                } else {
                    Self::validate_guest_mutation_path(&path)
                        .and_then(|()| disk.create_dir(&path))
                        .map(|()| (0, 0, Vec::new()))
                }
            }
            FOLDER_FS_DELETE => {
                if path.is_empty() {
                    Err("filesystem path is empty".to_owned())
                } else {
                    Self::validate_guest_mutation_path(&path)
                        .and_then(|()| disk.delete_entry(&path))
                        .map(|()| (0, 0, Vec::new()))
                }
            }
            FOLDER_FS_RENAME => {
                let destination = Self::decode_filesystem_path(&self.filesystem_device.destination);
                match destination {
                    Ok(destination) if !path.is_empty() && !destination.is_empty() => {
                        Self::validate_guest_mutation_path(&path)
                            .and_then(|()| Self::validate_guest_mutation_path(&destination))
                            .and_then(|()| disk.rename_entry(&path, &destination))
                            .map(|()| (0, 0, Vec::new()))
                    }
                    Ok(_) => Err("filesystem rename path is empty".to_owned()),
                    Err(error) => Err(error),
                }
            }
            FOLDER_FS_STAT => {
                if path.is_empty() {
                    Err("filesystem path is empty".to_owned())
                } else {
                    disk.metadata(&path).and_then(|entry| {
                        let size = u32::try_from(entry.size_bytes)
                            .map_err(|_| "filesystem entry exceeds 32-bit size".to_owned())?;
                        let mut data = vec![u8::from(entry.is_directory)];
                        data.extend_from_slice(&size.to_be_bytes());
                        Ok((5, size, data))
                    })
                }
            }
            _ => Err("unsupported filesystem operation".to_owned()),
        };
        match output {
            Ok((result, file_size, data)) => {
                if data.len() > FILESYSTEM_DATA_SIZE {
                    self.filesystem_device.fail(2);
                    self.debug_console.log_error(
                        "filesystem",
                        "directory entry exceeds the inline response size",
                    );
                } else if let Err(error) = self.filesystem_device.complete(result, file_size, &data)
                {
                    self.debug_console.log_error("filesystem", &error);
                }
            }
            Err(error) => {
                self.filesystem_device.fail(4);
                self.debug_console
                    .log_error("filesystem", &format!("operation failed: {error}"));
            }
        }
    }

    fn decode_filesystem_path(bytes: &[u8; 128]) -> Result<String, String> {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "filesystem path must be NUL-terminated within 128 bytes".to_owned())?;
        if bytes[end..].iter().any(|byte| *byte != 0) {
            return Err("filesystem path has data after its terminator".to_owned());
        }
        let path = std::str::from_utf8(&bytes[..end])
            .map_err(|_| "filesystem path is not valid UTF-8".to_owned())?;
        if !path.is_ascii() {
            return Err("filesystem paths must be ASCII".to_owned());
        }
        Ok(path.to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeMode {
    Supervisor,
    User,
}

fn branch_condition(flags: Flags, condition: u8) -> bool {
    match condition {
        0 => !flags.zero,
        1 => !flags.carry,
        2 => flags.negative,
        3 => !flags.negative,
        4 => flags.overflow,
        5 => !flags.overflow,
        6 => flags.negative != flags.overflow,
        7 => flags.negative == flags.overflow,
        8 => !flags.zero && flags.negative == flags.overflow,
        9 => flags.zero || flags.negative != flags.overflow,
        10 => !flags.carry && !flags.zero,
        11 => flags.carry || flags.zero,
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySegment {
    pub address: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedSegmentMetadata {
    pub address: u32,
    pub length: usize,
    pub alignment: u32,
    pub permissions: crate::image::SegmentPermissions,
}

pub trait LoadableSegment {
    fn address(&self) -> u32;
    fn data(&self) -> &[u8];
    fn image_metadata(&self) -> Option<(u32, crate::image::SegmentPermissions)> {
        None
    }
}

impl LoadableSegment for MemorySegment {
    fn address(&self) -> u32 {
        self.address
    }

    fn data(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CpuError {
    InvalidMemorySize(usize),
    MemoryMap(String),
    SegmentOutOfBounds { address: u32, length: usize },
    InvalidEntryPoint(u32),
    InvalidUserStack(u32),
    InvalidRegisterPair(u16),
    UnsupportedInstruction(u16),
    PageFault(crate::mmu::PageFault),
    Idt(crate::interrupts::IdtError),
    Scheduler(SchedulerError),
    PrivilegeViolation { pc: u32, operation: &'static str },
    UnhandledException { vector: u16, cause: u16 },
    DoubleFault,
}

impl std::fmt::Display for CpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMemorySize(size) => {
                write!(f, "memory size must be a power of two, got {size}")
            }
            Self::MemoryMap(error) => write!(f, "cannot allocate SC-16 memory: {error}"),
            Self::SegmentOutOfBounds { address, length } => {
                write!(
                    f,
                    "program segment at {address:#x} with length {length} is out of bounds"
                )
            }
            Self::InvalidEntryPoint(address) => {
                write!(f, "entry point {address:#x} is outside memory")
            }
            Self::InvalidUserStack(address) => {
                write!(
                    f,
                    "user stack pointer {address:#x} is invalid or overlaps the kernel stack"
                )
            }
            Self::InvalidRegisterPair(instruction) => {
                write!(f, "invalid register pair in instruction {instruction:#06x}")
            }
            Self::UnsupportedInstruction(instruction) => {
                write!(f, "unsupported instruction {instruction:#06x}")
            }
            Self::PageFault(fault) => write!(
                f,
                "page fault {:?} at {:#x} during {:?}",
                fault.reason, fault.virtual_address, fault.access
            ),
            Self::Idt(error) => write!(f, "invalid interrupt descriptor table: {error:?}"),
            Self::Scheduler(error) => write!(f, "scheduler error: {error:?}"),
            Self::PrivilegeViolation { pc, operation } => {
                write!(f, "user-mode {operation} at {pc:#x} is privileged")
            }
            Self::UnhandledException { vector, cause } => {
                write!(f, "unhandled exception {cause} at vector {vector}")
            }
            Self::DoubleFault => f.write_str("CPU stopped after a double fault"),
        }
    }
}

impl std::error::Error for CpuError {}

pub struct Cpu {
    monotonic_epoch: std::time::Instant,
    #[cfg(test)]
    pub(crate) test_monotonic_ms: Option<u32>,
    pub memory: MmapMut,
    pub(crate) vram: Vec<u8>,
    pub registers: [i64; 8],
    pub pc: u32,
    pub sp: u32,
    pub kernel_sp: u32,
    pub loaded_image_segments: Vec<LoadedSegmentMetadata>,
    pub privilege_mode: PrivilegeMode,
    pub flags: Flags,
    pub running: bool,
    pub frame_yield: bool,
    pub frame_presented: bool,
    pub frame_count: u16,
    pub console: ConsoleDevice,
    pub debug_console: ConsoleDevice,
    pub terminal_active: bool,
    pub terminal_dirty: bool,
    pub guest_text: GuestTextDisplay,

    pub random_state: u32,
    pub palette: [[u8; 3]; 256],
    audio: crate::machine::AudioDevice,
    disk_root: PathBuf,
    bios_preselected_disk: Option<String>,
    bios_settings_path: Option<PathBuf>,
    bios_settings: BiosSettings,
    booted_disk_path: Option<PathBuf>,
    program_folder_disk: Option<FolderDisk>,
    folder_disk: Option<FolderDisk>,
    network: Option<NetNode>,
    network_registers: crate::machine::NetworkDevice,
    firmware_gateway: crate::machine::FirmwareGateway,
    filesystem_device: crate::machine::FilesystemDevice,
    graphics_device: crate::machine::GraphicsDevice,
    network_port: u16,
    scnet: Option<ScNetNode>,
    scnet_enabled: bool,
    scnet_port: u16,
    scnet_name: Option<String>,
    input: crate::machine::InputDevice,
    memory_mask: u32,
    graphics_mode: u8,
    interrupts: crate::machine::InterruptController,
    clock: crate::machine::ClockDevice,
    idt_base: Option<u32>,
    mmu_enabled: bool,
    page_directory: u32,
    last_page_fault: Option<crate::mmu::PageFault>,
    scheduler: Scheduler,
    syscall_state: KernelState,
    page_memory_manager: Option<crate::mmu::PageMemoryManager>,
}

impl Cpu {
    pub fn new() -> Self {
        Self::with_memory_size(MEMORY_SIZE).expect("the SC-16 memory size is valid")
    }

    pub fn with_memory_size(size: usize) -> Result<Self, CpuError> {
        if !size.is_power_of_two() || !(8..=MEMORY_SIZE).contains(&size) {
            return Err(CpuError::InvalidMemorySize(size));
        }
        let memory_mask = (size - 1) as u32;
        let memory =
            MmapMut::map_anon(size).map_err(|error| CpuError::MemoryMap(error.to_string()))?;
        let mut memory = memory;
        let font = FONT_DATA.get_or_init(|| {
            let image =
                crate::assembler::assemble(include_str!("../../firmware/fonts/font5x7.asm"), 0)
                    .expect("embedded bitmap font assembles");
            image
                .segments
                .into_iter()
                .find(|segment| segment.address == FONT_ADDR as u32)
                .expect("embedded bitmap font segment exists")
                .data
        });
        if size > FONT_ADDR {
            let font_size = font.len().min(size - FONT_ADDR);
            memory[FONT_ADDR..FONT_ADDR + font_size].copy_from_slice(&font[..font_size]);
        }
        let mut palette = [[0; 3]; 256];
        for (index, color) in palette.iter_mut().enumerate() {
            *color = [index as u8; 3];
        }
        const PALETTE_COLORS: [[u8; 3]; 15] = [
            [0, 0, 0],
            [255, 255, 255],
            [255, 48, 48],
            [48, 220, 64],
            [64, 96, 255],
            [255, 224, 0],
            [0, 224, 224],
            [224, 64, 224],
            [255, 144, 0],
            [150, 150, 150],
            [80, 80, 80],
            [128, 0, 0],
            [255, 128, 192],
            [150, 100, 50],
            [46, 30, 22],
        ];
        palette[..PALETTE_COLORS.len()].copy_from_slice(&PALETTE_COLORS);
        palette[25] = [0, 0, 255];
        Ok(Self {
            memory,
            vram: vec![0; VRAM_BYTES],
            registers: [0; 8],
            monotonic_epoch: std::time::Instant::now(),
            #[cfg(test)]
            test_monotonic_ms: None,
            pc: 0,
            sp: crate::machine::usable_ram_size(size) as u32 - 1,
            kernel_sp: crate::machine::usable_ram_size(size) as u32 - 1,
            loaded_image_segments: Vec::new(),
            privilege_mode: PrivilegeMode::Supervisor,
            flags: Flags::default(),
            running: true,
            frame_yield: false,
            frame_presented: false,
            frame_count: 0,
            console: ConsoleDevice::new(),
            debug_console: ConsoleDevice::new(),
            terminal_active: false,
            terminal_dirty: false,
            guest_text: GuestTextDisplay::new(),

            random_state: 0x16c0_ffee,
            palette,
            audio: crate::machine::AudioDevice::default(),
            disk_root: PathBuf::from("disks"),
            bios_preselected_disk: None,
            bios_settings_path: None,
            bios_settings: BiosSettings::default(),
            booted_disk_path: None,
            program_folder_disk: None,
            folder_disk: None,
            network: None,
            network_registers: crate::machine::NetworkDevice::default(),
            firmware_gateway: crate::machine::FirmwareGateway::default(),
            filesystem_device: crate::machine::FilesystemDevice::default(),
            graphics_device: crate::machine::GraphicsDevice::default(),
            network_port: DEFAULT_PORT,
            scnet: None,
            scnet_enabled: false,
            scnet_port: crate::sc_net::DEFAULT_PORT,
            scnet_name: None,
            input: crate::machine::InputDevice::default(),
            memory_mask,
            graphics_mode: 0,
            interrupts: crate::machine::InterruptController::default(),
            clock: crate::machine::ClockDevice::default(),
            idt_base: None,
            mmu_enabled: false,
            page_directory: 0,
            last_page_fault: None,
            scheduler: Scheduler::default(),
            syscall_state: KernelState::default(),
            page_memory_manager: None,
        })
    }

    pub fn set_bios_preselected_disk(&mut self, disk_id: Option<&str>) {
        self.bios_preselected_disk = disk_id.map(str::to_owned);
    }

    pub fn configure_bios_settings(&mut self, path: PathBuf) -> Result<(), String> {
        self.bios_settings = BiosSettings::load(&path)?;
        self.bios_settings_path = Some(path);
        Ok(())
    }

    pub fn bios_settings(&self) -> &BiosSettings {
        &self.bios_settings
    }

    pub fn take_booted_disk_path(&mut self) -> Option<PathBuf> {
        self.booted_disk_path.take()
    }

    fn firmware_operation(
        &mut self,
        command: u8,
        request: &mut [u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<Option<String>, String> {
        let index = usize::from(u16::from_be_bytes([request[2], request[3]]));
        match command {
            1 => self.bios_list_count(request)?,
            2 => return self.bios_list_id(index).map(Some),
            3 => self.bios_boot_disk(index)?,
            4 => self.bios_save_settings(request)?,
            5 => self.bios_reload_settings(request)?,
            6 => self.bios_read_setting(request)?,
            7 => self.bios_write_setting(request)?,
            8 => return self.bios_disk_info(index).map(Some),
            9 => self.bios_save_preferred_disk(index)?,
            _ => return Err("unsupported firmware operation".to_owned()),
        }
        Ok(None)
    }

    fn high_firmware_command(&mut self, command: u8) {
        let mut request = self.firmware_gateway.request();
        let result = self
            .firmware_operation(command, &mut request)
            .and_then(|output| self.firmware_gateway.complete(request, output.as_deref()));
        if let Err(error) = result {
            self.firmware_gateway.fail();
            self.debug_console.log_error("bios", &error);
        }
    }

    fn bios_disk_paths(&self) -> Result<Vec<PathBuf>, String> {
        FolderDisk::discover_paths(&self.disk_root)
    }

    fn bios_list_count(
        &mut self,
        request: &mut [u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<(), String> {
        if self.scnet_enabled {
            self.ensure_scnet_node(std::time::Duration::from_millis(650))?;
        }
        let paths = self.bios_disk_paths()?;
        let count = u16::try_from(paths.len())
            .map_err(|_| "firmware disk list exceeds 65535 entries".to_owned())?;
        let preselected = self.bios_preselected_disk.take();
        let find_disk = |id: &str| {
            paths.iter().position(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == id)
            })
        };
        let selected_by_cli = preselected.as_deref().and_then(find_disk);
        if let Some(id) = preselected.as_deref().filter(|_| selected_by_cli.is_none()) {
            self.debug_console
                .log_error("bios", &format!("preselected disk {id} was not found"));
        }
        let selected_by_settings = self
            .bios_settings
            .preferred_disk
            .as_deref()
            .and_then(find_disk);
        if let Some(id) = self
            .bios_settings
            .preferred_disk
            .as_deref()
            .filter(|_| selected_by_settings.is_none())
        {
            self.debug_console
                .log_warning("bios", &format!("preferred disk {id} was not found"));
        }
        let selected = if preselected.is_some() {
            selected_by_cli.unwrap_or(0)
        } else {
            selected_by_settings.unwrap_or(0)
        };
        let auto_boot = self.bios_settings.startup_behavior == StartupBehavior::BootPreferred
            && if preselected.is_some() {
                selected_by_cli.is_some()
            } else {
                selected_by_settings.is_some()
            };
        let [index_high, index_low] = u16::try_from(selected)
            .map_err(|_| "selected firmware disk index exceeds 65535".to_owned())?
            .to_be_bytes();
        let [count_high, count_low] = count.to_be_bytes();
        request[2] = index_high;
        request[3] = index_low;
        request[4] = count_high;
        request[5] = count_low;
        request[6] = u8::from(auto_boot);
        request[0x0e] = u8::try_from(self.bios_settings.additional.len())
            .map_err(|_| "too many additional BIOS settings".to_owned())?;
        request[0x10..0x21].fill(0);
        let network_name = self.bios_settings.network_name.as_bytes();
        request[0x10..0x10 + network_name.len()].copy_from_slice(network_name);
        Ok(())
    }

    fn bios_list_id(&self, index: usize) -> Result<String, String> {
        let paths = self.bios_disk_paths()?;
        let path = paths
            .get(index)
            .ok_or_else(|| format!("firmware disk index {index} is out of range"))?;
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "firmware disk ID is not valid text".to_owned())?;
        Ok(id.to_owned())
    }

    fn bios_disk_info(&self, index: usize) -> Result<String, String> {
        let paths = self.bios_disk_paths()?;
        let path = paths
            .get(index)
            .ok_or_else(|| format!("firmware disk index {index} is out of range"))?;
        let disk = FolderDisk::open(path)?;
        let boot_mode = match disk.load_boot_image() {
            Ok(boot) => match boot.mode {
                FolderBootMode::Kernel => "KERNEL",
                FolderBootMode::User => "USER",
            },
            Err(error) if error.starts_with("NOT_BOOT:") => "NOT_BOOT",
            Err(error) => return Err(error),
        };
        let label = disk
            .manifest()
            .label
            .chars()
            .map(|character| {
                if character.is_ascii_graphic() || character == ' ' {
                    character
                } else {
                    '?'
                }
            })
            .collect::<String>();
        let summary = format!(
            "{label} | {} B | {} | {boot_mode}",
            disk.manifest().capacity_bytes,
            if disk.manifest().read_only {
                "READ-ONLY"
            } else {
                "READ-WRITE"
            }
        );
        Ok(summary)
    }

    fn bios_boot_disk(&mut self, index: usize) -> Result<(), String> {
        if self.scnet_enabled {
            self.ensure_scnet_node(std::time::Duration::from_millis(650))?;
        }
        let paths = self.bios_disk_paths()?;
        let path = paths
            .get(index)
            .ok_or_else(|| format!("firmware disk index {index} is out of range"))?;
        let disk = FolderDisk::open(path)?;
        let disk_id = disk.manifest().id.clone();
        let disk_label = disk.manifest().label.clone();
        let boot = disk.load_boot_image()?;
        self.validate_program(&boot.image.segments, boot.image.entry_point)
            .map_err(|error| format!("cannot load selected firmware disk: {error}"))?;
        self.prepare_image_entry();
        self.load_program(&boot.image.segments, boot.image.entry_point)
            .map_err(|error| format!("cannot load selected firmware disk: {error}"))?;
        if let Some(marker) = self.memory.get_mut(BIOS_FIRMWARE_MARKER as usize) {
            *marker = 0;
        }
        self.booted_disk_path = Some(path.clone());
        self.folder_disk = Some(disk);
        match boot.mode {
            FolderBootMode::Kernel => self.privilege_mode = PrivilegeMode::Supervisor,
            FolderBootMode::User => {
                let user_stack = self.initial_user_stack_pointer();
                self.enter_user_mode(self.pc, user_stack)
                    .map_err(|error| format!("cannot enter user mode: {error}"))?;
            }
        }
        self.guest_text.clear(0);
        self.guest_text.enabled = false;
        self.console.clear_input();
        self.terminal_active = true;
        let program = match boot.mode {
            FolderBootMode::Kernel => "kernel.bin",
            FolderBootMode::User => "main.bin",
        };
        self.debug_console.log_info(
            "bios",
            &format!(
                "Loaded boot device path={} id={} label={:?} program={} mode={:?} entry={:#06x} segments={}",
                path.display(),
                disk_id,
                disk_label,
                program,
                boot.mode,
                self.pc,
                boot.image.segments.len(),
            ),
        );
        Ok(())
    }

    fn bios_save_settings(
        &mut self,
        request: &[u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<(), String> {
        let path = self
            .bios_settings_path
            .as_ref()
            .ok_or_else(|| "BIOS settings file path is not configured".to_owned())?;
        let paths = self.bios_disk_paths()?;
        let selected = usize::from(u16::from_be_bytes([request[2], request[3]]));
        if selected >= paths.len() && !paths.is_empty() {
            return Err(format!("firmware disk index {selected} is out of range"));
        }
        let preferred_disk = paths
            .get(selected)
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .ok_or_else(|| "selected disk ID is not valid text".to_owned())
            })
            .transpose()?;
        let startup_behavior = match request[6] {
            0 => StartupBehavior::Selector,
            1 => StartupBehavior::BootPreferred,
            _ => return Err("BIOS startup behavior value is invalid".to_owned()),
        };
        let mut network_name = Vec::with_capacity(16);
        for byte in &request[0x10..0x20] {
            if *byte == 0 {
                break;
            }
            network_name.push(*byte);
        }
        if !network_name.is_ascii() {
            return Err("BIOS network name must be ASCII".to_owned());
        }
        let network_name = String::from_utf8(network_name)
            .map_err(|_| "BIOS network name is not valid UTF-8".to_owned())?;
        let mut settings = self.bios_settings.clone();
        settings.preferred_disk = preferred_disk;
        settings.startup_behavior = startup_behavior;
        settings.set_network_name(network_name)?;
        settings.save(path)?;
        self.bios_settings = settings;
        Ok(())
    }

    fn bios_save_preferred_disk(&mut self, selected: usize) -> Result<(), String> {
        let path = self
            .bios_settings_path
            .as_ref()
            .ok_or_else(|| "BIOS settings file path is not configured".to_owned())?;
        let paths = self.bios_disk_paths()?;
        let disk_path = paths
            .get(selected)
            .ok_or_else(|| format!("firmware disk index {selected} is out of range"))?;
        let preferred_disk = disk_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "selected disk ID is not valid text".to_owned())?
            .to_owned();
        let mut settings = self.bios_settings.clone();
        settings.preferred_disk = Some(preferred_disk);
        settings.save(path)?;
        self.bios_settings = settings;
        Ok(())
    }

    fn bios_reload_settings(
        &mut self,
        request: &mut [u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<(), String> {
        let path = self
            .bios_settings_path
            .as_ref()
            .ok_or_else(|| "BIOS settings file path is not configured".to_owned())?;
        self.bios_settings = BiosSettings::load(path)?;
        self.bios_list_count(request)
    }

    fn bios_read_setting(
        &self,
        request: &mut [u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<(), String> {
        let index = usize::from(u16::from_be_bytes([request[2], request[3]]));
        let (name, value) = self
            .bios_settings
            .additional
            .iter()
            .nth(index)
            .ok_or_else(|| format!("additional BIOS setting index {index} is out of range"))?;
        let record = 0x30;
        request[record..record + 0x2a].fill(0);
        request[record..record + name.len()].copy_from_slice(name.as_bytes());
        request[record + 17] = match value {
            BiosSettingValue::Boolean(_) => 0,
            BiosSettingValue::String(_) => 1,
            BiosSettingValue::Integer(_) => 2,
        };
        let encoded_value = match value {
            BiosSettingValue::Boolean(value) => {
                if *value {
                    "Yes".to_owned()
                } else {
                    "No".to_owned()
                }
            }
            BiosSettingValue::String(value) => value.clone(),
            BiosSettingValue::Integer(value) => value.to_string(),
        };
        request[record + 18..record + 18 + encoded_value.len()]
            .copy_from_slice(encoded_value.as_bytes());
        Ok(())
    }

    fn bios_write_setting(
        &mut self,
        request: &[u8; crate::machine::FIRMWARE_REQUEST_BYTES],
    ) -> Result<(), String> {
        let record = 0x30;
        let name_bytes = &request[record..record + 17];
        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "additional BIOS setting name is not terminated".to_owned())?;
        let name = std::str::from_utf8(&name_bytes[..name_end])
            .map_err(|_| "additional BIOS setting name is not valid UTF-8".to_owned())?;
        let value_bytes = &request[record + 18..record + 0x2a];
        let value_end = value_bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "additional BIOS setting value is not terminated".to_owned())?;
        let value = std::str::from_utf8(&value_bytes[..value_end])
            .map_err(|_| "additional BIOS setting value is not valid UTF-8".to_owned())?;
        let existing = self
            .bios_settings
            .additional
            .get(name)
            .ok_or_else(|| format!("additional BIOS setting {name:?} was not found"))?;
        let new_value = match (request[record + 17], existing) {
            (0, BiosSettingValue::Boolean(_)) => match value {
                "Yes" => BiosSettingValue::Boolean(true),
                "No" => BiosSettingValue::Boolean(false),
                _ => return Err("boolean BIOS setting must be Yes or No".to_owned()),
            },
            (1, BiosSettingValue::String(_)) => BiosSettingValue::String(value.to_owned()),
            (2, BiosSettingValue::Integer(_)) => BiosSettingValue::Integer(
                value
                    .parse()
                    .map_err(|_| "number BIOS setting must be a signed integer".to_owned())?,
            ),
            _ => return Err("additional BIOS setting type cannot be changed".to_owned()),
        };
        let mut settings = self.bios_settings.clone();
        settings.additional.insert(name.to_owned(), new_value);
        settings.validate()?;
        self.bios_settings = settings;
        Ok(())
    }

    pub fn validate_program<S: LoadableSegment>(
        &self,
        segments: &[S],
        entry_point: u32,
    ) -> Result<(), CpuError> {
        let ram_size = crate::machine::usable_ram_size(self.memory.len());
        if entry_point as usize >= ram_size {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        for segment in segments {
            let address = segment.address();
            let data = segment.data();
            let start = address as usize;
            let Some(end) = start.checked_add(data.len()) else {
                return Err(CpuError::SegmentOutOfBounds {
                    address,
                    length: data.len(),
                });
            };
            if end > ram_size {
                return Err(CpuError::SegmentOutOfBounds {
                    address,
                    length: data.len(),
                });
            }
        }
        Ok(())
    }

    pub fn load_program<S: LoadableSegment>(
        &mut self,
        segments: &[S],
        entry_point: u32,
    ) -> Result<(), CpuError> {
        self.validate_program(segments, entry_point)?;
        let mut loaded_image_segments = Vec::new();
        for segment in segments {
            if let Some((alignment, permissions)) = segment.image_metadata() {
                loaded_image_segments.push(LoadedSegmentMetadata {
                    address: segment.address(),
                    length: segment.data().len(),
                    alignment,
                    permissions,
                });
            }
        }
        for segment in segments {
            let start = segment.address() as usize;
            let end = start + segment.data().len();
            self.memory[start..end].copy_from_slice(segment.data());
        }
        self.pc = entry_point;
        self.loaded_image_segments = loaded_image_segments;
        self.debug_console
            .log_debug("cpu", &format!("Program image loaded at {entry_point:#x}"));
        Ok(())
    }

    pub fn prepare_image_entry(&mut self) {
        self.debug_console
            .log_debug("cpu", "Resetting CPU state for image entry");
        self.registers.fill(0);
        self.flags = Flags::default();
        self.sp = crate::machine::usable_ram_size(self.memory.len()) as u32 - 1;
        self.kernel_sp = self.sp;
        self.loaded_image_segments.clear();
        self.privilege_mode = PrivilegeMode::Supervisor;
        self.mmu_enabled = false;
        self.page_directory = 0;
        self.last_page_fault = None;
        self.interrupts.control = 0;
        self.idt_base = None;
        self.scheduler = Scheduler::default();
        self.syscall_state = KernelState::default();
        self.page_memory_manager = None;
        self.running = true;
        self.booted_disk_path = None;
        self.terminal_active = false;
        self.terminal_dirty = false;
    }

    pub fn initial_user_stack_pointer(&self) -> u32 {
        let ram_size = crate::machine::usable_ram_size(self.memory.len());
        (ram_size as u32 - 1).saturating_sub((ram_size / 4).min(0x1_0000) as u32)
    }

    pub fn enter_user_mode(&mut self, user_pc: u32, user_sp: u32) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "enter user mode",
            });
        }
        if user_pc > self.guest_address_mask() || crate::machine::is_device_aperture(user_pc) {
            return Err(CpuError::InvalidEntryPoint(user_pc));
        }
        if user_sp == 0
            || user_sp > self.guest_address_mask()
            || crate::machine::is_device_aperture(user_sp - 1)
        {
            return Err(CpuError::InvalidUserStack(user_sp));
        }
        if !self.mmu_enabled && user_sp > self.kernel_sp.saturating_sub(0x1000) {
            return Err(CpuError::InvalidUserStack(user_sp));
        }
        if !self.mmu_enabled {
            self.validate_user_image_entry(user_pc)?;
        }
        if self.mmu_enabled {
            self.validate_user_mapping(user_pc, user_sp)?;
        }
        self.kernel_sp = self.sp;
        self.sp = user_sp;
        self.pc = user_pc;
        self.privilege_mode = PrivilegeMode::User;
        Ok(())
    }

    pub fn create_user_task(
        &mut self,
        entry_point: u32,
        user_sp: u32,
        kernel_sp: u32,
        page_directory: Option<u32>,
    ) -> Result<TaskId, CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "create task",
            });
        }
        if entry_point
            > page_directory.map_or(self.memory_mask, |_| crate::mmu::ADDRESS_SPACE_SIZE - 1)
            || crate::machine::is_device_aperture(entry_point)
        {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        if page_directory.is_none() {
            self.validate_user_image_entry(entry_point)?;
        }
        if user_sp == 0
            || user_sp
                > page_directory.map_or(self.memory_mask, |_| crate::mmu::ADDRESS_SPACE_SIZE - 1)
            || kernel_sp as usize >= self.memory.len()
            || crate::machine::is_device_aperture(user_sp - 1)
            || crate::machine::is_device_aperture(kernel_sp)
            || (page_directory.is_none()
                && user_sp > kernel_sp.saturating_sub(crate::mmu::PAGE_SIZE as u32))
        {
            return Err(CpuError::InvalidUserStack(user_sp));
        }
        if let Some(directory) = page_directory {
            self.validate_page_directory(directory)?;
            self.validate_user_mapping_in(directory, entry_point, user_sp)?;
        }
        let context = TaskContext::user(
            entry_point,
            user_sp,
            kernel_sp,
            page_directory.is_some(),
            page_directory.unwrap_or(0),
        );
        self.scheduler
            .create_task(context)
            .map_err(CpuError::Scheduler)
    }

    pub fn start_task(&mut self, id: TaskId) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor
            || self.scheduler.current_task().is_some()
        {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "start task",
            });
        }
        let context = self.scheduler.activate(id).map_err(CpuError::Scheduler)?;
        self.restore_task_context(&context);
        Ok(())
    }

    pub fn yield_current_task(&mut self) -> Result<bool, CpuError> {
        self.schedule_current_task(TaskState::Runnable, None)
    }

    pub fn block_current_task(&mut self) -> Result<bool, CpuError> {
        self.schedule_current_task(TaskState::Blocked, None)
    }

    pub fn exit_current_task(&mut self, exit_code: i32) -> Result<bool, CpuError> {
        self.schedule_current_task(TaskState::Exited, Some(exit_code))
    }

    pub fn wake_task(&mut self, id: TaskId) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "wake task",
            });
        }
        self.scheduler.wake(id).map_err(CpuError::Scheduler)?;
        if self.scheduler.current_task().is_none() {
            let context = self.scheduler.activate(id).map_err(CpuError::Scheduler)?;
            self.restore_task_context(&context);
        }
        Ok(())
    }

    pub fn task_context(&self, id: TaskId) -> Option<&TaskContext> {
        self.scheduler.task(id)
    }

    pub fn current_task(&self) -> Option<TaskId> {
        self.scheduler.current_task()
    }

    fn schedule_current_task(
        &mut self,
        state: TaskState,
        exit_code: Option<i32>,
    ) -> Result<bool, CpuError> {
        let id = self
            .scheduler
            .current_task()
            .ok_or(CpuError::Scheduler(SchedulerError::NoCurrentTask))?;
        let mut context = self.capture_task_context(id);
        context.exit_code = exit_code;
        let next = self
            .scheduler
            .reschedule(context, state)
            .map_err(CpuError::Scheduler)?;
        if let Some(context) = next {
            self.restore_task_context(&context);
            Ok(true)
        } else {
            self.running = false;
            self.debug_console
                .log_info("cpu", "CPU halted: no runnable tasks remain");
            Ok(false)
        }
    }

    fn capture_task_context(&self, id: TaskId) -> TaskContext {
        TaskContext {
            id,
            registers: self.registers,
            pc: self.pc,
            sp: self.sp,
            kernel_sp: self.kernel_sp,
            flags: self.flags,
            privilege_mode: self.privilege_mode,
            interrupt_enabled: self.interrupts.control & 1 != 0,
            mmu_enabled: self.mmu_enabled,
            page_directory: self.page_directory,
            state: TaskState::Running,
            exit_code: None,
        }
    }

    fn restore_task_context(&mut self, context: &TaskContext) {
        self.registers = context.registers;
        self.pc = context.pc;
        self.sp = context.sp;
        self.kernel_sp = context.kernel_sp;
        self.flags = context.flags;
        self.privilege_mode = context.privilege_mode;
        self.interrupts.control =
            (self.interrupts.control & !1) | u8::from(context.interrupt_enabled);
        self.mmu_enabled = context.mmu_enabled;
        self.page_directory = context.page_directory;
        self.running = true;
        if let Some(result) = self.syscall_state.pending_results.remove(&context.id) {
            self.registers[crate::syscall::REGISTER_NUMBER] = result;
        }
        if let Some((child_id, exit_code)) =
            self.syscall_state.pending_wait_status.remove(&context.id)
        {
            self.registers[crate::syscall::REGISTER_NUMBER] = i64::from(exit_code);
            self.registers[crate::syscall::REGISTER_ARGUMENT_0] = i64::from(child_id);
            self.reap_process(context.id, child_id);
        }
    }

    pub fn load_idt(&mut self, base: u32) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "load IDT",
            });
        }
        crate::interrupts::descriptor_address(base, 0, self.memory.len()).map_err(CpuError::Idt)?;
        self.idt_base = Some(base);
        Ok(())
    }

    pub fn set_idt_descriptor(
        &mut self,
        vector: u16,
        descriptor: crate::interrupts::IdtDescriptor,
    ) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "set IDT descriptor",
            });
        }
        if descriptor.handler > self.guest_address_mask()
            || crate::machine::is_device_aperture(descriptor.handler)
            || (self.mmu_enabled
                && crate::mmu::translate(
                    &self.memory,
                    self.page_directory,
                    descriptor.handler,
                    crate::mmu::MemoryAccess::Fetch,
                    false,
                )
                .is_err())
        {
            return Err(CpuError::InvalidEntryPoint(descriptor.handler));
        }
        let base = self
            .idt_base
            .ok_or(CpuError::Idt(crate::interrupts::IdtError::InvalidBase))?;
        crate::interrupts::write_descriptor(&mut self.memory, base, vector, descriptor)
            .map_err(CpuError::Idt)
    }

    fn idt_descriptor(&self, vector: u16) -> Result<crate::interrupts::IdtDescriptor, CpuError> {
        let Some(base) = self.idt_base else {
            return Err(CpuError::UnhandledException {
                vector,
                cause: vector,
            });
        };
        let descriptor =
            crate::interrupts::read_descriptor(&self.memory, base, vector).map_err(|error| {
                match error {
                    crate::interrupts::IdtError::MissingDescriptor => {
                        CpuError::UnhandledException {
                            vector,
                            cause: vector,
                        }
                    }
                    _ => CpuError::DoubleFault,
                }
            })?;
        if descriptor.handler > self.guest_address_mask()
            || crate::machine::is_device_aperture(descriptor.handler)
        {
            return Err(CpuError::DoubleFault);
        }
        Ok(descriptor)
    }

    fn validate_page_directory(&self, page_directory: u32) -> Result<(), CpuError> {
        let end = page_directory
            .checked_add(crate::mmu::PAGE_TABLE_BYTES as u32)
            .ok_or(CpuError::SegmentOutOfBounds {
                address: page_directory,
                length: crate::mmu::PAGE_TABLE_BYTES,
            })?;
        if page_directory & 0x7ff != 0
            || end as usize > crate::machine::usable_ram_size(self.memory.len())
        {
            return Err(CpuError::SegmentOutOfBounds {
                address: page_directory,
                length: crate::mmu::PAGE_TABLE_BYTES,
            });
        }
        Ok(())
    }

    fn validate_user_mapping(&self, entry_point: u32, user_sp: u32) -> Result<(), CpuError> {
        self.validate_user_mapping_in(self.page_directory, entry_point, user_sp)
    }

    fn validate_user_image_entry(&self, entry_point: u32) -> Result<(), CpuError> {
        if !self.loaded_image_segments.is_empty()
            && !self.loaded_image_segments.iter().any(|segment| {
                segment.permissions.execute
                    && entry_point >= segment.address
                    && ((entry_point - segment.address) as usize) < segment.length
            })
        {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        Ok(())
    }

    fn validate_user_mapping_in(
        &self,
        page_directory: u32,
        entry_point: u32,
        user_sp: u32,
    ) -> Result<(), CpuError> {
        crate::mmu::translate(
            &self.memory,
            page_directory,
            entry_point,
            crate::mmu::MemoryAccess::Fetch,
            true,
        )
        .map_err(CpuError::PageFault)?;
        crate::mmu::translate(
            &self.memory,
            page_directory,
            user_sp - 1,
            crate::mmu::MemoryAccess::Write,
            true,
        )
        .map_err(CpuError::PageFault)?;
        Ok(())
    }

    fn guest_address_mask(&self) -> u32 {
        crate::mmu::ADDRESS_SPACE_SIZE - 1
    }

    pub fn enable_mmu(&mut self, page_directory: u32) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "enable MMU",
            });
        }
        self.validate_page_directory(page_directory)?;
        self.page_directory = page_directory;
        self.mmu_enabled = true;
        Ok(())
    }

    pub fn switch_address_space(&mut self, page_directory: u32) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "switch address space",
            });
        }
        self.validate_page_directory(page_directory)?;
        self.page_directory = page_directory;
        Ok(())
    }

    pub fn disable_mmu(&mut self) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "disable MMU",
            });
        }
        self.mmu_enabled = false;
        Ok(())
    }

    pub fn is_mmu_enabled(&self) -> bool {
        self.mmu_enabled
    }

    pub fn page_directory(&self) -> u32 {
        self.page_directory
    }

    pub fn last_page_fault(&self) -> Option<crate::mmu::PageFault> {
        self.last_page_fault
    }

    pub fn read(&mut self, address: u32) -> u8 {
        if crate::machine::is_device_aperture(address) {
            if let Some(value) = self.read_terminal_register(address) {
                return value;
            }
            if let Some(value) = self.input.read(address) {
                return value;
            }
            return crate::machine::system_register_byte(address, self.memory.len())
                .or_else(|| self.interrupts.read(address))
                .or_else(|| self.clock.read(address))
                .or_else(|| self.audio.read(address))
                .or_else(|| self.network_registers.read(address))
                .or_else(|| self.graphics_device.read(address))
                .or_else(|| self.firmware_gateway.read(address))
                .or_else(|| self.filesystem_device.read(address))
                .or_else(|| self.fault_register_byte(address))
                .expect("host read from an unimplemented device register");
        }
        self.memory[address as usize]
    }

    pub fn configure_disk(
        &mut self,
        storage_dir: PathBuf,
        program_file_root: PathBuf,
    ) -> Result<(), String> {
        self.disk_root = storage_dir;
        self.program_folder_disk = if program_file_root.exists() {
            Some(FolderDisk::open_read_only_directory(&program_file_root)?)
        } else {
            None
        };
        Ok(())
    }

    pub fn available_disks(&self) -> Result<Vec<String>, String> {
        FolderDisk::discover(&self.disk_root).map(|disks| {
            disks
                .into_iter()
                .map(|disk| disk.manifest().id.clone())
                .collect()
        })
    }

    pub fn mount_data_disk(&mut self, disk_id: &str) -> Result<(), String> {
        if !self
            .available_disks()?
            .iter()
            .any(|available_id| available_id == disk_id)
        {
            return Err(format!("folder disk {disk_id:?} does not exist"));
        }
        let disk_path = self.disk_root.join(disk_id);
        self.folder_disk = Some(FolderDisk::open(&disk_path)?);
        Ok(())
    }

    pub fn mount_folder_disk(&mut self, path: &std::path::Path) -> Result<(), String> {
        let folder_disk = FolderDisk::open(path)?;
        self.folder_disk = Some(folder_disk);
        Ok(())
    }

    pub fn set_network_port(&mut self, port: u16) {
        self.network_port = port;
    }

    pub fn enable_scnet(&mut self, port: u16) {
        self.scnet_enabled = true;
        self.scnet_port = port;
    }

    fn ensure_scnet_node(&mut self, settle_time: std::time::Duration) -> Result<(), String> {
        let name = self.bios_settings.network_name.clone();
        let new_identity = self.scnet_name.as_deref() != Some(name.as_str());
        if new_identity {
            self.scnet = None;
            self.scnet_name = Some(name.clone());
        }
        let created = self.scnet.is_none();
        if created {
            self.scnet = Some(ScNetNode::new(&name, self.scnet_port)?);
        }
        let deadline = std::time::Instant::now()
            + if created {
                settle_time
            } else {
                std::time::Duration::ZERO
            };
        loop {
            let node = self.scnet.as_mut().expect("SC-NET node initialized");
            node.poll()?;
            if node.has_name_collision() {
                return Err(format!(
                    "SC-NET node name {:?} is already in use",
                    self.bios_settings.network_name
                ));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn validate_guest_mutation_path(path: &str) -> Result<(), String> {
        if matches!(path, "main.bin" | "kernel.bin") {
            return Err("boot executable paths cannot be changed by guest code".to_owned());
        }
        Ok(())
    }

    fn guest_memory_addresses(
        &self,
        address: u32,
        length: usize,
        access: crate::mmu::MemoryAccess,
        user_mode: bool,
    ) -> Result<Vec<usize>, String> {
        let mut addresses = Vec::with_capacity(length);
        for offset in 0..length {
            let offset = u32::try_from(offset)
                .map_err(|_| "guest buffer length exceeds address space".to_owned())?;
            let virtual_address = address
                .checked_add(offset)
                .filter(|value| {
                    if self.mmu_enabled {
                        *value < crate::mmu::ADDRESS_SPACE_SIZE
                    } else {
                        (*value as usize) < self.memory.len()
                    }
                })
                .ok_or_else(|| "guest buffer range is outside address space".to_owned())?;
            if Self::is_device_backing_address(virtual_address) {
                return Err("guest buffer overlaps device registers".to_owned());
            }
            let physical_address = if self.mmu_enabled {
                crate::mmu::translate(
                    &self.memory,
                    self.page_directory,
                    virtual_address,
                    access,
                    user_mode,
                )
                .map_err(|fault| format!("guest buffer page fault: {fault:?}"))?
            } else {
                virtual_address
            };
            if Self::is_device_backing_address(physical_address)
                || physical_address as usize >= self.memory.len()
            {
                return Err("guest buffer is outside accessible RAM".to_owned());
            }
            addresses.push(physical_address as usize);
        }
        Ok(addresses)
    }

    fn is_device_backing_address(address: u32) -> bool {
        crate::machine::is_device_aperture(address)
    }

    fn read_guest_path(&self, address: u32, user_mode: bool) -> Result<Option<String>, String> {
        let mut bytes = Vec::new();
        for offset in 0..=128u32 {
            let current = address
                .checked_add(offset)
                .ok_or_else(|| "guest path address overflow".to_owned())?;
            let physical =
                self.guest_memory_addresses(current, 1, crate::mmu::MemoryAccess::Read, user_mode)?
                    [0];
            let byte = self.memory[physical];
            if byte == 0 {
                if bytes.is_empty() {
                    return Ok(None);
                }
                let path =
                    String::from_utf8(bytes).map_err(|_| "guest path must be ASCII".to_owned())?;
                if !path.is_ascii() {
                    return Err("guest path must be ASCII".to_owned());
                }
                return Ok(Some(path));
            }
            if !byte.is_ascii() {
                return Err("guest path must be ASCII".to_owned());
            }
            bytes.push(byte);
        }
        Err("guest path is not null-terminated within 128 bytes".to_owned())
    }

    fn check_account_disk_access(
        &self,
        task_id: TaskId,
        disk: &crate::disk::FolderDisk,
        path: &str,
        is_directory: bool,
        required: u16,
    ) -> Result<(), i64> {
        if !disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            return Ok(());
        }
        let accounts =
            crate::accounts::AccountStore::load(disk).map_err(|_| crate::syscall::error::IO)?;
        let process = self
            .syscall_state
            .processes
            .get(&task_id)
            .ok_or(crate::syscall::error::INVALID)?;
        let (uid, mut gids) = if process.bootstrap_authority {
            (0, vec![0])
        } else if let Some(credentials) = &process.credentials {
            (
                credentials.uid,
                std::iter::once(credentials.primary_gid)
                    .chain(credentials.supplementary_gids.iter().copied())
                    .collect(),
            )
        } else {
            return Err(crate::syscall::error::PERMISSION);
        };
        gids.sort_unstable();
        gids.dedup();

        let components: Vec<_> = path.split('/').collect();
        let mut parent = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(component);
            if !accounts.can_access(&parent, true, uid, &gids, 0o1) {
                return Err(crate::syscall::error::PERMISSION);
            }
        }
        if !accounts.can_access(path, is_directory, uid, &gids, required) {
            return Err(crate::syscall::error::PERMISSION);
        }
        Ok(())
    }

    fn check_account_parent_access(
        &self,
        task_id: TaskId,
        disk: &crate::disk::FolderDisk,
        path: &str,
        required: u16,
    ) -> Result<(), i64> {
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.check_account_disk_access(task_id, disk, parent, true, required)
    }

    fn register_created_account_path(
        &self,
        task_id: TaskId,
        disk: &crate::disk::FolderDisk,
        path: &str,
        is_directory: bool,
    ) -> Result<(), i64> {
        if !disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            return Ok(());
        }
        let process = self
            .syscall_state
            .processes
            .get(&task_id)
            .ok_or(crate::syscall::error::INVALID)?;
        let (uid, gid) = if process.bootstrap_authority {
            (0, 0)
        } else if let Some(credentials) = &process.credentials {
            (credentials.uid, credentials.primary_gid)
        } else {
            return Err(crate::syscall::error::PERMISSION);
        };
        let mut accounts =
            crate::accounts::AccountStore::load(disk).map_err(|_| crate::syscall::error::IO)?;
        accounts
            .register_created_path(path, is_directory, uid, gid)
            .map_err(|error| match error {
                crate::accounts::AccountError::CapacityExceeded => crate::syscall::error::NO_MEMORY,
                _ => crate::syscall::error::INVALID,
            })?;
        accounts.save(disk).map_err(|_| crate::syscall::error::IO)
    }

    fn dispatch_user_syscall(&mut self) -> Result<(), CpuError> {
        let version = self.registers[crate::syscall::REGISTER_ABI_VERSION];
        let number = self.registers[crate::syscall::REGISTER_NUMBER];
        let task_id = self.scheduler.current_task();
        if let Some(id) = task_id {
            let has_bootstrap_authority = self.syscall_state.processes.is_empty()
                && self
                    .folder_disk
                    .as_ref()
                    .is_some_and(|disk| disk.manifest().id.eq_ignore_ascii_case("sisuos"));
            self.syscall_state.register_process(id, None);
            let record = self
                .syscall_state
                .processes
                .get_mut(&id)
                .expect("registered process");
            record.bootstrap_authority |= has_bootstrap_authority;
            if has_bootstrap_authority && record.credentials.is_none() {
                record.credentials = Some(crate::syscall::ProcessCredentials {
                    uid: 0,
                    primary_gid: 0,
                    supplementary_gids: Vec::new(),
                    home: "root".to_owned(),
                });
            }
            if record.name.is_empty() {
                record.name = self
                    .folder_disk
                    .as_ref()
                    .map_or_else(|| "init".to_owned(), |disk| disk.manifest().id.clone());
                if record.arguments.is_empty() {
                    record.arguments.push(record.name.clone());
                }
            }
        }

        let mut scheduled_state = None;
        let result = if version != crate::syscall::ABI_VERSION {
            crate::syscall::error::INVALID
        } else {
            match SyscallNumber::try_from(number) {
                Ok(SyscallNumber::GetPid) => task_id
                    .map(i64::from)
                    .unwrap_or(crate::syscall::error::INVALID),
                Ok(SyscallNumber::Exit) => match self.registers[1] {
                    status @ 0..=255 => {
                        scheduled_state = Some((TaskState::Exited, status as i32));
                        0
                    }
                    _ => crate::syscall::error::INVALID,
                },
                Ok(SyscallNumber::Yield) => {
                    scheduled_state = Some((TaskState::Runnable, 0));
                    0
                }
                Ok(SyscallNumber::Open) => self.syscall_open(task_id),
                Ok(SyscallNumber::Read) => self.syscall_read(task_id),
                Ok(SyscallNumber::Write) => self.syscall_write(task_id),
                Ok(SyscallNumber::Close) => self.syscall_close(task_id),
                Ok(
                    SyscallNumber::Spawn | SyscallNumber::SpawnArgument | SyscallNumber::SpawnExec,
                ) => self.syscall_spawn(task_id),
                Ok(SyscallNumber::ArgumentCount) => task_id
                    .and_then(|id| self.syscall_state.processes.get(&id))
                    .map(|record| record.arguments.len() as i64)
                    .unwrap_or(crate::syscall::error::INVALID),
                Ok(SyscallNumber::Argument) => self.syscall_argument(task_id),
                Ok(SyscallNumber::Mmap) => self.syscall_mmap(task_id),
                Ok(SyscallNumber::Sleep) => self.syscall_sleep(task_id, &mut scheduled_state),
                Ok(SyscallNumber::SleepMillis) => {
                    self.syscall_sleep_millis(task_id, &mut scheduled_state)
                }
                Ok(SyscallNumber::Wait) => self.syscall_wait(task_id, &mut scheduled_state),
                Ok(SyscallNumber::TryWait) => self.syscall_wait(task_id, &mut scheduled_state),
                Ok(SyscallNumber::Semaphore) => {
                    self.syscall_semaphore(task_id, &mut scheduled_state)
                }
                Ok(SyscallNumber::Seek) => self.syscall_seek(task_id),
                Ok(SyscallNumber::ScNet) => self.syscall_scnet(task_id),
                Ok(SyscallNumber::Graphics) => self.syscall_graphics(task_id),
                Ok(SyscallNumber::InputSnapshot) => self.syscall_input_snapshot(task_id),
                Ok(SyscallNumber::AccountStatus) => self.syscall_account_status(task_id),
                Ok(SyscallNumber::InitializeRootPassword) => {
                    self.syscall_initialize_root_password(task_id)
                }
                Ok(SyscallNumber::Authenticate) => self.syscall_authenticate(task_id),
                Ok(SyscallNumber::SetConsoleEcho) => self.syscall_set_console_echo(task_id),
                Ok(SyscallNumber::AddGroup) => self.syscall_add_group(task_id),
                Ok(SyscallNumber::AddUser) => self.syscall_add_user(task_id),
                Ok(SyscallNumber::SetPassword) => self.syscall_set_password(task_id),
                Ok(SyscallNumber::DirectoryEntry) => self.syscall_directory_entry(task_id),
                Ok(SyscallNumber::MakeDirectory) => self.syscall_make_directory(task_id),
                Ok(SyscallNumber::DeleteFile) => self.syscall_delete_file(task_id),
                Ok(SyscallNumber::SetFileMode) => self.syscall_set_file_mode(task_id),
                Ok(SyscallNumber::SetFileOwner) => self.syscall_set_file_owner(task_id),
                Ok(SyscallNumber::Kill) => self.syscall_kill(task_id),
                Ok(SyscallNumber::RenameFile) => self.syscall_rename_file(task_id),
                Ok(SyscallNumber::ProcessAccounting) => self.syscall_process_accounting(),
                Ok(SyscallNumber::ProcessInfo | SyscallNumber::ProcessStatus) => {
                    self.syscall_process_info()
                }
                Ok(SyscallNumber::StartupArgument) => task_id
                    .and_then(|id| self.syscall_state.processes.get(&id))
                    .map(|record| i64::from(record.startup_argument))
                    .unwrap_or(crate::syscall::error::INVALID),
                Ok(SyscallNumber::MonotonicMillis) => self.monotonic_milliseconds(),
                Err(()) => crate::syscall::error::INVALID,
            }
        };

        self.registers[crate::syscall::REGISTER_NUMBER] = result;
        self.return_from_exception()?;
        if let Some((state, exit_code)) = scheduled_state {
            let Some(id) = task_id else {
                self.registers[crate::syscall::REGISTER_NUMBER] = crate::syscall::error::INVALID;
                return Ok(());
            };
            if state == TaskState::Exited {
                self.complete_process(id, exit_code);
            }
            self.schedule_current_task(state, (state == TaskState::Exited).then_some(exit_code))?;
        }
        Ok(())
    }

    fn monotonic_milliseconds(&self) -> i64 {
        #[cfg(test)]
        if let Some(milliseconds) = self.test_monotonic_ms {
            return i64::from(milliseconds & ADDRESS_MASK);
        }
        (self.monotonic_epoch.elapsed().as_millis() & u128::from(ADDRESS_MASK)) as i64
    }

    fn syscall_process_info(&mut self) -> i64 {
        let Ok(index) = usize::try_from(self.registers[1]) else {
            return crate::syscall::error::INVALID;
        };
        let Ok(destination) = u32::try_from(self.registers[2]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let Ok(capacity) = usize::try_from(self.registers[3]) else {
            return crate::syscall::error::INVALID;
        };
        if capacity > crate::syscall::MAX_TRANSFER_BYTES {
            return crate::syscall::error::INVALID;
        }
        let Some(task) = self
            .scheduler
            .tasks()
            .iter()
            .filter(|task| self.syscall_state.processes.contains_key(&task.id))
            .nth(index)
        else {
            return 0;
        };
        let record = &self.syscall_state.processes[&task.id];
        let detailed = self.registers[0] == SyscallNumber::ProcessStatus as i64;
        let state = match task.state {
            TaskState::Runnable if detailed => "ready",
            TaskState::Blocked
                if detailed
                    && (self.syscall_state.sleeping.contains_key(&task.id)
                        || self.syscall_state.sleeping_millis.contains_key(&task.id)) =>
            {
                "sleeping"
            }
            TaskState::Blocked
                if detailed && self.syscall_state.waiting_for.contains_key(&task.id) =>
            {
                "waiting"
            }
            TaskState::Runnable => "runnable",
            TaskState::Running => "running",
            TaskState::Blocked => "blocked",
            TaskState::Exited => "exited",
        };
        let parent = record.parent.map_or("-".to_owned(), |id| id.to_string());
        let text = if detailed {
            let name = if record.name.is_empty() {
                "init"
            } else {
                &record.name
            };
            let name: String = name
                .chars()
                .map(|character| {
                    if character.is_ascii_graphic() {
                        character
                    } else {
                        '?'
                    }
                })
                .collect();
            format!("{} {parent} {state} {name}\n", task.id)
        } else {
            format!("{} {parent} {state}\n", task.id)
        };
        if text.len() > capacity {
            return crate::syscall::error::OVERFLOW;
        }
        let addresses = match self.guest_memory_addresses(
            destination,
            text.len(),
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        for (address, byte) in addresses.into_iter().zip(text.bytes()) {
            self.memory[address] = byte;
        }
        text.len() as i64
    }

    fn syscall_process_accounting(&mut self) -> i64 {
        let Ok(index) = usize::try_from(self.registers[1]) else {
            return crate::syscall::error::INVALID;
        };
        let Ok(destination) = u32::try_from(self.registers[2]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let Ok(capacity) = usize::try_from(self.registers[3]) else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[4..=6].iter().any(|argument| *argument != 0)
            || capacity > crate::syscall::MAX_TRANSFER_BYTES
        {
            return crate::syscall::error::INVALID;
        }
        let Some(task) = self
            .scheduler
            .tasks()
            .iter()
            .filter(|task| self.syscall_state.processes.contains_key(&task.id))
            .nth(index)
        else {
            return 0;
        };
        let instructions = self
            .syscall_state
            .process_instructions
            .get(&task.id)
            .copied()
            .unwrap_or_default();
        let text = format!("{} {instructions}\n", task.id);
        if text.len() > capacity {
            return crate::syscall::error::OVERFLOW;
        }
        let addresses = match self.guest_memory_addresses(
            destination,
            text.len(),
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        for (address, byte) in addresses.into_iter().zip(text.bytes()) {
            self.memory[address] = byte;
        }
        text.len() as i64
    }

    fn syscall_kill(&mut self, caller: Option<TaskId>) -> i64 {
        let Some(caller) = caller else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[3..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let target = match u32::try_from(self.registers[1]) {
            Ok(id) => id,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let exit_code = match i32::try_from(self.registers[2]) {
            Ok(code) if (0..=255).contains(&code) => code,
            _ => return crate::syscall::error::INVALID,
        };
        if target == 0 || target == caller {
            return crate::syscall::error::PERMISSION;
        }
        let Some(target_record) = self.syscall_state.processes.get(&target) else {
            return crate::syscall::error::NO_ENTRY;
        };
        let is_child = target_record.parent == Some(caller)
            && self
                .syscall_state
                .processes
                .get(&caller)
                .is_some_and(|record| record.children.contains(&target));
        let is_root = self
            .syscall_state
            .processes
            .get(&caller)
            .and_then(|record| record.credentials.as_ref())
            .is_some_and(|credentials| credentials.uid == 0);
        if !is_child && !is_root {
            return crate::syscall::error::PERMISSION;
        }
        if target_record.exit_code.is_some() {
            return crate::syscall::error::NO_ENTRY;
        }
        if !target_record.children.is_empty() {
            return crate::syscall::error::BUSY;
        }
        if self.scheduler.terminate(target, exit_code).is_err() {
            return crate::syscall::error::NO_ENTRY;
        }
        self.complete_process(target, exit_code);
        0
    }

    fn syscall_directory_entry(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let directory_address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let index = match usize::try_from(self.registers[2]) {
            Ok(index) => index,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let destination = match u32::try_from(self.registers[3]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let capacity = match usize::try_from(self.registers[4]) {
            Ok(capacity) if capacity <= crate::syscall::MAX_TRANSFER_BYTES => capacity,
            _ => return crate::syscall::error::INVALID,
        };
        let directory = match self.read_guest_path(directory_address, true) {
            Ok(Some(directory)) => directory,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let is_root = directory == "/";
        let permission_path = if is_root { "" } else { directory.as_str() };
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if let Err(error) =
            self.check_account_disk_access(task_id, disk, permission_path, true, 0o5)
        {
            return error;
        }
        let entries = match disk.list_dir((!is_root).then_some(directory.as_str())) {
            Ok(entries) => entries,
            Err(_) => return crate::syscall::error::IO,
        };
        let Some(entry) = entries.get(index) else {
            return 0;
        };
        let suffix = if entry.is_directory { "/" } else { "" };
        let length = entry.name.len() + suffix.len();
        if capacity <= length {
            return crate::syscall::error::OVERFLOW;
        }
        let addresses = match self.guest_memory_addresses(
            destination,
            length + 1,
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        for (address, byte) in addresses
            .into_iter()
            .zip(entry.name.bytes().chain(suffix.bytes()).chain([0]))
        {
            self.memory[address] = byte;
        }
        length as i64
    }

    fn syscall_make_directory(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[2..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let path = match self.read_guest_path(address, true) {
            Ok(Some(path)) => path,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        if Self::validate_guest_mutation_path(&path).is_err() {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        if let Err(error) = self.check_account_parent_access(task_id, disk, &path, 0o3) {
            return error;
        }
        if disk.metadata(&path).is_ok() {
            return crate::syscall::error::INVALID;
        }
        if disk.create_dir(&path).is_err() {
            return crate::syscall::error::NO_ENTRY;
        }
        if let Err(error) = self.register_created_account_path(task_id, disk, &path, true) {
            if let Err(cleanup_error) = disk.delete_entry(&path) {
                self.debug_console.log_error(
                    "filesystem",
                    &format!("failed to roll back created directory {path:?}: {cleanup_error}"),
                );
            }
            return error;
        }
        0
    }

    fn syscall_delete_file(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[2..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let path = match self.read_guest_path(address, true) {
            Ok(Some(path)) => path,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        if Self::validate_guest_mutation_path(&path).is_err() {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        if let Err(error) = self.check_account_parent_access(task_id, disk, &path, 0o3) {
            return error;
        }
        let mut accounts = if disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            let mut accounts = match crate::accounts::AccountStore::load(disk) {
                Ok(accounts) => accounts,
                Err(_) => return crate::syscall::error::IO,
            };
            if accounts.remove_file_permission(&path).is_err() {
                return crate::syscall::error::INVALID;
            }
            Some(accounts)
        } else {
            None
        };
        if disk.delete_file(&path).is_err() {
            return crate::syscall::error::NO_ENTRY;
        }
        if let Some(accounts) = accounts.as_mut()
            && accounts.save(disk).is_err()
        {
            self.debug_console.log_error(
                "filesystem",
                &format!("deleted {path:?} but could not persist its permission update"),
            );
            return crate::syscall::error::IO;
        }
        0
    }

    fn syscall_rename_file(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[3..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let values = match self.account_request_strings(&[self.registers[1], self.registers[2]], 2)
        {
            Ok(values) => values,
            Err(error) => return error,
        };
        let [source, destination]: [String; 2] = match values.try_into() {
            Ok(values) => values,
            Err(_) => return crate::syscall::error::INVALID,
        };
        if Self::validate_guest_mutation_path(&source).is_err()
            || Self::validate_guest_mutation_path(&destination).is_err()
        {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::NOT_IMPLEMENTED;
        };
        if !disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            return crate::syscall::error::NOT_IMPLEMENTED;
        }
        if disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        for path in [&source, &destination] {
            if let Err(error) = self.check_account_parent_access(task_id, disk, path, 0o3) {
                return error;
            }
        }
        match disk.metadata(&source) {
            Ok(metadata) if !metadata.is_directory => {}
            Ok(_) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::NO_ENTRY,
        }
        if disk.metadata(&destination).is_ok() {
            return crate::syscall::error::INVALID;
        }
        if self
            .syscall_state
            .open_files
            .values()
            .flat_map(|files| files.values())
            .any(|file| file.path.eq_ignore_ascii_case(&source))
        {
            return crate::syscall::error::BUSY;
        }
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        if accounts
            .rename_file_permission(&source, &destination)
            .is_err()
        {
            return crate::syscall::error::INVALID;
        }
        if disk.rename_file(&source, &destination).is_err() {
            return crate::syscall::error::NO_ENTRY;
        }
        if accounts.save(disk).is_err() {
            if let Err(error) = disk.rename_file(&destination, &source) {
                self.debug_console.log_error(
                    "filesystem",
                    &format!(
                        "renamed {source:?} but failed to save permissions and rollback: {error}"
                    ),
                );
            }
            return crate::syscall::error::IO;
        }
        0
    }

    fn syscall_set_file_mode(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[3..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let mode = match u16::try_from(self.registers[2]) {
            Ok(mode) if mode <= 0o777 => mode,
            _ => return crate::syscall::error::INVALID,
        };
        let path = match self.read_guest_path(address, true) {
            Ok(Some(path)) => path,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::NOT_IMPLEMENTED;
        };
        if !disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            return crate::syscall::error::NOT_IMPLEMENTED;
        }
        if disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        if Self::validate_guest_mutation_path(&path).is_err() {
            return crate::syscall::error::PERMISSION;
        }
        if let Err(error) = self.check_account_parent_access(task_id, disk, &path, 0o1) {
            return error;
        }
        let is_directory = match disk.metadata(&path) {
            Ok(metadata) => metadata.is_directory,
            Err(_) => return crate::syscall::error::NO_ENTRY,
        };
        let caller_uid = match self
            .syscall_state
            .processes
            .get(&task_id)
            .and_then(|process| process.credentials.as_ref())
        {
            Some(credentials) => credentials.uid,
            None => return crate::syscall::error::PERMISSION,
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        if let Err(error) = accounts.set_file_mode(&path, is_directory, caller_uid, mode) {
            return match error {
                crate::accounts::AccountError::PermissionDenied => {
                    crate::syscall::error::PERMISSION
                }
                crate::accounts::AccountError::CapacityExceeded => crate::syscall::error::NO_MEMORY,
                _ => crate::syscall::error::INVALID,
            };
        }
        if accounts.save(disk).is_err() {
            return crate::syscall::error::IO;
        }
        0
    }

    fn syscall_set_file_owner(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[4..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let values = match self.account_request_strings(
            &[self.registers[1], self.registers[2], self.registers[3]],
            3,
        ) {
            Ok(values) => values,
            Err(error) => return error,
        };
        let [path, user_name, group_name]: [String; 3] = match values.try_into() {
            Ok(values) => values,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::NOT_IMPLEMENTED;
        };
        if !disk.manifest().id.eq_ignore_ascii_case("sisuos") {
            return crate::syscall::error::NOT_IMPLEMENTED;
        }
        if disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        if Self::validate_guest_mutation_path(&path).is_err() {
            return crate::syscall::error::PERMISSION;
        }
        if let Err(error) = self.check_account_parent_access(task_id, disk, &path, 0o1) {
            return error;
        }
        let is_directory = match disk.metadata(&path) {
            Ok(metadata) => metadata.is_directory,
            Err(_) => return crate::syscall::error::NO_ENTRY,
        };
        let credentials = match self
            .syscall_state
            .processes
            .get(&task_id)
            .and_then(|process| process.credentials.as_ref())
        {
            Some(credentials) => credentials,
            None => return crate::syscall::error::PERMISSION,
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        let Some(uid) = accounts.user(&user_name).map(|user| user.uid) else {
            return crate::syscall::error::NO_ENTRY;
        };
        let Some(gid) = accounts.group(&group_name).map(|group| group.gid) else {
            return crate::syscall::error::NO_ENTRY;
        };
        let gids = std::iter::once(credentials.primary_gid)
            .chain(credentials.supplementary_gids.iter().copied())
            .collect::<Vec<_>>();
        if let Err(error) =
            accounts.set_file_owner(&path, is_directory, credentials.uid, uid, gid, &gids)
        {
            return match error {
                crate::accounts::AccountError::PermissionDenied => {
                    crate::syscall::error::PERMISSION
                }
                _ => crate::syscall::error::INVALID,
            };
        }
        if accounts.save(disk).is_err() {
            return crate::syscall::error::IO;
        }
        0
    }

    fn syscall_open(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let flags = match u32::try_from(self.registers[2]) {
            Ok(flags) if flags != 0 && flags & !crate::syscall::open_flags::ALL == 0 => flags,
            _ => return crate::syscall::error::INVALID,
        };
        if flags & (crate::syscall::open_flags::READ | crate::syscall::open_flags::WRITE) == 0
            || (flags & crate::syscall::open_flags::CREATE != 0
                && flags & crate::syscall::open_flags::WRITE == 0)
            || (flags & crate::syscall::open_flags::TRUNCATE != 0
                && flags & crate::syscall::open_flags::WRITE == 0)
            || (flags & crate::syscall::open_flags::APPEND != 0
                && flags & crate::syscall::open_flags::WRITE == 0)
            || (flags & crate::syscall::open_flags::APPEND != 0
                && flags & crate::syscall::open_flags::TRUNCATE != 0)
        {
            return crate::syscall::error::INVALID;
        }
        let path = match self.read_guest_path(address, true) {
            Ok(Some(path)) => path,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        if flags & crate::syscall::open_flags::WRITE != 0
            && Self::validate_guest_mutation_path(&path).is_err()
        {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if flags & crate::syscall::open_flags::WRITE != 0 && disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        let target_exists = disk.metadata(&path).is_ok();
        if flags & crate::syscall::open_flags::READ != 0 {
            if let Err(error) = self.check_account_disk_access(task_id, disk, &path, false, 0o4) {
                return error;
            }
        }
        if flags & crate::syscall::open_flags::WRITE != 0 {
            let access = if flags & crate::syscall::open_flags::CREATE != 0 && !target_exists {
                self.check_account_parent_access(task_id, disk, &path, 0o3)
            } else {
                self.check_account_disk_access(task_id, disk, &path, false, 0o2)
            };
            if let Err(error) = access {
                return error;
            }
        }
        if disk
            .ensure_regular_file(&path, flags & crate::syscall::open_flags::CREATE != 0)
            .is_err()
        {
            return crate::syscall::error::NO_ENTRY;
        }
        if flags & crate::syscall::open_flags::CREATE != 0 && !target_exists {
            if let Err(error) = self.register_created_account_path(task_id, disk, &path, false) {
                if let Err(cleanup_error) = disk.delete_file(&path) {
                    self.debug_console.log_error(
                        "filesystem",
                        &format!("failed to roll back created file {path:?}: {cleanup_error}"),
                    );
                }
                return error;
            }
        }
        if flags & crate::syscall::open_flags::TRUNCATE != 0 && disk.write_file(&path, &[]).is_err()
        {
            return crate::syscall::error::PERMISSION;
        }
        let offset = if flags & crate::syscall::open_flags::APPEND != 0 {
            match disk.metadata(&path) {
                Ok(metadata) => metadata.size_bytes,
                Err(_) => return crate::syscall::error::IO,
            }
        } else {
            0
        };
        match self.syscall_state.allocate_file_handle(
            task_id,
            OpenFile {
                path,
                offset,
                flags,
            },
        ) {
            Ok(handle) => i64::from(handle),
            Err(error) => error,
        }
    }

    fn syscall_read(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let handle = match u32::try_from(self.registers[1]) {
            Ok(handle) => handle,
            Err(_) => return crate::syscall::error::BAD_HANDLE,
        };
        let destination = match u32::try_from(self.registers[2]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let length = match usize::try_from(self.registers[3]) {
            Ok(length) if length <= crate::syscall::MAX_TRANSFER_BYTES => length,
            _ => return crate::syscall::error::INVALID,
        };
        let output = match self.guest_memory_addresses(
            destination,
            length,
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        if handle == 0 {
            let bytes_read = length.min(self.console.receive_buffer().len());
            for address in output.iter().take(bytes_read) {
                let byte = self.console.read_data();
                self.memory[*address] = byte;
                if self.console.control() & 1 != 0 && byte != 0 {
                    self.terminal_active = true;
                    self.terminal_dirty = true;
                }
            }
            return bytes_read as i64;
        }
        if handle < 3 {
            return crate::syscall::error::BAD_HANDLE;
        }
        let Some(file) = self.syscall_state.file(task_id, handle).cloned() else {
            return crate::syscall::error::BAD_HANDLE;
        };
        if file.flags & crate::syscall::open_flags::READ == 0 {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if let Err(error) = self.check_account_disk_access(task_id, disk, &file.path, false, 0o4) {
            return error;
        }
        let data = match disk.read_file(&file.path, file.offset, length) {
            Ok((data, _)) => data,
            Err(_) => return crate::syscall::error::IO,
        };
        for (address, byte) in output.into_iter().zip(data.iter().copied()) {
            self.memory[address] = byte;
        }
        if let Some(file) = self.syscall_state.file_mut(task_id, handle) {
            file.offset = file.offset.saturating_add(data.len() as u64);
        }
        data.len() as i64
    }

    fn syscall_write(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let handle = match u32::try_from(self.registers[1]) {
            Ok(handle) => handle,
            Err(_) => return crate::syscall::error::BAD_HANDLE,
        };
        let source = match u32::try_from(self.registers[2]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let length = match usize::try_from(self.registers[3]) {
            Ok(length) if length <= crate::syscall::MAX_TRANSFER_BYTES => length,
            _ => return crate::syscall::error::INVALID,
        };
        let input =
            match self.guest_memory_addresses(source, length, crate::mmu::MemoryAccess::Read, true)
            {
                Ok(addresses) => addresses,
                Err(_) => return crate::syscall::error::BAD_ADDRESS,
            };
        let data: Vec<u8> = input
            .into_iter()
            .map(|address| self.memory[address])
            .collect();
        if handle == 1 || handle == 2 {
            let has_output = !data.is_empty();
            for byte in data {
                self.console.write_data(byte);
            }
            if has_output {
                self.terminal_active = true;
                self.terminal_dirty = true;
            }
            return length as i64;
        }
        if handle == 0 {
            return crate::syscall::error::BAD_HANDLE;
        }
        let Some(file) = self.syscall_state.file(task_id, handle).cloned() else {
            return crate::syscall::error::BAD_HANDLE;
        };
        if file.flags & crate::syscall::open_flags::WRITE == 0 {
            return crate::syscall::error::PERMISSION;
        }
        let Some(disk) = self
            .folder_disk
            .as_ref()
            .or(self.program_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if let Err(error) = self.check_account_disk_access(task_id, disk, &file.path, false, 0o2) {
            return error;
        }
        let offset = if file.flags & crate::syscall::open_flags::APPEND != 0 {
            match disk.metadata(&file.path) {
                Ok(metadata) => metadata.size_bytes,
                Err(_) => return crate::syscall::error::IO,
            }
        } else {
            file.offset
        };
        if disk.write_file_at(&file.path, offset, &data).is_err() {
            return crate::syscall::error::IO;
        }
        if let Some(file) = self.syscall_state.file_mut(task_id, handle) {
            file.offset = offset.saturating_add(length as u64);
        }
        length as i64
    }

    fn syscall_close(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let handle = match u32::try_from(self.registers[1]) {
            Ok(handle) => handle,
            Err(_) => return crate::syscall::error::BAD_HANDLE,
        };
        if handle < 3 || !self.syscall_state.close_file(task_id, handle) {
            crate::syscall::error::BAD_HANDLE
        } else {
            0
        }
    }

    fn syscall_seek(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let handle = match u32::try_from(self.registers[1]) {
            Ok(handle) if handle >= 3 => handle,
            _ => return crate::syscall::error::BAD_HANDLE,
        };
        let high = match self.registers[2] {
            value @ -32_768..=65_535 => value as u16,
            _ => return crate::syscall::error::INVALID,
        };
        let low = match self.registers[3] {
            value @ -32_768..=65_535 => value as u16,
            _ => return crate::syscall::error::INVALID,
        };
        let origin = match self.registers[4] {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => return crate::syscall::error::INVALID,
        };
        let Some(file) = self.syscall_state.file(task_id, handle).cloned() else {
            return crate::syscall::error::BAD_HANDLE;
        };
        let base = match origin {
            0 => 0,
            1 => match i64::try_from(file.offset) {
                Ok(offset) => offset,
                Err(_) => return crate::syscall::error::OVERFLOW,
            },
            2 => {
                let Some(disk) = self
                    .folder_disk
                    .as_ref()
                    .or(self.program_folder_disk.as_ref())
                else {
                    return crate::syscall::error::IO;
                };
                match disk.metadata(&file.path) {
                    Ok(metadata) => match i64::try_from(metadata.size_bytes) {
                        Ok(size) => size,
                        Err(_) => return crate::syscall::error::OVERFLOW,
                    },
                    Err(_) => return crate::syscall::error::IO,
                }
            }
            _ => unreachable!(),
        };
        let offset_bits = (u32::from(high) << 16) | u32::from(low);
        let offset = i64::from(i32::from_be_bytes(offset_bits.to_be_bytes()));
        let Some(position) = base
            .checked_add(offset)
            .filter(|position| (0..=i64::from(i32::MAX)).contains(position))
        else {
            return crate::syscall::error::INVALID;
        };
        let Some(file) = self.syscall_state.file_mut(task_id, handle) else {
            return crate::syscall::error::BAD_HANDLE;
        };
        file.offset = position as u64;
        position
    }

    fn syscall_scnet(&mut self, task_id: Option<TaskId>) -> i64 {
        if task_id.is_none() {
            return crate::syscall::error::INVALID;
        }
        if self.scnet.is_none() && self.ensure_scnet_node(std::time::Duration::ZERO).is_err() {
            return crate::syscall::error::IO;
        }
        let operation = self.registers[1];
        match operation {
            0 => {
                let address = match u32::try_from(self.registers[2]) {
                    Ok(address) => address,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let capacity = match usize::try_from(self.registers[3]) {
                    Ok(capacity) if capacity <= 256 => capacity,
                    _ => return crate::syscall::error::INVALID,
                };
                let byte_count = capacity * crate::sc_net::NAME_CAPACITY;
                let destinations = match self.guest_memory_addresses(
                    address,
                    byte_count,
                    crate::mmu::MemoryAccess::Write,
                    true,
                ) {
                    Ok(destinations) => destinations,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let node = self.scnet.as_mut().expect("SC-NET node initialized");
                if node.poll().is_err() {
                    return crate::syscall::error::IO;
                }
                if node.has_name_collision() {
                    return crate::syscall::error::BUSY;
                }
                let mut peers = node.peers();
                peers.sort();
                if peers.len() > capacity {
                    return crate::syscall::error::OVERFLOW;
                }
                for (index, name) in peers.iter().enumerate() {
                    let mut encoded = [0; crate::sc_net::NAME_CAPACITY];
                    encoded[..name.len()].copy_from_slice(name.as_bytes());
                    for (offset, byte) in encoded.into_iter().enumerate() {
                        self.memory[destinations[index * crate::sc_net::NAME_CAPACITY + offset]] =
                            byte;
                    }
                }

                peers.len() as i64
            }
            1 => {
                let name_address = match u32::try_from(self.registers[2]) {
                    Ok(address) => address,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let data_address = match u32::try_from(self.registers[3]) {
                    Ok(address) => address,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let length = match usize::try_from(self.registers[4]) {
                    Ok(length) if length <= crate::sc_net::MAX_PAYLOAD => length,
                    _ => return crate::syscall::error::INVALID,
                };
                let recipient = match self.read_guest_path(name_address, true) {
                    Ok(Some(name))
                        if name.len() <= crate::sc_net::NAME_CAPACITY && !name.is_empty() =>
                    {
                        name
                    }
                    _ => return crate::syscall::error::INVALID,
                };
                let sources = match self.guest_memory_addresses(
                    data_address,
                    length,
                    crate::mmu::MemoryAccess::Read,
                    true,
                ) {
                    Ok(sources) => sources,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let payload = sources
                    .iter()
                    .map(|address| self.memory[*address])
                    .collect::<Vec<_>>();
                let node = self.scnet.as_mut().expect("SC-NET node initialized");
                if node.poll().is_err() {
                    return crate::syscall::error::IO;
                }
                if !node.peers().iter().any(|peer| peer == &recipient) {
                    return crate::syscall::error::NO_ENTRY;
                }
                match node.send(&recipient, &payload) {
                    Ok(()) => length as i64,
                    Err(_) => crate::syscall::error::IO,
                }
            }
            2 => {
                let name_address = match u32::try_from(self.registers[2]) {
                    Ok(address) => address,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let data_address = match u32::try_from(self.registers[3]) {
                    Ok(address) => address,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let capacity = match usize::try_from(self.registers[4]) {
                    Ok(capacity) if capacity <= crate::sc_net::MAX_PAYLOAD => capacity,
                    _ => return crate::syscall::error::INVALID,
                };
                let name_destinations = match self.guest_memory_addresses(
                    name_address,
                    crate::sc_net::NAME_CAPACITY,
                    crate::mmu::MemoryAccess::Write,
                    true,
                ) {
                    Ok(destinations) => destinations,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let data_destinations = match self.guest_memory_addresses(
                    data_address,
                    capacity,
                    crate::mmu::MemoryAccess::Write,
                    true,
                ) {
                    Ok(destinations) => destinations,
                    Err(_) => return crate::syscall::error::BAD_ADDRESS,
                };
                let node = self.scnet.as_mut().expect("SC-NET node initialized");
                if node.poll().is_err() {
                    return crate::syscall::error::IO;
                }
                if node.has_name_collision() {
                    return crate::syscall::error::BUSY;
                }
                let (sender, payload) = match node.receive(capacity) {
                    Ok(Some(message)) => message,
                    Ok(None) => return crate::syscall::error::TRY_AGAIN,
                    Err(_) => return crate::syscall::error::OVERFLOW,
                };
                let mut encoded_name = [0; crate::sc_net::NAME_CAPACITY];
                encoded_name[..sender.len()].copy_from_slice(sender.as_bytes());
                for (address, byte) in name_destinations.into_iter().zip(encoded_name) {
                    self.memory[address] = byte;
                }
                for (address, byte) in data_destinations.into_iter().zip(payload.iter().copied()) {
                    self.memory[address] = byte;
                }
                payload.len() as i64
            }
            _ => crate::syscall::error::INVALID,
        }
    }

    fn syscall_graphics(&mut self, task_id: Option<TaskId>) -> i64 {
        use crate::machine::*;

        if task_id.is_none() || self.registers[5] != 0 || self.registers[6] != 0 {
            return crate::syscall::error::INVALID;
        }
        let Ok(operation) = u8::try_from(self.registers[1]) else {
            return crate::syscall::error::INVALID;
        };
        let Ok(parameters_address) = u32::try_from(self.registers[2]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let Ok(data_address) = u32::try_from(self.registers[3]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let Ok(data_length) = usize::try_from(self.registers[4]) else {
            return crate::syscall::error::INVALID;
        };
        let valid_data_length = match operation {
            GRAPHICS_OP_BLIT => (1..=GRAPHICS_DATA_SIZE).contains(&data_length),
            GRAPHICS_OP_PALETTE => data_length == 3,
            GRAPHICS_OP_CLEAR | GRAPHICS_OP_PIXEL | GRAPHICS_OP_RECT | GRAPHICS_OP_LINE
            | GRAPHICS_OP_PRESENT | GRAPHICS_OP_MODE => data_length == 0,
            _ => false,
        };
        if !valid_data_length {
            return crate::syscall::error::INVALID;
        }

        let parameter_addresses = match self.guest_memory_addresses(
            parameters_address,
            10,
            crate::mmu::MemoryAccess::Read,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let mut parameters = [0; 10];
        for (destination, source) in parameters.iter_mut().zip(parameter_addresses) {
            *destination = self.memory[source];
        }

        let mut data = [0; GRAPHICS_DATA_SIZE];
        if data_length != 0 {
            let data_addresses = match self.guest_memory_addresses(
                data_address,
                data_length,
                crate::mmu::MemoryAccess::Read,
                true,
            ) {
                Ok(addresses) => addresses,
                Err(_) => return crate::syscall::error::BAD_ADDRESS,
            };
            for (destination, source) in data.iter_mut().zip(data_addresses) {
                *destination = self.memory[source];
            }
        }

        let read_u16 =
            |offset: usize| u16::from_be_bytes([parameters[offset], parameters[offset + 1]]);
        let request = GraphicsRequest {
            operation,
            color: parameters[0],
            mode: parameters[1],
            x: read_u16(2),
            y: read_u16(4),
            width: read_u16(6),
            height: read_u16(8),
            length: data_length as u16,
            data: &data,
        };
        self.execute_graphics_request(request)
            .map(i64::from)
            .unwrap_or(crate::syscall::error::INVALID)
    }

    fn syscall_input_snapshot(&mut self, task_id: Option<TaskId>) -> i64 {
        if task_id.is_none() || self.registers[2..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let Ok(destination) = u32::try_from(self.registers[1]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let destinations = match self.guest_memory_addresses(
            destination,
            crate::machine::INPUT_SNAPSHOT_SIZE,
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(destinations) => destinations,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let snapshot = self.input.take_snapshot();
        for (address, byte) in destinations.into_iter().zip(snapshot) {
            self.memory[address] = byte;
        }
        crate::machine::INPUT_SNAPSHOT_SIZE as i64
    }

    fn syscall_account_status(&self, task_id: Option<TaskId>) -> i64 {
        if task_id.is_none() {
            return crate::syscall::error::INVALID;
        }
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        match crate::accounts::AccountStore::load(disk) {
            Ok(store)
                if store
                    .user("root")
                    .is_some_and(|root| root.password_hash.is_some()) =>
            {
                1
            }
            Ok(store) if store.user("root").is_some() => 0,
            Ok(_) => crate::syscall::error::IO,
            Err(_) => crate::syscall::error::IO,
        }
    }

    fn syscall_initialize_root_password(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[2..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        if !self
            .syscall_state
            .processes
            .get(&task_id)
            .is_some_and(|process| process.bootstrap_authority)
        {
            return crate::syscall::error::PERMISSION;
        }
        let Ok(password_address) = u32::try_from(self.registers[1]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let password = match self.read_guest_path(password_address, true) {
            Ok(Some(password)) => password,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        if accounts.initialize_root_password(&password).is_err() {
            return crate::syscall::error::INVALID;
        }
        if accounts.save(disk).is_err() {
            return crate::syscall::error::IO;
        }
        self.syscall_state
            .processes
            .get_mut(&task_id)
            .expect("task account record exists")
            .bootstrap_authority = false;
        0
    }

    fn syscall_authenticate(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[3..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let (Ok(name_address), Ok(password_address)) = (
            u32::try_from(self.registers[1]),
            u32::try_from(self.registers[2]),
        ) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let name = match self.read_guest_path(name_address, true) {
            Ok(Some(name)) => name,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let password = match self.read_guest_path(password_address, true) {
            Ok(Some(password)) => password,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        let accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        let Some(user) = accounts.authenticate(&name, &password).cloned() else {
            return crate::syscall::error::PERMISSION;
        };
        let process = self
            .syscall_state
            .processes
            .get_mut(&task_id)
            .expect("task account record exists");
        process.credentials = Some(crate::syscall::ProcessCredentials {
            uid: user.uid,
            primary_gid: user.primary_gid,
            supplementary_gids: user.supplementary_gids,
            home: user.home,
        });
        process.bootstrap_authority = false;
        i64::from(user.uid)
    }

    fn syscall_set_console_echo(&mut self, task_id: Option<TaskId>) -> i64 {
        if task_id.is_none() {
            return crate::syscall::error::INVALID;
        }
        let echo = match self.registers[1] {
            0 => false,
            1 => true,
            _ => return crate::syscall::error::INVALID,
        };
        let control = self.console.control();
        self.console.set_control((control & !1) | u8::from(echo));
        0
    }

    fn account_request_strings(
        &mut self,
        addresses: &[i64],
        argument_count: usize,
    ) -> Result<Vec<String>, i64> {
        if addresses.len() != argument_count || argument_count == 0 || argument_count > 3 {
            return Err(crate::syscall::error::INVALID);
        }
        let mut strings = Vec::with_capacity(argument_count);
        for address in addresses {
            let address = u32::try_from(*address).map_err(|_| {
                self.debug_console.log_error(
                    "account",
                    "guest account-string address is outside the address range",
                );
                crate::syscall::error::BAD_ADDRESS
            })?;
            match self.read_guest_path(address, true) {
                Ok(Some(value)) => strings.push(value),
                Ok(None) => return Err(crate::syscall::error::INVALID),
                Err(error) => {
                    self.debug_console.log_error(
                        "account",
                        &format!("guest account string at {address:#x}: {error}"),
                    );
                    return Err(crate::syscall::error::BAD_ADDRESS);
                }
            }
        }
        Ok(strings)
    }

    fn is_root_process(&self, task_id: TaskId) -> bool {
        self.syscall_state
            .processes
            .get(&task_id)
            .and_then(|process| process.credentials.as_ref())
            .is_some_and(|credentials| credentials.uid == 0)
    }

    fn syscall_add_group(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if !self.is_root_process(task_id) {
            return crate::syscall::error::PERMISSION;
        }
        let name = match self.account_request_strings(&[self.registers[1]], 1) {
            Ok(mut values) => values.remove(0),
            Err(error) => return error,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        let gid = match accounts.add_group(&name) {
            Ok(gid) => gid,
            Err(crate::accounts::AccountError::NameInUse) => {
                return crate::syscall::error::INVALID;
            }
            Err(_) => return crate::syscall::error::INVALID,
        };
        if accounts.save(disk).is_err() {
            return crate::syscall::error::IO;
        }
        i64::from(gid)
    }

    fn syscall_add_user(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if !self.is_root_process(task_id) {
            return crate::syscall::error::PERMISSION;
        }
        let values = match self.account_request_strings(
            &[self.registers[1], self.registers[2], self.registers[3]],
            3,
        ) {
            Ok(values) => values,
            Err(error) => return error,
        };
        let [name, group, home]: [String; 3] = match values.try_into() {
            Ok(values) => values,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        let gid = match accounts.group(&group) {
            Some(group) => group.gid,
            None => return crate::syscall::error::NO_ENTRY,
        };
        let uid = match accounts.add_user(&name, &group, &home) {
            Ok(uid) => uid,
            Err(crate::accounts::AccountError::NameInUse) => {
                return crate::syscall::error::INVALID;
            }
            Err(_) => return crate::syscall::error::INVALID,
        };
        if disk.metadata(&home).is_ok() {
            return crate::syscall::error::INVALID;
        }
        let parent = home.rsplit_once('/').map_or("", |(parent, _)| parent);
        let mut current = String::new();
        for component in parent.split('/').filter(|component| !component.is_empty()) {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            match disk.metadata(&current) {
                Ok(entry) if entry.is_directory => {}
                Ok(_) => return crate::syscall::error::INVALID,
                Err(_) if disk.create_dir(&current).is_ok() => {}
                Err(_) => return crate::syscall::error::IO,
            }
        }
        if disk.create_dir(&home).is_err() {
            return crate::syscall::error::IO;
        }
        if accounts
            .set_file_owner(&home, true, 0, uid, gid, &[0])
            .and_then(|()| accounts.set_file_mode(&home, true, 0, 0o700))
            .is_err()
            || accounts.save(disk).is_err()
        {
            let _ = disk.delete_entry(&home);
            return crate::syscall::error::IO;
        }
        i64::from(uid)
    }

    fn syscall_set_password(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if self.registers[3..=6].iter().any(|argument| *argument != 0) {
            return crate::syscall::error::INVALID;
        }
        let values = match self.account_request_strings(&[self.registers[1], self.registers[2]], 2)
        {
            Ok(values) => values,
            Err(error) => return error,
        };
        let [name, password]: [String; 2] = match values.try_into() {
            Ok(values) => values,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let Some(process) = self.syscall_state.processes.get(&task_id) else {
            return crate::syscall::error::INVALID;
        };
        let Some(credentials) = process.credentials.as_ref() else {
            return crate::syscall::error::PERMISSION;
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        let mut accounts = match crate::accounts::AccountStore::load(disk) {
            Ok(accounts) => accounts,
            Err(_) => return crate::syscall::error::IO,
        };
        if credentials.uid != 0
            && accounts.user(&name).map(|user| user.uid) != Some(credentials.uid)
        {
            return crate::syscall::error::PERMISSION;
        }
        if accounts.set_password(&name, &password).is_err() {
            return crate::syscall::error::INVALID;
        }
        if accounts.save(disk).is_err() {
            return crate::syscall::error::IO;
        }
        0
    }

    fn ensure_page_memory_manager(&mut self) -> Result<(), crate::mmu::PageTableError> {
        if self.page_memory_manager.is_some() {
            return Ok(());
        }
        let mut manager = crate::mmu::PageMemoryManager::new(self.memory.len(), 0x8000)?;
        for segment in &self.loaded_image_segments {
            manager.reserve_physical_range(segment.address as usize, segment.length)?;
        }
        for task in self
            .scheduler
            .tasks()
            .iter()
            .filter(|task| task.mmu_enabled)
        {
            manager.adopt_address_space(&self.memory, task.page_directory)?;
        }
        if let Some(idt_base) = self.idt_base {
            manager.reserve_physical_range(idt_base as usize, crate::interrupts::IDT_BYTES)?;
            if let Ok(descriptor) = self.idt_descriptor(crate::isa::SVC_VECTOR) {
                manager.reserve_physical_range(descriptor.handler as usize, 1)?;
            }
        }
        let stack_end = self
            .sp
            .checked_add(EXCEPTION_ENTRY_BYTES)
            .unwrap_or(self.sp)
            .min(self.memory.len() as u32);
        let stack_start = stack_end.saturating_sub(16 * crate::mmu::PAGE_SIZE as u32);
        manager.reserve_physical_range(stack_start as usize, (stack_end - stack_start) as usize)?;
        self.page_memory_manager = Some(manager);
        Ok(())
    }

    fn map_spawn_kernel_page(
        &mut self,
        root: u32,
        virtual_page: u32,
        permissions: crate::mmu::PagePermissions,
        mapped: &mut HashMap<u32, crate::mmu::PagePermissions>,
    ) -> Result<(), crate::mmu::PageTableError> {
        if let Some(existing) = mapped.get_mut(&virtual_page) {
            existing.writable |= permissions.writable;
            existing.executable |= permissions.executable;
            return self
                .page_memory_manager
                .as_mut()
                .ok_or(crate::mmu::PageTableError::InvalidAddressSpace)?
                .set_permissions(&mut self.memory, root, virtual_page, *existing);
        }
        let frame = virtual_page / crate::mmu::PAGE_SIZE as u32;
        self.page_memory_manager
            .as_mut()
            .ok_or(crate::mmu::PageTableError::InvalidAddressSpace)?
            .map_borrowed_page(&mut self.memory, root, virtual_page, frame, permissions)?;
        mapped.insert(virtual_page, permissions);
        Ok(())
    }

    fn syscall_argument(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(record) = task_id.and_then(|id| self.syscall_state.processes.get(&id)) else {
            return crate::syscall::error::INVALID;
        };
        let Ok(index) = usize::try_from(self.registers[1]) else {
            return crate::syscall::error::INVALID;
        };
        let Some(argument) = record.arguments.get(index) else {
            return crate::syscall::error::INVALID;
        };
        let Ok(capacity) = usize::try_from(self.registers[3]) else {
            return crate::syscall::error::INVALID;
        };
        if capacity > crate::syscall::MAX_TRANSFER_BYTES {
            return crate::syscall::error::INVALID;
        }
        if capacity <= argument.len() {
            return crate::syscall::error::OVERFLOW;
        }
        let Ok(destination) = u32::try_from(self.registers[2]) else {
            return crate::syscall::error::BAD_ADDRESS;
        };
        let bytes: Vec<u8> = argument.bytes().chain(std::iter::once(0)).collect();
        let addresses = match self.guest_memory_addresses(
            destination,
            bytes.len(),
            crate::mmu::MemoryAccess::Write,
            true,
        ) {
            Ok(addresses) => addresses,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let length = bytes.len() - 1;
        for (address, byte) in addresses.into_iter().zip(bytes) {
            self.memory[address] = byte;
        }
        length as i64
    }

    fn syscall_spawn(&mut self, parent: Option<TaskId>) -> i64 {
        let argument = if self.registers[0] == SyscallNumber::SpawnArgument as i64 {
            match u32::try_from(self.registers[2]) {
                Ok(argument) if argument <= ADDRESS_MASK => argument,
                _ => return crate::syscall::error::INVALID,
            }
        } else {
            0
        };
        let Some(parent) = parent else {
            return crate::syscall::error::INVALID;
        };
        if !self.mmu_enabled {
            return crate::syscall::error::PERMISSION;
        }
        let path_address = match u32::try_from(self.registers[1]) {
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let path = match self.read_guest_path(path_address, true) {
            Ok(Some(path)) => path,
            Ok(None) => return crate::syscall::error::INVALID,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        let arguments = if self.registers[0] == SyscallNumber::SpawnExec as i64 {
            let Ok(length) = usize::try_from(self.registers[3]) else {
                return crate::syscall::error::INVALID;
            };
            let Ok(count) = usize::try_from(self.registers[4]) else {
                return crate::syscall::error::INVALID;
            };
            if length == 0
                || length > crate::syscall::MAX_ARGUMENT_BYTES
                || count == 0
                || count > crate::syscall::MAX_ARGUMENTS
            {
                return crate::syscall::error::INVALID;
            }
            let Ok(address) = u32::try_from(self.registers[2]) else {
                return crate::syscall::error::BAD_ADDRESS;
            };
            let addresses = match self.guest_memory_addresses(
                address,
                length,
                crate::mmu::MemoryAccess::Read,
                true,
            ) {
                Ok(addresses) => addresses,
                Err(_) => return crate::syscall::error::BAD_ADDRESS,
            };
            let bytes: Vec<u8> = addresses
                .into_iter()
                .map(|address| self.memory[address])
                .collect();
            match crate::syscall::decode_arguments(&bytes, count) {
                Ok(arguments) => arguments,
                Err(error) => return error,
            }
        } else {
            vec![path.clone()]
        };
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
        if let Err(error) = self.check_account_disk_access(parent, disk, &path, false, 0o5) {
            return error;
        }
        let image = match disk.load_executable(&path) {
            Ok(image) => image,
            Err(_) => return crate::syscall::error::NO_ENTRY,
        };
        if image.entry_point >= crate::mmu::ADDRESS_SPACE_SIZE {
            return crate::syscall::error::INVALID;
        }
        if self.ensure_page_memory_manager().is_err() {
            return crate::syscall::error::NO_MEMORY;
        }
        let root = match self
            .page_memory_manager
            .as_mut()
            .expect("memory manager initialized")
            .create_address_space(&mut self.memory)
        {
            Ok(root) => root,
            Err(_) => return crate::syscall::error::NO_MEMORY,
        };
        let mut mapped_kernel = HashMap::new();
        let mut result = Ok(());
        let kernel_segments = self.loaded_image_segments.clone();
        for segment in &kernel_segments {
            let first = segment.address & !crate::mmu::PAGE_MASK;
            let end = match segment
                .address
                .checked_add(segment.length as u32)
                .and_then(|end| end.checked_add(crate::mmu::PAGE_MASK))
            {
                Some(end) => end & !crate::mmu::PAGE_MASK,
                None => {
                    result = Err(crate::mmu::PageTableError::InvalidVirtualAddress);
                    break;
                }
            };
            let permissions = crate::mmu::PagePermissions {
                writable: segment.permissions.write,
                user: false,
                executable: segment.permissions.execute,
            };
            let mut page = first;
            while page < end {
                if let Err(error) =
                    self.map_spawn_kernel_page(root, page, permissions, &mut mapped_kernel)
                {
                    result = Err(error);
                    break;
                }
                page += crate::mmu::PAGE_SIZE as u32;
            }
            if result.is_err() {
                break;
            }
        }
        if result.is_ok() {
            if let Some(idt_base) = self.idt_base {
                let idt_page = idt_base & !crate::mmu::PAGE_MASK;
                let end = idt_page + crate::mmu::PAGE_SIZE as u32;
                let mut page = idt_page;
                while page < end {
                    if !mapped_kernel.contains_key(&page) {
                        result = self.map_spawn_kernel_page(
                            root,
                            page,
                            crate::mmu::PagePermissions::KERNEL_DATA,
                            &mut mapped_kernel,
                        );
                        if result.is_err() {
                            break;
                        }
                    }
                    page += crate::mmu::PAGE_SIZE as u32;
                }
                if let Ok(descriptor) = self.idt_descriptor(crate::isa::SVC_VECTOR) {
                    let page = descriptor.handler & !crate::mmu::PAGE_MASK;
                    if result.is_ok() {
                        result = self.map_spawn_kernel_page(
                            root,
                            page,
                            crate::mmu::PagePermissions::KERNEL_CODE,
                            &mut mapped_kernel,
                        );
                    }
                }
            } else {
                result = Err(crate::mmu::PageTableError::InvalidAddressSpace);
            }
        }
        let kernel_sp = self.sp.saturating_add(EXCEPTION_ENTRY_BYTES);
        if result.is_ok() {
            let kernel_start = kernel_sp.saturating_sub(16 * crate::mmu::PAGE_SIZE as u32)
                & !crate::mmu::PAGE_MASK;
            let kernel_end = kernel_sp
                .checked_add(crate::mmu::PAGE_MASK)
                .unwrap_or(kernel_sp)
                & !crate::mmu::PAGE_MASK;
            let mut page = kernel_start;
            while page < kernel_end && page < self.memory.len() as u32 {
                if !mapped_kernel.contains_key(&page) {
                    result = self.map_spawn_kernel_page(
                        root,
                        page,
                        crate::mmu::PagePermissions::KERNEL_DATA,
                        &mut mapped_kernel,
                    );
                    if result.is_err() {
                        break;
                    }
                }
                page += crate::mmu::PAGE_SIZE as u32;
            }
        }

        let mut mapped_user = HashMap::<u32, crate::mmu::PagePermissions>::new();
        if result.is_ok() {
            for segment in &image.segments {
                let first = segment.address & !crate::mmu::PAGE_MASK;
                let end = match segment
                    .address
                    .checked_add(segment.data.len() as u32)
                    .and_then(|end| end.checked_add(crate::mmu::PAGE_MASK))
                {
                    Some(end) => end & !crate::mmu::PAGE_MASK,
                    None => {
                        result = Err(crate::mmu::PageTableError::InvalidVirtualAddress);
                        break;
                    }
                };
                let mut page = first;
                while page < end {
                    if !mapped_user.contains_key(&page) {
                        let permissions = crate::mmu::PagePermissions::USER_DATA;
                        if self
                            .page_memory_manager
                            .as_mut()
                            .expect("memory manager initialized")
                            .allocate_and_map_page(&mut self.memory, root, page, permissions)
                            .is_err()
                        {
                            result = Err(crate::mmu::PageTableError::AlreadyMapped);
                            break;
                        }
                        mapped_user.insert(page, permissions);
                    }
                    page += crate::mmu::PAGE_SIZE as u32;
                }
                if result.is_err() {
                    break;
                }
                for (offset, byte) in segment.data.iter().copied().enumerate() {
                    let virtual_address = segment.address + offset as u32;
                    match crate::mmu::translate(
                        &self.memory,
                        root,
                        virtual_address,
                        crate::mmu::MemoryAccess::Write,
                        false,
                    ) {
                        Ok(physical) => self.memory[physical as usize] = byte,
                        Err(fault) => {
                            self.record_page_fault(fault);
                            result = Err(crate::mmu::PageTableError::InvalidVirtualAddress);
                            break;
                        }
                    }
                }
                if result.is_err() {
                    break;
                }
                let segment_permissions = crate::mmu::PagePermissions {
                    writable: segment.permissions.write,
                    user: true,
                    executable: segment.permissions.execute,
                };
                let mut page = first;
                while page < end {
                    let mut page_permissions = segment_permissions;
                    if let Some(existing) = mapped_user.get(&page) {
                        page_permissions.writable |= existing.writable;
                        page_permissions.executable |= existing.executable;
                    }
                    if let Err(error) = self
                        .page_memory_manager
                        .as_mut()
                        .expect("memory manager initialized")
                        .set_permissions(&mut self.memory, root, page, page_permissions)
                    {
                        result = Err(error);
                        break;
                    }
                    mapped_user.insert(page, page_permissions);
                    page += crate::mmu::PAGE_SIZE as u32;
                }
                if result.is_err() {
                    break;
                }
            }
        }
        let user_stack_top = crate::machine::DEVICE_APERTURE_START - 0x1_0000;
        let user_stack_page = user_stack_top - crate::mmu::PAGE_SIZE as u32;
        if result.is_ok()
            && !mapped_user.contains_key(&user_stack_page)
            && !mapped_kernel.contains_key(&user_stack_page)
        {
            result = self
                .page_memory_manager
                .as_mut()
                .expect("memory manager initialized")
                .allocate_and_map_page(
                    &mut self.memory,
                    root,
                    user_stack_page,
                    crate::mmu::PagePermissions::USER_DATA,
                )
                .map(|_| ());
        } else if result.is_ok() {
            result = Err(crate::mmu::PageTableError::AlreadyMapped);
        }
        if result.is_ok()
            && self
                .validate_user_mapping_in(root, image.entry_point, user_stack_top)
                .is_err()
        {
            result = Err(crate::mmu::PageTableError::InvalidVirtualAddress);
        }
        if let Err(error) = result {
            let _ = self
                .page_memory_manager
                .as_mut()
                .expect("memory manager initialized")
                .destroy_address_space(&mut self.memory, root);
            return match error {
                crate::mmu::PageTableError::OutOfFrames => crate::syscall::error::NO_MEMORY,
                _ => crate::syscall::error::INVALID,
            };
        }
        let mut context =
            TaskContext::user(image.entry_point, user_stack_top, kernel_sp, true, root);
        context.registers[1] = i64::from(parent);
        let id = match self.scheduler.create_task(context) {
            Ok(id) => id,
            Err(_) => {
                let _ = self
                    .page_memory_manager
                    .as_mut()
                    .expect("memory manager initialized")
                    .destroy_address_space(&mut self.memory, root);
                return crate::syscall::error::NO_MEMORY;
            }
        };
        let parent_credentials = self
            .syscall_state
            .processes
            .get(&parent)
            .and_then(|process| process.credentials.clone());
        let bootstrap_authority = self
            .syscall_state
            .processes
            .get(&parent)
            .is_some_and(|process| process.bootstrap_authority);
        self.syscall_state.register_process(id, Some(parent));
        let record = self
            .syscall_state
            .processes
            .get_mut(&id)
            .expect("registered child");
        record.startup_argument = argument;
        record.arguments = arguments;
        record.name = path;
        record.credentials = parent_credentials;
        record.bootstrap_authority = bootstrap_authority;
        if bootstrap_authority {
            self.syscall_state
                .processes
                .get_mut(&parent)
                .expect("parent account record exists")
                .bootstrap_authority = false;
        }
        i64::from(id)
    }

    fn syscall_mmap(&mut self, task_id: Option<TaskId>) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        if !self.mmu_enabled {
            return crate::syscall::error::PERMISSION;
        }
        let length = match usize::try_from(self.registers[1]) {
            Ok(length) if length > 0 && length <= 16 * 1024 * 1024 => length,
            _ => return crate::syscall::error::INVALID,
        };
        let flags = match u32::try_from(self.registers[2]) {
            Ok(flags) if flags & !7 == 0 && flags & 1 != 0 => flags,
            _ => return crate::syscall::error::INVALID,
        };
        let page_count = length.div_ceil(crate::mmu::PAGE_SIZE);
        let rounded_length = match page_count.checked_mul(crate::mmu::PAGE_SIZE) {
            Some(length) => length as u32,
            None => return crate::syscall::error::OVERFLOW,
        };
        let mut address = match u32::try_from(self.registers[3]) {
            Ok(0) => {
                self.syscall_state
                    .processes
                    .entry(task_id)
                    .or_default()
                    .next_mmap
            }
            Ok(address) => address,
            Err(_) => return crate::syscall::error::BAD_ADDRESS,
        };
        if address & crate::mmu::PAGE_MASK != 0 {
            return crate::syscall::error::INVALID;
        }
        let end = match address.checked_add(rounded_length) {
            Some(end) if end <= crate::machine::DEVICE_APERTURE_START => end,
            _ => return crate::syscall::error::OVERFLOW,
        };
        let root = self.page_directory;
        if self.page_memory_manager.is_none() {
            return crate::syscall::error::NOT_IMPLEMENTED;
        }
        let permissions = crate::mmu::PagePermissions {
            writable: flags & 2 != 0,
            user: true,
            executable: flags & 4 != 0,
        };
        let mut mapped = Vec::new();
        while address < end {
            let allocation = self
                .page_memory_manager
                .as_mut()
                .expect("manager checked")
                .allocate_and_map_page(&mut self.memory, root, address, permissions);
            if let Err(error) = allocation {
                for mapped_address in mapped {
                    let _ = self
                        .page_memory_manager
                        .as_mut()
                        .expect("manager checked")
                        .unmap_page(&mut self.memory, root, mapped_address);
                }
                return match error {
                    crate::mmu::PageTableError::OutOfFrames => crate::syscall::error::NO_MEMORY,
                    _ => crate::syscall::error::INVALID,
                };
            }
            mapped.push(address);
            address += crate::mmu::PAGE_SIZE as u32;
        }
        self.syscall_state
            .processes
            .entry(task_id)
            .or_default()
            .next_mmap = end;
        i64::from(mapped[0])
    }

    fn syscall_sleep(
        &mut self,
        task_id: Option<TaskId>,
        scheduled_state: &mut Option<(TaskState, i32)>,
    ) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let ticks = match u64::try_from(self.registers[1]) {
            Ok(ticks) if ticks <= 1_000_000_000 => ticks,
            _ => return crate::syscall::error::INVALID,
        };
        if ticks == 0 {
            return 0;
        }
        let Some(deadline) = self.syscall_state.ticks.checked_add(ticks) else {
            return crate::syscall::error::OVERFLOW;
        };
        self.syscall_state.sleeping.insert(task_id, deadline);
        scheduled_state.replace((TaskState::Blocked, 0));
        0
    }

    fn syscall_sleep_millis(
        &mut self,
        task_id: Option<TaskId>,
        scheduled_state: &mut Option<(TaskState, i32)>,
    ) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let milliseconds = match u32::try_from(self.registers[1]) {
            Ok(milliseconds) if milliseconds <= 1_000_000_000 => milliseconds,
            _ => return crate::syscall::error::INVALID,
        };
        if milliseconds == 0 {
            return 0;
        }
        let started = self.monotonic_milliseconds() as u32;
        self.syscall_state
            .sleeping_millis
            .insert(task_id, (started, milliseconds));
        scheduled_state.replace((TaskState::Blocked, 0));
        0
    }

    fn syscall_wait(
        &mut self,
        task_id: Option<TaskId>,
        scheduled_state: &mut Option<(TaskState, i32)>,
    ) -> i64 {
        let nonblocking = self.registers[0] == SyscallNumber::TryWait as i64;
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let child_id = match u32::try_from(self.registers[1]) {
            Ok(id) => id,
            Err(_) => return crate::syscall::error::NO_CHILD,
        };
        let Some(parent) = self.syscall_state.processes.get(&task_id) else {
            return crate::syscall::error::NO_CHILD;
        };
        if !parent.children.contains(&child_id) {
            return crate::syscall::error::NO_CHILD;
        }
        let Some(child) = self.syscall_state.processes.get(&child_id) else {
            return crate::syscall::error::NO_CHILD;
        };
        if child.parent != Some(task_id) {
            return crate::syscall::error::NO_CHILD;
        }
        if let Some(exit_code) = child.exit_code {
            self.registers[1] = i64::from(child_id);
            self.reap_process(task_id, child_id);
            return i64::from(exit_code);
        }
        if nonblocking {
            return crate::syscall::error::TRY_AGAIN;
        }
        self.syscall_state
            .processes
            .get_mut(&child_id)
            .expect("child record checked")
            .waiters
            .push_back(task_id);
        self.syscall_state.waiting_for.insert(task_id, child_id);
        scheduled_state.replace((TaskState::Blocked, 0));
        0
    }

    fn syscall_semaphore(
        &mut self,
        task_id: Option<TaskId>,
        scheduled_state: &mut Option<(TaskState, i32)>,
    ) -> i64 {
        let Some(task_id) = task_id else {
            return crate::syscall::error::INVALID;
        };
        let operation = match u32::try_from(self.registers[1]) {
            Ok(operation) => operation,
            Err(_) => return crate::syscall::error::INVALID,
        };
        let semaphore_id = match u32::try_from(self.registers[2]) {
            Ok(id) => id,
            Err(_) => return crate::syscall::error::INVALID,
        };
        match operation {
            crate::syscall::semaphore_operation::CREATE => {
                let initial = match u32::try_from(self.registers[3]) {
                    Ok(value) => value,
                    Err(_) => return crate::syscall::error::INVALID,
                };
                self.syscall_state.next_semaphore =
                    match self.syscall_state.next_semaphore.checked_add(1) {
                        Some(id) => id,
                        None => return crate::syscall::error::OVERFLOW,
                    };
                let id = self.syscall_state.next_semaphore;
                self.syscall_state.semaphores.insert(
                    id,
                    crate::syscall::Semaphore {
                        owner: task_id,
                        count: initial,
                        waiters: Default::default(),
                    },
                );
                i64::from(id)
            }
            crate::syscall::semaphore_operation::WAIT => {
                let Some(semaphore) = self.syscall_state.semaphores.get_mut(&semaphore_id) else {
                    return crate::syscall::error::INVALID;
                };
                if semaphore.count > 0 {
                    semaphore.count -= 1;
                    0
                } else {
                    if !semaphore.waiters.contains(&task_id) {
                        semaphore.waiters.push_back(task_id);
                    }
                    scheduled_state.replace((TaskState::Blocked, 0));
                    0
                }
            }
            crate::syscall::semaphore_operation::POST => {
                let Some(semaphore) = self.syscall_state.semaphores.get_mut(&semaphore_id) else {
                    return crate::syscall::error::INVALID;
                };
                if let Some(waiter) = semaphore.waiters.pop_front() {
                    self.syscall_state.pending_results.insert(waiter, 0);
                    let _ = self.scheduler.wake(waiter);
                } else if let Some(count) = semaphore.count.checked_add(1) {
                    semaphore.count = count;
                } else {
                    return crate::syscall::error::OVERFLOW;
                }
                0
            }
            crate::syscall::semaphore_operation::DESTROY => {
                let Some(semaphore) = self.syscall_state.semaphores.get(&semaphore_id) else {
                    return crate::syscall::error::INVALID;
                };
                if semaphore.owner != task_id {
                    return crate::syscall::error::PERMISSION;
                }
                if !semaphore.waiters.is_empty() {
                    return crate::syscall::error::BUSY;
                }
                self.syscall_state.semaphores.remove(&semaphore_id);
                0
            }
            _ => crate::syscall::error::INVALID,
        }
    }

    fn reap_process(&mut self, parent: TaskId, child: TaskId) {
        if let Some(record) = self.syscall_state.processes.get_mut(&parent) {
            record.children.retain(|id| *id != child);
        }
        if let Some(context) = self.scheduler.task(child) {
            if context.mmu_enabled {
                if let Some(manager) = self.page_memory_manager.as_mut() {
                    let _ = manager.destroy_address_space(&mut self.memory, context.page_directory);
                }
            }
        }
        self.syscall_state.processes.remove(&child);
        self.syscall_state.process_instructions.remove(&child);
        self.syscall_state.open_files.remove(&child);
    }

    fn complete_process(&mut self, id: TaskId, exit_code: i32) {
        if let Some(record) = self.syscall_state.processes.get_mut(&id) {
            record.exit_code = Some(exit_code);
        }
        let waiters: Vec<_> = self
            .syscall_state
            .processes
            .get_mut(&id)
            .map(|record| record.waiters.drain(..).collect())
            .unwrap_or_default();
        for waiter in waiters {
            self.syscall_state
                .pending_wait_status
                .insert(waiter, (id, exit_code));
            self.syscall_state.waiting_for.remove(&waiter);
            let _ = self.scheduler.wake(waiter);
        }
        self.syscall_state.open_files.remove(&id);
        self.syscall_state.sleeping.remove(&id);
        self.syscall_state.sleeping_millis.remove(&id);
        self.syscall_state.waiting_for.remove(&id);
        self.syscall_state.pending_results.remove(&id);
        self.syscall_state.pending_wait_status.remove(&id);
        for record in self.syscall_state.processes.values_mut() {
            record.waiters.retain(|waiter| *waiter != id);
        }
        for semaphore in self.syscall_state.semaphores.values_mut() {
            semaphore.waiters.retain(|waiter| *waiter != id);
        }
        let owned_semaphores: Vec<_> = self
            .syscall_state
            .semaphores
            .iter()
            .filter_map(|(semaphore_id, semaphore)| {
                (semaphore.owner == id).then_some(*semaphore_id)
            })
            .collect();
        for semaphore_id in owned_semaphores {
            if let Some(semaphore) = self.syscall_state.semaphores.remove(&semaphore_id) {
                for waiter in semaphore.waiters {
                    self.syscall_state
                        .pending_results
                        .insert(waiter, crate::syscall::error::INTERRUPTED);
                    let _ = self.scheduler.wake(waiter);
                }
            }
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        if crate::machine::is_device_aperture(address) {
            assert_eq!(
                crate::machine::device_register_writable(address),
                Some(true),
                "host write to a read-only or unimplemented device register"
            );
            if self.write_terminal_register(address, value) {
                return;
            } else if (crate::machine::FIRMWARE_GATEWAY_BASE
                ..crate::machine::FIRMWARE_GATEWAY_BASE + crate::machine::DEVICE_BANK_SIZE)
                .contains(&address)
            {
                if address == crate::machine::FIRMWARE_COMMAND {
                    self.high_firmware_command(value);
                } else {
                    self.firmware_gateway.write(address, value);
                }
            } else if (crate::machine::FILESYSTEM_BASE
                ..crate::machine::FILESYSTEM_BASE + crate::machine::DEVICE_BANK_SIZE)
                .contains(&address)
            {
                if address == crate::machine::FILESYSTEM_COMMAND {
                    self.high_filesystem_command(value);
                } else {
                    self.filesystem_device.write(address, value);
                }
            } else if (crate::machine::GRAPHICS_BASE
                ..crate::machine::GRAPHICS_BASE + crate::machine::DEVICE_BANK_SIZE)
                .contains(&address)
            {
                if address == crate::machine::GRAPHICS_COMMAND {
                    self.high_graphics_command(value);
                } else {
                    self.graphics_device.write(address, value);
                }
            } else if (crate::machine::AUDIO_BASE
                ..crate::machine::AUDIO_BASE + crate::machine::DEVICE_BANK_SIZE)
                .contains(&address)
            {
                self.audio.write(address, value);
            } else if (crate::machine::NETWORK_BASE
                ..crate::machine::NETWORK_BASE + crate::machine::DEVICE_BANK_SIZE)
                .contains(&address)
            {
                if address == crate::machine::NETWORK_COMMAND {
                    let node = self
                        .network
                        .get_or_insert_with(|| NetNode::new(self.network_port));
                    if let Err(error) = self.network_registers.command(value, node) {
                        self.debug_console
                            .log_error("network", &format!("command failed: {error}"));
                    }
                } else {
                    self.network_registers.write(address, value);
                }
            } else if address == crate::machine::CLOCK_CAPTURE {
                if value & 1 != 0 {
                    self.clock
                        .capture_time(self.monotonic_milliseconds() as u32, self.frame_count);
                }
                if value & 2 != 0 {
                    let random = self.next_random();
                    self.clock.capture_random(random);
                }
            } else {
                self.interrupts.write(address, value);
            }
            return;
        }
        self.memory[address as usize] = value;
    }

    fn read_terminal_register(&mut self, address: u32) -> Option<u8> {
        match address {
            crate::machine::CONSOLE_DATA => {
                let value = self.console.read_data();
                if self.console.control() & 1 != 0 && value != 0 {
                    self.terminal_active = true;
                    self.terminal_dirty = true;
                }
                Some(value)
            }
            crate::machine::CONSOLE_STATUS => {
                if self.console.receive_buffer().is_empty() && self.running {
                    self.frame_yield = true;
                }
                Some(self.console.status())
            }
            crate::machine::CONSOLE_CONTROL => Some(self.console.control()),
            address
                if (crate::machine::FIRMWARE_TEXT_BASE..=crate::machine::FIRMWARE_TEXT_ENABLED)
                    .contains(&address) =>
            {
                self.guest_text
                    .read_register(address - crate::machine::FIRMWARE_TEXT_BASE)
            }
            _ => None,
        }
    }

    fn write_terminal_register(&mut self, address: u32, value: u8) -> bool {
        match address {
            crate::machine::CONSOLE_DATA => {
                self.console.write_data(value);
                if value != 0 {
                    self.terminal_active = true;
                    self.terminal_dirty = true;
                }
            }
            crate::machine::CONSOLE_STATUS => {
                if value & 4 != 0 {
                    self.console.clear_overrun();
                }
            }
            crate::machine::CONSOLE_CONTROL => self.console.set_control(value),
            address
                if (crate::machine::FIRMWARE_TEXT_BASE..=crate::machine::FIRMWARE_TEXT_COLUMN)
                    .contains(&address) =>
            {
                self.guest_text
                    .write_register(address - crate::machine::FIRMWARE_TEXT_BASE, value);
            }
            _ => return false,
        }
        true
    }

    fn fault_register_byte(&self, address: u32) -> Option<u8> {
        use crate::machine::*;
        let fault_address = self
            .last_page_fault
            .map_or(0, |fault| fault.virtual_address);
        match address {
            address if (FAULT_ADDRESS..FAULT_ADDRESS + 4).contains(&address) => {
                Some(fault_address.to_be_bytes()[(address - FAULT_ADDRESS) as usize])
            }
            address if (FAULT_INFO..FAULT_INFO + 2).contains(&address) => {
                Some(self.page_fault_info().to_be_bytes()[(address - FAULT_INFO) as usize])
            }
            FAULT_VALID => Some(u8::from(self.last_page_fault.is_some())),
            _ => None,
        }
    }

    fn page_fault_info(&self) -> u16 {
        let Some(fault) = self.last_page_fault else {
            return 0;
        };
        let reason = match fault.reason {
            crate::mmu::PageFaultReason::AddressOutOfRange => 1,
            crate::mmu::PageFaultReason::NotPresent => 2,
            crate::mmu::PageFaultReason::ReadOnly => 3,
            crate::mmu::PageFaultReason::SupervisorOnly => 4,
            crate::mmu::PageFaultReason::NotExecutable => 5,
            crate::mmu::PageFaultReason::MalformedEntry => 6,
            crate::mmu::PageFaultReason::PhysicalOutOfRange => 7,
        };
        let access = match fault.access {
            crate::mmu::MemoryAccess::Fetch => 0,
            crate::mmu::MemoryAccess::Read => 1,
            crate::mmu::MemoryAccess::Write => 2,
        };
        (access << 8) | reason
    }

    pub fn raise_rtc_interrupt(&mut self) {
        self.interrupts.control |= 8;
    }

    pub fn update_rtc(&mut self, timestamp: u64) {
        self.clock.rtc = [
            (timestamp % 60) as u8,
            ((timestamp / 60) % 60) as u8,
            ((timestamp / 3600) % 24) as u8,
        ];
        self.raise_rtc_interrupt();
    }

    fn next_random(&mut self) -> u32 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.random_state = value;
        value
    }

    pub fn read_u16(&mut self, address: u32) -> u16 {
        u16::from_be_bytes([self.read(address), self.read(address.wrapping_add(1))])
    }

    pub fn write_u16(&mut self, address: u32, value: u16) {
        let [high, low] = value.to_be_bytes();
        self.write(address, high);
        self.write(address.wrapping_add(1), low);
    }

    #[cfg(test)]
    fn read_u32(&mut self, address: u32) -> u32 {
        (u32::from(self.read(address)) << 24)
            | (u32::from(self.read(address.wrapping_add(1))) << 16)
            | (u32::from(self.read(address.wrapping_add(2))) << 8)
            | u32::from(self.read(address.wrapping_add(3)))
    }

    #[cfg(test)]
    fn write_u32(&mut self, address: u32, value: u32) {
        for offset in 0..4 {
            self.write(
                address.wrapping_add(offset),
                (value >> (24 - offset * 8)) as u8,
            );
        }
    }

    fn check_guest_access(
        &self,
        address: u32,
        access: crate::mmu::MemoryAccess,
    ) -> Result<(), CpuError> {
        if crate::machine::is_device_aperture(address) {
            let reason = if !crate::machine::is_implemented_device_page(address) {
                crate::mmu::PageFaultReason::PhysicalOutOfRange
            } else if self.privilege_mode == PrivilegeMode::User {
                crate::mmu::PageFaultReason::SupervisorOnly
            } else if access == crate::mmu::MemoryAccess::Fetch {
                crate::mmu::PageFaultReason::NotExecutable
            } else if crate::machine::device_register_writable(address).is_none() {
                crate::mmu::PageFaultReason::PhysicalOutOfRange
            } else if access == crate::mmu::MemoryAccess::Write
                && crate::machine::device_register_writable(address) == Some(false)
            {
                crate::mmu::PageFaultReason::ReadOnly
            } else {
                return Ok(());
            };
            return Err(CpuError::PageFault(crate::mmu::PageFault {
                virtual_address: address,
                access,
                reason,
            }));
        }
        Ok(())
    }

    fn resolve_guest_range(
        &self,
        address: u32,
        length: u32,
        access: crate::mmu::MemoryAccess,
    ) -> Result<Vec<u32>, CpuError> {
        let mut physical_addresses = Vec::with_capacity(length as usize);
        for offset in 0..length {
            let virtual_address = address
                .checked_add(offset)
                .filter(|value| *value < crate::mmu::ADDRESS_SPACE_SIZE)
                .ok_or_else(|| {
                    CpuError::PageFault(crate::mmu::PageFault {
                        virtual_address: address.saturating_add(offset),
                        access,
                        reason: crate::mmu::PageFaultReason::AddressOutOfRange,
                    })
                })?;
            self.check_guest_access(virtual_address, access)?;
            let physical_address = if self.mmu_enabled {
                crate::mmu::translate(
                    &self.memory,
                    self.page_directory,
                    virtual_address,
                    access,
                    self.privilege_mode == PrivilegeMode::User,
                )
                .map_err(CpuError::PageFault)?
            } else {
                virtual_address
            };
            if !self.mmu_enabled
                && !crate::machine::is_device_aperture(physical_address)
                && physical_address as usize >= self.memory.len()
            {
                return Err(CpuError::PageFault(crate::mmu::PageFault {
                    virtual_address,
                    access,
                    reason: crate::mmu::PageFaultReason::PhysicalOutOfRange,
                }));
            }
            self.check_guest_access(physical_address, access)
                .map_err(|error| {
                    if let CpuError::PageFault(mut fault) = error {
                        fault.virtual_address = virtual_address;
                        CpuError::PageFault(fault)
                    } else {
                        error
                    }
                })?;
            physical_addresses.push(physical_address);
        }
        Ok(physical_addresses)
    }

    fn read_guest(&mut self, address: u32) -> Result<u8, CpuError> {
        let physical = self.resolve_guest_range(address, 1, crate::mmu::MemoryAccess::Read)?[0];
        Ok(self.read(physical))
    }

    fn read_guest_u16(&mut self, address: u32) -> Result<u16, CpuError> {
        let physical = self.resolve_guest_range(address, 2, crate::mmu::MemoryAccess::Read)?;
        Ok(u16::from_be_bytes([
            self.read(physical[0]),
            self.read(physical[1]),
        ]))
    }

    fn read_guest_u32(&mut self, address: u32) -> Result<u32, CpuError> {
        let physical = self.resolve_guest_range(address, 4, crate::mmu::MemoryAccess::Read)?;
        Ok(u32::from_be_bytes([
            self.read(physical[0]),
            self.read(physical[1]),
            self.read(physical[2]),
            self.read(physical[3]),
        ]))
    }

    fn fetch_guest_u16(&mut self, address: u32) -> Result<u16, CpuError> {
        let physical = self.resolve_guest_range(address, 2, crate::mmu::MemoryAccess::Fetch)?;
        Ok(u16::from_be_bytes([
            self.read(physical[0]),
            self.read(physical[1]),
        ]))
    }

    fn write_guest(&mut self, address: u32, value: u8) -> Result<(), CpuError> {
        let physical = self.resolve_guest_range(address, 1, crate::mmu::MemoryAccess::Write)?[0];
        self.write(physical, value);
        Ok(())
    }

    fn write_guest_u16(&mut self, address: u32, value: u16) -> Result<(), CpuError> {
        let physical = self.resolve_guest_range(address, 2, crate::mmu::MemoryAccess::Write)?;
        self.write(physical[0], (value >> 8) as u8);
        self.write(physical[1], value as u8);
        Ok(())
    }

    fn write_guest_u32(&mut self, address: u32, value: u32) -> Result<(), CpuError> {
        let physical = self.resolve_guest_range(address, 4, crate::mmu::MemoryAccess::Write)?;
        for (offset, physical_address) in physical.into_iter().enumerate() {
            self.write(physical_address, (value >> (24 - offset * 8)) as u8);
        }
        Ok(())
    }

    fn record_page_fault(&mut self, fault: crate::mmu::PageFault) {
        self.last_page_fault = Some(fault);
    }

    fn read_u64_registers(&self, register: usize) -> u64 {
        (0..4).fold(0, |value, offset| {
            (value << 16) | u64::from(self.registers[register + offset] as u16)
        })
    }

    fn write_u64_registers(&mut self, register: usize, value: u64) {
        for offset in 0..4 {
            self.registers[register + offset] = ((value >> (48 - offset * 16)) & 0xffff) as i64;
        }
    }

    fn register_u16(&self, register: usize) -> u16 {
        self.registers[register] as u16
    }

    fn register_pair_u32(&self, register: usize) -> u32 {
        (u32::from(self.register_u16(register)) << 16) | u32::from(self.register_u16(register + 1))
    }

    fn read_float_register(&self, register: usize, width: u8) -> f64 {
        if width == 32 {
            let bits = ((self.registers[register] as u16 as u32) << 16)
                | self.registers[register + 1] as u16 as u32;
            f64::from(f32::from_bits(bits))
        } else {
            f64::from_bits(self.read_u64_registers(register))
        }
    }

    fn write_float_register(&mut self, register: usize, width: u8, value: f64) {
        if width == 32 {
            let bits = if value.is_nan() {
                0x7fc0_0000
            } else {
                (value as f32).to_bits()
            };
            self.registers[register] = i64::from(bits >> 16);
            self.registers[register + 1] = i64::from(bits & 0xffff);
        } else {
            let bits = if value.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                value.to_bits()
            };
            self.write_u64_registers(register, bits);
        }
    }

    fn read_integer_register(&self, register: usize, width: u8, unsigned: bool) -> i64 {
        let bits = if width == 16 {
            u64::from(self.registers[register] as u16)
        } else {
            u64::from(
                ((self.registers[register] as u16 as u32) << 16)
                    | self.registers[register + 1] as u16 as u32,
            )
        };
        if !unsigned && bits & (1 << (width - 1)) != 0 {
            (bits as i64) - (1i64 << width)
        } else {
            bits as i64
        }
    }

    fn write_integer_register(&mut self, register: usize, width: u8, value: u64) {
        let masked = value & ((1u64 << width) - 1);
        if width == 16 {
            self.registers[register] = masked as i64;
        } else {
            self.registers[register] = (masked >> 16) as i64;
            self.registers[register + 1] = (masked & 0xffff) as i64;
        }
    }

    fn execute_float(&mut self, action: u16, destination: usize, source: usize) {
        if action <= 9 {
            let width = if action < 5 { 32 } else { 64 };
            let left = self.read_float_register(destination, width);
            let right = self.read_float_register(source, width);
            match action % 5 {
                0 => self.write_float_register(destination, width, left + right),
                1 => self.write_float_register(destination, width, left - right),
                2 => self.write_float_register(destination, width, left * right),
                3 => self.write_float_register(destination, width, left / right),
                _ => {
                    let unordered = left.is_nan() || right.is_nan();
                    self.flags.zero = !unordered && left == right;
                    self.flags.carry = !unordered && left < right;
                    self.flags.negative = false;
                    self.flags.overflow = false;
                }
            }
            return;
        }
        if (10..=17).contains(&action) {
            let format = (action - 10) % 4;
            let integer_width = if format < 2 { 16 } else { 32 };
            let unsigned = format == 1 || format == 3;
            let precision = if action < 14 { 32 } else { 64 };
            let value = self.read_integer_register(source, integer_width, unsigned);
            self.write_float_register(destination, precision, value as f64);
            return;
        }
        if action == 18 || action == 19 {
            let (from, to) = if action == 18 { (32, 64) } else { (64, 32) };
            let value = self.read_float_register(source, from);
            self.write_float_register(destination, to, value);
            return;
        }
        if (20..=23).contains(&action) {
            let width = if action == 20 || action == 22 { 32 } else { 64 };
            let value = self.read_float_register(source, width);
            let result = match action {
                20 | 21 => -value,
                _ => value.sqrt(),
            };
            self.write_float_register(destination, width, result);
            return;
        }

        let precision = if action < 28 { 32 } else { 64 };
        let conversion = (action - 24) % 4;
        let integer_width = if conversion < 2 { 16 } else { 32 };
        let value = self.read_float_register(source, precision);
        let mask = (1u64 << integer_width) - 1;
        let converted = if value.is_finite() {
            value.trunc().rem_euclid((mask + 1) as f64) as u64
        } else {
            0
        };
        self.write_integer_register(destination, integer_width, converted);
    }

    fn execute_muldiv(&mut self, destination: usize) -> Result<(), CpuError> {
        let control = self.fetch_guest_u16(self.pc)?;
        self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
        let operation = control >> 12;
        let source = (control & 7) as usize;
        if operation == 7 {
            let high = self.fetch_guest_u16(self.pc)? & 0x3fff;
            let low = self.fetch_guest_u16(self.pc.wrapping_add(2))?;
            self.registers[destination] = i64::from((u32::from(high) << 16) | u32::from(low));
            self.pc = self.pc.wrapping_add(4) & self.guest_address_mask();
            return Ok(());
        }
        if operation == 8 {
            let address = self.registers[source] as u32 & ADDRESS_MASK;
            self.registers[destination] = i64::from(self.read_guest_u32(address)? & ADDRESS_MASK);
            return Ok(());
        }
        if operation == 9 {
            let address = self.registers[source] as u32 & ADDRESS_MASK;
            let value = self.registers[destination] as u32 & ADDRESS_MASK;
            self.write_guest_u32(address, value)?;
            return Ok(());
        }
        if operation == 10 {
            let value = self.registers[destination] as u32 & ADDRESS_MASK;
            self.registers[destination] = i64::from(value >> 16);
            self.registers[source] = i64::from(value & 0xffff);
            return Ok(());
        }
        if operation == 11 {
            self.registers[destination] = (((self.registers[destination] as u32 & 0x3fff) << 16)
                | (self.registers[source] as u16 as u32))
                as i64;
            return Ok(());
        }
        let right = if control & 0x0800 != 0 {
            let value = self.fetch_guest_u16(self.pc)?;
            self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
            value
        } else {
            self.registers[source] as u16
        };
        let left = self.registers[destination] as u16;
        let signed_left = left as i16 as i32;
        let signed_right = right as i16 as i32;
        let mut carry = false;
        let result = match operation {
            0 => {
                let product = u32::from(left) * u32::from(right);
                carry = product > 0xffff;
                product as u16
            }
            1 => ((u32::from(left) * u32::from(right)) >> 16) as u16,
            2 => (((signed_left * signed_right) >> 16) & 0xffff) as u16,
            3 if right != 0 => left / right,
            5 if right != 0 => left % right,
            4 if right != 0 => (signed_left / signed_right) as u16,
            6 if right != 0 => (signed_left % signed_right) as u16,
            3..=6 => {
                carry = true;
                if operation == 5 || operation == 6 {
                    left
                } else {
                    0
                }
            }
            _ => return Ok(()),
        };
        self.registers[destination] = i64::from(result);
        self.flags.zero = result == 0;
        self.flags.carry = carry;
        Ok(())
    }

    fn execute_wide(&mut self, instruction: u16, destination: usize) -> Result<(), CpuError> {
        let control = self.fetch_guest_u16(self.pc)?;
        self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
        let operation = control >> 12;
        let immediate = control & 0x0800 != 0;
        let source = (control & 7) as usize;
        if destination & 1 != 0
            || destination > 6
            || (!matches!(operation, 6..=9) && (source & 1 != 0 || source > 6))
        {
            return Err(CpuError::InvalidRegisterPair(instruction));
        }
        let left = self.register_pair_u32(destination);
        if operation == 9 {
            let result = left.wrapping_neg();
            self.set_32_flags(result, left != 0, left == 0x8000_0000);
            self.set_pair(destination, result);
            return Ok(());
        }
        if operation == 10 {
            self.set_pair(destination, self.register_pair_u32(source));
            return Ok(());
        }
        if operation == 11 || operation == 12 {
            let address = self.register_pair_u32(source) & ADDRESS_MASK;
            if operation == 11 {
                let value = self.read_guest_u32(address)?;
                self.set_pair(destination, value);
            } else {
                self.write_guest_u32(address, left)?;
            }
            return Ok(());
        }
        if (13..=15).contains(&operation) {
            let right = self.register_pair_u32(source);
            let result = if operation == 13 {
                let product = u64::from(left) * u64::from(right);
                let signed_product = (left as i32 as i64) * (right as i32 as i64);
                self.flags.carry = product > u64::from(u32::MAX);
                self.flags.overflow =
                    signed_product < i64::from(i32::MIN) || signed_product > i64::from(i32::MAX);
                product as u32
            } else {
                let signed = control & 0x0800 != 0;
                if right == 0 {
                    self.flags.carry = true;
                    if operation == 15 { left } else { 0 }
                } else if signed {
                    let dividend = left as i32 as i64;
                    let divisor = right as i32 as i64;
                    self.flags.carry = false;
                    if operation == 14 {
                        dividend.wrapping_div(divisor) as u32
                    } else {
                        dividend.wrapping_rem(divisor) as u32
                    }
                } else {
                    self.flags.carry = false;
                    if operation == 14 {
                        left / right
                    } else {
                        left % right
                    }
                }
            };
            self.flags.zero = result == 0;
            self.flags.negative = result & 0x8000_0000 != 0;
            if operation != 13 {
                self.flags.overflow = false;
            }
            self.set_pair(destination, result);
            return Ok(());
        }
        let result = if matches!(operation, 6..=8) {
            let count = if immediate {
                u32::from(control & 0x1f)
            } else {
                (self.registers[source] as u32) & 0x1f
            };
            let carry = if count == 0 {
                self.flags.carry
            } else if operation == 6 {
                (left >> (32 - count)) & 1 != 0
            } else {
                (left >> (count - 1)) & 1 != 0
            };
            self.flags.carry = carry;
            self.flags.overflow = false;
            match operation {
                6 => left.wrapping_shl(count),
                7 => left.wrapping_shr(count),
                _ => ((left as i32) >> count) as u32,
            }
        } else {
            let right = self.register_pair_u32(source);
            match operation {
                0 => {
                    let (result, carry) = left.overflowing_add(right);
                    self.flags.carry = carry;
                    self.flags.overflow = (!(left ^ right) & (left ^ result) & 0x8000_0000) != 0;
                    result
                }
                1 | 2 => {
                    let (result, borrow) = left.overflowing_sub(right);
                    self.flags.carry = borrow;
                    self.flags.overflow = ((left ^ right) & (left ^ result) & 0x8000_0000) != 0;
                    if operation == 2 {
                        self.flags.zero = result == 0;
                        self.flags.negative = result & 0x8000_0000 != 0;
                        return Ok(());
                    }
                    result
                }
                3 => left & right,
                4 => left | right,
                _ => left ^ right,
            }
        };
        self.flags.zero = result == 0;
        self.flags.negative = result & 0x8000_0000 != 0;
        if (3..=5).contains(&operation) {
            self.flags.carry = false;
            self.flags.overflow = false;
        }
        self.set_pair(destination, result);
        Ok(())
    }

    fn set_pair(&mut self, register: usize, value: u32) {
        self.registers[register] = i64::from(value >> 16);
        self.registers[register + 1] = i64::from(value & 0xffff);
    }

    fn set_32_flags(&mut self, result: u32, carry: bool, overflow: bool) {
        self.flags.zero = result == 0;
        self.flags.negative = result & 0x8000_0000 != 0;
        self.flags.carry = carry;
        self.flags.overflow = overflow;
    }

    fn execute_shift(&mut self, destination: usize, value: u16, operation: u16, count: u32) {
        if count == 0 {
            self.set_flags(value, self.flags.carry, false);
            self.registers[destination] = i64::from(value);
            return;
        }
        let (result, carry) = match operation {
            0 => (
                ((u32::from(value) << count) & 0xffff) as u16,
                (value >> (16 - count)) & 1,
            ),
            1 => (value >> count, (value >> (count - 1)) & 1),
            2 => (
                ((value as i16) >> count) as u16,
                (((value as i16) >> (count - 1)) & 1) as u16,
            ),
            3 => {
                let result = value.rotate_left(count);
                (result, result & 1)
            }
            _ => {
                let result = value.rotate_right(count);
                (result, result >> 15)
            }
        };
        self.set_flags(result, carry != 0, false);
        self.registers[destination] = i64::from(result);
    }

    fn set_flags(&mut self, result: u16, carry: bool, overflow: bool) {
        self.flags.zero = result == 0;
        self.flags.negative = result & 0x8000 != 0;
        self.flags.carry = carry;
        self.flags.overflow = overflow;
    }

    fn execute_misc(
        &mut self,
        destination: usize,
        value: u16,
        selector: u16,
        control: u16,
    ) -> Result<(), CpuError> {
        match selector {
            0 => self.registers[destination] = i64::from(value.rotate_left(8)),
            1 => {
                let result = (value & 0xff) | if value & 0x80 != 0 { 0xff00 } else { 0 };
                self.registers[destination] = i64::from(result);
                self.set_flags(result, self.flags.carry, false);
            }
            2 => self.pc = u32::from(value) & ADDRESS_MASK,
            3 => {
                self.push_u32(self.pc)?;
                self.pc = u32::from(value) & ADDRESS_MASK;
            }
            4 => self.push_u16(self.pack_flags())?,
            5 => {
                let flags = self.pop_u16()?;
                self.restore_flags(flags);
            }
            6 => self.flags.carry = true,
            7 => self.flags.carry = false,
            8 => self.sp = u32::from(value) & ADDRESS_MASK,
            9 => {
                self.registers[destination] = i64::from(value.leading_zeros());
                self.flags.zero = value == 0;
            }
            10 => self.registers[destination] = i64::from((self.sp >> 16) & 0x3fff),
            11 | 12 => {
                let high_register = ((control >> 9) & 7) as usize;
                let target =
                    ((self.registers[high_register] as u32 & 0x3fff) << 16) | u32::from(value);
                if selector == 11 {
                    self.push_u32(self.pc)?;
                }
                self.pc = target & ADDRESS_MASK;
            }
            13 => {
                self.sp = (self.sp & 0xffff) | (u32::from(value & 0x3fff) << 16);
            }
            14..=16 | 18..=19 if destination & 1 != 0 || destination > 6 => {
                return Err(CpuError::InvalidRegisterPair(control));
            }
            14 => self.load_idt(self.register_pair_u32(destination))?,
            15 => self.enable_mmu(self.register_pair_u32(destination))?,
            16 => self.switch_address_space(self.register_pair_u32(destination))?,
            17 => self.disable_mmu()?,
            18 => {
                let stack_register = ((control >> 9) & 7) as usize;
                if stack_register & 1 != 0 || stack_register > 6 {
                    return Err(CpuError::InvalidRegisterPair(control));
                }
                self.enter_user_mode(
                    self.register_pair_u32(destination),
                    self.register_pair_u32(stack_register),
                )?;
            }
            19 => {
                let stack_register = ((control >> 9) & 7) as usize;
                if stack_register & 1 != 0 || stack_register > 6 {
                    return Err(CpuError::InvalidRegisterPair(control));
                }
                let kernel_stack = self.sp;
                let task = self.create_user_task(
                    self.register_pair_u32(destination),
                    self.register_pair_u32(stack_register),
                    kernel_stack,
                    self.mmu_enabled.then_some(self.page_directory),
                )?;
                self.start_task(task)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_alu(&mut self, destination: usize) -> Result<(), CpuError> {
        let control = self.fetch_guest_u16(self.pc)?;
        self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
        let operation = control >> 12;
        let left = self.registers[destination] as u16;
        if operation == 15 {
            self.execute_misc(destination, left, control & 0x1f, control)?;
            return Ok(());
        }
        let immediate = control & 0x0800 != 0;
        let source = (control & 7) as usize;
        if operation <= 4 {
            let count = if immediate {
                u32::from(control & 0xf)
            } else {
                (self.registers[source] as u32) & 0xf
            };
            self.execute_shift(destination, left, operation, count);
            return Ok(());
        }
        if operation == 7 {
            let result = left.wrapping_neg();
            self.set_flags(result, left != 0, left == 0x8000);
            self.registers[destination] = i64::from(result);
            return Ok(());
        }
        let right = if immediate {
            let value = self.fetch_guest_u16(self.pc)?;
            self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
            value
        } else {
            self.registers[source] as u16
        };
        let carry_in = u16::from(self.flags.carry);
        let result;
        match operation {
            5 | 9 => {
                let total = u32::from(left)
                    + u32::from(right)
                    + if operation == 5 {
                        u32::from(carry_in)
                    } else {
                        0
                    };
                result = total as u16;
                let overflow = (!(left ^ right) & (left ^ result) & 0x8000) != 0;
                self.set_flags(result, total > 0xffff, overflow);
            }
            6 | 10 | 14 => {
                let borrow = if operation == 6 { carry_in } else { 0 };
                let total = i32::from(left) - i32::from(right) - i32::from(borrow);
                result = total as u16;
                let overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0;
                self.set_flags(result, total < 0, overflow);
            }
            _ => {
                result = match operation {
                    8 | 11 => left & right,
                    12 => left | right,
                    _ => left ^ right,
                };
                self.set_flags(result, false, false);
            }
        }
        if operation != 8 && operation != 14 {
            self.registers[destination] = i64::from(result);
        }
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), CpuError> {
        if !self.running {
            return Ok(());
        }
        let executing_task = self.current_task();
        let saved_pc = self.pc;
        let saved_registers = self.registers;
        let saved_sp = self.sp;
        let saved_kernel_sp = self.kernel_sp;
        let saved_flags = self.flags;
        let saved_mode = self.privilege_mode;
        let saved_running = self.running;
        let result = match self.execute_instruction() {
            Err(CpuError::PrivilegeViolation { operation, .. }) => {
                self.debug_console.log_error(
                    "cpu",
                    &format!("Privilege violation at {saved_pc:#x}: {operation}"),
                );
                self.registers = saved_registers;
                self.sp = saved_sp;
                self.kernel_sp = saved_kernel_sp;
                self.flags = saved_flags;
                self.privilege_mode = saved_mode;
                self.running = saved_running;
                self.enter_exception(EXCEPTION_PRIVILEGE_VIOLATION, saved_pc)
            }
            Err(CpuError::UnsupportedInstruction(_) | CpuError::InvalidRegisterPair(_)) => {
                self.registers = saved_registers;
                self.sp = saved_sp;
                self.kernel_sp = saved_kernel_sp;
                self.flags = saved_flags;
                self.privilege_mode = saved_mode;
                self.running = saved_running;
                self.enter_exception(EXCEPTION_ILLEGAL_INSTRUCTION, saved_pc)
            }
            Err(CpuError::PageFault(fault)) => {
                self.registers = saved_registers;
                self.sp = saved_sp;
                self.kernel_sp = saved_kernel_sp;
                self.flags = saved_flags;
                self.privilege_mode = saved_mode;
                self.running = saved_running;
                self.record_page_fault(fault);
                self.enter_exception(EXCEPTION_PAGE_FAULT, saved_pc)
            }
            result => result,
        };
        if let Some(task_id) = executing_task
            && let Some(instructions) = self.syscall_state.process_instructions.get_mut(&task_id)
        {
            *instructions = instructions.saturating_add(1);
        }
        result
    }

    fn execute_instruction(&mut self) -> Result<(), CpuError> {
        if !self.running {
            return Ok(());
        }

        let instruction_pc = self.pc;
        let instruction = self.fetch_guest_u16(self.pc)?;
        self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
        let opcode = instruction >> 11;
        let rd = ((instruction >> 8) & 7) as usize;
        let rs1 = ((instruction >> 5) & 7) as usize;
        let rs2 = ((instruction >> 2) & 7) as usize;
        let immediate = (instruction & 0xff) as u32;

        match opcode {
            OP_HALT if instruction == 0 => {
                self.running = false;
                self.debug_console
                    .log_info("cpu", &format!("CPU halted at {instruction_pc:#x}"));
            }
            OP_HALT if instruction == crate::isa::SVC_VECTOR => {
                let vector = crate::isa::SVC_VECTOR;
                let descriptor = self.idt_descriptor(vector)?;
                let caller_privilege = u8::from(self.privilege_mode == PrivilegeMode::User) * 3;
                let is_syscall = self.privilege_mode == PrivilegeMode::User
                    && self.scheduler.current_task().is_some()
                    && self.registers[crate::syscall::REGISTER_ABI_VERSION] != 0;
                if caller_privilege > descriptor.privilege {
                    self.enter_exception(EXCEPTION_PRIVILEGE_VIOLATION, self.pc)?;
                } else {
                    self.enter_event(vector, self.pc, descriptor.handler, 0, descriptor.gate_type)?;
                    if is_syscall {
                        self.dispatch_user_syscall()?;
                    }
                }
            }
            OP_HALT => return Err(CpuError::UnsupportedInstruction(instruction)),
            OP_LDI => self.registers[rd] = i64::from(immediate),
            OP_LD => {
                let address = self.registers[rs1] as u32 & ADDRESS_MASK;
                self.registers[rd] = i64::from(self.read_guest(address)?);
            }
            OP_ST => {
                let address = self.registers[rs1] as u32 & ADDRESS_MASK;
                self.write_guest(address, self.registers[rd] as u8)?;
            }
            OP_MOV => self.registers[rd] = self.registers[rs1],
            OP_ADD => {
                let left = self.registers[rs1];
                let right = self.registers[rs2];
                let result = left.wrapping_add(right);
                self.registers[rd] = result;
                self.flags.zero = result == 0;
                self.flags.carry = result > 0xff;
                let word = result as u16;
                self.flags.negative = word & 0x8000 != 0;
                self.flags.overflow = ((left ^ result) & (right ^ result) & 0x8000) != 0;
            }
            OP_SUB => {
                let left = self.registers[rs1];
                let right = self.registers[rs2];
                let result = left.wrapping_sub(right);
                self.registers[rd] = result;
                self.flags.zero = result == 0;
                self.flags.carry = result < 0;
                self.flags.negative = result as u16 & 0x8000 != 0;
                self.flags.overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0;
            }
            OP_AND => self.registers[rd] = self.registers[rs1] & self.registers[rs2],
            OP_OR => self.registers[rd] = self.registers[rs1] | self.registers[rs2],
            OP_XOR => self.registers[rd] = self.registers[rs1] ^ self.registers[rs2],
            OP_NOT => self.registers[rd] = !self.registers[rs1] & 0xffff,
            OP_JMP => self.pc = (instruction & 0x7ff) as u32,
            OP_JZ => {
                if self.registers[rd] == 0 {
                    self.pc = immediate & self.guest_address_mask();
                }
            }
            OP_INC => self.registers[rd] = self.registers[rd].wrapping_add(1),
            OP_DEC => self.registers[rd] = self.registers[rd].wrapping_sub(1),
            OP_CMP => {
                let left = self.registers[rs1];
                let right = self.registers[rs2];
                let result = left.wrapping_sub(right);
                self.flags.zero = result == 0;
                self.flags.carry = result < 0;
                self.flags.negative = result as u16 & 0x8000 != 0;
                self.flags.overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0;
            }
            OP_LDI_H => {
                self.registers[rd] = (i64::from(immediate) << 8) | (self.registers[rd] & 0xff);
            }
            OP_EI => {
                if self.privilege_mode != PrivilegeMode::Supervisor {
                    return Err(CpuError::PrivilegeViolation {
                        pc: instruction_pc,
                        operation: "EI",
                    });
                }
                self.interrupts.control |= 1;
            }
            OP_DI => {
                if self.privilege_mode != PrivilegeMode::Supervisor {
                    return Err(CpuError::PrivilegeViolation {
                        pc: instruction_pc,
                        operation: "DI",
                    });
                }
                self.interrupts.control &= !1;
            }
            OP_RTI => self.return_from_exception()?,
            OP_PUSH => self.push_u16(self.registers[rd] as u16)?,
            OP_POP => self.registers[rd] = i64::from(self.pop_u16()?),
            OP_CALL => {
                self.push_u32(self.pc)?;
                self.pc = (instruction & 0x7ff) as u32;
            }
            OP_RET => self.pc = self.pop_u32()? & ADDRESS_MASK,
            OP_BZ => {
                if self.flags.zero {
                    self.pc = (instruction & 0x7ff) as u32;
                }
            }
            OP_LDW => {
                let address = self.registers[rs1] as u32 & ADDRESS_MASK;
                self.registers[rd] = i64::from(self.read_guest_u16(address)?);
            }
            OP_STW => {
                let address = self.registers[rs1] as u32 & ADDRESS_MASK;
                self.write_guest_u16(address, self.registers[rd] as u16)?;
            }
            OP_LDWS => {
                self.registers[rd] =
                    i64::from(self.read_guest_u16(self.sp.wrapping_add(immediate) & ADDRESS_MASK)?)
            }
            OP_STWS => self.write_guest_u16(
                self.sp.wrapping_add(immediate) & ADDRESS_MASK,
                self.registers[rd] as u16,
            )?,
            OP_ADJSP => {
                let offset = (immediate as u8 as i8) as i32;
                self.sp = self.sp.wrapping_add_signed(offset) & ADDRESS_MASK;
            }
            OP_BC => {
                if self.flags.carry {
                    self.pc = (instruction & 0x7ff) as u32;
                }
            }
            OP_SHR if instruction & FLOAT_MASK == FLOAT_PREFIX => {
                let control = self.fetch_guest_u16(self.pc)?;
                self.pc = self.pc.wrapping_add(2) & self.guest_address_mask();
                self.execute_float((control >> 11) & 0x1f, rd, (control & 7) as usize);
            }
            OP_SHR if instruction & MULDIV_MASK == MULDIV_PREFIX => {
                self.execute_muldiv(rd)?;
            }
            OP_SHR if instruction & WIDE_MASK == WIDE_PREFIX => {
                self.execute_wide(instruction, rd)?;
            }
            OP_SHR if instruction & ALU_MASK == ALU_PREFIX => {
                self.execute_alu(rd)?;
            }
            OP_SHR if instruction & FAR_BRANCH_MASK == FAR_BRANCH_PREFIX => {
                let condition = ((instruction >> 7) & 0xf) as u8;
                let target = (((self.fetch_guest_u16(self.pc)? & 0x3fff) as u32) << 16)
                    | self.fetch_guest_u16(self.pc.wrapping_add(2))? as u32;
                self.pc = self.pc.wrapping_add(4) & self.guest_address_mask();
                if branch_condition(self.flags, condition) {
                    self.pc = target & ADDRESS_MASK;
                }
            }
            OP_SHR if instruction & FAR_CONTROL_MASK == FAR_CONTROL_PREFIX => {
                let operation = (instruction >> 9) & 3;
                let target = (((self.fetch_guest_u16(self.pc)? & 0x3fff) as u32) << 16)
                    | self.fetch_guest_u16(self.pc.wrapping_add(2))? as u32;
                self.pc = self.pc.wrapping_add(4) & self.guest_address_mask();
                match operation {
                    0 => self.pc = target & ADDRESS_MASK,
                    1 => {
                        self.push_u32(self.pc)?;
                        self.pc = target & ADDRESS_MASK;
                    }
                    2 if self.flags.zero => self.pc = target & ADDRESS_MASK,
                    3 if self.flags.carry => self.pc = target & ADDRESS_MASK,
                    _ => {}
                }
            }
            OP_SHR => {
                let extension = (instruction >> 5) & 7;
                if extension == 0 {
                    self.registers[rd] = if instruction & 0xff == 0x1c {
                        i64::from(self.sp)
                    } else {
                        (self.registers[rd] & 0xffff) >> 1
                    };
                } else {
                    let source = ((instruction >> 2) & 7) as usize;
                    let left = f16_to_f32(self.registers[rd] as u16);
                    let right = f16_to_f32(self.registers[source] as u16);
                    match extension {
                        1 => self.registers[rd] = i64::from(f32_to_f16(left + right)),
                        2 => self.registers[rd] = i64::from(f32_to_f16(left - right)),
                        3 => self.registers[rd] = i64::from(f32_to_f16(left * right)),
                        4 => self.registers[rd] = i64::from(f32_to_f16(left / right)),
                        5 => {
                            self.flags.zero = left == right;
                            self.flags.carry = left < right;
                        }
                        6 => {
                            self.registers[rd] =
                                i64::from(f32_to_f16((self.registers[rd] as u16 as i16) as f32))
                        }
                        7 => {
                            self.registers[rd] =
                                i64::from((f16_to_f32(self.registers[rd] as u16) as i32) as u16)
                        }
                        _ => {}
                    }
                }
            }
            _ => return Err(CpuError::UnsupportedInstruction(instruction)),
        }

        self.advance_timer();
        if self.interrupts.control & 1 != 0 {
            self.check_interrupts()?;
        }
        Ok(())
    }

    pub fn has_pending_work(&self) -> bool {
        self.running
            || !self.syscall_state.sleeping.is_empty()
            || !self.syscall_state.sleeping_millis.is_empty()
    }

    pub fn run(&mut self, instruction_budget: usize) -> Result<usize, CpuError> {
        let mut executed = 0;
        while executed < instruction_budget && !self.frame_yield && self.has_pending_work() {
            if self.running {
                self.step()?;
            } else {
                self.advance_timer();
            }
            executed += 1;
        }
        Ok(executed)
    }

    pub fn begin_frame(&mut self) {
        self.frame_yield = false;
        self.frame_presented = false;
        self.frame_count = self.frame_count.wrapping_add(1);
    }

    pub fn receive_console_input(&mut self, bytes: &[u8]) {
        self.console.receive(bytes);
    }

    pub fn signal_keyboard_event(&mut self, keycode: u8) {
        self.input.key = keycode;
        self.interrupts.control |= 2;
    }

    pub fn set_key(&mut self, mask: u32, pressed: bool) {
        self.input.set_key(mask, pressed);
    }

    pub fn set_mouse_position(&mut self, x: u16, y: u16) {
        let (width, height) = if self.graphics_mode == 1 {
            (640, 480)
        } else {
            (320, 240)
        };
        let x = x.min(width - 1);
        let y = y.min(height - 1);
        let [x_high, x_low] = x.to_be_bytes();
        let [y_high, y_low] = y.to_be_bytes();
        self.input.mouse[..4].copy_from_slice(&[x_high, x_low, y_high, y_low]);
    }

    pub fn mouse_button(&mut self, button: usize, down: bool, x: u16, y: u16, now_ms: u64) {
        self.input.mouse_button(button, down, x, y, now_ms);
    }

    pub fn set_mouse_wheel(&mut self, delta: i16) {
        self.input.add_wheel(delta);
    }

    pub fn end_mouse_frame(&mut self) {
        self.input.mouse[6..8].fill(0);
    }

    pub fn take_sound_commands(&mut self) -> Vec<crate::machine::SoundCommand> {
        self.audio.take_commands()
    }

    fn screen_size(&self) -> (i32, i32) {
        if self.graphics_mode == 1 {
            (640, 480)
        } else {
            (320, 240)
        }
    }

    pub fn graphics_mode(&self) -> u8 {
        self.graphics_mode
    }

    pub fn set_graphics_mode(&mut self, mode: u8) -> Result<(), &'static str> {
        if mode > 1 {
            return Err("unsupported graphics mode");
        }
        if self.graphics_mode != mode {
            self.graphics_mode = mode;
            self.vram.fill(0);
        }
        Ok(())
    }

    pub fn composited_framebuffer(&self) -> Vec<u8> {
        let (width, height) = self.screen_size();
        let width = width as usize;
        let height = height as usize;
        let mut pixels = vec![0; width * height];
        self.composited_framebuffer_into(&mut pixels);
        pixels
    }

    pub fn composited_framebuffer_into(&self, pixels: &mut [u8]) {
        let (width, height) = self.screen_size();
        let width = width as usize;
        let height = height as usize;
        if width == 320 {
            pixels[..width * height].copy_from_slice(&self.vram[..width * height]);
        } else {
            for (index, output) in pixels[..width * height].iter_mut().enumerate() {
                let packed = self.vram[index / 2];
                *output = if index & 1 == 0 {
                    packed >> 4
                } else {
                    packed & 0x0f
                };
            }
        }
    }

    pub fn palette_color(&self, index: u8) -> [u8; 3] {
        self.palette[index as usize]
    }

    fn graphics_pixel(&mut self, x: i32, y: i32, color: u8) {
        let (width, height) = self.screen_size();
        if x < 0 || y < 0 || x >= width || y >= height {
            return;
        }
        if width == 320 {
            let address = (y * width + x) as usize;
            self.vram[address] = color;
        } else {
            let address = (y * (width / 2) + x / 2) as usize;
            let shift = if x & 1 == 0 { 4 } else { 0 };
            let mask = 0xf << shift;
            self.vram[address] = (self.vram[address] & !mask) | ((color & 0xf) << shift);
        }
    }

    fn pack_flags(&self) -> u16 {
        u16::from(self.flags.zero)
            | (u16::from(self.flags.carry) << 1)
            | (u16::from(self.flags.negative) << 2)
            | (u16::from(self.flags.overflow) << 3)
    }

    fn restore_flags(&mut self, value: u16) {
        self.flags.zero = value & 1 != 0;
        self.flags.carry = value & 2 != 0;
        self.flags.negative = value & 4 != 0;
        self.flags.overflow = value & 8 != 0;
    }

    fn push_u16(&mut self, value: u16) -> Result<(), CpuError> {
        let stack_pointer = self.sp.wrapping_sub(2) & ADDRESS_MASK;
        self.write_guest_u16(stack_pointer, value)?;
        self.sp = stack_pointer;
        Ok(())
    }

    fn pop_u16(&mut self) -> Result<u16, CpuError> {
        let value = self.read_guest_u16(self.sp)?;
        self.sp = self.sp.wrapping_add(2) & ADDRESS_MASK;
        Ok(value)
    }

    fn enter_exception(&mut self, cause: u16, saved_pc: u32) -> Result<(), CpuError> {
        let descriptor = match self.idt_descriptor(cause) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.running = false;
                return Err(error);
            }
        };
        self.enter_event(cause, saved_pc, descriptor.handler, 0, descriptor.gate_type)
    }

    fn resolve_event_frame(
        &self,
        start: u32,
        access: crate::mmu::MemoryAccess,
    ) -> Result<[usize; EXCEPTION_ENTRY_BYTES as usize], CpuError> {
        let mut addresses = [0; EXCEPTION_ENTRY_BYTES as usize];
        for (offset, physical) in addresses.iter_mut().enumerate() {
            let address = start
                .checked_add(offset as u32)
                .filter(|address| *address <= self.guest_address_mask())
                .ok_or(CpuError::DoubleFault)?;
            let translated = if self.mmu_enabled {
                crate::mmu::translate(&self.memory, self.page_directory, address, access, false)
                    .map_err(|_| CpuError::DoubleFault)?
            } else {
                address
            };
            if translated as usize >= self.memory.len()
                || Self::is_device_backing_address(address)
                || Self::is_device_backing_address(translated)
            {
                return Err(CpuError::DoubleFault);
            }
            *physical = translated as usize;
        }
        Ok(addresses)
    }

    fn enter_event(
        &mut self,
        cause: u16,
        saved_pc: u32,
        handler: u32,
        entry_kind: u16,
        gate_type: crate::interrupts::GateType,
    ) -> Result<(), CpuError> {
        if handler > self.guest_address_mask()
            || crate::machine::is_device_aperture(handler)
            || saved_pc > self.guest_address_mask()
            || (self.mmu_enabled
                && crate::mmu::translate(
                    &self.memory,
                    self.page_directory,
                    handler,
                    crate::mmu::MemoryAccess::Fetch,
                    false,
                )
                .is_err())
        {
            self.running = false;
            return Err(CpuError::DoubleFault);
        }
        let old_mode = self.privilege_mode;
        let old_sp = self.sp;
        let stack_top = if old_mode == PrivilegeMode::User {
            self.kernel_sp
        } else {
            self.sp
        };
        let Some(frame_start) = stack_top.checked_sub(EXCEPTION_ENTRY_BYTES) else {
            self.running = false;
            return Err(CpuError::DoubleFault);
        };
        let addresses = match self.resolve_event_frame(frame_start, crate::mmu::MemoryAccess::Write)
        {
            Ok(addresses) => addresses,
            Err(error) => {
                self.running = false;
                return Err(error);
            }
        };

        let flags = self.pack_flags().to_be_bytes();
        let interrupt_enabled = self.interrupts.control & 1 != 0;
        let status = (u16::from(old_mode == PrivilegeMode::User)
            | (u16::from(interrupt_enabled) << 1))
            .to_be_bytes();
        let cause_bytes = cause.to_be_bytes();
        let kind = entry_kind.to_be_bytes();
        let mut frame = [0; EXCEPTION_ENTRY_BYTES as usize];
        frame[0..4].copy_from_slice(&saved_pc.to_be_bytes());
        frame[4..6].copy_from_slice(&flags);
        frame[6..8].copy_from_slice(&status);
        frame[8..12].copy_from_slice(&old_sp.to_be_bytes());
        frame[12..14].copy_from_slice(&cause_bytes);
        frame[14..16].copy_from_slice(&kind);
        for (address, byte) in addresses.into_iter().zip(frame) {
            self.memory[address] = byte;
        }

        if gate_type == crate::interrupts::GateType::Interrupt {
            self.interrupts.control &= !1;
        }
        self.privilege_mode = PrivilegeMode::Supervisor;
        self.sp = frame_start;
        self.kernel_sp = frame_start;
        self.pc = handler;
        Ok(())
    }

    fn return_from_exception(&mut self) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc.wrapping_sub(2) & self.guest_address_mask(),
                operation: "RTI",
            });
        }
        let addresses = match self.resolve_event_frame(self.sp, crate::mmu::MemoryAccess::Read) {
            Ok(addresses) => addresses,
            Err(error) => {
                self.running = false;
                return Err(error);
            }
        };
        let Some(frame_end) = self.sp.checked_add(EXCEPTION_ENTRY_BYTES) else {
            self.running = false;
            return Err(CpuError::DoubleFault);
        };
        let frame = addresses.map(|address| self.memory[address]);
        let saved_pc =
            u32::from_be_bytes(frame[0..4].try_into().map_err(|_| CpuError::DoubleFault)?);
        let saved_flags =
            u16::from_be_bytes(frame[4..6].try_into().map_err(|_| CpuError::DoubleFault)?);
        let status = u16::from_be_bytes(frame[6..8].try_into().map_err(|_| CpuError::DoubleFault)?);
        let saved_sp =
            u32::from_be_bytes(frame[8..12].try_into().map_err(|_| CpuError::DoubleFault)?);
        let entry_kind = u16::from_be_bytes(
            frame[14..16]
                .try_into()
                .map_err(|_| CpuError::DoubleFault)?,
        );
        if status & !0x3 != 0
            || entry_kind > 1
            || saved_pc > self.guest_address_mask()
            || crate::machine::is_device_aperture(saved_pc)
            || saved_sp == 0
            || saved_sp > self.guest_address_mask()
            || crate::machine::is_device_aperture(saved_sp - 1)
        {
            self.running = false;
            return Err(CpuError::DoubleFault);
        }

        let prior_mode = if status & 1 != 0 {
            PrivilegeMode::User
        } else {
            PrivilegeMode::Supervisor
        };
        if self.mmu_enabled {
            let user_mode = prior_mode == PrivilegeMode::User;
            for (address, access) in [
                (saved_pc, crate::mmu::MemoryAccess::Fetch),
                (saved_sp - 1, crate::mmu::MemoryAccess::Write),
            ] {
                if crate::mmu::translate(
                    &self.memory,
                    self.page_directory,
                    address,
                    access,
                    user_mode,
                )
                .is_err()
                {
                    self.running = false;
                    return Err(CpuError::DoubleFault);
                }
            }
        }
        self.restore_flags(saved_flags);
        self.pc = saved_pc;
        self.privilege_mode = prior_mode;
        self.interrupts.control = (self.interrupts.control & !1) | ((status >> 1) as u8 & 1);
        if prior_mode == PrivilegeMode::User {
            self.kernel_sp = frame_end;
            self.sp = saved_sp;
        } else {
            self.sp = frame_end;
            self.kernel_sp = frame_end;
        }
        Ok(())
    }

    fn push_u32(&mut self, value: u32) -> Result<(), CpuError> {
        let stack_pointer = self.sp.wrapping_sub(4) & ADDRESS_MASK;
        self.write_guest_u32(stack_pointer, value)?;
        self.sp = stack_pointer;
        Ok(())
    }

    fn pop_u32(&mut self) -> Result<u32, CpuError> {
        let value = self.read_guest_u32(self.sp)?;
        self.sp = self.sp.wrapping_add(4) & ADDRESS_MASK;
        Ok(value)
    }

    fn check_interrupts(&mut self) -> Result<(), CpuError> {
        let icr = self.interrupts.control;
        if icr & 1 == 0 {
            return Ok(());
        }
        let (pending_bit, vector) = if icr & 2 != 0 {
            (2, u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD))
        } else if icr & 0x10 != 0 && self.interrupts.pit_enabled {
            (0x10, u16::from(crate::interrupts::IDT_VECTOR_PIT))
        } else if icr & 8 != 0 {
            (8, u16::from(crate::interrupts::IDT_VECTOR_RTC))
        } else {
            return Ok(());
        };
        self.interrupts.control = icr & !pending_bit;
        let descriptor = match self.idt_descriptor(vector) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.running = false;
                return Err(error);
            }
        };
        if vector == u16::from(crate::interrupts::IDT_VECTOR_PIT) {
            if let Some(task_id) = self.scheduler.current_task() {
                let context = self.capture_task_context(task_id);
                let next = self
                    .scheduler
                    .reschedule(context, TaskState::Runnable)
                    .map_err(CpuError::Scheduler)?;
                if let Some(context) = next {
                    self.restore_task_context(&context);
                } else {
                    self.running = false;
                }
                return Ok(());
            }
        }
        self.enter_event(vector, self.pc, descriptor.handler, 1, descriptor.gate_type)
    }

    fn advance_timer(&mut self) {
        self.syscall_state.ticks = self.syscall_state.ticks.saturating_add(1);
        let now = self.monotonic_milliseconds() as u32;
        let awake: Vec<_> = self
            .syscall_state
            .sleeping
            .iter()
            .filter_map(|(id, deadline)| (*deadline <= self.syscall_state.ticks).then_some(*id))
            .chain(self.syscall_state.sleeping_millis.iter().filter_map(
                |(id, (started, duration))| {
                    ((now.wrapping_sub(*started) & ADDRESS_MASK) >= *duration).then_some(*id)
                },
            ))
            .collect();
        for id in awake {
            self.syscall_state.sleeping.remove(&id);
            self.syscall_state.sleeping_millis.remove(&id);
            self.syscall_state.pending_results.insert(id, 0);
            let _ = self.scheduler.wake(id);
            if self.scheduler.current_task().is_none() {
                if let Ok(context) = self.scheduler.activate(id) {
                    self.restore_task_context(&context);
                }
            }
        }
        self.interrupts.tick();
    }
}

fn f16_to_f32(value: u16) -> f32 {
    let sign = u32::from(value & 0x8000) << 16;
    let exponent = (value >> 10) & 0x1f;
    let fraction = u32::from(value & 0x03ff);
    let bits = match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let mut significand = fraction;
            let mut exponent = 113u32;
            while significand & 0x400 == 0 {
                significand <<= 1;
                exponent -= 1;
            }
            sign | (exponent << 23) | ((significand & 0x3ff) << 13)
        }
        0x1f => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | ((u32::from(exponent) + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(bits)
}

fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let fraction = bits & 0x7f_ff_ff;
    if exponent == 0xff {
        return if fraction == 0 { sign | 0x7c00 } else { 0x7e00 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        let significand = fraction | 0x80_0000;
        let shift = (14 - half_exponent) as u32;
        let mut rounded = significand >> shift;
        let remainder = significand & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        if remainder > halfway || (remainder == halfway && rounded & 1 != 0) {
            rounded += 1;
        }
        return sign | rounded as u16;
    }
    let mut rounded = (fraction >> 13) as u16;
    let remainder = fraction & 0x1fff;
    if remainder > 0x1000 || (remainder == 0x1000 && rounded & 1 != 0) {
        rounded += 1;
        if rounded == 0x400 {
            let next_exponent = half_exponent + 1;
            if next_exponent >= 0x1f {
                return sign | 0x7c00;
            }
            return sign | ((next_exponent as u16) << 10);
        }
    }
    sign | ((half_exponent as u16) << 10) | rounded
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_interrupt_control(cpu: &Cpu) -> u8 {
        cpu.interrupts.control
    }

    fn write_interrupt_control(cpu: &mut Cpu, value: u8) {
        cpu.interrupts.control = value;
    }

    fn cpu_with_program(program: &[u16]) -> Cpu {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for (index, word) in program.iter().enumerate() {
            cpu.write_u16((index * 2) as u32, *word);
        }
        cpu
    }

    #[test]
    fn configured_full_memory_reserves_aperture_from_loading_stacks_and_roots() {
        let mut cpu = Cpu::new();
        let aperture = crate::machine::DEVICE_APERTURE_START;
        assert_eq!(cpu.sp, aperture - 1);
        assert_eq!(cpu.kernel_sp, aperture - 1);
        for (address, entry, data) in [
            (aperture, 0, vec![0xaa, 0xbb]),
            (aperture - 1, 0, vec![0xaa, 0xbb]),
            (0, aperture, vec![0xaa, 0xbb]),
        ] {
            assert!(
                cpu.load_program(&[crate::image::ImageSegment::new(address, data)], entry)
                    .is_err()
            );
            assert_eq!(&cpu.memory[..2], &[0, 0]);
            assert_eq!(
                &cpu.memory[aperture as usize - 1..aperture as usize + 2],
                &[0, 0, 0]
            );
        }
        assert!(cpu.enable_mmu(aperture).is_err());
        assert!(!cpu.is_mmu_enabled());
        assert!(cpu.load_idt(aperture).is_err());
        assert_eq!(
            cpu.enter_user_mode(aperture, 0x7000),
            Err(CpuError::InvalidEntryPoint(aperture))
        );
        assert_eq!(
            cpu.enter_user_mode(0, aperture + 16),
            Err(CpuError::InvalidUserStack(aperture + 16))
        );
        assert!(
            cpu.create_user_task(aperture, 0x7000, 0xf000, None)
                .is_err()
        );
        assert!(
            cpu.create_user_task(0, aperture + 16, 0xf000, None)
                .is_err()
        );
        assert!(
            cpu.create_user_task(0, 0x7000, aperture + 16, None)
                .is_err()
        );
        cpu.load_idt(0x800).unwrap();
        assert!(
            cpu.set_idt_descriptor(
                128,
                crate::interrupts::IdtDescriptor {
                    handler: aperture,
                    present: true,
                    privilege: 3,
                    gate_type: crate::interrupts::GateType::Interrupt,
                }
            )
            .is_err()
        );
        cpu.registers[0] = 123;
        cpu.prepare_image_entry();
        assert_eq!(cpu.sp, aperture - 1);
        assert_eq!(cpu.kernel_sp, aperture - 1);
        assert!(cpu.initial_user_stack_pointer() < aperture);
        cpu.load_program(
            &[crate::image::ImageSegment::new(aperture - 2, vec![0, 0])],
            aperture - 2,
        )
        .unwrap();
        cpu.run(1).unwrap();
        assert!(!cpu.running);
    }

    #[test]
    fn reserved_aperture_guest_access_never_aliases_small_ram() {
        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
                cpu.privilege_mode = mode;
                for address in [
                    crate::machine::FIRMWARE_GATEWAY_BASE + crate::machine::DEVICE_BANK_SIZE,
                    ADDRESS_MASK,
                ] {
                    let aliased = (address & cpu.memory_mask) as usize;
                    cpu.memory[aliased] = 0xa5;
                    assert!(cpu.read_guest(address).is_err());
                    assert!(cpu.write_guest(address, 0x5a).is_err());
                    assert!(
                        cpu.resolve_guest_range(address, 2, crate::mmu::MemoryAccess::Fetch)
                            .is_err()
                    );
                    assert_eq!(cpu.memory[aliased], 0xa5);
                }
                let address = crate::machine::DEVICE_APERTURE_START - 1;
                let aliased = (address & cpu.memory_mask) as usize;
                cpu.memory[aliased] = 0xa5;
                if mode == PrivilegeMode::User {
                    assert!(cpu.read_guest_u16(address).is_err());
                }
                assert!(cpu.write_guest_u16(address, 0x5a5a).is_err());
                assert_eq!(cpu.memory[aliased], 0xa5);
            }
        }
    }

    #[test]
    fn interrupt_and_fault_banks_have_no_ram_backing_or_read_side_effects() {
        use crate::machine::*;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            if size == 1 << 16 {
                assert_eq!(cpu.memory[0x3016], 0);
            }
            let interrupt_alias = (INTERRUPT_BASE & cpu.memory_mask) as usize;
            let fault_alias = (FAULT_BASE & cpu.memory_mask) as usize;
            cpu.memory[interrupt_alias..interrupt_alias + 16].fill(0xa5);
            cpu.memory[fault_alias..fault_alias + 8].fill(0xa5);
            assert_eq!(cpu.read_guest(INTERRUPT_ENABLE).unwrap(), 0);
            assert_eq!(cpu.read_guest(INTERRUPT_PIT_ENABLE).unwrap(), 1);
            assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_PERIOD).unwrap(), 0);
            assert_eq!(cpu.read_guest_u32(FAULT_ADDRESS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FAULT_INFO).unwrap(), 0);
            assert_eq!(cpu.read_guest(FAULT_VALID).unwrap(), 0);
            cpu.write_guest(INTERRUPT_PIT_ENABLE, 0).unwrap();
            cpu.write_guest_u16(INTERRUPT_PIT_PERIOD, 2).unwrap();
            cpu.advance_timer();
            assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_COUNTDOWN).unwrap(), 1);
            cpu.advance_timer();
            assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0x10);
            assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0x10);
            cpu.write_guest(INTERRUPT_ENABLE, 0xff).unwrap();
            assert_eq!(cpu.read_guest(INTERRUPT_ENABLE).unwrap(), 1);
            assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0x10);
            cpu.write_guest(INTERRUPT_ACK, 8).unwrap();
            assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0x10);
            cpu.write_guest(INTERRUPT_ACK, 0xff).unwrap();
            assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0);
            assert_eq!(cpu.read_guest(INTERRUPT_ACK).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_PERIOD).unwrap(), 2);
            assert!(
                cpu.memory[interrupt_alias..interrupt_alias + 16]
                    .iter()
                    .all(|b| *b == 0xa5)
            );
            assert!(
                cpu.memory[fault_alias..fault_alias + 8]
                    .iter()
                    .all(|b| *b == 0xa5)
            );
        }
    }

    #[test]
    fn high_pit_commits_on_low_byte_and_rejected_writes_are_atomic() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest_u16(INTERRUPT_PIT_PERIOD, 0x1234).unwrap();
        cpu.advance_timer();
        cpu.write_guest(INTERRUPT_PIT_PERIOD, 0xab).unwrap();
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_PERIOD).unwrap(), 0x1234);
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_COUNTDOWN).unwrap(), 0x1233);
        assert!(
            cpu.write_guest_u32(INTERRUPT_PIT_PERIOD, 0xffffffff)
                .is_err()
        );
        assert!(cpu.write_guest_u16(INTERRUPT_ACK, 0xffff).is_ok());
        assert!(cpu.write_guest_u16(INTERRUPT_PENDING, 0xff10).is_err());
        assert!(cpu.write_guest_u16(INTERRUPT_PIT_ENABLE, 0xffff).is_err());
        assert_eq!(cpu.read_guest(INTERRUPT_PIT_ENABLE).unwrap(), 1);
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_PERIOD).unwrap(), 0x1234);
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_COUNTDOWN).unwrap(), 0x1233);
        cpu.write_guest(INTERRUPT_PIT_PERIOD + 1, 0xcd).unwrap();
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_PERIOD).unwrap(), 0xabcd);
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_COUNTDOWN).unwrap(), 0xabcd);
        cpu.write_guest_u16(INTERRUPT_PIT_PERIOD, 0).unwrap();
        cpu.advance_timer();
        assert_eq!(cpu.read_guest_u16(INTERRUPT_PIT_COUNTDOWN).unwrap(), 0);
        assert_eq!(cpu.read_guest(INTERRUPT_PENDING).unwrap(), 0);
    }

    #[test]
    fn clock_snapshots_are_coherent_side_effect_free_and_independent_of_ram() {
        use crate::machine::*;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let alias = (CLOCK_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 20].fill(0xa5);
            assert_eq!(cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), 0);
            cpu.test_monotonic_ms = Some(0x12345678);
            cpu.frame_count = 0xfffe;
            cpu.update_rtc(23 * 3600 + 59 * 60 + 58);
            let seed = cpu.random_state;
            cpu.write_guest(CLOCK_CAPTURE, 1).unwrap();
            assert_eq!(cpu.random_state, seed);
            cpu.test_monotonic_ms = Some(0x23456789);
            cpu.frame_count = 1;
            cpu.update_rtc(0);
            for _ in 0..2 {
                assert_eq!(
                    cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(),
                    0x12345678
                );
                assert_eq!(cpu.read_guest_u16(CLOCK_FRAMES).unwrap(), 0xfffe);
                assert_eq!(cpu.read_guest_u16(CLOCK_RTC).unwrap(), 0x3a3b);
                assert_eq!(cpu.read_guest(CLOCK_RTC + 2).unwrap(), 23);
                assert_eq!(cpu.read_guest(CLOCK_CAPTURE).unwrap(), 0);
            }
            cpu.write_guest(CLOCK_CAPTURE, 2).unwrap();
            let random = cpu.random_state;
            assert_ne!(random, seed);
            assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), random);
            assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), random);
            assert_eq!(
                cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(),
                0x12345678
            );
            cpu.write_guest(CLOCK_CAPTURE, 0xfc).unwrap();
            assert_eq!(cpu.random_state, random);
            assert!(cpu.write_guest_u16(CLOCK_CAPTURE, 0x0300).is_err());
            assert_eq!(cpu.random_state, random);
            assert_eq!(
                cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(),
                0x12345678
            );
            cpu.write_guest(CLOCK_CAPTURE, 3).unwrap();
            assert_eq!(
                cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(),
                0x23456789
            );
            assert_eq!(cpu.read_guest_u16(CLOCK_FRAMES).unwrap(), 1);
            assert_eq!(cpu.read_guest_u16(CLOCK_RTC).unwrap(), 0);
            assert_ne!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), random);
            assert!(
                cpu.memory[alias..alias + 20]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn clock_rollover_and_image_entry_preserve_the_documented_clock_lifetime() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.test_monotonic_ms = Some(ADDRESS_MASK);
        cpu.frame_count = u16::MAX;
        cpu.write_guest(CLOCK_CAPTURE, 3).unwrap();
        let random = cpu.read_guest_u32(CLOCK_RANDOM).unwrap();
        assert_eq!(
            cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(),
            ADDRESS_MASK
        );
        cpu.test_monotonic_ms = Some(ADDRESS_MASK + 1);
        cpu.begin_frame();
        cpu.write_guest(CLOCK_CAPTURE, 1).unwrap();
        assert_eq!(cpu.read_guest_u32(CLOCK_MONOTONIC_MILLIS).unwrap(), 0);
        assert_eq!(cpu.read_guest_u16(CLOCK_FRAMES).unwrap(), 0);
        assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), random);
        cpu.prepare_image_entry();
        assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), random);
        assert_eq!(cpu.random_state, random);
    }

    #[test]
    fn high_clock_bank_captures_rtc_time_and_random_values() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.update_rtc(3661);
        cpu.write(CLOCK_CAPTURE, 1);
        assert_eq!(cpu.read(CLOCK_RTC), 1);
        assert_eq!(cpu.read(CLOCK_RTC + 1), 1);
        assert_eq!(cpu.read(CLOCK_RTC + 2), 1);
        let mut expected = cpu.random_state;
        expected ^= expected << 13;
        expected ^= expected >> 17;
        expected ^= expected << 5;
        cpu.write(CLOCK_CAPTURE, 2);
        assert_eq!(cpu.read_guest_u32(CLOCK_RANDOM).unwrap(), expected);
        cpu.frame_count = 0x1234;
        cpu.write(CLOCK_CAPTURE, 1);
        assert_eq!(cpu.read_u16(CLOCK_FRAMES), 0x1234);
    }

    #[test]
    fn input_bank_consumes_shared_events_only_after_full_access_validation() {
        use crate::machine::*;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let alias = (INPUT_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 16].fill(0xa5);
            if size > 0x43000 {
                cpu.write(0x43000, b'Z');
                assert_eq!(cpu.read(0x43000), b'Z');
                assert_eq!(cpu.read(INPUT_KEY), 0);
            }
            cpu.signal_keyboard_event(b'A');
            cpu.set_key(0xff12ab34, true);
            cpu.set_mouse_position(600, 400);
            cpu.mouse_button(0, true, 12, 13, 100);
            cpu.set_mouse_wheel(-2);
            assert!(cpu.read_guest_u16(INPUT_KEY).is_err());
            assert!(cpu.read_guest_u32(INPUT_MOUSE_EVENTS).is_err());
            assert!(cpu.write_guest(INPUT_KEY, 0).is_err());
            assert!(cpu.write_guest(INPUT_MOUSE_EVENTS, 0).is_err());
            cpu.privilege_mode = PrivilegeMode::User;
            assert!(cpu.read_guest(INPUT_KEY).is_err());
            assert!(cpu.read_guest(INPUT_MOUSE_EVENTS).is_err());
            cpu.privilege_mode = PrivilegeMode::Supervisor;
            assert_eq!(cpu.read_guest(INPUT_KEY).unwrap(), b'A');
            assert_eq!(cpu.read(INPUT_KEY), 0);
            assert_eq!(cpu.read(INTERRUPT_PENDING), 2);
            cpu.signal_keyboard_event(b'B');
            cpu.signal_keyboard_event(b'C');
            assert_eq!(cpu.read(INPUT_KEY), b'C');
            assert_eq!(cpu.read_guest(INPUT_KEY).unwrap(), 0);
            assert_eq!(cpu.read_guest_u32(INPUT_KEYS).unwrap(), 0x12ab34);
            cpu.set_key(0x020004, false);
            assert_eq!(cpu.read_guest_u32(INPUT_KEYS).unwrap(), 0x10ab30);
            assert_eq!(cpu.read_guest_u16(INPUT_MOUSE_X).unwrap(), 319);
            assert_eq!(cpu.read_guest_u16(INPUT_MOUSE_Y).unwrap(), 239);
            assert_eq!(cpu.read_guest(INPUT_MOUSE_BUTTONS).unwrap(), 1);
            assert_eq!(cpu.read_guest(INPUT_MOUSE_EVENTS).unwrap(), 1);
            assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_EVENTS), 0);
            assert_eq!(
                cpu.read_guest_u16(INPUT_MOUSE_WHEEL).unwrap(),
                (-2i16) as u16
            );
            assert_eq!(
                cpu.read_guest_u16(INPUT_MOUSE_WHEEL).unwrap(),
                (-2i16) as u16
            );
            cpu.mouse_button(0, false, 12, 13, 150);
            cpu.mouse_button(0, true, 15, 16, 500);
            assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_EVENTS), 0x15);
            assert_eq!(cpu.read_guest(INPUT_MOUSE_EVENTS).unwrap(), 0);
            cpu.end_mouse_frame();
            assert_eq!(cpu.read_guest_u16(INPUT_MOUSE_WHEEL).unwrap(), 0);
            assert!(
                cpu.memory[alias..alias + 16]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn input_wheel_saturates_and_mouse_position_obeys_video_mode() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.set_graphics_mode(1).unwrap();
        cpu.set_mouse_position(u16::MAX, u16::MAX);
        assert_eq!(cpu.read_guest_u16(INPUT_MOUSE_X).unwrap(), 639);
        assert_eq!(cpu.read_guest_u16(INPUT_MOUSE_Y).unwrap(), 479);
        cpu.set_mouse_wheel(i16::MAX);
        cpu.set_mouse_wheel(10);
        assert_eq!(
            cpu.read_guest_u16(INPUT_MOUSE_WHEEL).unwrap(),
            i16::MAX as u16
        );
        cpu.end_mouse_frame();
        cpu.set_mouse_wheel(i16::MIN);
        cpu.set_mouse_wheel(-10);
        assert_eq!(
            cpu.read_guest_u16(INPUT_MOUSE_WHEEL).unwrap(),
            i16::MIN as u16
        );
        cpu.signal_keyboard_event(b'Q');
        cpu.prepare_image_entry();
        assert_eq!(cpu.read_guest(INPUT_KEY).unwrap(), b'Q');
        assert_eq!(cpu.read(crate::machine::INTERRUPT_PENDING), 0);
        cpu.memory[0x43004..0x43007].fill(0xff);
        assert_eq!(cpu.read_guest_u32(INPUT_KEYS).unwrap(), 0);
    }

    #[test]
    fn terminal_banks_share_devices_without_ram_backing() {
        use crate::machine;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let console_alias = (machine::CONSOLE_BASE & cpu.memory_mask) as usize;
            let text_alias = (machine::FIRMWARE_TEXT_BASE & cpu.memory_mask) as usize;
            cpu.memory[console_alias..console_alias + 8].fill(0xa5);
            cpu.memory[text_alias..text_alias + 8].fill(0xa5);
            assert_eq!(cpu.read_guest(machine::CONSOLE_CONTROL).unwrap(), 3);
            assert_eq!(
                cpu.read_guest(machine::FIRMWARE_TEXT_FOREGROUND).unwrap(),
                1
            );
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ENABLED).unwrap(), 0);
            cpu.receive_console_input(b"AB");
            assert_eq!(cpu.read_guest(machine::CONSOLE_DATA).unwrap(), b'A');
            assert_eq!(cpu.read(machine::CONSOLE_DATA), b'B');
            assert_eq!(cpu.console.output(), b"AB");
            cpu.write_guest(machine::CONSOLE_CONTROL, 0xfe).unwrap();
            assert_eq!(cpu.read(machine::CONSOLE_CONTROL), 2);
            cpu.write_guest(machine::CONSOLE_DATA, b'C').unwrap();
            cpu.write(machine::CONSOLE_DATA, b'D');
            assert_eq!(cpu.console.output(), b"ABCD");
            assert!(cpu.terminal_active && cpu.terminal_dirty);
            assert_eq!(cpu.read_guest(machine::CONSOLE_DATA).unwrap(), 0);
            assert_eq!(cpu.read_guest(machine::CONSOLE_STATUS).unwrap(), 2);
            assert!(cpu.frame_yield);
            cpu.write_guest(machine::FIRMWARE_TEXT_FOREGROUND, 3)
                .unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_BACKGROUND, 4)
                .unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_ROW, 1).unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_COLUMN, 2).unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_CONTROL, 2).unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_DATA, b'A').unwrap();
            if size > 0x43300 {
                cpu.write(0x43300, b'B');
                assert_eq!(cpu.read(0x43300), b'B');
            }
            let cell = cpu.guest_text.cells[crate::guest_text::GUEST_TEXT_COLUMNS + 2];
            assert_eq!(
                (cell.character, cell.foreground, cell.background),
                (b'A', 3, 4)
            );
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ROW).unwrap(), 1);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_COLUMN).unwrap(), 3);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ENABLED).unwrap(), 1);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_DATA).unwrap(), 0);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_CONTROL).unwrap(), 0);
            cpu.write_guest(machine::FIRMWARE_TEXT_ROW, 255).unwrap();
            cpu.write_guest(machine::FIRMWARE_TEXT_COLUMN, 255).unwrap();
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ROW).unwrap(), 24);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_COLUMN).unwrap(), 52);
            cpu.write_guest(machine::FIRMWARE_TEXT_CONTROL, 1).unwrap();
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_COLUMN).unwrap(), 0);
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ROW).unwrap(), 0);
            assert!(
                cpu.guest_text
                    .cells
                    .iter()
                    .all(|cell| cell.character == b' ' && cell.background == 4)
            );
            cpu.write_guest(machine::FIRMWARE_TEXT_CONTROL, 0xff)
                .unwrap();
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ENABLED).unwrap(), 1);
            cpu.write_guest(machine::FIRMWARE_TEXT_CONTROL, 3).unwrap();
            assert_eq!(cpu.read_guest(machine::FIRMWARE_TEXT_ENABLED).unwrap(), 0);
            assert!(
                cpu.memory[console_alias..console_alias + 8]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
            assert!(
                cpu.memory[text_alias..text_alias + 8]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn terminal_bank_invalid_accesses_preserve_input_output_and_cursor() {
        use crate::machine;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.receive_console_input(&vec![b'X'; crate::console::RX_CAPACITY + 1]);
        assert_eq!(cpu.console.status(), 7);
        assert!(cpu.read_guest_u32(machine::CONSOLE_DATA).is_err());
        assert!(
            cpu.write_guest_u32(machine::CONSOLE_DATA, 0x41040300)
                .is_err()
        );
        assert!(
            cpu.write_guest_u32(machine::FIRMWARE_TEXT_ROW, 0x01020100)
                .is_err()
        );
        cpu.guest_text.write_text("unchanged", 3, 4);
        assert!(
            cpu.write_guest_u32(machine::FIRMWARE_TEXT_CONTROL, 0x01000000)
                .is_err()
        );
        for mode in [PrivilegeMode::User, PrivilegeMode::Supervisor] {
            cpu.privilege_mode = mode;
            if mode == PrivilegeMode::User {
                assert!(cpu.read_guest(machine::CONSOLE_DATA).is_err());
                assert!(cpu.write_guest(machine::CONSOLE_DATA, b'Q').is_err());
                assert!(cpu.write_guest(machine::FIRMWARE_TEXT_DATA, b'Q').is_err());
            }
            assert!(
                cpu.resolve_guest_range(machine::CONSOLE_DATA, 1, crate::mmu::MemoryAccess::Fetch)
                    .is_err()
            );
            assert!(
                cpu.resolve_guest_range(
                    machine::FIRMWARE_TEXT_DATA,
                    1,
                    crate::mmu::MemoryAccess::Fetch
                )
                .is_err()
            );
        }
        assert_eq!(
            cpu.console.receive_buffer().len(),
            crate::console::RX_CAPACITY
        );
        assert!(cpu.console.output().is_empty());
        assert_eq!(cpu.console.control(), 3);
        assert_eq!(cpu.console.status(), 7);
        assert_eq!(
            (cpu.guest_text.cursor_row, cpu.guest_text.cursor_column),
            (0, 9)
        );
        assert_eq!(cpu.guest_text.cells[0].character, b'u');
        assert!(!cpu.frame_yield);
        cpu.write_guest(machine::CONSOLE_STATUS, 0xfb).unwrap();
        assert_eq!(cpu.console.status(), 7);
        cpu.write_guest(machine::CONSOLE_STATUS, 4).unwrap();
        assert_eq!(cpu.console.status(), 3);
        cpu.write_guest(machine::CONSOLE_CONTROL, 0).unwrap();
        cpu.receive_console_input(b"Z");
        assert_eq!(
            cpu.console.receive_buffer().len(),
            crate::console::RX_CAPACITY
        );
        cpu.prepare_image_entry();
        assert_eq!(cpu.console.control(), 0);
        assert_eq!(
            cpu.console.receive_buffer().len(),
            crate::console::RX_CAPACITY
        );
    }

    #[test]
    fn removed_low_device_registers_are_plain_ram() {
        use crate::machine::*;
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        for (address, value) in [
            (0x43002, 0xa2),
            (0x43010, 0xa3),
            (0x43016, 0xa4),
            (0x43018, 0xa5),
            (0x4301c, 0xa6),
            (0x43014, 0xa7),
            (0x43020, 0xa8),
            (0x430ff, 0xa9),
            (0x43100, 0xa7),
            (0x431ff, 0xa8),
            (0x43200, 0xa7),
            (0x43300, 0xa8),
            (0x43400, 0xa9),
        ] {
            cpu.write(address, value);
            assert_eq!(cpu.read(address), value);
        }
        cpu.write(CLOCK_CAPTURE, 1);
        assert_eq!(cpu.read(CLOCK_RTC), 0);
        assert_eq!(cpu.read(INTERRUPT_ENABLE), 0);
        assert_eq!(cpu.read(FAULT_ADDRESS), 0);
        assert_eq!(cpu.read(crate::machine::CONSOLE_DATA), 0);
        assert_eq!(cpu.read(FIRMWARE_TEXT_DATA), 0);
    }

    #[test]
    fn audio_bank_stages_commands_without_ram_backing_and_drains_once_per_channel() {
        use crate::machine::*;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let alias = (AUDIO_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 16].fill(0xa5);
            assert_eq!(cpu.read_guest(AUDIO_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 0);
            cpu.write_guest(AUDIO_CHANNEL, 3).unwrap();
            cpu.write_guest_u16(AUDIO_FREQUENCY, 60000).unwrap();
            cpu.write_guest_u16(AUDIO_FRAMES, u16::MAX).unwrap();
            cpu.write_guest_u16(AUDIO_WAVEFORM, 2).unwrap();
            cpu.write_guest_u16(AUDIO_VOLUME, 75).unwrap();
            assert!(cpu.take_sound_commands().is_empty());
            cpu.write_guest(AUDIO_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(AUDIO_COMMAND).unwrap(), 0);
            assert_eq!(cpu.read_guest(AUDIO_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 8);
            cpu.write_guest_u16(AUDIO_FREQUENCY, 440).unwrap();
            assert_eq!(cpu.read_guest_u16(AUDIO_FREQUENCY).unwrap(), 440);
            assert_eq!(cpu.read_guest_u16(AUDIO_FRAMES).unwrap(), u16::MAX);
            assert_eq!(cpu.read_guest_u16(AUDIO_WAVEFORM).unwrap(), 2);
            assert_eq!(cpu.read_guest_u16(AUDIO_VOLUME).unwrap(), 75);
            assert_eq!(
                cpu.take_sound_commands(),
                [crate::machine::SoundCommand {
                    channel: 3,
                    frequency: 60000,
                    frames: u16::MAX,
                    waveform: 2,
                    volume: 75,
                }]
            );
            assert!(cpu.take_sound_commands().is_empty());
            assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 0);
            cpu.write_guest(AUDIO_COMMAND, 1).unwrap();
            cpu.write_guest_u16(AUDIO_FREQUENCY, 220).unwrap();
            cpu.write_guest(AUDIO_COMMAND, 1).unwrap();
            cpu.write_guest(AUDIO_CHANNEL, 0).unwrap();
            cpu.write_guest(AUDIO_COMMAND, 2).unwrap();
            assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 9);
            assert_eq!(
                cpu.take_sound_commands(),
                [
                    crate::machine::SoundCommand {
                        channel: 0,
                        frequency: 0,
                        frames: 0,
                        waveform: 0,
                        volume: 0
                    },
                    crate::machine::SoundCommand {
                        channel: 3,
                        frequency: 220,
                        frames: u16::MAX,
                        waveform: 2,
                        volume: 75
                    },
                ]
            );
            assert!(
                cpu.memory[alias..alias + 16]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn audio_rejects_bad_commands_without_replacing_pending_sound_or_panicking() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest_u16(AUDIO_FREQUENCY, 440).unwrap();
        cpu.write_guest(AUDIO_COMMAND, 1).unwrap();
        let expected = crate::machine::SoundCommand {
            channel: 0,
            frequency: 440,
            frames: 0,
            waveform: 0,
            volume: 0,
        };
        for (command, channel, waveform, volume, status) in [
            (0, 0, 0, 0, 1),
            (255, 0, 0, 0, 1),
            (1, 4, 0, 0, 2),
            (2, 255, 0, 0, 2),
            (1, 0, 3, 0, 3),
            (1, 0, u16::MAX, 0, 3),
            (1, 0, 0, 101, 4),
            (1, 0, 0, u16::MAX, 4),
        ] {
            cpu.write_guest(AUDIO_CHANNEL, channel).unwrap();
            cpu.write_guest_u16(AUDIO_WAVEFORM, waveform).unwrap();
            cpu.write_guest_u16(AUDIO_VOLUME, volume).unwrap();
            cpu.write_guest(AUDIO_COMMAND, command).unwrap();
            assert_eq!(cpu.read_guest(AUDIO_STATUS).unwrap(), status);
            assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 1);
        }
        assert_eq!(cpu.take_sound_commands(), [expected]);
        cpu.write_guest(AUDIO_CHANNEL, 2).unwrap();
        cpu.write_guest(AUDIO_COMMAND, 2).unwrap();
        assert_eq!(cpu.read_guest(AUDIO_STATUS).unwrap(), 0);
        assert_eq!(
            cpu.take_sound_commands(),
            [crate::machine::SoundCommand {
                channel: 2,
                frequency: 0,
                frames: 0,
                waveform: 0,
                volume: 0,
            }]
        );
    }

    #[test]
    fn audio_validation_is_atomic_and_image_entry_preserves_pending_commands() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest(AUDIO_CHANNEL, 1).unwrap();
        cpu.write_guest_u16(AUDIO_FREQUENCY, 440).unwrap();
        cpu.write_guest(AUDIO_COMMAND, 1).unwrap();
        assert!(cpu.write_guest_u16(AUDIO_COMMAND, 0x0200).is_err());
        assert!(cpu.write_guest_u16(AUDIO_VOLUME + 1, 0x4b00).is_err());
        assert!(cpu.write_guest_u16(AUDIO_CHANNEL, 0x0300).is_err());
        cpu.privilege_mode = PrivilegeMode::User;
        assert!(cpu.write_guest(AUDIO_COMMAND, 2).is_err());
        cpu.privilege_mode = PrivilegeMode::Supervisor;
        assert_eq!(cpu.read_guest(AUDIO_CHANNEL).unwrap(), 1);
        assert_eq!(cpu.read_guest_u16(AUDIO_VOLUME).unwrap(), 0);
        assert_eq!(cpu.read_guest(AUDIO_STATUS).unwrap(), 0);
        cpu.prepare_image_entry();
        assert_eq!(cpu.read_guest(AUDIO_PENDING).unwrap(), 2);
        assert_eq!(cpu.read_guest_u16(AUDIO_FREQUENCY).unwrap(), 440);
        assert_eq!(
            cpu.take_sound_commands(),
            [crate::machine::SoundCommand {
                channel: 1,
                frequency: 440,
                frames: 0,
                waveform: 0,
                volume: 0,
            }]
        );
    }

    #[test]
    fn small_memory_guest_accesses_fault_instead_of_wrapping_into_ram() {
        use crate::mmu::{MemoryAccess, PageFaultReason};

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.memory[0] = 0x5a;

        assert_eq!(
            cpu.read_guest(1 << 16),
            Err(CpuError::PageFault(crate::mmu::PageFault {
                virtual_address: 1 << 16,
                access: MemoryAccess::Read,
                reason: PageFaultReason::PhysicalOutOfRange,
            }))
        );
        assert_eq!(
            cpu.write_guest(1 << 16, 0xa5),
            Err(CpuError::PageFault(crate::mmu::PageFault {
                virtual_address: 1 << 16,
                access: MemoryAccess::Write,
                reason: PageFaultReason::PhysicalOutOfRange,
            }))
        );
        assert_eq!(cpu.memory[0], 0x5a);
        assert_eq!(
            cpu.read_guest(u32::MAX),
            Err(CpuError::PageFault(crate::mmu::PageFault {
                virtual_address: u32::MAX,
                access: MemoryAccess::Read,
                reason: PageFaultReason::AddressOutOfRange,
            }))
        );
    }

    #[test]
    fn high_control_banks_validate_permissions_offsets_and_translated_aliases() {
        use crate::machine::*;
        use crate::mmu::{MemoryAccess, PageFaultReason};

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
        cpu.memory[0x3024..0x3028].copy_from_slice(&(INTERRUPT_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3028..0x302c].copy_from_slice(&(FAULT_BASE | 0xf).to_be_bytes());
        cpu.memory[0x302c..0x3030].copy_from_slice(&(CLOCK_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3030..0x3034].copy_from_slice(&(INPUT_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3034..0x3038]
            .copy_from_slice(&(crate::machine::CONSOLE_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3038..0x303c].copy_from_slice(&(FIRMWARE_TEXT_BASE | 0xf).to_be_bytes());
        cpu.memory[0x303c..0x3040].copy_from_slice(&(AUDIO_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3040..0x3044].copy_from_slice(&(NETWORK_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3044..0x3048].copy_from_slice(&(FIRMWARE_GATEWAY_BASE | 0xf).to_be_bytes());
        cpu.memory[0x3048..0x304c].copy_from_slice(&(FILESYSTEM_BASE | 0xf).to_be_bytes());
        cpu.enable_mmu(0x2000).unwrap();
        for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
            cpu.privilege_mode = mode;
            for (base, page) in [
                (0x9000, INTERRUPT_BASE),
                (0xa000, FAULT_BASE),
                (0xb000, CLOCK_BASE),
                (0xc000, INPUT_BASE),
                (0xd000, crate::machine::CONSOLE_BASE),
                (0xe000, FIRMWARE_TEXT_BASE),
                (0xf000, AUDIO_BASE),
                (0x10000, NETWORK_BASE),
                (0x11000, FIRMWARE_GATEWAY_BASE),
                (0x12000, FILESYSTEM_BASE),
            ] {
                for offset in 0..DEVICE_BANK_SIZE {
                    for access in [MemoryAccess::Read, MemoryAccess::Write, MemoryAccess::Fetch] {
                        let expected = if mode == PrivilegeMode::User {
                            Some(PageFaultReason::SupervisorOnly)
                        } else if access == MemoryAccess::Fetch {
                            Some(PageFaultReason::NotExecutable)
                        } else {
                            match device_register_writable(page + offset) {
                                None => Some(PageFaultReason::PhysicalOutOfRange),
                                Some(false) if access == MemoryAccess::Write => {
                                    Some(PageFaultReason::ReadOnly)
                                }
                                _ => None,
                            }
                        };
                        let result = cpu.resolve_guest_range(base + offset, 1, access);
                        if let Some(reason) = expected {
                            assert_eq!(
                                result,
                                Err(CpuError::PageFault(crate::mmu::PageFault {
                                    virtual_address: base + offset,
                                    access,
                                    reason,
                                }))
                            );
                        } else {
                            assert_eq!(result.unwrap(), vec![page + offset]);
                        }
                    }
                }
                for user in [false, true] {
                    assert!(
                        cpu.guest_memory_addresses(base, 1, MemoryAccess::Read, user)
                            .is_err()
                    );
                }
            }
        }
        cpu.privilege_mode = PrivilegeMode::Supervisor;
        cpu.write_guest_u16(0x9008, 0x1234).unwrap();
        assert_eq!(cpu.read_guest_u16(0x9008).unwrap(), 0x1234);
        cpu.record_page_fault(crate::mmu::PageFault {
            virtual_address: 0x9123,
            access: MemoryAccess::Write,
            reason: PageFaultReason::ReadOnly,
        });
        assert_eq!(cpu.read_guest_u32(0xa000).unwrap(), 0x9123);
        assert_eq!(cpu.read_guest_u16(0xa004).unwrap(), 0x0203);
        assert_eq!(cpu.read_guest(0xa006).unwrap(), 1);
        assert_eq!(cpu.read_u16(crate::machine::FAULT_INFO), 0x0203);
        cpu.prepare_image_entry();
        assert_eq!(cpu.read(FAULT_VALID), 0);
        assert_eq!(cpu.read_u32(FAULT_ADDRESS), 0);
        assert_eq!(cpu.read_u16(FAULT_INFO), 0);
        assert_eq!(cpu.read(INTERRUPT_ENABLE), 0);
    }

    #[test]
    fn firmware_gateway_uses_inline_responses_without_ram_backing() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-high-firmware-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("other")).unwrap();
        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            cpu.disk_root = root.clone();
            cpu.bios_settings.network_name = "NODE".to_owned();
            cpu.bios_settings.preferred_disk = Some("other".to_owned());
            cpu.bios_settings.startup_behavior = StartupBehavior::BootPreferred;
            cpu.bios_settings
                .additional
                .insert("count".to_owned(), BiosSettingValue::Integer(i64::MIN));
            let alias = (FIRMWARE_GATEWAY_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 0x300].fill(0xa5);
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 0);
            cpu.write_guest(FIRMWARE_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FIRMWARE_INDEX).unwrap(), 1);
            assert_eq!(cpu.read_guest_u16(FIRMWARE_DISK_COUNT).unwrap(), 2);
            assert_eq!(cpu.read_guest(FIRMWARE_STARTUP).unwrap(), 1);
            assert_eq!(cpu.read_guest(FIRMWARE_SETTING_COUNT).unwrap(), 1);
            assert_eq!(cpu.read_guest(FIRMWARE_NETWORK_NAME).unwrap(), b'N');
            cpu.write_guest(FIRMWARE_COMMAND, 2).unwrap();
            assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 5);
            for (offset, byte) in b"other\0".iter().enumerate() {
                assert_eq!(
                    cpu.read_guest(FIRMWARE_OUTPUT + offset as u32).unwrap(),
                    *byte
                );
            }
            cpu.write_guest_u16(FIRMWARE_INDEX, u16::MAX).unwrap();
            cpu.write_guest(FIRMWARE_COMMAND, 2).unwrap();
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
            assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 0);
            assert_eq!(cpu.read_guest(FIRMWARE_OUTPUT).unwrap(), b'o');
            cpu.write_guest_u16(FIRMWARE_INDEX, 0).unwrap();
            cpu.write_guest(FIRMWARE_COMMAND, 2).unwrap();
            assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 3);
            assert_eq!(cpu.read_guest(FIRMWARE_OUTPUT + 4).unwrap(), 0);
            cpu.write_guest(FIRMWARE_COMMAND, 6).unwrap();
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest(FIRMWARE_SETTING + 17).unwrap(), 2);
            let value = i64::MIN.to_string();
            for (offset, byte) in value.bytes().enumerate() {
                assert_eq!(
                    cpu.read_guest(FIRMWARE_SETTING + 18 + offset as u32)
                        .unwrap(),
                    byte
                );
            }
            cpu.write_guest(FIRMWARE_COMMAND, 255).unwrap();
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
            cpu.prepare_image_entry();
            assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
            assert_eq!(cpu.read_guest(FIRMWARE_SETTING + 17).unwrap(), 2);
            assert_eq!(cpu.read_guest(FIRMWARE_OUTPUT).unwrap(), 0);
            assert!(
                cpu.memory[alias..alias + 0x300]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn firmware_gateway_rejects_wide_side_effects_and_oversized_responses_atomically() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest_u16(FIRMWARE_INDEX, 7).unwrap();
        assert!(cpu.write_guest_u16(FIRMWARE_COMMAND, 0x0100).is_err());
        assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 0);
        assert_eq!(cpu.read_guest_u16(FIRMWARE_INDEX).unwrap(), 7);
        assert!(cpu.write_guest_u32(FIRMWARE_INDEX, 0x12345678).is_err());
        assert_eq!(cpu.read_guest_u16(FIRMWARE_INDEX).unwrap(), 7);
        cpu.write_guest(FIRMWARE_SETTING + 41, 0x33).unwrap();
        assert!(cpu.write_guest_u16(FIRMWARE_SETTING + 41, 0x4455).is_err());
        assert_eq!(cpu.read_guest(FIRMWARE_SETTING + 41).unwrap(), 0x33);
        assert!(cpu.read_guest_u16(FIRMWARE_OUTPUT + 511).is_err());
        let request = cpu.firmware_gateway.request();
        cpu.firmware_gateway
            .complete(request, Some(&"x".repeat(511)))
            .unwrap();
        assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 511);
        assert_eq!(cpu.read_guest(FIRMWARE_OUTPUT + 511).unwrap(), 0);
        assert!(
            cpu.firmware_gateway
                .complete(request, Some(&"y".repeat(512)))
                .is_err()
        );
        assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
        assert_eq!(cpu.read_guest_u16(FIRMWARE_OUTPUT_LENGTH).unwrap(), 0);
        assert_eq!(cpu.read_guest(FIRMWARE_OUTPUT).unwrap(), b'x');
        cpu.write_guest(FIRMWARE_SETTING + 17, 0).unwrap();
        cpu.write_guest(FIRMWARE_COMMAND, 7).unwrap();
        assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
        assert!(cpu.bios_settings.additional.is_empty());
    }

    #[test]
    fn high_firmware_gateway_shares_settings_and_validates_persistence() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-high-firmware-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disks = root.join("disks");
        let app = disks.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let manifest = serde_json::json!({
            "version": 1, "id": "app", "label": "Application",
            "capacity_bytes": 100000, "read_only": false, "protected_files": []
        });
        std::fs::write(app.join("disk.json"), manifest.to_string()).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.disk_root = disks;
        cpu.configure_bios_settings(root.join("bios.json")).unwrap();
        cpu.bios_settings.network_name = "NODE".to_owned();
        cpu.bios_settings
            .additional
            .insert("enabled".to_owned(), BiosSettingValue::Boolean(false));
        cpu.write(FIRMWARE_COMMAND, 1);
        cpu.write(FIRMWARE_COMMAND, 6);
        assert_eq!(cpu.read(FIRMWARE_SETTING + 17), 0);
        for (offset, byte) in b"Yes\0".iter().enumerate() {
            cpu.write(FIRMWARE_SETTING + 18 + offset as u32, *byte);
        }
        cpu.write(FIRMWARE_COMMAND, 7);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(cpu.read(FIRMWARE_SETTING + 18), b'Y');
        for (offset, byte) in b"No\0\0".iter().enumerate() {
            cpu.write(FIRMWARE_SETTING + 18 + offset as u32, *byte);
        }
        cpu.write(FIRMWARE_COMMAND, 7);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        cpu.write(FIRMWARE_COMMAND, 6);
        assert_eq!(cpu.read(FIRMWARE_SETTING + 18), b'N');
        cpu.write(FIRMWARE_STARTUP, 1);
        cpu.write(FIRMWARE_COMMAND, 4);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        let saved = std::fs::read(root.join("bios.json")).unwrap();
        assert_eq!(cpu.bios_settings.preferred_disk.as_deref(), Some("app"));
        cpu.write(FIRMWARE_STARTUP, 2);
        cpu.write(FIRMWARE_COMMAND, 4);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 1);
        assert_eq!(std::fs::read(root.join("bios.json")).unwrap(), saved);
        cpu.bios_settings.network_name = "CHANGED".to_owned();
        cpu.write(FIRMWARE_COMMAND, 5);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(cpu.read(FIRMWARE_STARTUP), 1);
        assert_eq!(cpu.bios_settings.network_name, "NODE");
        cpu.write(FIRMWARE_COMMAND, 9);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(
            cpu.bios_settings.startup_behavior,
            StartupBehavior::BootPreferred
        );
        cpu.write(FIRMWARE_COMMAND, 8);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        let summary = b"Application | 100000 B | READ-WRITE | NOT_BOOT";
        assert_eq!(cpu.read_u16(FIRMWARE_OUTPUT_LENGTH), summary.len() as u16);
        for (offset, byte) in summary.iter().enumerate() {
            assert_eq!(cpu.read(FIRMWARE_OUTPUT + offset as u32), *byte);
        }
        let mut oversized = manifest;
        oversized["label"] = serde_json::Value::String("x".repeat(512));
        std::fs::write(app.join("disk.json"), oversized.to_string()).unwrap();
        cpu.write(FIRMWARE_COMMAND, 8);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 1);
        assert_eq!(cpu.read_u16(FIRMWARE_OUTPUT_LENGTH), 0);
        assert_eq!(cpu.read(FIRMWARE_OUTPUT), b'A');
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn high_firmware_boot_rejects_out_of_memory_images_before_mutation() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-high-firmware-invalid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            app.join("disk.json"),
            r#"{"version":1,"id":"app","label":"Application","capacity_bytes":65536,"read_only":false,"protected_files":[]}"#,
        )
        .unwrap();
        let invalid =
            crate::assembler::assemble("HALT\n.address 0x10000\n.data hex ff", 0x4000).unwrap();
        crate::image::write_image(&app.join("main.bin"), &invalid).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.disk_root = root.clone();
        cpu.pc = 0x1000;
        cpu.registers[4] = 123;
        cpu.memory[0x4000] = 0xa5;
        cpu.write_guest(FIRMWARE_COMMAND, 3).unwrap();
        assert_eq!(cpu.read_guest(FIRMWARE_STATUS).unwrap(), 1);
        assert_eq!(cpu.pc, 0x1000);
        assert_eq!(cpu.registers[4], 123);
        assert_eq!(cpu.memory[0x4000], 0xa5);
        assert_eq!(cpu.take_booted_disk_path(), None);
        assert!(cpu.folder_disk.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filesystem_bank_uses_bounded_inline_io_and_preserves_disk_guards() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-high-filesystem-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk_path = root.join("DISK0001");
        std::fs::create_dir_all(&disk_path).unwrap();
        std::fs::write(
            disk_path.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Test","capacity_bytes":65536,"read_only":false,"protected_files":["protected.txt"]}"#,
        )
        .unwrap();
        std::fs::write(disk_path.join("data.bin"), b"abcdef").unwrap();
        std::fs::write(disk_path.join("protected.txt"), b"fixed").unwrap();
        fn stage_path(cpu: &mut Cpu, address: u32, path: &[u8]) {
            for offset in 0..128 {
                cpu.write_guest(address + offset, 0).unwrap();
            }
            for (offset, byte) in path.iter().enumerate() {
                cpu.write_guest(address + offset as u32, *byte).unwrap();
            }
        }

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            std::fs::write(disk_path.join("data.bin"), b"abcdef").unwrap();
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            cpu.mount_folder_disk(&disk_path).unwrap();
            let alias = (FILESYSTEM_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 0x400].fill(0xa5);

            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_LIST)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_RESULT).unwrap(), 2);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_OUTPUT_LENGTH).unwrap(), 14);
            assert_eq!(&cpu.filesystem_device.data[..14], b"data.bin\0\0\0\0\0\x06");

            stage_path(&mut cpu, FILESYSTEM_PATH, b"data.bin\0");
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_READ)
                .unwrap();
            cpu.write_guest_u32(FILESYSTEM_OFFSET, 2).unwrap();
            cpu.write_guest_u16(FILESYSTEM_LENGTH, 2).unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_RESULT).unwrap(), 2);
            assert_eq!(cpu.read_guest_u32(FILESYSTEM_FILE_SIZE).unwrap(), 6);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_OUTPUT_LENGTH).unwrap(), 2);
            assert_eq!(&cpu.filesystem_device.data[..2], b"cd");

            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_WRITE)
                .unwrap();
            cpu.write_guest_u16(FILESYSTEM_LENGTH, 3).unwrap();
            for (offset, byte) in b"new".iter().enumerate() {
                cpu.write_guest(FILESYSTEM_DATA + offset as u32, *byte)
                    .unwrap();
            }
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u32(FILESYSTEM_FILE_SIZE).unwrap(), 3);

            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_STAT)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_RESULT).unwrap(), 5);
            assert_eq!(cpu.read_guest_u32(FILESYSTEM_FILE_SIZE).unwrap(), 3);
            assert_eq!(cpu.filesystem_device.data[0], 0);

            stage_path(&mut cpu, FILESYSTEM_PATH, b"docs\0");
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_CREATE_DIR)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_LIST)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_RESULT).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_OUTPUT_LENGTH).unwrap(), 1);
            assert_eq!(cpu.filesystem_device.data[0], 0);

            stage_path(&mut cpu, FILESYSTEM_PATH, b"docs\0");
            stage_path(&mut cpu, FILESYSTEM_DESTINATION, b"archive\0");
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_RENAME)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            assert!(disk_path.join("archive").is_dir());
            stage_path(&mut cpu, FILESYSTEM_PATH, b"archive\0");
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_DELETE)
                .unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
            assert!(!disk_path.join("archive").exists());

            stage_path(&mut cpu, FILESYSTEM_PATH, b"protected.txt\0");
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_WRITE)
                .unwrap();
            cpu.write_guest(FILESYSTEM_LENGTH, 1).unwrap();
            cpu.write_guest(FILESYSTEM_DATA, b'X').unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 4);
            assert_eq!(
                std::fs::read(disk_path.join("protected.txt")).unwrap(),
                b"fixed"
            );

            cpu.write_guest(FILESYSTEM_OPERATION, 0xff).unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 1);
            cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_READ)
                .unwrap();
            cpu.write_guest_u16(FILESYSTEM_LENGTH, 513).unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 2);
            assert_eq!(cpu.read_guest_u16(FILESYSTEM_OUTPUT_LENGTH).unwrap(), 0);

            for offset in 0..128 {
                cpu.write_guest(FILESYSTEM_PATH + offset, b'x').unwrap();
            }
            cpu.write_guest(FILESYSTEM_LENGTH, 0).unwrap();
            cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 2);

            cpu.prepare_image_entry();
            assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 2);
            assert!(
                cpu.memory[alias..alias + 0x400]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filesystem_bank_rejects_wide_side_effects_and_maps_no_disk_errors() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest(FILESYSTEM_OPERATION, FOLDER_FS_LIST)
            .unwrap();
        assert!(cpu.write_guest_u16(FILESYSTEM_COMMAND, 0x0100).is_err());
        assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 0);
        assert!(cpu.write_guest_u32(FILESYSTEM_LENGTH, 0x12345678).is_err());
        assert_eq!(cpu.read_guest_u16(FILESYSTEM_LENGTH).unwrap(), 0);
        cpu.write_guest(FILESYSTEM_DATA + 511, 0x33).unwrap();
        assert!(cpu.write_guest_u16(FILESYSTEM_DATA + 511, 0x4455).is_err());
        assert_eq!(cpu.read_guest(FILESYSTEM_DATA + 511).unwrap(), 0x33);
        assert!(cpu.read_guest_u16(FILESYSTEM_DATA + 511).is_err());
        cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
        assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 3);
        assert_eq!(cpu.read_guest_u16(FILESYSTEM_RESULT).unwrap(), 0);
        cpu.write_guest(FILESYSTEM_OPERATION, 0xff).unwrap();
        cpu.write_guest(FILESYSTEM_COMMAND, 1).unwrap();
        assert_eq!(cpu.read_guest(FILESYSTEM_STATUS).unwrap(), 1);
    }

    #[test]
    fn network_bank_staging_status_and_lifetime_are_independent_of_ram() {
        use crate::machine::*;

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let alias = (NETWORK_BASE & cpu.memory_mask) as usize;
            cpu.memory[alias..alias + 128].fill(0xa5);
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 0);
            assert_eq!(cpu.read_guest_u16(NETWORK_RESULT).unwrap(), 0);
            for offset in 0..16 {
                assert_eq!(cpu.read_guest(NETWORK_TITLE + offset).unwrap(), 0);
                cpu.write_guest(NETWORK_TITLE + offset, (offset + 1) as u8)
                    .unwrap();
            }
            for offset in 0..48 {
                assert_eq!(cpu.read_guest(NETWORK_TRANSMIT + offset).unwrap(), 0);
                assert_eq!(cpu.read_guest(NETWORK_RECEIVE + offset).unwrap(), 0);
                cpu.write_guest(NETWORK_TRANSMIT + offset, (offset + 2) as u8)
                    .unwrap();
            }
            assert!(cpu.network.is_none());
            cpu.write_guest(NETWORK_COMMAND, 0).unwrap();
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 1);
            cpu.write_guest(NETWORK_LENGTH, 49).unwrap();
            cpu.write_guest(NETWORK_COMMAND, 3).unwrap();
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 2);
            cpu.write_guest(NETWORK_LENGTH, 48).unwrap();
            cpu.write_guest(NETWORK_COMMAND, 3).unwrap();
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 4);
            cpu.write_guest(NETWORK_COMMAND, 4).unwrap();
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 4);
            cpu.write_guest(NETWORK_QUERY, 7).unwrap();
            cpu.write_guest(NETWORK_COMMAND, 5).unwrap();
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 7);
            cpu.write_guest(NETWORK_QUERY, 1).unwrap();
            cpu.write_guest(NETWORK_COMMAND, 5).unwrap();
            assert_eq!(cpu.read_guest_u16(NETWORK_RESULT).unwrap(), u16::MAX);
            assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 0);
            cpu.prepare_image_entry();
            assert_eq!(cpu.read_guest_u16(NETWORK_RESULT).unwrap(), u16::MAX);
            assert_eq!(cpu.read_guest(NETWORK_COMMAND).unwrap(), 0);
            for offset in 0..16 {
                assert_eq!(
                    cpu.read_guest(NETWORK_TITLE + offset).unwrap(),
                    (offset + 1) as u8
                );
            }
            for offset in 0..48 {
                assert_eq!(
                    cpu.read_guest(NETWORK_TRANSMIT + offset).unwrap(),
                    (offset + 2) as u8
                );
            }
            assert!(
                cpu.memory[alias..alias + 128]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn rejected_network_wide_accesses_do_not_commit_or_change_staging() {
        use crate::machine::*;
        use crate::mmu::PageFaultReason;

        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_guest(NETWORK_TITLE, b'G').unwrap();
        let error = cpu.write_guest_u16(NETWORK_COMMAND, 0x0100).unwrap_err();
        assert!(matches!(
            error,
            CpuError::PageFault(crate::mmu::PageFault {
                virtual_address: NETWORK_STATUS,
                reason: PageFaultReason::ReadOnly,
                ..
            })
        ));
        assert!(cpu.network.is_none());
        assert_eq!(cpu.read_guest(NETWORK_STATUS).unwrap(), 0);
        cpu.write_guest(NETWORK_RECEIVE - 1, 0x33).unwrap();
        assert!(cpu.write_guest_u16(NETWORK_RECEIVE - 1, 0x4455).is_err());
        assert_eq!(cpu.read_guest(NETWORK_RECEIVE - 1).unwrap(), 0x33);
        assert!(cpu.read_guest_u16(NETWORK_RECEIVE + 47).is_err());
        assert!(cpu.write_guest_u16(NETWORK_RESULT - 1, 0x1234).is_err());
        assert_eq!(cpu.read_guest_u16(NETWORK_ARGUMENT).unwrap(), 0);
    }

    #[test]
    fn high_interrupt_registers_are_independent_of_retired_ram_addresses() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(INTERRUPT_ENABLE, 1);
        cpu.signal_keyboard_event(b'K');
        cpu.raise_rtc_interrupt();
        assert_eq!(cpu.read(INTERRUPT_ENABLE), 1);
        assert_eq!(cpu.read(INTERRUPT_PENDING), 0x0a);
        cpu.write(INTERRUPT_ACK, 0xff);
        assert_eq!(cpu.read(INTERRUPT_PENDING), 0);
        cpu.write_guest(crate::machine::INTERRUPT_PIT_ENABLE, 0)
            .unwrap();
        assert_eq!(cpu.read(INTERRUPT_PIT_ENABLE), 0);
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 0x1234)
            .unwrap();
        assert_eq!(cpu.read_u16(INTERRUPT_PIT_PERIOD), 0x1234);
        cpu.write_u16(INTERRUPT_PIT_PERIOD, 0x5678);
        assert_eq!(
            cpu.read_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD)
                .unwrap(),
            0x5678
        );
        for (address, value) in [(0x43002, 0xff), (0x43016, 0xfe), (0x4301e, 0xfd)] {
            cpu.write(address, value);
            assert_eq!(cpu.read(address), value);
        }
        assert_eq!(cpu.read(INTERRUPT_ENABLE), 1);
        assert_eq!(cpu.read(INTERRUPT_PENDING), 0);
        assert_eq!(
            cpu.read_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD)
                .unwrap(),
            0x5678
        );
    }

    #[test]
    fn system_bank_is_read_only_supervisor_data_without_ram_backing() {
        use crate::machine::*;
        use crate::mmu::{MemoryAccess, PageFaultReason};

        for size in [1 << 16, 1 << 20, MEMORY_SIZE] {
            let mut cpu = Cpu::with_memory_size(size).unwrap();
            let aliased = (SYSTEM_BASE & cpu.memory_mask) as usize;
            cpu.memory[aliased..aliased + 20].fill(0xa5);
            for (register, value) in [
                (SYSTEM_MACHINE_REVISION, MACHINE_REVISION),
                (SYSTEM_DEVICE_ABI, DEVICE_ABI),
                (SYSTEM_RAM_BYTES, usable_ram_size(size) as u32),
                (SYSTEM_ADDRESS_BITS, 30),
                (SYSTEM_BANKS, IMPLEMENTED_BANKS),
            ] {
                assert_eq!(cpu.read_guest_u32(register).unwrap(), value);
                assert_eq!(cpu.read_u32(register), value);
                assert_eq!(
                    cpu.read_guest_u16(register + 1).unwrap(),
                    (value >> 8) as u16
                );
                assert!(matches!(
                    cpu.write_guest_u32(register, 0x5a5a5a5a),
                    Err(CpuError::PageFault(crate::mmu::PageFault {
                        reason: PageFaultReason::ReadOnly,
                        ..
                    }))
                ));
            }
            assert!(cpu.read_guest_u16(SYSTEM_BANKS + 3).is_err());
            assert!(cpu.read_guest_u32(SYSTEM_BANKS + 1).is_err());
            for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
                cpu.privilege_mode = mode;
                for address in SYSTEM_BASE..SYSTEM_BASE + 20 {
                    for access in [MemoryAccess::Read, MemoryAccess::Write, MemoryAccess::Fetch] {
                        let result = cpu.check_guest_access(address, access);
                        let reason = if mode == PrivilegeMode::User {
                            Some(PageFaultReason::SupervisorOnly)
                        } else {
                            match access {
                                MemoryAccess::Read => None,
                                MemoryAccess::Write => Some(PageFaultReason::ReadOnly),
                                MemoryAccess::Fetch => Some(PageFaultReason::NotExecutable),
                            }
                        };
                        if let Some(reason) = reason {
                            assert_eq!(
                                result,
                                Err(CpuError::PageFault(crate::mmu::PageFault {
                                    virtual_address: address,
                                    access,
                                    reason,
                                }))
                            );
                        } else {
                            result.unwrap();
                        }
                    }
                }
                for address in [
                    SYSTEM_BASE + 20,
                    SYSTEM_BASE + 4095,
                    FIRMWARE_GATEWAY_BASE + DEVICE_BANK_SIZE,
                    ADDRESS_MASK,
                ] {
                    assert!(cpu.read_guest(address).is_err());
                }
            }
            assert!(
                cpu.memory[aliased..aliased + 20]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );
        }
    }

    #[test]
    fn translated_system_bank_rejects_aliases_fetches_and_buffer_copies() {
        use crate::machine::*;
        use crate::mmu::{MemoryAccess, PageFault, PageFaultReason};

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
        cpu.memory[0x3024..0x3028].copy_from_slice(&(SYSTEM_BASE | 0xf).to_be_bytes());
        cpu.enable_mmu(0x2000).unwrap();
        assert_eq!(cpu.read_guest_u32(0x9008).unwrap(), 1 << 20);
        for (access, reason) in [
            (MemoryAccess::Write, PageFaultReason::ReadOnly),
            (MemoryAccess::Fetch, PageFaultReason::NotExecutable),
        ] {
            assert_eq!(
                cpu.resolve_guest_range(0x9000, 4, access),
                Err(CpuError::PageFault(PageFault {
                    virtual_address: 0x9000,
                    access,
                    reason
                }))
            );
        }
        assert_eq!(
            cpu.read_guest(0x9014),
            Err(CpuError::PageFault(PageFault {
                virtual_address: 0x9014,
                access: MemoryAccess::Read,
                reason: PageFaultReason::PhysicalOutOfRange,
            }))
        );
        for user in [false, true] {
            assert!(
                cpu.guest_memory_addresses(0x9000, 4, MemoryAccess::Read, user)
                    .is_err()
            );
        }
        cpu.privilege_mode = PrivilegeMode::User;
        assert_eq!(
            cpu.read_guest(0x9000),
            Err(CpuError::PageFault(PageFault {
                virtual_address: 0x9000,
                access: MemoryAccess::Read,
                reason: PageFaultReason::SupervisorOnly,
            }))
        );
    }

    #[test]
    fn instruction_operands_cannot_mask_reserved_devices_into_small_ram() {
        let address = crate::machine::FIRMWARE_GATEWAY_BASE + crate::machine::DEVICE_BANK_SIZE;
        for source in [
            "LD R2, R0",
            "ST R0, R2",
            "LDW R2, R0",
            "STW R0, R2",
            "LDP R2, R0",
            "STP R2, R0",
            "LD32 R4, R0",
            "ST32 R4, R0",
            "LDWS R2, 0",
            "STWS R2, 0",
            "PUSH R2",
            "POP R2",
            "RET",
        ] {
            let image = crate::assembler::assemble(source, 0x100).unwrap();
            for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
                let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
                cpu.load_program(&image.segments, image.entry_point)
                    .unwrap();
                cpu.registers[0] = i64::from(address);
                if source.contains("32") {
                    cpu.registers[0] = i64::from(address >> 16);
                    cpu.registers[1] = i64::from(address & 0xffff);
                }
                cpu.sp = address + if source == "PUSH R2" { 2 } else { 0 };
                cpu.privilege_mode = mode;
                let memory = cpu.memory.to_vec();
                let registers = cpu.registers;
                let sp = cpu.sp;
                assert!(
                    matches!(cpu.execute_instruction(), Err(CpuError::PageFault(_))),
                    "{source}: {mode:?}"
                );
                assert_eq!(&cpu.memory[..], memory, "{source}");
                assert_eq!(cpu.registers, registers, "{source}");
                assert_eq!(cpu.sp, sp, "{source}");
            }
        }
    }

    #[test]
    fn far_control_flow_does_not_mask_device_fetch_targets_into_ram() {
        let target = crate::machine::SYSTEM_BASE;
        for source in [
            format!("JMPX {target}"),
            format!("CALLX {target}"),
            format!("LDI R0, {}\nLDI R1, 0\nJMPRP R1, R0", target >> 16),
            format!("LDI R0, {}\nLDI R1, 0\nCALLRP R1, R0", target >> 16),
        ] {
            let image = crate::assembler::assemble(&source, 0x100).unwrap();
            let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            for _ in 0..8 {
                if cpu.pc == target {
                    break;
                }
                cpu.execute_instruction().unwrap();
            }
            assert_eq!(cpu.pc, target, "{source}");
            assert!(matches!(
                cpu.execute_instruction(),
                Err(CpuError::PageFault(crate::mmu::PageFault {
                    virtual_address,
                    reason: crate::mmu::PageFaultReason::NotExecutable,
                    ..
                })) if virtual_address == target
            ));
        }
    }

    #[test]
    fn rejected_program_load_preserves_memory_and_running_image() {
        let initial = crate::assembler::assemble("LDI R0, 42\nHALT", 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&initial.segments, initial.entry_point)
            .unwrap();
        cpu.registers[3] = 0x1234;
        cpu.sp = 0x7000;
        let memory = cpu.memory.to_vec();
        let metadata = cpu.loaded_image_segments.clone();
        let logs = cpu.debug_console.output().to_vec();
        for (segments, entry, expected) in [
            (
                vec![
                    crate::image::ImageSegment::new(0x100, vec![0xaa; 4]),
                    crate::image::ImageSegment::new(0xffff, vec![0xbb; 2]),
                ],
                0x100,
                CpuError::SegmentOutOfBounds {
                    address: 0xffff,
                    length: 2,
                },
            ),
            (
                vec![crate::image::ImageSegment::new(0x100, vec![0xaa; 4])],
                0x10000,
                CpuError::InvalidEntryPoint(0x10000),
            ),
        ] {
            assert_eq!(cpu.load_program(&segments, entry), Err(expected));
            assert_eq!(&cpu.memory[..], memory);
            assert_eq!(cpu.loaded_image_segments, metadata);
            assert_eq!(cpu.debug_console.output(), logs);
            assert_eq!(cpu.pc, 0x100);
            assert_eq!(cpu.sp, 0x7000);
            assert_eq!(cpu.registers[3], 0x1234);
            assert!(cpu.running);
        }
        cpu.run(2).unwrap();
        assert_eq!(cpu.registers[0], 42);
        assert!(!cpu.running);
    }

    fn install_test_handler(cpu: &mut Cpu, vector: u16, handler: u32, privilege: u8) {
        if cpu.idt_base.is_none() {
            cpu.load_idt(0x800).unwrap();
        }
        cpu.set_idt_descriptor(
            vector,
            crate::interrupts::IdtDescriptor {
                handler,
                present: true,
                privilege,
                gate_type: crate::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
    }

    fn cpu_with_syscall_task() -> (Cpu, TaskId) {
        cpu_with_syscall_task_memory(1 << 16)
    }

    fn cpu_with_syscall_task_memory(memory_size: usize) -> (Cpu, TaskId) {
        let source = "SVC\n".repeat(32) + "HALT";
        let image = crate::assembler::assemble(&source, 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(memory_size).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0x100, 0x7000, 0xf000, None).unwrap();
        cpu.start_task(task).unwrap();
        (cpu, task)
    }

    #[test]
    fn user_buffers_use_ordinary_ram_separate_from_device_framebuffer() {
        let (mut cpu, task) = cpu_with_syscall_task_memory(1 << 20);
        cpu.console.receive(b"input");
        cpu.memory[FORMER_FRAMEBUFFER_START as usize - 1..FORMER_FRAMEBUFFER_START as usize + 8]
            .fill(0xa5);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [0, i64::from(FORMER_FRAMEBUFFER_START), 2, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        assert_eq!(
            &cpu.memory[FORMER_FRAMEBUFFER_START as usize..FORMER_FRAMEBUFFER_START as usize + 2],
            b"in"
        );
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Write,
            [1, i64::from(FORMER_FRAMEBUFFER_START), 2, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        assert!(cpu.console.output().ends_with(b"in"));
        assert_eq!(cpu.memory[FORMER_FRAMEBUFFER_START as usize - 1], 0xa5);
        cpu.memory[FORMER_FRAMEBUFFER_START as usize] = 77;
        assert_eq!(cpu.vram[0], 0);
        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .arguments = vec!["app".to_owned()];
        invoke_syscall(&mut cpu, SyscallNumber::Argument, [0, 0x1000, 8, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 3);
        assert_eq!(&cpu.memory[0x1000..0x1003], b"app");
        invoke_syscall(&mut cpu, SyscallNumber::Read, [0, 0x1000, 3, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 3);
        assert_eq!(&cpu.memory[0x1000..0x1003], b"put");
    }

    #[test]
    fn translated_user_buffers_can_use_former_framebuffer_frames_as_ram() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let image = crate::assembler::assemble(&("SVC\n".repeat(32) + "HALT"), 0x100).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let root = manager.create_address_space(&mut cpu.memory).unwrap();
        for (page, permissions) in [
            (0, crate::mmu::PagePermissions::USER_CODE),
            (0x7000, crate::mmu::PagePermissions::USER_DATA),
            (0xe000, crate::mmu::PagePermissions::KERNEL_DATA),
        ] {
            manager
                .map_borrowed_page(
                    &mut cpu.memory,
                    root,
                    page,
                    page / crate::mmu::PAGE_SIZE as u32,
                    permissions,
                )
                .unwrap();
        }
        for (virtual_address, frame) in [
            (0x1000, 0x90),
            (
                0x2000,
                FORMER_FRAMEBUFFER_START / crate::mmu::PAGE_SIZE as u32,
            ),
            (
                0x3000,
                (FORMER_FRAMEBUFFER_START + 153599) / crate::mmu::PAGE_SIZE as u32,
            ),
        ] {
            manager
                .map_borrowed_page(
                    &mut cpu.memory,
                    root,
                    virtual_address,
                    frame,
                    crate::mmu::PagePermissions::USER_DATA,
                )
                .unwrap();
        }
        cpu.enable_mmu(root).unwrap();
        let task = cpu
            .create_user_task(0x100, 0x8000, 0xf000, Some(root))
            .unwrap();
        cpu.start_task(task).unwrap();
        cpu.console.receive(b"input");
        cpu.memory[0x90fff] = 0xa5;
        assert_eq!(
            cpu.guest_memory_addresses(0x2000, 2, crate::mmu::MemoryAccess::Write, true)
                .unwrap(),
            [
                FORMER_FRAMEBUFFER_START as usize,
                FORMER_FRAMEBUFFER_START as usize + 1
            ]
        );
        assert_eq!(
            cpu.guest_memory_addresses(0x3000, 2, crate::mmu::MemoryAccess::Write, true)
                .unwrap(),
            [
                (FORMER_FRAMEBUFFER_START + 153599) as usize & !0xfff,
                ((FORMER_FRAMEBUFFER_START + 153599) as usize & !0xfff) + 1
            ]
        );
        invoke_syscall(&mut cpu, SyscallNumber::Read, [0, 0x2000, 2, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 2);
        assert_eq!(
            &cpu.memory[FORMER_FRAMEBUFFER_START as usize..FORMER_FRAMEBUFFER_START as usize + 2],
            b"in"
        );
        invoke_syscall(&mut cpu, SyscallNumber::Write, [1, 0x2000, 2, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 2);
        assert!(cpu.console.output().ends_with(b"in"));
        assert_eq!(cpu.memory[0x90fff], 0xa5);
    }

    #[test]
    fn file_transfers_use_former_framebuffer_address_as_ordinary_ram() {
        let disk = folder_disk_for_test("framebuffer-transfers", false);
        std::fs::write(disk.join("note.txt"), b"original").unwrap();
        let (mut cpu, _task) = cpu_with_syscall_task_memory(1 << 20);
        cpu.mount_folder_disk(&disk).unwrap();
        cpu.memory[0x1000..0x1009].copy_from_slice(b"note.txt\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1000,
                i64::from(crate::syscall::open_flags::READ | crate::syscall::open_flags::WRITE),
                0,
                0,
                0,
                0,
            ],
        );
        let handle = cpu.registers[0];
        assert!(handle >= 3);
        cpu.memory[FORMER_FRAMEBUFFER_START as usize..FORMER_FRAMEBUFFER_START as usize + 8]
            .copy_from_slice(b"updated!");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Write,
            [handle, i64::from(FORMER_FRAMEBUFFER_START), 8, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 8);
        assert_eq!(std::fs::read(disk.join("note.txt")).unwrap(), b"updated!");
        invoke_syscall(&mut cpu, SyscallNumber::Seek, [handle, 0, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 0);
        invoke_syscall(&mut cpu, SyscallNumber::Read, [handle, 0x2000, 8, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 8);
        assert_eq!(&cpu.memory[0x2000..0x2008], b"updated!");
        std::fs::remove_dir_all(disk.parent().unwrap()).unwrap();
    }

    #[test]
    fn guest_buffers_allow_retired_ram_addresses_but_exclude_device_registers() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        for mode in [0, 1] {
            cpu.set_graphics_mode(mode).unwrap();
            for (address, length) in [
                (crate::machine::DEVICE_APERTURE_START - 1, 2),
                (crate::machine::DEVICE_APERTURE_START, 1),
                (crate::machine::ADDRESS_SPACE_SIZE - 1, 2),
            ] {
                for user in [false, true] {
                    for access in [
                        crate::mmu::MemoryAccess::Read,
                        crate::mmu::MemoryAccess::Write,
                    ] {
                        assert!(
                            cpu.guest_memory_addresses(address, length, access, user)
                                .is_err()
                        );
                    }
                }
            }
            for address in [
                0x43000, 0x43010, 0x43014, 0x43016, 0x43018, 0x4301c, 0x43020, 0x430ff, 0x43100,
                0x431ff, 0x43200, 0x43300, 0x43400, 0x43fff, 0x44000,
            ] {
                assert_eq!(
                    cpu.guest_memory_addresses(address, 1, crate::mmu::MemoryAccess::Read, true)
                        .unwrap(),
                    [address as usize]
                );
            }
            for address in [
                FORMER_FRAMEBUFFER_START - 1,
                FORMER_FRAMEBUFFER_START,
                FORMER_FRAMEBUFFER_START + 153600 - 1,
            ] {
                assert_eq!(
                    cpu.guest_memory_addresses(address, 1, crate::mmu::MemoryAccess::Read, true)
                        .unwrap(),
                    [address as usize]
                );
            }
        }
    }

    #[test]
    fn process_allocations_use_only_usable_ram_frames() {
        for memory_size in [1 << 16, 1 << 19, 1 << 20] {
            let mut cpu = Cpu::with_memory_size(memory_size).unwrap();
            cpu.ensure_page_memory_manager().unwrap();
            let manager = cpu.page_memory_manager.as_mut().unwrap();
            let free = manager.free_frames();
            if free == 0 {
                assert_eq!(
                    manager.create_address_space(&mut cpu.memory),
                    Err(crate::mmu::PageTableError::OutOfFrames)
                );
                continue;
            }
            for _ in 0..2 {
                let root = manager.create_address_space(&mut cpu.memory).unwrap();
                let mut virtual_address = 0x100000;
                let mut allocated_former_framebuffer = false;
                loop {
                    match manager.allocate_and_map_page(
                        &mut cpu.memory,
                        root,
                        virtual_address,
                        crate::mmu::PagePermissions::USER_DATA,
                    ) {
                        Ok(physical) => {
                            let physical = physical * crate::mmu::PAGE_SIZE as u32;
                            assert!(physical < crate::machine::usable_ram_size(memory_size) as u32);
                            allocated_former_framebuffer |= (FORMER_FRAMEBUFFER_START
                                ..FORMER_FRAMEBUFFER_START + 153600)
                                .contains(&physical);
                        }
                        Err(crate::mmu::PageTableError::OutOfFrames) => break,
                        Err(error) => panic!("unexpected allocation failure: {error:?}"),
                    }
                    virtual_address += crate::mmu::PAGE_SIZE as u32;
                }
                if memory_size >= FORMER_FRAMEBUFFER_START as usize + 153600 {
                    assert!(allocated_former_framebuffer);
                }
                manager
                    .destroy_address_space(&mut cpu.memory, root)
                    .unwrap();
                assert_eq!(manager.free_frames(), free);
            }
        }
    }

    fn invoke_syscall(cpu: &mut Cpu, number: SyscallNumber, arguments: [i64; 6]) {
        cpu.registers[crate::syscall::REGISTER_NUMBER] = number as i64;
        cpu.registers[crate::syscall::REGISTER_ARGUMENT_0..=crate::syscall::REGISTER_ARGUMENT_5]
            .copy_from_slice(&arguments);
        cpu.registers[crate::syscall::REGISTER_ABI_VERSION] = crate::syscall::ABI_VERSION;
        cpu.step().unwrap();
    }

    #[test]
    fn graphics_syscall_copies_validated_user_data_before_drawing() {
        use crate::machine::*;

        let (mut cpu, _) = cpu_with_syscall_task_memory(1 << 20);
        cpu.memory[0x400..0x40a].copy_from_slice(&[23, 0, 0, 4, 0, 7, 0, 1, 0, 1]);
        cpu.memory[0x500..0x502].copy_from_slice(&[31, 32]);
        cpu.graphics_device
            .write(GRAPHICS_OPERATION, GRAPHICS_OP_LINE);
        cpu.graphics_device.write(GRAPHICS_COLOR, 0x63);
        cpu.graphics_device.complete(0x1234);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Graphics,
            [i64::from(GRAPHICS_OP_PIXEL), 0x400, 0, 0, 0, 0],
        );

        assert_eq!(cpu.registers[0], 1);
        assert_eq!(cpu.composited_framebuffer()[7 * 320 + 4], 23);
        assert_eq!(
            cpu.graphics_device.read(GRAPHICS_OPERATION),
            Some(GRAPHICS_OP_LINE)
        );
        assert_eq!(cpu.graphics_device.read(GRAPHICS_COLOR), Some(0x63));
        assert_eq!(cpu.graphics_device.read(GRAPHICS_STATUS), Some(0));
        assert_eq!(cpu.graphics_device.read(GRAPHICS_RESULT), Some(0x12));
        assert_eq!(cpu.graphics_device.read(GRAPHICS_RESULT + 1), Some(0x34));

        cpu.memory[0x400..0x40a].copy_from_slice(&[0, 0, 0, 6, 0, 9, 0, 2, 0, 1]);
        cpu.graphics_device.fail(GRAPHICS_STATUS_INVALID_REQUEST);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Graphics,
            [i64::from(GRAPHICS_OP_BLIT), 0x400, 0x500, 2, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        let framebuffer = cpu.composited_framebuffer();
        assert_eq!(framebuffer[9 * 320 + 6], 31);
        assert_eq!(framebuffer[9 * 320 + 7], 32);
        assert_eq!(
            cpu.graphics_device.read(GRAPHICS_STATUS),
            Some(GRAPHICS_STATUS_INVALID_REQUEST)
        );

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Graphics,
            [
                i64::from(GRAPHICS_OP_BLIT),
                0x400,
                i64::from(GRAPHICS_BASE + 0x10),
                2,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::BAD_ADDRESS);
        let framebuffer_after_rejection = cpu.composited_framebuffer();
        assert_eq!(framebuffer_after_rejection, framebuffer);
    }

    #[test]
    fn input_snapshot_validates_output_before_consuming_events() {
        let (mut cpu, _) = cpu_with_syscall_task_memory(1 << 20);
        cpu.input.key = 0x41;
        cpu.input.keys = 0x0000_1234;
        cpu.input.mouse = [0, 42, 0, 43, 1, 0x08, 0xff, 0xfe];

        invoke_syscall(
            &mut cpu,
            SyscallNumber::InputSnapshot,
            [i64::from(crate::machine::INPUT_BASE), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::BAD_ADDRESS);
        assert_eq!(cpu.input.key, 0x41);
        assert_eq!(cpu.input.mouse[5], 0x08);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::InputSnapshot,
            [0x400, 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 13);
        assert_eq!(
            &cpu.memory[0x400..0x40d],
            &[0x41, 0, 0, 0x12, 0x34, 0, 42, 0, 43, 1, 0x08, 0xff, 0xfe]
        );
        assert_eq!(cpu.input.key, 0);
        assert_eq!(cpu.input.mouse[5], 0);
        assert_eq!(cpu.memory[0x43000], 0);
        assert_eq!(cpu.memory[0x4300d], 0);
    }

    #[test]
    fn process_info_validates_buffers_and_reports_live_processes() {
        let (mut cpu, parent) = cpu_with_syscall_task();
        let child = cpu
            .scheduler
            .create_task(TaskContext::user(0x100, 0x6000, 0xe000, false, 0))
            .unwrap();
        cpu.syscall_state.register_process(child, Some(parent));
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessInfo,
            [0, 0x400, 64, 0, 0, 0],
        );
        let length = cpu.registers[0] as usize;
        assert_eq!(&cpu.memory[0x400..0x400 + length], b"0 - running\n");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessInfo,
            [1, 0x400, 64, 0, 0, 0],
        );
        let length = cpu.registers[0] as usize;
        assert_eq!(&cpu.memory[0x400..0x400 + length], b"1 0 runnable\n");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessInfo,
            [2, 0x400, 64, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        invoke_syscall(&mut cpu, SyscallNumber::ProcessInfo, [0, 0x400, 1, 0, 0, 0]);
        assert_eq!(cpu.registers[0], crate::syscall::error::OVERFLOW);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessInfo,
            [0, 0xffff, 64, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::BAD_ADDRESS);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessInfo,
            [0, 0x400, 4097, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
    }

    #[test]
    fn process_status_distinguishes_ready_sleeping_waiting_and_exited_tasks() {
        for (state, reason, expected) in [
            (TaskState::Runnable, "", "ready"),
            (TaskState::Blocked, "ticks", "sleeping"),
            (TaskState::Blocked, "millis", "sleeping"),
            (TaskState::Blocked, "child", "waiting"),
            (TaskState::Blocked, "", "blocked"),
            (TaskState::Exited, "", "exited"),
        ] {
            let (mut cpu, parent) = cpu_with_syscall_task();
            let child = cpu
                .scheduler
                .create_task(TaskContext::user(0x100, 0x6000, 0xe000, false, 0))
                .unwrap();
            cpu.syscall_state.register_process(child, Some(parent));
            cpu.syscall_state.processes.get_mut(&child).unwrap().name = "demo.bin".to_owned();
            cpu.yield_current_task().unwrap();
            cpu.schedule_current_task(state, None).unwrap();
            match reason {
                "ticks" => {
                    cpu.syscall_state.sleeping.insert(child, u64::MAX);
                }
                "millis" => {
                    cpu.syscall_state
                        .sleeping_millis
                        .insert(child, (0, 1_000_000_000));
                }
                "child" => {
                    cpu.syscall_state.waiting_for.insert(child, parent);
                }
                _ => {}
            }
            cpu.registers[0] = SyscallNumber::ProcessStatus as i64;
            cpu.registers[1..4].copy_from_slice(&[1, 0x400, 64]);
            let length = cpu.syscall_process_info();
            assert!(length > 0);
            assert_eq!(
                &cpu.memory[0x400..0x400 + length as usize],
                format!("1 0 {expected} demo.bin\n").as_bytes(),
            );
            cpu.registers[1] = 0;
            let length = cpu.syscall_process_info();
            assert!(length > 0);
            assert_eq!(
                &cpu.memory[0x400..0x400 + length as usize],
                b"0 - running init\n"
            );
        }
    }

    #[test]
    fn millisecond_sleep_blocks_until_the_monotonic_deadline_including_rollover() {
        for started in [1000, ADDRESS_MASK - 499] {
            let (mut cpu, task) = cpu_with_syscall_task();
            cpu.test_monotonic_ms = Some(started);
            invoke_syscall(&mut cpu, SyscallNumber::SleepMillis, [1000, 0, 0, 0, 0, 0]);
            assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Blocked);
            assert_eq!(cpu.current_task(), None);
            assert!(cpu.has_pending_work());
            cpu.test_monotonic_ms = Some(started.wrapping_add(999) & ADDRESS_MASK);
            cpu.run(100).unwrap();
            assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Blocked);
            assert_eq!(cpu.current_task(), None);
            cpu.test_monotonic_ms = Some(started.wrapping_add(1000) & ADDRESS_MASK);
            cpu.run(1).unwrap();
            assert_eq!(cpu.current_task(), Some(task));
            assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Running);
            assert!(cpu.syscall_state.sleeping_millis.is_empty());
        }
    }

    #[test]
    fn millisecond_sleep_validates_duration_without_blocking() {
        for duration in [-1, 0, 1_000_000_001, i64::MAX] {
            let (mut cpu, task) = cpu_with_syscall_task();
            invoke_syscall(
                &mut cpu,
                SyscallNumber::SleepMillis,
                [duration, 0, 0, 0, 0, 0],
            );
            assert_eq!(cpu.current_task(), Some(task));
            assert!(cpu.syscall_state.sleeping_millis.is_empty());
            assert_eq!(
                cpu.registers[0],
                if duration == 0 {
                    0
                } else {
                    crate::syscall::error::INVALID
                }
            );
        }
    }

    #[test]
    fn process_status_validates_buffers_and_sanitizes_program_names() {
        let (mut cpu, parent) = cpu_with_syscall_task();
        cpu.syscall_state.register_process(parent, None);
        cpu.syscall_state.processes.get_mut(&parent).unwrap().name = "a\nb c".to_owned();
        invoke_syscall(
            &mut cpu,
            SyscallNumber::ProcessStatus,
            [0, 0x400, 64, 0, 0, 0],
        );
        assert_eq!(
            &cpu.memory[0x400..0x400 + cpu.registers[0] as usize],
            b"0 - running a?b?c\n"
        );
        for (index, destination, capacity, expected) in [
            (0, 0x400, 1, crate::syscall::error::OVERFLOW),
            (0, 0xffff, 64, crate::syscall::error::BAD_ADDRESS),
            (0, 0x400, 4097, crate::syscall::error::INVALID),
            (1, 0x400, 64, 0),
        ] {
            invoke_syscall(
                &mut cpu,
                SyscallNumber::ProcessStatus,
                [index, destination, capacity, 0, 0, 0],
            );
            assert_eq!(cpu.registers[0], expected);
        }
    }

    #[test]
    fn process_accounting_counts_instructions_and_validates_output_buffers() {
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.syscall_state.register_process(task, None);
        assert_eq!(cpu.syscall_state.process_instructions.get(&task), Some(&0));
        cpu.step().unwrap();
        assert_eq!(cpu.syscall_state.process_instructions.get(&task), Some(&1));

        cpu.registers[1..4].copy_from_slice(&[0, 0x400, 32]);
        cpu.registers[4..=6].fill(0);
        cpu.registers[0] = SyscallNumber::ProcessAccounting as i64;
        let length = cpu.syscall_process_accounting();
        assert_eq!(length, 4);
        assert_eq!(&cpu.memory[0x400..0x400 + length as usize], b"0 1\n");
        for (index, destination, capacity, extra, expected) in [
            (0, 0x400, 1, 0, crate::syscall::error::OVERFLOW),
            (0, 0xffff, 32, 0, crate::syscall::error::BAD_ADDRESS),
            (0, 0x400, 4097, 0, crate::syscall::error::INVALID),
            (0, 0x400, 32, 1, crate::syscall::error::INVALID),
            (1, 0x400, 32, 0, 0),
        ] {
            cpu.registers[1..4].copy_from_slice(&[index, destination, capacity]);
            cpu.registers[4] = extra;
            cpu.registers[5..=6].fill(0);
            assert_eq!(cpu.syscall_process_accounting(), expected);
        }
    }

    #[test]
    fn background_process_services_preserve_arguments_and_do_not_block_waiters() {
        let (mut cpu, parent) = cpu_with_syscall_task();
        let child = cpu
            .scheduler
            .create_task(TaskContext::user(0x100, 0x6000, 0xe000, false, 0))
            .unwrap();
        cpu.syscall_state.register_process(child, Some(parent));
        cpu.syscall_state
            .processes
            .get_mut(&parent)
            .unwrap()
            .startup_argument = 65535;
        invoke_syscall(&mut cpu, SyscallNumber::StartupArgument, [0; 6]);
        assert_eq!(cpu.registers[0], 65535);
        cpu.test_monotonic_ms = Some(123_456);
        invoke_syscall(&mut cpu, SyscallNumber::MonotonicMillis, [0; 6]);
        assert_eq!(cpu.registers[0], 123_456);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::TryWait,
            [i64::from(child), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::TRY_AGAIN);
        assert_eq!(cpu.current_task(), Some(parent));
        assert!(cpu.syscall_state.waiting_for.is_empty());
        assert!(cpu.syscall_state.processes[&child].waiters.is_empty());
        invoke_syscall(
            &mut cpu,
            SyscallNumber::TryWait,
            [i64::from(parent), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::NO_CHILD);
    }

    fn run_assembly(source: &str) -> Cpu {
        let image = crate::assembler::assemble(source, 0).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(2000).unwrap();
        assert!(!cpu.running, "program did not halt");
        cpu
    }

    fn folder_disk_for_test(name: &str, read_only: bool) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sc16-fs-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk = root.join("DISK0001");
        std::fs::create_dir_all(&disk).unwrap();
        std::fs::write(
            disk.join("disk.json"),
            format!(
                r#"{{"version":1,"id":"DISK0001","label":"Guest FS","capacity_bytes":65536,"read_only":{read_only}}}"#
            ),
        )
        .unwrap();
        std::fs::write(disk.join("main.bin"), [1, 2, 3]).unwrap();
        std::fs::write(disk.join("kernel.bin"), [4, 5, 6]).unwrap();
        disk
    }

    #[test]
    fn reset_starts_in_supervisor_mode_with_interrupts_disabled() {
        let cpu = Cpu::with_memory_size(1 << 19).unwrap();

        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.registers, [0; 8]);
        assert_eq!(cpu.sp, 0x7ffff);
        assert_eq!(cpu.kernel_sp, cpu.sp);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.flags, Flags::default());
        assert_eq!(read_interrupt_control(&cpu) & 1, 0);
    }

    #[test]
    fn user_privilege_fault_uses_kernel_stack_and_rti_restores_user_state() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        install_test_handler(&mut cpu, EXCEPTION_PRIVILEGE_VIOLATION, 0x300, 0);
        cpu.write_u16(0x2000, OP_DI << 11);
        cpu.write_u16(0x300, OP_RTI << 11);
        cpu.pc = 0x2000;
        cpu.sp = 0x7000;
        cpu.kernel_sp = 0xf000;
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.flags = Flags {
            zero: true,
            carry: false,
            negative: true,
            overflow: false,
        };
        write_interrupt_control(&mut cpu, 1);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.sp, 0xeff0);
        assert_eq!(cpu.read_u32(cpu.sp), 0x2000);
        assert_eq!(cpu.read_u16(cpu.sp + 4), 0b0101);
        assert_eq!(cpu.read_u16(cpu.sp + 6), 0b11);
        assert_eq!(cpu.read_u32(cpu.sp + 8), 0x7000);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PRIVILEGE_VIOLATION);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x2000);
        assert_eq!(cpu.sp, 0x7000);
        assert_eq!(cpu.kernel_sp, 0xf000);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.flags.zero, true);
        assert_eq!(cpu.flags.negative, true);
        assert_eq!(read_interrupt_control(&cpu) & 1, 1);
    }

    #[test]
    fn invalid_instruction_enters_the_configured_exception_handler() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        install_test_handler(&mut cpu, EXCEPTION_ILLEGAL_INSTRUCTION, 0x300, 0);
        cpu.write_u16(0, 0xff1a);
        cpu.write_u16(0x300, OP_RTI << 11);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.read_u32(cpu.sp), 0);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_ILLEGAL_INSTRUCTION);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
    }

    #[test]
    fn user_instruction_cannot_write_or_read_interrupt_control_registers() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        install_test_handler(&mut cpu, EXCEPTION_PRIVILEGE_VIOLATION, 0x300, 0);
        install_test_handler(&mut cpu, EXCEPTION_PAGE_FAULT, 0x300, 0);
        cpu.write_u16(0, (OP_ST << 11) | (1 << 5));
        cpu.write_u16(2, (OP_LD << 11) | (2 << 8) | (1 << 5));
        cpu.write_u16(0x300, OP_RTI << 11);
        cpu.registers[0] = 0x10;
        cpu.registers[1] = i64::from(crate::machine::INTERRUPT_ENABLE);
        cpu.sp = 0x7000;
        cpu.kernel_sp = 0xf000;
        cpu.privilege_mode = PrivilegeMode::User;
        write_interrupt_control(&mut cpu, 1);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.read(crate::machine::INTERRUPT_ENABLE), 0);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PAGE_FAULT);

        cpu.write_u32(cpu.sp, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        write_interrupt_control(&mut cpu, 1);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.registers[2], 0);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PAGE_FAULT);
        assert_eq!(cpu.read_u32(cpu.sp), 2);
    }

    #[test]
    fn software_call_enters_supervisor_handler_and_returns_to_user() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let image = crate::assembler::assemble("SVC\nHALT", 0x100).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        cpu.write_u16(0x300, OP_RTI << 11);
        let user_stack = cpu.initial_user_stack_pointer();
        cpu.enter_user_mode(0x100, user_stack).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.read_u32(cpu.sp), 0x102);
        assert_eq!(cpu.read_u16(cpu.sp + 6), 1);
        assert_eq!(cpu.read_u16(cpu.sp + 12), crate::isa::SVC_VECTOR);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x102);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.sp, user_stack);
        cpu.step().unwrap();
        assert!(!cpu.running);
    }

    #[test]
    fn user_software_call_requires_a_user_callable_idt_descriptor() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        let image = crate::assembler::assemble("SVC\nHALT", 0x100).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 0);
        install_test_handler(&mut cpu, EXCEPTION_PRIVILEGE_VIOLATION, 0x400, 0);
        cpu.write_u16(0x300, OP_RTI << 11);
        cpu.write_u16(0x400, OP_RTI << 11);
        let user_stack = cpu.initial_user_stack_pointer();
        cpu.enter_user_mode(0x100, user_stack).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x400);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PRIVILEGE_VIOLATION);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x102);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    }

    #[test]
    fn syscall_abi_returns_pid_and_rejects_unknown_numbers_and_versions() {
        let (mut cpu, task) = cpu_with_syscall_task();
        invoke_syscall(&mut cpu, SyscallNumber::GetPid, [0; 6]);
        assert_eq!(cpu.registers[0], i64::from(task));
        assert_eq!(cpu.pc, 0x102);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);

        cpu.registers[0] = 99;
        cpu.registers[7] = crate::syscall::ABI_VERSION;
        cpu.step().unwrap();
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        assert_eq!(cpu.pc, 0x104);

        cpu.registers[0] = SyscallNumber::GetPid as i64;
        cpu.registers[7] = crate::syscall::ABI_VERSION + 1;
        cpu.step().unwrap();
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        assert_eq!(cpu.pc, 0x106);

        invoke_syscall(&mut cpu, SyscallNumber::Exit, [256, 0, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        assert_eq!(cpu.current_task(), Some(task));
        assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Running);
    }

    #[test]
    fn account_syscalls_initialize_root_authenticate_and_assign_credentials() {
        let disk_root = folder_disk_for_test("syscall-accounts", false);
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        crate::accounts::AccountStore::first_boot()
            .save(&disk)
            .unwrap();
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.mount_folder_disk(&disk_root).unwrap();
        cpu.memory[0x1000..0x1005].copy_from_slice(b"root\0");
        cpu.memory[0x1100..0x1109].copy_from_slice(b"Secret12\0");

        invoke_syscall(
            &mut cpu,
            SyscallNumber::AccountStatus,
            [17, 18, 19, 20, 21, 22],
        );
        assert_eq!(cpu.registers[0], 0);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::InitializeRootPassword,
            [0x1100, 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .bootstrap_authority = true;
        invoke_syscall(
            &mut cpu,
            SyscallNumber::InitializeRootPassword,
            [0x1100, 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        assert!(!cpu.syscall_state.processes[&task].bootstrap_authority);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::AccountStatus,
            [17, 18, 19, 20, 21, 22],
        );
        assert_eq!(cpu.registers[0], 1);
        cpu.memory[0x1100..0x1107].copy_from_slice(b"Wrong1\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Authenticate,
            [0x1000, 0x1100, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);
        assert!(cpu.syscall_state.processes[&task].credentials.is_none());

        cpu.memory[0x1100..0x1109].copy_from_slice(b"Secret12\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Authenticate,
            [0x1000, 0x1100, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        assert_eq!(
            cpu.syscall_state.processes[&task].credentials,
            Some(crate::syscall::ProcessCredentials {
                uid: 0,
                primary_gid: 0,
                supplementary_gids: Vec::new(),
                home: "root".to_owned(),
            })
        );
    }

    #[test]
    fn sisuos_filesystem_syscalls_enforce_account_and_parent_directory_modes() {
        let container = std::env::temp_dir().join(format!(
            "sc16-account-permissions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk_root = container.join("sisuos");
        std::fs::create_dir_all(disk_root.join("private")).unwrap();
        std::fs::create_dir_all(disk_root.join("home/alice")).unwrap();
        std::fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"sisuos","label":"SisuOS","capacity_bytes":65536,"read_only":false}"#,
        )
        .unwrap();
        std::fs::write(disk_root.join("public.txt"), b"hello").unwrap();
        std::fs::write(disk_root.join("private/secret.txt"), b"hidden").unwrap();
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        let mut accounts = crate::accounts::AccountStore::first_boot();
        let group = accounts.add_group("users").unwrap();
        let uid = accounts.add_user("alice", "users", "home/alice").unwrap();
        accounts.set_password("alice", "Secret12").unwrap();
        accounts.set_file_mode("private", true, 0, 0o700).unwrap();
        accounts
            .set_file_owner("home/alice", true, 0, uid, group, &[0])
            .unwrap();
        accounts
            .set_file_mode("home/alice", true, 0, 0o700)
            .unwrap();
        accounts.save(&disk).unwrap();

        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.mount_folder_disk(&disk_root).unwrap();
        cpu.syscall_state.register_process(task, None);
        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .credentials = Some(crate::syscall::ProcessCredentials {
            uid,
            primary_gid: group,
            supplementary_gids: Vec::new(),
            home: "home/alice".to_owned(),
        });

        cpu.memory[0x1000..0x100b].copy_from_slice(b"public.txt\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1000,
                i64::from(crate::syscall::open_flags::READ),
                0,
                0,
                0,
                0,
            ],
        );
        let handle = cpu.registers[0] as u32;
        assert!(handle >= 3);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [i64::from(handle), 0x2000, 5, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 5);
        assert_eq!(&cpu.memory[0x2000..0x2005], b"hello");

        cpu.memory[0x1200..0x1208].copy_from_slice(b"private\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::DirectoryEntry,
            [0x1200, 0, 0x2000, 128, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        cpu.memory[0x1200..0x1202].copy_from_slice(b"/\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::DirectoryEntry,
            [0x1200, 0, 0x2000, 128, 0, 0],
        );
        assert_eq!(cpu.registers[0], 5);
        assert_eq!(&cpu.memory[0x2000..0x2006], b"home/\0");

        cpu.memory[0x1300..0x1313].copy_from_slice(b"home/alice/new.txt\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1300,
                i64::from(crate::syscall::open_flags::WRITE | crate::syscall::open_flags::CREATE),
                0,
                0,
                0,
                0,
            ],
        );
        assert!(cpu.registers[0] >= 3);
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        let accounts = crate::accounts::AccountStore::load(&disk).unwrap();
        assert_eq!(
            accounts.file_permission("home/alice/new.txt", false),
            (uid, group, 0o644)
        );

        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetFileMode,
            [0x1300, 0o600, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetFileMode,
            [0x1000, 0o600, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        cpu.memory[0x1100..0x1106].copy_from_slice(b"alice\0");
        cpu.memory[0x1200..0x1206].copy_from_slice(b"users\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetFileOwner,
            [0x1300, 0x1100, 0x1200, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        cpu.memory[0x1100..0x1105].copy_from_slice(b"root\0");
        cpu.memory[0x1200..0x1205].copy_from_slice(b"root\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetFileOwner,
            [0x1300, 0x1100, 0x1200, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        let accounts = crate::accounts::AccountStore::load(&disk).unwrap();
        assert_eq!(
            accounts.file_permission("home/alice/new.txt", false),
            (uid, group, 0o600)
        );

        cpu.memory[0x1300..0x1313].copy_from_slice(b"home/alice/new-dir\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::MakeDirectory,
            [0x1300, 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        let accounts = crate::accounts::AccountStore::load(&disk).unwrap();
        assert_eq!(
            accounts.file_permission("home/alice/new-dir", true),
            (uid, group, 0o755)
        );

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1000,
                i64::from(crate::syscall::open_flags::WRITE),
                0,
                0,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        cpu.memory[0x1100..0x1113].copy_from_slice(b"private/secret.txt\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1100,
                i64::from(crate::syscall::open_flags::READ),
                0,
                0,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .credentials = None;
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1000,
                i64::from(crate::syscall::open_flags::READ),
                0,
                0,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);
        drop(cpu);
        std::fs::remove_dir_all(container).unwrap();
    }

    #[test]
    fn account_administration_syscalls_are_authorized_persisted_and_create_home_permissions() {
        let container = std::env::temp_dir().join(format!(
            "sc16-account-admin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk_root = container.join("sisuos");
        std::fs::create_dir_all(&disk_root).unwrap();
        std::fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"sisuos","label":"SisuOS","capacity_bytes":65536,"read_only":false}"#,
        )
        .unwrap();
        let disk = crate::disk::FolderDisk::open(&disk_root).unwrap();
        let mut accounts = crate::accounts::AccountStore::first_boot();
        accounts.initialize_root_password("RootPass1").unwrap();
        accounts.save(&disk).unwrap();

        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.mount_folder_disk(&disk_root).unwrap();
        cpu.syscall_state.register_process(task, None);
        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .credentials = Some(crate::syscall::ProcessCredentials {
            uid: 0,
            primary_gid: 0,
            supplementary_gids: Vec::new(),
            home: "root".to_owned(),
        });
        cpu.memory[0x1000..0x1006].copy_from_slice(b"staff\0");
        cpu.memory[0x1100..0x1106].copy_from_slice(b"alice\0");
        cpu.memory[0x1200..0x1206].copy_from_slice(b"staff\0");
        cpu.memory[0x1300..0x130b].copy_from_slice(b"home/alice\0");
        cpu.memory[0x1400..0x140a].copy_from_slice(b"UserPass1\0");

        invoke_syscall(&mut cpu, SyscallNumber::AddGroup, [0x1000, 0, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 1);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::AddUser,
            [0x1100, 0x1200, 0x1300, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 1);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetPassword,
            [0x1100, 0x1400, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);

        let accounts = crate::accounts::AccountStore::load(&disk).unwrap();
        assert!(accounts.authenticate("alice", "UserPass1").is_some());
        assert_eq!(accounts.file_permission("home/alice", true), (1, 1, 0o700));
        assert!(disk.metadata("home/alice").unwrap().is_directory);

        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .credentials
            .as_mut()
            .unwrap()
            .uid = 1;
        cpu.memory[0x1500..0x1506].copy_from_slice(b"guest\0");
        invoke_syscall(&mut cpu, SyscallNumber::AddGroup, [0x1500, 0, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);
        cpu.memory[0x1400..0x140a].copy_from_slice(b"UserPass2\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::SetPassword,
            [0x1100, 0x1400, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        let accounts = crate::accounts::AccountStore::load(&disk).unwrap();
        assert!(accounts.authenticate("alice", "UserPass2").is_some());
        assert!(accounts.authenticate("alice", "UserPass1").is_none());
        drop(cpu);
        std::fs::remove_dir_all(container).unwrap();
    }

    #[test]
    fn syscall_yield_switches_to_another_user_task() {
        let (mut cpu, first) = cpu_with_syscall_task();
        cpu.privilege_mode = PrivilegeMode::Supervisor;
        let second = cpu.create_user_task(0x100, 0x6000, 0xe000, None).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);

        assert_eq!(cpu.current_task(), Some(second));
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.task_context(first).unwrap().state, TaskState::Runnable);
    }

    #[test]
    fn syscall_file_handles_read_write_partial_data_and_validate_guest_memory() {
        let root = folder_disk_for_test("syscalls", false);
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.mount_folder_disk(&root).unwrap();
        cpu.memory[0x1000..0x1009].copy_from_slice(b"note.txt\0");
        cpu.memory[0x2000..0x2003].copy_from_slice(b"abc");

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x1000,
                i64::from(crate::syscall::open_flags::CREATE | crate::syscall::open_flags::WRITE),
                0,
                0,
                0,
                0,
            ],
        );
        let write_handle = cpu.registers[0] as u32;
        assert!(write_handle >= 3);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Write,
            [i64::from(write_handle), 0x2000, 3, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 3);
        assert_eq!(std::fs::read(root.join("note.txt")).unwrap(), b"abc");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [i64::from(write_handle), 0x2100, 1, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Close,
            [i64::from(write_handle), 0, 0, 0, 0, 0],
        );
        cpu.memory[0x4000..0x4009].copy_from_slice(b"note.txt\0");
        cpu.memory[0x2003..0x2005].copy_from_slice(b"de");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x4000,
                i64::from(crate::syscall::open_flags::APPEND),
                0,
                0,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        let append_flags = crate::syscall::open_flags::WRITE | crate::syscall::open_flags::APPEND;
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [0x4000, i64::from(append_flags), 0, 0, 0, 0],
        );
        let first_append_handle = cpu.registers[0] as u32;
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [0x4000, i64::from(append_flags), 0, 0, 0, 0],
        );
        let second_append_handle = cpu.registers[0] as u32;
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Write,
            [i64::from(first_append_handle), 0x2003, 1, 0, 0, 0],
        );
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Write,
            [i64::from(second_append_handle), 0x2004, 1, 0, 0, 0],
        );
        assert_eq!(std::fs::read(root.join("note.txt")).unwrap(), b"abcde");

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x4000,
                i64::from(crate::syscall::open_flags::READ),
                0,
                0,
                0,
                0,
            ],
        );
        let read_handle = cpu.registers[0] as u32;
        assert!(
            read_handle >= 3 && cpu.syscall_state.file(task, read_handle).is_some(),
            "open returned {} for task {task}; handles: {:?}",
            cpu.registers[0],
            cpu.syscall_state.open_files
        );
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [i64::from(read_handle), 0x2100, 8, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 5);
        assert_eq!(&cpu.memory[0x2100..0x2105], b"abcde");

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [i64::from(read_handle), 0xffff, 8, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::BAD_ADDRESS);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [
                i64::from(read_handle),
                0x2100,
                (crate::syscall::MAX_TRANSFER_BYTES + 1) as i64,
                0,
                0,
                0,
            ],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn syscall_seek_handles_set_current_end_and_invalid_negative_positions() {
        let disk = folder_disk_for_test("seek", false);
        std::fs::write(disk.join("main.bin"), b"abc").unwrap();
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.mount_folder_disk(&disk).unwrap();
        cpu.memory[0x4000..0x4009].copy_from_slice(b"main.bin\0");
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Open,
            [
                0x4000,
                i64::from(crate::syscall::open_flags::READ),
                0,
                0,
                0,
                0,
            ],
        );
        let handle = cpu.registers[0] as u32;
        assert!(handle >= 3);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Seek,
            [i64::from(handle), 0, 2, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Seek,
            [i64::from(handle), 0, 0, 1, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Seek,
            [i64::from(handle), -1, -1, 2, 0, 0],
        );
        assert_eq!(cpu.registers[0], 2);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Seek,
            [i64::from(handle), -1, -4, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::INVALID);
        assert_eq!(cpu.syscall_state.file(task, handle).unwrap().offset, 2);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Read,
            [i64::from(handle), 0x5000, 1, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 1);
        assert_eq!(cpu.memory[0x5000], b'c');
        let _ = std::fs::remove_dir_all(disk.parent().unwrap());
    }

    #[test]
    fn scnet_syscall_validates_buffers_and_reports_discovery_and_empty_receive() {
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.enable_scnet(0);

        cpu.registers[1] = 0;
        cpu.registers[2] = 0x5000;
        cpu.registers[3] = 1;
        assert_eq!(cpu.syscall_scnet(Some(task)), 0);

        cpu.registers[1] = 0;
        cpu.registers[2] = i64::from(crate::machine::CONSOLE_DATA);
        cpu.registers[3] = 1;
        assert_eq!(
            cpu.syscall_scnet(Some(task)),
            crate::syscall::error::BAD_ADDRESS
        );

        cpu.registers[1] = 2;
        cpu.registers[2] = 0x5100;
        cpu.registers[3] = 0x5200;
        cpu.registers[4] = 32;
        assert_eq!(
            cpu.syscall_scnet(Some(task)),
            crate::syscall::error::TRY_AGAIN
        );
    }

    #[test]
    fn bios_refuses_to_boot_when_scnet_name_is_in_use() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.bios_settings.network_name = "duplicate".to_owned();
        cpu.enable_scnet(0);
        cpu.ensure_scnet_node(std::time::Duration::ZERO).unwrap();
        cpu.scnet
            .as_mut()
            .unwrap()
            .inject_hello_for_test("duplicate");

        let error = cpu
            .ensure_scnet_node(std::time::Duration::ZERO)
            .unwrap_err();
        assert!(error.contains("already in use"));
    }

    #[test]
    fn compiled_c_scnet_discovery_wrapper_executes_through_syscall_abi() {
        let source = "#include <sc16_net.h>\n\
                      int main(void) {\n\
                          char names[SC16_NET_NAME_CAPACITY];\n\
                          return sc16_net_discover(names, 1);\n\
                      }";
        let assembly =
            crate::c_compiler::compile_minimal_program_to_assembly(source, "scnet-runtime.c")
                .unwrap()
                .expect("SC-NET C wrapper should compile");
        let image = crate::assembler::assemble(&assembly, 0).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.enable_scnet(0);
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0, 0x90000, 0xf0000, None).unwrap();
        cpu.start_task(task).unwrap();

        cpu.run(100_000).unwrap();

        assert!(!cpu.running, "SC-NET test program did not terminate");
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn syscall_sleep_uses_deterministic_idle_ticks() {
        let (mut cpu, task) = cpu_with_syscall_task();
        cpu.write_u16(0x102, 0);
        invoke_syscall(&mut cpu, SyscallNumber::Sleep, [3, 0, 0, 0, 0, 0]);
        assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Blocked);

        let executed = cpu.run(8).unwrap();

        assert_eq!(executed, 3);
        assert_eq!(cpu.current_task(), Some(task));
        assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn syscall_wait_blocks_for_child_exit_and_collects_status() {
        let (mut cpu, parent) = cpu_with_syscall_task();
        let mut child_context = TaskContext::user(0x100, 0x6000, 0xe000, false, 0);
        child_context.registers[0] = SyscallNumber::Exit as i64;
        child_context.registers[1] = 42;
        child_context.registers[7] = crate::syscall::ABI_VERSION;
        let child = cpu.scheduler.create_task(child_context).unwrap();
        cpu.syscall_state.register_process(child, Some(parent));

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Wait,
            [i64::from(child), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.current_task(), Some(child));

        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), Some(parent));
        assert_eq!(cpu.registers[0], 42);
        assert_eq!(cpu.registers[1], i64::from(child));
        assert!(cpu.syscall_state.processes.get(&child).is_none());
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Wait,
            [i64::from(child), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::NO_CHILD);
    }

    #[test]
    fn syscall_kill_terminates_only_children_and_preserves_wait_status() {
        let (mut cpu, parent) = cpu_with_syscall_task();
        let child = cpu
            .scheduler
            .create_task(TaskContext::user(0x100, 0x6000, 0xe000, false, 0))
            .unwrap();
        let unrelated_parent = cpu
            .scheduler
            .create_task(TaskContext::user(0x120, 0x6000, 0xe000, false, 0))
            .unwrap();
        let unrelated_child = cpu
            .scheduler
            .create_task(TaskContext::user(0x140, 0x6000, 0xe000, false, 0))
            .unwrap();
        cpu.syscall_state.register_process(parent, None);
        cpu.syscall_state.register_process(child, Some(parent));
        cpu.syscall_state.register_process(unrelated_parent, None);
        cpu.syscall_state
            .register_process(unrelated_child, Some(unrelated_parent));

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Kill,
            [i64::from(unrelated_child), 137, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::PERMISSION);
        cpu.syscall_state
            .processes
            .get_mut(&child)
            .unwrap()
            .children
            .push(99);
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Kill,
            [i64::from(child), 137, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], crate::syscall::error::BUSY);
        cpu.syscall_state
            .processes
            .get_mut(&child)
            .unwrap()
            .children
            .clear();
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Kill,
            [i64::from(child), 137, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 0);
        assert_eq!(cpu.scheduler.task(child).unwrap().state, TaskState::Exited);

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Wait,
            [i64::from(child), 0, 0, 0, 0, 0],
        );
        assert_eq!(cpu.registers[0], 137);
        assert_eq!(cpu.registers[1], i64::from(child));
        assert!(!cpu.syscall_state.processes.contains_key(&child));
    }

    #[test]
    fn compiled_c_syscall_wrappers_execute_through_svc() {
        let source = "#include <sc16_syscall.h>\n\
                      int main(void) {\n\
                          long semaphore;\n\
                          if (sc16_write(1, \"ok\", 2) != 2) return 7;\n\
                          if (sc16_sleep(1UL) != 0) return 9;\n\
                          if (sc16_getpid() < 0L) return 8;\n\
                          if (sc16_mmap(4096UL, SC16_PROT_READ, 0) != -13L) return 14;\n\
                          if (sc16_spawn(\"missing\") != -13L) return 12;\n\
                          if (sc16_spawn_exec(\"missing\", \"app\", 4, 1) != -13L) return 15;\n\
                          if (sc16_argument_count() != 3) return 16;\n\
                          { char buffer[32];\n\
                            if (sc16_argument(1, buffer, sizeof(buffer)) != 10) return 17;\n\
                            if (buffer[0] != 'a' || buffer[1] != ' ' || buffer[10] != 0) return 18;\n\
                            if (sc16_argument(2, buffer, sizeof(buffer)) != 0 || buffer[0] != 0) return 19;\n\
                            if (sc16_argument(3, buffer, sizeof(buffer)) != -22) return 20;\n\
                            if (sc16_argument(1, buffer, 10) != -75) return 21;\n\
                          }\n\
                          semaphore = sc16_semop(SC16_SEM_CREATE, 0UL, 1);\n\
                          if (semaphore < 0L) return 10;\n\
                          if (sc16_semop(SC16_SEM_DESTROY, (unsigned long)semaphore, 0) != 0L) return 11;\n\
                          if (sc16_semop(SC16_SEM_DESTROY, 0UL, 0) != -22L) return 13;\n\
                          if (sc16_kill(0UL, 137) != -13) return 22;\n\
                          if (sc16_kill(1UL, 137) != -2) return 23;\n\
                          return 0;\n\
                      }";
        let assembly =
            crate::c_compiler::compile_minimal_program_to_assembly(source, "syscall-runtime.c")
                .unwrap()
                .expect("C runtime bridge should compile");
        let image = crate::assembler::assemble(&assembly, 0).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0, 0x90000, 0xf0000, None).unwrap();
        cpu.syscall_state.register_process(task, None);
        cpu.syscall_state
            .processes
            .get_mut(&task)
            .unwrap()
            .arguments = vec!["app".to_owned(), "a file.txt".to_owned(), String::new()];
        cpu.start_task(task).unwrap();

        let mut saw_exec = false;
        for _ in 0..10_000 {
            if !cpu.running {
                break;
            }
            if cpu.current_task() == Some(task)
                && cpu.read_u16(cpu.pc) == 0x0080
                && cpu.registers[0] == SyscallNumber::SpawnExec as i64
            {
                assert!(cpu.registers[1] > 0xffff);
                assert!(cpu.registers[2] > 0xffff);
                assert_eq!(
                    cpu.read_guest_path(cpu.registers[1] as u32, true).unwrap(),
                    Some("missing".to_owned())
                );
                let addresses = cpu
                    .guest_memory_addresses(
                        cpu.registers[2] as u32,
                        4,
                        crate::mmu::MemoryAccess::Read,
                        true,
                    )
                    .unwrap();
                assert_eq!(
                    addresses
                        .into_iter()
                        .map(|address| cpu.memory[address])
                        .collect::<Vec<_>>(),
                    b"app\0"
                );
                assert_eq!(cpu.registers[3], 4);
                assert_eq!(cpu.registers[4], 1);
                saw_exec = true;
            }
            cpu.run(1).unwrap();
        }

        assert!(saw_exec, "compiled program did not invoke spawn_exec");
        assert!(!cpu.running, "compiled syscall program did not terminate");
        assert_eq!(cpu.registers[0], 0);
        assert!(
            cpu.console
                .output()
                .windows(b"ok".len())
                .any(|output| output == b"ok")
        );
    }

    #[test]
    fn compiled_c_stdio_file_streams_execute_through_syscalls() {
        let source = "#include <stdio.h>\n\
                      #include <errno.h>\n\
                      int main(void) {\n\
                          FILE *stream;\n\
                          char output[4];\n\
                          char input[4200];\n\
                          char large_output[4202];\n\
                          int index;\n\
                          stream = fopen(\"stream.txt\", \"w+\");\n\
                          if (stream == 0) return 1;\n\
                          if (fwrite(\"abc\", 1, 3, stream) != 3) return 2;\n\
                          if (ftell(stream) != 3L) return 3;\n\
                          if (fseek(stream, 0L, SEEK_SET) != 0) return 4;\n\
                          if (fread(output, 1, 3, stream) != 3) return 5;\n\
                          if (output[0] != 'a' || output[1] != 'b' || output[2] != 'c') return 6;\n\
                          if (fgetc(stream) != EOF || !feof(stream)) return 7;\n\
                          if (fseek(stream, -1L, SEEK_END) != 0) return 8;\n\
                          if (fgetc(stream) != 'c') return 9;\n\
                          if (fclose(stream) != 0) return 10;\n\
                          if (fopen(\"missing.txt\", \"r\") != 0 || errno != ENOENT) return 11;\n\
                          stream = fopen(\"stream.txt\", \"rb\");\n\
                          if (stream == 0 || fgetc(stream) != 'a') return 24;\n\
                          if (fclose(stream) != 0) return 25;\n\
                          if (fopen(\"invalid.txt\", \"r++\") != 0 || errno != EINVAL) return 26;\n\
                          if (remove(\"stream.txt\") != -1 || errno != ENOSYS) return 12;\n\
                          for (index = 0; index < 4200; ++index)\n\
                              input[index] = (char)index;\n\
                          stream = fopen(\"large.bin\", \"w+\");\n\
                          if (stream == 0) return 13;\n\
                          if (fwrite(input, 1, 4200, stream) != 4200) return 14;\n\
                          if (fseek(stream, 0L, SEEK_SET) != 0) return 15;\n\
                          if (fread(large_output, 2, 2101, stream) != 2100) return 16;\n\
                          if (!feof(stream)) return 17;\n\
                          for (index = 0; index < 4200; ++index)\n\
                              if (large_output[index] != input[index]) return 18;\n\
                          stream = freopen(\"reopened.txt\", \"w\", stream);\n\
                          if (stream == 0) return 19;\n\
                          if (fputc('r', stream) != 'r') return 20;\n\
                          if (fclose(stream) != 0) return 21;\n\
                          if (fclose(stream) != EOF || errno != EBADF) return 22;\n\
                          stream = fopen(\"append.txt\", \"a+b\");\n\
                          if (stream == 0 || fwrite(\"xy\", 1, 2, stream) != 2) return 27;\n\
                          if (fclose(stream) != 0) return 28;\n\
                          stream = fopen(\"append.txt\", \"ab\");\n\
                          if (stream == 0 || fputc('z', stream) != 'z') return 29;\n\
                          if (fclose(stream) != 0) return 30;\n\
                          return 0;\n\
                      }";
        let assembly =
            crate::c_compiler::compile_minimal_program_to_assembly(source, "stdio-syscall.c")
                .unwrap()
                .expect("stdio file runtime should compile");
        let image = crate::assembler::assemble(&assembly, 0).unwrap();
        let disk = folder_disk_for_test("stdio-streams", false);
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.mount_folder_disk(&disk).unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0, 0x90000, 0xf0000, None).unwrap();
        cpu.start_task(task).unwrap();

        cpu.run(1_000_000).unwrap();

        assert!(!cpu.running, "stdio program did not terminate");
        let large_contents = std::fs::read(disk.join("large.bin")).unwrap();
        assert_eq!(large_contents.len(), 4200);
        assert_eq!(
            &large_contents[4096..],
            &(0..104).map(|value| value as u8).collect::<Vec<_>>()
        );
        assert_eq!(cpu.registers[0], 0, "stdio program returned an error code");
        assert_eq!(std::fs::read(disk.join("stream.txt")).unwrap(), b"abc");
        assert_eq!(std::fs::read(disk.join("reopened.txt")).unwrap(), b"r");
        assert_eq!(std::fs::read(disk.join("append.txt")).unwrap(), b"xyz");
        let _ = std::fs::remove_dir_all(disk.parent().unwrap());
    }

    #[test]
    fn compiled_c_struct_pointer_members_execute_correctly() {
        let source = "typedef struct { int value; } Item;\n\
                      int main(void) {\n\
                          Item item;\n\
                          Item *pointer;\n\
                          pointer = &item;\n\
                          pointer->value = 42;\n\
                          return pointer->value == 42 ? 0 : 1;\n\
                      }";
        let assembly =
            crate::c_compiler::compile_minimal_program_to_assembly(source, "struct-pointer.c")
                .unwrap()
                .expect("struct-pointer program should compile");
        let image = crate::assembler::assemble(&assembly, 0).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0, 0x70000, 0xf0000, None).unwrap();
        cpu.start_task(task).unwrap();

        cpu.run(20_000).unwrap();

        assert!(!cpu.running, "struct-pointer program did not terminate");
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn syscall_semaphore_wait_blocks_and_post_wakes_one_waiter() {
        let (mut cpu, owner) = cpu_with_syscall_task();
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Semaphore,
            [
                i64::from(crate::syscall::semaphore_operation::CREATE),
                0,
                0,
                0,
                0,
                0,
            ],
        );
        let semaphore_id = cpu.registers[0] as u32;
        assert!(semaphore_id > 0);

        let mut waiter_context = TaskContext::user(0x100, 0x6000, 0xe000, false, 0);
        waiter_context.registers[0] = SyscallNumber::Semaphore as i64;
        waiter_context.registers[1] = i64::from(crate::syscall::semaphore_operation::WAIT);
        waiter_context.registers[2] = i64::from(semaphore_id);
        waiter_context.registers[7] = crate::syscall::ABI_VERSION;
        let waiter = cpu.scheduler.create_task(waiter_context).unwrap();
        cpu.syscall_state.register_process(waiter, None);

        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);
        assert_eq!(cpu.current_task(), Some(waiter));
        cpu.step().unwrap();
        assert_eq!(cpu.current_task(), Some(owner));

        invoke_syscall(
            &mut cpu,
            SyscallNumber::Semaphore,
            [
                i64::from(crate::syscall::semaphore_operation::POST),
                i64::from(semaphore_id),
                0,
                0,
                0,
                0,
            ],
        );
        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);

        assert_eq!(cpu.current_task(), Some(waiter));
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn syscall_semaphore_wait_is_interrupted_when_owner_exits() {
        let (mut cpu, owner) = cpu_with_syscall_task();
        invoke_syscall(
            &mut cpu,
            SyscallNumber::Semaphore,
            [
                i64::from(crate::syscall::semaphore_operation::CREATE),
                0,
                0,
                0,
                0,
                0,
            ],
        );
        let semaphore_id = cpu.registers[0] as u32;
        let mut waiter_context = TaskContext::user(0x100, 0x6000, 0xe000, false, 0);
        waiter_context.registers[0] = SyscallNumber::Semaphore as i64;
        waiter_context.registers[1] = i64::from(crate::syscall::semaphore_operation::WAIT);
        waiter_context.registers[2] = i64::from(semaphore_id);
        waiter_context.registers[7] = crate::syscall::ABI_VERSION;
        let waiter = cpu.scheduler.create_task(waiter_context).unwrap();
        cpu.syscall_state.register_process(waiter, None);

        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);
        assert_eq!(cpu.current_task(), Some(waiter));
        cpu.step().unwrap();
        assert_eq!(cpu.current_task(), Some(owner));

        invoke_syscall(&mut cpu, SyscallNumber::Exit, [0; 6]);

        assert_eq!(cpu.current_task(), Some(waiter));
        assert_eq!(cpu.registers[0], crate::syscall::error::INTERRUPTED);
        assert!(cpu.syscall_state.semaphores.is_empty());
    }

    #[test]
    fn syscall_spawn_creates_isolated_task_and_mmap_allocates_user_pages() {
        check_spawn_and_mmap(false);
    }

    #[test]
    fn syscall_spawn_exec_copies_arguments_into_isolated_child() {
        check_spawn_and_mmap(true);
    }

    fn check_spawn_and_mmap(with_arguments: bool) {
        let disk_root = folder_disk_for_test("syscall-spawn", false);
        let child_assembly = if with_arguments {
            let source = disk_root.join("child.c");
            std::fs::write(
                &source,
                "#include <sc16_syscall.h>\n\
                int main(void) {\n\
                    char buffer[32];\n\
                    if (sc16_argument_count() != 3) return 1;\n\
                    if (sc16_argument(0, buffer, sizeof(buffer)) != 5) return 2;\n\
                    if (buffer[0] != 'c' || buffer[5] != 0) return 3;\n\
                    if (sc16_argument(1, (char *)0x3f000000UL, 32) != -14) return 4;\n\
                    buffer[0] = 'z';\n\
                    if (sc16_argument(1, buffer, 10) != -75 || buffer[0] != 'z') return 5;\n\
                    if (sc16_argument(1, buffer, 11) != 10) return 6;\n\
                    if (sc16_write(1, buffer, 10) != 10 || buffer[10] != 0) return 7;\n\
                    if (sc16_argument(2, buffer, 1) != 0 || buffer[0] != 0) return 8;\n\
                    if (sc16_argument(3, buffer, 32) != -22) return 9;\n\
                    if (sc16_mmap(4096UL, 3, 0) != 0x20000000L) return 10;\n\
                    return 0;\n\
                }",
            )
            .unwrap();
            let image = crate::image::build_c_program_at(&source, 0x10000).unwrap();
            std::fs::remove_file(source).unwrap();
            image
        } else {
            crate::assembler::assemble(
                "LDI R0, 4\nLDI R1, 4096\nLDI R2, 3\nLDI R3, 0\nLDI R7, 1\nSVC\nHALT",
                0x1000,
            )
            .unwrap()
        };
        let child_image = crate::image::ProgramImage {
            entry_point: child_assembly.entry_point,
            segments: child_assembly
                .segments
                .iter()
                .map(|segment| {
                    crate::image::ImageSegment::new(segment.address, segment.data.clone())
                })
                .collect(),
        };
        crate::image::write_image(&disk_root.join("child.img"), &child_image).unwrap();

        let parent_assembly =
            crate::assembler::assemble(&("SVC\n".repeat(16) + "HALT"), 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&parent_assembly.segments, parent_assembly.entry_point)
            .unwrap();
        let kernel_assembly = crate::assembler::assemble("HALT", 0x3000).unwrap();
        cpu.load_program(&kernel_assembly.segments, kernel_assembly.entry_point)
            .unwrap();
        cpu.loaded_image_segments.clear();
        let mut parent_mmu = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let parent_root = parent_mmu.create_address_space(&mut cpu.memory).unwrap();
        parent_mmu
            .map_borrowed_page(
                &mut cpu.memory,
                parent_root,
                0,
                0x40,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        parent_mmu
            .map_borrowed_page(
                &mut cpu.memory,
                parent_root,
                0x2000,
                0x41,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        parent_mmu
            .map_borrowed_page(
                &mut cpu.memory,
                parent_root,
                0x3000,
                3,
                crate::mmu::PagePermissions::KERNEL_CODE,
            )
            .unwrap();
        parent_mmu
            .map_borrowed_page(
                &mut cpu.memory,
                parent_root,
                0x7000,
                0x42,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        parent_mmu
            .map_borrowed_page(
                &mut cpu.memory,
                parent_root,
                0xff000,
                0xff,
                crate::mmu::PagePermissions::KERNEL_DATA,
            )
            .unwrap();
        let parent_data = &parent_assembly.segments[0].data;
        cpu.memory[0x40_000 + 0x100..0x40_000 + 0x100 + parent_data.len()]
            .copy_from_slice(parent_data);
        cpu.memory[0x41_000..0x41_00a].copy_from_slice(b"child.img\0");
        cpu.load_idt(0x800).unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x3000, 3);
        cpu.mount_folder_disk(&disk_root).unwrap();
        cpu.mmu_enabled = true;
        cpu.page_directory = parent_root;
        let parent = cpu
            .create_user_task(0x100, 0x8000, 0x000f_ffff, Some(parent_root))
            .unwrap();
        cpu.start_task(parent).unwrap();
        cpu.syscall_state.register_process(parent, None);
        let parent_credentials = crate::syscall::ProcessCredentials {
            uid: 42,
            primary_gid: 7,
            supplementary_gids: vec![8],
            home: "users/test".to_owned(),
        };
        cpu.syscall_state
            .processes
            .get_mut(&parent)
            .unwrap()
            .credentials = Some(parent_credentials.clone());
        let arguments = b"child\0a file.txt\0\0";
        if with_arguments {
            cpu.memory[0x41_100..0x41_100 + arguments.len()].copy_from_slice(arguments);
            for (address, length, count, expected) in [
                (
                    0x2100,
                    arguments.len() as i64,
                    17,
                    crate::syscall::error::INVALID,
                ),
                (0x2100, 1025, 3, crate::syscall::error::INVALID),
                (
                    0x9000,
                    arguments.len() as i64,
                    3,
                    crate::syscall::error::BAD_ADDRESS,
                ),
                (
                    0x2100,
                    arguments.len() as i64 - 1,
                    3,
                    crate::syscall::error::INVALID,
                ),
            ] {
                invoke_syscall(
                    &mut cpu,
                    SyscallNumber::SpawnExec,
                    [0x2000, address, length, count, 0, 0],
                );
                assert_eq!(cpu.registers[0], expected);
                assert_eq!(cpu.scheduler.tasks().len(), 1);
                assert!(cpu.syscall_state.processes[&parent].children.is_empty());
            }
        }
        cpu.registers[0] = if with_arguments {
            SyscallNumber::SpawnExec
        } else {
            SyscallNumber::Spawn
        } as i64;
        cpu.registers[1] = 0x2000;
        cpu.registers[2] = 0x2100;
        cpu.registers[3] = arguments.len() as i64;
        cpu.registers[4] = 3;
        cpu.registers[7] = crate::syscall::ABI_VERSION;

        cpu.step().unwrap();
        let child = cpu.registers[0] as u32;
        assert_ne!(child, parent);
        assert_eq!(
            cpu.syscall_state.processes[&child].credentials,
            Some(parent_credentials)
        );
        assert_eq!(
            cpu.syscall_state.processes[&child].arguments,
            if with_arguments {
                vec!["child".to_owned(), "a file.txt".to_owned(), String::new()]
            } else {
                vec!["child.img".to_owned()]
            },
        );
        let child_context = cpu.task_context(child).unwrap();
        assert!(child_context.mmu_enabled);
        assert_ne!(child_context.page_directory, parent_root);
        assert_eq!(
            crate::mmu::translate(
                &cpu.memory,
                child_context.page_directory,
                0x3000,
                crate::mmu::MemoryAccess::Fetch,
                true
            )
            .unwrap_err()
            .reason,
            crate::mmu::PageFaultReason::SupervisorOnly
        );

        invoke_syscall(&mut cpu, SyscallNumber::Write, [1, 0x3000, 1, 0, 0, 0]);
        assert_eq!(cpu.registers[0], crate::syscall::error::BAD_ADDRESS);
        let free_frames = cpu.page_memory_manager.as_ref().unwrap().free_frames();
        for (length, address) in [
            (4096, crate::machine::DEVICE_APERTURE_START),
            (8192, crate::machine::DEVICE_APERTURE_START - 4096),
        ] {
            invoke_syscall(
                &mut cpu,
                SyscallNumber::Mmap,
                [length, 3, i64::from(address), 0, 0, 0],
            );
            assert_eq!(cpu.registers[0], crate::syscall::error::OVERFLOW);
            assert_eq!(
                cpu.page_memory_manager.as_ref().unwrap().free_frames(),
                free_frames
            );
        }
        invoke_syscall(&mut cpu, SyscallNumber::Mmap, [4096, 3, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 0x2000_0000);

        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);
        assert_eq!(cpu.current_task(), Some(child));
        if with_arguments {
            cpu.memory[0x41_100..0x41_100 + arguments.len()].fill(b'x');
        }
        cpu.run(10_000).unwrap();
        assert!(!cpu.running);
        assert_eq!(
            cpu.registers[0],
            if with_arguments { 0 } else { 0x2000_0000 }
        );
        if with_arguments {
            assert!(cpu.console.output().ends_with(b"a file.txt"));
        }
        assert!(
            crate::mmu::translate(
                &cpu.memory,
                cpu.page_directory,
                0x2000_0000,
                crate::mmu::MemoryAccess::Write,
                true
            )
            .is_ok()
        );
        let _ = std::fs::remove_dir_all(disk_root.parent().unwrap());
    }

    #[test]
    fn idt_installation_is_privileged_and_missing_vectors_stop_with_an_error() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        assert!(matches!(
            cpu.set_idt_descriptor(
                1,
                crate::interrupts::IdtDescriptor {
                    handler: 0x300,
                    present: true,
                    privilege: 0,
                    gate_type: crate::interrupts::GateType::Interrupt,
                }
            ),
            Err(CpuError::PrivilegeViolation { .. })
        ));
        cpu.privilege_mode = PrivilegeMode::Supervisor;
        cpu.write_u16(0, 0xff1a);

        assert_eq!(
            cpu.step(),
            Err(CpuError::UnhandledException {
                vector: EXCEPTION_ILLEGAL_INSTRUCTION,
                cause: EXCEPTION_ILLEGAL_INSTRUCTION,
            })
        );
        assert!(!cpu.running);
    }

    #[test]
    fn cpu_logs_program_load_halt_and_privilege_events() {
        let halt_image = crate::assembler::assemble("HALT", 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&halt_image.segments, halt_image.entry_point)
            .unwrap();
        cpu.run(10).unwrap();
        let log = cpu.debug_console.text_snapshot().join("\n");
        assert!(
            log.contains("[DEBUG] cpu: Program image loaded at 0x100"),
            "{log}"
        );
        assert!(log.contains("[INFO] cpu: CPU halted at 0x100"), "{log}");

        let privileged_image = crate::assembler::assemble("LIDT R0", 0x200).unwrap();
        cpu.prepare_image_entry();
        cpu.load_program(&privileged_image.segments, privileged_image.entry_point)
            .unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        assert!(cpu.step().is_err());
        let log = cpu.debug_console.text_snapshot().join("\n");
        assert!(
            log.contains("[ERROR] cpu: Privilege violation at 0x200: load IDT"),
            "{log}"
        );
    }

    #[test]
    fn guest_supervisor_instructions_control_idt_and_mmu_state() {
        let image = crate::assembler::assemble(
            "LDI R0, 0\nLDI R1, 0x800\nLIDT R0\n\
             LDI R2, 0\nLDI R3, 0x2000\nMMUON R2\n\
             LDI R4, 0\nLDI R5, 0x2800\nMMUSWITCH R4\nMMUOFF\nHALT",
            0x4000,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        for (directory, table) in [(0x2000usize, 0x3000usize), (0x2800, 0x5000)] {
            cpu.memory[directory..directory + 4]
                .copy_from_slice(&((table as u32) | 0x0f).to_be_bytes());
            cpu.memory[table + 4 * 4..table + 4 * 4 + 4]
                .copy_from_slice(&(0x4000u32 | 0x0f).to_be_bytes());
        }
        cpu.run(32).unwrap();

        assert_eq!(cpu.idt_base, Some(0x800));
        assert_eq!(cpu.page_directory, 0x2800);
        assert!(!cpu.mmu_enabled);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
    }

    #[test]
    fn guest_supervisor_control_instructions_reject_user_mode() {
        let image = crate::assembler::assemble("MMUON R0\nHALT", 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            EXCEPTION_PRIVILEGE_VIOLATION,
            crate::interrupts::IdtDescriptor {
                handler: 0x300,
                present: true,
                privilege: 0,
                gate_type: crate::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
        cpu.write_u16(0x300, crate::isa::OP_HALT << 11);
        cpu.registers[0] = 0;
        cpu.registers[1] = 0x2000;
        cpu.enter_user_mode(0x100, 0x7000).unwrap();
        cpu.run(4).unwrap();

        assert!(!cpu.mmu_enabled);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x302);
    }

    #[test]
    fn guest_supervisor_control_instructions_reject_odd_register_pairs() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0x100, 0xf91f);
        cpu.write_u16(0x102, 0xf00e);
        cpu.pc = 0x100;

        assert_eq!(
            cpu.step(),
            Err(CpuError::UnhandledException {
                vector: EXCEPTION_ILLEGAL_INSTRUCTION,
                cause: EXCEPTION_ILLEGAL_INSTRUCTION,
            })
        );
    }

    #[test]
    fn guest_enter_user_instruction_validates_and_switches_context() {
        let image = crate::assembler::assemble(
            "LDI R0, 0\nLDI R1, 0x200\nLDI R2, 0\nLDI R3, 0x7000\nENTERU R0, R2\n\
             .address 0x200\nHALT",
            0x100,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();

        cpu.run(16).unwrap();

        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.pc, 0x202);
        assert_eq!(cpu.sp, 0x7000);
        assert_eq!(cpu.kernel_sp, 0xffff);
    }

    #[test]
    fn guest_start_user_instruction_activates_the_initial_scheduled_task() {
        let image = crate::assembler::assemble(
            "LDI R0, 0\nLDI R1, 0x200\nLDI R2, 0\nLDI R3, 0x7000\nSTARTU R0, R2\n\
             .address 0x200\nHALT",
            0x100,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();

        cpu.run(16).unwrap();

        assert_eq!(cpu.current_task(), Some(0));
        assert_eq!(cpu.task_context(0).unwrap().state, TaskState::Running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.pc, 0x202);
        assert_eq!(cpu.sp, 0x7000);
        assert_eq!(cpu.kernel_sp, 0xffff);
    }

    #[test]
    fn mmu_data_page_fault_records_address_and_enters_supervisor_handler() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        install_test_handler(&mut cpu, EXCEPTION_PAGE_FAULT, 0x300, 0);
        let directory_flags = crate::mmu::PagePermissions {
            writable: true,
            user: true,
            executable: true,
        };
        let directory_entry = crate::mmu::encode_entry(4, directory_flags).unwrap();
        let code_entry = crate::mmu::encode_entry(5, directory_flags).unwrap();
        let stack_entry =
            crate::mmu::encode_entry(6, crate::mmu::PagePermissions::USER_DATA).unwrap();
        cpu.memory[0x2000..0x2004].copy_from_slice(&directory_entry.to_be_bytes());
        cpu.memory[0x4000..0x4004].copy_from_slice(&code_entry.to_be_bytes());
        let stack_table_entry = 0x4000 + (((0xdffff >> 12) & 0x1ff) as usize) * 4;
        cpu.memory[stack_table_entry..stack_table_entry + 4]
            .copy_from_slice(&stack_entry.to_be_bytes());
        let kernel_stack_entry =
            crate::mmu::encode_entry(0xef, crate::mmu::PagePermissions::KERNEL_DATA).unwrap();
        cpu.memory[0x43bc..0x43c0].copy_from_slice(&kernel_stack_entry.to_be_bytes());
        cpu.memory[0x5000..0x5002].copy_from_slice(&((OP_LD << 11) | (1 << 5)).to_be_bytes());
        cpu.memory[0x5300..0x5302].copy_from_slice(&(OP_RTI << 11).to_be_bytes());
        cpu.enable_mmu(0x2000).unwrap();
        cpu.sp = 0xf0000;
        cpu.registers[1] = 0x2000;
        cpu.enter_user_mode(0, 0xe0000).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(
            cpu.last_page_fault(),
            Some(crate::mmu::PageFault {
                virtual_address: 0x2000,
                access: crate::mmu::MemoryAccess::Read,
                reason: crate::mmu::PageFaultReason::NotPresent,
            })
        );
        assert_eq!(cpu.read_u32(crate::machine::FAULT_ADDRESS), 0x2000);
        assert_eq!(cpu.read_u16(crate::machine::FAULT_INFO), 0x0102);
        assert_eq!(cpu.read_u32(cpu.sp), 0);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.sp, 0xe0000);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    }

    #[test]
    fn high_device_aperture_is_supervisor_only() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        for address in [
            crate::machine::SYSTEM_BASE,
            crate::machine::INTERRUPT_ENABLE,
            crate::machine::CONSOLE_DATA,
            crate::machine::FIRMWARE_TEXT_DATA,
            crate::machine::FIRMWARE_COMMAND,
        ] {
            for access in [
                crate::mmu::MemoryAccess::Read,
                crate::mmu::MemoryAccess::Write,
                crate::mmu::MemoryAccess::Fetch,
            ] {
                assert!(matches!(
                    cpu.check_guest_access(address, access),
                    Err(CpuError::PageFault(crate::mmu::PageFault {
                        reason: crate::mmu::PageFaultReason::SupervisorOnly,
                        ..
                    }))
                ));
            }
        }
        for address in [
            0x43010, 0x43014, 0x43016, 0x43018, 0x4301c, 0x43020, 0x430ff, 0x43100, 0x431ff,
            0x43200, 0x43300, 0x43400,
        ] {
            for access in [
                crate::mmu::MemoryAccess::Read,
                crate::mmu::MemoryAccess::Write,
                crate::mmu::MemoryAccess::Fetch,
            ] {
                assert!(cpu.check_guest_access(address, access).is_ok());
            }
        }
    }

    #[test]
    fn executes_load_add_compare_and_halt() {
        let mut cpu = cpu_with_program(&[
            (OP_LDI << 11) | (1 << 8) | 0xff,
            (OP_LDI << 11) | (2 << 8) | 1,
            (OP_ADD << 11) | (1 << 5) | (2 << 2),
            (OP_CMP << 11) | (2 << 5) | (1 << 2),
            OP_HALT << 11,
        ]);
        assert_eq!(cpu.run(20).unwrap(), 5);
        assert_eq!(cpu.registers[0], 0x100);
        assert_eq!(
            cpu.flags,
            Flags {
                zero: false,
                carry: true,
                negative: true,
                overflow: false,
            }
        );
        assert!(!cpu.running);
    }

    #[test]
    fn calls_and_returns_use_big_endian_30_bit_stack_addresses() {
        let mut cpu = cpu_with_program(&[(OP_CALL << 11) | 4, OP_HALT << 11, OP_RET << 11]);
        cpu.run(3).unwrap();
        assert_eq!(cpu.pc, 4);
        assert_eq!(cpu.sp, 0xffff);
    }

    #[test]
    fn memory_accesses_fault_at_configured_limit() {
        let mut cpu = Cpu::with_memory_size(256).unwrap();
        cpu.memory[255] = 0x12;
        cpu.memory[0] = 0x34;
        assert_eq!(cpu.read_guest(255).unwrap(), 0x12);
        assert!(cpu.read_guest(256).is_err());
        assert!(cpu.read_guest_u16(255).is_err());
        assert!(cpu.write_guest_u16(255, 0x5678).is_err());
        assert_eq!((cpu.memory[0], cpu.memory[255]), (0x34, 0x12));
    }

    #[test]
    fn executes_far_conditional_branch() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0, FAR_BRANCH_PREFIX);
        cpu.write_u16(2, 0);
        cpu.write_u16(4, 0x10);
        cpu.write_u16(0x10, OP_HALT << 11);
        cpu.run(2).unwrap();
        assert_eq!(cpu.pc, 0x12);
        assert!(!cpu.running);
    }

    #[test]
    fn executes_extended_alu_immediates_and_sets_flags() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.registers[2] = 0xffff;
        cpu.write_u16(0, ALU_PREFIX | (2 << 8));
        cpu.write_u16(2, (9 << 12) | 0x0800);
        cpu.write_u16(4, 1);
        cpu.write_u16(6, OP_HALT << 11);
        cpu.run(2).unwrap();
        assert_eq!(cpu.registers[2], 0);
        assert!(cpu.flags.zero);
        assert!(cpu.flags.carry);
    }

    #[test]
    fn executes_muldiv_and_preserves_divide_by_zero_behavior() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.registers[0] = 0xfffe;
        cpu.registers[1] = 0;
        cpu.write_u16(0, MULDIV_PREFIX);
        cpu.write_u16(2, (6 << 12) | 1);
        cpu.step().unwrap();
        assert_eq!(cpu.registers[0], 0xfffe);
        assert!(cpu.flags.carry);
    }

    #[test]
    fn executes_wide_pair_add_and_cmp() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.registers[0] = 0x1234;
        cpu.registers[1] = 0xffff;
        cpu.registers[2] = 0;
        cpu.registers[3] = 1;
        cpu.write_u16(0, WIDE_PREFIX);
        cpu.write_u16(2, 2);
        cpu.write_u16(4, OP_HALT << 11);
        cpu.run(2).unwrap();
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0x1235, 0));
        assert!(!cpu.flags.zero);
    }

    #[test]
    fn executes_binary32_float_add() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        let left = 1.5f32.to_bits();
        let right = 2.25f32.to_bits();
        cpu.registers[0] = i64::from(left >> 16);
        cpu.registers[1] = i64::from(left & 0xffff);
        cpu.registers[2] = i64::from(right >> 16);
        cpu.registers[3] = i64::from(right & 0xffff);
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, 2);
        cpu.write_u16(4, OP_HALT << 11);
        cpu.run(2).unwrap();
        let result = ((cpu.registers[0] as u16 as u32) << 16) | cpu.registers[1] as u16 as u32;
        assert_eq!(f32::from_bits(result), 3.75);
    }

    #[test]
    fn classic_subtraction_preserves_signed_register_results() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.registers[1] = 1;
        cpu.registers[2] = 2;
        cpu.write_u16(0, (OP_SUB << 11) | (1 << 5) | (2 << 2));
        cpu.write_u16(2, OP_HALT << 11);
        cpu.run(2).unwrap();
        assert_eq!(cpu.registers[0], -1);
        assert!(cpu.flags.carry);
        assert!(cpu.flags.negative);

        cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.registers[1] = 1;
        cpu.registers[2] = 2;
        cpu.write_u16(0, (OP_CMP << 11) | (1 << 5) | (2 << 2));
        cpu.write_u16(2, OP_HALT << 11);
        cpu.run(2).unwrap();
        assert_eq!(cpu.registers[0], 0);
        assert!(cpu.flags.carry);
        assert!(cpu.flags.negative);
    }

    #[test]
    fn assembled_base_opcodes_cover_register_moves_and_stack_round_trips() {
        let cpu = run_assembly("LDI R0, 0x12\nMOV R1, R0\nINC R1\nDEC R1\nHALT");
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0x12, 0x12));

        let mut source = String::new();
        for register in 0..8 {
            source.push_str(&format!("LDI R{register}, {}\n", register + 10));
        }
        for register in 0..8 {
            source.push_str(&format!("PUSH R{register}\n"));
        }
        for register in (0..8).rev() {
            source.push_str(&format!("POP R{register}\n"));
        }
        source.push_str("HALT\n");
        let cpu = run_assembly(&source);
        assert_eq!(
            cpu.registers,
            std::array::from_fn(|register| register as i64 + 10)
        );
        assert_eq!(cpu.sp, cpu.memory_mask);
    }

    #[test]
    fn assembled_base_opcodes_cover_label_loops_calls_and_stack_relative_memory() {
        let cpu = run_assembly(
            "LDI R0, 5\nLDI R1, 0\nloop: JZ R0, done\nINC R1\nDEC R0\nJMP loop\n\
             done: LDI R2, 5\nCMP R1, R2\nBZ equal\nLDI R3, 1\nHALT\n\
             equal: LDI R3, 2\nLDI R4, 0\nLDI R5, 0x1000\nSETSP R5\n\
             LDI R6, 0xBEEF\nSTWS R6, 2\nLDWS R7, 2\nHALT",
        );
        assert_eq!(cpu.registers[3], 2);
        assert_eq!(cpu.registers[7], 0xbeef);
        assert_eq!(cpu.sp, 0x1000);

        let cpu = run_assembly(
            "LDI R0, 3\nLDI R1, 5\nCMP R0, R1\nBC less\nLDI R2, 1\nHALT\n\
             less: CALL sub\nHALT\nsub: LDI R2, 9\nRET",
        );
        assert_eq!(cpu.registers[2], 9);
    }

    #[test]
    fn classic_integer_and_logic_ops_cover_register_and_value_edges() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        let edges = [0, 1, 0x7f, 0x80, 0xff, 0x100, 0x8000, 0xffff];
        for opcode in [OP_ADD, OP_SUB, OP_CMP, OP_AND, OP_OR, OP_XOR] {
            for destination in 0..8 {
                for left_register in 0..8 {
                    for right_register in 0..8 {
                        for left in edges {
                            for right in edges {
                                let mut registers =
                                    std::array::from_fn(|index| (0x50 + 0x11 * index) as i64);
                                registers[left_register] = left;
                                registers[right_register] = right;
                                let actual_left = registers[left_register];
                                let actual_right = registers[right_register];
                                cpu.registers = registers;
                                cpu.flags = Flags {
                                    zero: true,
                                    carry: true,
                                    negative: true,
                                    overflow: true,
                                };
                                cpu.pc = 0;
                                cpu.running = true;
                                cpu.write_u16(
                                    0,
                                    (opcode << 11)
                                        | ((destination as u16) << 8)
                                        | ((left_register as u16) << 5)
                                        | ((right_register as u16) << 2),
                                );
                                cpu.step().unwrap();

                                let result = match opcode {
                                    OP_ADD => actual_left + actual_right,
                                    OP_SUB | OP_CMP => actual_left - actual_right,
                                    OP_AND => actual_left & actual_right,
                                    OP_OR => actual_left | actual_right,
                                    OP_XOR => actual_left ^ actual_right,
                                    _ => unreachable!(),
                                };
                                if opcode != OP_CMP {
                                    registers[destination] = result;
                                }
                                assert_eq!(cpu.registers, registers);
                                match opcode {
                                    OP_ADD => {
                                        assert_eq!(cpu.flags.zero, result == 0);
                                        assert_eq!(cpu.flags.carry, result > 0xff);
                                    }
                                    OP_SUB | OP_CMP => {
                                        assert_eq!(cpu.flags.zero, result == 0);
                                        assert_eq!(cpu.flags.carry, result < 0);
                                    }
                                    _ => assert_eq!(
                                        cpu.flags,
                                        Flags {
                                            zero: true,
                                            carry: true,
                                            negative: true,
                                            overflow: true,
                                        }
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }

        for destination in 0..8 {
            for source in 0..8 {
                for value in edges {
                    let mut registers = std::array::from_fn(|index| (0x50 + 0x11 * index) as i64);
                    registers[source] = value;
                    cpu.registers = registers;
                    cpu.pc = 0;
                    cpu.running = true;
                    cpu.write_u16(
                        0,
                        (OP_NOT << 11) | ((destination as u16) << 8) | ((source as u16) << 5),
                    );
                    cpu.step().unwrap();
                    registers[destination] = i64::from(!(value as u16));
                    assert_eq!(cpu.registers, registers);
                }
            }
        }
    }

    #[test]
    fn assembled_extended_opcode_programs_cover_shifts_arithmetic_and_misc() {
        let cpu = run_assembly(
            "LDI R0, 1\nSHL R0, 4\nLDI R1, 0x8000\nLSR R1, 3\n\
             LDI R2, 0x8000\nASR R2, 3\nLDI R3, 0x8001\nROL R3, 1\n\
             LDI R4, 0x8001\nROR R4, 1\nLDI R5, 2\nLDI R6, 1\nSHL R6, R5\nHALT",
        );
        assert_eq!(&cpu.registers[..7], &[16, 0x1000, 0xf000, 3, 0xc000, 2, 4]);

        let cpu = run_assembly(
            "LDI R0, 0xFFFF\nLDI R1, 1\nADD R0, R1\nLDI R2, 0\nLDI R3, 0\n\
             ADC R2, R3\nHALT",
        );
        assert_eq!((cpu.registers[0] as u16, cpu.registers[2]), (0, 1));

        let cpu = run_assembly(
            "LDI R0, 0\nLDI R1, 1\nSUB R0, R1\nLDI R2, 5\nLDI R3, 0\n\
             SBC R2, R3\nHALT",
        );
        assert_eq!(cpu.registers[2], 4);

        let cpu = run_assembly(
            "LDI R0, 1\nNEG R0\nLDI R1, 0x1234\nSWAP R1\nLDI R2, 0x80\n\
             SEXT R2\nLDI R4, 1\nCLZ R4\nHALT",
        );
        assert_eq!(
            (
                cpu.registers[0],
                cpu.registers[1],
                cpu.registers[2],
                cpu.registers[4]
            ),
            (0xffff, 0x3412, 0xff80, 15)
        );

        let cpu = run_assembly(
            "LDI R0, 100\nADD R0, 1000\nLDI R1, 0xFFFF\nAND R1, 0x0F0F\n\
             LDI R2, 5\nSUB R2, 2\nHALT",
        );
        assert_eq!(
            (cpu.registers[0], cpu.registers[1], cpu.registers[2]),
            (1100, 0x0f0f, 3)
        );

        let cpu = run_assembly("LDI R0, 0xF0\nTEST R0, 0x0F\nHALT");
        assert_eq!(cpu.registers[0], 0xf0);
        assert!(cpu.flags.zero);
    }

    #[test]
    fn assembled_wide_opcode_programs_cover_arithmetic_memory_and_flags() {
        let cpu = run_assembly(
            "LDI R0, 0xFFFF\nLDI R1, 0xFFFF\nLDI R2, 0\nLDI R3, 1\n\
             ADD32 R0, R2\nLDI R4, 0x8000\nLDI R5, 0\nNEG32 R4\n\
             LDI R6, 0\nLDI R7, 3\nSHL32 R6, 1\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0, 0));
        assert_eq!((cpu.registers[4], cpu.registers[5]), (0x8000, 0));
        assert_eq!((cpu.registers[6], cpu.registers[7]), (0, 6));

        let cpu = run_assembly("LDI R0, 1\nLDI R1, 0\nLDI R2, 1\nLDI R3, 0\nCMP32 R0, R2\nHALT");
        assert!(cpu.flags.zero);

        let cpu = run_assembly(
            "LDI R0, 0x7FFF\nLDI R1, 0xFFFF\nLDI R2, 0\nLDI R3, 1\n\
             ADD32 R0, R2\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0x8000, 0));
        assert_eq!(
            (cpu.flags.negative, cpu.flags.overflow, cpu.flags.carry),
            (true, true, false)
        );

        let cpu = run_assembly(
            "LDI R0, 0\nLDI R1, 10\nLDI R2, 0\nLDI R3, 3\nSUB32 R0, R2\n\
             MOV32 R4, R0\nAND32 R0, R2\nOR32 R0, R2\nXOR32 R0, R2\n\
             LDI R6, 0x8000\nLDI R7, 0\nASR32 R6, 1\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0, 0));
        assert_eq!((cpu.registers[4], cpu.registers[5]), (0, 7));
        assert_eq!((cpu.registers[6], cpu.registers[7]), (0xc000, 0));
        assert!(cpu.flags.negative);

        for source in [
            "ADD32 R1, R2",
            "ADD32 R0, R3",
            "NEG32 R3",
            "SHL32 R0, 32",
            "LD32 R0, R3",
            "ST32 R0, R1",
        ] {
            assert!(
                crate::assembler::assemble(source, 0).is_err(),
                "expected invalid pair instruction to be rejected: {source}"
            );
        }
    }

    #[test]
    fn address_values_keep_thirty_bits_independently_of_configured_ram_size() {
        let image = crate::assembler::assemble(
            "LDI30 R0, 0x12345678\nSPLIT30 R0, R1\nJOIN30 R0, R1\n\
             LDI30 R2, 0x100\nSTP R0, R2\nLDP R3, R2\n\
             STP R7, R2\nLDP R6, R2\nSPLIT30 R7, R4\n\
             LDI30 R5, 0x3FFFFFFF\nHALT",
            0,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.registers[7] = i64::from(u32::MAX);
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 0x1234_5678);
        assert_eq!(cpu.registers[1], 0x5678);
        assert_eq!(cpu.registers[3], 0x1234_5678);
        assert_eq!(cpu.registers[4], 0xffff);
        assert_eq!(cpu.registers[5], 0x3fff_ffff);
        assert_eq!(cpu.registers[6], 0x3fff_ffff);
        assert_eq!(cpu.registers[7], 0x3fff);
        assert_eq!(&cpu.memory[0x100..0x104], &[0x3f, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn wide_shifts_preserve_the_last_shifted_bit_and_zero_count_carry() {
        for (instruction, input, expected, carry) in [
            ("SHL32 R0, 1", 0x8000_0001u32, 2u32, true),
            ("SHL32 R0, 1", 0x4000_0000, 0x8000_0000, false),
            ("LSR32 R0, 1", 0x8000_0001, 0x4000_0000, true),
            ("ASR32 R0, 1", 0x8000_0001, 0xc000_0000, true),
            ("LSR32 R0, 31", 0x4000_0000, 0, true),
            ("SHL32 R0, 31", 2, 0, true),
            ("SHL32 R0, 0", 1, 1, true),
            ("LSR32 R0, 0", 0x8000_0001, 0x8000_0001, true),
            ("ASR32 R0, 0", 0x8000_0001, 0x8000_0001, true),
            ("SHL32 R0, R2", 0x8000_0001, 2, true),
            ("LSR32 R0, R2", 3, 1, true),
        ] {
            let image = crate::assembler::assemble(&format!("{instruction}\nHALT"), 0).unwrap();
            let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.set_pair(0, input);
            cpu.registers[2] = 1;
            cpu.flags.carry = true;
            cpu.flags.overflow = true;
            cpu.run(2).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.register_pair_u32(0), expected, "{instruction}");
            assert_eq!(cpu.flags.carry, carry, "{instruction}");
            assert!(!cpu.flags.overflow, "{instruction}");
            assert_eq!(cpu.flags.zero, expected == 0, "{instruction}");
            assert_eq!(
                cpu.flags.negative,
                expected & 0x8000_0000 != 0,
                "{instruction}"
            );
        }
    }

    #[test]
    fn assembled_wide_opcodes_cover_signed_math_and_bounded_memory() {
        let cpu = run_assembly(
            "LDI R0, 0\nLDI R1, 100\nLDI R2, 0\nLDI R3, 9\nMUL32 R0, R2\n\
             LDI R4, 0\nLDI R5, 10\nDIV32 R0, R4\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0, 90));

        let cpu = run_assembly(
            "LDI R0, 0xFFFF\nLDI R1, 0xFF9C\nLDI R2, 0\nLDI R3, 7\n\
             DIV32S R0, R2\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0xffff, 0xfff2));

        let cpu = run_assembly(
            "LDI R0, 0xFFFF\nLDI R1, 0xFF9C\nLDI R2, 0\nLDI R3, 7\n\
             MOD32S R0, R2\nHALT",
        );
        assert_eq!((cpu.registers[0], cpu.registers[1]), (0xffff, 0xfffe));

        let cpu = run_assembly(
            "LDI R0, 0x000F\nLDI R1, 0xFFFC\nLDI R2, 0x1234\nLDI R3, 0x5678\n\
             ST32 R2, R0\nLD32 R4, R0\nHALT",
        );
        assert_eq!((cpu.registers[4], cpu.registers[5]), (0x1234, 0x5678));
        assert_eq!(&cpu.memory[0xffffc..0x100000], &[0x12, 0x34, 0x56, 0x78]);

        let cpu = run_assembly(
            "LDI30 R0, 0x12345678\nLDI30 R1, 0x000123f0\nSTP R0, R1\n\
             LDP R2, R1\nHALT",
        );
        assert_eq!(cpu.registers[2], 0x12345678);
        assert_eq!(&cpu.memory[0x123f0..0x123f3], &[0x12, 0x34, 0x56]);
        assert_eq!(&cpu.memory[0x123f3..0x123f6], &[0x78, 0x00, 0x00]);
    }

    #[test]
    fn assembled_extended_opcodes_cover_stack_pointer_control_and_branches() {
        let cpu = run_assembly("MOVSPH R0\nHALT");
        assert_eq!(cpu.registers[0], i64::from(cpu.memory_mask >> 16));

        let cpu = run_assembly(
            "LDI R1, 0x1234\nSETSP R1\nSETC\nLDI R0, 0xFFFF\nSETSPH R0\n\
             MOVSP R1\nMOVSPH R2\nHALT",
        );
        assert_eq!(cpu.sp, 0x3fff_1234);
        assert_eq!((cpu.registers[1], cpu.registers[2]), (0x3fff_1234, 0x3fff));
        assert!(cpu.flags.carry);

        let cpu = run_assembly("LDI30 R0, 0x51234\nHALT");
        assert_eq!(cpu.registers[0], 0x51234);

        let template = "LDI R0, {a}\nLDI R1, {b}\nCMP R0, R1\n{branch} yes\n\
                        LDI R7, 0\nHALT\nyes: LDI R7, 1\nHALT\n";
        for (branch, left, right, taken) in [
            ("BLT", 0xffff, 1, 1),
            ("BLT", 1, 0xffff, 0),
            ("BGE", 5, 5, 1),
            ("BGT", 5, 5, 0),
            ("BLE", 5, 5, 1),
            ("BHI", 0xffff, 1, 1),
            ("BHI", 1, 0xffff, 0),
            ("BLS", 1, 0xffff, 1),
            ("BLT", 0x7fff, 0x8000, 0),
        ] {
            let source = template
                .replace("{a}", &left.to_string())
                .replace("{b}", &right.to_string())
                .replace("{branch}", branch);
            assert_eq!(
                run_assembly(&source).registers[7],
                taken,
                "{branch} with {left:#06x}, {right:#06x}"
            );
        }

        let cpu =
            run_assembly("LDI30 R1, target\nJMPR R1\nLDI R0, 1\nHALT\ntarget: LDI R0, 7\nHALT");
        assert_eq!(cpu.registers[0], 7);

        let cpu = run_assembly("LDI30 R1, fn\nCALLR R1\nHALT\nfn: LDI R0, 9\nRET");
        assert_eq!(cpu.registers[0], 9);

        let cpu = run_assembly("SETC\nPUSHF\nCLC\nPOPF\nHALT");
        assert!(cpu.flags.carry);
    }

    #[test]
    fn memory_load_store_stack_and_wrap_cases_cover_register_pairs() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for destination in 0..8 {
            for address_register in 0..8 {
                let mut registers = [0; 8];
                registers[address_register] = 0x1000 + (address_register as i64 * 8);
                cpu.registers = registers;
                cpu.pc = 0;
                cpu.running = true;
                cpu.write_u16(
                    0,
                    (OP_LD << 11) | ((destination as u16) << 8) | ((address_register as u16) << 5),
                );
                cpu.write(registers[address_register] as u32, 0xa5);
                cpu.step().unwrap();
                registers[destination] = 0xa5;
                assert_eq!(cpu.registers, registers);

                registers[destination] = 0x1abc;
                cpu.registers = registers;
                cpu.pc = 0;
                cpu.running = true;
                cpu.write_u16(
                    0,
                    (OP_ST << 11) | ((destination as u16) << 8) | ((address_register as u16) << 5),
                );
                cpu.step().unwrap();
                assert_eq!(cpu.read(registers[address_register] as u32), 0xbc);

                registers[address_register] = 0x1100 + (address_register as i64 * 8);
                cpu.registers = registers;
                cpu.pc = 0;
                cpu.running = true;
                cpu.write_u16(
                    0,
                    (OP_LDW << 11) | ((destination as u16) << 8) | ((address_register as u16) << 5),
                );
                cpu.write_u16(registers[address_register] as u32, 0x1234);
                cpu.step().unwrap();
                registers[destination] = 0x1234;
                assert_eq!(cpu.registers, registers);

                registers[destination] = 0xbeef;
                cpu.registers = registers;
                cpu.pc = 0;
                cpu.running = true;
                cpu.write_u16(
                    0,
                    (OP_STW << 11) | ((destination as u16) << 8) | ((address_register as u16) << 5),
                );
                cpu.step().unwrap();
                assert_eq!(cpu.read_u16(registers[address_register] as u32), 0xbeef);
            }
        }

        cpu.write(0xffff, 0x12);
        cpu.write(0, 0x34);
        assert!(cpu.read_guest_u16(0xffff).is_err());
        assert!(cpu.write_guest_u16(0xffff, 0x5678).is_err());
        assert_eq!(cpu.read(0xffff), 0x12);
        assert_eq!(cpu.read(0), 0x34);
    }

    #[test]
    fn classic_half_float_ops_cover_each_action_and_register_field() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for destination in 0..8 {
            for source in 0..8 {
                for action in 1..=7 {
                    let mut registers =
                        std::array::from_fn(|index| f32_to_f16(index as f32 + 0.5) as i64);
                    registers[destination] = f32_to_f16(6.0) as i64;
                    registers[source] = f32_to_f16(1.5) as i64;
                    cpu.registers = registers;
                    cpu.flags = Flags::default();
                    cpu.pc = 0;
                    cpu.running = true;
                    cpu.write_u16(
                        0,
                        (OP_SHR << 11)
                            | ((destination as u16) << 8)
                            | (action << 5)
                            | ((source as u16) << 2),
                    );
                    cpu.step().unwrap();

                    if action <= 4 {
                        let left = f16_to_f32(registers[destination] as u16);
                        let right = f16_to_f32(registers[source] as u16);
                        let value = match action {
                            1 => left + right,
                            2 => left - right,
                            3 => left * right,
                            _ => left / right,
                        };
                        registers[destination] = i64::from(f32_to_f16(value));
                        assert_eq!(cpu.registers, registers);
                    } else if action == 5 {
                        let left = f16_to_f32(registers[destination] as u16);
                        let right = f16_to_f32(registers[source] as u16);
                        assert_eq!(cpu.registers, registers);
                        assert_eq!(cpu.flags.zero, left == right);
                        assert_eq!(cpu.flags.carry, left < right);
                    } else if action == 6 {
                        let signed = (registers[destination] as u16) as i16;
                        registers[destination] = i64::from(f32_to_f16(f32::from(signed)));
                        assert_eq!(cpu.registers, registers);
                    } else {
                        let value = f16_to_f32(registers[destination] as u16).trunc() as i64;
                        registers[destination] = value as u16 as i64;
                        assert_eq!(cpu.registers, registers);
                    }
                }
            }
        }

        for (action, left, right, expected) in [
            (3u16, 60_000.0f32, 2.0f32, 0x7c00u16),
            (3, f32::INFINITY, 0.0, 0x7e00),
            (4, 2.0, 0.0, 0x7c00),
            (4, -2.0, 0.0, 0xfc00),
            (4, 0.0, 0.0, 0x7e00),
            (2, -0.0, 0.0, 0x8000),
        ] {
            cpu.registers[0] = i64::from(f32_to_f16(left));
            cpu.registers[1] = i64::from(f32_to_f16(right));
            cpu.pc = 0;
            cpu.running = true;
            cpu.write_u16(0, (OP_SHR << 11) | (action << 5) | (1 << 2));
            cpu.step().unwrap();
            assert_eq!(cpu.registers[0], i64::from(expected));
        }

        cpu.registers[0] = i64::from(f32_to_f16(f32::NAN));
        cpu.registers[1] = i64::from(f32_to_f16(1.0));
        cpu.flags = Flags::default();
        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(0, (OP_SHR << 11) | (5 << 5) | (1 << 2));
        cpu.step().unwrap();
        assert!(!cpu.flags.zero);
        assert!(!cpu.flags.carry);

        cpu.registers[0] = i64::from(f32_to_f16(-0.0));
        cpu.registers[1] = i64::from(f32_to_f16(0.0));
        cpu.flags = Flags::default();
        cpu.pc = 0;
        cpu.running = true;
        cpu.step().unwrap();
        assert!(cpu.flags.zero);
        assert!(!cpu.flags.carry);
    }

    #[test]
    fn ieee_float_binary_operations_cover_every_valid_register_pair() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for destination in [0usize, 2, 4, 6] {
            for source in [0usize, 2, 4, 6] {
                for action in 0..=4u16 {
                    let left = 6.0f32;
                    let right = if destination == source { left } else { 1.5 };
                    cpu.registers = [0; 8];
                    cpu.write_float_register(destination, 32, f64::from(left));
                    cpu.write_float_register(source, 32, f64::from(right));
                    cpu.flags = Flags::default();
                    cpu.pc = 0;
                    cpu.running = true;
                    cpu.write_u16(
                        0,
                        (OP_SHR << 11) | ((destination as u16) << 8) | FLOAT_PREFIX,
                    );
                    cpu.write_u16(2, (action << 11) | source as u16);
                    cpu.step().unwrap();

                    if action == 4 {
                        assert_eq!(cpu.flags.zero, left == right);
                        assert_eq!(cpu.flags.carry, left < right);
                    } else {
                        let expected = match action {
                            0 => left + right,
                            1 => left - right,
                            2 => left * right,
                            _ => left / right,
                        };
                        assert_eq!(
                            cpu.read_float_register(destination, 32) as f32,
                            expected,
                            "F32 action {action}, R{destination}, R{source}"
                        );
                    }
                }
            }
        }

        for destination in [0usize, 4] {
            for source in [0usize, 4] {
                for action in 5..=9u16 {
                    let left = 6.0f64;
                    let right = if destination == source { left } else { 1.5 };
                    cpu.registers = [0; 8];
                    cpu.write_float_register(destination, 64, left);
                    cpu.write_float_register(source, 64, right);
                    cpu.flags = Flags::default();
                    cpu.pc = 0;
                    cpu.running = true;
                    cpu.write_u16(
                        0,
                        (OP_SHR << 11) | ((destination as u16) << 8) | FLOAT_PREFIX,
                    );
                    cpu.write_u16(2, (action << 11) | source as u16);
                    cpu.step().unwrap();

                    if action == 9 {
                        assert_eq!(cpu.flags.zero, left == right);
                        assert_eq!(cpu.flags.carry, left < right);
                    } else {
                        let expected = match action {
                            5 => left + right,
                            6 => left - right,
                            7 => left * right,
                            _ => left / right,
                        };
                        assert_eq!(
                            cpu.read_float_register(destination, 64),
                            expected,
                            "F64 action {action}, R{destination}, R{source}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ieee_float_instructions_cover_arithmetic_precision_and_special_values() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for (width, base, actions) in [(32u8, 0usize, 0u16..=4), (64, 0, 5u16..=9)] {
            for action in actions {
                let mut registers = [0i64; 8];
                if width == 32 {
                    let left = 6.0f32.to_bits();
                    let right = 1.5f32.to_bits();
                    registers[base] = i64::from(left >> 16);
                    registers[base + 1] = i64::from(left & 0xffff);
                    registers[base + 2] = i64::from(right >> 16);
                    registers[base + 3] = i64::from(right & 0xffff);
                } else {
                    let left = 6.0f64.to_bits();
                    let right = 1.5f64.to_bits();
                    for offset in 0..4 {
                        registers[offset] = ((left >> (48 - offset * 16)) & 0xffff) as i64;
                        registers[offset + 4] = ((right >> (48 - offset * 16)) & 0xffff) as i64;
                    }
                }
                cpu.registers = registers;
                cpu.flags = Flags::default();
                cpu.pc = 0;
                cpu.running = true;
                cpu.write_u16(0, FLOAT_PREFIX);
                let source = if width == 32 { 2 } else { 4 };
                cpu.write_u16(2, (action << 11) | source);
                cpu.step().unwrap();
                if action % 5 == 4 {
                    assert!(!cpu.flags.zero);
                    assert!(!cpu.flags.carry);
                    assert_eq!(cpu.registers, registers);
                } else if width == 32 {
                    let left = 6.0f32;
                    let right = 1.5f32;
                    let result = match action {
                        0 => left + right,
                        1 => left - right,
                        2 => left * right,
                        _ => left / right,
                    };
                    let bits = result.to_bits();
                    assert_eq!(
                        &cpu.registers[..2],
                        &[i64::from(bits >> 16), i64::from(bits & 0xffff)]
                    );
                } else {
                    let left = 6.0f64;
                    let right = 1.5f64;
                    let result = match action {
                        5 => left + right,
                        6 => left - right,
                        7 => left * right,
                        _ => left / right,
                    };
                    let bits = result.to_bits();
                    let expected = std::array::from_fn::<_, 4, _>(|offset| {
                        ((bits >> (48 - offset * 16)) & 0xffff) as i64
                    });
                    assert_eq!(&cpu.registers[..4], &expected);
                }
            }
        }

        for (action, value, expected) in [
            (0u16, f32::INFINITY, 0x7f80_0000u32),
            (0, f32::NEG_INFINITY, 0xff80_0000),
            (1, -0.0, 0x8000_0000),
            (0, f32::from_bits(1), 0x0000_0001),
        ] {
            let bits = value.to_bits();
            cpu.registers[..4].copy_from_slice(&[
                i64::from(bits >> 16),
                i64::from(bits & 0xffff),
                0,
                0,
            ]);
            cpu.pc = 0;
            cpu.running = true;
            cpu.write_u16(0, FLOAT_PREFIX);
            cpu.write_u16(2, action << 11 | 2);
            cpu.step().unwrap();
            let actual = ((cpu.registers[0] as u16 as u32) << 16) | cpu.registers[1] as u16 as u32;
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn ieee_float_conversions_cover_integer_widths_precision_changes_and_unary_ops() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        let integer_cases = [
            (10u16, -12_345i64),
            (11, 65_530),
            (12, -1_234_567),
            (13, 3_456_789),
            (14, -12_345),
            (15, 65_530),
            (16, -1_234_567),
            (17, 3_456_789),
        ];
        for (action, value) in integer_cases {
            let mut registers = [0i64; 8];
            let format = (action - 10) % 4;
            if format < 2 {
                registers[2] = value;
            } else {
                registers[2] = (value as u32 >> 16) as i64;
                registers[3] = (value as u32 & 0xffff) as i64;
            }
            cpu.registers = registers;
            assert_eq!(
                cpu.read_integer_register(
                    2,
                    if format < 2 { 16 } else { 32 },
                    format == 1 || format == 3
                ),
                value
            );
            cpu.execute_float(action, 0, 2);
            let expected = if action < 14 {
                (value as f32).to_bits() as u64
            } else {
                (value as f64).to_bits()
            };
            let width = if action < 14 { 2 } else { 4 };
            for offset in 0..width {
                assert_eq!(
                    cpu.registers[offset],
                    ((expected >> ((width - offset - 1) * 16)) & 0xffff) as i64,
                    "integer conversion action {action}"
                );
            }
        }

        let value = 1.0f32 + 2.0f32.powi(-24);
        let bits = value.to_bits();
        cpu.registers[..4].copy_from_slice(&[
            0,
            0,
            i64::from(bits >> 16),
            i64::from(bits & 0xffff),
        ]);
        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, (18 << 11) | 2);
        cpu.step().unwrap();
        assert_eq!(cpu.read_u64_registers(0), f64::from(value).to_bits());

        cpu.write_u64_registers(4, (1.0f64 + f64::EPSILON).to_bits());
        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, (19 << 11) | 4);
        cpu.step().unwrap();
        assert_eq!(
            ((cpu.registers[0] as u16 as u32) << 16) | cpu.registers[1] as u16 as u32,
            1.0f32.to_bits()
        );

        for (action, source, value, expected) in [
            (20u16, 2usize, 3.25f64, -3.25f64),
            (21, 4, 3.25, -3.25),
            (22, 2, 9.0, 3.0),
            (23, 4, 9.0, 3.0),
        ] {
            let mut registers = [0i64; 8];
            if action == 20 || action == 22 {
                let bits = (value as f32).to_bits();
                registers[source] = i64::from(bits >> 16);
                registers[source + 1] = i64::from(bits & 0xffff);
            } else {
                let bits = value.to_bits();
                for offset in 0..4 {
                    registers[source + offset] = ((bits >> (48 - offset * 16)) & 0xffff) as i64;
                }
            }
            cpu.registers = registers;
            cpu.pc = 0;
            cpu.running = true;
            cpu.write_u16(0, FLOAT_PREFIX);
            cpu.write_u16(2, (action << 11) | source as u16);
            cpu.step().unwrap();
            if action == 20 || action == 22 {
                let actual =
                    ((cpu.registers[0] as u16 as u32) << 16) | cpu.registers[1] as u16 as u32;
                assert_eq!(actual, (expected as f32).to_bits());
            } else {
                assert_eq!(cpu.read_u64_registers(0), expected.to_bits());
            }
        }

        for (action, source, value, expected, width) in [
            (24u16, 2usize, -12.75f64, (-12i16) as u16 as u32, 16u8),
            (25, 2, -12.75, (-12i16) as u16 as u32, 16),
            (26, 2, -12.75, (-12i32) as u32, 32),
            (27, 2, -12.75, (-12i32) as u32, 32),
            (28, 4, -12.75, (-12i16) as u16 as u32, 16),
            (29, 4, -12.75, (-12i16) as u16 as u32, 16),
            (30, 4, -12.75, (-12i32) as u32, 32),
            (31, 4, -12.75, (-12i32) as u32, 32),
        ] {
            let mut registers = [0i64; 8];
            if source == 2 {
                let bits = (value as f32).to_bits();
                registers[source] = i64::from(bits >> 16);
                registers[source + 1] = i64::from(bits & 0xffff);
            } else {
                let bits = value.to_bits();
                for offset in 0..4 {
                    registers[source + offset] = ((bits >> (48 - offset * 16)) & 0xffff) as i64;
                }
            }
            cpu.registers = registers;
            cpu.pc = 0;
            cpu.running = true;
            cpu.write_u16(0, FLOAT_PREFIX);
            cpu.write_u16(2, (action << 11) | source as u16);
            cpu.step().unwrap();
            if width == 16 {
                assert_eq!(cpu.registers[0], i64::from(expected as u16));
            } else {
                assert_eq!(
                    ((cpu.registers[0] as u16 as u32) << 16) | cpu.registers[1] as u16 as u32,
                    expected
                );
            }
        }

        let nan = f32::INFINITY.to_bits();
        let negative_nan = f32::NEG_INFINITY.to_bits();
        cpu.registers[..4].copy_from_slice(&[
            i64::from(nan >> 16),
            i64::from(nan & 0xffff),
            i64::from(negative_nan >> 16),
            i64::from(negative_nan & 0xffff),
        ]);
        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.register_pair_u32(0), 0x7fc0_0000);

        cpu.registers[..4].copy_from_slice(&[0x8000, 0, 0, 0]);
        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, (4 << 11) | 2);
        cpu.step().unwrap();
        assert!(cpu.flags.zero);
        assert!(!cpu.flags.carry);
    }

    #[test]
    fn key_masks_use_sc16_key_bit_assignments() {
        let mut cpu = Cpu::with_memory_size(1 << 19).unwrap();
        cpu.set_key(0x20_0405, true);
        assert_eq!(
            &[
                cpu.read(crate::machine::INPUT_KEYS + 1),
                cpu.read(crate::machine::INPUT_KEYS + 2),
                cpu.read(crate::machine::INPUT_KEYS + 3),
            ],
            &[0x20, 0x04, 0x05]
        );
        cpu.set_key(0x20_0405, false);
        assert_eq!(
            &[
                cpu.read(crate::machine::INPUT_KEYS + 1),
                cpu.read(crate::machine::INPUT_KEYS + 2),
                cpu.read(crate::machine::INPUT_KEYS + 3),
            ],
            &[0, 0, 0]
        );
    }

    #[test]
    fn mounting_data_disks_preserves_the_loaded_program_and_rejects_missing_ids() {
        let root = std::env::temp_dir().join(format!(
            "sc16-cpu-data-disk-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = root.join("disks");
        let disk_path = storage.join("DATA0001");
        std::fs::create_dir_all(&disk_path).unwrap();
        std::fs::write(
            disk_path.join("disk.json"),
            r#"{"version":1,"id":"DATA0001","label":"Data","capacity_bytes":4096,"read_only":false,"protected_files":[]}"#,
        )
        .unwrap();
        std::fs::write(disk_path.join("payload.bin"), [0xa5; 256]).unwrap();

        let module_files = root.join("program");
        std::fs::create_dir_all(&module_files).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.configure_disk(storage, module_files).unwrap();
        cpu.pc = 0x1234;
        cpu.memory[0x1234] = 0x5a;
        assert_eq!(cpu.available_disks().unwrap(), ["DATA0001"]);
        assert!(cpu.mount_data_disk("MISSING1").is_err());
        assert!(cpu.mount_data_disk("../DATA0001").is_err());
        cpu.mount_data_disk("DATA0001").unwrap();
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.memory[0x1234], 0x5a);
        assert!(cpu.folder_disk.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_additional_settings_can_be_read_and_updated_with_typed_values() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.bios_settings
            .additional
            .insert("enabled".to_owned(), BiosSettingValue::Boolean(false));
        cpu.bios_settings
            .additional
            .insert("retries".to_owned(), BiosSettingValue::Integer(5));
        cpu.bios_settings.additional.insert(
            "label".to_owned(),
            BiosSettingValue::String("demo".to_owned()),
        );

        for (index, name, kind, value) in [
            (0, "enabled", 0, "No"),
            (1, "label", 1, "demo"),
            (2, "retries", 2, "5"),
        ] {
            cpu.write_u16(FIRMWARE_INDEX, index);
            cpu.write(FIRMWARE_COMMAND, 6);
            assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
            let name_bytes: Vec<_> = (0..17)
                .map(|offset| cpu.read(FIRMWARE_SETTING + offset))
                .take_while(|byte| *byte != 0)
                .collect();
            assert_eq!(name_bytes, name.as_bytes());
            assert_eq!(cpu.read(FIRMWARE_SETTING + 17), kind);
            let value_bytes: Vec<_> = (0..24)
                .map(|offset| cpu.read(FIRMWARE_SETTING + 18 + offset))
                .take_while(|byte| *byte != 0)
                .collect();
            assert_eq!(value_bytes, value.as_bytes());
        }

        cpu.write_u16(FIRMWARE_INDEX, 0);
        cpu.write(FIRMWARE_COMMAND, 6);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        cpu.write(FIRMWARE_SETTING + 17, 0);
        for offset in 0..24 {
            cpu.write(FIRMWARE_SETTING + 18 + offset, 0);
        }
        for (offset, byte) in b"Yes".iter().enumerate() {
            cpu.write(FIRMWARE_SETTING + 18 + offset as u32, *byte);
        }
        cpu.write(FIRMWARE_COMMAND, 7);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(
            cpu.bios_settings.additional.get("enabled"),
            Some(&BiosSettingValue::Boolean(true))
        );

        cpu.write_u16(FIRMWARE_INDEX, 2);
        cpu.write(FIRMWARE_COMMAND, 6);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        cpu.write(FIRMWARE_SETTING + 17, 2);
        for offset in 0..24 {
            cpu.write(FIRMWARE_SETTING + 18 + offset, 0);
        }
        for (offset, byte) in b"-9".iter().enumerate() {
            cpu.write(FIRMWARE_SETTING + 18 + offset as u32, *byte);
        }
        cpu.write(FIRMWARE_COMMAND, 7);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(
            cpu.bios_settings.additional.get("retries"),
            Some(&BiosSettingValue::Integer(-9))
        );
    }

    #[test]
    fn bios_preferred_disk_save_does_not_change_startup_policy() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-bios-preferred-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disks = root.join("disks");
        for id in ["DISK0001", "DISK0002", "DISK0003"] {
            std::fs::create_dir_all(disks.join(id)).unwrap();
        }

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.disk_root = disks;
        cpu.configure_bios_settings(root.join("bios.json")).unwrap();
        cpu.bios_settings.preferred_disk = Some("DISK0001".to_owned());
        cpu.bios_settings.startup_behavior = StartupBehavior::BootPreferred;
        cpu.bios_settings.network_name = "NODE".to_owned();
        cpu.bios_settings.save(&root.join("bios.json")).unwrap();
        cpu.write_u16(FIRMWARE_INDEX, 2);
        cpu.write(FIRMWARE_STARTUP, 1);
        cpu.write(FIRMWARE_COMMAND, 9);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);

        assert_eq!(
            cpu.bios_settings.preferred_disk.as_deref(),
            Some("DISK0003")
        );
        assert_eq!(
            cpu.bios_settings.startup_behavior,
            StartupBehavior::BootPreferred
        );
        assert_eq!(cpu.read(FIRMWARE_STARTUP), 1);
        let saved = BiosSettings::load(&root.join("bios.json")).unwrap();
        assert_eq!(saved.preferred_disk.as_deref(), Some("DISK0003"));
        assert_eq!(saved.startup_behavior, StartupBehavior::BootPreferred);

        cpu.write_u16(FIRMWARE_INDEX, 3);
        cpu.write(FIRMWARE_COMMAND, 9);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 1);
        assert_eq!(
            cpu.bios_settings.preferred_disk.as_deref(),
            Some("DISK0003")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_firmware_shows_selected_disk_manifest_and_boot_metadata() {
        use crate::machine::*;

        let root = std::env::temp_dir().join(format!(
            "sc16-bios-disk-info-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk_path = root.join("anaclock");
        std::fs::create_dir_all(&disk_path).unwrap();
        std::fs::write(
            disk_path.join("disk.json"),
            r#"{"version":1,"id":"anaclock","label":"Friendly App","capacity_bytes":123456,"read_only":false,"protected_files":["main.bin"]}"#,
        )
        .unwrap();
        let image = crate::assembler::assemble("start: HALT\n", 0).unwrap();
        crate::image::write_image(&disk_path.join("main.bin"), &image).unwrap();

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.disk_root = root.clone();
        cpu.write(FIRMWARE_COMMAND, 8);
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(
            cpu.read_u16(FIRMWARE_OUTPUT_LENGTH),
            b"Friendly App | 123456 B | READ-WRITE | USER".len() as u16
        );
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        cpu.load_program(&firmware.segments, firmware.entry_point)
            .unwrap();
        for _ in 0..30_000 {
            cpu.step().unwrap();
        }

        let disk_info = cpu.memory[0x5080..0x5100]
            .iter()
            .take_while(|byte| **byte != 0)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(cpu.read(FIRMWARE_STATUS), 0);
        assert_eq!(disk_info, b"Friendly App | 123456 B | READ-WRITE | USER");
        let text = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| cell.character as char)
            .collect::<String>();
        assert!(
            text.contains("Friendly App | 123456 B | READ-WRITE | USER"),
            "{text}"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_firmware_renders_dynamic_typed_settings() {
        let root = std::env::temp_dir().join(format!(
            "sc16-bios-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("CUSTOM01")).unwrap();

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.disk_root = root.clone();
        let settings_path = root.join("bios.json");
        cpu.configure_bios_settings(settings_path.clone()).unwrap();
        cpu.bios_settings.additional.insert(
            "feature_enabled".to_owned(),
            BiosSettingValue::Boolean(true),
        );
        cpu.bios_settings
            .additional
            .insert("retry_count".to_owned(), BiosSettingValue::Integer(42));
        cpu.bios_settings.additional.insert(
            "display_name".to_owned(),
            BiosSettingValue::String("example".to_owned()),
        );
        let image = crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();

        for _ in 0..30_000 {
            cpu.step().unwrap();
        }

        let text = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| cell.character as char)
            .collect::<String>();
        assert!(text.contains("feature_enabled [BOOL] Yes"), "{text}");
        assert!(text.contains("retry_count [NUMBER] 42"), "{text}");
        assert!(text.contains("display_name [STRING] example"), "{text}");
        assert_eq!(cpu.registers[0], 0, "disk register after initial render");

        let send_key = |cpu: &mut Cpu, key| {
            cpu.signal_keyboard_event(key);
            for _ in 0..5_000 {
                cpu.step().unwrap();
                if cpu.input.key == 0 {
                    break;
                }
            }
            for _ in 0..2_000 {
                cpu.step().unwrap();
            }
        };
        for selected in 1u8..=5 {
            send_key(&mut cpu, 0x81);
            assert_eq!(cpu.memory[0x5021], selected, "selection after down key");
        }
        assert_eq!(cpu.memory[0x5021], 5);
        send_key(&mut cpu, 13);
        assert_eq!(cpu.read(crate::machine::FIRMWARE_SETTING + 17), 2);
        send_key(&mut cpu, 8);
        send_key(&mut cpu, 8);
        send_key(&mut cpu, b'9');
        send_key(&mut cpu, 13);
        assert_eq!(
            cpu.bios_settings.additional.get("retry_count"),
            Some(&BiosSettingValue::Integer(9))
        );
        send_key(&mut cpu, 0x80);
        send_key(&mut cpu, 0x80);
        send_key(&mut cpu, 13);
        send_key(&mut cpu, 8);
        send_key(&mut cpu, b'X');
        send_key(&mut cpu, 13);
        assert_eq!(
            cpu.bios_settings.additional.get("display_name"),
            Some(&BiosSettingValue::String("examplX".to_owned()))
        );
        send_key(&mut cpu, 0x81);
        send_key(&mut cpu, 13);
        send_key(&mut cpu, 0x83);
        send_key(&mut cpu, 13);
        assert_eq!(
            cpu.bios_settings.additional.get("feature_enabled"),
            Some(&BiosSettingValue::Boolean(false))
        );
        assert_eq!(cpu.memory[0x5022], 1);
        for _ in 0..4 {
            send_key(&mut cpu, 0x80);
        }
        assert_eq!(cpu.memory[0x5021], 0);
        send_key(&mut cpu, 13);
        assert_eq!(cpu.memory[BIOS_FIRMWARE_MARKER as usize], 1);
        assert!(cpu.booted_disk_path.is_none());
        assert_eq!(cpu.read_u16(crate::machine::FIRMWARE_INDEX), 0);
        send_key(&mut cpu, 0x89);
        assert_eq!(
            cpu.read(crate::machine::FIRMWARE_STATUS),
            0,
            "{:?}",
            cpu.debug_console.text_snapshot()
        );
        assert_eq!(cpu.memory[0x5022], 0);
        assert_eq!(
            BiosSettings::load(&settings_path)
                .unwrap()
                .additional
                .get("retry_count"),
            Some(&BiosSettingValue::Integer(9))
        );

        for _ in 0..5 {
            send_key(&mut cpu, 0x81);
        }
        send_key(&mut cpu, 13);
        send_key(&mut cpu, b'8');
        send_key(&mut cpu, 13);
        assert_eq!(
            cpu.bios_settings.additional.get("retry_count"),
            Some(&BiosSettingValue::Integer(98))
        );
        send_key(&mut cpu, 0x88);
        assert_eq!(cpu.memory[0x5022], 0);
        assert_eq!(
            cpu.bios_settings.additional.get("retry_count"),
            Some(&BiosSettingValue::Integer(9))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn binary16_conversion_rounds_and_handles_special_values() {
        assert_eq!(f16_to_f32(f32_to_f16(1.5)), 1.5);
        assert_eq!(f16_to_f32(f32_to_f16(f32::INFINITY)), f32::INFINITY);
        assert!(f16_to_f32(f32_to_f16(f32::NAN)).is_nan());
        assert_eq!(f32_to_f16(1.0 + f32::EPSILON), 0x3c00);
    }

    #[test]
    fn console_mmio_reads_and_keyboard_events_are_stateful() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.receive_console_input(b"A");
        assert_eq!(cpu.read(crate::machine::CONSOLE_STATUS), 3);
        assert_eq!(cpu.read(crate::machine::CONSOLE_DATA), b'A');
        assert_eq!(cpu.read(crate::machine::CONSOLE_STATUS), 2);
        assert_eq!(cpu.console.output(), b"A");
        assert!(cpu.frame_yield);
        assert!(!cpu.frame_presented);
        cpu.frame_yield = false;
        cpu.signal_keyboard_event(b'B');
        assert_eq!(cpu.input.key, b'B');
        assert_eq!(cpu.read(crate::machine::INPUT_KEY), b'B');
        assert_eq!(cpu.input.key, 0);
        assert_eq!(read_interrupt_control(&cpu) & 2, 2);
    }

    #[test]
    fn guest_text_mmio_controls_display_and_writes_to_selected_cursor() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(crate::machine::FIRMWARE_TEXT_FOREGROUND, 3);
        cpu.write(crate::machine::FIRMWARE_TEXT_BACKGROUND, 4);
        cpu.write(crate::machine::FIRMWARE_TEXT_ROW, 1);
        cpu.write(crate::machine::FIRMWARE_TEXT_COLUMN, 2);
        cpu.write(crate::machine::FIRMWARE_TEXT_CONTROL, 2);
        cpu.write(crate::machine::FIRMWARE_TEXT_DATA, b'A');

        let cell = cpu.guest_text.cells[crate::guest_text::GUEST_TEXT_COLUMNS + 2];
        assert_eq!(cell.character, b'A');
        assert_eq!(cell.foreground, 3);
        assert_eq!(cell.background, 4);
        assert_eq!(cpu.guest_text.cursor_row, 1);
        assert_eq!(cpu.guest_text.cursor_column, 3);
        assert!(cpu.guest_text.enabled);

        cpu.write(crate::machine::FIRMWARE_TEXT_CONTROL, 1);
        assert_eq!(cpu.guest_text.cursor_row, 0);
        assert_eq!(cpu.guest_text.cursor_column, 0);
        assert!(
            cpu.guest_text
                .cells
                .iter()
                .all(|cell| cell.character == b' ')
        );
        assert!(cpu.guest_text.cells.iter().all(|cell| cell.background == 4));

        cpu.write(crate::machine::FIRMWARE_TEXT_CONTROL, 3);
        assert!(!cpu.guest_text.enabled);
    }

    #[test]
    fn console_mmio_bounds_input_and_clears_overrun_status() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.receive_console_input(&vec![b'x'; crate::console::RX_CAPACITY + 1]);
        assert_eq!(cpu.read(crate::machine::CONSOLE_STATUS), 7);
        cpu.write(crate::machine::CONSOLE_STATUS, 0x04);
        assert_eq!(cpu.read(crate::machine::CONSOLE_STATUS), 3);

        cpu.write(crate::machine::CONSOLE_CONTROL, 0x02);
        for _ in 0..crate::console::RX_CAPACITY {
            cpu.read(crate::machine::CONSOLE_DATA);
        }
        cpu.receive_console_input(b"B");
        assert_eq!(cpu.read(crate::machine::CONSOLE_DATA), b'B');
        assert_eq!(cpu.console.output(), b"");
    }

    #[test]
    fn dispatches_pending_keyboard_interrupt_after_enabling_interrupts() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0, OP_EI << 11);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD),
            0x20,
            0,
        );
        write_interrupt_control(&mut cpu, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x20);
        assert_eq!(cpu.read_u32(cpu.sp), 2);
        assert_eq!(cpu.read_u16(cpu.sp + 6), 2);
        assert_eq!(
            cpu.read_u16(cpu.sp + 12),
            u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD)
        );
        assert_eq!(cpu.read_u16(cpu.sp + 14), 1);
        assert_eq!(read_interrupt_control(&cpu), 0);
    }

    #[test]
    fn interrupt_priority_and_rti_restore_flags_pc_and_stack_for_both_sources() {
        for pending in [2u8, 8, 10] {
            let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
            cpu.write_u16(0, OP_EI << 11);
            install_test_handler(
                &mut cpu,
                u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD),
                0x300,
                0,
            );
            install_test_handler(
                &mut cpu,
                u16::from(crate::interrupts::IDT_VECTOR_RTC),
                0x400,
                0,
            );
            cpu.write_u16(0x300, OP_RTI << 11);
            cpu.write_u16(0x400, OP_RTI << 11);
            cpu.flags = Flags {
                zero: true,
                carry: false,
                negative: true,
                overflow: true,
            };
            write_interrupt_control(&mut cpu, pending);

            cpu.step().unwrap();
            let handler = if pending & 2 != 0 { 0x300 } else { 0x400 };
            assert_eq!(cpu.pc, handler);
            assert_eq!(cpu.read_u32(cpu.sp), 2);
            assert_eq!(cpu.read_u16(cpu.sp + 4), 0b1101);
            assert_eq!(cpu.read_u16(cpu.sp + 6), 2);
            assert_eq!(
                read_interrupt_control(&cpu),
                pending & !(if pending & 2 != 0 { 2 } else { 8 })
            );

            cpu.step().unwrap();
            if pending == 10 {
                assert_eq!(cpu.pc, 0x400);
                cpu.step().unwrap();
            }
            assert_eq!(cpu.pc, 2);
            assert_eq!(cpu.sp, cpu.memory_mask);
            assert_eq!(
                cpu.flags,
                Flags {
                    zero: true,
                    carry: false,
                    negative: true,
                    overflow: true,
                }
            );
        }
    }

    #[test]
    fn interrupt_gate_masks_nested_delivery_until_handler_reenables_interrupts() {
        let mut cpu = cpu_with_program(&[(OP_EI << 11)]);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD),
            0x300,
            0,
        );
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_RTC),
            0x400,
            0,
        );
        cpu.write_u16(0x300, OP_EI << 11);
        cpu.write_u16(0x400, OP_RTI << 11);
        write_interrupt_control(&mut cpu, 0x0a);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x300);
        assert_eq!(read_interrupt_control(&cpu), 8);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x400);
        assert_eq!(
            cpu.read_u16(cpu.sp + 12),
            u16::from(crate::interrupts::IDT_VECTOR_RTC)
        );
    }

    #[test]
    fn instruction_timer_interrupts_at_its_period_and_preserves_other_pending_sources() {
        let mut cpu = cpu_with_program(&[(OP_MOV << 11), (OP_MOV << 11), (OP_MOV << 11)]);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_PIT),
            0x100,
            0,
        );
        cpu.write_u16(0x100, OP_RTI << 11);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_RTC),
            0x200,
            0,
        );
        cpu.write_u16(0x200, OP_RTI << 11);
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 2)
            .unwrap();
        write_interrupt_control(&mut cpu, 1);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        write_interrupt_control(&mut cpu, 1 | 8);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.read_u32(cpu.sp), 4);
        assert_eq!(read_interrupt_control(&cpu), 8);
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 0)
            .unwrap();

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x200);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn pit_mask_latches_a_deterministic_tick_until_delivery_and_acknowledgement() {
        let mut cpu = cpu_with_program(&[(OP_MOV << 11), (OP_MOV << 11), (OP_MOV << 11)]);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_PIT),
            0x100,
            0,
        );
        cpu.write_u16(0x100, OP_RTI << 11);
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 1)
            .unwrap();
        cpu.write_guest(crate::machine::INTERRUPT_PIT_ENABLE, 0)
            .unwrap();
        write_interrupt_control(&mut cpu, 1);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        assert_eq!(read_interrupt_control(&cpu), 0x11);

        cpu.write_guest(crate::machine::INTERRUPT_PIT_ENABLE, 1)
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(read_interrupt_control(&cpu) & 0x10, 0);
        assert_eq!(cpu.read_u32(cpu.sp), 4);
        assert_eq!(
            cpu.read_u16(cpu.sp + 12),
            u16::from(crate::interrupts::IDT_VECTOR_PIT)
        );
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 0)
            .unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 4);
        assert_eq!(read_interrupt_control(&cpu) & 1, 1);
    }

    #[test]
    fn scheduler_switches_complete_user_contexts_on_timer_yield_and_exit() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0x100, OP_MOV << 11);
        cpu.write_u16(0x200, OP_MOV << 11);
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_PIT),
            0x400,
            0,
        );
        let first = cpu.create_user_task(0x100, 0x7000, 0xf000, None).unwrap();
        let second = cpu.create_user_task(0x200, 0x5000, 0xd000, None).unwrap();
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 1)
            .unwrap();
        write_interrupt_control(&mut cpu, 1);
        cpu.start_task(first).unwrap();
        cpu.registers[3] = 0x1234;
        cpu.flags = Flags {
            zero: true,
            carry: false,
            negative: true,
            overflow: false,
        };

        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), Some(second));
        assert_eq!(cpu.pc, 0x200);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.sp, 0x5000);
        cpu.registers[4] = 0x5678;
        cpu.flags.carry = true;
        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), Some(first));
        assert_eq!(cpu.pc, 0x102);
        assert_eq!(cpu.registers[3], 0x1234);
        assert!(cpu.flags.zero);
        assert!(cpu.flags.negative);
        assert_eq!(cpu.kernel_sp, 0xf000);

        assert!(cpu.yield_current_task().unwrap());
        assert_eq!(cpu.current_task(), Some(second));
        assert_eq!(cpu.registers[4], 0x5678);
        assert!(cpu.flags.carry);
        assert!(cpu.exit_current_task(7).unwrap());
        assert_eq!(cpu.current_task(), Some(first));
        assert_eq!(cpu.task_context(second).unwrap().state, TaskState::Exited);
        assert_eq!(cpu.task_context(second).unwrap().exit_code, Some(7));
    }

    #[test]
    fn timer_preemption_restores_each_tasks_mmu_address_space() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let first_directory = manager.create_address_space(&mut cpu.memory).unwrap();
        let second_directory = manager.create_address_space(&mut cpu.memory).unwrap();
        let virtual_entry = 0x1000;
        let first_code = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                first_directory,
                virtual_entry,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        let second_code = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                second_directory,
                virtual_entry,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        manager
            .allocate_and_map_page(
                &mut cpu.memory,
                first_directory,
                0x2000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        manager
            .allocate_and_map_page(
                &mut cpu.memory,
                second_directory,
                0x4000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        cpu.memory[first_code as usize * crate::mmu::PAGE_SIZE
            ..first_code as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&((OP_LDI << 11) | 0x11).to_be_bytes());
        cpu.memory[second_code as usize * crate::mmu::PAGE_SIZE
            ..second_code as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&((OP_LDI << 11) | 0x22).to_be_bytes());
        install_test_handler(
            &mut cpu,
            u16::from(crate::interrupts::IDT_VECTOR_PIT),
            0x400,
            0,
        );
        let first = cpu
            .create_user_task(virtual_entry, 0x3000, 0x7000, Some(first_directory))
            .unwrap();
        let second = cpu
            .create_user_task(virtual_entry, 0x5000, 0x9000, Some(second_directory))
            .unwrap();
        cpu.write_guest_u16(crate::machine::INTERRUPT_PIT_PERIOD, 1)
            .unwrap();
        write_interrupt_control(&mut cpu, 1);
        cpu.start_task(first).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), Some(second));
        assert_eq!(cpu.page_directory(), second_directory);
        assert_eq!(cpu.registers[0], 0);
        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), Some(first));
        assert_eq!(cpu.page_directory(), first_directory);
        assert_eq!(cpu.registers[0], 0x11);
        assert!(cpu.is_mmu_enabled());
        assert!(
            crate::mmu::translate(
                &cpu.memory,
                first_directory,
                0x4000,
                crate::mmu::MemoryAccess::Read,
                true
            )
            .is_err()
        );
        assert!(
            crate::mmu::translate(
                &cpu.memory,
                second_directory,
                0x2000,
                crate::mmu::MemoryAccess::Read,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn blocked_task_is_woken_and_restored_when_scheduler_has_no_runnable_task() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0x100, OP_MOV << 11);
        let task = cpu.create_user_task(0x100, 0x7000, 0xf000, None).unwrap();
        cpu.start_task(task).unwrap();

        assert!(!cpu.block_current_task().unwrap());
        assert!(!cpu.running);
        assert_eq!(cpu.task_context(task).unwrap().state, TaskState::Blocked);
        cpu.privilege_mode = PrivilegeMode::Supervisor;
        cpu.wake_task(task).unwrap();
        assert!(cpu.running);
        assert_eq!(cpu.current_task(), Some(task));
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    }

    #[test]
    fn mmu_executes_and_stacks_through_high_virtual_addresses() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let directory = manager.create_address_space(&mut cpu.memory).unwrap();
        let code_address = 0x2004_3000;
        let stack_pointer = 0x3000_2000;
        let code_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                code_address,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        let stack_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                stack_pointer - crate::mmu::PAGE_SIZE as u32,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        let handler_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x4000,
                crate::mmu::PagePermissions::KERNEL_CODE,
            )
            .unwrap();
        manager
            .map_borrowed_page(
                &mut cpu.memory,
                directory,
                0xff000,
                0xff,
                crate::mmu::PagePermissions::KERNEL_DATA,
            )
            .unwrap();
        cpu.memory[code_frame as usize * crate::mmu::PAGE_SIZE
            ..code_frame as usize * crate::mmu::PAGE_SIZE + 4]
            .copy_from_slice(
                &[
                    (OP_PUSH << 11).to_be_bytes(),
                    crate::isa::SVC_VECTOR.to_be_bytes(),
                ]
                .concat(),
            );
        cpu.memory[handler_frame as usize * crate::mmu::PAGE_SIZE
            ..handler_frame as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&(OP_RTI << 11).to_be_bytes());
        cpu.enable_mmu(directory).unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            crate::isa::SVC_VECTOR,
            crate::interrupts::IdtDescriptor {
                handler: 0x4000,
                present: true,
                privilege: 3,
                gate_type: crate::interrupts::GateType::Trap,
            },
        )
        .unwrap();
        cpu.registers[0] = 0x1234;
        cpu.enter_user_mode(code_address, stack_pointer).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.pc, code_address + 2);
        assert_eq!(cpu.sp, stack_pointer - 2);
        let stack_offset = stack_frame as usize * crate::mmu::PAGE_SIZE + crate::mmu::PAGE_SIZE - 2;
        assert_eq!(
            u16::from_be_bytes([cpu.memory[stack_offset], cpu.memory[stack_offset + 1]]),
            0x1234
        );

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x4000);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.read_u32(cpu.sp), code_address + 4);
        assert_eq!(cpu.read_u32(cpu.sp + 8), stack_pointer - 2);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, code_address + 4);
        assert_eq!(cpu.sp, stack_pointer - 2);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    }

    #[test]
    fn mmu_user_instruction_treats_removed_low_interrupt_alias_as_ram() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let directory = manager.create_address_space(&mut cpu.memory).unwrap();
        let code_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x1000,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x2000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        manager
            .map_borrowed_page(
                &mut cpu.memory,
                directory,
                0x43000,
                0x43,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        cpu.memory[code_frame as usize * crate::mmu::PAGE_SIZE
            ..code_frame as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&((OP_ST << 11) | (1 << 5)).to_be_bytes());
        cpu.enable_mmu(directory).unwrap();
        cpu.registers[0] = 0x5a5a;
        cpu.registers[1] = 0x43002;
        cpu.enter_user_mode(0x1000, 0x3000).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.pc, 0x1002);
        assert_eq!(cpu.read(0x43002), 0x5a);
        assert_eq!(cpu.read(crate::machine::INTERRUPT_ENABLE), 0);
        assert_eq!(cpu.read(crate::machine::INTERRUPT_PENDING), 0);
    }

    #[test]
    fn disabled_interrupts_leave_pending_sources_untouched() {
        let mut cpu = cpu_with_program(&[(OP_INC << 11) | (3 << 8)]);
        cpu.registers[3] = 7;
        write_interrupt_control(&mut cpu, 0x0a);
        cpu.step().unwrap();
        assert_eq!(cpu.registers[3], 8);
        assert_eq!(cpu.pc, 2);
        assert_eq!(read_interrupt_control(&cpu), 0x0a);
    }

    #[test]
    fn ieee_float_comparisons_and_conversions_cover_nan_zero_and_range_edges() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0, FLOAT_PREFIX);
        cpu.write_u16(2, (4 << 11) | 2);

        cpu.write_float_register(0, 32, f64::NAN);
        cpu.write_float_register(2, 32, 1.0);
        cpu.step().unwrap();
        assert!(!cpu.flags.zero);
        assert!(!cpu.flags.carry);

        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(2, (4 << 11) | 2);
        cpu.write_float_register(0, 32, -0.0);
        cpu.write_float_register(2, 32, 0.0);
        cpu.step().unwrap();
        assert!(cpu.flags.zero);
        assert!(!cpu.flags.carry);

        cpu.pc = 0;
        cpu.running = true;
        cpu.write_u16(2, (3 << 11) | 2);
        cpu.write_float_register(0, 32, 1.0);
        cpu.write_float_register(2, 32, 0.0);
        cpu.step().unwrap();
        assert_eq!(cpu.register_pair_u32(0), f32::INFINITY.to_bits());

        cpu.registers = [0; 8];
        cpu.write_float_register(2, 32, -1.0);
        cpu.execute_float(27, 0, 2);
        assert_eq!(cpu.register_pair_u32(0), u32::MAX);
        cpu.registers = [0; 8];
        cpu.write_float_register(2, 32, f64::NAN);
        cpu.execute_float(26, 0, 2);
        assert_eq!(cpu.register_pair_u32(0), 0);
    }

    #[test]
    fn rejects_out_of_range_program_segments() {
        let mut cpu = Cpu::with_memory_size(256).unwrap();
        let result = cpu.load_program(
            &[MemorySegment {
                address: 255,
                data: vec![1, 2],
            }],
            0,
        );
        assert_eq!(
            result,
            Err(CpuError::SegmentOutOfBounds {
                address: 255,
                length: 2
            })
        );
    }
}
