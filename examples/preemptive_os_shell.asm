.address 0x5000
shell_start:
    LDI R4, 0
    LDA R6, shell_banner
    CALLX print_string

shell_prompt:
    LDA R6, prompt
    CALLX print_string
    LDI R5, 0
shell_read:
    LDI R0, 6
    LDI R1, 0
    LDA R2, input_byte
    LDI R3, 1
    LDI R7, 1
    SVC
    CMP R0, R4
    BZX shell_yield
    LDA R1, input_byte
    LD R2, R1
    LDI R1, 10
    CMP R2, R1
    BZX shell_submit
    LDI R1, 8
    CMP R2, R1
    BZX shell_backspace
    LDI R1, 31
    CMP R5, R1
    BZX shell_overflow
    LDA R1, command_line
    ADD R1, R5
    ST R1, R2
    INC R5
    JMPX shell_read
shell_yield:
    LDI R0, 2
    LDI R7, 1
    SVC
    JMPX shell_read
shell_backspace:
    CMP R5, R4
    BZX shell_read
    DEC R5
    LDA R1, command_line
    ADD R1, R5
    ST R1, R4
    JMPX shell_read
shell_overflow:
    LDA R6, line_too_long
    CALLX print_string
    LDI R5, 0
    JMPX shell_prompt

shell_submit:
    LDA R1, command_line
    ADD R1, R5
    ST R1, R4
    LDA R6, command_line
    LDA R3, command_help
    CALLX string_equal
    LDI R1, 1
    CMP R0, R1
    BZX command_help_selected
    LDA R6, command_line
    LDA R3, command_list
    CALLX string_equal
    LDI R1, 1
    CMP R0, R1
    BZX command_list_selected
    LDA R6, command_line
    LDA R3, command_info
    CALLX string_equal
    LDI R1, 1
    CMP R0, R1
    BZX command_info_selected
    LDA R6, command_line
    LDA R3, command_run_demo
    CALLX string_equal
    LDI R1, 1
    CMP R0, R1
    BZX command_run_demo_selected
    LDA R6, unknown_command
    CALLX print_string
    JMPX shell_prompt

command_help_selected:
    LDA R6, help_text
    CALLX print_string
    JMPX shell_prompt
command_list_selected:
    LDA R6, list_text
    CALLX print_string
    JMPX shell_prompt
command_info_selected:
    LDA R6, info_text
    CALLX print_string
    JMPX shell_prompt
command_run_demo_selected:
    LDI R0, 3
    LDA R1, demo_path
    LDI R7, 1
    SVC
    CMP R0, R4
    BNX spawn_failed
    LDA R1, child_id
    ST R1, R0
    LDA R6, demo_started
    CALLX print_string
    LDI R2, 64
shell_preempt_work:
    SUB R2, 1
    BNZ shell_preempt_work
    LDA R6, waiting_for_demo
    CALLX print_string
    LDI R0, 10
    LDA R1, child_id
    LD R1, R1
    LDI R7, 1
    SVC
    LDI R1, 7
    CMP R0, R1
    BNZX demo_failed
    LDA R6, demo_finished
    CALLX print_string
    JMPX shell_prompt
spawn_failed:
    LDA R6, spawn_error
    CALLX print_string
    JMPX shell_prompt
demo_failed:
    LDA R6, demo_error
    CALLX print_string
    JMPX shell_prompt

string_equal:
string_equal_loop:
    LD R0, R6
    LD R1, R3
    CMP R0, R1
    BNZX string_not_equal
    CMP R0, R4
    BZX string_is_equal
    INC R6
    INC R3
    JMPX string_equal_loop
string_not_equal:
    LDI R0, 0
    RET
string_is_equal:
    LDI R0, 1
    RET

print_string:
    LDI R3, 0
print_string_copy:
    LD R2, R6
    CMP R2, R4
    BZX print_string_done
    LDA R1, output_buffer
    ADD R1, R3
    ST R1, R2
    INC R3
    INC R6
    JMPX print_string_copy
print_string_done:
    CMP R3, R4
    BZX print_string_return
    LDI R0, 7
    LDI R1, 1
    LDA R2, output_buffer
    LDI R7, 1
    SVC
print_string_return:
    RET

.address 0x6000
command_line:
    .data hex 00
.address 0x6100
input_byte:
    .data hex 00
child_id:
    .data hex 00, 00
.address 0x6400
output_buffer:
    .data hex 00
.address 0x6200
shell_banner:
    .data string "SisuOS interactive shell\r\n\0"
prompt:
    .data string "sc16> \0"
help_text:
    .data string "Commands: help, list, run demo, info\r\n\0"
list_text:
    .data string "Programs: demo\r\n\0"
info_text:
    .data string "SisuOS | Protected mode | Syscall ABI 1 | Preemptive scheduler active\r\n\0"
demo_started:
    .data string "Running demo...\r\n\0"
waiting_for_demo:
    .data string "Waiting for demo...\r\n\0"
demo_finished:
    .data string "demo exited with status 7\r\n\0"
spawn_error:
    .data string "Unable to start demo\r\n\0"
demo_error:
    .data string "demo returned an unexpected status\r\n\0"
unknown_command:
    .data string "Unknown command. Type help.\r\n\0"
line_too_long:
    .data string "Command too long (31 characters maximum)\r\n\0"
command_help:
    .data string "help\0"
command_list:
    .data string "list\0"
command_info:
    .data string "info\0"
command_run_demo:
    .data string "run demo\0"
demo_path:
    .data string "demo.bin\0"
