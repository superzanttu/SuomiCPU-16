; Common supervisor-mode firmware. BIOS services use 0x43400-0x4340C;
; the text display and keyboard are memory-mapped at 0x43300 and 0x43000.
.address 0
bios_entry:
    ; Keep zero in R4 for comparisons and terminate the initial screen.
    LDI R4, 0
    ; Select the fast execution mode, clear the text display, then enable it.
    LDI R0, 4
    LDA R5, 0x43003
    ST R5, R0
    LDI R0, 2
    LDA R5, 0x43303
    ST R5, R0
    LDI R0, 1
    ST R5, R0
    LDI R0, 3
    LDA R5, 0x43301
    ST R5, R0

    ; BIOS command 1 returns the number of disks in R1 and the initial
    ; selection index in R0 (including any runtime-provided preselection).
    LDI R0, 1
    LDA R5, 0x43400
    ST R5, R0
    LDA R5, 0x43404
    LDW R1, R5
    LDA R5, 0x43402
    LDW R0, R5
    CMP R1, R4
    BNZ menu_render

    ; Stay in the no-disk screen until reset; there is nothing to boot.
empty_disks:
    LDI R0, 1
    LDA R5, 0x43303
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
    LDA R6, title
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

    LDA R6, selected_label
    CALL print_string
    LDA R6, 0x5000
    CALL print_string
    LDA R6, menu_instructions
    CALL print_string

menu_input:
    ; Consume one key at a time; zero means no key is waiting.
    LDA R5, 0x43000
    LD R2, R5
    ST R5, R4
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
    JMP menu_input

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

.address 0x0200
; Firmware UI strings are explicitly zero-terminated by the assembler source.
title:
    .data string "SC-16 SYSTEM BIOS\n\n\0"
selected_label:
    .data string "Selected disk: \0"
menu_instructions:
    .data string "\n\nUP/DOWN selects; ENTER boots.\n\0"
no_disks:
    .data string "No boot disks found.\n\0"
boot_error:
    .data string "\nBoot failed. Select another disk.\n\0"
