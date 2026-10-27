.address 0x4000
kernel_start:
    LDI R0, 0
    LDI R1, 0x800
    LIDT R0
    LDI30 R6, 0x4301e
    LDI R0, 4
    STW R6, R0
    LDI R0, 0
    SETSPH R0
    LDI R0, 0x8000
    SETSP R0
    EI
    LDI R2, 0
    LDI R3, 0x2000
    MMUON R2
    LDI R4, 0
    LDI R5, 0x5000
    LDI R6, 0
    LDI R7, 0x9000
    STARTU R4, R6
.address 0x4200
timer_handler:
    RTI
.address 0x5000
user_entry:
    LDI R0, 1
    LDI R1, 0
    LDI R7, 1
    SVC
    HALT
.address 0x2000
    .data hex 00, 00, 30, 0F
.address 0x3000
    .data hex 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, 0B, 00, 00, 50, 0F, 00, 00, 00, 00, 00, 00, 70, 03, 00, 00, 80, 07
.address 0x0910
    .data hex 00, 00, 42, 00, 80, 00, 00, 00
.address 0x0C00
    .data hex 00, 00, 42, 00, 86, 00, 00, 00
