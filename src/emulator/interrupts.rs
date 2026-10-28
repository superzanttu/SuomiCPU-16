//! Defines the SC-16 interrupt descriptor table contract.
//! Provides vector constants, gate descriptors, encoding/validation, and table access.

pub const IDT_ENTRY_COUNT: usize = 256;
pub const IDT_ENTRY_BYTES: usize = 8;
pub const IDT_BYTES: usize = IDT_ENTRY_COUNT * IDT_ENTRY_BYTES;
pub const IDT_ALIGNMENT: u32 = 0x800;
pub const IDT_VECTOR_KEYBOARD: u8 = 32;
pub const IDT_VECTOR_RTC: u8 = 33;
pub const IDT_VECTOR_PIT: u8 = 34;
pub const IDT_VECTOR_SYSCALL: u8 = 0x80;

const PRESENT: u8 = 0x80;
const TRAP_GATE: u8 = 0x01;
const DPL_MASK: u8 = 0x06;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateType {
    Interrupt,
    Trap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdtDescriptor {
    pub handler: u32,
    pub present: bool,
    pub privilege: u8,
    pub gate_type: GateType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdtError {
    InvalidBase,
    InvalidVector,
    MissingDescriptor,
    MalformedDescriptor,
    InvalidHandler,
}

impl IdtDescriptor {
    pub fn encode(self) -> Result<[u8; IDT_ENTRY_BYTES], IdtError> {
        if self.handler >= crate::machine::ADDRESS_SPACE_SIZE || self.privilege > 3 {
            return Err(IdtError::InvalidHandler);
        }
        let mut descriptor = [0; IDT_ENTRY_BYTES];
        descriptor[..4].copy_from_slice(&self.handler.to_be_bytes());
        descriptor[4] = u8::from(self.present) * PRESENT
            | (self.privilege << 1)
            | u8::from(self.gate_type == GateType::Trap) * TRAP_GATE;
        Ok(descriptor)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, IdtError> {
        if bytes.len() != IDT_ENTRY_BYTES || bytes[5..].iter().any(|byte| *byte != 0) {
            return Err(IdtError::MalformedDescriptor);
        }
        let attributes = bytes[4];
        if attributes & !(PRESENT | DPL_MASK | TRAP_GATE) != 0 {
            return Err(IdtError::MalformedDescriptor);
        }
        let handler = u32::from_be_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| IdtError::MalformedDescriptor)?,
        );
        if handler >= crate::machine::ADDRESS_SPACE_SIZE {
            return Err(IdtError::InvalidHandler);
        }
        if attributes & PRESENT == 0 {
            return Err(IdtError::MissingDescriptor);
        }
        Ok(Self {
            handler,
            present: true,
            privilege: (attributes & DPL_MASK) >> 1,
            gate_type: if attributes & TRAP_GATE != 0 {
                GateType::Trap
            } else {
                GateType::Interrupt
            },
        })
    }
}

pub fn descriptor_address(base: u32, vector: u16, memory_size: usize) -> Result<usize, IdtError> {
    if usize::from(vector) >= IDT_ENTRY_COUNT {
        return Err(IdtError::InvalidVector);
    }
    if base & (IDT_ALIGNMENT - 1) != 0 {
        return Err(IdtError::InvalidBase);
    }
    let start = usize::try_from(base).map_err(|_| IdtError::InvalidBase)?;
    let end = start.checked_add(IDT_BYTES).ok_or(IdtError::InvalidBase)?;
    if end > crate::machine::usable_ram_size(memory_size) {
        return Err(IdtError::InvalidBase);
    }
    Ok(start + usize::from(vector) * IDT_ENTRY_BYTES)
}

pub fn read_descriptor(memory: &[u8], base: u32, vector: u16) -> Result<IdtDescriptor, IdtError> {
    let start = descriptor_address(base, vector, memory.len())?;
    IdtDescriptor::decode(&memory[start..start + IDT_ENTRY_BYTES])
}

pub fn write_descriptor(
    memory: &mut [u8],
    base: u32,
    vector: u16,
    descriptor: IdtDescriptor,
) -> Result<(), IdtError> {
    let start = descriptor_address(base, vector, memory.len())?;
    let bytes = descriptor.encode()?;
    memory[start..start + IDT_ENTRY_BYTES].copy_from_slice(&bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_encoding_preserves_gate_type_and_privilege() {
        let mut memory = vec![0; 0x4000];
        let descriptor = IdtDescriptor {
            handler: 0x1234,
            present: true,
            privilege: 3,
            gate_type: GateType::Trap,
        };
        write_descriptor(
            &mut memory,
            0x800,
            u16::from(IDT_VECTOR_SYSCALL),
            descriptor,
        )
        .unwrap();

        assert_eq!(
            read_descriptor(&memory, 0x800, u16::from(IDT_VECTOR_SYSCALL)),
            Ok(descriptor)
        );
    }

    #[test]
    fn descriptor_reader_rejects_bad_roots_vectors_and_reserved_bits() {
        let memory = vec![0; 0x4000];
        assert_eq!(
            read_descriptor(&memory, 0x801, 1),
            Err(IdtError::InvalidBase)
        );
        assert_eq!(
            read_descriptor(&memory, 0x800, IDT_ENTRY_COUNT as u16),
            Err(IdtError::InvalidVector)
        );
        assert_eq!(
            read_descriptor(&memory, 0x4000, 0),
            Err(IdtError::InvalidBase)
        );
        let mut malformed = memory;
        malformed[0x800 + 5] = 1;
        assert_eq!(
            read_descriptor(&malformed, 0x800, 0),
            Err(IdtError::MalformedDescriptor)
        );
    }
}
