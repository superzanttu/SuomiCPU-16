use memmap2::MmapMut;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::console::ConsoleDevice;
use crate::disk::DiskDevice;
use crate::net::{DEFAULT_PORT, NetNode};

pub const MEMORY_SIZE: usize = 1 << 30;
pub const ADDRESS_MASK: u32 = (MEMORY_SIZE as u32) - 1;

const ICR_ADDR: u32 = 0x43002;
const KEY_VECTOR_ADDR: u32 = 0x0002;
const RTC_VECTOR_ADDR: u32 = 0x0006;
const KEYBOARD_ADDR: u32 = 0x43000;
const KEYS_ADDR: u32 = 0x43004;
const GPU_BASE: u32 = 0x43020;
const GPU_MODE_ADDR: u32 = GPU_BASE - 12;
const GPU_LAYER_SELECT: u32 = GPU_BASE + 16;
const GPU_LAYER_MASK: u32 = GPU_BASE + 17;
const GPU_LAYER_TRANSPARENCY: u32 = GPU_BASE + 18;
const GPU_LAYER_ORDER: u32 = GPU_BASE + 23;
const GPU_LAYER_COUNT: usize = 5;
const GPU_PALETTE: u8 = 27;
const RTC_START: u32 = 0x43010;
const DISK_BASE: u32 = 0x43100;
const DISK_STATUS: u32 = DISK_BASE + 16;
const CONSOLE_DATA: u32 = 0x43200;
const CONSOLE_STATUS: u32 = 0x43201;
const CONSOLE_CONTROL: u32 = 0x43202;
const RAM_START: u32 = 0x40000;
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
const DISK_BLOCK_SIZE: usize = 256;
const DISK_MAX_FILE_READ_SIZE: usize = 4096;
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
    InvalidRegisterPair(u16),
    UnsupportedInstruction(u16),
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
            Self::InvalidRegisterPair(instruction) => {
                write!(f, "invalid register pair in instruction {instruction:#06x}")
            }
            Self::UnsupportedInstruction(instruction) => {
                write!(f, "unsupported instruction {instruction:#06x}")
            }
        }
    }
}

impl std::error::Error for CpuError {}

pub struct Cpu {
    pub memory: MmapMut,
    pub registers: [i64; 8],
    pub pc: u32,
    pub sp: u32,
    pub flags: Flags,
    pub running: bool,
    pub frame_yield: bool,
    pub frame_count: u16,
    pub console: ConsoleDevice,
    pub saved_video: Option<Vec<u8>>,
    pub random_state: u32,
    pub layers: [Vec<u8>; GPU_LAYER_COUNT],
    pub palette: [[u8; 3]; 256],
    pub gpu_triangles: Vec<GpuTriangle>,
    pub gpu_lines: Vec<GpuLine>,
    sound_commands: [Option<GpuSoundCommand>; 4],
    disk: DiskDevice,
    network: Option<NetNode>,
    network_port: u16,
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
        if size > CONSOLE_CONTROL as usize {
            memory[CONSOLE_CONTROL as usize] = 3;
            memory[(GPU_BASE - 12) as usize] = 0;
            memory[(GPU_BASE - 13) as usize] = 2;
            memory[GPU_LAYER_MASK as usize] = (1 << GPU_LAYER_COUNT) - 1;
            for layer in 0..GPU_LAYER_COUNT {
                memory[(GPU_LAYER_ORDER as usize) + layer] = layer as u8;
            }
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
            flags: Flags::default(),
            running: true,
            frame_yield: false,
            frame_count: 0,
            console: ConsoleDevice::new(),
            saved_video: None,
            random_state: 0x16c0_ffee,
            layers: std::array::from_fn(|_| vec![0; 640 * 480]),
            palette,
            gpu_triangles: Vec::new(),
            gpu_lines: Vec::new(),
            sound_commands: [None; 4],
            disk: DiskDevice::new(PathBuf::from("disks")),
            network: None,
            network_port: DEFAULT_PORT,
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
        })
    }

    pub fn load_program(
        &mut self,
        segments: &[MemorySegment],
        entry_point: u32,
    ) -> Result<(), CpuError> {
        for segment in segments {
            let start = segment.address as usize;
            let Some(end) = start.checked_add(segment.data.len()) else {
                return Err(CpuError::SegmentOutOfBounds {
                    address: segment.address,
                    length: segment.data.len(),
                });
            };
            if end > self.memory.len() {
                return Err(CpuError::SegmentOutOfBounds {
                    address: segment.address,
                    length: segment.data.len(),
                });
            }
            self.memory[start..end].copy_from_slice(&segment.data);
        }
        if entry_point as usize >= self.memory.len() {
            return Err(CpuError::InvalidEntryPoint(entry_point));
        }
        self.pc = entry_point;
        Ok(())
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
            _ => self.memory[address as usize],
        }
    }

    pub fn configure_disk(
        &mut self,
        storage_dir: PathBuf,
        module_file_root: PathBuf,
    ) -> Result<(), String> {
        let mut disk = DiskDevice::new(storage_dir);
        disk.set_file_root(module_file_root)?;
        self.disk = disk;
        Ok(())
    }

    pub fn available_disks(&self) -> Result<Vec<String>, String> {
        self.disk.list_disks()
    }

    pub fn mount_data_disk(&mut self, disk_id: &str) -> Result<(), String> {
        if !self.disk.list_disks()?.iter().any(|id| id == disk_id) {
            return Err(format!("virtual data disk {disk_id:?} does not exist"));
        }
        self.disk.load_disk(disk_id)?;
        self.memory[DISK_STATUS as usize] = 0;
        Ok(())
    }

    pub fn set_network_port(&mut self, port: u16) {
        self.network_port = port;
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
        self.memory[DISK_STATUS as usize] = 0;
        if let Err(error) = self.disk_control_inner(command) {
            self.memory[DISK_STATUS as usize] = 1;
            eprintln!("ERROR: Disk operation failed: {error}");
        }
    }

    fn disk_control_inner(&mut self, command: u8) -> Result<(), String> {
        if command & 0x01 != 0 {
            let id_bytes = &self.memory[DISK_BASE as usize + 1..DISK_BASE as usize + 9];
            let disk_id = std::str::from_utf8(id_bytes)
                .map_err(|_| "disk ID must be ASCII".to_owned())?
                .to_owned();
            self.disk.load_disk(&disk_id)?;
            self.disk_write_result(u16::from(self.disk.is_protected(&disk_id)?));
        }
        if command & 0x20 != 0 {
            let protected = self.disk_register_u16(10) != 0;
            self.disk.set_protected(protected)?;
            self.disk_write_result(u16::from(protected));
        }
        if command & 0x02 != 0 {
            self.disk.unload_disk()?;
        }
        if command & 0x04 != 0 {
            let pointer = self.disk_register_u16(10);
            let offset = usize::from(self.disk_register_u16(12));
            let address = RAM_START as usize + offset;
            let block = self.disk.read_block(pointer)?;
            for (index, byte) in block.into_iter().enumerate() {
                self.memory[(address + index) & self.memory_mask as usize] = byte;
            }
        }
        if command & 0x10 != 0 {
            let index = usize::from(self.disk_register_u16(10));
            let offset = usize::from(self.disk_register_u16(12));
            let disks = self.disk.list_disks()?;
            self.disk_write_result(disks.len().min(u16::MAX as usize) as u16);
            let name = disks.get(index).map(String::as_str).unwrap_or("");
            let base = (RAM_START as usize + offset) & self.memory_mask as usize;
            for (position, byte) in name.bytes().chain([0]).enumerate() {
                self.memory[(base + position) & self.memory_mask as usize] = byte;
            }
            let protected = if name.is_empty() {
                0
            } else {
                u8::from(self.disk.is_protected(name)?)
            };
            self.memory[(base + 9) & self.memory_mask as usize] = protected;
        }
        if command & 0x80 != 0 {
            let index = usize::from(self.disk_register_u16(10));
            let buffer_address = self.disk_register_u32(26) as usize;
            let capacity = usize::from(self.disk_register_u16(30));
            let files = self.disk.list_files()?;
            self.disk_write_result(files.len().min(u16::MAX as usize) as u16);
            let name = files.get(index).map(String::as_str).unwrap_or("");
            let encoded = name.as_bytes();
            let length = encoded.len() + 1;
            if length > capacity
                || buffer_address
                    .checked_add(length)
                    .is_none_or(|end| end > self.memory.len())
            {
                return Err("virtual file name buffer is too small".to_owned());
            }
            self.memory[buffer_address..buffer_address + encoded.len()].copy_from_slice(encoded);
            self.memory[buffer_address + encoded.len()] = 0;
        }
        if command & 0x40 != 0 {
            let path_address = self.disk_register_u32(22) as usize;
            let buffer_address = self.disk_register_u32(26) as usize;
            let file_offset = self.disk_register_u32(18);
            let length = usize::from(self.disk_register_u16(30)).min(DISK_MAX_FILE_READ_SIZE);
            let _buffer_end = buffer_address
                .checked_add(length)
                .filter(|end| *end <= self.memory.len())
                .ok_or_else(|| "virtual file address is outside memory".to_owned())?;
            let mut path_bytes = Vec::new();
            let mut terminated = false;
            for offset in 0..=128usize {
                let address = path_address
                    .checked_add(offset)
                    .filter(|address| *address < self.memory.len())
                    .ok_or_else(|| "virtual file path exceeds memory".to_owned())?;
                let byte = self.memory[address];
                if byte == 0 {
                    terminated = true;
                    break;
                }
                path_bytes.push(byte);
            }
            if !terminated {
                return Err("virtual file path is not null-terminated".to_owned());
            }
            let path = std::str::from_utf8(&path_bytes)
                .map_err(|_| "virtual file path must be ASCII".to_owned())?;
            if !path.is_ascii() {
                return Err("virtual file path must be ASCII".to_owned());
            }
            let (data, file_size) = self.disk.read_file(path, file_offset, length)?;
            self.memory[buffer_address..buffer_address + data.len()].copy_from_slice(&data);
            self.disk_write_result(data.len().min(u16::MAX as usize) as u16);
            self.memory[DISK_BASE as usize + 32..DISK_BASE as usize + 36]
                .copy_from_slice(&file_size.to_be_bytes());
        }
        if command & 0x08 != 0 {
            let pointer = self.disk_register_u16(10);
            let offset = usize::from(self.disk_register_u16(12));
            let base = RAM_START as usize + offset;
            let data = (0..DISK_BLOCK_SIZE)
                .map(|index| self.memory[(base + index) & self.memory_mask as usize])
                .collect::<Vec<_>>();
            self.disk.write_block(pointer, &data)?;
        }
        Ok(())
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
                        eprintln!("ERROR: Network open failed: {error}");
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
        match address {
            GPU_BASE => self.gpu_command(value),
            DISK_BASE => self.disk_control(value),
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

    fn execute_muldiv(&mut self, destination: usize) {
        let control = self.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2) & self.memory_mask;
        let operation = control >> 12;
        let source = (control & 7) as usize;
        if operation == 7 {
            let high = self.read_u16(self.pc) & 0x3fff;
            let low = self.read_u16(self.pc.wrapping_add(2));
            self.registers[destination] =
                i64::from(((u32::from(high) << 16) | u32::from(low)) & self.memory_mask);
            self.pc = self.pc.wrapping_add(4) & self.memory_mask;
            return;
        }
        if operation == 8 {
            let address = self.registers[source] as u32 & self.memory_mask;
            let low = self.read_u16(address);
            let high = self.read_u16(address.wrapping_add(2)) & 0x3fff;
            self.registers[destination] = i64::from((u32::from(high) << 16) | u32::from(low));
            return;
        }
        if operation == 9 {
            let address = self.registers[source] as u32 & self.memory_mask;
            let value = self.registers[destination] as u32 & self.memory_mask;
            self.write_u16(address, value as u16);
            self.write_u16(address.wrapping_add(2), (value >> 16) as u16);
            return;
        }
        if operation == 10 {
            let value = self.registers[destination] as u32 & self.memory_mask;
            self.registers[destination] = i64::from(value >> 16);
            self.registers[source] = i64::from(value & 0xffff);
            return;
        }
        if operation == 11 {
            self.registers[destination] = (((self.registers[destination] as u32 & 0x3fff) << 16)
                | (self.registers[source] as u16 as u32))
                as i64;
            return;
        }
        let right = if control & 0x0800 != 0 {
            let value = self.read_u16(self.pc);
            self.pc = self.pc.wrapping_add(2) & self.memory_mask;
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
            _ => return,
        };
        self.registers[destination] = i64::from(result);
        self.flags.zero = result == 0;
        self.flags.carry = carry;
    }

    fn execute_wide(&mut self, instruction: u16, destination: usize) -> Result<(), CpuError> {
        let control = self.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2) & self.memory_mask;
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
            let address = self.register_pair_u32(source) & self.memory_mask;
            if operation == 11 {
                let value = self.read_u32(address);
                self.set_pair(destination, value);
            } else {
                self.write_u32(address, left);
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
        if operation >= 3 {
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

    fn execute_misc(&mut self, destination: usize, value: u16, selector: u16, control: u16) {
        match selector {
            0 => self.registers[destination] = i64::from(value.rotate_left(8)),
            1 => {
                let result = (value & 0xff) | if value & 0x80 != 0 { 0xff00 } else { 0 };
                self.registers[destination] = i64::from(result);
                self.set_flags(result, self.flags.carry, false);
            }
            2 => self.pc = u32::from(value) & self.memory_mask,
            3 => {
                self.push_u32(self.pc);
                self.pc = u32::from(value) & self.memory_mask;
            }
            4 => self.push_u16(self.pack_flags()),
            5 => {
                let flags = self.pop_u16();
                self.restore_flags(flags);
            }
            6 => self.flags.carry = true,
            7 => self.flags.carry = false,
            8 => self.sp = u32::from(value) & self.memory_mask,
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
                    self.push_u32(self.pc);
                }
                self.pc = target & self.memory_mask;
            }
            13 => {
                self.sp = (self.sp & 0xffff) | (u32::from(value & 0x3fff) << 16);
            }
            _ => {}
        }
    }

    fn execute_alu(&mut self, destination: usize) {
        let control = self.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2) & self.memory_mask;
        let operation = control >> 12;
        let left = self.registers[destination] as u16;
        if operation == 15 {
            self.execute_misc(destination, left, control & 0x1f, control);
            return;
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
            return;
        }
        if operation == 7 {
            let result = left.wrapping_neg();
            self.set_flags(result, left != 0, left == 0x8000);
            self.registers[destination] = i64::from(result);
            return;
        }
        let right = if immediate {
            let value = self.read_u16(self.pc);
            self.pc = self.pc.wrapping_add(2) & self.memory_mask;
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
    }

    pub fn step(&mut self) -> Result<(), CpuError> {
        if !self.running {
            return Ok(());
        }

        let instruction = self.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2) & self.memory_mask;
        let opcode = instruction >> 11;
        let rd = ((instruction >> 8) & 7) as usize;
        let rs1 = ((instruction >> 5) & 7) as usize;
        let rs2 = ((instruction >> 2) & 7) as usize;
        let immediate = (instruction & 0xff) as u32;

        match opcode {
            OP_HALT => self.running = false,
            OP_LDI => self.registers[rd] = i64::from(immediate),
            OP_LD => {
                let address = self.registers[rs1] as u32 & self.memory_mask;
                self.registers[rd] = i64::from(self.read(address));
            }
            OP_ST => {
                let address = self.registers[rs1] as u32 & self.memory_mask;
                self.write(address, self.registers[rd] as u8);
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
                    self.pc = immediate & self.memory_mask;
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
                let value = self.read(ICR_ADDR) | 1;
                self.write(ICR_ADDR, value);
            }
            OP_DI => {
                let value = self.read(ICR_ADDR) & !1;
                self.write(ICR_ADDR, value);
            }
            OP_RTI => {
                let saved_flags = self.pop_u16();
                self.restore_flags(saved_flags);
                self.pc = self.pop_u32() & self.memory_mask;
            }
            OP_PUSH => self.push_u16(self.registers[rd] as u16),
            OP_POP => self.registers[rd] = i64::from(self.pop_u16()),
            OP_CALL => {
                self.push_u32(self.pc);
                self.pc = (instruction & 0x7ff) as u32;
            }
            OP_RET => self.pc = self.pop_u32() & self.memory_mask,
            OP_BZ => {
                if self.flags.zero {
                    self.pc = (instruction & 0x7ff) as u32;
                }
            }
            OP_LDW => {
                let address = self.registers[rs1] as u32 & self.memory_mask;
                self.registers[rd] = i64::from(self.read_u16(address));
            }
            OP_STW => {
                let address = self.registers[rs1] as u32 & self.memory_mask;
                self.write_u16(address, self.registers[rd] as u16);
            }
            OP_LDWS => {
                self.registers[rd] =
                    i64::from(self.read_u16(self.sp.wrapping_add(immediate) & self.memory_mask))
            }
            OP_STWS => self.write_u16(
                self.sp.wrapping_add(immediate) & self.memory_mask,
                self.registers[rd] as u16,
            ),
            OP_ADJSP => {
                let offset = (immediate as u8 as i8) as i32;
                self.sp = self.sp.wrapping_add_signed(offset) & self.memory_mask;
            }
            OP_BC => {
                if self.flags.carry {
                    self.pc = (instruction & 0x7ff) as u32;
                }
            }
            OP_SHR if instruction & FLOAT_MASK == FLOAT_PREFIX => {
                let control = self.read_u16(self.pc);
                self.pc = self.pc.wrapping_add(2) & self.memory_mask;
                self.execute_float((control >> 11) & 0x1f, rd, (control & 7) as usize);
            }
            OP_SHR if instruction & MULDIV_MASK == MULDIV_PREFIX => {
                self.execute_muldiv(rd);
            }
            OP_SHR if instruction & WIDE_MASK == WIDE_PREFIX => {
                self.execute_wide(instruction, rd)?;
            }
            OP_SHR if instruction & ALU_MASK == ALU_PREFIX => {
                self.execute_alu(rd);
            }
            OP_SHR if instruction & FAR_BRANCH_MASK == FAR_BRANCH_PREFIX => {
                let condition = ((instruction >> 7) & 0xf) as u8;
                let target = (((self.read_u16(self.pc) & 0x3fff) as u32) << 16)
                    | self.read_u16(self.pc.wrapping_add(2)) as u32;
                self.pc = self.pc.wrapping_add(4) & self.memory_mask;
                if branch_condition(self.flags, condition) {
                    self.pc = target & self.memory_mask;
                }
            }
            OP_SHR if instruction & FAR_CONTROL_MASK == FAR_CONTROL_PREFIX => {
                let operation = (instruction >> 9) & 3;
                let target = (((self.read_u16(self.pc) & 0x3fff) as u32) << 16)
                    | self.read_u16(self.pc.wrapping_add(2)) as u32;
                self.pc = self.pc.wrapping_add(4) & self.memory_mask;
                match operation {
                    0 => self.pc = target & self.memory_mask,
                    1 => {
                        self.push_u32(self.pc);
                        self.pc = target & self.memory_mask;
                    }
                    2 if self.flags.zero => self.pc = target & self.memory_mask,
                    3 if self.flags.carry => self.pc = target & self.memory_mask,
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

        if self.read(ICR_ADDR) & 1 != 0 {
            self.check_interrupts();
        }
        Ok(())
    }

    pub fn run(&mut self, instruction_budget: usize) -> Result<usize, CpuError> {
        let mut executed = 0;
        while executed < instruction_budget && self.running && !self.frame_yield {
            self.step()?;
            executed += 1;
        }
        Ok(executed)
    }

    pub fn begin_frame(&mut self) {
        self.frame_yield = false;
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
        let visible = self.memory[GPU_LAYER_MASK as usize] & ((1 << GPU_LAYER_COUNT) - 1);
        let mut order = (0..GPU_LAYER_COUNT).collect::<Vec<_>>();
        order.sort_by_key(|&layer| (self.memory[GPU_LAYER_ORDER as usize + layer], layer));
        let mut pixels = vec![0; width * height];
        for layer in order {
            if visible & (1 << layer) == 0 {
                continue;
            }
            let transparent = self.memory[GPU_LAYER_TRANSPARENCY as usize + layer];
            for (index, output) in pixels.iter_mut().enumerate() {
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
        pixels
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
            GPU_PRESENT => self.frame_yield = true,
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

    fn push_u16(&mut self, value: u16) {
        self.sp = self.sp.wrapping_sub(2) & self.memory_mask;
        self.write_u16(self.sp, value);
    }

    fn pop_u16(&mut self) -> u16 {
        let value = self.read_u16(self.sp);
        self.sp = self.sp.wrapping_add(2) & self.memory_mask;
        value
    }

    fn push_u32(&mut self, value: u32) {
        self.sp = self.sp.wrapping_sub(4) & self.memory_mask;
        for offset in 0..4 {
            self.write(
                self.sp.wrapping_add(offset),
                (value >> (24 - offset * 8)) as u8,
            );
        }
    }

    fn pop_u32(&mut self) -> u32 {
        let mut value = 0;
        for offset in 0..4 {
            value = (value << 8) | self.read(self.sp.wrapping_add(offset)) as u32;
        }
        self.sp = self.sp.wrapping_add(4) & self.memory_mask;
        value
    }

    fn check_interrupts(&mut self) {
        let icr = self.read(ICR_ADDR);
        let vector = if icr & 2 != 0 {
            Some(KEY_VECTOR_ADDR)
        } else if icr & 8 != 0 {
            Some(RTC_VECTOR_ADDR)
        } else {
            None
        };
        if let Some(vector) = vector {
            self.push_u32(self.pc);
            self.push_u16(self.pack_flags());
            let icr = self.read(ICR_ADDR) & !1;
            self.write(ICR_ADDR, icr);
            let icr = self.read(ICR_ADDR) & !2 & !8;
            self.write(ICR_ADDR, icr);
            self.pc = self.read_u32(vector) & self.memory_mask;
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
    fn disk_mmio_loads_persists_reads_and_enforces_write_protection() {
        let root = std::env::temp_dir().join(format!(
            "sc16-cpu-disk-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let module_files = root.join("module");
        std::fs::create_dir_all(&module_files).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.configure_disk(root.join("disks"), module_files.clone())
            .unwrap();
        cpu.memory[DISK_BASE as usize + 1..DISK_BASE as usize + 9].copy_from_slice(b"TEST0001");
        cpu.write(DISK_BASE, 1);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);

        cpu.write_u16(DISK_BASE + 10, 7);
        cpu.write_u16(DISK_BASE + 12, 0x20);
        let block_start = RAM_START as usize + 0x20;
        for index in 0..DISK_BLOCK_SIZE {
            cpu.memory[block_start + index] = index as u8;
        }
        cpu.write(DISK_BASE, 8);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);

        cpu.write_u16(DISK_BASE + 12, 0x300);
        cpu.memory[RAM_START as usize + 0x300..RAM_START as usize + 0x400].fill(0);
        cpu.write(DISK_BASE, 4);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert_eq!(
            &cpu.memory[RAM_START as usize + 0x300..RAM_START as usize + 0x400],
            &(0..256).map(|value| value as u8).collect::<Vec<_>>()
        );

        cpu.write_u16(DISK_BASE + 10, 1);
        cpu.write(DISK_BASE, 0x20);
        cpu.write(DISK_BASE, 8);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 1);

        cpu.write(DISK_BASE, 2);
        assert_eq!(cpu.memory[DISK_STATUS as usize], 0);
        assert!(root.join("disks").join("TEST0001.json").is_file());
        std::fs::remove_dir_all(root).unwrap();
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
        cpu.write(DISK_BASE, 0x40);
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
    }

    #[test]
    fn console_mmio_reads_and_keyboard_events_are_stateful() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.receive_console_input(b"A");
        assert_eq!(cpu.read(CONSOLE_STATUS), 3);
        assert_eq!(cpu.read(CONSOLE_DATA), b'A');
        assert_eq!(cpu.read(CONSOLE_STATUS), 2);
        assert!(cpu.frame_yield);
        cpu.frame_yield = false;
        cpu.signal_keyboard_event(b'B');
        assert_eq!(cpu.memory[KEYBOARD_ADDR as usize], b'B');
        assert_eq!(cpu.read(ICR_ADDR) & 2, 2);
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
        assert!(cpu.console.output().is_empty());
    }

    #[test]
    fn dispatches_pending_keyboard_interrupt_after_enabling_interrupts() {
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.write_u16(0, OP_EI << 11);
        cpu.write_u32_for_test(KEY_VECTOR_ADDR, 0x20);
        cpu.write(ICR_ADDR, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x20);
        assert_eq!(cpu.read_u16(cpu.sp), 0);
        assert_eq!(cpu.read_u32(cpu.sp + 2), 2);
        assert_eq!(cpu.read(ICR_ADDR), 0);
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

    impl Cpu {
        fn write_u32_for_test(&mut self, address: u32, value: u32) {
            for offset in 0..4 {
                self.write(address + offset, (value >> (24 - offset * 8)) as u8);
            }
        }
    }
}
