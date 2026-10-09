; Graphics coprocessor services for the C compiler (gfx_* builtins).
; Arguments are on the stack (pushed right to left); results are returned in R0.
; Coprocessor registers at 0x43020: +0 CMD, +1 COLOR, +2 X, +4 Y, +6 W, +8 H,
; +10 SRC word, +12 SRC high byte, +14 RESULT.
.address 0x1000

CGFX_HIGH:
    LDI R7, 0xFFFF
    INC R7
    ADD R7, R7
    ADD R7, R7
    RET

CGFX_BASE:
    CALLX CGFX_HIGH
    LDI R6, 0x3020
    ADD R7, R6
    RET

CGFX_FIELDS:
    CALLX CGFX_BASE
    LDI R6, 2
    ADD R7, R6
    STW R7, R0
    ADD R7, R6
    STW R7, R1
    ADD R7, R6
    STW R7, R2
    ADD R7, R6
    STW R7, R3
    RET

CGFX_COLOR:
    CALLX CGFX_BASE
    INC R7
    ST R7, R4
    RET

CGFX_SRC:
    CALLX CGFX_BASE
    LDI R6, 10
    ADD R7, R6
    STW R7, R4
    INC R7
    INC R7
    ST R7, R5
    RET

CGFX_GO:
    CALLX CGFX_BASE
    ST R7, R5
    RET

gfx_clear:
    LDWS R4, 2
    CALLX CGFX_COLOR
    LDI R5, 1
    CALLX CGFX_GO
    RET

gfx_pixel:
    LDWS R0, 2
    LDWS R1, 4
    CALLX CGFX_FIELDS
    LDWS R4, 6
    CALLX CGFX_COLOR
    LDI R5, 2
    CALLX CGFX_GO
    RET

gfx_rect:
    LDWS R0, 2
    LDWS R1, 4
    LDWS R2, 6
    LDWS R3, 8
    CALLX CGFX_FIELDS
    LDWS R4, 10
    CALLX CGFX_COLOR
    LDI R5, 3
    CALLX CGFX_GO
    RET

gfx_line:
    LDWS R0, 2
    LDWS R1, 4
    LDWS R2, 6
    LDWS R3, 8
    CALLX CGFX_FIELDS
    LDWS R4, 10
    CALLX CGFX_COLOR
    LDI R5, 4
    CALLX CGFX_GO
    RET

gfx_sprite:
    LDWS R0, 2
    LDWS R1, 4
    LDWS R2, 6
    LDWS R3, 8
    CALLX CGFX_FIELDS
    LDWS R4, 10
    MOVSP R7
    LDI R6, 12
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDI R5, 5
    CALLX CGFX_GO
    RET

gfx_bitmap:
    LDWS R0, 2
    LDWS R1, 4
    LDWS R2, 6
    LDWS R3, 8
    CALLX CGFX_FIELDS
    LDWS R4, 10
    MOVSP R7
    LDI R6, 12
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDWS R4, 13
    CALLX CGFX_COLOR
    LDI R5, 6
    CALLX CGFX_GO
    RET

gfx_poly:
    LDWS R0, 2
    LDWS R1, 4
    LDWS R2, 6
    LDI R3, 0
    CALLX CGFX_FIELDS
    LDWS R4, 8
    MOVSP R7
    LDI R6, 10
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDWS R4, 11
    CALLX CGFX_COLOR
    LDI R5, 10
    CALLX CGFX_GO
    RET

gfx_text:
    LDWS R0, 2
    LDWS R1, 4
    LDI R2, 0
    LDI R3, 0
    CALLX CGFX_FIELDS
    LDWS R4, 6
    MOVSP R7
    LDI R6, 8
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDWS R4, 9
    CALLX CGFX_COLOR
    LDI R5, 7
    CALLX CGFX_GO
    RET

gfx_present:
    LDI R5, 8
    CALLX CGFX_GO
    RET

; gfx_save() stores the back buffer as a static layer; gfx_restore() copies it back.
gfx_save:
    LDI R5, 14
    CALLX CGFX_GO
    RET

gfx_restore:
    LDI R5, 15
    CALLX CGFX_GO
    RET

gfx_keys:
    CALLX CGFX_HIGH
    LDI R6, 0x3004
    ADD R7, R6
    LD R0, R7
    RET

; Extended held keys (Q, E, thrust, reverse) live in the byte after gfx_keys.
gfx_keys_ext:
    CALLX CGFX_HIGH
    LDI R6, 0x3005
    ADD R7, R6
    LD R0, R7
    RET

; Mouse registers at 0x43008: X word, Y word, held buttons byte, event byte.
gfx_mouse_x:
    CALLX CGFX_HIGH
    LDI R6, 0x3008
    ADD R7, R6
    LDW R0, R7
    RET

gfx_mouse_y:
    CALLX CGFX_HIGH
    LDI R6, 0x300A
    ADD R7, R6
    LDW R0, R7
    RET

gfx_mouse_buttons:
    CALLX CGFX_HIGH
    LDI R6, 0x300C
    ADD R7, R6
    LD R0, R7
    RET

gfx_mouse_events:
    CALLX CGFX_HIGH
    LDI R6, 0x300D
    ADD R7, R6
    LD R0, R7
    RET

gfx_random:
    LDI R5, 9
    CALLX CGFX_GO
    LDI R6, 14
    ADD R7, R6
    LD R0, R7
    RET

gfx_ticks:
    LDI R5, 11
    CALLX CGFX_GO
    LDI R6, 14
    ADD R7, R6
    LDW R0, R7
    RET

; gfx_sound(channel, freq, frames, wave, volume): COLOR=channel, X=freq, Y=frames, W=wave, H=volume.
gfx_sound:
    LDWS R0, 4
    LDWS R1, 6
    LDWS R2, 8
    LDWS R3, 10
    CALLX CGFX_FIELDS
    LDWS R4, 2
    CALLX CGFX_COLOR
    LDI R5, 13
    CALLX CGFX_GO
    RET

; gfx_speed(mode): writes the speed register at 0x43003. 0 = fixed 60 fps, nonzero = maximum speed.
gfx_speed:
    LDWS R0, 2
    CALLX CGFX_HIGH
    LDI R6, 0x3003
    ADD R7, R6
    ST R7, R0
    RET

gfx_rtc:
    LDWS R4, 2
    CALLX CGFX_COLOR
    LDI R5, 12
    CALLX CGFX_GO
    LDI R6, 14
    ADD R7, R6
    LD R0, R7
    RET

; Interrupt support.  gfx_irq_init() just executes EI.  Handlers live at fixed
; addresses 0x0010 (RTC, vector 0x0004, once per display frame) and 0x0030
; (keyboard, vector 0x0002).  The handlers preserve R6/R7 and the flags, so they are
; invisible to the interrupted program.  Counters live at 0x4340 and 0x4342.
gfx_irq_init:
    EI
    RET

gfx_irq_ticks:
    LDI R7, 0x4340
    LDW R0, R7
    RET

gfx_irq_keys:
    LDI R7, 0x4342
    LDW R0, R7
    RET

.address 0x0002
.data hex 00, 30
.address 0x0004
.data hex 00, 10
.address 0x0010
irq_timer:
    ADJSP -4
    STWS R6, 0
    STWS R7, 2
    LDI R7, 0x4340
    LDW R6, R7
    INC R6
    STW R7, R6
    LDWS R6, 0
    LDWS R7, 2
    ADJSP 4
    EI
    RTI
.address 0x0030
irq_key:
    ADJSP -4
    STWS R6, 0
    STWS R7, 2
    LDI R7, 0x4342
    LDW R6, R7
    INC R6
    STW R7, R6
    LDWS R6, 0
    LDWS R7, 2
    ADJSP 4
    EI
    RTI

; ---- LAN networking: GPU commands 16-20 (see tools/sc16net.py) ----
; Pointer argument at sp+N: word at N, high byte at N+2.
net_open:
    LDI R0, 0
    LDI R1, 0
    LDI R2, 0
    LDI R3, 0
    CALLX CGFX_FIELDS
    LDWS R4, 2
    MOVSP R7
    LDI R6, 4
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDI R5, 16
    CALLX NET_RUN
    RET

net_close:
    LDI R5, 17
    CALLX NET_RUN
    RET

; net_send(data, len)
net_send:
    LDWS R0, 5
    LDI R1, 0
    LDI R2, 0
    LDI R3, 0
    CALLX CGFX_FIELDS
    LDWS R4, 2
    MOVSP R7
    LDI R6, 4
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDI R5, 18
    CALLX NET_RUN
    RET

net_recv:
    LDI R0, 0
    LDI R1, 0
    LDI R2, 0
    LDI R3, 0
    CALLX CGFX_FIELDS
    LDWS R4, 2
    MOVSP R7
    LDI R6, 4
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDI R5, 19
    CALLX NET_RUN
    RET

; Run command R5 and return the result word in R0.
NET_RUN:
    CALLX CGFX_GO
    LDI R6, 14
    ADD R7, R6
    LDW R0, R7
    RET

; Query: R4 = query code, R0 = argument.
NET_QUERY:
    LDI R1, 0
    LDI R2, 0
    LDI R3, 0
    CALLX CGFX_FIELDS
    CALLX CGFX_COLOR
    LDI R5, 20
    CALLX NET_RUN
    RET

net_players:
    LDI R0, 0
    LDI R4, 0
    CALLX NET_QUERY
    RET

net_slot:
    LDI R0, 0
    LDI R4, 1
    CALLX NET_QUERY
    RET

net_active:
    LDWS R0, 2
    LDI R4, 2
    CALLX NET_QUERY
    RET

net_full:
    LDI R0, 0
    LDI R4, 3
    CALLX NET_QUERY
    RET

net_sender:
    LDI R0, 0
    LDI R4, 4
    CALLX NET_QUERY
    RET

net_ready:
    LDI R0, 0
    LDI R4, 5
    CALLX NET_QUERY
    RET

net_is_open:
    LDI R0, 0
    LDI R4, 6
    CALLX NET_QUERY
    RET

; gfx_tilemap(map, map_w, map_h, cam_x, cam_y): GPU command 21
gfx_tilemap:
    LDWS R0, 9
    LDWS R1, 11
    LDWS R2, 5
    LDWS R3, 7
    CALLX CGFX_FIELDS
    LDWS R4, 2
    MOVSP R7
    LDI R6, 4
    ADD R7, R6
    LD R5, R7
    CALLX CGFX_SRC
    LDI R5, 21
    CALLX CGFX_GO
    RET

; gfx_getchar(): next typed ASCII character from the keyboard register (0 = none)
gfx_getchar:
    CALLX CGFX_HIGH
    LDI R6, 0x3000
    ADD R7, R6
    LD R0, R7
    LDI R6, 0
    ST R7, R6
    RET
