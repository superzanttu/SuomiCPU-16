; Instruction reference for the SC-16 assembler.
; Load with: python SuomiCPU.py examples/commands.asm
; Interrupt instructions and the extended stack/control operations are placed
; after HALT so the tour can run without enabling interrupts.

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
    CALL sample_routine    ; CALL/RET and BZ encoding examples; also unreachable
    BZ sample_routine
    BC sample_routine
    LDA R0, sample_routine ; Load a full memory address into a register
    MOVSP R1                ; Copy the stack pointer into R1
    FADD R2, R3
    FSUB R2, R3
    FMUL R2, R3
    FDIV R2, R3
    FCMP R2, R3
    ITOF R2
    FTOI R2
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
sample_routine:
    RET
