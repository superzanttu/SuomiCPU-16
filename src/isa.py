OP_HALT = 0x00
OP_LDI = 0x01
OP_LD = 0x02
OP_ST = 0x03
OP_MOV = 0x04
OP_ADD = 0x05
OP_SUB = 0x06
OP_AND = 0x07
OP_OR = 0x08
OP_XOR = 0x09
OP_NOT = 0x0A
OP_JMP = 0x0B
OP_JZ = 0x0C
OP_INC = 0x0D
OP_DEC = 0x0E
OP_CMP = 0x0F
OP_LDI_H = 0x10
OP_EI = 0x11
OP_DI = 0x12
OP_RTI = 0x13
OP_PUSH = 0x14
OP_POP = 0x15
OP_CALL = 0x16
OP_RET = 0x17
OP_BZ = 0x18
OP_LDW = 0x19
OP_STW = 0x1A
OP_LDWS = 0x1B
OP_STWS = 0x1C
OP_ADJSP = 0x1D
OP_BC = 0x1E
OP_SHR = 0x1F
# Far control instructions use the reserved 0x3E low-six-bit selector in the
# OP_SHR space; the next two words carry the 30-bit target.
FAR_CONTROL_MASK = 0xF83F
FAR_CONTROL_PREFIX = 0xF83E
FAR_CONTROL_OPERATIONS = {
    "JMPX": 0,
    "CALLX": 1,
    "BZX": 2,
    "BCX": 3,
}
FAR_BRANCH_MASK = 0xF80F
FAR_BRANCH_PREFIX = 0xF80D
OP_MOVSP = OP_SHR
OP_FADD = OP_SHR
OP_FSUB = OP_SHR
OP_FMUL = OP_SHR
OP_FDIV = OP_SHR
OP_FCMP = OP_SHR
OP_ITOF = OP_SHR
OP_FTOI = OP_SHR

# 32-bit register-pair ALU group. The even register is the high word and the
# following register is the low word. Pair operations use the reserved 0x1A
# extension selector in the OP_SHR space.
WIDE_PREFIX = 0x1A
WIDE_IMMEDIATE_FLAG = 0x0800
WIDE_OPERATIONS = {
    "ADD32": 0,
    "SUB32": 1,
    "CMP32": 2,
    "AND32": 3,
    "OR32": 4,
    "XOR32": 5,
    "SHL32": 6,
    "LSR32": 7,
    "ASR32": 8,
    "NEG32": 9,
    "MOV32": 10,
    "LD32": 11,
    "ST32": 12,
    "MUL32": 13,
    "DIV32": 14,
    "DIV32S": 14,
    "MOD32": 15,
    "MOD32S": 15,
}

# Integer multiply/divide group: a 32-bit (or 48-bit with an immediate) instruction.
#   word 1: 0xF800 | (Rd << 8) | 0x1E   (OP_SHR extension space, prefix 0x1E)
#   word 2: (operation << 12) | (immediate flag << 11) | Rs
#   word 3: 16-bit immediate, present only when the immediate flag is set
# Operation 7 loads a 30-bit address from the next two words.
MULDIV_PREFIX = 0x1E
MULDIV_IMMEDIATE_FLAG = 0x0800
MULDIV_OPERATIONS = {
    "MUL": 0,    # Rd = low 16 bits of Rd * Rs; C = product did not fit in 16 bits
    "MULHU": 1,  # Rd = high 16 bits of the unsigned product
    "MULHS": 2,  # Rd = high 16 bits of the signed product
    "DIV": 3,    # Rd = Rd / Rs, unsigned
    "DIVS": 4,   # Rd = Rd / Rs, signed (truncates toward zero)
    "MOD": 5,    # Rd = Rd % Rs, unsigned
    "MODS": 6,   # Rd = Rd % Rs, signed (sign follows the dividend)
    "LDI19": 7,
    "LDI30": 7,
    "LDP": 8,
    "STP": 9,
    "SPLIT30": 10,
    "JOIN30": 11,
}

# IEEE-754 operations use a two-word instruction in the OP_SHR extension
# space. Word one's low five bits identify the prefix; word two stores the
# operation in bits 15:11 and the source register in bits 2:0. Binary32 values
# occupy an even register pair and binary64 values occupy R0:R3 or R4:R7.
FLOAT_PREFIX = 0x1B
FLOAT_OPERATIONS = {
    "F32ADD": 0,
    "F32SUB": 1,
    "F32MUL": 2,
    "F32DIV": 3,
    "F32CMP": 4,
    "F64ADD": 5,
    "F64SUB": 6,
    "F64MUL": 7,
    "F64DIV": 8,
    "F64CMP": 9,
    "I16TOF32": 10,
    "U16TOF32": 11,
    "I32TOF32": 12,
    "U32TOF32": 13,
    "I16TOF64": 14,
    "U16TOF64": 15,
    "I32TOF64": 16,
    "U32TOF64": 17,
    "F32TOF64": 18,
    "F64TOF32": 19,
    "F32NEG": 20,
    "F64NEG": 21,
    "F32SQRT": 22,
    "F64SQRT": 23,
    "F32TOI16": 24,
    "F32TOU16": 25,
    "F32TOI32": 26,
    "F32TOU32": 27,
    "F64TOI16": 28,
    "F64TOU16": 29,
    "F64TOI32": 30,
    "F64TOU32": 31,
}

# Extended ALU group: shifts, rotates, add/subtract with carry, immediate ALU forms
# and miscellaneous register operations. Every instruction is 4 bytes, or 6 bytes when
# a 16-bit immediate follows.
#   word 1: 0xF800 | (Rd << 8) | 0x1F
#   word 2: (operation << 12) | (immediate flag << 11) | operand
#   word 3: 16-bit immediate, present only for ADD/SUB/AND/OR/XOR/CMP/TEST/ADC/SBC
#           with the immediate flag set
# For shifts and rotates the operand is a register (count = Rs & 15) or, with the
# immediate flag, a count 0-15 held in the low four bits. Flags: Z, N, V, and C (last
# bit shifted out, carry out of an add, or borrow of a subtract).
ALU_PREFIX = 0x1F
ALU_IMMEDIATE_FLAG = 0x0800
ALU_OPERATIONS = {
    "SHL": 0,   # logical shift left
    "LSR": 1,   # logical shift right
    "ASR": 2,   # arithmetic shift right
    "ROL": 3,   # rotate left
    "ROR": 4,   # rotate right
    "ADC": 5,   # Rd = Rd + Rs + C
    "SBC": 6,   # Rd = Rd - Rs - C
    "NEG": 7,   # Rd = -Rd
    "TEST": 8,  # flags of Rd & Rs, result discarded
    "ADD": 9,   # 16-bit add with immediate form
    "SUB": 10,  # 16-bit subtract with immediate form
    "AND": 11,
    "OR": 12,
    "XOR": 13,
    "CMP": 14,  # flags of Rd - Rs, result discarded
}
ALU_SHIFTS = ("SHL", "LSR", "ASR", "ROL", "ROR")
ALU_IMMEDIATE_OPERATIONS = ("ADD", "SUB", "AND", "OR", "XOR", "CMP", "TEST", "ADC", "SBC")
# Operation 15 selects a miscellaneous instruction through the operand field.
ALU_MISC_OPERATION = 15
MISC_OPERATIONS = {
    "SWAP": 0,    # swap the bytes of Rd
    "SEXT": 1,    # sign-extend the low byte of Rd
    "JMPR": 2,    # PC = Rd
    "CALLR": 3,   # push return address, PC = Rd
    "PUSHF": 4,   # push the flags
    "POPF": 5,    # pop the flags
    "SETC": 6,    # C = 1
    "CLC": 7,     # C = 0
    "SETSP": 8,   # SP = Rd (low 16 bits)
    "CLZ": 9,     # Rd = number of leading zero bits of Rd
    "MOVSPH": 10, # Rd = high 14 bits of the 30-bit SP
    "CALLRP": 11,
    "JMPRP": 12,
    "SETSPH": 13,
}

# Far conditional branches: 6 bytes, 30-bit target.
#   word 1: 0xF80D | (condition << 7)
#   word 2: target[29:16]
#   word 3: target[15:0]
BRANCH_CONDITIONS = {
    "BNZX": 0,   # Z = 0
    "BNCX": 1,   # C = 0
    "BNX": 2,    # N = 1 (negative)
    "BNNX": 3,   # N = 0
    "BVX": 4,    # V = 1 (signed overflow)
    "BNVX": 5,   # V = 0
    "BLTX": 6,   # signed less than: N != V
    "BGEX": 7,   # signed greater or equal: N == V
    "BGTX": 8,   # signed greater: Z = 0 and N == V
    "BLEX": 9,   # signed less or equal: Z = 1 or N != V
    "BHIX": 10,  # unsigned higher: C = 0 and Z = 0
    "BLSX": 11,  # unsigned lower or same: C = 1 or Z = 1
}
BRANCH_ALIASES = {name[:-1]: name for name in BRANCH_CONDITIONS}
OPCODES = {
    "HALT": OP_HALT,
    "LDI": OP_LDI,
    "LDA": OP_LDI,
    "LD": OP_LD,
    "ST": OP_ST,
    "MOV": OP_MOV,
    "ADD": OP_ADD,
    "SUB": OP_SUB,
    "AND": OP_AND,
    "OR": OP_OR,
    "XOR": OP_XOR,
    "NOT": OP_NOT,
    "JMP": OP_JMP,
    "JZ": OP_JZ,
    "INC": OP_INC,
    "DEC": OP_DEC,
    "CMP": OP_CMP,
    "LDI_H": OP_LDI_H,
    "EI": OP_EI,
    "DI": OP_DI,
    "RTI": OP_RTI,
    "PUSH": OP_PUSH,
    "POP": OP_POP,
    "CALL": OP_CALL,
    "RET": OP_RET,
    "BZ": OP_BZ,
    "LDW": OP_LDW,
    "STW": OP_STW,
    "LDWS": OP_LDWS,
    "STWS": OP_STWS,
    "ADJSP": OP_ADJSP,
    "BC": OP_BC,
    "SHR": OP_SHR,
    "MOVSP": OP_MOVSP,
    "FADD": OP_FADD,
    "FSUB": OP_FSUB,
    "FMUL": OP_FMUL,
    "FDIV": OP_FDIV,
    "FCMP": OP_FCMP,
    "ITOF": OP_ITOF,
    "FTOI": OP_FTOI,
    "JMPX": OP_SHR,
    "CALLX": OP_SHR,
    "BZX": OP_SHR,
    "BCX": OP_SHR,
}
OPCODES.update({name: OP_SHR for name in MULDIV_OPERATIONS})
OPCODES.update({name: OP_SHR for name in FLOAT_OPERATIONS})
OPCODES.update({name: OP_SHR for name in WIDE_OPERATIONS})
OPCODES.update({name: OP_SHR for name in MISC_OPERATIONS})
OPCODES.update({name: OP_SHR for name in BRANCH_CONDITIONS})
OPCODES.update({name: OP_SHR for name in BRANCH_ALIASES})
OPCODES.update({name: OP_SHR for name in ALU_OPERATIONS if name not in OPCODES})
OPCODES["NOP"] = OP_MOV
