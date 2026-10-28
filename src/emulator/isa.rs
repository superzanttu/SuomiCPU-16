pub const OP_HALT: u16 = 0x00;
pub const OP_LDI: u16 = 0x01;
pub const OP_LD: u16 = 0x02;
pub const OP_ST: u16 = 0x03;
pub const OP_MOV: u16 = 0x04;
pub const OP_ADD: u16 = 0x05;
pub const OP_SUB: u16 = 0x06;
pub const OP_AND: u16 = 0x07;
pub const OP_OR: u16 = 0x08;
pub const OP_XOR: u16 = 0x09;
pub const OP_NOT: u16 = 0x0a;
pub const OP_JMP: u16 = 0x0b;
pub const OP_JZ: u16 = 0x0c;
pub const OP_INC: u16 = 0x0d;
pub const OP_DEC: u16 = 0x0e;
pub const OP_CMP: u16 = 0x0f;
pub const OP_LDI_H: u16 = 0x10;
pub const OP_EI: u16 = 0x11;
pub const OP_DI: u16 = 0x12;
pub const OP_RTI: u16 = 0x13;
pub const OP_PUSH: u16 = 0x14;
pub const OP_POP: u16 = 0x15;
pub const OP_CALL: u16 = 0x16;
pub const OP_RET: u16 = 0x17;
pub const OP_BZ: u16 = 0x18;
pub const OP_LDW: u16 = 0x19;
pub const OP_STW: u16 = 0x1a;
pub const OP_LDWS: u16 = 0x1b;
pub const OP_STWS: u16 = 0x1c;
pub const OP_ADJSP: u16 = 0x1d;
pub const OP_BC: u16 = 0x1e;
pub const OP_SHR: u16 = 0x1f;
pub const SVC_VECTOR: u16 = 0x80;
pub const OP_MOVSP: u16 = OP_SHR;
pub const OP_FADD: u16 = OP_SHR;
pub const OP_FSUB: u16 = OP_SHR;
pub const OP_FMUL: u16 = OP_SHR;
pub const OP_FDIV: u16 = OP_SHR;
pub const OP_FCMP: u16 = OP_SHR;
pub const OP_ITOF: u16 = OP_SHR;
pub const OP_FTOI: u16 = OP_SHR;

pub const FAR_CONTROL_MASK: u16 = 0xf83f;
pub const FAR_CONTROL_PREFIX: u16 = 0xf83e;
pub const FAR_BRANCH_MASK: u16 = 0xf80f;
pub const FAR_BRANCH_PREFIX: u16 = 0xf80d;
pub const WIDE_PREFIX: u16 = 0x1a;
pub const WIDE_IMMEDIATE_FLAG: u16 = 0x0800;
pub const MULDIV_PREFIX: u16 = 0x1e;
pub const MULDIV_IMMEDIATE_FLAG: u16 = 0x0800;
pub const FLOAT_PREFIX: u16 = 0x1b;
pub const ALU_PREFIX: u16 = 0x1f;
pub const ALU_IMMEDIATE_FLAG: u16 = 0x0800;
pub const ALU_MISC_OPERATION: u16 = 15;

pub const FAR_CONTROL_OPERATIONS: &[(&str, u16)] =
    &[("JMPX", 0), ("CALLX", 1), ("BZX", 2), ("BCX", 3)];

pub const CLASSIC_EXTENSIONS: &[(&str, (u16, bool))] = &[
    ("MOVSP", (0, false)),
    ("FADD", (1, true)),
    ("FSUB", (2, true)),
    ("FMUL", (3, true)),
    ("FDIV", (4, true)),
    ("FCMP", (5, true)),
    ("ITOF", (6, false)),
    ("FTOI", (7, false)),
];

pub const WIDE_OPERATIONS: &[(&str, u16)] = &[
    ("ADD32", 0),
    ("SUB32", 1),
    ("CMP32", 2),
    ("AND32", 3),
    ("OR32", 4),
    ("XOR32", 5),
    ("SHL32", 6),
    ("LSR32", 7),
    ("ASR32", 8),
    ("NEG32", 9),
    ("MOV32", 10),
    ("LD32", 11),
    ("ST32", 12),
    ("MUL32", 13),
    ("DIV32", 14),
    ("DIV32S", 14),
    ("MOD32", 15),
    ("MOD32S", 15),
];

pub const MULDIV_OPERATIONS: &[(&str, u16)] = &[
    ("MUL", 0),
    ("MULHU", 1),
    ("MULHS", 2),
    ("DIV", 3),
    ("DIVS", 4),
    ("MOD", 5),
    ("MODS", 6),
    ("LDI19", 7),
    ("LDI30", 7),
    ("LDP", 8),
    ("STP", 9),
    ("SPLIT30", 10),
    ("JOIN30", 11),
];

pub const FLOAT_OPERATIONS: &[(&str, u16)] = &[
    ("F32ADD", 0),
    ("F32SUB", 1),
    ("F32MUL", 2),
    ("F32DIV", 3),
    ("F32CMP", 4),
    ("F64ADD", 5),
    ("F64SUB", 6),
    ("F64MUL", 7),
    ("F64DIV", 8),
    ("F64CMP", 9),
    ("I16TOF32", 10),
    ("U16TOF32", 11),
    ("I32TOF32", 12),
    ("U32TOF32", 13),
    ("I16TOF64", 14),
    ("U16TOF64", 15),
    ("I32TOF64", 16),
    ("U32TOF64", 17),
    ("F32TOF64", 18),
    ("F64TOF32", 19),
    ("F32NEG", 20),
    ("F64NEG", 21),
    ("F32SQRT", 22),
    ("F64SQRT", 23),
    ("F32TOI16", 24),
    ("F32TOU16", 25),
    ("F32TOI32", 26),
    ("F32TOU32", 27),
    ("F64TOI16", 28),
    ("F64TOU16", 29),
    ("F64TOI32", 30),
    ("F64TOU32", 31),
];

pub const ALU_OPERATIONS: &[(&str, u16)] = &[
    ("SHL", 0),
    ("LSR", 1),
    ("ASR", 2),
    ("ROL", 3),
    ("ROR", 4),
    ("ADC", 5),
    ("SBC", 6),
    ("NEG", 7),
    ("TEST", 8),
    ("ADD", 9),
    ("SUB", 10),
    ("AND", 11),
    ("OR", 12),
    ("XOR", 13),
    ("CMP", 14),
];

pub const ALU_SHIFTS: &[&str] = &["SHL", "LSR", "ASR", "ROL", "ROR"];
pub const ALU_IMMEDIATE_OPERATIONS: &[&str] = &[
    "ADD", "SUB", "AND", "OR", "XOR", "CMP", "TEST", "ADC", "SBC",
];

pub const MISC_OPERATIONS: &[(&str, u16)] = &[
    ("SWAP", 0),
    ("SEXT", 1),
    ("JMPR", 2),
    ("CALLR", 3),
    ("PUSHF", 4),
    ("POPF", 5),
    ("SETC", 6),
    ("CLC", 7),
    ("SETSP", 8),
    ("CLZ", 9),
    ("MOVSPH", 10),
    ("CALLRP", 11),
    ("JMPRP", 12),
    ("SETSPH", 13),
    ("LIDT", 14),
    ("MMUON", 15),
    ("MMUSWITCH", 16),
    ("MMUOFF", 17),
    ("ENTERU", 18),
    ("STARTU", 19),
];

pub const BRANCH_CONDITIONS: &[(&str, u16)] = &[
    ("BNZX", 0),
    ("BNCX", 1),
    ("BNX", 2),
    ("BNNX", 3),
    ("BVX", 4),
    ("BNVX", 5),
    ("BLTX", 6),
    ("BGEX", 7),
    ("BGTX", 8),
    ("BLEX", 9),
    ("BHIX", 10),
    ("BLSX", 11),
];

pub const BASE_OPCODES: &[(&str, u16)] = &[
    ("HALT", OP_HALT),
    ("SVC", OP_HALT),
    ("LDI", OP_LDI),
    ("LDA", OP_LDI),
    ("LD", OP_LD),
    ("ST", OP_ST),
    ("MOV", OP_MOV),
    ("ADD", OP_ADD),
    ("SUB", OP_SUB),
    ("AND", OP_AND),
    ("OR", OP_OR),
    ("XOR", OP_XOR),
    ("NOT", OP_NOT),
    ("JMP", OP_JMP),
    ("JZ", OP_JZ),
    ("INC", OP_INC),
    ("DEC", OP_DEC),
    ("CMP", OP_CMP),
    ("LDI_H", OP_LDI_H),
    ("EI", OP_EI),
    ("DI", OP_DI),
    ("RTI", OP_RTI),
    ("PUSH", OP_PUSH),
    ("POP", OP_POP),
    ("CALL", OP_CALL),
    ("RET", OP_RET),
    ("BZ", OP_BZ),
    ("LDW", OP_LDW),
    ("STW", OP_STW),
    ("LDWS", OP_LDWS),
    ("STWS", OP_STWS),
    ("ADJSP", OP_ADJSP),
    ("BC", OP_BC),
    ("SHR", OP_SHR),
    ("MOVSP", OP_SHR),
    ("FADD", OP_SHR),
    ("FSUB", OP_SHR),
    ("FMUL", OP_SHR),
    ("FDIV", OP_SHR),
    ("FCMP", OP_SHR),
    ("ITOF", OP_SHR),
    ("FTOI", OP_SHR),
    ("JMPX", OP_SHR),
    ("CALLX", OP_SHR),
    ("BZX", OP_SHR),
    ("BCX", OP_SHR),
    ("NOP", OP_MOV),
];

pub fn lookup(table: &[(&str, u16)], name: &str) -> Option<u16> {
    table
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
}

pub fn branch_condition(name: &str) -> Option<u16> {
    lookup(BRANCH_CONDITIONS, name).or_else(|| {
        let extended = format!("{name}X");
        lookup(BRANCH_CONDITIONS, &extended)
    })
}

pub fn opcode(name: &str) -> Option<u16> {
    lookup(BASE_OPCODES, name).or_else(|| {
        let extension_tables = [
            FAR_CONTROL_OPERATIONS,
            WIDE_OPERATIONS,
            MULDIV_OPERATIONS,
            FLOAT_OPERATIONS,
            ALU_OPERATIONS,
            MISC_OPERATIONS,
        ];
        extension_tables
            .into_iter()
            .any(|table| lookup(table, name).is_some())
            .then_some(OP_SHR)
            .or_else(|| branch_condition(name).map(|_| OP_SHR))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_opcode_table_covers_all_slots_and_declares_shared_extensions() {
        let mut opcode_values = BASE_OPCODES
            .iter()
            .filter_map(|(name, value)| {
                (!matches!(
                    *name,
                    "LDA"
                        | "MOVSP"
                        | "FADD"
                        | "FSUB"
                        | "FMUL"
                        | "FDIV"
                        | "FCMP"
                        | "ITOF"
                        | "FTOI"
                        | "JMPX"
                        | "CALLX"
                        | "BZX"
                        | "BCX"
                        | "NOP"
                ))
                .then_some(*value)
            })
            .collect::<Vec<_>>();
        opcode_values.sort_unstable();
        opcode_values.dedup();
        assert_eq!(opcode_values, (0..32).collect::<Vec<_>>());

        let shared_extensions = BASE_OPCODES
            .iter()
            .filter_map(|(name, value)| (*value == OP_SHR).then_some(*name))
            .collect::<Vec<_>>();
        assert_eq!(
            shared_extensions,
            [
                "SHR", "MOVSP", "FADD", "FSUB", "FMUL", "FDIV", "FCMP", "ITOF", "FTOI", "JMPX",
                "CALLX", "BZX", "BCX"
            ]
        );
    }

    #[test]
    fn aliases_and_extension_mnemonics_resolve_to_the_expected_opcode() {
        assert_eq!(lookup(BASE_OPCODES, "NOP"), Some(OP_MOV));
        assert_eq!(lookup(BASE_OPCODES, "LDA"), Some(OP_LDI));
        assert_eq!(lookup(BASE_OPCODES, "JMPX"), Some(OP_SHR));
        assert_eq!(lookup(WIDE_OPERATIONS, "DIV32S"), Some(14));
        assert_eq!(lookup(MULDIV_OPERATIONS, "LDI30"), Some(7));
        assert_eq!(lookup(FLOAT_OPERATIONS, "F64TOU32"), Some(31));
        assert_eq!(lookup(MISC_OPERATIONS, "SETSPH"), Some(13));
    }

    #[test]
    fn branch_aliases_resolve_to_far_branch_conditions() {
        for (name, condition) in BRANCH_CONDITIONS {
            assert_eq!(branch_condition(name), Some(*condition));
            assert_eq!(
                branch_condition(name.strip_suffix('X').unwrap()),
                Some(*condition)
            );
        }
        assert_eq!(branch_condition("BADC"), None);
    }

    #[test]
    fn opcode_lookup_covers_base_and_extension_mnemonics() {
        assert_eq!(opcode("HALT"), Some(OP_HALT));
        assert_eq!(opcode("NOP"), Some(OP_MOV));
        assert_eq!(opcode("LDA"), Some(OP_LDI));
        assert_eq!(opcode("MOVSP"), Some(OP_SHR));
        for table in [
            FAR_CONTROL_OPERATIONS,
            WIDE_OPERATIONS,
            MULDIV_OPERATIONS,
            FLOAT_OPERATIONS,
            ALU_OPERATIONS,
            MISC_OPERATIONS,
            BRANCH_CONDITIONS,
        ] {
            for (name, _) in table {
                assert!(opcode(name).is_some(), "{name}");
            }
        }
        for (name, _) in BRANCH_CONDITIONS {
            assert_eq!(opcode(name.strip_suffix('X').unwrap()), Some(OP_SHR));
        }
        assert_eq!(opcode("UNKNOWN"), None);
    }
}
