use memmap2::MmapMut;
use std::collections::VecDeque;

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
const CONSOLE_DATA: u32 = 0x43200;
const CONSOLE_STATUS: u32 = 0x43201;
const CONSOLE_CONTROL: u32 = 0x43202;
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
const GPU_3D: u8 = 22;
const GPU_SAVE: u8 = 14;
const GPU_LOAD: u8 = 15;
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
    pub console_input: VecDeque<u8>,
    pub console_output: Vec<u8>,
    pub saved_video: Option<Vec<u8>>,
    pub random_state: u32,
    pub layers: [Vec<u8>; GPU_LAYER_COUNT],
    pub palette: [[u8; 3]; 256],
    pub gpu_triangles: Vec<GpuTriangle>,
    pub gpu_lines: Vec<GpuLine>,
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
        Ok(Self {
            memory,
            registers: [0; 8],
            pc: 0,
            sp: memory_mask,
            flags: Flags::default(),
            running: true,
            frame_yield: false,
            frame_count: 0,
            console_input: VecDeque::new(),
            console_output: Vec::new(),
            saved_video: None,
            random_state: 0x16c0_ffee,
            layers: std::array::from_fn(|_| vec![0; 640 * 480]),
            palette,
            gpu_triangles: Vec::new(),
            gpu_lines: Vec::new(),
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
                let value = self.console_input.pop_front().unwrap_or(0);
                if self.memory[CONSOLE_CONTROL as usize] & 1 != 0 && value != 0 {
                    self.console_output.push(value);
                    print!("{}", char::from(value));
                }
                value
            }
            CONSOLE_STATUS => {
                if self.console_input.is_empty() && self.running {
                    self.frame_yield = true;
                }
                u8::from(!self.console_input.is_empty()) | 2
            }
            CONSOLE_CONTROL => self.memory[address as usize] & 3,
            _ => self.memory[address as usize],
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let address = address & self.memory_mask;
        let index = address as usize;
        self.memory[index] = value;
        match address {
            GPU_BASE => self.gpu_command(value),
            GPU_MODE_ADDR => {
                if self.layer_mode != value {
                    self.layer_mode = value;
                    for layer in &mut self.layers {
                        layer.fill(0);
                    }
                }
            }
            CONSOLE_DATA => {
                self.console_output.push(value);
                print!("{}", char::from(value));
                if value == b'\n' {
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            }
            CONSOLE_CONTROL => self.memory[index] = value & 3,
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
        self.console_input.extend(bytes.iter().copied());
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
            GPU_PALETTE => {
                let red = self.gpu_word(2) as u8;
                let green = self.gpu_word(4) as u8;
                let blue = self.gpu_word(6) as u8;
                self.palette[color as usize] = [red, green, blue];
                self.memory[(GPU_BASE + 14) as usize] = 0;
                self.memory[(GPU_BASE + 15) as usize] = 0;
            }
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
