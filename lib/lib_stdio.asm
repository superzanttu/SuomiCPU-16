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

bug_monitor_debug:
    LDI30 R7, 0x43200
    LDI R0, 91
    ST R7, R0
    LDI R0, 68
    ST R7, R0
    LDI R0, 69
    ST R7, R0
    LDI R0, 66
    ST R7, R0
    LDI R0, 85
    ST R7, R0
    LDI R0, 71
    ST R7, R0
    LDI R0, 93
    ST R7, R0
    LDI R0, 32
    ST R7, R0
    JMPX bug_monitor_write_message

bug_monitor_info:
    LDI30 R7, 0x43200
    LDI R0, 91
    ST R7, R0
    LDI R0, 73
    ST R7, R0
    LDI R0, 78
    ST R7, R0
    LDI R0, 70
    ST R7, R0
    LDI R0, 79
    ST R7, R0
    LDI R0, 93
    ST R7, R0
    LDI R0, 32
    ST R7, R0
    JMPX bug_monitor_write_message

bug_monitor_warning:
    LDI30 R7, 0x43200
    LDI R0, 91
    ST R7, R0
    LDI R0, 87
    ST R7, R0
    LDI R0, 65
    ST R7, R0
    LDI R0, 82
    ST R7, R0
    LDI R0, 78
    ST R7, R0
    LDI R0, 93
    ST R7, R0
    LDI R0, 32
    ST R7, R0
    JMPX bug_monitor_write_message

bug_monitor_error:
    LDI30 R7, 0x43200
    LDI R0, 91
    ST R7, R0
    LDI R0, 69
    ST R7, R0
    LDI R0, 82
    ST R7, R0
    LDI R0, 82
    ST R7, R0
    LDI R0, 79
    ST R7, R0
    LDI R0, 82
    ST R7, R0
    LDI R0, 93
    ST R7, R0
    LDI R0, 32
    ST R7, R0
    JMPX bug_monitor_write_message

bug_monitor_write_message:
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
bug_monitor_write_message_next:
    LD R0, R4
    LDI R1, 0
    CMP R0, R1
    BZX bug_monitor_write_message_done
    ST R7, R0
    INC R4
    JMPX bug_monitor_write_message_next
bug_monitor_write_message_done:
    LDI R0, 10
    ST R7, R0
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
    MOVSP R2
    LDI R7, 8
    ADD R2, R7
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
    LDI R7, 100
    CMP R0, R7
    BZX printf_signed_integer
    LDI R7, 105
    CMP R0, R7
    BZX printf_signed_integer
    LDI R7, 117
    CMP R0, R7
    BZX printf_unsigned_integer
    LDI R7, 99
    CMP R0, R7
    BZX printf_character
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
printf_signed_integer:
    LDW R0, R2
    INC R2
    INC R2
    LDI R7, 0
    CMP R0, R7
    BGEX printf_integer_digits
    LDI R7, 45
    LDI30 R6, 0x43200
    ST R6, R7
    INC R1
    NOT R0
    INC R0
    JMPX printf_integer_digits
printf_unsigned_integer:
    LDW R0, R2
    INC R2
    INC R2
printf_integer_digits:
    LDI R5, 0
    LDI R6, 10
printf_integer_divide:
    MOV R7, R0
    MOD R7, R6
    PUSH R7
    INC R5
    DIV R0, R6
    LDI R7, 0
    CMP R0, R7
    BNZX printf_integer_divide
printf_integer_emit:
    POP R0
    LDI R7, 48
    ADD R0, R7
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    DEC R5
    LDI R7, 0
    CMP R5, R7
    BNZX printf_integer_emit
    INC R4
    JMPX printf_next
printf_character:
    LDW R0, R2
    INC R2
    INC R2
    LDI30 R7, 0x43200
    ST R7, R0
    INC R1
    INC R4
    JMPX printf_next
printf_string:
    LDW R5, R2
    MOV R7, R2
    INC R7
    INC R7
    LDW R6, R7
    INC R2
    INC R2
    INC R2
    INC R2
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
