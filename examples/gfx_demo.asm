; Demonstrates reusable routines from lib_gfx.asm and glyph data from the font.
; Load with: python SuomiCPU.py examples/gfx_demo.asm
.include "../fonts/font5x7.asm"
.address 0x0100

start:
    LDI R0, 10
    LDI R1, 10
    LDI R2, 30
    LDI R3, 12
    LDI R4, 25
    CALL GFX_RECT

    LDI R0, 10
    LDI R1, 30
    LDI R2, 40
    LDI R3, 1
    CALL GFX_HLINE

    LDI R0, 20
    LDI R1, 50
    LDI R2, 0x80
    LDI R3, 1
    CALL GFX_TEXT

    LDI R0, 60
    LDI R1, 50
    LDI R2, 4
    LDI R3, 4
    LDI R4, 0x90
    CALL GFX_SPRITE

    LDI R0, 100
    LDI R1, 80
    LDI R2, 1
    CALL GFX_PLOT
    HALT

.address 0x0080
; Glyph IDs are ASCII code minus 31; zero ends the text.
; 52, 36, 25, 1, 40, 39, 57 spell "SC8 GFX".
.data hex 34, 24, 19, 01, 28, 27, 39, 00
.address 0x0090
; 4x4 sprite pixels; zero bytes are transparent.
.data hex 00, 19, 19, 00, 19, 01, 01, 19, 19, 01, 01, 19, 00, 19, 19, 00

.include "../lib_gfx.asm"
