.address 0x2800

sc16_getpid:
    LDI R0, 0
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_exit:
    LDWS R1, 4
    LDI R0, 1
    LDI R7, 1
    SVC
    RET

sc16_yield:
    LDI R0, 2
    LDI R7, 1
    SVC
    RET

sc16_spawn:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 3
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_mmap:
    LDWS R4, 12
    LDWS R5, 10
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDWS R2, 8
    AND R2, R6
    LDWS R4, 4
    LDWS R5, 6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 4
    LDI R7, 1
    SVC
    JMPX __sc16_mmap_result_long

sc16_open:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R2, 8
    AND R2, R6
    LDI R0, 5
    LDI R7, 1
    SVC
    RET

sc16_read:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 6
    LDI R7, 1
    SVC
    RET

sc16_write:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 7
    LDI R7, 1
    SVC
    RET

sc16_close:
    LDWS R1, 4
    LDI R6, 0xFFFF
    AND R1, R6
    LDI R0, 8
    LDI R7, 1
    SVC
    RET

sc16_seek:
    LDWS R1, 4
    LDWS R2, 6
    LDWS R3, 8
    LDWS R4, 10
    LDI R6, 0xFFFF
    AND R1, R6
    AND R2, R6
    AND R3, R6
    AND R4, R6
    LDI R0, 12
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_net_discover:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 8
    AND R3, R6
    LDI R1, 0
    LDI R0, 13
    LDI R7, 1
    SVC
    RET

sc16_net_send:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDWS R4, 12
    AND R4, R6
    LDI R1, 1
    LDI R0, 13
    LDI R7, 1
    SVC
    RET

sc16_net_receive:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDWS R4, 12
    AND R4, R6
    LDI R1, 2
    LDI R0, 13
    LDI R7, 1
    SVC
    RET

sc16_sleep:
    LDWS R4, 4
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 9
    LDI R7, 1
    SVC
    RET

sc16_wait:
    LDWS R4, 4
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 10
    LDI R7, 1
    SVC
    RET

sc16_semop:
    LDWS R1, 4
    LDWS R4, 6
    LDWS R5, 8
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 11
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

__sc16_id_result_long:
    MOV R5, R0
    SPLIT30 R5, R1
    LDI R6, 0x3FFF
    CMP R5, R6
    BNZX __sc16_id_result_success
    LDI R6, 0x8000
    AND R1, R6
    LDI R7, 0
    CMP R1, R7
    BZX __sc16_id_result_success
__sc16_id_result_error:
    MOV R1, R0
    LDI R6, 0xFFFF
    AND R1, R6
    LDI R0, 0xFFFF
    RET
__sc16_id_result_success:
    MOV R0, R5
    RET

__sc16_mmap_result_long:
    MOV R5, R0
    SPLIT30 R5, R1
    LDI R6, 0x0FFF
    AND R6, R1
    LDI R7, 0
    CMP R6, R7
    BZX __sc16_mmap_result_success
    MOV R1, R0
    LDI R6, 0xFFFF
    AND R1, R6
    LDI R0, 0xFFFF
    RET
__sc16_mmap_result_success:
    MOV R0, R5
    RET

sc16_spawn_arg:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 8
    LDWS R5, 10
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDI R0, 17
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_startup_argument:
    LDI R0, 15
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_monotonic_ms:
    LDI R0, 16
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_process_info:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 14
    LDI R7, 1
    SVC
    RET

sc16_trywait:
    LDWS R4, 4
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 18
    LDI R7, 1
    SVC
    RET

sc16_kill:
    LDWS R4, 4
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R2, 8
    AND R2, R6
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 38
    LDI R7, 1
    SVC
    RET

sc16_sleep_ms:
    LDWS R4, 4
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 19
    LDI R7, 1
    SVC
    RET

sc16_process_status:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 20
    LDI R7, 1
    SVC
    RET

sc16_process_accounting:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 40
    LDI R7, 1
    SVC
    RET

sc16_directory_entry:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R2, 8
    AND R2, R6
    LDWS R4, 12
    LDWS R5, 10
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDWS R4, 14
    AND R4, R6
    LDI R0, 33
    LDI R7, 1
    SVC
    RET

sc16_mkdir:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R2, 0
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 34
    LDI R7, 1
    SVC
    RET

sc16_delete_file:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R2, 0
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 35
    LDI R7, 1
    SVC
    RET

sc16_rename_file:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 39
    LDI R7, 1
    SVC
    RET

sc16_set_file_mode:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R2, 8
    AND R2, R6
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 36
    LDI R7, 1
    SVC
    RET

sc16_set_file_owner:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R4, 14
    LDWS R5, 12
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 37
    LDI R7, 1
    SVC
    RET

sc16_spawn_exec:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 12
    AND R3, R6
    LDWS R4, 14
    AND R4, R6
    LDI R0, 21
    LDI R7, 1
    SVC
    JMPX __sc16_id_result_long

sc16_argument_count:
    LDI R0, 22
    LDI R7, 1
    SVC
    RET

sc16_argument:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R3, 10
    AND R3, R6
    LDI R0, 23
    LDI R7, 1
    SVC
    RET

sc16_graphics:
    LDWS R1, 4
    LDWS R4, 8
    LDWS R5, 6
    LDI R6, 0xFFFF
    AND R1, R6
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R4, 12
    LDWS R5, 10
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDWS R4, 14
    AND R4, R6
    LDI R5, 0
    LDI R6, 0
    LDI R0, 24
    LDI R7, 1
    SVC
    RET

sc16_input_snapshot:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R2, 0
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 25
    LDI R7, 1
    SVC
    RET

sc16_account_status:
    LDI R0, 26
    LDI R7, 1
    SVC
    RET

sc16_account_initialize_root:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R2, 0
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 27
    LDI R7, 1
    SVC
    RET

sc16_authenticate:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 28
    LDI R7, 1
    SVC
    RET

sc16_console_echo:
    LDWS R1, 4
    LDI R6, 0xFFFF
    AND R1, R6
    LDI R0, 29
    LDI R7, 1
    SVC
    RET

sc16_group_add:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDI R0, 30
    LDI R7, 1
    SVC
    RET

sc16_user_add:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDWS R4, 14
    LDWS R5, 12
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R3, R4
    LDI R0, 31
    LDI R7, 1
    SVC
    RET

sc16_set_password:
    LDWS R4, 6
    LDWS R5, 4
    LDI R6, 0xFFFF
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R1, R4
    LDWS R4, 10
    LDWS R5, 8
    AND R4, R6
    AND R5, R6
    JOIN30 R4, R5
    MOV R2, R4
    LDI R3, 0
    LDI R4, 0
    LDI R5, 0
    LDI R6, 0
    LDI R0, 32
    LDI R7, 1
    SVC
    RET
