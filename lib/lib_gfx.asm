; Reusable SC-8 graphics routines.
; Pixel colors are palette indices; zero is transparent for GFX_SPRITE.
; See user_guide.md for calling conventions and register clobbers.
.address 0x0400

GFX_PLOT:
; In: R0=x, R1=y, R2=color. Clobbers R4-R7.
    MOV R5, R1
    LDI R6, 0
    MOV R7, R1
    SUB R7, R6              ; set Z when y=0
    BZ GFX_PLOT_ROW_DONE
    LDI R4, 320
    LDI R7, 1
GFX_PLOT_ROW_LOOP:
    ADD R6, R4
    SUB R5, R7
    BZ GFX_PLOT_ROW_DONE
    JMP GFX_PLOT_ROW_LOOP
GFX_PLOT_ROW_DONE:
    LDI R7, 0xFFFF
    INC R7
    MOV R4, R7
    ADD R7, R4
    ADD R7, R4              ; VRAM base = 0x30000
    ADD R7, R6
    ADD R7, R0
    ST R7, R2
    RET

GFX_CLEAR:
; In: none. Clears all 320x240 visible pixels to palette index 0.
; Clobbers R0-R7.
    LDI R6, 0xFFFF
    INC R6
    MOV R7, R6
    ADD R6, R7
    ADD R6, R7
    LDI R0, 240
    LDI R1, 320
    LDI R2, 1
    LDI R3, 0
GFX_CLEAR_ROW:
    MOV R4, R1
GFX_CLEAR_PIXEL:
    ST R6, R3
    INC R6
    SUB R4, R2
    BZ GFX_CLEAR_NEXT_ROW
    JMP GFX_CLEAR_PIXEL
GFX_CLEAR_NEXT_ROW:
    SUB R0, R2
    BZ GFX_CLEAR_DONE
    JMP GFX_CLEAR_ROW
GFX_CLEAR_DONE:
    RET

GFX_HLINE:
; In: R0=x, R1=y, R2=length (1-255), R3=color. Clobbers R4-R7.
    MOV R5, R2
    LDI R6, 0
    SUB R5, R6
    BZ GFX_HLINE_DONE
GFX_HLINE_LOOP:
    PUSH R2
    PUSH R3
    PUSH R5
    MOV R2, R3
    CALL GFX_PLOT
    POP R5
    POP R3
    POP R2
    INC R0
    LDI R4, 1
    SUB R2, R4
    BZ GFX_HLINE_DONE
    JMP GFX_HLINE_LOOP
GFX_HLINE_DONE:
    RET

GFX_RECT:
; In: R0=x, R1=y, R2=width, R3=height, R4=color.
; Filled rectangle; dimensions must be 1-255. Clobbers R0-R7.
    MOV R6, R2
    LDI R7, 0
    SUB R6, R7
    BZ GFX_RECT_DONE
    MOV R6, R3
    SUB R6, R7
    BZ GFX_RECT_DONE
    MOV R5, R0
GFX_RECT_ROW:
    MOV R7, R2
GFX_RECT_PIXEL:
    PUSH R2
    PUSH R3
    PUSH R4
    PUSH R5
    PUSH R6
    PUSH R7
    MOV R2, R4
    CALL GFX_PLOT
    POP R7
    POP R6
    POP R5
    POP R4
    POP R3
    POP R2
    INC R0
    LDI R6, 1
    SUB R7, R6
    BZ GFX_RECT_NEXT_ROW
    JMP GFX_RECT_PIXEL
GFX_RECT_NEXT_ROW:
    MOV R0, R5
    INC R1
    LDI R6, 1
    SUB R3, R6
    BZ GFX_RECT_DONE
    JMP GFX_RECT_ROW
GFX_RECT_DONE:
    RET

GFX_SPRITE:
; In: R0=x, R1=y, R2=width, R3=height, R4=pixel-data address.
; Row-major bytes; zero is transparent and other bytes are palette indices.
; Clobbers R0-R7. Dimensions must be 1-255.
    MOV R6, R2
    LDI R7, 0
    SUB R6, R7
    BZ GFX_SPRITE_DONE
    MOV R6, R3
    SUB R6, R7
    BZ GFX_SPRITE_DONE
    MOV R5, R0
    LDI R6, 0
GFX_SPRITE_ROW:
    LDI R7, 0
GFX_SPRITE_PIXEL:
    LD R7, R4
    INC R4
    PUSH R5
    PUSH R6
    MOV R5, R7
    LDI R6, 0
    SUB R5, R6
    POP R6
    POP R5
    BZ GFX_SPRITE_SKIP_PIXEL
    PUSH R2
    PUSH R3
    PUSH R4
    PUSH R5
    PUSH R6
    PUSH R7
    MOV R2, R7
    CALL GFX_PLOT
    POP R7
    POP R6
    POP R5
    POP R4
    POP R3
    POP R2
GFX_SPRITE_SKIP_PIXEL:
    INC R0
    INC R6
    MOV R7, R6
    SUB R7, R2
    BZ GFX_SPRITE_NEXT_ROW
    JMP GFX_SPRITE_PIXEL
GFX_SPRITE_NEXT_ROW:
    MOV R0, R5
    INC R1
    LDI R7, 1
    SUB R3, R7
    BZ GFX_SPRITE_DONE
    LDI R6, 0
    JMP GFX_SPRITE_ROW
GFX_SPRITE_DONE:
    RET

GFX_TEXT:
; In: R0=x, R1=y, R2=address of glyph-ID bytes, R3=color.
; Glyph IDs: ASCII code minus 31; 96-101 are öäåÖÄÅ; 0 terminates.
; Uses fonts/font5x7.asm at 0x4400. Draws 6x8 cells; clobbers R2-R7.
; Scratch bytes 0x4300-0x4307 and 0x431B are reserved while GFX_TEXT executes.
    MOV R4, R2
    MOV R5, R2
    CALL GFX_TEXT_SAVE_POINTER
    LDI R7, 0x4302
    ST R7, R3              ; text color
    INC R7
    ST R7, R0              ; current cell x
    INC R7
    ST R7, R1              ; cell y
GFX_TEXT_NEXT_GLYPH:
    LDI R4, 0x4300
    LDW R5, R4
    LDI R4, 0x431B
    LD R6, R4
    CALL GFX_TEXT_EXPAND_POINTER
    LD R6, R5
    MOV R2, R6
    LDI R7, 0
    SUB R2, R7
    BZ GFX_TEXT_DONE
    INC R5
    CALL GFX_TEXT_SAVE_POINTER
    MOV R6, R2
    LDI R7, 1
    SUB R6, R7             ; zero-based glyph index
    LDI R4, 0x4305
    ST R4, R6
    LDI R6, 0
    LDI R4, 0x4306
    ST R4, R6
    INC R4
    ST R4, R6
GFX_TEXT_ROW:
    LDI R4, 0x4305
    LD R5, R4              ; glyph index
    LDI R7, 0x4400
    LDI R3, 8
    LDI R2, 1
GFX_TEXT_GLYPH_ADDRESS:
    ADD R7, R5
    SUB R3, R2
    BZ GFX_TEXT_GLYPH_READY
    JMP GFX_TEXT_GLYPH_ADDRESS
GFX_TEXT_GLYPH_READY:
    INC R4
    LD R5, R4              ; current row
    ADD R7, R5
    LD R2, R7              ; glyph row bits
    LDI R4, 0x4307
    LD R5, R4              ; column index
    LDI R7, 0x4200         ; mask table starts at 0x4200
    ADD R7, R5
    LD R7, R7              ; column mask
    AND R2, R7
    LDI R7, 0
    SUB R2, R7
    BZ GFX_TEXT_SKIP_PIXEL
    LDI R4, 0x4303
    LD R0, R4              ; cell x
    ADD R0, R5
    INC R4
    LD R1, R4              ; cell y
    INC R4
    INC R4
    LD R7, R4              ; row index
    ADD R1, R7
    DEC R4
    DEC R4
    DEC R4
    DEC R4                  ; scratch color address 0x4302
    LD R2, R4
    CALL GFX_PLOT
GFX_TEXT_SKIP_PIXEL:
    LDI R4, 0x4307
    LD R5, R4
    INC R5
    ST R4, R5
    MOV R2, R5
    LDI R7, 5
    SUB R2, R7
    BZ GFX_TEXT_NEXT_ROW
    JMP GFX_TEXT_ROW
GFX_TEXT_NEXT_ROW:
    LDI R4, 0x4306
    LD R5, R4
    INC R5
    ST R4, R5
    LDI R4, 0x4307
    LDI R6, 0
    ST R4, R6
    MOV R2, R5
    LDI R7, 8
    SUB R2, R7
    BZ GFX_TEXT_GLYPH_DONE
    JMP GFX_TEXT_ROW
GFX_TEXT_GLYPH_DONE:
    LDI R4, 0x4303
    LD R5, R4
    LDI R7, 6
    ADD R5, R7
    ST R4, R5
    JMP GFX_TEXT_NEXT_GLYPH
GFX_TEXT_DONE:
    RET

GFX_TEXT_EXPAND_POINTER:
    LDI R4, 0xFFFF
    INC R4
    LDI R7, 0
GFX_TEXT_EXPAND_LOOP:
    CMP R6, R7
    BZ GFX_TEXT_EXPAND_DONE
    ADD R5, R4
    DEC R6
    CMP R6, R7
    BZ GFX_TEXT_EXPAND_DONE
    JMP GFX_TEXT_EXPAND_LOOP
GFX_TEXT_EXPAND_DONE:
    RET

GFX_TEXT_SAVE_POINTER:
    LDI R4, 0x4300
    STW R4, R5
    MOV R6, R5
    LDI R4, 0xFFFF
    INC R4
    LDI R7, 0
GFX_TEXT_SAVE_LOOP:
    CMP R6, R4
    BC GFX_TEXT_SAVE_DONE
    SUB R6, R4
    INC R7
    JMP GFX_TEXT_SAVE_LOOP
GFX_TEXT_SAVE_DONE:
    LDI R4, 0x431B
    ST R4, R7
    RET

.address 0x4200
GFX_FONT_MASKS:
.data hex 80, 40, 20, 10, 08
