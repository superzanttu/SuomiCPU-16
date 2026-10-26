; Common kernel-mode firmware. BIOS services begin at 0x43400; settings text
; uses 0x43410-0x43420. Display and keyboard are mapped at 0x43300 and 0x43000.
.address 0
bios_entry:
    ; Keep zero in R4 for comparisons and terminate the initial screen.
    LDI R4, 0
    ; Select the fast execution mode, clear the text display, then enable it.
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
    LDI R0, 3
    LDA R5, 0x43301
    ST R5, R0

    ; BIOS command 1 returns the disk count, preferred disk, startup behavior,
    ; and SC-NET node ID.
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
    CMP R1, R4
    BZ empty_disks
    LDA R5, 0x5020
    LD R2, R5
    CMP R2, R4
    BZ menu_render
    JMP boot_selected

    ; Stay in the no-disk screen until reset; there is nothing to boot.
empty_disks:
    LDI R0, 1
    LDA R5, 0x43303
    ST R5, R0
    LDI R0, 6
    LDA R5, 0x43301
    ST R5, R0
    LDA R6, no_disks
    CALL print_string
empty_wait:
    JMP empty_wait

menu_render:
    ; Redraw the menu without changing R0, the selected disk index.
    LDI R3, 1
    LDA R5, 0x43303
    ST R5, R3
    LDI R3, 6
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, frame_top
    CALL print_string
    LDA R6, title
    CALL print_string
    LDA R6, frame_middle
    CALL print_string
    LDA R6, boot_heading
    CALL print_string
    LDA R6, frame_middle
    CALL print_string

    ; BIOS command 2 copies the selected disk ID into the buffer at 0x5000.
    ; R0 is the disk index; R3 supplies the output pointer and capacity.
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
    LDA R6, selected_label
    CALL print_string
    LDI R3, 5
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, 0x5000
    CALL print_string
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, selected_end
    CALL print_string
    LDA R6, menu_instructions
    CALL print_string
    LDA R6, startup_label
    CALL print_string
    LDA R5, 0x5020
    LD R2, R5
    CMP R2, R4
    BNZ startup_auto
    LDA R6, startup_selector
    CALL print_string
    JMP menu_node_name
startup_auto:
    LDA R6, startup_boot
    CALL print_string
menu_node_name:
    LDA R6, node_label
    CALL print_string
    LDI R3, 5
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, 0x43410
    CALL print_string
    LDI R3, 1
    LDA R5, 0x43301
    ST R5, R3
    LDA R6, menu_footer
    CALL print_string
    LDA R6, frame_bottom
    CALL print_string
    JMP menu_input

menu_input:
    ; Reading the keyboard register consumes the pending key atomically.
    LDA R5, 0x43000
    LD R2, R5
    CMP R2, R4
    BZ menu_input
    LDI R3, 0x80
    CMP R2, R3
    BZ previous_disk
    LDI R3, 0x81
    CMP R2, R3
    BZ next_disk
    LDI R3, 13
    CMP R2, R3
    BZ boot_selected
    LDI R3, 0x73
    CMP R2, R3
    BZ save_settings
    LDI R3, 0x61
    CMP R2, R3
    BZ toggle_startup
    LDI R3, 0x6e
    CMP R2, R3
    BZ edit_network_name
    JMP menu_input

toggle_startup:
    LDA R5, 0x5020
    LD R3, R5
    CMP R3, R4
    BNZ startup_disable
    LDI R3, 1
    ST R5, R3
    JMP menu_render
startup_disable:
    LDI R3, 0
    ST R5, R3
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
    LDA R6, settings_saved
    CALL print_string
    JMP menu_input
settings_failed:
    LDA R6, settings_error
    CALL print_string
    JMP menu_input

edit_network_name:
    PUSH R0
    PUSH R1
    LDI R3, 0
    LDA R7, 0x43410
network_length:
    LD R2, R7
    CMP R2, R4
    BZ network_draw
    INC R3
    INC R7
    JMP network_length
network_draw:
    LDI R0, 1
    LDA R5, 0x43303
    ST R5, R0
    LDI R0, 1
    LDA R5, 0x43301
    ST R5, R0
    LDA R6, network_prompt
    CALL print_string
    LDI R0, 5
    LDA R5, 0x43301
    ST R5, R0
    LDA R6, 0x43410
    CALL print_string
network_input:
    LDA R5, 0x43000
    LD R2, R5
    CMP R2, R4
    BZ network_input
    LDI R6, 13
    CMP R2, R6
    BZ network_done
    LDI R6, 8
    CMP R2, R6
    BZ network_backspace
    LDI R6, 0x80
    CMP R2, R6
    BZ network_input
    LDI R6, 0x81
    CMP R2, R6
    BZ network_input
    LDI R6, 0x20
    CMP R2, R6
    BC network_input
    LDI R6, 0x7f
    CMP R2, R6
    BZ network_input
    BC network_append
    JMP network_input
network_append:
    LDI R6, 16
    CMP R3, R6
    BZ network_input
    LDA R7, 0x43410
    ADD R7, R3
    ST R7, R2
    INC R3
    INC R7
    LDI R6, 0
    ST R7, R6
    JMP network_draw
network_backspace:
    CMP R3, R4
    BZ network_input
    DEC R3
    LDA R7, 0x43410
    ADD R7, R3
    LDI R6, 0
    ST R7, R6
    JMP network_draw
network_done:
    POP R1
    POP R0
    JMP menu_render

; Wrap to the last disk when Up is pressed at index zero.
previous_disk:
    LDA R5, 0x43402
    LDW R0, R5
    BNZ previous_disk_decrement
    MOV R0, R1
    DEC R0
    JMP menu_render
previous_disk_decrement:
    DEC R0
    JMP menu_render

; Wrap to the first disk when Down is pressed past the final index.
next_disk:
    INC R0
    CMP R0, R1
    BZ first_disk
    JMP menu_render
first_disk:
    LDI R0, 0
    JMP menu_render

; BIOS command 3 boots the selected index. A nonzero status means it failed.
boot_selected:
    LDA R5, 0x43402
    STW R5, R0
    LDI R3, 3
    LDA R5, 0x43400
    ST R5, R3
    LDA R5, 0x4340c
    LD R3, R5
    CMP R3, R4
    BNZ boot_failed
    JMP menu_input

; Keep the selector available so the user can choose a different disk.
boot_failed:
    LDI R3, 1
    LDA R5, 0x43303
    ST R5, R3
    LDA R6, boot_error
    CALL print_string
    JMP menu_input

; Print the zero-terminated string addressed by R6 to the text output port.
; R4 must contain zero; this routine advances R6 and uses R5 and R7.
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

.address 0x0400
; Firmware UI strings are explicitly zero-terminated by the assembler source.
frame_top:
    .data string "+-------------------------------------------------+\r\n\0"
title:
    .data string "|            SC-16 BIOS SETUP UTILITY            |\r\n\0"
frame_middle:
    .data string "|-------------------------------------------------|\r\n\0"
boot_heading:
    .data string "|                 BOOT SELECTION                  |\r\n\0"
selected_label:
    .data string "|  Boot device: [ \0"
selected_end:
    .data string " ]\r\n\0"
menu_instructions:
    .data string "|\r\n|  Use UP/DOWN to choose a boot device.\r\n|  Press ENTER to start the selected device.\r\n|\r\n"
startup_label:
    .data string "|  Startup behavior: \0"
startup_selector:
    .data string "Selector\r\n\0"
startup_boot:
    .data string "Boot preferred disk\r\n\0"
node_label:
    .data string "|  SC-NET node ID: \0"
menu_footer:
    .data string "|\r\n|  N edit node ID   A toggle startup   S save\r\n|  Settings persist when saved with S.\r\n|  Emulator ID is separate from guest app titles.\r\n\0"
frame_bottom:
    .data string "+-------------------------------------------------+\r\n\0"
network_prompt:
    .data string "+-------------------------------------------------+\r\n|                 SC-NET NODE ID                  |\r\n|  Set this emulator's identity on SC-NET.\r\n|  Guest applications use their own network title.\r\n|\r\n|  Node ID (1-16 printable ASCII characters):\r\n|  \0"
settings_saved:
    .data string "\r\nSettings saved to bios.json.\r\n\0"
settings_error:
    .data string "\r\nCould not save settings; see Bug Monitor.\r\n\0"
no_disks:
    .data string "+-------------------------------------------------+\r\n|                 NO BOOT DISKS                  |\r\n|  No boot disks found.                           |\r\n+-------------------------------------------------+\r\n\0"
boot_error:
    .data string "\r\nBoot failed. Select another disk.\r\n\0"
