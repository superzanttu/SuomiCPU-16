.address 0x1800

puts:
    LDWS R4, 4
    LDWS R5, 6
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R4, R5
    LDI R6, 0
puts_next:
    LD R0, R4
    LDI R7, 0
    CMP R0, R7
    BZX puts_newline
    LDI30 R7, 0x43200
    ST R7, R0
    INC R4
    INC R6
    JMPX puts_next
puts_newline:
    LDI R0, 10
    LDI30 R7, 0x43200
    ST R7, R0
    INC R6
    MOV R0, R6
    RET

fgets:
    LDWS R4, 4
    MOV R3, R4
    LDWS R5, 6
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R4, R5
    LDWS R6, 8
    LDI R0, 1
    CMP R6, R0
    BLTX fgets_invalid
    LDWS R0, 10
    LDI R1, 1
    CMP R0, R1
    BNZX fgets_invalid
    LDWS R0, 12
    LDI R1, 0
    CMP R0, R1
    BNZX fgets_invalid
    LDI R2, 0
    DEC R6
fgets_read:
    CMP R2, R6
    BZX fgets_terminate
fgets_poll:
    LDI30 R7, 0x43201
    LD R0, R7
    LDI R1, 1
    AND R0, R1
    LDI R1, 0
    CMP R0, R1
    BZX fgets_poll
    LDI30 R7, 0x43200
    LD R0, R7
    MOV R7, R4
    ADD R7, R2
    ST R7, R0
    INC R2
    LDI R1, 10
    CMP R0, R1
    BZX fgets_terminate
    JMPX fgets_read
fgets_terminate:
    MOV R7, R4
    ADD R7, R2
    LDI R0, 0
    ST R7, R0
    MOV R0, R4
    RET
fgets_invalid:
    LDI R0, 0
    RET

printf:
    LDWS R4, 4
    LDWS R5, 6
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R5, R5
    ADD R4, R5
    LDI R1, 0
printf_next:
    LD R0, R4
    LDI R7, 0
    CMP R0, R7
    BZX printf_done
    LDI R7, 37
    CMP R0, R7
    BZX printf_format
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    INC R4
    JMPX printf_next
printf_format:
    INC R4
    LD R0, R4
    LDI R7, 37
    CMP R0, R7
    BZX printf_percent
    LDI R7, 115
    CMP R0, R7
    BZX printf_string
    LDI R0, 37
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    JMPX printf_emit_format_character
printf_percent:
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    INC R4
    JMPX printf_next
printf_emit_format_character:
    LD R0, R4
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    INC R4
    JMPX printf_next
printf_string:
    LDWS R5, 8
    LDWS R6, 10
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R6, R6
    ADD R5, R6
printf_string_next:
    LD R0, R5
    LDI R7, 0
    CMP R0, R7
    BZX printf_string_done
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    INC R5
    JMPX printf_string_next
printf_string_done:
    INC R4
    JMPX printf_next
printf_done:
    MOV R0, R1
    RET
