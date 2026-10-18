pub const MEMORY_SIZE: usize = 1 << 30;
pub const ADDRESS_MASK: u32 = (MEMORY_SIZE as u32) - 1;

const ICR_ADDR: u32 = 0x43002;
const KEY_VECTOR_ADDR: u32 = 0x0002;
const RTC_VECTOR_ADDR: u32 = 0x0006;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CpuError {
    InvalidMemorySize(usize),
    SegmentOutOfBounds { address: u32, length: usize },
    InvalidEntryPoint(u32),
    UnsupportedInstruction(u16),
}

impl std::fmt::Display for CpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMemorySize(size) => {
                write!(f, "memory size must be a power of two, got {size}")
            }
            Self::SegmentOutOfBounds { address, length } => {
                write!(
                    f,
                    "program segment at {address:#x} with length {length} is out of bounds"
                )
            }
            Self::InvalidEntryPoint(address) => {
                write!(f, "entry point {address:#x} is outside memory")
            }
            Self::UnsupportedInstruction(instruction) => {
                write!(f, "unsupported instruction {instruction:#06x}")
            }
        }
    }
}

impl std::error::Error for CpuError {}

pub struct Cpu {
    pub memory: Vec<u8>,
    pub registers: [u32; 8],
    pub pc: u32,
    pub sp: u32,
    pub flags: Flags,
    pub running: bool,
    memory_mask: u32,
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
        Ok(Self {
            memory: vec![0; size],
            registers: [0; 8],
            pc: 0,
            sp: memory_mask,
            flags: Flags::default(),
            running: true,
            memory_mask,
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

    pub fn read(&self, address: u32) -> u8 {
        self.memory[(address & self.memory_mask) as usize]
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let index = (address & self.memory_mask) as usize;
        self.memory[index] = value;
    }

    pub fn read_u16(&self, address: u32) -> u16 {
        u16::from_be_bytes([self.read(address), self.read(address.wrapping_add(1))])
    }

    pub fn write_u16(&mut self, address: u32, value: u16) {
        let [high, low] = value.to_be_bytes();
        self.write(address, high);
        self.write(address.wrapping_add(1), low);
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
            OP_LDI => self.registers[rd] = immediate,
            OP_LD => self.registers[rd] = self.read(self.registers[rs1]) as u32,
            OP_ST => self.write(self.registers[rs1], self.registers[rd] as u8),
            OP_MOV => self.registers[rd] = self.registers[rs1],
            OP_ADD => {
                let left = self.registers[rs1];
                let right = self.registers[rs2];
                let result = left.wrapping_add(right);
                self.registers[rd] = result;
                self.flags.zero = result == 0;
                self.flags.carry = left as u64 + right as u64 > 0xff;
                let word = result as u16;
                self.flags.negative = word & 0x8000 != 0;
                self.flags.overflow =
                    (!(left as u16 ^ right as u16) & (left as u16 ^ word) & 0x8000) != 0;
            }
            OP_SUB => {
                let left = self.registers[rs1];
                let right = self.registers[rs2];
                let result = left.wrapping_sub(right);
                self.registers[rd] = result;
                self.set_sub_flags(left as u16, right as u16, result as u16);
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
                let left = self.registers[rs1] as u16;
                let right = self.registers[rs2] as u16;
                self.set_sub_flags(left, right, left.wrapping_sub(right));
            }
            OP_LDI_H => self.registers[rd] = (immediate << 8) | (self.registers[rd] & 0xff),
            OP_EI => self.write(ICR_ADDR, self.read(ICR_ADDR) | 1),
            OP_DI => self.write(ICR_ADDR, self.read(ICR_ADDR) & !1),
            OP_RTI => {
                let saved_flags = self.pop_u16();
                self.restore_flags(saved_flags);
                self.pc = self.pop_u32() & self.memory_mask;
            }
            OP_PUSH => self.push_u16(self.registers[rd] as u16),
            OP_POP => self.registers[rd] = self.pop_u16() as u32,
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
            OP_LDW => self.registers[rd] = self.read_u16(self.registers[rs1]) as u32,
            OP_STW => self.write_u16(self.registers[rs1], self.registers[rd] as u16),
            OP_LDWS => {
                self.registers[rd] =
                    self.read_u16(self.sp.wrapping_add(immediate) & self.memory_mask) as u32
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
            OP_SHR
                if instruction & 0xf81f == 0xf81b
                    || instruction & 0xf8ff == 0xf81e
                    || instruction & 0xf81f == 0xf81a
                    || instruction & 0xf8ff == 0xf81f =>
            {
                return Err(CpuError::UnsupportedInstruction(instruction));
            }
            OP_SHR => {
                let extension = (instruction >> 5) & 7;
                if extension == 0 {
                    self.registers[rd] = if instruction & 0xff == 0x1c {
                        self.sp
                    } else {
                        (self.registers[rd] & 0xffff) >> 1
                    };
                } else {
                    return Err(CpuError::UnsupportedInstruction(instruction));
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
        while executed < instruction_budget && self.running {
            self.step()?;
            executed += 1;
        }
        Ok(executed)
    }

    fn set_sub_flags(&mut self, left: u16, right: u16, result: u16) {
        self.flags.zero = result == 0;
        self.flags.carry = left < right;
        self.flags.negative = result & 0x8000 != 0;
        self.flags.overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0;
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
            self.write(ICR_ADDR, self.read(ICR_ADDR) & !1);
            self.write(ICR_ADDR, self.read(ICR_ADDR) & !2 & !8);
            self.pc = self.read_u32(vector) & self.memory_mask;
        }
    }

    fn read_u32(&self, address: u32) -> u32 {
        (0..4).fold(0, |value, offset| {
            (value << 8) | self.read(address.wrapping_add(offset)) as u32
        })
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
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
