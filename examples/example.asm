; Example program to draw a letter 'H'
; R0: VRAM Address, R1: Color, R2: Row Offset (320), R3: Temp/Column

LDI R0, 0x0A
LDI_H R0, 0x30       ; R0 = 0x300A (VRAM start)
LDI R1, 25           ; R1 = 25 (Blue)
LDI R2, 64
LDI_H R2, 1          ; R2 = 320 (Screen Width)

; Row 0
ST R0, R1
LDI R3, 5
ADD R0, R3
ST R0, R1

; Row 1
LDI R3, 315
ADD R0, R3
ST R0, R1
LDI R3, 5
ADD R0, R3
ST R0, R1

; Row 2 (The bridge)
LDI R3, 315
ADD R0, R3
ST R0, R1
LDI R3, 1
ADD R0, R3
ST R0, R1
ADD R0, R3
ST R0, R1
ADD R0, R3
ST R0, R1
ADD R0, R3
ST R0, R1
LDI R3, 5
ADD R0, R3
ST R0, R1

; Row 3
LDI R3, 315
ADD R0, R3
ST R0, R1
LDI R3, 5
ADD R0, R3
ST R0, R1

; Row 4
LDI R3, 315
ADD R0, R3
ST R0, R1
LDI R3, 5
ADD R0, R3
ST R0, R1

HALT
