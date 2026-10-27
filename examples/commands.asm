; Instruction reference for the SC-16 assembler.
; Load with: cargo run -- examples/commands.asm
; Interrupt, privilege, and extended stack/control operations follow HALT so
; the tour can run without enabling interrupts or entering kernel-only paths.

.address 0x0080
start:
    LDI R0, 0x20
    LDI R1, 5
    LDI R6, 0x34
    LDI_H R6, 0x12       ; R6 = 0x1234

    LD R2, R0            ; Read one byte from memory[R0]
    ST R0, R1            ; Write R1's low byte to memory[R0]
    LD R2, R0            ; Read the value just stored
    LDW R2, R0           ; Read a big-endian 16-bit word
    STW R0, R2           ; Store a big-endian 16-bit word
    LDWS R2, 0           ; Read a word at SP + 0
    STWS R2, 0           ; Store a word at SP + 0
    ADJSP -2             ; Adjust the stack pointer by a signed byte
    ADJSP 2
    MOV R3, R2

    ADD R3, R1
    SUB R3, R1
    AND R3, R1
    OR R3, R1
    XOR R3, R1
    NOT R3, R1
    INC R3
    DEC R3
    SHR R3
    CMP R3, R1

    PUSH R3
    POP R4

    LDI R5, 0
    JZ R5, zero_path     ; This branch is taken
    LDI R6, 0xEE          ; Skipped

zero_path:
    JMP jump_target
    LDI R6, 0xDD          ; Skipped by JMP

jump_target:
    HALT
    EI                     ; Encoding example; not reached during this run
    DI                     ; Encoding example; not reached during this run
    RTI
    LIDT R0
    MMUON R0
    MMUSWITCH R0
    MMUOFF
    ENTERU R0, R2
    STARTU R0, R2
    CALL sample_routine    ; CALL/RET and BZ encoding examples; also unreachable
    BZ sample_routine
    BC sample_routine
    LDA R0, sample_routine ; Load a full memory address into a register
    LDI19 R0, 0x12345      ; Legacy spelling for the 30-bit immediate opcode
    LDI30 R0, 0x3F51234    ; Load a 30-bit absolute address directly
    LDP R0, R1             ; Load a 30-bit pointer from the address in R1
    STP R0, R1             ; Store a 30-bit pointer through the address in R1
    SPLIT30 R0, R1         ; Split R0 into high/low words R0:R1
    JOIN30 R0, R1          ; Join high/low words R0:R1 as a 30-bit pointer
    MOVSP R1                ; Copy the stack pointer into R1
    FADD R2, R3
    FSUB R2, R3
    FMUL R2, R3
    FDIV R2, R3
    FCMP R2, R3
    ITOF R2
    FTOI R2
    F32ADD R0, R2
    F32SUB R0, R2
    F32MUL R0, R2
    F32DIV R0, R2
    F32CMP R0, R2
    F64ADD R0, R4
    F64SUB R0, R4
    F64MUL R0, R4
    F64DIV R0, R4
    F64CMP R0, R4
    I16TOF32 R0, R2
    U16TOF32 R0, R2
    I32TOF32 R0, R2
    U32TOF32 R0, R2
    I16TOF64 R0, R2
    U16TOF64 R0, R2
    I32TOF64 R0, R2
    U32TOF64 R0, R2
    F32TOF64 R0, R2
    F64TOF32 R2, R0
    F32NEG R2
    F64NEG R0
    F32SQRT R2
    F64SQRT R0
    F32TOI16 R1, R2
    F32TOU16 R1, R2
    F32TOI32 R2, R0
    F32TOU32 R2, R0
    F64TOI16 R1, R0
    F64TOU16 R1, R0
    F64TOI32 R2, R4
    F64TOU32 R2, R4
    MUL R2, R3             ; Integer multiply/divide group (register or immediate source)
    MULHU R2, R3
    MULHS R2, R3
    DIV R2, R3
    DIVS R2, R3
    MOD R2, 10
    MODS R2, -10
    JMPX sample_routine    ; Far control-flow encodings; unreachable
    CALLX sample_routine
    BZX sample_routine
    BCX sample_routine
    SHL R2, 3              ; Extended group: shifts/rotates (register or 0-15 immediate count)
    LSR R2, R3
    ASR R2, 1
    ROL R2, 1
    ROR R2, R3
    ADC R2, R3
    SBC R2, 1
    NEG R2
    TEST R2, 255
    SWAP R2
    SEXT R2
    CLZ R2
    PUSHF
    POPF
    SETC
    CLC
    SETSP R2
    NOP
    JMPR R2
    JMPRP R0, R1         ; 30-bit indirect jump through the pointer pair R0:R1
    CALLR R2
    CALLRP R0, R1         ; Indirect call through the 30-bit pointer pair R0:R1
    BNZX sample_routine    ; Far conditional branches (flag/signed/unsigned)
    BNCX sample_routine
    BNX sample_routine
    BNNX sample_routine
    BVX sample_routine
    BNVX sample_routine
    BLTX sample_routine
    BGEX sample_routine
    BGTX sample_routine
    BLEX sample_routine
    BHIX sample_routine
    BLSX sample_routine
    BNZ sample_routine
    BNC sample_routine
    BN sample_routine
    BNN sample_routine
    BV sample_routine
    BNV sample_routine
    BLT sample_routine
    BGE sample_routine
    BGT sample_routine
    BLE sample_routine
    BHI sample_routine
    BLS sample_routine
    ADD32 R0, R2           ; 32-bit pair operations (even register is high word)
    SUB32 R0, R2
    CMP32 R0, R2
    AND32 R0, R2
    OR32 R0, R2
    XOR32 R0, R2
    SHL32 R0, 1
    LSR32 R0, R1
    ASR32 R0, 1
    NEG32 R0
    MOV32 R0, R2
    LD32 R0, R2
    ST32 R0, R2
    MUL32 R0, R2
    DIV32 R0, R2
    DIV32S R0, R2
    MOD32 R0, R2
    MOD32S R0, R2
    MOVSPH R0
    SETSPH R0               ; Set the stack pointer's high three address bits
sample_routine:
    RET
