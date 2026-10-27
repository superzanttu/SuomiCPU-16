.address 0x9000
child_entry:
    LDI R0, 0x4000
    LD R1, R0
    LDI R0, 4
    LDI R1, 4096
    LDI R2, 3
    LDI R3, 0
    LDI R7, 1
    SVC
    MOV R6, R0
    LDI R0, 5
    LDI R1, 0x9100
    LDI R2, 1
    LDI R7, 1
    SVC
    MOV R4, R0
    LDI R0, 6
    MOV R1, R4
    MOV R2, R6
    LDI R3, 5
    LDI R7, 1
    SVC
    MOV R5, R0
    LDI R0, 7
    LDI R1, 1
    MOV R2, R6
    MOV R3, R5
    LDI R7, 1
    SVC
    LDI R0, 8
    MOV R1, R4
    LDI R7, 1
    SVC
    LDI R0, 5
    LDI R1, 0x9120
    LDI R2, 14
    LDI R7, 1
    SVC
    MOV R4, R0
    LDI R0, 7
    MOV R1, R4
    MOV R2, R6
    LDI R3, 5
    LDI R7, 1
    SVC
    LDI R0, 8
    MOV R1, R4
    LDI R7, 1
    SVC
    LDI R0, 1
    LDI R1, 42
    LDI R7, 1
    SVC
    HALT

.address 0x9100
input_path:
    .data string "input.txt", "\0"
.address 0x9120
output_path:
    .data string "result.txt", "\0"
