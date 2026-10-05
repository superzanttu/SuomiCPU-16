; SC-16 5x7 bitmap font in 6x8 cells.
; ASCII glyphs are ordered from space (index 0) through tilde (index 94).
; The next six glyphs are ö, ä, å, Ö, Ä, Å in that order.
; Each row uses bits 7..3 for five pixels; bits 1..0 are unused.
; Each glyph is eight bytes, with a blank eighth row.
.address 0x4400
font5x7:
; ASCII 32-47: space and punctuation
.data hex 00, 00, 00, 00, 00, 00, 00, 00
.data hex 20, 20, 20, 20, 20, 00, 20, 00
.data hex 50, 50, 50, 00, 00, 00, 00, 00
.data hex 50, F8, 50, 50, F8, 50, 00, 00
.data hex 20, 78, A0, 70, 28, F0, 20, 00
.data hex C8, D0, 20, 40, A8, 98, 00, 00
.data hex 60, 90, A0, 40, A8, 90, 68, 00
.data hex 20, 20, 40, 00, 00, 00, 00, 00
.data hex 10, 20, 40, 40, 40, 20, 10, 00
.data hex 40, 20, 10, 10, 10, 20, 40, 00
.data hex 00, 50, 20, F8, 20, 50, 00, 00
.data hex 00, 20, 20, F8, 20, 20, 00, 00
.data hex 00, 00, 00, 00, 20, 20, 40, 00
.data hex 00, 00, 00, F8, 00, 00, 00, 00
.data hex 00, 00, 00, 00, 00, 60, 60, 00
.data hex 08, 10, 10, 20, 40, 40, 80, 00
; ASCII 48-57: digits 0-9
.data hex 70, 88, 98, A8, C8, 88, 70, 00
.data hex 20, 60, 20, 20, 20, 20, 70, 00
.data hex 70, 88, 08, 10, 20, 40, F8, 00
.data hex F0, 08, 08, 70, 08, 08, F0, 00
.data hex 10, 30, 50, 90, F8, 10, 10, 00
.data hex F8, 80, 80, F0, 08, 08, F0, 00
.data hex 30, 40, 80, F0, 88, 88, 70, 00
.data hex F8, 08, 10, 20, 40, 40, 40, 00
.data hex 70, 88, 88, 70, 88, 88, 70, 00
.data hex 70, 88, 88, 78, 08, 10, 60, 00
; ASCII 58-64: punctuation
.data hex 00, 60, 60, 00, 60, 60, 00, 00
.data hex 00, 60, 60, 00, 20, 20, 40, 00
.data hex 10, 20, 40, 80, 40, 20, 10, 00
.data hex 00, 00, F8, 00, F8, 00, 00, 00
.data hex 40, 20, 10, 08, 10, 20, 40, 00
.data hex 70, 88, 08, 10, 20, 00, 20, 00
.data hex 70, 88, B8, A8, B8, 80, 70, 00
; ASCII 65-90: uppercase A-Z
.data hex 70, 88, 88, F8, 88, 88, 88, 00
.data hex F0, 88, 88, F0, 88, 88, F0, 00
.data hex 70, 88, 80, 80, 80, 88, 70, 00
.data hex F0, 88, 88, 88, 88, 88, F0, 00
.data hex F8, 80, 80, F0, 80, 80, F8, 00
.data hex F8, 80, 80, F0, 80, 80, 80, 00
.data hex 70, 88, 80, B8, 88, 88, 70, 00
.data hex 88, 88, 88, F8, 88, 88, 88, 00
.data hex 70, 20, 20, 20, 20, 20, 70, 00
.data hex 38, 10, 10, 10, 10, 90, 60, 00
.data hex 88, 90, A0, C0, A0, 90, 88, 00
.data hex 80, 80, 80, 80, 80, 80, F8, 00
.data hex 88, D8, A8, A8, 88, 88, 88, 00
.data hex 88, C8, A8, 98, 88, 88, 88, 00
.data hex 70, 88, 88, 88, 88, 88, 70, 00
.data hex F0, 88, 88, F0, 80, 80, 80, 00
.data hex 70, 88, 88, 88, A8, 90, 68, 00
.data hex F0, 88, 88, F0, A0, 90, 88, 00
.data hex 78, 80, 80, 70, 08, 08, F0, 00
.data hex F8, 20, 20, 20, 20, 20, 20, 00
.data hex 88, 88, 88, 88, 88, 88, 70, 00
.data hex 88, 88, 88, 88, 88, 50, 20, 00
.data hex 88, 88, 88, A8, A8, D8, 88, 00
.data hex 88, 88, 50, 20, 50, 88, 88, 00
.data hex 88, 88, 50, 20, 20, 20, 20, 00
.data hex F8, 08, 10, 20, 40, 80, F8, 00
; ASCII 91-96: punctuation
.data hex 70, 40, 40, 40, 40, 40, 70, 00
.data hex 80, 40, 40, 20, 10, 10, 08, 00
.data hex 70, 10, 10, 10, 10, 10, 70, 00
.data hex 20, 50, 88, 00, 00, 00, 00, 00
.data hex 00, 00, 00, 00, 00, 00, F8, 00
.data hex 40, 20, 10, 00, 00, 00, 00, 00
; ASCII 97-122: lowercase a-z
.data hex 00, 00, 70, 08, 78, 88, 78, 00
.data hex 80, 80, F0, 88, 88, 88, F0, 00
.data hex 00, 00, 70, 88, 80, 88, 70, 00
.data hex 08, 08, 78, 88, 88, 88, 78, 00
.data hex 00, 00, 70, 88, F8, 80, 70, 00
.data hex 30, 48, 40, E0, 40, 40, 40, 00
.data hex 00, 00, 78, 88, 88, 78, 08, F0
.data hex 80, 80, F0, 88, 88, 88, 88, 00
.data hex 20, 00, 60, 20, 20, 20, 70, 00
.data hex 10, 00, 30, 10, 10, 90, 60, 00
.data hex 80, 80, 90, A0, C0, A0, 90, 00
.data hex 60, 20, 20, 20, 20, 20, 70, 00
.data hex 00, 00, D0, A8, A8, A8, A8, 00
.data hex 00, 00, F0, 88, 88, 88, 88, 00
.data hex 00, 00, 70, 88, 88, 88, 70, 00
.data hex 00, 00, F0, 88, 88, F0, 80, 80
.data hex 00, 00, 78, 88, 88, 78, 08, 08
.data hex 00, 00, B0, C8, 80, 80, 80, 00
.data hex 00, 00, 78, 80, 70, 08, F0, 00
.data hex 40, 40, E0, 40, 40, 48, 30, 00
.data hex 00, 00, 88, 88, 88, 98, 68, 00
.data hex 00, 00, 88, 88, 88, 50, 20, 00
.data hex 00, 00, 88, 88, A8, A8, 50, 00
.data hex 00, 00, 88, 50, 20, 50, 88, 00
.data hex 00, 00, 88, 88, 88, 78, 08, F0
.data hex 00, 00, F8, 10, 20, 40, F8, 00
; ASCII 123-126: punctuation
.data hex 18, 20, 20, C0, 20, 20, 18, 00
.data hex 20, 20, 20, 20, 20, 20, 20, 00
.data hex C0, 20, 20, 18, 20, 20, C0, 00
.data hex 68, 90, 00, 00, 00, 00, 00, 00
; Finnish extended letters: ö, ä, å, Ö, Ä, Å
.data hex 50, 00, 70, 88, 88, 88, 70, 00
.data hex 50, 00, 70, 08, 78, 88, 78, 00
.data hex 20, 50, 70, 08, 78, 88, 78, 00
.data hex 50, 70, 88, 88, 88, 88, 70, 00
.data hex 50, 70, 88, 88, F8, 88, 88, 00
.data hex 20, 50, 70, 88, F8, 88, 88, 00
