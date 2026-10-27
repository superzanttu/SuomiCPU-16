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
    LDI R0, 0
    LDI R1, 0x2000
    LDI R2, 1024
clear_directory:
    STW R1, R0
    ADD R1, 2
    SUB R2, 1
    BNZ clear_directory
    LDI R1, 0x3000
    LDI R2, 1024
clear_table:
    STW R1, R0
    ADD R1, 2
    SUB R2, 1
    BNZ clear_table
    LDI R1, 0x300f
    LDI R2, 0x2000
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R1, 0x400b
    LDI R2, 0x3010
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R1, 0x500f
    LDI R2, 0x3014
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R1, 0x600f
    LDI R2, 0x3018
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R1, 0x7003
    LDI R2, 0x301c
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R1, 0x8007
    LDI R2, 0x3020
    STW R2, R0
    ADD R2, 2
    STW R2, R1
    LDI R4, 0
    LDI R5, 0x2000
    MMUON R4
    EI
    LDI R4, 0
    LDI R5, 0x5000
    LDI R6, 0
    LDI R7, 0x9000
    STARTU R4, R6

.address 0x4200
timer_handler:
    RTI

.address 0x4240
page_fault_handler:
    LDWS R0, 0
    LDWS R1, 2
    LDI R2, 2
    ADD R1, R2
    STWS R1, 2
    RTI

.address 0x0810
    .data hex 00, 00, 42, 40, 80, 00, 00, 00
.address 0x0900
    .data hex 00, 00, 42, 00, 80, 00, 00, 00
.address 0x0908
    .data hex 00, 00, 42, 00, 80, 00, 00, 00
.address 0x0910
    .data hex 00, 00, 42, 00, 80, 00, 00, 00
.address 0x0c00
    .data hex 00, 00, 42, 00, 86, 00, 00, 00

.include "preemptive_os_shell.asm"
