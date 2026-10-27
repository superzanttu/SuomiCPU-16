; Common kernel-mode firmware. BIOS services begin at 0x43400. The display and
; keyboard are memory-mapped at 0x43300 and 0x43000.
.address 0
bios_entry:
    LDI R4, 0
    LDI R0, 1
    LDA R5, 0x4342f
    ST R5, R0
    LDI R0, 0
    LDA R5, 0x4342e
    ST R5, R0
    LDI R0, 4
    LDA R5, 0x43003
    ST R5, R0
    LDI R0, 4
    LDA R5, 0x43302
    ST R5, R0
    LDI R0, 2
    LDA R5, 0x43303
    ST R5, R0
    LDI R0, 1
    ST R5, R0

    LDI R0, 1
    LDA R5, 0x43400
    ST R5, R0
    LDA R5, 0x43404
    LDW R1, R5
    LDA R5, 0x43402
    LDW R0, R5
    LDA R5, 0x43406
    LD R2, R5
    LDA R5, 0x5020
    ST R5, R2
    LDI R2, 0
    LDA R5, 0x5021
    ST R5, R2
    LDA R5, 0x5022
    ST R5, R2
    CMP R1, R4
    BZ empty_disks
    LDA R5, 0x5020
    LD R2, R5
    CMP R2, R4
    BNZ automatic_boot
    JMP menu_render

automatic_boot:
    LDA R5, 0x43402
    STW R5, R0
    LDA R6, startup_prompt
    CALL print_string
    LDI R3, 1
    LDA R5, 0x4342e
    ST R5, R3
autoboot_wait:
    LDA R5, 0x43000
    LD R2, R5
    CMP R2, R4
    BZ autoboot_wait
    LDI R3, 0x20
    CMP R2, R3
    BZ enter_setup
    LDI R3, 0x8a
    CMP R2, R3
    BNZ autoboot_wait
    LDI R3, 0
    LDA R5, 0x4342e
    ST R5, R3
    LDA R5, 0x43402
    LDW R0, R5
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 3
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZX boot_failed
    LDI R3, 0
    LDA R5, 0x4342f
    ST R5, R3
    JMPX autoboot_halted
enter_setup:
    LDI R3, 0
    LDA R5, 0x4342e
    ST R5, R3
    LDA R5, 0x43402
    LDW R0, R5
    JMPX menu_render
autoboot_halted:
    JMPX autoboot_halted

; The host raises key 0x8a after the real-time five-second grace period.

startup_prompt:
    .data string "SC-16 automatic startup in 5 seconds. Press SPACE for BIOS setup.\r\n\0"

print_string:
    LDA R5, 0x43300
print_string_next:
    LD R7, R6
    CMP R7, R4
    BZ print_string_done
    ST R5, R7
    INC R6
    JMP print_string_next
print_string_done:
    RET

print_string_limited:
    LDA R5, 0x43300
    LDI R3, 46
print_string_limited_next:
    LD R7, R6
    CMP R7, R4
    BZ print_string_limited_done
    ST R5, R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ print_string_limited_next
print_string_limited_done:
    RET

render_custom_settings:
    PUSH R0
    LDI R3, 0
    LDA R5, 0x5026
    ST R5, R3
render_custom_loop:
    LDA R5, 0x5026
    LD R3, R5
    LDA R5, 0x4340e
    LD R2, R5
    CMP R3, R2
    BZ render_custom_done
    MOV R0, R3
    LDA R5, 0x43402
    STW R5, R0
    LDI R2, 6
    LDA R5, 0x43400
    ST R5, R2
    LDI R6, 3
    ADD R6, R3
    LDA R5, 0x5023
    LD R2, R5
    CMP R2, R6
    BNZ render_custom_value
    PUSH R0
    PUSH R1
    LDA R7, 0x5120
    LDA R6, 0x43430
    LDI R3, 42
render_custom_edit_overlay:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ render_custom_edit_overlay
    POP R1
    POP R0
render_custom_value:
    LDA R5, 0x5026
    LD R3, R5
    LDI R6, 3
    ADD R6, R3
    LDA R5, 0x5021
    LD R2, R5
    CMP R2, R6
    BNZ render_custom_unselected
    LDI R3, 5
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, custom_selected
    JMP render_custom_prefix
render_custom_unselected:
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, custom_unselected
render_custom_prefix:
    CALL print_string
    LDA R6, 0x43430
    CALL print_string
    LDA R5, 0x43441
    LD R2, R5
    CMP R2, R4
    BZ render_custom_boolean
    LDI R3, 1
    CMP R2, R3
    BZ render_custom_string
    LDA R6, custom_number_type
    JMP render_custom_type
render_custom_boolean:
    LDA R6, custom_boolean_type
    JMP render_custom_type
render_custom_string:
    LDA R6, custom_string_type
render_custom_type:
    CALL print_string
    LDA R6, 0x43442
    CALL print_string
    LDA R6, newline
    CALL print_string
    LDA R5, 0x5026
    LD R3, R5
    INC R3
    ST R5, R3
    JMP render_custom_loop
render_custom_done:
    LDA R5, 0x5023
    LD R3, R5
    CMP R3, R4
    BZX render_custom_done_color
    LDI R6, 3
    CMP R3, R6
    BCX render_custom_done_color
    PUSH R1
    LDI R7, 0x5120
    LDA R6, 0x43430
    LDI R3, 42
render_custom_done_restore:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZX render_custom_done_restore
    POP R1
render_custom_done_color:
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    POP R0
    RET

empty_disks:
    LDI R0, 6
    LDA R5, 0x43301
    ST R5, R0
    LDA R6, no_disks
    CALL print_string
empty_wait:
    JMP empty_wait

menu_render:
    PUSH R0
    LDI R0, 1
    LDA R5, 0x43303
    ST R5, R0
    LDI R0, 6
    LDA R5, 0x43301
    ST R5, R0
    LDA R6, frame_top
    CALL print_string
    LDA R6, title
    CALL print_string
    LDA R6, frame_middle
    CALL print_string
    LDA R6, settings_heading
    CALL print_string
    LDA R6, frame_middle
    CALL print_string

    POP R0
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 0
    LDA R5, 0x43406
    STW R5, R3
    LDI R3, 0x5000
    LDA R5, 0x43408
    STW R5, R3
    LDI R3, 16
    LDA R5, 0x4340a
    STW R5, R3
    LDI R3, 2
    LDA R5, 0x43400
    ST R5, R3

    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, boot_label
    LDA R5, 0x5021
    LD R2, R5
    CMP R2, R4
    BZ render_boot_label
    LDA R6, boot_unselected
    JMP render_boot_label
render_boot_label:
    LDA R5, 0x5021
    LD R2, R5
    CMP R2, R4
    BNZ render_boot_color
    LDI R3, 5
    LDA R5, 0x43301
    ST R5, R3
render_boot_color:
    CALL print_string
    LDA R6, 0x5000
    CALL print_string
    LDA R6, boot_close
    CALL print_string
    LDA R6, newline
    CALL print_string
    LDI R3, 0
    LDA R5, 0x43406
    STW R5, R3
    LDI R3, 0x5080
    LDA R5, 0x43408
    STW R5, R3
    LDI R3, 128
    LDA R5, 0x4340a
    STW R5, R3
    LDI R3, 8
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    BNZ render_disk_info_invalid
    LDA R6, disk_info_prefix
    CALL print_string
    LDA R6, 0x5080
    CALL print_string_limited
    JMP render_disk_info_done
render_disk_info_invalid:
    LDA R6, disk_info_prefix
    CALL print_string
    LDA R6, invalid_disk_info
    CALL print_string_limited
render_disk_info_done:
    LDA R6, newline
    CALL print_string
    LDA R6, boot_suffix
    CALL print_string

    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, startup_label
    LDA R5, 0x5021
    LD R2, R5
    LDI R3, 1
    CMP R2, R3
    BZ render_startup_selected
    LDA R6, startup_unselected
    JMP render_startup_label
render_startup_selected:
    LDA R6, startup_label
render_startup_label:
    LDA R5, 0x5021
    LD R2, R5
    LDI R3, 1
    CMP R2, R3
    BZ render_startup_color_selected
    LDI R3, 1
    JMP render_startup_color
render_startup_color_selected:
    LDI R3, 5
render_startup_color:
    LDA R5, 0x43301
    ST R5, R3
    CALL print_string
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R5, 0x5020
    LD R2, R5
    CMP R2, R4
    BNZ render_yes
    LDA R6, value_no
    CALL print_string
    JMP render_node
render_yes:
    LDA R6, value_yes
    CALL print_string

render_node:
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, node_label
    LDA R5, 0x5021
    LD R2, R5
    LDI R3, 2
    CMP R2, R3
    BZ render_node_selected
    LDA R6, node_unselected
    JMP render_node_label
render_node_selected:
    LDA R6, node_label
render_node_label:
    LDA R5, 0x5021
    LD R2, R5
    LDI R3, 2
    CMP R2, R3
    BZ render_node_color_selected
    LDI R3, 1
    JMP render_node_color
render_node_color_selected:
    LDI R3, 5
render_node_color:
    LDA R5, 0x43301
    ST R5, R3
    CALL print_string
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, 0x43410
    CALL print_string
    LDA R6, newline
    CALL print_string
    CALL render_custom_settings
    LDA R5, 0x5023
    LD R2, R5
    CMP R2, R4
    BNZ render_edit_help
    LDA R6, help_text
    JMP render_help
render_edit_help:
    LDA R6, edit_active_help
render_help:
    CALL print_string

    LDA R5, 0x5022
    LD R2, R5
    CMP R2, R4
    BZ render_clean
    LDA R6, unsaved_warning
    CALL print_string
render_clean:
    LDA R5, 0x5021
    LD R2, R5
    CMP R2, R4
    BNZ render_not_boot
    LDA R5, 0x5022
    LD R2, R5
    CMP R2, R4
    BNZ render_no_start
    LDA R6, start_hint
    CALL print_string
    JMP render_bottom
render_no_start:
    LDA R6, dirty_hint
    CALL print_string
    JMP render_bottom
render_not_boot:
    LDA R6, edit_hint
    CALL print_string
render_bottom:
    LDA R6, frame_bottom
    CALL print_string
    JMP menu_input

menu_input:
    LDA R5, 0x5023
    LD R3, R5
    CMP R3, R4
    BNZ edit_input
    LDA R5, 0x43000
    LD R2, R5
    CMP R2, R4
    BZ menu_input
    LDI R3, 0x80
    CMP R2, R3
    BZ select_previous
    LDI R3, 0x81
    CMP R2, R3
    BZ select_next
    LDI R3, 0x82
    CMP R2, R3
    BZX disk_left
    LDI R3, 0x83
    CMP R2, R3
    BZX disk_right
    LDI R3, 13
    CMP R2, R3
    BZX activate_row
    LDI R3, 0x88
    CMP R2, R3
    BZX reload_settings
    LDI R3, 0x89
    CMP R2, R3
    BZX save_settings
    JMP menu_input

select_previous:
    LDA R5, 0x5021
    LD R3, R5
    CMP R3, R4
    BZ select_last
    DEC R3
    ST R5, R3
    JMP menu_render
select_last:
    LDA R5, 0x4340e
    LD R3, R5
    LDI R6, 2
    ADD R3, R6
    LDA R5, 0x5021
    ST R5, R3
    JMP menu_render
select_next:
    LDA R5, 0x5021
    LD R3, R5
    LDA R5, 0x4340e
    LD R6, R5
    LDI R5, 2
    ADD R6, R5
    CMP R3, R6
    BZ select_first
    INC R3
    LDA R5, 0x5021
    ST R5, R3
    JMP menu_render
select_first:
    LDI R3, 0
    LDA R5, 0x5021
    ST R5, R3
    JMP menu_render

edit_input:
    LDA R5, 0x43000
    LD R2, R5
    CMP R2, R4
    BZX menu_input
    LDI R6, 27
    CMP R2, R6
    BZX edit_cancel
    LDI R6, 13
    CMP R2, R6
    BZX edit_confirm
    LDI R6, 0x82
    CMP R2, R6
    BZX edit_toggle
    LDI R6, 0x83
    CMP R2, R6
    BZX edit_toggle
    LDI R6, 8
    CMP R2, R6
    BZX edit_backspace
    LDA R5, 0x5023
    LD R3, R5
    LDI R6, 1
    CMP R3, R6
    BZX edit_node_character
    LDI R6, 2
    CMP R3, R6
    BZX edit_node_character
    JMPX edit_custom_character

edit_toggle:
    LDA R5, 0x5023
    LD R3, R5
    LDI R6, 1
    CMP R3, R6
    BZX edit_startup_toggle
    LDI R6, 3
    CMP R3, R6
    BC menu_input
    LDA R5, 0x43441
    LD R6, R5
    CMP R6, R4
    BNZX menu_input
    LDA R7, 0x43442
    LD R6, R7
    LDI R5, 0x59
    CMP R6, R5
    BNZX edit_custom_set_yes
    LDI R6, 0x4e
    ST R7, R6
    INC R7
    LDI R6, 0x6f
    ST R7, R6
    INC R7
    LDI R6, 0
    ST R7, R6
    JMPX edit_custom_stage
edit_custom_set_yes:
    LDI R6, 0x59
    ST R7, R6
    INC R7
    LDI R6, 0x65
    ST R7, R6
    INC R7
    LDI R6, 0x73
    ST R7, R6
    INC R7
    LDI R6, 0
    ST R7, R6
    JMPX edit_custom_stage
edit_startup_toggle:
    LDA R5, 0x5020
    LD R3, R5
    LDI R6, 1
    SUB R6, R3
    ST R5, R6
    JMP menu_render

edit_backspace:
    LDA R5, 0x5023
    LD R3, R5
    LDI R6, 1
    CMP R3, R6
    BZX menu_input
    LDI R6, 2
    CMP R3, R6
    BZX edit_node_backspace
    LDA R5, 0x43441
    LD R6, R5
    CMP R6, R4
    BZX menu_input
    LDA R5, 0x5025
    LD R3, R5
    CMP R3, R4
    BZX menu_input
    DEC R3
    LDA R7, 0x43442
    ADD R7, R3
    LDI R6, 0
    ST R7, R6
    LDA R5, 0x5025
    ST R5, R3
    JMPX edit_custom_stage

edit_node_backspace:
    LDA R5, 0x5025
    LD R3, R5
    CMP R3, R4
    BZX menu_input
    DEC R3
    LDA R7, 0x43410
    ADD R7, R3
    LDI R6, 0
    ST R7, R6
    LDA R5, 0x5025
    ST R5, R3
    JMP menu_render

edit_node_character:
    LDI R6, 0x20
    CMP R2, R6
    BC menu_input
    LDI R6, 0x7f
    CMP R2, R6
    BZ menu_input
    BCX edit_node_append
    JMP menu_input
edit_node_append:
    LDA R5, 0x5025
    LD R3, R5
    LDI R6, 16
    CMP R3, R6
    BZ menu_input
    LDA R7, 0x43410
    ADD R7, R3
    ST R7, R2
    INC R3
    INC R7
    LDI R6, 0
    ST R7, R6
    LDA R5, 0x5025
    ST R5, R3
    JMP menu_render

edit_custom_character:
    LDA R5, 0x43441
    LD R6, R5
    LDI R5, 1
    CMP R6, R5
    BZX edit_custom_string_character
    LDI R5, 2
    CMP R6, R5
    BNZ menu_input
    LDI R6, 0x2d
    CMP R2, R6
    BZX edit_custom_number_minus
    LDI R6, 0x30
    CMP R2, R6
    BC menu_input
    LDI R6, 0x3a
    CMP R2, R6
    BCX edit_custom_append
    JMP menu_input
edit_custom_number_minus:
    LDA R5, 0x5025
    LD R3, R5
    CMP R3, R4
    BNZ menu_input
    JMPX edit_custom_append
edit_custom_string_character:
    LDI R6, 0x20
    CMP R2, R6
    BC menu_input
    LDI R6, 0x7f
    CMP R2, R6
    BCX edit_custom_append
    JMP menu_input
edit_custom_append:
    LDA R5, 0x5025
    LD R3, R5
    LDA R5, 0x43441
    LD R6, R5
    LDI R5, 1
    CMP R6, R5
    BZX edit_custom_string_limit
    LDI R6, 20
    JMPX edit_custom_check_limit
edit_custom_string_limit:
    LDI R6, 16
edit_custom_check_limit:
    CMP R3, R6
    BZ menu_input
    LDA R7, 0x43442
    ADD R7, R3
    ST R7, R2
    INC R3
    INC R7
    LDI R6, 0
    ST R7, R6
    LDA R5, 0x5025
    ST R5, R3
    JMPX edit_custom_stage

edit_custom_stage:
    PUSH R0
    LDA R7, 0x43430
    LDI R6, 0x5120
    LDI R3, 42
edit_custom_stage_loop:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ edit_custom_stage_loop
    POP R0
    JMP menu_render

edit_cancel:
    LDA R5, 0x5023
    LD R3, R5
    LDI R6, 1
    CMP R3, R6
    BZX edit_cancel_startup
    LDI R6, 2
    CMP R3, R6
    BZX edit_cancel_node
    JMPX edit_finish
edit_cancel_startup:
    LDA R5, 0x5024
    LD R3, R5
    LDA R5, 0x5020
    ST R5, R3
    JMPX edit_finish
edit_cancel_node:
    LDA R7, 0x5030
    LDA R6, 0x43410
    LDI R3, 17
edit_cancel_node_loop:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ edit_cancel_node_loop
    JMPX edit_finish

edit_confirm:
    LDA R5, 0x5023
    LD R3, R5
    LDI R6, 1
    CMP R3, R6
    BZX edit_confirm_startup
    LDI R6, 2
    CMP R3, R6
    BZX edit_confirm_node
    JMPX edit_confirm_custom
edit_confirm_startup:
    LDA R5, 0x5020
    LD R3, R5
    LDA R5, 0x5024
    LD R2, R5
    CMP R3, R2
    BZX edit_finish
    JMPX edit_mark_dirty
edit_confirm_node:
    LDA R7, 0x43410
    LDI R6, 0x5030
    LDI R3, 17
edit_confirm_node_loop:
    LD R2, R7
    LD R5, R6
    CMP R2, R5
    BNZ edit_mark_dirty
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ edit_confirm_node_loop
    JMPX edit_finish
edit_confirm_custom:
    LDA R7, 0x5120
    LDI R6, 0x5050
    LDI R3, 42
edit_confirm_custom_compare:
    LD R2, R7
    LD R5, R6
    CMP R2, R5
    BNZ edit_confirm_custom_write
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ edit_confirm_custom_compare
    JMPX edit_finish
edit_confirm_custom_write:
    PUSH R0
    LDA R7, 0x5120
    LDA R6, 0x43430
    LDI R3, 42
edit_confirm_custom_copy:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZ edit_confirm_custom_copy
    LDA R5, 0x5023
    LD R0, R5
    LDI R3, 3
    SUB R0, R3
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 7
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    POP R0
    CMP R3, R4
    BNZ settings_failed
edit_mark_dirty:
    LDI R3, 1
    LDA R5, 0x5022
    ST R5, R3
edit_finish:
    LDI R3, 0
    LDA R5, 0x5023
    ST R5, R3
    JMP menu_render

disk_left:
    LDA R5, 0x5021
    LD R3, R5
    CMP R3, R4
    BNZ menu_input
    LDA R5, 0x43402
    LDW R0, R5
    CMP R0, R4
    BZX disk_left_wrap
    DEC R0
    JMPX disk_changed
disk_left_wrap:
    MOV R0, R1
    DEC R0
    JMPX disk_changed
disk_right:
    LDA R5, 0x5021
    LD R3, R5
    CMP R3, R4
    BNZ menu_input
    LDA R5, 0x43402
    LDW R0, R5
    INC R0
    CMP R0, R1
    BZX disk_right_wrap
    JMPX disk_changed
disk_right_wrap:
    LDI R0, 0
disk_changed:
    JMP menu_render

activate_row:
    LDA R5, 0x5021
    LD R3, R5
    CMP R3, R4
    BZX boot_selected
    LDI R6, 1
    CMP R3, R6
    BZX edit_startup
    LDI R6, 2
    CMP R3, R6
    BZX edit_node
    LDI R6, 3
    SUB R3, R6
    PUSH R0
    MOV R0, R3
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 6
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ activate_custom_failed
    POP R0
    JMPX edit_custom
activate_custom_failed:
    POP R0
    JMPX settings_failed

edit_startup:
    LDA R5, 0x5020
    LD R3, R5
    LDA R6, 0x5024
    ST R6, R3
    LDI R3, 1
    LDA R5, 0x5023
    ST R5, R3
    JMP menu_render
edit_node:
    LDI R3, 0
    LDA R5, 0x5025
    ST R5, R3
    LDA R7, 0x43410
node_length:
    LD R2, R7
    CMP R2, R4
    BZX node_length_done
    INC R3
    INC R7
    JMPX node_length
node_length_done:
    LDA R5, 0x5025
    ST R5, R3
node_backup:
    LDA R7, 0x43410
    LDI R6, 0x5030
    LDI R3, 17
node_backup_loop:
    LD R2, R7
    ST R6, R2
    INC R7
    INC R6
    DEC R3
    CMP R3, R4
    BNZX node_backup_loop
    LDI R3, 2
    LDA R5, 0x5023
    ST R5, R3
    JMP menu_render
edit_custom:
    LDA R5, 0x5021
    LD R3, R5
    LDA R5, 0x5023
    ST R5, R3
    PUSH R0
    PUSH R1
    LDA R7, 0x43430
    LDI R6, 0x5050
    LDI R1, 0x5120
    LDI R3, 42
custom_backup_loop:
    LD R2, R7
    ST R6, R2
    ST R1, R2
    INC R7
    INC R6
    INC R1
    DEC R3
    CMP R3, R4
    BNZ custom_backup_loop
    POP R1
    POP R0
    LDA R5, 0x5025
    LDI R3, 0
    ST R5, R3
    LDA R7, 0x43442
custom_value_length:
    LD R2, R7
    CMP R2, R4
    BZX custom_edit_ready
    INC R3
    INC R7
    JMPX custom_value_length
custom_edit_ready:
    LDA R5, 0x5025
    ST R5, R3
    JMP menu_render
reload_settings:
    LDI R3, 5
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ settings_failed
    LDA R5, 0x43404
    LDW R1, R5
    LDA R5, 0x43402
    LDW R0, R5
    LDA R5, 0x43406
    LD R2, R5
    LDA R5, 0x5020
    ST R5, R2
    LDI R3, 0
    LDA R5, 0x5022
    ST R5, R3
    LDA R5, 0x5023
    ST R5, R3
    LDA R6, settings_loaded
    CALL print_string
    JMP menu_render

save_settings:
    LDA R5, 0x43402
    STW R5, R0
    LDA R5, 0x5020
    LD R2, R5
    LDA R5, 0x43406
    ST R5, R2
    LDI R3, 4
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ settings_failed
    LDI R3, 0
    LDA R5, 0x5022
    ST R5, R3
    LDA R6, settings_saved
    CALL print_string
    JMP menu_render
settings_failed:
    LDA R6, settings_error
    CALL print_string
    JMP menu_input

boot_selected:
    LDA R5, 0x5022
    LD R3, R5
    CMP R3, R4
    BNZ boot_unsaved
    LDA R5, 0x5021
    LD R3, R5
    CMP R3, R4
    BNZ menu_input
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 9
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ settings_failed
    LDA R5, 0x43402
    LDW R0, R5
    LDI R3, 3
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ boot_failed
    LDA R5, 0x4342f
    ST R5, R4
    JMP menu_input
boot_unsaved:
    LDA R6, unsaved_warning
    CALL print_string
    JMP menu_render
boot_failed:
    LDA R6, boot_error
    CALL print_string
    JMP menu_input

.address 0x1000
frame_top:
    .data string "+-------------------------------------------------+\r\n\0"
title:
    .data string "|            SC-16 BIOS SETUP UTILITY            |\r\n\0"
frame_middle:
    .data string "|-------------------------------------------------|\r\n\0"
settings_heading:
    .data string "|                    SETTINGS                     |\r\n\0"
boot_label:
    .data string "|  > Boot device: [ \0"
boot_unselected:
    .data string "|    Boot device: [ \0"
boot_suffix:
    .data string "|  LEFT/RIGHT changes device.\r\n\0"
boot_close:
    .data string " ]\0"
disk_info_prefix:
    .data string "|  \0"
startup_label:
    .data string "|  > Automatic startup: \0"
startup_unselected:
    .data string "|    Automatic startup: \0"
node_label:
    .data string "|  > SC-NET node ID: \0"
node_unselected:
    .data string "|    SC-NET node ID: \0"
custom_selected:
    .data string "|  > \0"
custom_unselected:
    .data string "|    \0"
custom_boolean_type:
    .data string " [BOOL] \0"
custom_string_type:
    .data string " [STRING] \0"
custom_number_type:
    .data string " [NUMBER] \0"
value_yes:
    .data string "Yes\r\n\0"
value_no:
    .data string "No\r\n\0"
newline:
    .data string "\r\n\0"
help_text:
    .data string "|\r\n|  UP/DOWN select. ENTER edits or starts.\r\n|  F9 reloads; F10 saves settings.\r\n|  BOOL Yes/No, STRING max 16, NUMBER integer.\r\n\0"
edit_active_help:
    .data string "|\r\n|  Editing in place: LEFT/RIGHT changes BOOL.\r\n|  Type STRING/NUMBER; BACKSPACE deletes.\r\n|  ENTER confirms; ESC cancels.\r\n\0"
start_hint:
    .data string "|  Press ENTER to start from selected device.\r\n\0"
edit_hint:
    .data string "|  Press ENTER to modify the selected setting.\r\n\0"
dirty_hint:
    .data string "|  Save changes with F10 before starting.\r\n\0"
unsaved_warning:
    .data string "|  WARNING: settings changed but not saved. F10 saves.\r\n\0"
frame_bottom:
    .data string "+-------------------------------------------------+\r\n\0"
settings_loaded:
    .data string "|  Settings reloaded from bios.json.\r\n\0"
settings_saved:
    .data string "|  Settings saved to bios.json.\r\n\0"
settings_error:
    .data string "|  Settings operation failed; see Bug Monitor.\r\n\0"
no_disks:
    .data string "+-------------------------------------------------+\r\n|                 NO BOOT DISKS                  |\r\n|  No boot disks found.                           |\r\n+-------------------------------------------------+\r\n\0"
boot_failed_text:
    .data string "|  Boot failed. Select another disk.\r\n\0"
invalid_disk_info:
    .data string "INVALID DISK METADATA OR IMAGE\0"
boot_error:
    .data string "|  Boot failed. Select another disk.\r\n\0"
