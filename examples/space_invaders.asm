; Minimal Space-Invaders-style demo:
; A/D move the blue ship along the bottom; SPACE fires a white shot marker.
; The alien formation is a row of bright pixels near the top.
.include "../fonts/font5x7.asm"
.address 0

start:
    LDI R0, 40
    LDI R1, 40
    LDI R2, 5
    LDI R3, 3
    LDI R4, 25
    CALL GFX_RECT           ; Draw a simple alien formation
    LDI R0, 0xFFFF
    INC R0
    MOV R6, R0
    ADD R0, R6
    ADD R0, R6             ; VRAM base 0x30000
    LDI R1, 160            ; Ship x coordinate
    LDI R3, 25             ; Blue ship
    LDI R4, 0xFFFF
    MOV R6, R4
    ADD R4, R6
    ADD R4, R4
    LDI R6, 0x3004
    ADD R4, R6             ; keyboard address 0x43000
    LDI R5, 64000          ; Ship row 200
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    ST R6, R3

main_loop:
    LD R2, R4
    JZ R2, main_loop
    LDI R7, 97              ; 'a'
    SUB R2, R7
    JZ R2, move_left
    LD R2, R4
    LDI R7, 100             ; 'd'
    SUB R2, R7
    JZ R2, move_right
    LD R2, R4
    LDI R7, 32              ; SPACE fires
    SUB R2, R7
    JZ R2, fire
    JMP clear_key

move_left:
    LDI R2, 0
    ST R4, R2
    JZ R1, main_loop
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    LDI R2, 0
    ST R6, R2
    DEC R1
    JMP redraw_ship

move_right:
    LDI R2, 0
    ST R4, R2
    MOV R2, R1
    LDI R7, 319
    SUB R2, R7
    JZ R2, main_loop
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    LDI R2, 0
    ST R6, R2
    INC R1

redraw_ship:
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    ST R6, R3
    JMP main_loop

fire:
    LDI R2, 0
    ST R4, R2
    MOV R6, R0
    LDI R2, 57600          ; Shot lane at row 180
    ADD R6, R2
    ADD R6, R1
    LDI R2, 1
    ST R6, R2
    JMP main_loop

clear_key:
    LDI R2, 0
    ST R4, R2
    JMP main_loop

.include "../lib_gfx.asm"
