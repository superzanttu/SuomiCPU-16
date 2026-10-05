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
OP_MOVSP = OP_SHR
OP_FADD = OP_SHR
OP_FSUB = OP_SHR
OP_FMUL = OP_SHR
OP_FDIV = OP_SHR
OP_FCMP = OP_SHR
OP_ITOF = OP_SHR
OP_FTOI = OP_SHR

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
