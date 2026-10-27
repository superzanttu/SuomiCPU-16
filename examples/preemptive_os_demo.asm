.address 0x9000
demo_start:
    LDI R0, 7
    LDI R1, 1
    LDI R2, 0x9100
    LDI R3, 15
    LDI R7, 1
    SVC
    LDI R0, 1
    LDI R1, 7
    LDI R7, 1
    SVC
    HALT
.address 0x9100
demo_message:
    .data string "demo task ran\r\n\0"
