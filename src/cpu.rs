use memmap2::MmapMut;
use std::collections::{HashMap, HashSet};
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

pub const MEMORY_SIZE: usize = 1 << 30;
pub const ADDRESS_MASK: u32 = (MEMORY_SIZE as u32) - 1;

pub const ICR_ADDR: u32 = 0x43002;
const GPU_SPEED_ADDR: u32 = 0x43003;
pub const VSYNC_ADDR: usize = 0x43013;
const EXCEPTION_ILLEGAL_INSTRUCTION: u16 = 0;
const EXCEPTION_PRIVILEGE_VIOLATION: u16 = 1;
const EXCEPTION_PAGE_FAULT: u16 = 2;
const EXCEPTION_ENTRY_BYTES: u32 = 16;
const KEYBOARD_ADDR: u32 = 0x43000;
const KEYS_ADDR: u32 = 0x43004;
const MOUSE_ADDR: u32 = 0x43008;
const MOUSE_EVENTS_ADDR: u32 = MOUSE_ADDR + 5;
const GPU_BASE: u32 = 0x43020;
const GPU_MODE_ADDR: u32 = GPU_BASE - 12;
const GPU_LAYER_SELECT: u32 = GPU_BASE + 16;
const GPU_LAYER_MASK: u32 = GPU_BASE + 17;
const GPU_LAYER_TRANSPARENCY: u32 = GPU_BASE + 18;
const GPU_LAYER_ORDER: u32 = GPU_BASE + 23;
const GPU_LAYER_COUNT: usize = 5;
const GPU_PALETTE: u8 = 27;
const RTC_START: u32 = 0x43010;
pub const TIMER_PERIOD_ADDR: u32 = 0x4301e;
const PIT_CONTROL_ADDR: u32 = 0x43016;
const PAGE_FAULT_ADDRESS_ADDR: u32 = 0x43018;
const PAGE_FAULT_INFO_ADDR: u32 = 0x4301c;
const DISK_BASE: u32 = 0x43100;
const DISK_STATUS: u32 = DISK_BASE + 16;
const BIOS_BASE: u32 = 0x43400;
const BIOS_STATUS: u32 = BIOS_BASE + 12;
const BIOS_SETTING_RECORD: usize = BIOS_BASE as usize + 0x30;
pub const BIOS_FIRMWARE_MARKER: u32 = BIOS_BASE + 0x2f;
pub const BIOS_AUTOBOOT_WAIT_MARKER: u32 = BIOS_BASE + 0x2e;
const CONSOLE_DATA: u32 = 0x43200;
const CONSOLE_STATUS: u32 = 0x43201;
const CONSOLE_CONTROL: u32 = 0x43202;
pub const GUEST_TEXT_BASE: u32 = 0x43300;
pub const GUEST_TEXT_DATA: u32 = GUEST_TEXT_BASE;
pub const GUEST_TEXT_FOREGROUND: u32 = GUEST_TEXT_BASE + 1;
pub const GUEST_TEXT_BACKGROUND: u32 = GUEST_TEXT_BASE + 2;
pub const GUEST_TEXT_CONTROL: u32 = GUEST_TEXT_BASE + 3;
pub const GUEST_TEXT_ROW: u32 = GUEST_TEXT_BASE + 4;
pub const GUEST_TEXT_COLUMN: u32 = GUEST_TEXT_BASE + 5;
const GUEST_TEXT_CLEAR: u8 = 1;
const GUEST_TEXT_ENABLE: u8 = 2;
const GUEST_TEXT_DISABLE: u8 = 3;
const VRAM_START: u32 = 0x50000;
const GPU_CLEAR: u8 = 1;
const GPU_PIXEL: u8 = 2;
const GPU_RECT: u8 = 3;
const GPU_LINE: u8 = 4;
const GPU_SPRITE: u8 = 5;
const GPU_BITMAP: u8 = 6;
const GPU_TEXT: u8 = 7;
const GPU_PRESENT: u8 = 8;
const GPU_RANDOM: u8 = 9;
const GPU_POLY: u8 = 10;
const GPU_TICKS: u8 = 11;
const GPU_RTC: u8 = 12;
const GPU_SOUND: u8 = 13;
const GPU_NET_OPEN: u8 = 16;
const GPU_NET_CLOSE: u8 = 17;
const GPU_NET_SEND: u8 = 18;
const GPU_NET_RECV: u8 = 19;
const GPU_NET_INFO: u8 = 20;
const GPU_WORLD_EMITTERS: u8 = 34;
const GPU_TILEMAP: u8 = 21;
const GPU_3D: u8 = 22;
const GPU_SPRITE_UPLOAD: u8 = 23;
const GPU_SPRITE_MOVE: u8 = 24;
const GPU_TILE_UPLOAD: u8 = 25;
const GPU_TILE_CAMERA: u8 = 26;
const GPU_SAVE: u8 = 14;
const GPU_LOAD: u8 = 15;
const GPU_TILE_SIZE: u8 = 28;
const GPU_SPLIT_CAMERA: u8 = 29;
const GPU_MASK_OVERLAP: u8 = 31;
const GPU_WORLD_TILE: u8 = 30;
const GPU_WORLD_AMMO: u8 = 32;
const GPU_WORLD_RECT: u8 = 33;
const GPU_WORLD_PARTICLE: u8 = 35;
const GPU_TILE_CAMERA_WIDE: u8 = 36;
const GPU_SPLIT_CAMERA_WIDE: u8 = 37;
const GPU_TILEMAP_WIDE: u8 = 38;
const GPU_WORLD_TILE_WIDE: u8 = 39;
const GPU_WORLD_AMMO_WIDE: u8 = 40;
const GPU_WORLD_RECT_WIDE: u8 = 41;
const GPU_WORLD_PARTICLE_WIDE: u8 = 42;
const GPU_WORLD_EMITTERS_WIDE: u8 = 43;
const GPU_SPRITE_MOVE_WIDE: u8 = 44;
const GPU_MASK_OVERLAP_WIDE: u8 = 45;
const GPU_SPRITE_SLOTS: usize = 64;
const GPU_TILE_COUNT: usize = 256;
const GPU_TILE_MAX_SIZE: usize = 64;
const GPU_TILEMAP_MAX_SIZE: usize = 1024;
const GPU_SPRITE_MAX_SIZE: i32 = 64;
const DISK_MAX_FILE_READ_SIZE: usize = 4096;
const FOLDER_FS_LIST: u8 = 0x41;
const FOLDER_FS_READ: u8 = 0x42;
const FOLDER_FS_WRITE: u8 = 0x43;
const FOLDER_FS_CREATE_DIR: u8 = 0x44;
const FOLDER_FS_DELETE: u8 = 0x45;
const FOLDER_FS_RENAME: u8 = 0x46;
const FOLDER_FS_STAT: u8 = 0x47;
const MMIO_START: u32 = 0x43000;
const MMIO_END: u32 = 0x44000;
const GPU3D_FILL: u16 = 1;
const GPU3D_WIREFRAME: u16 = 2;
const GPU3D_SIN_8_8: [i32; 64] = [
    0, 25, 50, 74, 98, 121, 142, 162, 181, 198, 213, 226, 237, 245, 251, 255, 256, 255, 251, 245,
    237, 226, 213, 198, 181, 162, 142, 121, 98, 74, 50, 25, 0, -25, -50, -74, -98, -121, -142,
    -162, -181, -198, -213, -226, -237, -245, -251, -255, -256, -255, -251, -245, -237, -226, -213,
    -198, -181, -162, -142, -121, -98, -74, -50, -25,
];
const GPU3D_MAX_VERTICES: usize = 128;
const GPU3D_MAX_FACES: usize = 128;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuTriangle {
    pub points: [(i32, i32); 3],
    pub color: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuLine {
    pub start: (i32, i32),
    pub end: (i32, i32),
    pub color: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuSoundCommand {
    pub channel: u8,
    pub frequency: u16,
    pub frames: u16,
    pub waveform: u16,
    pub volume: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GpuSprite {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
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
    pub memory: MmapMut,
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
    pub guest_text: GuestTextDisplay,
    pub saved_video: Option<Vec<u8>>,
    pub random_state: u32,
    pub layers: [Vec<u8>; GPU_LAYER_COUNT],
    pub palette: [[u8; 3]; 256],
    pub gpu_triangles: Vec<GpuTriangle>,
    pub gpu_lines: Vec<GpuLine>,
    sound_commands: [Option<GpuSoundCommand>; 4],
    disk_root: PathBuf,
    bios_preselected_disk: Option<String>,
    bios_settings_path: Option<PathBuf>,
    bios_settings: BiosSettings,
    booted_disk_path: Option<PathBuf>,
    module_folder_disk: Option<FolderDisk>,
    folder_disk: Option<FolderDisk>,
    network: Option<NetNode>,
    network_port: u16,
    scnet: Option<ScNetNode>,
    scnet_enabled: bool,
    scnet_port: u16,
    scnet_name: Option<String>,
    last_mouse_click: [Option<(u64, u16, u16)>; 2],
    mouse_wheel_delta: i16,
    sprites: [Option<GpuSprite>; GPU_SPRITE_SLOTS],
    sprite_descriptors: [(i32, i32, u8); GPU_SPRITE_SLOTS],
    tile_memory: Vec<u8>,
    tile_pixel_size: usize,
    tilemap: Vec<u8>,
    tilemap_width: usize,
    tilemap_height: usize,
    tile_camera: (i32, i32),
    split_cameras: Option<[(i32, i32); 2]>,
    memory_mask: u32,
    layer_mode: u8,
    timer_period: u16,
    timer_countdown: u16,
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
            let image = crate::assembler::assemble(include_str!("../fonts/font5x7.asm"), 0)
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
        if size > CONSOLE_CONTROL as usize {
            memory[ICR_ADDR as usize] = 0;
            memory[CONSOLE_CONTROL as usize] = 3;
            memory[(GPU_BASE - 12) as usize] = 0;
            memory[(GPU_BASE - 13) as usize] = 2;
            memory[GPU_LAYER_MASK as usize] = (1 << GPU_LAYER_COUNT) - 1;
            for layer in 0..GPU_LAYER_COUNT {
                memory[(GPU_LAYER_ORDER as usize) + layer] = layer as u8;
            }
        }
        memory[(PIT_CONTROL_ADDR & memory_mask) as usize] = 1;
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
        let mut tile_memory = vec![0; GPU_TILE_COUNT * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE];
        for tile in 1..GPU_TILE_COUNT {
            for row in 0..16 {
                let start = tile * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE + row * 16;
                tile_memory[start..start + 16].fill(tile as u8);
            }
        }
        Ok(Self {
            memory,
            registers: [0; 8],
            pc: 0,
            sp: memory_mask,
            kernel_sp: memory_mask,
            loaded_image_segments: Vec::new(),
            privilege_mode: PrivilegeMode::Supervisor,
            flags: Flags::default(),
            running: true,
            frame_yield: false,
            frame_presented: false,
            frame_count: 0,
            console: ConsoleDevice::new(),
            guest_text: GuestTextDisplay::new(),
            saved_video: None,
            random_state: 0x16c0_ffee,
            layers: std::array::from_fn(|_| vec![0; 640 * 480]),
            palette,
            gpu_triangles: Vec::new(),
            gpu_lines: Vec::new(),
            sound_commands: [None; 4],
            disk_root: PathBuf::from("disks"),
            bios_preselected_disk: None,
            bios_settings_path: None,
            bios_settings: BiosSettings::default(),
            booted_disk_path: None,
            module_folder_disk: None,
            folder_disk: None,
            network: None,
            network_port: DEFAULT_PORT,
            scnet: None,
            scnet_enabled: false,
            scnet_port: crate::sc_net::DEFAULT_PORT,
            scnet_name: None,
            last_mouse_click: [None; 2],
            mouse_wheel_delta: 0,
            sprites: std::array::from_fn(|_| None),
            sprite_descriptors: [(0, 0, 0); GPU_SPRITE_SLOTS],
            tile_memory,
            tile_pixel_size: 16,
            tilemap: Vec::new(),
            tilemap_width: 0,
            tilemap_height: 0,
            tile_camera: (0, 0),
            split_cameras: None,
            memory_mask,
            layer_mode: 0,
            timer_period: 0,
            timer_countdown: 0,
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

    fn bios_register_u16(&self, offset: usize) -> u16 {
        u16::from_be_bytes([
            self.memory[BIOS_BASE as usize + offset],
            self.memory[BIOS_BASE as usize + offset + 1],
        ])
    }

    fn bios_register_u32(&self, offset: usize) -> u32 {
        u32::from_be_bytes([
            self.memory[BIOS_BASE as usize + offset],
            self.memory[BIOS_BASE as usize + offset + 1],
            self.memory[BIOS_BASE as usize + offset + 2],
            self.memory[BIOS_BASE as usize + offset + 3],
        ])
    }

    fn bios_status(&mut self, status: u8) {
        self.memory[BIOS_STATUS as usize] = status;
    }

    fn bios_control(&mut self, command: u8) {
        self.bios_status(0);
        if self.privilege_mode != PrivilegeMode::Supervisor {
            self.bios_status(1);
            self.console
                .log_error("bios", "user-mode firmware operation was rejected");
            return;
        }
        let result = match command {
            1 => self.bios_list_count(),
            2 => self.bios_list_id(),
            3 => self.bios_boot_disk(),
            4 => self.bios_save_settings(),
            5 => self.bios_reload_settings(),
            6 => self.bios_read_setting(),
            7 => self.bios_write_setting(),
            8 => self.bios_disk_info(),
            _ => Err("unsupported firmware operation".to_owned()),
        };
        if let Err(error) = result {
            self.bios_status(1);
            self.console.log_error("bios", &error);
        }
    }

    fn bios_disk_paths(&self) -> Result<Vec<PathBuf>, String> {
        FolderDisk::discover_paths(&self.disk_root)
    }

    fn bios_list_count(&mut self) -> Result<(), String> {
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
            self.console
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
            self.console
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
        self.memory[BIOS_BASE as usize + 2] = index_high;
        self.memory[BIOS_BASE as usize + 3] = index_low;
        self.memory[BIOS_BASE as usize + 4] = count_high;
        self.memory[BIOS_BASE as usize + 5] = count_low;
        self.memory[BIOS_BASE as usize + 6] = u8::from(auto_boot);
        self.memory[BIOS_BASE as usize + 0x0e] = u8::try_from(self.bios_settings.additional.len())
            .map_err(|_| "too many additional BIOS settings".to_owned())?;
        self.memory[BIOS_BASE as usize + 0x10..BIOS_BASE as usize + 0x21].fill(0);
        let network_name = self.bios_settings.network_name.as_bytes();
        self.memory[BIOS_BASE as usize + 0x10..BIOS_BASE as usize + 0x10 + network_name.len()]
            .copy_from_slice(network_name);
        Ok(())
    }

    fn bios_list_id(&mut self) -> Result<(), String> {
        let index = usize::from(self.bios_register_u16(2));
        let capacity = usize::from(self.bios_register_u16(10));
        let output = self.guest_range(self.bios_register_u32(6), capacity)?;
        let paths = self.bios_disk_paths()?;
        let path = paths
            .get(index)
            .ok_or_else(|| format!("firmware disk index {index} is out of range"))?;
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "firmware disk ID is not valid text".to_owned())?;
        let encoded = id.as_bytes();
        let required = encoded.len() + 1;
        if required > capacity {
            return Err(format!(
                "firmware disk ID output buffer at {:#x} has capacity {capacity}; {required} required",
                output.start
            ));
        }
        let start = output.start;
        self.memory[start..start + encoded.len()].copy_from_slice(encoded);
        self.memory[start + encoded.len()] = 0;
        Ok(())
    }

    fn bios_disk_info(&mut self) -> Result<(), String> {
        let index = usize::from(self.bios_register_u16(2));
        let capacity = usize::from(self.bios_register_u16(10));
        let output = self.guest_range(self.bios_register_u32(6), capacity)?;
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
        if summary.len() + 1 > capacity {
            return Err(format!(
                "firmware disk information buffer has capacity {capacity}; {} required",
                summary.len() + 1
            ));
        }
        let start = output.start;
        self.memory[start..start + summary.len()].copy_from_slice(summary.as_bytes());
        self.memory[start + summary.len()] = 0;
        Ok(())
    }

    fn bios_boot_disk(&mut self) -> Result<(), String> {
        if self.scnet_enabled {
            self.ensure_scnet_node(std::time::Duration::from_millis(650))?;
        }
        let index = usize::from(self.bios_register_u16(2));
        let paths = self.bios_disk_paths()?;
        let path = paths
            .get(index)
            .ok_or_else(|| format!("firmware disk index {index} is out of range"))?;
        let disk = FolderDisk::open(path)?;
        let boot = disk.load_boot_image()?;
        for segment in &boot.image.segments {
            let start = usize::try_from(segment.address)
                .map_err(|_| "boot image segment address exceeds host memory".to_owned())?;
            let end = start
                .checked_add(segment.data.len())
                .ok_or_else(|| "boot image segment range overflow".to_owned())?;
            if end > self.memory.len() {
                return Err("boot image segment exceeds configured memory".to_owned());
            }
        }
        if boot.image.entry_point as usize >= self.memory.len() {
            return Err("boot image entry point exceeds configured memory".to_owned());
        }
        self.prepare_image_entry();
        self.load_program(&boot.image.segments, boot.image.entry_point)
            .map_err(|error| format!("cannot load selected firmware disk: {error}"))?;
        self.memory[BIOS_FIRMWARE_MARKER as usize] = 0;
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
        let mode = match boot.mode {
            FolderBootMode::Kernel => "KERNEL",
            FolderBootMode::User => "USER",
        };
        self.console.log_info(
            "bios",
            &format!("Booting {} in {mode} mode", path.display()),
        );
        Ok(())
    }

    fn bios_save_settings(&mut self) -> Result<(), String> {
        let path = self
            .bios_settings_path
            .as_ref()
            .ok_or_else(|| "BIOS settings file path is not configured".to_owned())?;
        let paths = self.bios_disk_paths()?;
        let selected = usize::from(self.bios_register_u16(2));
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
        let startup_behavior = match self.memory[BIOS_BASE as usize + 6] {
            0 => StartupBehavior::Selector,
            1 => StartupBehavior::BootPreferred,
            _ => return Err("BIOS startup behavior value is invalid".to_owned()),
        };
        let mut network_name = Vec::with_capacity(16);
        for byte in &self.memory[BIOS_BASE as usize + 0x10..BIOS_BASE as usize + 0x20] {
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

    fn bios_reload_settings(&mut self) -> Result<(), String> {
        let path = self
            .bios_settings_path
            .as_ref()
            .ok_or_else(|| "BIOS settings file path is not configured".to_owned())?;
        self.bios_settings = BiosSettings::load(path)?;
        self.bios_list_count()
    }

    fn bios_read_setting(&mut self) -> Result<(), String> {
        let index = usize::from(self.bios_register_u16(2));
        let (name, value) = self
            .bios_settings
            .additional
            .iter()
            .nth(index)
            .ok_or_else(|| format!("additional BIOS setting index {index} is out of range"))?;
        let record = BIOS_SETTING_RECORD;
        self.memory[record..record + 0x2a].fill(0);
        self.memory[record..record + name.len()].copy_from_slice(name.as_bytes());
        self.memory[record + 17] = match value {
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
        self.memory[record + 18..record + 18 + encoded_value.len()]
            .copy_from_slice(encoded_value.as_bytes());
        Ok(())
    }

    fn bios_write_setting(&mut self) -> Result<(), String> {
        let record = BIOS_SETTING_RECORD;
        let name_bytes = &self.memory[record..record + 17];
        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "additional BIOS setting name is not terminated".to_owned())?;
        let name = std::str::from_utf8(&name_bytes[..name_end])
            .map_err(|_| "additional BIOS setting name is not valid UTF-8".to_owned())?;
        let value_bytes = &self.memory[record + 18..record + 0x2a];
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
        let new_value = match (self.memory[record + 17], existing) {
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

    pub fn load_program<S: LoadableSegment>(
        &mut self,
        segments: &[S],
        entry_point: u32,
    ) -> Result<(), CpuError> {
        let mut loaded_image_segments = Vec::new();
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
            if end > self.memory.len() {
                return Err(CpuError::SegmentOutOfBounds {
                    address,
                    length: data.len(),
                });
            }
            self.memory[start..end].copy_from_slice(data);
            if let Some((alignment, permissions)) = segment.image_metadata() {
                loaded_image_segments.push(LoadedSegmentMetadata {
                    address,
                    length: data.len(),
                    alignment,
                    permissions,
                });
            }
        }
        if entry_point as usize >= self.memory.len() {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        self.pc = entry_point;
        self.loaded_image_segments = loaded_image_segments;
        Ok(())
    }

    pub fn prepare_image_entry(&mut self) {
        self.registers.fill(0);
        self.flags = Flags::default();
        self.sp = self.memory_mask;
        self.kernel_sp = self.memory_mask;
        self.loaded_image_segments.clear();
        self.privilege_mode = PrivilegeMode::Supervisor;
        self.mmu_enabled = false;
        self.page_directory = 0;
        self.last_page_fault = None;
        self.idt_base = None;
        self.scheduler = Scheduler::default();
        self.syscall_state = KernelState::default();
        self.page_memory_manager = None;
        self.running = true;
        self.booted_disk_path = None;
        if let Some(interrupt_control) = self.memory.get_mut(ICR_ADDR as usize) {
            *interrupt_control = 0;
        }
    }

    pub fn initial_user_stack_pointer(&self) -> u32 {
        self.memory_mask
            .saturating_sub((self.memory.len() / 4).min(0x1_0000) as u32)
    }

    pub fn enter_user_mode(&mut self, user_pc: u32, user_sp: u32) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::Supervisor {
            return Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: "enter user mode",
            });
        }
        if user_pc > self.guest_address_mask() {
            return Err(CpuError::InvalidEntryPoint(user_pc));
        }
        if user_sp == 0 || user_sp > self.guest_address_mask() {
            return Err(CpuError::InvalidUserStack(user_sp));
        }
        if !self.mmu_enabled && user_sp > self.kernel_sp.saturating_sub(0x1000) {
            return Err(CpuError::InvalidUserStack(user_sp));
        }
        if !self.loaded_image_segments.is_empty()
            && !self.loaded_image_segments.iter().any(|segment| {
                segment.permissions.execute
                    && user_pc >= segment.address
                    && ((user_pc - segment.address) as usize) < segment.length
            })
        {
            return Err(CpuError::InvalidEntryPoint(user_pc));
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
            || (!self.loaded_image_segments.is_empty()
                && !self.loaded_image_segments.iter().any(|segment| {
                    segment.permissions.execute
                        && entry_point >= segment.address
                        && ((entry_point - segment.address) as usize) < segment.length
                }))
        {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        if user_sp == 0
            || user_sp
                > page_directory.map_or(self.memory_mask, |_| crate::mmu::ADDRESS_SPACE_SIZE - 1)
            || kernel_sp as usize >= self.memory.len()
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
            interrupt_enabled: self.memory[(ICR_ADDR & self.memory_mask) as usize] & 1 != 0,
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
        let interrupt_control = (self.read(ICR_ADDR) & !1) | u8::from(context.interrupt_enabled);
        self.write(ICR_ADDR, interrupt_control);
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
        if descriptor.handler > self.guest_address_mask() {
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
        if page_directory & 0x7ff != 0 || end as usize > self.memory.len() {
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
        if self.mmu_enabled {
            crate::mmu::ADDRESS_SPACE_SIZE - 1
        } else {
            self.memory_mask
        }
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
        let address = address & self.memory_mask;
        match address {
            CONSOLE_DATA => {
                let value = self.console.read_data();
                if self.memory[CONSOLE_CONTROL as usize] & 1 != 0 && value != 0 {
                    print!("{}", char::from(value));
                }
                value
            }
            CONSOLE_STATUS => {
                if self.console.receive_buffer().is_empty() && self.running {
                    self.frame_yield = true;
                }
                self.console.status()
            }
            CONSOLE_CONTROL => self.console.control(),
            KEYBOARD_ADDR => {
                let key = self.memory[address as usize];
                self.memory[address as usize] = 0;
                key
            }
            MOUSE_EVENTS_ADDR => {
                let events = self.memory[address as usize];
                self.memory[address as usize] = 0;
                events
            }
            _ => self.memory[address as usize],
        }
    }

    pub fn configure_disk(
        &mut self,
        storage_dir: PathBuf,
        module_file_root: PathBuf,
    ) -> Result<(), String> {
        self.disk_root = storage_dir;
        self.module_folder_disk = if module_file_root.exists() {
            Some(FolderDisk::open_read_only_directory(&module_file_root)?)
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
        self.memory[DISK_STATUS as usize] = 0;
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

    fn disk_register_u16(&self, offset: usize) -> u16 {
        u16::from_be_bytes([
            self.memory[DISK_BASE as usize + offset],
            self.memory[DISK_BASE as usize + offset + 1],
        ])
    }

    fn disk_register_u32(&self, offset: usize) -> u32 {
        u32::from_be_bytes([
            self.memory[DISK_BASE as usize + offset],
            self.memory[DISK_BASE as usize + offset + 1],
            self.memory[DISK_BASE as usize + offset + 2],
            self.memory[DISK_BASE as usize + offset + 3],
        ])
    }

    fn disk_write_result(&mut self, result: u16) {
        let [high, low] = result.to_be_bytes();
        self.memory[DISK_BASE as usize + 14] = high;
        self.memory[DISK_BASE as usize + 15] = low;
    }

    fn disk_control(&mut self, command: u8) {
        let caller_mode = self.privilege_mode;
        if !(command == 0x80 || (FOLDER_FS_LIST..=FOLDER_FS_STAT).contains(&command)) {
            self.memory[DISK_STATUS as usize] = 1;
            self.console
                .log_error("disk", "unsupported disk operation was rejected");
            return;
        }
        self.privilege_mode = PrivilegeMode::Supervisor;
        self.memory[DISK_STATUS as usize] = 0;
        if let Err(error) = self.disk_control_inner(command, caller_mode == PrivilegeMode::User) {
            self.memory[DISK_STATUS as usize] = 1;
            self.console
                .log_error("disk", &format!("operation failed: {error}"));
        }
        self.privilege_mode = caller_mode;
    }

    fn disk_control_inner(&mut self, command: u8, user_mode: bool) -> Result<(), String> {
        match command {
            0x80 => self.legacy_file_list(user_mode),
            FOLDER_FS_LIST..=FOLDER_FS_STAT => self.folder_fs_control(command, user_mode),
            _ => Err("unknown disk service operation".to_owned()),
        }
    }

    fn legacy_file_list(&mut self, user_mode: bool) -> Result<(), String> {
        let index = usize::from(self.disk_register_u16(10));
        let capacity = usize::from(self.disk_register_u16(30));
        let disk = self
            .folder_disk
            .as_ref()
            .or(self.module_folder_disk.as_ref())
            .ok_or_else(|| "no folder filesystem is mounted".to_owned())?;
        let files = disk.list_files()?;
        let name = files.get(index).map(String::as_str).unwrap_or("");
        let encoded = name.as_bytes();
        let length = encoded.len() + 1;
        if length > capacity {
            return Err("virtual file name buffer is too small".to_owned());
        }
        let output = self.guest_memory_addresses(
            self.disk_register_u32(26),
            length,
            crate::mmu::MemoryAccess::Write,
            user_mode,
        )?;
        for (address, byte) in output.into_iter().zip(encoded.iter().copied().chain([0])) {
            self.memory[address] = byte;
        }
        self.disk_write_result(u16::try_from(files.len()).unwrap_or(u16::MAX));
        Ok(())
    }

    fn folder_fs_control(&mut self, command: u8, user_mode: bool) -> Result<(), String> {
        let disk = self
            .folder_disk
            .as_ref()
            .or(self.module_folder_disk.as_ref())
            .ok_or_else(|| "no folder filesystem is mounted".to_owned())?;
        match command {
            FOLDER_FS_LIST => {
                let directory = self.read_guest_path(self.disk_register_u32(22), user_mode)?;
                let entries = disk.list_dir(directory.as_deref())?;
                let index = usize::from(self.disk_register_u16(10));
                let encoded = if let Some(entry) = entries.get(index) {
                    let size = u32::try_from(entry.size_bytes)
                        .map_err(|_| "folder filesystem entry exceeds 32-bit size".to_owned())?;
                    let mut encoded = entry.name.as_bytes().to_vec();
                    encoded.push(0);
                    encoded.push(u8::from(entry.is_directory));
                    encoded.extend_from_slice(&size.to_be_bytes());
                    encoded
                } else {
                    vec![0]
                };
                let capacity = usize::from(self.disk_register_u16(30));
                if encoded.len() > capacity {
                    return Err("folder filesystem listing buffer is too small".to_owned());
                }
                let output = self.guest_memory_addresses(
                    self.disk_register_u32(26),
                    encoded.len(),
                    crate::mmu::MemoryAccess::Write,
                    user_mode,
                )?;
                for (address, byte) in output.into_iter().zip(encoded) {
                    self.memory[address] = byte;
                }
                self.disk_write_result(u16::try_from(entries.len()).unwrap_or(u16::MAX));
            }
            FOLDER_FS_READ => {
                let path = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem path is empty".to_owned())?;
                let length = usize::from(self.disk_register_u16(30));
                if length > DISK_MAX_FILE_READ_SIZE {
                    return Err("folder filesystem transfer exceeds 4096 bytes".to_owned());
                }
                let output = self.guest_memory_addresses(
                    self.disk_register_u32(26),
                    length,
                    crate::mmu::MemoryAccess::Write,
                    user_mode,
                )?;
                let offset = u64::from(self.disk_register_u32(18));
                let (data, file_size) = disk.read_file(&path, offset, length)?;
                let file_size = u32::try_from(file_size)
                    .map_err(|_| "folder filesystem file exceeds 32-bit size".to_owned())?;
                let bytes_read = data.len();
                for (address, byte) in output.into_iter().zip(data) {
                    self.memory[address] = byte;
                }
                self.memory[DISK_BASE as usize + 32..DISK_BASE as usize + 36]
                    .copy_from_slice(&file_size.to_be_bytes());
                self.disk_write_result(u16::try_from(bytes_read).unwrap_or(u16::MAX));
            }
            FOLDER_FS_WRITE => {
                let path = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem path is empty".to_owned())?;
                Self::validate_guest_mutation_path(&path)?;
                let length = usize::from(self.disk_register_u16(30));
                if length > DISK_MAX_FILE_READ_SIZE {
                    return Err("folder filesystem transfer exceeds 4096 bytes".to_owned());
                }
                let input = self.guest_memory_addresses(
                    self.disk_register_u32(26),
                    length,
                    crate::mmu::MemoryAccess::Read,
                    user_mode,
                )?;
                let data = input
                    .into_iter()
                    .map(|address| self.memory[address])
                    .collect::<Vec<_>>();
                disk.write_file(&path, &data)?;
                self.disk_write_result(u16::try_from(length).unwrap_or(u16::MAX));
            }
            FOLDER_FS_CREATE_DIR => {
                let path = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem path is empty".to_owned())?;
                Self::validate_guest_mutation_path(&path)?;
                disk.create_dir(&path)?;
                self.disk_write_result(0);
            }
            FOLDER_FS_DELETE => {
                let path = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem path is empty".to_owned())?;
                Self::validate_guest_mutation_path(&path)?;
                disk.delete_entry(&path)?;
                self.disk_write_result(0);
            }
            FOLDER_FS_RENAME => {
                let source = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem source path is empty".to_owned())?;
                let destination = self
                    .read_guest_path(self.disk_register_u32(18), user_mode)?
                    .ok_or_else(|| "folder filesystem destination path is empty".to_owned())?;
                Self::validate_guest_mutation_path(&source)?;
                Self::validate_guest_mutation_path(&destination)?;
                disk.rename_entry(&source, &destination)?;
                self.disk_write_result(0);
            }
            FOLDER_FS_STAT => {
                let path = self
                    .read_guest_path(self.disk_register_u32(22), user_mode)?
                    .ok_or_else(|| "folder filesystem path is empty".to_owned())?;
                let entry = disk.metadata(&path)?;
                let size = u32::try_from(entry.size_bytes)
                    .map_err(|_| "folder filesystem entry exceeds 32-bit size".to_owned())?;
                let output = self.guest_memory_addresses(
                    self.disk_register_u32(26),
                    5,
                    crate::mmu::MemoryAccess::Write,
                    user_mode,
                )?;
                for (address, byte) in output.into_iter().zip([
                    u8::from(entry.is_directory),
                    size.to_be_bytes()[0],
                    size.to_be_bytes()[1],
                    size.to_be_bytes()[2],
                    size.to_be_bytes()[3],
                ]) {
                    self.memory[address] = byte;
                }
                self.disk_write_result(5);
            }
            _ => return Err("unknown folder filesystem operation".to_owned()),
        }
        Ok(())
    }

    fn validate_guest_mutation_path(path: &str) -> Result<(), String> {
        if matches!(path, "main.bin" | "kernel.bin") {
            return Err("boot executable paths cannot be changed by guest code".to_owned());
        }
        Ok(())
    }

    fn guest_range(&self, address: u32, length: usize) -> Result<std::ops::Range<usize>, String> {
        let start = usize::try_from(address)
            .map_err(|_| "guest buffer address is outside memory".to_owned())?;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= self.memory.len())
            .ok_or_else(|| "guest buffer range is outside memory".to_owned())?;
        if address < MMIO_END && end > MMIO_START as usize {
            return Err("guest buffer overlaps memory-mapped devices".to_owned());
        }
        Ok(start..end)
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
            if self.guest_range_contains(virtual_address, MMIO_START, MMIO_END - MMIO_START) {
                return Err("guest buffer overlaps memory-mapped devices".to_owned());
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
            if self.guest_range_contains(physical_address, MMIO_START, MMIO_END - MMIO_START)
                || physical_address as usize >= self.memory.len()
            {
                return Err("guest buffer is outside accessible RAM".to_owned());
            }
            addresses.push(physical_address as usize);
        }
        Ok(addresses)
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

    fn dispatch_user_syscall(&mut self) -> Result<(), CpuError> {
        let version = self.registers[crate::syscall::REGISTER_ABI_VERSION];
        let number = self.registers[crate::syscall::REGISTER_NUMBER];
        let task_id = self.scheduler.current_task();
        if let Some(id) = task_id {
            self.syscall_state.register_process(id, None);
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
                Ok(SyscallNumber::Spawn) => self.syscall_spawn(task_id),
                Ok(SyscallNumber::Mmap) => self.syscall_mmap(task_id),
                Ok(SyscallNumber::Sleep) => self.syscall_sleep(task_id, &mut scheduled_state),
                Ok(SyscallNumber::Wait) => self.syscall_wait(task_id, &mut scheduled_state),
                Ok(SyscallNumber::Semaphore) => {
                    self.syscall_semaphore(task_id, &mut scheduled_state)
                }
                Ok(SyscallNumber::Seek) => self.syscall_seek(task_id),
                Ok(SyscallNumber::ScNet) => self.syscall_scnet(task_id),
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
            .or(self.module_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
        if flags & crate::syscall::open_flags::WRITE != 0 && disk.manifest().read_only {
            return crate::syscall::error::PERMISSION;
        }
        if disk
            .ensure_regular_file(&path, flags & crate::syscall::open_flags::CREATE != 0)
            .is_err()
        {
            return crate::syscall::error::NO_ENTRY;
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
                self.memory[*address] = self.console.read_data();
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
            .or(self.module_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
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
            for byte in data {
                self.console.write_data(byte);
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
            .or(self.module_folder_disk.as_ref())
        else {
            return crate::syscall::error::IO;
        };
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
                    .or(self.module_folder_disk.as_ref())
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

    fn ensure_page_memory_manager(&mut self) -> Result<(), crate::mmu::PageTableError> {
        if self.page_memory_manager.is_some() {
            return Ok(());
        }
        let mut manager = crate::mmu::PageMemoryManager::new(self.memory.len(), 0x8000)?;
        if self.memory.len() > MMIO_START as usize {
            manager
                .reserve_physical_range(MMIO_START as usize, (MMIO_END - MMIO_START) as usize)?;
        }
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

    fn syscall_spawn(&mut self, parent: Option<TaskId>) -> i64 {
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
        let Some(disk) = self.folder_disk.as_ref() else {
            return crate::syscall::error::IO;
        };
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
                let segment_end = segment.address as usize + segment.data.len();
                if (segment.address as usize) < MMIO_END as usize
                    && segment_end > MMIO_START as usize
                {
                    result = Err(crate::mmu::PageTableError::InvalidVirtualAddress);
                    break;
                }
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
        let user_stack_top = crate::mmu::ADDRESS_SPACE_SIZE - 0x1_0000;
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
        self.syscall_state.register_process(id, Some(parent));
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
            Some(end) if end <= crate::mmu::ADDRESS_SPACE_SIZE => end,
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

    fn syscall_wait(
        &mut self,
        task_id: Option<TaskId>,
        scheduled_state: &mut Option<(TaskState, i32)>,
    ) -> i64 {
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
        self.syscall_state.waiting_for.remove(&id);
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

    fn gpu_network(&mut self, command: u8, query: u8, argument: i16) {
        if self.network.is_none() {
            self.network = Some(NetNode::new(self.network_port));
        }
        let mut node = self.network.take().expect("network node initialized");
        node.poll();
        let source = self.gpu_source();
        match command {
            GPU_NET_OPEN => {
                let mut title = Vec::with_capacity(16);
                for offset in 0..16u32 {
                    let byte = self.memory
                        [source.wrapping_add(offset) as usize & self.memory_mask as usize];
                    if byte == 0 {
                        break;
                    }
                    title.push(byte);
                }
                match node.open(&title) {
                    Ok(opened) => self.set_gpu_result(u16::from(opened)),
                    Err(error) => {
                        self.console
                            .log_error("network", &format!("open failed: {error}"));
                        self.set_gpu_result(0);
                    }
                }
            }
            GPU_NET_CLOSE => node.close(),
            GPU_NET_SEND => {
                let length = usize::try_from(argument.max(0)).unwrap_or_default().min(48);
                let data = (0..length)
                    .map(|offset| {
                        self.memory[source.wrapping_add(offset as u32) as usize
                            & self.memory_mask as usize]
                    })
                    .collect::<Vec<_>>();
                self.set_gpu_result(u16::from(node.send(&data)));
            }
            GPU_NET_RECV => {
                if let Some((_, data)) = node.receive() {
                    for (offset, byte) in data.iter().copied().enumerate() {
                        self.memory[source.wrapping_add(offset as u32) as usize
                            & self.memory_mask as usize] = byte;
                    }
                    self.set_gpu_result(data.len() as u16);
                } else {
                    self.set_gpu_result(0);
                }
            }
            GPU_NET_INFO => {
                let value = match query {
                    0 => node.players(),
                    1 => node.slot() as u16,
                    2 => u16::from(argument >= 0 && node.active(argument as u16)),
                    3 => u16::from(node.is_full()),
                    4 => node.last_sender(),
                    5 => u16::from(node.ready()),
                    6 => u16::from(node.is_open()),
                    _ => 0,
                };
                self.set_gpu_result(value);
            }
            _ => {}
        }
        self.network = Some(node);
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let address = address & self.memory_mask;
        let index = address as usize;
        self.memory[index] = value;
        let timer_period_address = TIMER_PERIOD_ADDR & self.memory_mask;
        if address == timer_period_address
            || address == (timer_period_address.wrapping_add(1) & self.memory_mask)
        {
            self.timer_period = self.read_u16(TIMER_PERIOD_ADDR);
            self.timer_countdown = self.timer_period;
        }
        match address {
            GPU_BASE => self.gpu_command(value),
            DISK_BASE => self.disk_control(value),
            BIOS_BASE => self.bios_control(value),
            GPU_MODE_ADDR => {
                if self.layer_mode != value {
                    self.layer_mode = value;
                    for layer in &mut self.layers {
                        layer.fill(0);
                    }
                }
            }
            CONSOLE_DATA => {
                self.console.write_data(value);
                print!("{}", char::from(value));
                if value == b'\n' {
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            }
            CONSOLE_STATUS if value & 0x04 != 0 => self.console.clear_overrun(),
            CONSOLE_CONTROL => {
                self.console.set_control(value);
                self.memory[index] = self.console.control();
            }
            GUEST_TEXT_DATA => self.guest_text.write_byte(
                value,
                self.guest_text.foreground,
                self.guest_text.background,
            ),
            GUEST_TEXT_FOREGROUND => self.guest_text.foreground = value,
            GUEST_TEXT_BACKGROUND => self.guest_text.background = value,
            GUEST_TEXT_CONTROL => match value {
                GUEST_TEXT_CLEAR => self.guest_text.clear(self.guest_text.background),
                GUEST_TEXT_ENABLE => self.guest_text.enabled = true,
                GUEST_TEXT_DISABLE => self.guest_text.enabled = false,
                _ => {}
            },
            GUEST_TEXT_ROW | GUEST_TEXT_COLUMN => {
                let row = usize::from(self.memory[GUEST_TEXT_ROW as usize]);
                let column = usize::from(self.memory[GUEST_TEXT_COLUMN as usize]);
                self.guest_text.set_cursor(row, column);
            }
            _ => {}
        }
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

    fn write_u32(&mut self, address: u32, value: u32) {
        for offset in 0..4 {
            self.write(
                address.wrapping_add(offset),
                (value >> (24 - offset * 8)) as u8,
            );
        }
    }

    fn wrapped_range_contains(&self, address: u32, base: u32, length: u32) -> bool {
        let address = address & self.memory_mask;
        let start = base & self.memory_mask;
        let end = base.wrapping_add(length - 1) & self.memory_mask;
        if start <= end {
            (start..=end).contains(&address)
        } else {
            address >= start || address <= end
        }
    }

    fn guest_range_contains(&self, address: u32, base: u32, length: u32) -> bool {
        if self.mmu_enabled {
            address >= base && address < base.saturating_add(length)
        } else {
            self.wrapped_range_contains(address, base, length)
        }
    }

    fn check_guest_access(
        &self,
        address: u32,
        access: crate::mmu::MemoryAccess,
    ) -> Result<(), CpuError> {
        if self.privilege_mode != PrivilegeMode::User
            || !self.guest_range_contains(address, MMIO_START, MMIO_END - MMIO_START)
        {
            return Ok(());
        }
        let write = access == crate::mmu::MemoryAccess::Write;
        let user_service = access != crate::mmu::MemoryAccess::Fetch
            && (self.guest_range_contains(address, GPU_SPEED_ADDR, 1)
                || self.guest_range_contains(address, VSYNC_ADDR as u32, 1)
                || self.guest_range_contains(address, GPU_MODE_ADDR, 1)
                || self.guest_range_contains(address, DISK_BASE, 0x40)
                || self.guest_range_contains(address, GPU_BASE, 0x80)
                || self.guest_range_contains(address, CONSOLE_DATA, 3)
                || self.guest_range_contains(address, GUEST_TEXT_BASE, 6)
                || (!write
                    && (self.guest_range_contains(address, KEYBOARD_ADDR, 1)
                        || self.guest_range_contains(address, KEYS_ADDR, 3)
                        || self.guest_range_contains(address, MOUSE_ADDR, 8)
                        || self.guest_range_contains(address, RTC_START, 4))));
        if user_service {
            Ok(())
        } else {
            Err(CpuError::PrivilegeViolation {
                pc: self.pc,
                operation: match access {
                    crate::mmu::MemoryAccess::Fetch => "MMIO instruction fetch",
                    crate::mmu::MemoryAccess::Read => "MMIO read",
                    crate::mmu::MemoryAccess::Write => "MMIO write",
                },
            })
        }
    }

    fn resolve_guest_range(
        &self,
        address: u32,
        length: u32,
        access: crate::mmu::MemoryAccess,
    ) -> Result<Vec<u32>, CpuError> {
        let mut physical_addresses = Vec::with_capacity(length as usize);
        for offset in 0..length {
            let virtual_address = if self.mmu_enabled {
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
                virtual_address
            } else {
                address.wrapping_add(offset) & self.memory_mask
            };
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
                virtual_address & self.memory_mask
            };
            self.check_guest_access(physical_address, access)?;
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
        self.last_page_fault = Some(fault);
        self.write_u32(PAGE_FAULT_ADDRESS_ADDR, fault.virtual_address);
        self.write_u16(PAGE_FAULT_INFO_ADDR, (access << 8) | reason);
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
            let address = self.registers[source] as u32 & self.guest_address_mask();
            self.registers[destination] = i64::from(self.read_guest_u32(address)? & ADDRESS_MASK);
            return Ok(());
        }
        if operation == 9 {
            let address = self.registers[source] as u32 & self.guest_address_mask();
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
            let address = self.register_pair_u32(source) & self.guest_address_mask();
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
            2 => self.pc = u32::from(value) & self.guest_address_mask(),
            3 => {
                self.push_u32(self.pc)?;
                self.pc = u32::from(value) & self.guest_address_mask();
            }
            4 => self.push_u16(self.pack_flags())?,
            5 => {
                let flags = self.pop_u16()?;
                self.restore_flags(flags);
            }
            6 => self.flags.carry = true,
            7 => self.flags.carry = false,
            8 => self.sp = u32::from(value) & self.guest_address_mask(),
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
                self.pc = target & self.guest_address_mask();
            }
            13 => {
                self.sp = (self.sp & 0xffff) | (u32::from(value & 0x3fff) << 16);
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
        let saved_pc = self.pc;
        let saved_registers = self.registers;
        let saved_sp = self.sp;
        let saved_kernel_sp = self.kernel_sp;
        let saved_flags = self.flags;
        let saved_mode = self.privilege_mode;
        let saved_running = self.running;
        match self.execute_instruction() {
            Err(CpuError::PrivilegeViolation { .. }) => {
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
        }
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
            OP_HALT if instruction == 0 => self.running = false,
            OP_HALT if instruction == crate::isa::SVC_VECTOR => {
                let vector = crate::isa::SVC_VECTOR;
                let descriptor = self.idt_descriptor(vector)?;
                let caller_privilege = u8::from(self.privilege_mode == PrivilegeMode::User) * 3;
                let is_syscall = self.privilege_mode == PrivilegeMode::User
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
                let address = self.registers[rs1] as u32 & self.guest_address_mask();
                self.registers[rd] = i64::from(self.read_guest(address)?);
            }
            OP_ST => {
                let address = self.registers[rs1] as u32 & self.guest_address_mask();
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
                let value = self.read(ICR_ADDR) | 1;
                self.write(ICR_ADDR, value);
            }
            OP_DI => {
                if self.privilege_mode != PrivilegeMode::Supervisor {
                    return Err(CpuError::PrivilegeViolation {
                        pc: instruction_pc,
                        operation: "DI",
                    });
                }
                let value = self.read(ICR_ADDR) & !1;
                self.write(ICR_ADDR, value);
            }
            OP_RTI => self.return_from_exception()?,
            OP_PUSH => self.push_u16(self.registers[rd] as u16)?,
            OP_POP => self.registers[rd] = i64::from(self.pop_u16()?),
            OP_CALL => {
                self.push_u32(self.pc)?;
                self.pc = (instruction & 0x7ff) as u32;
            }
            OP_RET => self.pc = self.pop_u32()? & self.guest_address_mask(),
            OP_BZ => {
                if self.flags.zero {
                    self.pc = (instruction & 0x7ff) as u32;
                }
            }
            OP_LDW => {
                let address = self.registers[rs1] as u32 & self.guest_address_mask();
                self.registers[rd] = i64::from(self.read_guest_u16(address)?);
            }
            OP_STW => {
                let address = self.registers[rs1] as u32 & self.guest_address_mask();
                self.write_guest_u16(address, self.registers[rd] as u16)?;
            }
            OP_LDWS => {
                self.registers[rd] =
                    i64::from(self.read_guest_u16(
                        self.sp.wrapping_add(immediate) & self.guest_address_mask(),
                    )?)
            }
            OP_STWS => self.write_guest_u16(
                self.sp.wrapping_add(immediate) & self.guest_address_mask(),
                self.registers[rd] as u16,
            )?,
            OP_ADJSP => {
                let offset = (immediate as u8 as i8) as i32;
                self.sp = self.sp.wrapping_add_signed(offset) & self.guest_address_mask();
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
                    self.pc = target & self.guest_address_mask();
                }
            }
            OP_SHR if instruction & FAR_CONTROL_MASK == FAR_CONTROL_PREFIX => {
                let operation = (instruction >> 9) & 3;
                let target = (((self.fetch_guest_u16(self.pc)? & 0x3fff) as u32) << 16)
                    | self.fetch_guest_u16(self.pc.wrapping_add(2))? as u32;
                self.pc = self.pc.wrapping_add(4) & self.guest_address_mask();
                match operation {
                    0 => self.pc = target & self.guest_address_mask(),
                    1 => {
                        self.push_u32(self.pc)?;
                        self.pc = target & self.guest_address_mask();
                    }
                    2 if self.flags.zero => self.pc = target & self.guest_address_mask(),
                    3 if self.flags.carry => self.pc = target & self.guest_address_mask(),
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
        if self.read(ICR_ADDR) & 1 != 0 {
            self.check_interrupts()?;
        }
        Ok(())
    }

    pub fn run(&mut self, instruction_budget: usize) -> Result<usize, CpuError> {
        let mut executed = 0;
        while executed < instruction_budget
            && !self.frame_yield
            && (self.running || !self.syscall_state.sleeping.is_empty())
        {
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
        self.write(KEYBOARD_ADDR, keycode);
        let icr = self.read(ICR_ADDR) | 2;
        self.write(ICR_ADDR, icr);
    }

    pub fn set_key(&mut self, mask: u32, pressed: bool) {
        for offset in 0..3 {
            let address = KEYS_ADDR + offset;
            let shift = offset * 8;
            let bit = ((mask >> shift) & 0xff) as u8;
            if pressed {
                self.memory[address as usize] |= bit;
            } else {
                self.memory[address as usize] &= !bit;
            }
        }
    }

    pub fn set_mouse_position(&mut self, x: u16, y: u16) {
        let (width, height) = if self.memory[GPU_MODE_ADDR as usize] == 1 {
            (640, 480)
        } else {
            (320, 240)
        };
        let x = x.min(width - 1);
        let y = y.min(height - 1);
        let [x_high, x_low] = x.to_be_bytes();
        let [y_high, y_low] = y.to_be_bytes();
        self.memory[MOUSE_ADDR as usize..MOUSE_ADDR as usize + 4]
            .copy_from_slice(&[x_high, x_low, y_high, y_low]);
    }

    pub fn mouse_button(&mut self, button: usize, down: bool, x: u16, y: u16, now_ms: u64) {
        if button > 1 {
            return;
        }
        let held_index = MOUSE_ADDR as usize + 4;
        let event_index = held_index + 1;
        let bit = 1 << button;
        if !down {
            self.memory[held_index] &= !bit;
            self.memory[event_index] |= bit << 4;
            return;
        }
        self.memory[held_index] |= bit;
        self.memory[event_index] |= bit;
        let is_double_click =
            self.last_mouse_click[button].is_some_and(|(previous_ms, previous_x, previous_y)| {
                now_ms.saturating_sub(previous_ms) <= 400
                    && x.abs_diff(previous_x) <= 4
                    && y.abs_diff(previous_y) <= 4
            });
        if is_double_click {
            self.memory[event_index] |= bit << 2;
            self.last_mouse_click[button] = None;
        } else {
            self.last_mouse_click[button] = Some((now_ms, x, y));
        }
    }

    pub fn set_mouse_wheel(&mut self, delta: i16) {
        self.mouse_wheel_delta = self.mouse_wheel_delta.saturating_add(delta);
        self.memory[MOUSE_ADDR as usize + 6..MOUSE_ADDR as usize + 8]
            .copy_from_slice(&self.mouse_wheel_delta.to_be_bytes());
    }

    pub fn end_mouse_frame(&mut self) {
        self.mouse_wheel_delta = 0;
        self.memory[MOUSE_ADDR as usize + 6..MOUSE_ADDR as usize + 8].fill(0);
    }

    pub fn take_sound_commands(&mut self) -> Vec<GpuSoundCommand> {
        self.sound_commands
            .iter_mut()
            .filter_map(Option::take)
            .collect()
    }

    fn gpu_word(&self, offset: u32) -> i16 {
        let high = self.memory[(GPU_BASE + offset) as usize];
        let low = self.memory[(GPU_BASE + offset + 1) as usize];
        i16::from_be_bytes([high, low])
    }

    fn gpu_source(&self) -> u32 {
        let high = (u32::from(self.memory[(GPU_BASE + 12) as usize]) << 8)
            | u32::from(self.memory[(GPU_BASE + 13) as usize]);
        let low = (u32::from(self.memory[(GPU_BASE + 10) as usize]) << 8)
            | u32::from(self.memory[(GPU_BASE + 11) as usize]);
        ((high & 0x3fff) << 16) | low
    }

    fn gpu_wide_field(&self, low_offset: u32, high_offset: u32) -> i32 {
        let low = self.gpu_word(low_offset) as u16;
        let high = self.gpu_word(high_offset) as u16;
        (u32::from(high).wrapping_shl(16) | u32::from(low)) as i32
    }

    fn gpu_read_block(&self, source: u32, size: usize) -> Vec<u8> {
        (0..size)
            .map(|offset| {
                self.memory[source.wrapping_add(offset as u32) as usize & self.memory_mask as usize]
            })
            .collect()
    }

    fn gpu_tilemap(&mut self, camera_x: i32, camera_y: i32, map_width: i32, map_height: i32) {
        if !(1..=GPU_TILEMAP_MAX_SIZE as i32).contains(&map_width)
            || !(1..=GPU_TILEMAP_MAX_SIZE as i32).contains(&map_height)
        {
            self.set_gpu_result(1);
            return;
        }
        self.tilemap_width = map_width as usize;
        self.tilemap_height = map_height as usize;
        self.tilemap =
            self.gpu_read_block(self.gpu_source(), self.tilemap_width * self.tilemap_height);
        self.tile_camera = (camera_x, camera_y);
        self.render_tile_layer();
        self.set_gpu_result(0);
    }

    fn render_tile_layer(&mut self) {
        let (screen_width, screen_height) = self.screen_size();
        let target = &mut self.layers[1];
        target.fill(0);
        if self.tilemap.is_empty() {
            return;
        }
        let split_x = screen_width / 2;
        let views = self.split_cameras.map_or_else(
            || vec![(self.tile_camera, 0, screen_width)],
            |cameras| {
                vec![
                    (cameras[0], 0, split_x),
                    (cameras[1], split_x, screen_width),
                ]
            },
        );
        let tile_size = self.tile_pixel_size as i32;
        let color_mask = if screen_width == 640 { 0x0f } else { 0xff };
        for ((camera_x, camera_y), clip_left, clip_right) in views {
            let first_column = camera_x.div_euclid(tile_size);
            let first_row = camera_y.div_euclid(tile_size);
            let last_column = camera_x
                .saturating_add(clip_right - clip_left - 1)
                .div_euclid(tile_size);
            let last_row = camera_y
                .saturating_add(screen_height - 1)
                .div_euclid(tile_size);
            for map_y in first_row.max(0)..=(last_row.min(self.tilemap_height as i32 - 1)) {
                for map_x in first_column.max(0)..=(last_column.min(self.tilemap_width as i32 - 1))
                {
                    let tile =
                        self.tilemap[map_y as usize * self.tilemap_width + map_x as usize] as usize;
                    let screen_x = (map_x * tile_size)
                        .saturating_sub(camera_x)
                        .saturating_add(clip_left);
                    let screen_y = (map_y * tile_size).saturating_sub(camera_y);
                    let source_start = tile * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE;
                    for tile_y in 0..tile_size {
                        let y = screen_y + tile_y;
                        if !(0..screen_height).contains(&y) {
                            continue;
                        }
                        for tile_x in 0..tile_size {
                            let x = screen_x + tile_x;
                            if x < clip_left || x >= clip_right || x < 0 || x >= screen_width {
                                continue;
                            }
                            let source_index = source_start
                                + tile_y as usize * self.tile_pixel_size
                                + tile_x as usize;
                            target[(y * screen_width + x) as usize] =
                                self.tile_memory[source_index] & color_mask;
                        }
                    }
                }
            }
        }
    }

    fn render_sprites(&mut self) {
        let (screen_width, screen_height) = self.screen_size();
        let target = &mut self.layers[2];
        target.fill(0);
        let split_x = screen_width / 2;
        let views = self.split_cameras.map_or_else(
            || vec![(self.tile_camera, 0, screen_width)],
            |cameras| {
                vec![
                    (cameras[0], 0, split_x),
                    (cameras[1], split_x, screen_width),
                ]
            },
        );
        let color_mask = if screen_width == 640 { 0x0f } else { 0xff };
        for (slot, sprite) in self.sprites.iter().enumerate() {
            let Some(sprite) = sprite else {
                continue;
            };
            let (world_x, world_y, flags) = self.sprite_descriptors[slot];
            if flags & 1 == 0 {
                continue;
            }
            for ((camera_x, camera_y), clip_left, clip_right) in &views {
                let screen_x = world_x.saturating_sub(*camera_x).saturating_add(*clip_left);
                let screen_y = world_y.saturating_sub(*camera_y);
                let left = screen_x.max(*clip_left).max(0);
                let right = screen_x
                    .saturating_add(sprite.width as i32)
                    .min(*clip_right)
                    .min(screen_width);
                let top = screen_y.max(0);
                let bottom = screen_y
                    .saturating_add(sprite.height as i32)
                    .min(screen_height);
                for y in top..bottom {
                    let mut source_y = (y - screen_y) as usize;
                    if flags & 4 != 0 {
                        source_y = sprite.height - source_y - 1;
                    }
                    for x in left..right {
                        let mut source_x = (x - screen_x) as usize;
                        if flags & 2 != 0 {
                            source_x = sprite.width - source_x - 1;
                        }
                        let pixel = sprite.pixels[source_y * sprite.width + source_x];
                        if pixel != 0 {
                            target[(y * screen_width + x) as usize] = pixel & color_mask;
                        }
                    }
                }
            }
        }
    }

    fn collision_mask(&self, mask: u16) -> Option<(usize, usize, Vec<u8>)> {
        if usize::from(mask) < GPU_SPRITE_SLOTS {
            let sprite = self.sprites[mask as usize].as_ref()?;
            return Some((sprite.width, sprite.height, sprite.pixels.clone()));
        }
        let tile = usize::from(mask).checked_sub(GPU_SPRITE_SLOTS)?;
        if tile < GPU_TILE_COUNT {
            let size = self.tile_pixel_size;
            let start = tile * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE;
            return Some((
                size,
                size,
                self.tile_memory[start..start + size * size].to_vec(),
            ));
        }
        if usize::from(mask) == GPU_SPRITE_SLOTS + GPU_TILE_COUNT {
            let pixels = (0i32..7)
                .flat_map(|y| (0i32..7).map(move |x| u8::from((x - 3).abs() + (y - 3).abs() <= 3)))
                .collect();
            return Some((7, 7, pixels));
        }
        None
    }

    fn masks_overlap(
        first: &(usize, usize, Vec<u8>),
        first_x: i32,
        first_y: i32,
        second: &(usize, usize, Vec<u8>),
        second_x: i32,
        second_y: i32,
    ) -> bool {
        let left = i64::from(first_x).max(i64::from(second_x));
        let top = i64::from(first_y).max(i64::from(second_y));
        let right =
            (i64::from(first_x) + first.0 as i64).min(i64::from(second_x) + second.0 as i64);
        let bottom =
            (i64::from(first_y) + first.1 as i64).min(i64::from(second_y) + second.1 as i64);
        for y in top..bottom {
            for x in left..right {
                let first_index = ((y - i64::from(first_y)) as usize) * first.0
                    + (x - i64::from(first_x)) as usize;
                let second_index = ((y - i64::from(second_y)) as usize) * second.0
                    + (x - i64::from(second_x)) as usize;
                if first.2[first_index] != 0 && second.2[second_index] != 0 {
                    return true;
                }
            }
        }
        false
    }

    fn gpu_world_views(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Vec<(i64, i64, i64, i64, i64, i64, i64)> {
        let (screen_width, screen_height) = self.screen_size();
        let split_x = screen_width / 2;
        let views = self.split_cameras.map_or_else(
            || vec![(self.tile_camera, 0, screen_width)],
            |cameras| {
                vec![
                    (cameras[0], 0, split_x),
                    (cameras[1], split_x, screen_width),
                ]
            },
        );
        let mut result = Vec::with_capacity(views.len());
        for ((camera_x, camera_y), clip_left, clip_right) in views {
            let (camera_x, camera_y) = (i64::from(camera_x), i64::from(camera_y));
            let mut left = i64::from(x).max(camera_x);
            let mut right =
                (i64::from(x) + i64::from(width)).min(camera_x + i64::from(clip_right - clip_left));
            let mut top = i64::from(y).max(camera_y);
            let mut bottom =
                (i64::from(y) + i64::from(height)).min(camera_y + i64::from(screen_height));
            if self.tilemap_width > 0 && self.tilemap_height > 0 {
                left = left.max(0);
                right = right.min((self.tilemap_width * self.tile_pixel_size) as i64);
                top = top.max(0);
                bottom = bottom.min((self.tilemap_height * self.tile_pixel_size) as i64);
            }
            if left < right && top < bottom {
                result.push((
                    left,
                    top,
                    right,
                    bottom,
                    i64::from(clip_left) - camera_x,
                    -camera_y,
                    i64::from(screen_width),
                ));
            }
        }
        result
    }

    fn gpu_world_tile(&mut self, tile: u8, origin_x: i32, origin_y: i32) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        if layer == 0 {
            self.set_gpu_result(1);
            return;
        }
        let size = self.tile_pixel_size as i32;
        let views = self.gpu_world_views(origin_x, origin_y, size, size);
        let source_start = usize::from(tile) * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE;
        for (left, top, right, bottom, dx, dy, stride) in views {
            for pixel_y in top..bottom {
                let source_y = (pixel_y - i64::from(origin_y)) as usize;
                for pixel_x in left..right {
                    let source_x = (pixel_x - i64::from(origin_x)) as usize;
                    let pixel =
                        self.tile_memory[source_start + source_y * self.tile_pixel_size + source_x];
                    if pixel != 0 {
                        let screen_x = pixel_x + dx;
                        let screen_y = pixel_y + dy;
                        let index = (screen_y * stride + screen_x) as usize;
                        self.layers[layer][index] = pixel;
                    }
                }
            }
        }
        self.set_gpu_result(0);
    }

    fn gpu_world_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u8) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        if layer == 0 {
            self.set_gpu_result(1);
            return;
        }
        for (left, top, right, bottom, dx, dy, stride) in self.gpu_world_views(x, y, width, height)
        {
            for world_y in top..bottom {
                let start = ((world_y + dy) * stride + left + dx) as usize;
                let end = start + (right - left) as usize;
                self.layers[layer][start..end].fill(color);
            }
        }
        self.set_gpu_result(0);
    }

    fn gpu_world_disc(&mut self, x: i32, y: i32, radius: i32, color: u8, opacity: i32) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        if layer == 0 || !(0..=32).contains(&radius) || !(0..=16).contains(&opacity) {
            self.set_gpu_result(1);
            return;
        }
        const DITHER: [i32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
        for (left, top, right, bottom, dx, dy, stride) in self.gpu_world_views(
            x.saturating_sub(radius),
            y.saturating_sub(radius),
            radius * 2 + 1,
            radius * 2 + 1,
        ) {
            for world_y in top..bottom {
                for world_x in left..right {
                    let offset_x = world_x - i64::from(x);
                    let offset_y = world_y - i64::from(y);
                    let distance = offset_x * offset_x + offset_y * offset_y;
                    if distance > i64::from(radius) * i64::from(radius) {
                        continue;
                    }
                    let mut coverage = opacity;
                    if radius > 2 && distance > i64::from(radius - 1) * i64::from(radius - 1) {
                        coverage /= 2;
                    }
                    let dither_index = (world_y.rem_euclid(4) * 4 + world_x.rem_euclid(4)) as usize;
                    if DITHER[dither_index] < coverage {
                        let screen_x = world_x + dx;
                        let screen_y = world_y + dy;
                        self.layers[layer][(screen_y * stride + screen_x) as usize] = color;
                    }
                }
            }
        }
        self.set_gpu_result(0);
    }

    fn gpu_world_ammo(&mut self, world_x: i32, world_y: i32) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        if layer == 0 {
            self.set_gpu_result(1);
            return;
        }
        let (screen_width, screen_height) = self.screen_size();
        let split_x = screen_width / 2;
        let views = self.split_cameras.map_or_else(
            || vec![(self.tile_camera, 0, screen_width)],
            |cameras| {
                vec![
                    (cameras[0], 0, split_x),
                    (cameras[1], split_x, screen_width),
                ]
            },
        );
        for ((camera_x, camera_y), clip_left, clip_right) in views {
            for offset_y in -3i32..=3 {
                for offset_x in -3i32..=3 {
                    let distance = offset_x.abs() + offset_y.abs();
                    let x = world_x
                        .saturating_add(offset_x)
                        .saturating_sub(camera_x)
                        .saturating_add(clip_left);
                    let y = world_y.saturating_add(offset_y).saturating_sub(camera_y);
                    if distance <= 3
                        && (clip_left..clip_right).contains(&x)
                        && (0..screen_height).contains(&y)
                    {
                        self.layers[layer][(y * screen_width + x) as usize] =
                            if distance <= 1 { 1 } else { 5 };
                    }
                }
            }
        }
        self.set_gpu_result(0);
    }

    fn gpu_world_mask(&mut self, pixels: &[u8], width: usize, height: usize, x: i32, y: i32) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        let origin_x = x.saturating_sub((width / 2) as i32);
        let origin_y = y.saturating_sub((height / 2) as i32);
        for (left, top, right, bottom, dx, dy, stride) in
            self.gpu_world_views(origin_x, origin_y, width as i32, height as i32)
        {
            for world_y in top..bottom {
                let source_y = (world_y - i64::from(origin_y)) as usize;
                for world_x in left..right {
                    let source_x = (world_x - i64::from(origin_x)) as usize;
                    let pixel = pixels[source_y * width + source_x];
                    if pixel != 0 {
                        let color = if matches!(pixel, 1 | 10 | 11) { 10 } else { 12 };
                        let screen_x = world_x + dx;
                        let screen_y = world_y + dy;
                        self.layers[layer][(screen_y * stride + screen_x) as usize] = color;
                    }
                }
            }
        }
    }

    fn gpu_world_emitters(&mut self, wide: bool, source: u32, count: i32, tick: i32) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        let record_size = if wide { 12usize } else { 8 };
        if layer == 0 || count < 0 {
            self.set_gpu_result(1);
            return;
        }
        let length = match (count as usize).checked_mul(record_size) {
            Some(length) => length,
            None => {
                self.set_gpu_result(1);
                return;
            }
        };
        let Some(end) = (source as usize).checked_add(length) else {
            self.set_gpu_result(1);
            return;
        };
        if end > self.memory.len() {
            self.set_gpu_result(1);
            return;
        }
        let read_i16 = |address: usize| {
            i16::from_be_bytes([self.memory[address], self.memory[address + 1]]) as i32
        };
        let read_i32 = |address: usize| {
            i32::from_be_bytes([
                self.memory[address],
                self.memory[address + 1],
                self.memory[address + 2],
                self.memory[address + 3],
            ])
        };
        let records = (0..count as usize)
            .map(|index| {
                let address = source as usize + index * record_size;
                if wide {
                    (
                        read_i32(address),
                        read_i32(address + 4),
                        read_i16(address + 8),
                        read_i16(address + 10),
                    )
                } else {
                    (
                        read_i16(address),
                        read_i16(address + 2),
                        read_i16(address + 4),
                        read_i16(address + 6),
                    )
                }
            })
            .collect::<Vec<_>>();
        let mut drawn_sites = HashSet::new();
        let mut smoke_cells = HashSet::new();
        for (index, (x, y, life, mask)) in records.into_iter().enumerate() {
            if life <= 0
                || self
                    .gpu_world_views(x.saturating_sub(24), y.saturating_sub(56), 48, 80)
                    .is_empty()
                || !drawn_sites.insert((x, y, mask))
            {
                continue;
            }
            let wreck = if mask >= 0 {
                match self.collision_mask(mask as u16) {
                    Some((width, height, pixels)) => {
                        self.gpu_world_mask(&pixels, width, height, x, y);
                        true
                    }
                    None => {
                        self.set_gpu_result(1);
                        return;
                    }
                }
            } else {
                false
            };
            self.gpu_world_disc(x, y.saturating_add(2), if wreck { 10 } else { 4 }, 10, 12);
            self.gpu_world_disc(
                x,
                y,
                if wreck { 3 } else { 1 },
                if (tick + index as i32).rem_euclid(6) < 3 {
                    8
                } else {
                    5
                },
                16,
            );
            let cell = (x.div_euclid(16), y.div_euclid(16));
            if !smoke_cells.insert(cell) {
                continue;
            }
            for particle in 0..if wreck { 8 } else { 4 } {
                let seed =
                    (index as i64 * 137 + particle * 71 + i64::from(x) * 13 + i64::from(y) * 7)
                        .rem_euclid(0x1_0000) as i32;
                let age = (tick + seed).rem_euclid(36);
                let drift = seed.rem_euclid(7) - 3;
                let px = x
                    .saturating_add(seed.rem_euclid(9) - 4)
                    .saturating_add((drift * age).div_euclid(24));
                let py = y.saturating_sub(age * (if wreck { 2 } else { 1 }) / 3);
                if age < 16 {
                    let radius = ((if wreck { 5 } else { 3 }) - age / 5).max(1);
                    self.gpu_world_disc(
                        px,
                        py,
                        radius,
                        if age < 5 {
                            5
                        } else if age < 11 {
                            8
                        } else {
                            11
                        },
                        16,
                    );
                    if age < 5 {
                        self.gpu_world_disc(px, py, 1, 1, 12);
                    }
                } else {
                    self.gpu_world_disc(
                        px,
                        py,
                        2 + age / 8,
                        if age < 26 { 10 } else { 9 },
                        ((36 - age) / 2).max(1),
                    );
                }
            }
        }
        self.set_gpu_result(0);
    }

    fn screen_size(&self) -> (i32, i32) {
        if self.memory[GPU_MODE_ADDR as usize] == 1 {
            (640, 480)
        } else {
            (320, 240)
        }
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
        let visible = self.memory[GPU_LAYER_MASK as usize] & ((1 << GPU_LAYER_COUNT) - 1);
        pixels[..width * height].fill(0);
        let mut order = [0usize; GPU_LAYER_COUNT];
        for (layer, entry) in order.iter_mut().enumerate() {
            *entry = layer;
        }
        order.sort_unstable_by_key(|&layer| (self.memory[GPU_LAYER_ORDER as usize + layer], layer));
        for layer in order {
            if visible & (1 << layer) == 0 {
                continue;
            }
            let transparent = self.memory[GPU_LAYER_TRANSPARENCY as usize + layer];
            for (index, output) in pixels[..width * height].iter_mut().enumerate() {
                let color = if layer == 0 {
                    if width == 320 {
                        self.memory[VRAM_START as usize + index]
                    } else {
                        let packed = self.memory[VRAM_START as usize + index / 2];
                        if index & 1 == 0 {
                            packed >> 4
                        } else {
                            packed & 0x0f
                        }
                    }
                } else {
                    self.layers[layer][index]
                };
                if color != transparent {
                    *output = color;
                }
            }
        }
    }

    pub fn palette_color(&self, index: u8) -> [u8; 3] {
        self.palette[index as usize]
    }

    fn gpu_pixel(&mut self, x: i32, y: i32, color: u8) {
        let layer =
            self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8) as usize;
        self.gpu_pixel_on_layer(layer, x, y, color);
    }

    fn gpu_pixel_on_layer(&mut self, layer: usize, x: i32, y: i32, color: u8) {
        let (width, height) = self.screen_size();
        if x < 0 || y < 0 || x >= width || y >= height {
            return;
        }
        if width == 320 && layer != 0 {
            let address = (y * width + x) as usize;
            self.layers[layer][address] = color;
        } else if width == 320 {
            let address = VRAM_START + (y * width + x) as u32;
            self.memory[address as usize] = color;
        } else if layer != 0 {
            let address = (y * width + x) as usize;
            self.layers[layer][address] = color & 0x0f;
        } else {
            let address = VRAM_START + (y * (width / 2) + x / 2) as u32;
            let shift = if x & 1 == 0 { 4 } else { 0 };
            let mask = 0xf << shift;
            self.memory[address as usize] =
                (self.memory[address as usize] & !mask) | ((color & 0xf) << shift);
        }
    }

    fn gpu_3d(&mut self, source: u32) {
        self.gpu_triangles.clear();
        self.gpu_lines.clear();
        let header_bytes = 10 * 2;
        if (source as usize).saturating_add(header_bytes) > self.memory.len() {
            self.set_gpu_result(1);
            return;
        }
        let header = (0..10)
            .map(|index| self.read_u16(source + index * 2) as i16)
            .collect::<Vec<_>>();
        let vertex_count = header[0] as usize;
        let face_count = header[1] as usize;
        let angles = [
            header[2] as u16 as u8,
            header[3] as u16 as u8,
            header[4] as u16 as u8,
        ];
        let translation = [
            i32::from(header[5]),
            i32::from(header[6]),
            i32::from(header[7]),
        ];
        let focal = i32::from(header[8]);
        let flags = header[9] as u16;
        let word_count = 10usize
            .saturating_add(vertex_count.saturating_mul(3))
            .saturating_add(face_count.saturating_mul(4));
        if !(3..=GPU3D_MAX_VERTICES).contains(&vertex_count)
            || !(1..=GPU3D_MAX_FACES).contains(&face_count)
            || focal <= 0
            || flags == 0
            || flags & !(GPU3D_FILL | GPU3D_WIREFRAME) != 0
            || (source as usize).saturating_add(word_count.saturating_mul(2)) > self.memory.len()
        {
            self.set_gpu_result(1);
            return;
        }

        let vertex_offset = source + 20;
        let mut vertices = Vec::with_capacity(vertex_count);
        for index in 0..vertex_count {
            let offset = vertex_offset + (index * 3 * 2) as u32;
            let original = [
                i32::from(self.read_u16(offset) as i16),
                i32::from(self.read_u16(offset + 2) as i16),
                i32::from(self.read_u16(offset + 4) as i16),
            ];
            vertices.push(gpu3d_transform(original, angles, translation));
        }

        let face_offset = vertex_offset + (vertex_count * 3 * 2) as u32;
        let mut faces = Vec::with_capacity(face_count);
        for index in 0..face_count {
            let offset = face_offset + (index * 4 * 2) as u32;
            let face = [
                self.read_u16(offset),
                self.read_u16(offset + 2),
                self.read_u16(offset + 4),
            ];
            let color = self.read_u16(offset + 6) as u8;
            if face
                .iter()
                .any(|&vertex| usize::from(vertex) >= vertex_count)
            {
                self.set_gpu_result(2);
                return;
            }
            faces.push((face, color));
        }

        let (width, height) = self.screen_size();
        let projected = vertices
            .iter()
            .map(|&vertex| {
                if vertex.2 > 0 {
                    Some(gpu3d_project(vertex, focal, width, height))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.layers[3][..(width * height) as usize].fill(0);
        let mut painter_order = (0..face_count).collect::<Vec<_>>();
        painter_order.sort_by_key(|&index| {
            let face = faces[index].0;
            -face
                .iter()
                .map(|&vertex| vertices[usize::from(vertex)].2)
                .sum::<i32>()
        });
        for index in painter_order {
            let (face, color) = faces[index];
            let [a, b, c] = face.map(usize::from);
            let (Some(a), Some(b), Some(c)) = (projected[a], projected[b], projected[c]) else {
                continue;
            };
            let points = [a, b, c];
            if flags & GPU3D_FILL != 0 {
                self.gpu_triangles.push(GpuTriangle { points, color });
            } else if flags & GPU3D_WIREFRAME != 0 {
                self.gpu_triangles.push(GpuTriangle { points, color: 0 });
            }
            if flags & GPU3D_WIREFRAME != 0 {
                self.gpu_lines.extend([
                    GpuLine {
                        start: a,
                        end: b,
                        color,
                    },
                    GpuLine {
                        start: b,
                        end: c,
                        color,
                    },
                    GpuLine {
                        start: c,
                        end: a,
                        color,
                    },
                ]);
            }
        }
        self.set_gpu_result(0);
    }

    fn set_gpu_result(&mut self, value: u16) {
        let [high, low] = value.to_be_bytes();
        self.memory[(GPU_BASE + 14) as usize] = high;
        self.memory[(GPU_BASE + 15) as usize] = low;
    }

    fn gpu_command(&mut self, command: u8) {
        if self.mmu_enabled
            && self.privilege_mode == PrivilegeMode::User
            && matches!(
                command,
                GPU_SPRITE | GPU_BITMAP | GPU_TEXT | GPU_POLY | GPU_NET_OPEN
                    ..=GPU_NET_INFO
                        | GPU_TILEMAP
                        | GPU_TILEMAP_WIDE
                        | GPU_SPRITE_UPLOAD
                        | GPU_TILE_UPLOAD
                        | GPU_WORLD_EMITTERS
                        | GPU_WORLD_EMITTERS_WIDE
                        | GPU_3D
            )
        {
            self.set_gpu_result(1);
            self.console.log_error(
                "gpu",
                "MMU-backed user buffer commands require a validated buffer service",
            );
            return;
        }
        let color = self.memory[(GPU_BASE + 1) as usize];
        let x = i32::from(self.gpu_word(2));
        let y = i32::from(self.gpu_word(4));
        let w = i32::from(self.gpu_word(6));
        let h = i32::from(self.gpu_word(8));
        match command {
            GPU_CLEAR => {
                let (width, height) = self.screen_size();
                let layer = self.memory[GPU_LAYER_SELECT as usize].min((GPU_LAYER_COUNT - 1) as u8)
                    as usize;
                if layer != 0 {
                    self.layers[layer][..(width * height) as usize].fill(if width == 320 {
                        color
                    } else {
                        color & 0x0f
                    });
                } else if width == 320 {
                    let start = VRAM_START as usize;
                    self.memory[start..start + (width * height) as usize].fill(color);
                } else {
                    let start = VRAM_START as usize;
                    let fill = ((color & 0xf) << 4) | (color & 0xf);
                    self.memory[start..start + (width * height / 2) as usize].fill(fill);
                }
            }
            GPU_PIXEL => self.gpu_pixel(x, y, color),
            GPU_RECT => {
                for row in y.max(0)..(y + h).min(self.screen_size().1) {
                    for column in x.max(0)..(x + w).min(self.screen_size().0) {
                        self.gpu_pixel(column, row, color);
                    }
                }
            }
            GPU_LINE => {
                let (mut x0, mut y0) = (x, y);
                let (x1, y1) = (w, h);
                let dx = (x1 - x0).abs();
                let sx = if x0 < x1 { 1 } else { -1 };
                let dy = -(y1 - y0).abs();
                let sy = if y0 < y1 { 1 } else { -1 };
                let mut error = dx + dy;
                for _ in 0..2000 {
                    self.gpu_pixel(x0, y0, color);
                    if x0 == x1 && y0 == y1 {
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
            }
            GPU_SPRITE => {
                let source = self.gpu_source();
                let (screen_width, screen_height) = self.screen_size();
                for row in 0..h.max(0).min(screen_height) {
                    for column in 0..w.max(0).min(screen_width) {
                        let address =
                            source.wrapping_add((row * w + column) as u32) & self.memory_mask;
                        let pixel = self.memory[address as usize];
                        if pixel != 0 {
                            self.gpu_pixel(x + column, y + row, pixel);
                        }
                    }
                }
            }
            GPU_BITMAP => {
                let source = self.gpu_source();
                let (screen_width, screen_height) = self.screen_size();
                let draw_width = w.max(0).min(screen_width);
                let draw_height = h.max(0).min(screen_height);
                let row_bytes = (draw_width + 7) / 8;
                for row in 0..draw_height {
                    for column in 0..draw_width {
                        let address = source.wrapping_add((row * row_bytes + column / 8) as u32)
                            & self.memory_mask;
                        if self.memory[address as usize] & (0x80 >> (column % 8)) != 0 {
                            self.gpu_pixel(x + column, y + row, color);
                        }
                    }
                }
            }
            GPU_TEXT => self.gpu_text(x, y, color),
            GPU_PRESENT => {
                self.frame_yield = true;
                self.frame_presented = true;
            }
            GPU_RANDOM => {
                let mut value = self.random_state;
                value ^= value << 13;
                value ^= value >> 17;
                value ^= value << 5;
                self.random_state = value;
                self.memory[(GPU_BASE + 14) as usize] = value as u8;
            }
            GPU_POLY => self.gpu_polygon(x, y, w, color),
            GPU_TICKS => {
                let count = self.frame_count.to_be_bytes();
                self.memory[(GPU_BASE + 14) as usize] = count[0];
                self.memory[(GPU_BASE + 15) as usize] = count[1];
            }
            GPU_RTC => {
                let index = color as usize;
                self.memory[(GPU_BASE + 14) as usize] = if index < 3 {
                    self.memory[(RTC_START as usize) + index]
                } else {
                    0
                };
            }
            GPU_SOUND => {
                let command = GpuSoundCommand {
                    channel: color & 3,
                    frequency: self.gpu_word(2) as u16,
                    frames: self.gpu_word(4) as u16,
                    waveform: self.gpu_word(6) as u16,
                    volume: self.gpu_word(8) as u16,
                };
                self.sound_commands[command.channel as usize] = Some(command);
            }
            GPU_NET_OPEN..=GPU_NET_INFO => self.gpu_network(command, color, self.gpu_word(2)),
            GPU_PALETTE => {
                let red = self.gpu_word(2) as u8;
                let green = self.gpu_word(4) as u8;
                let blue = self.gpu_word(6) as u8;
                self.palette[color as usize] = [red, green, blue];
                self.memory[(GPU_BASE + 14) as usize] = 0;
                self.memory[(GPU_BASE + 15) as usize] = 0;
            }
            GPU_TILEMAP => self.gpu_tilemap(x, y, w, h),
            GPU_SPRITE_UPLOAD => {
                if usize::from(color) >= GPU_SPRITE_SLOTS
                    || !(1..=GPU_SPRITE_MAX_SIZE).contains(&w)
                    || !(1..=GPU_SPRITE_MAX_SIZE).contains(&h)
                {
                    self.set_gpu_result(1);
                } else {
                    let width = w as usize;
                    let height = h as usize;
                    let pixels = self.gpu_read_block(self.gpu_source(), width * height);
                    self.sprites[color as usize] = Some(GpuSprite {
                        width,
                        height,
                        pixels,
                    });
                    self.render_sprites();
                    self.set_gpu_result(0);
                }
            }
            GPU_SPRITE_MOVE => {
                if usize::from(color) < GPU_SPRITE_SLOTS {
                    self.sprite_descriptors[color as usize] = (x, y, w as u8 & 7);
                    self.render_sprites();
                }
            }
            GPU_TILE_UPLOAD => {
                if usize::from(color) >= GPU_TILE_COUNT {
                    self.set_gpu_result(1);
                } else {
                    let size = self.tile_pixel_size;
                    let data = self.gpu_read_block(self.gpu_source(), size * size);
                    let start = color as usize * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE;
                    self.tile_memory[start..start + size * size].copy_from_slice(&data);
                    self.render_tile_layer();
                    self.set_gpu_result(0);
                }
            }
            GPU_TILE_CAMERA => {
                self.tile_camera = (x, y);
                self.split_cameras = None;
                self.render_tile_layer();
                self.render_sprites();
            }
            GPU_TILE_SIZE => {
                if !(1..=GPU_TILE_MAX_SIZE as i32).contains(&x) {
                    self.set_gpu_result(1);
                } else {
                    self.tile_pixel_size = x as usize;
                    self.render_tile_layer();
                    self.set_gpu_result(0);
                }
            }
            GPU_SPLIT_CAMERA => {
                self.split_cameras = Some([(x, y), (w, h)]);
                self.render_tile_layer();
                self.render_sprites();
            }
            GPU_TILE_CAMERA_WIDE => {
                self.tile_camera = (self.gpu_wide_field(2, 24), self.gpu_wide_field(4, 26));
                self.split_cameras = None;
                self.render_tile_layer();
                self.render_sprites();
            }
            GPU_SPLIT_CAMERA_WIDE => {
                self.split_cameras = Some([
                    (self.gpu_wide_field(2, 24), self.gpu_wide_field(4, 26)),
                    (self.gpu_wide_field(6, 28), self.gpu_wide_field(8, 30)),
                ]);
                self.render_tile_layer();
                self.render_sprites();
            }
            GPU_TILEMAP_WIDE => {
                self.gpu_tilemap(self.gpu_wide_field(2, 24), self.gpu_wide_field(4, 26), w, h)
            }
            GPU_MASK_OVERLAP => {
                let first = self.collision_mask(self.gpu_word(10) as u16);
                let second = self.collision_mask(self.gpu_word(12) as u16);
                let result = match (first, second) {
                    (Some(first), Some(second)) => {
                        u16::from(Self::masks_overlap(&first, x, y, &second, w, h))
                    }
                    _ => u16::MAX,
                };
                self.set_gpu_result(result);
            }
            GPU_WORLD_TILE => self.gpu_world_tile(color, x, y),
            GPU_WORLD_AMMO => self.gpu_world_ammo(x, y),
            GPU_WORLD_RECT => self.gpu_world_rect(x, y, w, h, color),
            GPU_WORLD_PARTICLE => self.gpu_world_disc(x, y, w, color, h),
            GPU_WORLD_EMITTERS => self.gpu_world_emitters(false, self.gpu_source(), x, y),
            GPU_SPRITE_MOVE_WIDE => {
                let sprite_x = self.gpu_wide_field(2, 24);
                let sprite_y = self.gpu_wide_field(4, 26);
                if usize::from(color) < GPU_SPRITE_SLOTS {
                    self.sprite_descriptors[color as usize] = (sprite_x, sprite_y, w as u8 & 7);
                    self.render_sprites();
                }
            }
            GPU_MASK_OVERLAP_WIDE => {
                let first = self.collision_mask(self.gpu_word(10) as u16);
                let second = self.collision_mask(self.gpu_word(12) as u16);
                let result = match (first, second) {
                    (Some(first), Some(second)) => u16::from(Self::masks_overlap(
                        &first,
                        self.gpu_wide_field(2, 24),
                        self.gpu_wide_field(4, 26),
                        &second,
                        self.gpu_wide_field(6, 28),
                        self.gpu_wide_field(8, 30),
                    )),
                    _ => u16::MAX,
                };
                self.set_gpu_result(result);
            }
            GPU_WORLD_TILE_WIDE => self.gpu_world_tile(
                color,
                self.gpu_wide_field(2, 24),
                self.gpu_wide_field(4, 26),
            ),
            GPU_WORLD_AMMO_WIDE => {
                self.gpu_world_ammo(self.gpu_wide_field(2, 24), self.gpu_wide_field(4, 26))
            }
            GPU_WORLD_RECT_WIDE => self.gpu_world_rect(
                self.gpu_wide_field(2, 24),
                self.gpu_wide_field(4, 26),
                w,
                h,
                color,
            ),
            GPU_WORLD_PARTICLE_WIDE => self.gpu_world_disc(
                self.gpu_wide_field(2, 24),
                self.gpu_wide_field(4, 26),
                w,
                color,
                h,
            ),
            GPU_WORLD_EMITTERS_WIDE => self.gpu_world_emitters(true, self.gpu_source(), x, y),
            GPU_3D => self.gpu_3d(self.gpu_source()),
            GPU_SAVE => {
                let (width, height) = self.screen_size();
                let size = if width == 640 {
                    (width * height / 2) as usize
                } else {
                    (width * height) as usize
                };
                self.saved_video =
                    Some(self.memory[VRAM_START as usize..VRAM_START as usize + size].to_vec());
            }
            GPU_LOAD => {
                if let Some(saved) = &self.saved_video {
                    let end = VRAM_START as usize + saved.len();
                    self.memory[VRAM_START as usize..end].copy_from_slice(saved);
                }
            }
            _ => {}
        }
    }

    fn gpu_text(&mut self, mut x: i32, mut y: i32, color: u8) {
        let left = x;
        let source = self.gpu_source();
        let mut bytes = Vec::with_capacity(64);
        for offset in 0..255u32 {
            let address = source.wrapping_add(offset) & self.memory_mask;
            let value = self.memory[address as usize];
            if value == 0 {
                break;
            }
            bytes.push(value);
        }
        let text = String::from_utf8_lossy(&bytes);
        for character in text.chars() {
            if character == '\n' {
                x = left;
                y += 8;
                continue;
            }
            let glyph = match character {
                ' '..='~' => character as usize - 32,
                'ö' => 95,
                'ä' => 96,
                'å' => 97,
                'Ö' => 98,
                'Ä' => 99,
                'Å' => 100,
                _ => '?' as usize - 32,
            };
            for row in 0..7 {
                let address = FONT_ADDR + glyph * 8 + row;
                if address >= self.memory.len() {
                    continue;
                }
                let bits = self.memory[address];
                for column in 0..5 {
                    if bits & (0x80 >> column) != 0 {
                        self.gpu_pixel(x + column, y + row as i32, color);
                    }
                }
            }
            x += 6;
        }
    }

    fn gpu_polygon(&mut self, x: i32, y: i32, count: i32, color: u8) {
        let source = self.gpu_source();
        let point_count = count.clamp(0, 64) as u32;
        let mut points = Vec::with_capacity(point_count as usize);
        for index in 0..point_count {
            let address = source.wrapping_add(index * 2) & self.memory_mask;
            let dx = self.memory[address as usize] as i8 as i32;
            let dy =
                self.memory[(address.wrapping_add(1) & self.memory_mask) as usize] as i8 as i32;
            points.push((x + dx, y + dy));
        }
        match points.as_slice() {
            [] => {}
            [point] => self.gpu_pixel(point.0, point.1, color),
            _ => {
                for index in 0..points.len() {
                    let start = points[index];
                    let end = points[(index + 1) % points.len()];
                    self.gpu_line(start.0, start.1, end.0, end.1, color);
                }
            }
        }
    }

    fn gpu_line(&mut self, x: i32, y: i32, end_x: i32, end_y: i32, color: u8) {
        let (mut x, mut y) = (x, y);
        let dx = (end_x - x).abs();
        let step_x = if x < end_x { 1 } else { -1 };
        let dy = -(end_y - y).abs();
        let step_y = if y < end_y { 1 } else { -1 };
        let mut error = dx + dy;
        for _ in 0..2000 {
            self.gpu_pixel(x, y, color);
            if x == end_x && y == end_y {
                break;
            }
            let doubled = error * 2;
            if doubled >= dy {
                error += dy;
                x += step_x;
            }
            if doubled <= dx {
                error += dx;
                y += step_y;
            }
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
        let stack_pointer = self.sp.wrapping_sub(2) & self.guest_address_mask();
        self.write_guest_u16(stack_pointer, value)?;
        self.sp = stack_pointer;
        Ok(())
    }

    fn pop_u16(&mut self) -> Result<u16, CpuError> {
        let value = self.read_guest_u16(self.sp)?;
        self.sp = self.sp.wrapping_add(2) & self.guest_address_mask();
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

    fn enter_event(
        &mut self,
        cause: u16,
        saved_pc: u32,
        handler: u32,
        entry_kind: u16,
        gate_type: crate::interrupts::GateType,
    ) -> Result<(), CpuError> {
        if handler > self.guest_address_mask()
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
        let Some(frame_end) = frame_start.checked_add(EXCEPTION_ENTRY_BYTES) else {
            self.running = false;
            return Err(CpuError::DoubleFault);
        };
        if frame_end as usize > self.memory.len() {
            self.running = false;
            return Err(CpuError::DoubleFault);
        }

        let flags = self.pack_flags().to_be_bytes();
        let interrupt_enabled = self.read(ICR_ADDR) & 1 != 0;
        let status = (u16::from(old_mode == PrivilegeMode::User)
            | (u16::from(interrupt_enabled) << 1))
            .to_be_bytes();
        let cause_bytes = cause.to_be_bytes();
        let kind = entry_kind.to_be_bytes();
        let frame = &mut self.memory[frame_start as usize..frame_end as usize];
        frame[0..4].copy_from_slice(&saved_pc.to_be_bytes());
        frame[4..6].copy_from_slice(&flags);
        frame[6..8].copy_from_slice(&status);
        frame[8..12].copy_from_slice(&old_sp.to_be_bytes());
        frame[12..14].copy_from_slice(&cause_bytes);
        frame[14..16].copy_from_slice(&kind);

        if gate_type == crate::interrupts::GateType::Interrupt {
            let interrupt_control = self.read(ICR_ADDR) & !1;
            self.write(ICR_ADDR, interrupt_control);
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
        let frame_end = self
            .sp
            .checked_add(EXCEPTION_ENTRY_BYTES)
            .ok_or(CpuError::DoubleFault)?;
        if frame_end as usize > self.memory.len() {
            self.running = false;
            return Err(CpuError::DoubleFault);
        }
        let frame = &self.memory[self.sp as usize..frame_end as usize];
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
            || (if status & 1 != 0 {
                saved_sp > self.guest_address_mask()
            } else {
                saved_sp as usize >= self.memory.len()
            })
        {
            self.running = false;
            return Err(CpuError::DoubleFault);
        }

        let prior_mode = if status & 1 != 0 {
            PrivilegeMode::User
        } else {
            PrivilegeMode::Supervisor
        };
        self.restore_flags(saved_flags);
        self.pc = saved_pc;
        self.privilege_mode = prior_mode;
        let interrupt_control = (self.read(ICR_ADDR) & !1) | ((status >> 1) as u8 & 1);
        self.write(ICR_ADDR, interrupt_control);
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
        let stack_pointer = self.sp.wrapping_sub(4) & self.guest_address_mask();
        self.write_guest_u32(stack_pointer, value)?;
        self.sp = stack_pointer;
        Ok(())
    }

    fn pop_u32(&mut self) -> Result<u32, CpuError> {
        let value = self.read_guest_u32(self.sp)?;
        self.sp = self.sp.wrapping_add(4) & self.guest_address_mask();
        Ok(value)
    }

    fn check_interrupts(&mut self) -> Result<(), CpuError> {
        let icr = self.read(ICR_ADDR);
        if icr & 1 == 0 {
            return Ok(());
        }
        let (pending_bit, vector) = if icr & 2 != 0 {
            (2, u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD))
        } else if icr & 0x10 != 0 && self.read(PIT_CONTROL_ADDR) & 1 != 0 {
            (0x10, u16::from(crate::interrupts::IDT_VECTOR_PIT))
        } else if icr & 8 != 0 {
            (8, u16::from(crate::interrupts::IDT_VECTOR_RTC))
        } else {
            return Ok(());
        };
        self.write(ICR_ADDR, icr & !pending_bit);
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
        let awake: Vec<_> = self
            .syscall_state
            .sleeping
            .iter()
            .filter_map(|(id, deadline)| (*deadline <= self.syscall_state.ticks).then_some(*id))
            .collect();
        for id in awake {
            self.syscall_state.sleeping.remove(&id);
            self.syscall_state.pending_results.insert(id, 0);
            let _ = self.scheduler.wake(id);
            if self.scheduler.current_task().is_none() {
                if let Ok(context) = self.scheduler.activate(id) {
                    self.restore_task_context(&context);
                }
            }
        }
        let period = self.read_u16(TIMER_PERIOD_ADDR);
        if period != self.timer_period {
            self.timer_period = period;
            self.timer_countdown = period;
        }
        if period == 0 {
            self.timer_countdown = 0;
            return;
        }
        if self.timer_countdown <= 1 {
            self.timer_countdown = period;
            let icr = self.read(ICR_ADDR) | 0x10;
            self.write(ICR_ADDR, icr);
        } else {
            self.timer_countdown -= 1;
        }
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

fn gpu3d_sincos(angle: u8) -> (i32, i32) {
    let index = usize::from((angle >> 2) & 63);
    (GPU3D_SIN_8_8[index], GPU3D_SIN_8_8[(index + 16) & 63])
}

fn gpu3d_transform(
    [mut x, mut y, mut z]: [i32; 3],
    angles: [u8; 3],
    translation: [i32; 3],
) -> (i32, i32, i32) {
    let (sin_x, cos_x) = gpu3d_sincos(angles[0]);
    let (sin_y, cos_y) = gpu3d_sincos(angles[1]);
    let (sin_z, cos_z) = gpu3d_sincos(angles[2]);
    (y, z) = ((y * cos_x - z * sin_x) >> 8, (y * sin_x + z * cos_x) >> 8);
    (x, z) = ((x * cos_y + z * sin_y) >> 8, (-x * sin_y + z * cos_y) >> 8);
    (x, y) = ((x * cos_z - y * sin_z) >> 8, (x * sin_z + y * cos_z) >> 8);
    (x + translation[0], y + translation[1], z + translation[2])
}

fn gpu3d_project(
    (x, y, depth): (i32, i32, i32),
    focal: i32,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let projected_x = i64::from(x) * i64::from(focal);
    let projected_y = i64::from(y) * i64::from(focal);
    let screen_x = i64::from(width / 2) + (projected_x.div_euclid(i64::from(depth)) >> 8);
    let screen_y = i64::from(height / 2) - (projected_y.div_euclid(i64::from(depth)) >> 8);
    (
        screen_x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        screen_y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_with_program(program: &[u16]) -> Cpu {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        for (index, word) in program.iter().enumerate() {
            cpu.write_u16((index * 2) as u32, *word);
        }
        cpu
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
        let source = "SVC\n".repeat(32) + "HALT";
        let image = crate::assembler::assemble(&source, 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        install_test_handler(&mut cpu, crate::isa::SVC_VECTOR, 0x300, 3);
        let task = cpu.create_user_task(0x100, 0x7000, 0xf000, None).unwrap();
        cpu.start_task(task).unwrap();
        (cpu, task)
    }

    fn invoke_syscall(cpu: &mut Cpu, number: SyscallNumber, arguments: [i64; 6]) {
        cpu.registers[crate::syscall::REGISTER_NUMBER] = number as i64;
        cpu.registers[crate::syscall::REGISTER_ARGUMENT_0..=crate::syscall::REGISTER_ARGUMENT_5]
            .copy_from_slice(&arguments);
        cpu.registers[crate::syscall::REGISTER_ABI_VERSION] = crate::syscall::ABI_VERSION;
        cpu.step().unwrap();
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
        let mut cpu = Cpu::with_memory_size(1 << 19).unwrap();

        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.registers, [0; 8]);
        assert_eq!(cpu.sp, 0x7ffff);
        assert_eq!(cpu.kernel_sp, cpu.sp);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.flags, Flags::default());
        assert_eq!(cpu.read(ICR_ADDR) & 1, 0);
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
        cpu.write(ICR_ADDR, 1);

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
        assert_eq!(cpu.read(ICR_ADDR) & 1, 1);
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
        cpu.write_u16(0, (OP_ST << 11) | (1 << 5));
        cpu.write_u16(2, (OP_LD << 11) | (2 << 8) | (1 << 5));
        cpu.write_u16(0x300, OP_RTI << 11);
        cpu.registers[0] = 0x10;
        cpu.registers[1] = i64::from(ICR_ADDR);
        cpu.sp = 0x7000;
        cpu.kernel_sp = 0xf000;
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.write(ICR_ADDR, 1);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.memory[ICR_ADDR as usize], 0);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PRIVILEGE_VIOLATION);

        cpu.write_u32(cpu.sp, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        cpu.write(ICR_ADDR, 1);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.registers[2], 0);
        assert_eq!(cpu.read_u32(cpu.sp), 2);
    }

    #[test]
    fn user_instruction_can_set_safe_gpu_controls_without_accessing_interrupt_control() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write_u16(0, (OP_ST << 11) | (1 << 5));
        cpu.write_u16(2, (OP_ST << 11) | (1 << 5));
        cpu.write_u16(4, (OP_ST << 11) | (1 << 5));
        cpu.registers[0] = 1;
        cpu.registers[1] = i64::from(GPU_SPEED_ADDR);
        cpu.enter_user_mode(0, 0x7000).unwrap();

        cpu.step().unwrap();
        cpu.registers[1] = VSYNC_ADDR as i64;
        cpu.step().unwrap();
        cpu.registers[1] = i64::from(GPU_MODE_ADDR);
        cpu.step().unwrap();

        assert_eq!(cpu.read(GPU_SPEED_ADDR), 1);
        assert_eq!(cpu.memory[VSYNC_ADDR], 1);
        assert_eq!(cpu.screen_size(), (640, 480));
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
        cpu.registers[2] = i64::from(CONSOLE_DATA);
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
        let task = cpu.create_user_task(0, 0x70000, 0xf0000, None).unwrap();
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
    fn compiled_c_syscall_wrappers_execute_through_svc() {
        let source = "#include <sc16_syscall.h>\n\
                      int main(void) {\n\
                          long semaphore;\n\
                          if (sc16_write(1, \"ok\", 2) != 2) return 7;\n\
                          if (sc16_sleep(1UL) != 0) return 9;\n\
                          if (sc16_getpid() < 0L) return 8;\n\
                          if (sc16_mmap(4096UL, SC16_PROT_READ, 0) != -13L) return 14;\n\
                          if (sc16_spawn(\"missing\") != -13L) return 12;\n\
                          semaphore = sc16_semop(SC16_SEM_CREATE, 0UL, 1);\n\
                          if (semaphore < 0L) return 10;\n\
                          if (sc16_semop(SC16_SEM_DESTROY, (unsigned long)semaphore, 0) != 0L) return 11;\n\
                          if (sc16_semop(SC16_SEM_DESTROY, 0UL, 0) != -22L) return 13;\n\
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
        let task = cpu.create_user_task(0, 0x70000, 0xf0000, None).unwrap();
        cpu.start_task(task).unwrap();

        cpu.run(10_000).unwrap();

        assert!(!cpu.running, "compiled syscall program did not terminate");
        assert_eq!(cpu.registers[0], 0);
        assert_eq!(cpu.console.output(), b"ok");
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
        let task = cpu.create_user_task(0, 0x70000, 0xf0000, None).unwrap();
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
        let disk_root = folder_disk_for_test("syscall-spawn", false);
        let child_source = "LDI R0, 4\nLDI R1, 4096\nLDI R2, 3\nLDI R3, 0\nLDI R7, 1\nSVC\nHALT";
        let child_assembly = crate::assembler::assemble(child_source, 0x1000).unwrap();
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
            crate::assembler::assemble("SVC\nSVC\nSVC\nSVC\nHALT", 0x100).unwrap();
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
        cpu.memory[0x40_000 + 0x100..0x40_000 + 0x10a]
            .copy_from_slice(&parent_assembly.segments[0].data[0..10]);
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
        cpu.registers[0] = SyscallNumber::Spawn as i64;
        cpu.registers[1] = 0x2000;
        cpu.registers[7] = crate::syscall::ABI_VERSION;

        cpu.step().unwrap();
        let child = cpu.registers[0] as u32;
        assert_ne!(child, parent);
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
        invoke_syscall(&mut cpu, SyscallNumber::Mmap, [4096, 3, 0, 0, 0, 0]);
        assert_eq!(cpu.registers[0], 0x2000_0000);

        invoke_syscall(&mut cpu, SyscallNumber::Yield, [0; 6]);
        assert_eq!(cpu.current_task(), Some(child));
        cpu.run(20).unwrap();
        assert_eq!(cpu.registers[0], 0x2000_0000);
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
        assert_eq!(cpu.read_u32(PAGE_FAULT_ADDRESS_ADDR), 0x2000);
        assert_eq!(cpu.read_u16(PAGE_FAULT_INFO_ADDR), 0x0102);
        assert_eq!(cpu.read_u32(cpu.sp), 0);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.sp, 0xe0000);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    }

    #[test]
    fn user_filesystem_services_bound_memory_and_preserve_boot_files() {
        let root = folder_disk_for_test("operations", false);
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.mount_folder_disk(&root).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.memory[0x1000..0x1009].copy_from_slice(b"note.txt\0");
        cpu.memory[0x1100..0x1108].copy_from_slice(b"renamed\0");
        cpu.memory[0x1200] = 0;
        cpu.memory[0x2000..0x2003].copy_from_slice(b"abc");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 3);
        cpu.write(DISK_BASE, FOLDER_FS_WRITE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(cpu.disk_register_u16(14), 3);
        assert_eq!(std::fs::read(root.join("note.txt")).unwrap(), b"abc");
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);

        cpu.write_u32(DISK_BASE + 26, 0x3000);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(&cpu.memory[0x3000..0x3003], b"abc");
        assert_eq!(
            u32::from_be_bytes(
                cpu.memory[DISK_BASE as usize + 32..DISK_BASE as usize + 36]
                    .try_into()
                    .unwrap()
            ),
            3
        );

        cpu.write_u32(DISK_BASE + 26, 0x3000);
        cpu.write(DISK_BASE, FOLDER_FS_STAT);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(&cpu.memory[0x3000..0x3005], &[0, 0, 0, 0, 3]);

        cpu.write_u32(DISK_BASE + 18, 0x1100);
        cpu.write(DISK_BASE, FOLDER_FS_RENAME);
        assert!(root.join("renamed").is_file());
        assert!(!root.join("note.txt").exists());

        cpu.write_u32(DISK_BASE + 22, 0x1200);
        cpu.write_u16(DISK_BASE + 10, 2);
        cpu.write_u32(DISK_BASE + 26, 0x3000);
        cpu.write_u16(DISK_BASE + 30, 64);
        cpu.write(DISK_BASE, FOLDER_FS_LIST);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(cpu.disk_register_u16(14), 3);
        assert_eq!(&cpu.memory[0x3000..0x3009], b"renamed\0\0");

        cpu.write_u32(DISK_BASE + 22, 0x1100);
        cpu.write(DISK_BASE, FOLDER_FS_DELETE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert!(!root.join("renamed").exists());

        cpu.memory[0x1100..0x1107].copy_from_slice(b"subdir\0");
        cpu.write(DISK_BASE, FOLDER_FS_CREATE_DIR);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert!(root.join("subdir").is_dir());
        cpu.write(DISK_BASE, FOLDER_FS_DELETE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert!(!root.join("subdir").exists());

        cpu.memory[0x1000..0x100b].copy_from_slice(b"kernel.bin\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write(DISK_BASE, FOLDER_FS_WRITE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);

        cpu.write_u32(DISK_BASE + 22, 0xffff_fffc);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);

        cpu.memory[0x1000..0x100b].copy_from_slice(b"../outside\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert!(!root.parent().unwrap().join("outside").exists());

        cpu.memory[0x1000..0x1009].copy_from_slice(b"main.bin\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u16(DISK_BASE + 30, 4097);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert_eq!(&cpu.memory[0x2000..0x2003], b"abc");

        cpu.write_u16(DISK_BASE + 30, 1);
        cpu.write_u32(DISK_BASE + 26, DISK_BASE);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);

        cpu.write(DISK_BASE, 0x04);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn user_filesystem_service_enforces_protected_files_and_capacity() {
        let root = folder_disk_for_test("protected-capacity", false);
        std::fs::write(root.join("locked.txt"), b"xy").unwrap();
        std::fs::write(
            root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Guest FS","capacity_bytes":8,"read_only":false,"protected_files":["locked.txt"]}"#,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.mount_folder_disk(&root).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.memory[0x1000..0x100b].copy_from_slice(b"locked.txt\0");
        cpu.memory[0x100b..0x1014].copy_from_slice(b"note.txt\0");
        cpu.memory[0x2000] = b'z';
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 1);
        cpu.write(DISK_BASE, FOLDER_FS_WRITE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert_eq!(std::fs::read(root.join("locked.txt")).unwrap(), b"xy");

        cpu.write_u32(DISK_BASE + 22, 0x100b);
        cpu.write(DISK_BASE, FOLDER_FS_WRITE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert!(!root.join("note.txt").exists());
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn user_filesystem_service_rejects_read_only_disks() {
        let root = folder_disk_for_test("readonly", true);
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.mount_folder_disk(&root).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.memory[0x1000..0x1009].copy_from_slice(b"note.txt\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 0);
        cpu.write(DISK_BASE, FOLDER_FS_WRITE);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        assert!(!root.join("note.txt").exists());
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn guest_program_executes_a_bounded_user_filesystem_read() {
        let root = folder_disk_for_test("guest-execution", false);
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let image = crate::assembler::assemble("LDA R1, 0x43100\nLDI R0, 0x42\nST R1, R0\nHALT", 0)
            .unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.mount_folder_disk(&root).unwrap();
        cpu.memory[0x1000..0x1009].copy_from_slice(b"main.bin\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 3);
        cpu.privilege_mode = PrivilegeMode::User;
        assert_eq!(cpu.run(10).unwrap(), 4);
        assert!(!cpu.running);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(cpu.disk_register_u16(14), 3);
        assert_eq!(
            &cpu.memory[DISK_BASE as usize + 32..DISK_BASE as usize + 36],
            &[0, 0, 0, 3]
        );
        assert_eq!(&cpu.memory[0x2000..0x2003], &[1, 2, 3]);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
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
    fn wraps_memory_reads_at_configured_limit() {
        let mut cpu = Cpu::with_memory_size(256).unwrap();
        cpu.write(255, 0x12);
        cpu.write(0, 0x34);
        assert_eq!(cpu.read_u16(255), 0x1234);
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
    fn assembled_wide_opcodes_cover_signed_math_and_wrapping_memory() {
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
            "LDI R0, 0xFFFF\nLDI R1, 0xFFFF\nLDI R2, 0x1234\nLDI R3, 0x5678\n\
             ST32 R2, R0\nLD32 R4, R0\nHALT",
        );
        assert_eq!((cpu.registers[4], cpu.registers[5]), (0x1234, 0x5678));
        assert_eq!(&cpu.memory[..3], &[0x34, 0x56, 0x78]);
        assert_eq!(cpu.memory[cpu.memory_mask as usize], 0x12);

        let cpu = run_assembly(
            "LDI30 R0, 0x12345678\nLDI30 R1, 0x3FFFFFFD\nSTP R0, R1\n\
             LDP R2, R1\nHALT",
        );
        assert_eq!(cpu.registers[2], 0x12345678);
        assert_eq!(
            &cpu.memory[cpu.memory_mask as usize - 2..=cpu.memory_mask as usize],
            &[0x12, 0x34, 0x56]
        );
        assert_eq!(cpu.memory[0], 0x78);
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

        let cpu = run_assembly("LDA R0, 0x51234\nHALT");
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

        let cpu = run_assembly("LDA R1, target\nJMPR R1\nLDI R0, 1\nHALT\ntarget: LDI R0, 7\nHALT");
        assert_eq!(cpu.registers[0], 7);

        let cpu = run_assembly("LDA R1, fn\nCALLR R1\nHALT\nfn: LDI R0, 9\nRET");
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
        assert_eq!(cpu.read_u16(0xffff), 0x1234);
        cpu.write_u16(0xffff, 0x5678);
        assert_eq!(cpu.read(0xffff), 0x56);
        assert_eq!(cpu.read(0), 0x78);
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
    fn key_masks_use_sc16_little_byte_order() {
        let mut cpu = Cpu::with_memory_size(1 << 19).unwrap();
        cpu.set_key(0x20_0405, true);
        assert_eq!(
            &cpu.memory[KEYS_ADDR as usize..KEYS_ADDR as usize + 3],
            &[0x05, 0x04, 0x20]
        );
        cpu.set_key(0x20_0405, false);
        assert_eq!(
            &cpu.memory[KEYS_ADDR as usize..KEYS_ADDR as usize + 3],
            &[0, 0, 0]
        );
    }

    #[test]
    fn layered_clear_and_transparency_keep_black_background() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_BASE + 1, 0);
        cpu.write(GPU_BASE, GPU_CLEAR);
        cpu.write(GPU_BASE + 1, 4);
        cpu.write(GPU_BASE, GPU_CLEAR);

        cpu.write(GPU_LAYER_SELECT, 1);
        cpu.write(GPU_LAYER_TRANSPARENCY + 1, 255);
        cpu.write(GPU_BASE + 1, 255);
        cpu.write(GPU_BASE, GPU_CLEAR);
        cpu.write(GPU_BASE + 1, 2);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write(GPU_BASE, GPU_PIXEL);

        let framebuffer = cpu.composited_framebuffer();
        assert_eq!(framebuffer[0], 2);
        assert_eq!(framebuffer[1], 4);
        assert_eq!(framebuffer[framebuffer.len() - 1], 4);
        let mut reused = vec![0; framebuffer.len()];
        cpu.composited_framebuffer_into(&mut reused);
        assert_eq!(reused, framebuffer);
    }

    #[test]
    fn gpu_palette_command_updates_selected_color() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_BASE + 1, 42);
        cpu.write_u16(GPU_BASE + 2, 10);
        cpu.write_u16(GPU_BASE + 4, 20);
        cpu.write_u16(GPU_BASE + 6, 30);
        cpu.write(GPU_BASE, GPU_PALETTE);
        assert_eq!(cpu.palette_color(42), [10, 20, 30]);
    }

    #[test]
    fn gpu_sound_commands_capture_unsigned_parameters_per_channel() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_BASE + 1, 5);
        cpu.write_u16(GPU_BASE + 2, 440);
        cpu.write_u16(GPU_BASE + 4, 60);
        cpu.write_u16(GPU_BASE + 6, 2);
        cpu.write_u16(GPU_BASE + 8, 75);
        cpu.write(GPU_BASE, GPU_SOUND);
        cpu.write(GPU_BASE + 1, 2);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write(GPU_BASE, GPU_SOUND);

        assert_eq!(
            cpu.take_sound_commands(),
            [
                GpuSoundCommand {
                    channel: 1,
                    frequency: 440,
                    frames: 60,
                    waveform: 2,
                    volume: 75,
                },
                GpuSoundCommand {
                    channel: 2,
                    frequency: 0,
                    frames: 60,
                    waveform: 2,
                    volume: 75,
                },
            ]
        );
        assert!(cpu.take_sound_commands().is_empty());
    }

    #[test]
    fn tile_upload_and_tilemap_render_indexed_pixels() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        for index in 0..256 {
            cpu.memory[0x1000 + index] = index as u8;
        }
        cpu.write_u16(GPU_BASE + 10, 0x1000);
        cpu.write(GPU_BASE + 1, 1);
        cpu.write(GPU_BASE, GPU_TILE_UPLOAD);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.memory[0x2000] = 1;
        cpu.write_u16(GPU_BASE + 10, 0x2000);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write_u16(GPU_BASE + 6, 1);
        cpu.write_u16(GPU_BASE + 8, 1);
        cpu.write(GPU_BASE, GPU_TILEMAP);

        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert_eq!(cpu.layers[1][0], 0);
        assert_eq!(cpu.layers[1][15], 15);
        assert_eq!(cpu.layers[1][320], 16);
        assert_eq!(cpu.layers[1][15 * 320 + 15], 255);
        assert_eq!(cpu.layers[1][16], 0);
    }

    #[test]
    fn finland_sized_tilemap_upload_preserves_all_ten_thousand_tiles() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let expected: Vec<u8> = (0..10_000).map(|index| (index % 256) as u8).collect();
        cpu.memory[0x4000..0x4000 + expected.len()].copy_from_slice(&expected);
        cpu.write_u16(GPU_BASE + 10, 0x4000);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write_u16(GPU_BASE + 6, 100);
        cpu.write_u16(GPU_BASE + 8, 100);
        cpu.write(GPU_BASE, GPU_TILEMAP);

        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert_eq!((cpu.tilemap_width, cpu.tilemap_height), (100, 100));
        assert_eq!(cpu.tilemap, expected);
    }

    #[test]
    fn invalid_tilemap_dimensions_return_error_without_replacing_map() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.tilemap = vec![3];
        cpu.tilemap_width = 1;
        cpu.tilemap_height = 1;
        cpu.write_u16(GPU_BASE + 6, 0);
        cpu.write_u16(GPU_BASE + 8, 1);
        cpu.write(GPU_BASE, GPU_TILEMAP);

        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);
        assert_eq!(cpu.tilemap, [3]);
    }

    #[test]
    fn sprite_upload_and_move_render_transparency_and_horizontal_flip() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x1000..0x1004].copy_from_slice(&[1, 0, 2, 3]);
        cpu.write_u16(GPU_BASE + 10, 0x1000);
        cpu.write(GPU_BASE + 1, 0);
        cpu.write_u16(GPU_BASE + 6, 2);
        cpu.write_u16(GPU_BASE + 8, 2);
        cpu.write(GPU_BASE, GPU_SPRITE_UPLOAD);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.write_u16(GPU_BASE + 2, 5);
        cpu.write_u16(GPU_BASE + 4, 6);
        cpu.write_u16(GPU_BASE + 6, 3);
        cpu.write(GPU_BASE, GPU_SPRITE_MOVE);

        let row_start = 6 * 320 + 5;
        assert_eq!(&cpu.layers[2][row_start..row_start + 2], &[0, 1]);
        let next_row = row_start + 320;
        assert_eq!(&cpu.layers[2][next_row..next_row + 2], &[3, 2]);
    }

    #[test]
    fn collision_masks_check_opaque_pixels_and_report_missing_masks() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x1000..0x1004].copy_from_slice(&[1, 0, 0, 0]);
        cpu.memory[0x1100..0x1104].copy_from_slice(&[0, 1, 0, 0]);
        for (slot, source) in [(0, 0x1000), (1, 0x1100)] {
            cpu.write_u16(GPU_BASE + 10, source);
            cpu.write(GPU_BASE + 1, slot);
            cpu.write_u16(GPU_BASE + 6, 2);
            cpu.write_u16(GPU_BASE + 8, 2);
            cpu.write(GPU_BASE, GPU_SPRITE_UPLOAD);
            assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        }

        cpu.write_u16(GPU_BASE + 10, 0);
        cpu.write_u16(GPU_BASE + 12, 1);
        cpu.write_u16(GPU_BASE + 2, 10);
        cpu.write_u16(GPU_BASE + 4, 20);
        cpu.write_u16(GPU_BASE + 6, 10);
        cpu.write_u16(GPU_BASE + 8, 20);
        cpu.write(GPU_BASE, GPU_MASK_OVERLAP);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.write_u16(GPU_BASE + 6, 9);
        cpu.write(GPU_BASE, GPU_MASK_OVERLAP);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);

        cpu.write_u16(GPU_BASE + 12, 63);
        cpu.write(GPU_BASE, GPU_MASK_OVERLAP);
        assert_eq!(
            &cpu.memory[(GPU_BASE + 14) as usize..(GPU_BASE + 16) as usize],
            &[255, 255]
        );
    }

    #[test]
    fn wide_collision_masks_support_signed_32_bit_coordinates() {
        let first = (2, 2, vec![1, 0, 0, 0]);
        let second = (2, 2, vec![0, 1, 0, 0]);
        assert!(Cpu::masks_overlap(&first, -70_000, 0, &second, -70_001, 0));
        assert!(!Cpu::masks_overlap(
            &first,
            i32::MAX,
            0,
            &second,
            i32::MIN,
            0
        ));
    }

    #[test]
    fn world_rect_and_ammo_draw_relative_to_the_active_camera() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_LAYER_SELECT, 1);
        cpu.write_u16(GPU_BASE + 2, 10);
        cpu.write_u16(GPU_BASE + 4, 20);
        cpu.write(GPU_BASE, GPU_TILE_CAMERA);

        cpu.write(GPU_BASE + 1, 6);
        cpu.write_u16(GPU_BASE + 2, 11);
        cpu.write_u16(GPU_BASE + 4, 21);
        cpu.write_u16(GPU_BASE + 6, 2);
        cpu.write_u16(GPU_BASE + 8, 1);
        cpu.write(GPU_BASE, GPU_WORLD_RECT);
        assert_eq!(cpu.layers[1][320 + 1], 6);
        assert_eq!(cpu.layers[1][320 + 2], 6);
        assert_eq!(cpu.layers[1][320], 0);

        cpu.write_u16(GPU_BASE + 2, 13);
        cpu.write_u16(GPU_BASE + 4, 22);
        cpu.write(GPU_BASE, GPU_WORLD_AMMO);
        assert_eq!(cpu.layers[1][2 * 320 + 3], 1);
        assert_eq!(cpu.layers[1][2 * 320 + 5], 5);
    }

    #[test]
    fn world_tiles_preserve_zero_transparency_and_particle_inputs_validate() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let tile_start = 2 * GPU_TILE_MAX_SIZE * GPU_TILE_MAX_SIZE;
        cpu.tile_memory[tile_start..tile_start + 16 * 16].fill(0);
        cpu.tile_memory[tile_start] = 9;
        cpu.write(GPU_LAYER_SELECT, 2);
        cpu.write(GPU_BASE + 1, 2);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write(GPU_BASE, GPU_WORLD_TILE);
        assert_eq!(cpu.layers[2][0], 9);
        assert_eq!(cpu.layers[2][1], 0);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.write(GPU_BASE + 1, 5);
        cpu.write_u16(GPU_BASE + 2, 20);
        cpu.write_u16(GPU_BASE + 4, 20);
        cpu.write_u16(GPU_BASE + 6, 1);
        cpu.write_u16(GPU_BASE + 8, 16);
        cpu.write(GPU_BASE, GPU_WORLD_PARTICLE);
        assert_eq!(cpu.layers[2][20 * 320 + 20], 5);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.write_u16(GPU_BASE + 6, 33);
        cpu.write(GPU_BASE, GPU_WORLD_PARTICLE);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);
    }

    #[test]
    fn world_emitters_draw_fire_smoke_and_uploaded_wreck_masks() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_LAYER_SELECT, 2);
        cpu.memory[0x1000..0x1008].copy_from_slice(&[0, 100, 0, 100, 0, 1, 0xff, 0xff]);
        cpu.write_u16(GPU_BASE + 10, 0x1000);
        cpu.write_u16(GPU_BASE + 2, 1);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write(GPU_BASE, GPU_WORLD_EMITTERS);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert!(cpu.layers[2][100 * 320 + 100] != 0);

        let tile_mask = GPU_SPRITE_SLOTS as u16;
        cpu.tile_memory[..16 * 16].fill(1);
        cpu.memory[0x1010..0x1018].copy_from_slice(&[0, 120, 0, 120, 0, 1, 0, tile_mask as u8]);
        cpu.write_u16(GPU_BASE + 10, 0x1010);
        cpu.write_u16(GPU_BASE + 2, 1);
        cpu.write(GPU_BASE, GPU_WORLD_EMITTERS);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert_eq!(cpu.layers[2][112 * 320 + 112], 10);
    }

    #[test]
    fn world_emitter_commands_reject_invalid_buffers_and_layers() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write_u16(GPU_BASE + 10, 0xffff);
        cpu.write_u16(GPU_BASE + 2, 2);
        cpu.write(GPU_BASE, GPU_WORLD_EMITTERS);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);

        cpu.write(GPU_LAYER_SELECT, 0);
        cpu.write_u16(GPU_BASE + 10, 0);
        cpu.write_u16(GPU_BASE + 2, 0);
        cpu.write(GPU_BASE, GPU_WORLD_EMITTERS);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);
    }

    #[test]
    fn wide_world_emitters_use_camera_adjusted_32_bit_coordinates() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_LAYER_SELECT, 2);
        cpu.write_u16(GPU_BASE + 2, 69_990u32 as u16);
        cpu.write_u16(GPU_BASE + 24, 1);
        cpu.write_u16(GPU_BASE + 4, 90);
        cpu.write_u16(GPU_BASE + 26, 0);
        cpu.write(GPU_BASE, GPU_TILE_CAMERA_WIDE);
        assert_eq!(cpu.tile_camera, (69_990, 90));
        cpu.memory[0x2000..0x200c]
            .copy_from_slice(&[0, 1, 0x11, 0x70, 0, 0, 0, 100, 0, 1, 0xff, 0xff]);
        cpu.write_u16(GPU_BASE + 10, 0x2000);
        cpu.write_u16(GPU_BASE + 2, 1);
        cpu.write_u16(GPU_BASE + 4, 0);
        cpu.write(GPU_BASE, GPU_WORLD_EMITTERS_WIDE);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert!(cpu.layers[2][10 * 320 + 10] != 0);
    }

    #[test]
    fn disk_mmio_reads_virtual_files_and_lists_module_disk_contents() {
        let root = std::env::temp_dir().join(format!(
            "sc16-cpu-files-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("module")).unwrap();
        std::fs::write(root.join("module").join("hello.txt"), b"hello world").unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.configure_disk(root.join("disks"), root.join("module"))
            .unwrap();
        cpu.memory[0x1000..0x100a].copy_from_slice(b"hello.txt\0");
        cpu.write_u32(DISK_BASE + 18, 6);
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x1100);
        cpu.write_u16(DISK_BASE + 30, 4);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(cpu.disk_register_u16(14), 4);
        assert_eq!(&cpu.memory[0x1100..0x1104], b"worl");
        assert_eq!(cpu.disk_register_u32(32), 11);

        cpu.write_u16(DISK_BASE + 10, 0);
        cpu.write_u32(DISK_BASE + 26, 0x1200);
        cpu.write_u16(DISK_BASE + 30, 32);
        cpu.write(DISK_BASE, 0x80);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(&cpu.memory[0x1200..0x120a], b"hello.txt\0");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_mmio_reads_large_module_files_in_chunks_and_lists_only_module_files() {
        let root = std::env::temp_dir().join(format!(
            "sc16-cpu-file-chunks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let module_files = root.join("modules").join("tank16").join("disks");
        let other_files = root.join("modules").join("other").join("disks");
        std::fs::create_dir_all(module_files.join("maps")).unwrap();
        std::fs::create_dir_all(&other_files).unwrap();
        let payload = (0u8..=255).cycle().take(76_800).collect::<Vec<u8>>();
        std::fs::write(module_files.join("maps").join("world.json"), &payload).unwrap();
        std::fs::write(module_files.join("index.txt"), b"index").unwrap();
        std::fs::write(other_files.join("secret.json"), b"secret").unwrap();

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.configure_disk(root.join("disks"), module_files)
            .unwrap();
        cpu.memory[0x1000..0x1010].copy_from_slice(b"maps/world.json\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 4096);

        for offset in [0, 4096, 65_530, 72_704, 76_800] {
            cpu.write_u32(DISK_BASE + 18, offset);
            cpu.write(DISK_BASE, FOLDER_FS_READ);
            let expected = &payload[offset as usize..(offset as usize + 4096).min(payload.len())];
            assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
            assert_eq!(
                cpu.disk_register_u16(14) as usize,
                expected.len(),
                "file offset {offset}"
            );
            assert_eq!(
                cpu.disk_register_u32(32),
                payload.len() as u32,
                "reported file length at offset {offset}"
            );
            assert_eq!(
                &cpu.memory[0x2000..0x2000 + expected.len()],
                expected,
                "file bytes at offset {offset}"
            );
        }

        cpu.write_u16(DISK_BASE + 10, 0);
        cpu.write_u32(DISK_BASE + 26, 0x3000);
        cpu.write_u16(DISK_BASE + 30, 32);
        cpu.write(DISK_BASE, 0x80);
        assert_eq!(cpu.disk_register_u16(14), 2);
        assert_eq!(&cpu.memory[0x3000..0x300a], b"index.txt\0");

        cpu.write_u16(DISK_BASE + 10, 1);
        cpu.write(DISK_BASE, 0x80);
        assert_eq!(&cpu.memory[0x3000..0x3010], b"maps/world.json\0");

        cpu.memory[0x1000..0x100a].copy_from_slice(b"../secret\0");
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);
        std::fs::remove_dir_all(root).unwrap();
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

        let module_files = root.join("module");
        std::fs::create_dir_all(&module_files).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.configure_disk(storage, module_files).unwrap();
        cpu.pc = 0x1234;
        cpu.memory[0x1234] = 0x5a;
        assert_eq!(cpu.available_disks().unwrap(), ["DATA0001"]);
        assert!(cpu.mount_data_disk("MISSING1").is_err());
        assert!(cpu.mount_data_disk("../DATA0001").is_err());
        cpu.mount_data_disk("DATA0001").unwrap();
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.memory[0x1234], 0x5a);

        cpu.memory[0x1000..0x100c].copy_from_slice(b"payload.bin\0");
        cpu.write_u32(DISK_BASE + 22, 0x1000);
        cpu.write_u32(DISK_BASE + 26, 0x2000);
        cpu.write_u16(DISK_BASE + 30, 256);
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(&cpu.memory[0x2000..0x2100], &[0xa5; 256]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gpu_network_commands_open_query_send_and_close_peer() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.set_network_port(0);
        cpu.memory[0x1000..0x1005].copy_from_slice(b"game\0");
        cpu.write_u16(GPU_BASE + 10, 0x1000);
        cpu.write(GPU_BASE, GPU_NET_OPEN);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);

        cpu.write(GPU_BASE + 1, 0);
        cpu.write(GPU_BASE, GPU_NET_INFO);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);
        cpu.write(GPU_BASE + 1, 1);
        cpu.write(GPU_BASE, GPU_NET_INFO);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);

        cpu.memory[0x1100..0x1104].copy_from_slice(b"data");
        cpu.write_u16(GPU_BASE + 10, 0x1100);
        cpu.write_u16(GPU_BASE + 2, 4);
        cpu.write(GPU_BASE, GPU_NET_SEND);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 1);

        cpu.write(GPU_BASE, GPU_NET_CLOSE);
        cpu.write(GPU_BASE + 1, 6);
        cpu.write(GPU_BASE, GPU_NET_INFO);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
    }

    #[test]
    fn empty_guest_network_title_is_not_replaced_by_the_bios_node_name() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.set_network_port(0);
        cpu.bios_settings.network_name = "SC16-0123456789A".to_owned();
        cpu.write_u16(GPU_BASE + 10, 0x1000);

        cpu.write(GPU_BASE, GPU_NET_OPEN);

        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        cpu.write(GPU_BASE, GPU_NET_CLOSE);
    }

    #[test]
    fn bios_additional_settings_can_be_read_and_updated_with_typed_values() {
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
            cpu.memory[BIOS_BASE as usize + 2] = 0;
            cpu.memory[BIOS_BASE as usize + 3] = index;
            cpu.bios_read_setting().unwrap();
            let record = BIOS_SETTING_RECORD;
            let name_end = cpu.memory[record..record + 17]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap();
            assert_eq!(&cpu.memory[record..record + name_end], name.as_bytes());
            assert_eq!(cpu.memory[record + 17], kind);
            let value_bytes = &cpu.memory[record + 18..record + 42];
            let value_end = value_bytes.iter().position(|byte| *byte == 0).unwrap();
            assert_eq!(&value_bytes[..value_end], value.as_bytes());
        }

        let record = BIOS_SETTING_RECORD;
        cpu.memory[BIOS_BASE as usize + 3] = 0;
        cpu.bios_read_setting().unwrap();
        cpu.memory[record + 17] = 0;
        cpu.memory[record + 18..record + 42].fill(0);
        cpu.memory[record + 18..record + 21].copy_from_slice(b"Yes");
        cpu.bios_write_setting().unwrap();
        assert_eq!(
            cpu.bios_settings.additional.get("enabled"),
            Some(&BiosSettingValue::Boolean(true))
        );

        cpu.memory[BIOS_BASE as usize + 3] = 2;
        cpu.bios_read_setting().unwrap();
        cpu.memory[record + 17] = 2;
        cpu.memory[record + 18..record + 42].fill(0);
        cpu.memory[record + 18..record + 20].copy_from_slice(b"-9");
        cpu.bios_write_setting().unwrap();
        assert_eq!(
            cpu.bios_settings.additional.get("retries"),
            Some(&BiosSettingValue::Integer(-9))
        );
    }

    #[test]
    fn bios_firmware_shows_selected_disk_manifest_and_boot_metadata() {
        let root = std::env::temp_dir().join(format!(
            "sc16-bios-disk-info-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk_path = root.join("APP00001");
        std::fs::create_dir_all(&disk_path).unwrap();
        std::fs::write(
            disk_path.join("disk.json"),
            r#"{"version":1,"id":"APP00001","label":"Friendly App","capacity_bytes":123456,"read_only":false,"protected_files":["main.bin"]}"#,
        )
        .unwrap();
        let image = crate::assembler::assemble("start: HALT\n", 0).unwrap();
        crate::image::write_image(&disk_path.join("main.bin"), &image).unwrap();

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.disk_root = root.clone();
        cpu.memory[BIOS_BASE as usize + 6..BIOS_BASE as usize + 10]
            .copy_from_slice(&0x5080u32.to_be_bytes());
        cpu.memory[BIOS_BASE as usize + 10..BIOS_BASE as usize + 12]
            .copy_from_slice(&128u16.to_be_bytes());
        cpu.bios_disk_info().unwrap();
        let firmware = crate::assembler::assemble(include_str!("../bios.asm"), 0).unwrap();
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
        assert_eq!(cpu.memory[BIOS_STATUS as usize], 0);
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
        let image = crate::assembler::assemble(include_str!("../bios.asm"), 0).unwrap();
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
                if cpu.memory[KEYBOARD_ADDR as usize] == 0 {
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
        assert_eq!(cpu.memory[BIOS_SETTING_RECORD + 17], 2);
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
        assert_eq!(cpu.bios_register_u16(2), 0);
        send_key(&mut cpu, 0x89);
        assert_eq!(
            cpu.memory[BIOS_STATUS as usize],
            0,
            "{:?}",
            cpu.console.text_snapshot()
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
    fn gpu_3d_projects_scene_triangles_for_macroquad() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let scene = [
            3,
            1,
            0,
            0,
            0,
            0,
            0,
            1000,
            10000,
            GPU3D_FILL as i16,
            -1000,
            -1000,
            0,
            0,
            1000,
            0,
            1000,
            -1000,
            0,
            0,
            1,
            2,
            2,
        ];
        for (index, value) in scene.into_iter().enumerate() {
            cpu.write_u16(0x1000 + (index * 2) as u32, value as u16);
        }
        cpu.write(GPU_BASE + 10, 0x10);
        cpu.write(GPU_BASE + 11, 0);
        cpu.write(GPU_BASE, GPU_3D);
        assert_eq!(cpu.memory[(GPU_BASE + 15) as usize], 0);
        assert_eq!(
            cpu.gpu_triangles,
            [GpuTriangle {
                points: [(120, 160), (160, 81), (199, 160)],
                color: 2,
            }]
        );
    }

    #[test]
    fn binary16_conversion_rounds_and_handles_special_values() {
        assert_eq!(f16_to_f32(f32_to_f16(1.5)), 1.5);
        assert_eq!(f16_to_f32(f32_to_f16(f32::INFINITY)), f32::INFINITY);
        assert!(f16_to_f32(f32_to_f16(f32::NAN)).is_nan());
        assert_eq!(f32_to_f16(1.0 + f32::EPSILON), 0x3c00);
    }

    #[test]
    fn gpu_rect_command_updates_vram_and_present_yields() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GPU_BASE + 1, 7);
        cpu.write_u16(GPU_BASE + 2, 1);
        cpu.write_u16(GPU_BASE + 4, 2);
        cpu.write_u16(GPU_BASE + 6, 2);
        cpu.write_u16(GPU_BASE + 8, 2);
        assert_eq!(cpu.gpu_word(2), 1);
        assert_eq!(cpu.gpu_word(4), 2);
        cpu.write(GPU_BASE, GPU_RECT);
        assert_eq!(cpu.memory[(VRAM_START + 641) as usize], 7);
        assert_eq!(cpu.memory[(VRAM_START + 642) as usize], 7);
        assert_eq!(cpu.memory[(VRAM_START + 961) as usize], 7);
        cpu.write(GPU_BASE, GPU_PRESENT);
        assert!(cpu.frame_yield);
        assert!(cpu.frame_presented);
        cpu.begin_frame();
        assert!(!cpu.frame_yield);
        assert!(!cpu.frame_presented);
    }

    #[test]
    fn console_mmio_reads_and_keyboard_events_are_stateful() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.receive_console_input(b"A");
        assert_eq!(cpu.read(CONSOLE_STATUS), 3);
        assert_eq!(cpu.read(CONSOLE_DATA), b'A');
        assert_eq!(cpu.read(CONSOLE_STATUS), 2);
        assert_eq!(cpu.console.output(), b"A");
        assert!(cpu.frame_yield);
        assert!(!cpu.frame_presented);
        cpu.frame_yield = false;
        cpu.signal_keyboard_event(b'B');
        assert_eq!(cpu.memory[KEYBOARD_ADDR as usize], b'B');
        assert_eq!(cpu.read(KEYBOARD_ADDR), b'B');
        assert_eq!(cpu.memory[KEYBOARD_ADDR as usize], 0);
        assert_eq!(cpu.read(ICR_ADDR) & 2, 2);
    }

    #[test]
    fn guest_text_mmio_controls_display_and_writes_to_selected_cursor() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write(GUEST_TEXT_FOREGROUND, 3);
        cpu.write(GUEST_TEXT_BACKGROUND, 4);
        cpu.write(GUEST_TEXT_ROW, 1);
        cpu.write(GUEST_TEXT_COLUMN, 2);
        cpu.write(GUEST_TEXT_CONTROL, GUEST_TEXT_ENABLE);
        cpu.write(GUEST_TEXT_DATA, b'A');

        let cell = cpu.guest_text.cells[crate::guest_text::GUEST_TEXT_COLUMNS + 2];
        assert_eq!(cell.character, b'A');
        assert_eq!(cell.foreground, 3);
        assert_eq!(cell.background, 4);
        assert_eq!(cpu.guest_text.cursor_row, 1);
        assert_eq!(cpu.guest_text.cursor_column, 3);
        assert!(cpu.guest_text.enabled);

        cpu.write(GUEST_TEXT_CONTROL, GUEST_TEXT_CLEAR);
        assert_eq!(cpu.guest_text.cursor_row, 0);
        assert_eq!(cpu.guest_text.cursor_column, 0);
        assert!(
            cpu.guest_text
                .cells
                .iter()
                .all(|cell| cell.character == b' ')
        );
        assert!(cpu.guest_text.cells.iter().all(|cell| cell.background == 4));

        cpu.write(GUEST_TEXT_CONTROL, GUEST_TEXT_DISABLE);
        assert!(!cpu.guest_text.enabled);
    }

    #[test]
    fn console_mmio_bounds_input_and_clears_overrun_status() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.receive_console_input(&vec![b'x'; crate::console::RX_CAPACITY + 1]);
        assert_eq!(cpu.read(CONSOLE_STATUS), 7);
        cpu.write(CONSOLE_STATUS, 0x04);
        assert_eq!(cpu.read(CONSOLE_STATUS), 3);

        cpu.write(CONSOLE_CONTROL, 0x02);
        for _ in 0..crate::console::RX_CAPACITY {
            cpu.read(CONSOLE_DATA);
        }
        cpu.receive_console_input(b"B");
        assert_eq!(cpu.read(CONSOLE_DATA), b'B');
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
        cpu.write(ICR_ADDR, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x20);
        assert_eq!(cpu.read_u32(cpu.sp), 2);
        assert_eq!(cpu.read_u16(cpu.sp + 6), 2);
        assert_eq!(
            cpu.read_u16(cpu.sp + 12),
            u16::from(crate::interrupts::IDT_VECTOR_KEYBOARD)
        );
        assert_eq!(cpu.read_u16(cpu.sp + 14), 1);
        assert_eq!(cpu.read(ICR_ADDR), 0);
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
            cpu.write(ICR_ADDR, pending);

            cpu.step().unwrap();
            let handler = if pending & 2 != 0 { 0x300 } else { 0x400 };
            assert_eq!(cpu.pc, handler);
            assert_eq!(cpu.read_u32(cpu.sp), 2);
            assert_eq!(cpu.read_u16(cpu.sp + 4), 0b1101);
            assert_eq!(cpu.read_u16(cpu.sp + 6), 2);
            assert_eq!(
                cpu.read(ICR_ADDR),
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
        cpu.write(ICR_ADDR, 0x0a);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x300);
        assert_eq!(cpu.read(ICR_ADDR), 8);

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
        cpu.write_u16(TIMER_PERIOD_ADDR, 2);
        cpu.write(ICR_ADDR, 1);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        cpu.write(ICR_ADDR, 1 | 8);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.read_u32(cpu.sp), 4);
        assert_eq!(cpu.read(ICR_ADDR), 8);
        cpu.write_u16(TIMER_PERIOD_ADDR, 0);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x200);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn pit_period_register_does_not_overlap_gpu_mode_register() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.write_u16(TIMER_PERIOD_ADDR, 2);

        cpu.write(GPU_MODE_ADDR, 1);

        assert_eq!(cpu.read_u16(TIMER_PERIOD_ADDR), 2);
        assert_eq!(cpu.screen_size(), (640, 480));
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
        cpu.write_u16(TIMER_PERIOD_ADDR, 1);
        cpu.write(PIT_CONTROL_ADDR, 0);
        cpu.write(ICR_ADDR, 1);

        cpu.step().unwrap();
        assert_eq!(cpu.pc, 2);
        assert_eq!(cpu.read(ICR_ADDR), 0x11);

        cpu.write(PIT_CONTROL_ADDR, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.read(ICR_ADDR) & 0x10, 0);
        assert_eq!(cpu.read_u32(cpu.sp), 4);
        assert_eq!(
            cpu.read_u16(cpu.sp + 12),
            u16::from(crate::interrupts::IDT_VECTOR_PIT)
        );
        cpu.write_u16(TIMER_PERIOD_ADDR, 0);

        cpu.step().unwrap();

        assert_eq!(cpu.pc, 4);
        assert_eq!(cpu.read(ICR_ADDR) & 1, 1);
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
        cpu.write_u16(TIMER_PERIOD_ADDR, 1);
        cpu.write(ICR_ADDR, 1);
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
        cpu.write_u16(TIMER_PERIOD_ADDR, 1);
        cpu.write(ICR_ADDR, 1);
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
    fn mmu_user_gpu_commands_cannot_bypass_guest_buffer_permissions() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let directory = manager.create_address_space(&mut cpu.memory).unwrap();
        manager
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
        cpu.memory[0x9000..0x9002].copy_from_slice(&0u16.to_be_bytes());
        cpu.enable_mmu(directory).unwrap();
        cpu.enter_user_mode(0x1000, 0x3000).unwrap();
        cpu.memory[(GPU_BASE + 1) as usize] = 0;
        cpu.memory[(GPU_BASE + 6) as usize] = 0;
        cpu.memory[(GPU_BASE + 7) as usize] = 1;
        cpu.memory[(GPU_BASE + 8) as usize] = 0;
        cpu.memory[(GPU_BASE + 9) as usize] = 1;
        cpu.memory[(GPU_BASE + 10) as usize] = 0x40;
        cpu.memory[(GPU_BASE + 11) as usize] = 0;
        cpu.memory[(GPU_BASE + 12) as usize] = 0;
        cpu.memory[(GPU_BASE + 13) as usize] = 0;

        cpu.write(GPU_BASE, GPU_SPRITE_UPLOAD);

        assert!(cpu.sprites[0].is_none());
        assert_eq!(cpu.read_u16(GPU_BASE + 14), 1);
    }

    #[test]
    fn mmu_user_instruction_cannot_reach_mapped_protected_interrupt_registers() {
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
                0x43000,
                0x43,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        cpu.memory[code_frame as usize * crate::mmu::PAGE_SIZE
            ..code_frame as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&((OP_ST << 11) | (1 << 5)).to_be_bytes());
        cpu.memory[handler_frame as usize * crate::mmu::PAGE_SIZE
            ..handler_frame as usize * crate::mmu::PAGE_SIZE + 2]
            .copy_from_slice(&(OP_RTI << 11).to_be_bytes());
        cpu.enable_mmu(directory).unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            EXCEPTION_PRIVILEGE_VIOLATION,
            crate::interrupts::IdtDescriptor {
                handler: 0x4000,
                present: true,
                privilege: 0,
                gate_type: crate::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
        cpu.registers[0] = 0;
        cpu.registers[1] = i64::from(ICR_ADDR);
        cpu.write(ICR_ADDR, 9);
        cpu.enter_user_mode(0x1000, 0x3000).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.memory[ICR_ADDR as usize], 8);
        assert_eq!(cpu.pc, 0x4000);
        assert_eq!(cpu.read_u16(cpu.sp + 12), EXCEPTION_PRIVILEGE_VIOLATION);
    }

    #[test]
    fn mmu_user_filesystem_service_validates_noncontiguous_guest_buffers() {
        let disk_path = folder_disk_for_test("mmu-guest-buffer", false);
        let root = disk_path.parent().unwrap().to_path_buf();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.mount_folder_disk(&disk_path).unwrap();
        let mut manager = crate::mmu::PageMemoryManager::new(cpu.memory.len(), 0x8000).unwrap();
        let directory = manager.create_address_space(&mut cpu.memory).unwrap();
        let path_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x1000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        let first_output_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x3000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        let second_output_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x4000,
                crate::mmu::PagePermissions::USER_DATA,
            )
            .unwrap();
        let code_frame = manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x5000,
                crate::mmu::PagePermissions::USER_CODE,
            )
            .unwrap();
        manager
            .allocate_and_map_page(
                &mut cpu.memory,
                directory,
                0x7000,
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
        let path_start = path_frame as usize * crate::mmu::PAGE_SIZE;
        cpu.memory[path_start..path_start + 9].copy_from_slice(b"main.bin\0");
        let instruction = (OP_ST << 11) | (1 << 5);
        let code_start = code_frame as usize * crate::mmu::PAGE_SIZE;
        cpu.memory[code_start..code_start + 2].copy_from_slice(&instruction.to_be_bytes());
        cpu.memory[DISK_BASE as usize + 22..DISK_BASE as usize + 26]
            .copy_from_slice(&0x1000u32.to_be_bytes());
        cpu.memory[DISK_BASE as usize + 26..DISK_BASE as usize + 30]
            .copy_from_slice(&0x3fffu32.to_be_bytes());
        cpu.memory[DISK_BASE as usize + 30..DISK_BASE as usize + 32]
            .copy_from_slice(&3u16.to_be_bytes());
        cpu.enable_mmu(directory).unwrap();
        cpu.registers[0] = i64::from(FOLDER_FS_READ);
        cpu.registers[1] = i64::from(DISK_BASE);
        cpu.enter_user_mode(0x5000, 0x8000).unwrap();

        cpu.step().unwrap();

        let first_output_start = first_output_frame as usize * crate::mmu::PAGE_SIZE;
        let second_output_start = second_output_frame as usize * crate::mmu::PAGE_SIZE;
        assert_eq!(
            cpu.memory[first_output_start + crate::mmu::PAGE_SIZE - 1],
            1
        );
        assert_eq!(cpu.memory[second_output_start], 2);
        assert_eq!(cpu.memory[second_output_start + 1], 3);
        assert_eq!(cpu.memory[DISK_BASE as usize + 16], 0);

        cpu.memory[DISK_BASE as usize + 26..DISK_BASE as usize + 30]
            .copy_from_slice(&0x6000u32.to_be_bytes());
        cpu.write(DISK_BASE, FOLDER_FS_READ);
        assert_eq!(cpu.memory[DISK_BASE as usize + 16], 1);
        assert_eq!(cpu.memory[0x6000], 0);
        drop(cpu);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_interrupts_leave_pending_sources_untouched() {
        let mut cpu = cpu_with_program(&[(OP_INC << 11) | (3 << 8)]);
        cpu.registers[3] = 7;
        cpu.write(ICR_ADDR, 0x0a);
        cpu.step().unwrap();
        assert_eq!(cpu.registers[3], 8);
        assert_eq!(cpu.pc, 2);
        assert_eq!(cpu.read(ICR_ADDR), 0x0a);
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
