; Minimal Asteroids-style demo:
; A/D move the blue ship, a gray asteroid drifts across the screen,
; and SPACE places a white shot marker. Press Escape to close the emulator.
.include "../fonts/font5x7.asm"
.address 0

start:
    LDI R0, 20
    LDI R1, 20
    LDI R2, 1
    CALL GFX_PLOT           ; A star drawn using the included graphics library
    LDI R0, 0xFFFF
    INC R0
    MOV R6, R0
    ADD R0, R6
    ADD R0, R6
    LDI R1, 160
    LDI R3, 25
    LDI R4, 0xFFFF
    MOV R6, R4
    ADD R4, R6
    ADD R4, R4
    LDI R6, 0x3004
    ADD R4, R6             ; keyboard address 0x43000
    LDI R5, 64000
    LDI R7, 0
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    ST R6, R3

main_loop:
    LD R2, R4
    JZ R2, update_asteroid
    LDI R3, 97              ; 'a'
    SUB R2, R3
    JZ R2, move_left
    LD R2, R4
    LDI R3, 100             ; 'd'
    SUB R2, R3
    JZ R2, move_right
    LD R2, R4
    LDI R3, 32               ; SPACE fires
    SUB R2, R3
    JZ R2, fire
    JMP clear_key

move_left:
    LDI R2, 0
    ST R4, R2
    JZ R1, main_loop
    JMP move_ship

move_right:
    LDI R2, 0
    ST R4, R2
    MOV R2, R1
    LDI R3, 319
    SUB R2, R3
    JZ R2, main_loop
    INC R1
    JMP redraw_ship

move_ship:
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    LDI R2, 0
    ST R6, R2
    DEC R1

redraw_ship:
    MOV R6, R0
    ADD R6, R5
    ADD R6, R1
    LDI R3, 25
    ST R6, R3
    JMP update_asteroid

fire:
    LDI R2, 0
    ST R4, R2
    MOV R6, R0
    LDI R2, 63680          ; One row above the ship (row 199)
    ADD R6, R2
    ADD R6, R1
    LDI R2, 1
    ST R6, R2

clear_key:
    LDI R2, 0
    ST R4, R2

update_asteroid:
    MOV R6, R0
    LDI R2, 16000          ; Asteroid lane at row 50
    ADD R6, R2
    ADD R6, R7
    LDI R2, 0
    ST R6, R2
    INC R7
    MOV R2, R7
    LDI R3, 320
    SUB R2, R3
    JZ R2, asteroid_wrap
    JMP draw_asteroid

asteroid_wrap:
    LDI R7, 0

draw_asteroid:
    MOV R6, R0
    LDI R2, 16000
    ADD R6, R2
    ADD R6, R7
    LDI R2, 90
    ST R6, R2
    LDI R3, 25
    JMP main_loop

.include "../lib_gfx.asm"
