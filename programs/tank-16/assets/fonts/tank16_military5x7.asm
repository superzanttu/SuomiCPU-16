; Tank-16 military stencil bitmap font in the SC-16 5x7, 6x8-cell format.
; ASCII glyphs are ordered from space (index 0) through tilde (index 94).
; The next six glyphs are Finnish letters: oe, ae, aa, OE, AE, AA.
; Each row uses bits 7..3 for five pixels; the eighth row is blank.
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
; ASCII 48-57: military numerals
.data hex 70, D8, 88, A8, 88, D8, 70, 00
.data hex 20, 60, 20, 20, 20, 20, 70, 00
.data hex 70, D8, 18, 30, 60, C0, F8, 00
.data hex F0, 18, 18, 70, 18, 18, F0, 00
.data hex 10, 30, 50, 90, F8, 10, 10, 00
.data hex F8, C0, C0, F0, 18, 18, F0, 00
.data hex 38, 60, C0, F0, D8, D8, 70, 00
.data hex F8, 18, 30, 60, 60, 60, 60, 00
.data hex 70, D8, D8, 70, D8, D8, 70, 00
.data hex 70, D8, D8, 78, 18, 30, E0, 00
; ASCII 58-64: punctuation
.data hex 00, 60, 60, 00, 60, 60, 00, 00
.data hex 00, 60, 60, 00, 20, 20, 40, 00
.data hex 10, 20, 40, 80, 40, 20, 10, 00
.data hex 00, 00, F8, 00, F8, 00, 00, 00
.data hex 40, 20, 10, 08, 10, 20, 40, 00
.data hex 70, 88, 08, 10, 20, 00, 20, 00
.data hex 70, 88, B8, A8, B8, 80, 70, 00
; ASCII 65-90: military stencil capitals
.data hex 70, D8, 88, F8, 88, 88, 88, 00
.data hex F0, 98, 98, F0, 98, 98, F0, 00
.data hex 78, C0, 80, 80, 80, C0, 78, 00
.data hex F0, 98, 88, 88, 88, 98, F0, 00
.data hex F8, 80, 80, F0, 80, 80, F8, 00
.data hex F8, 80, 80, F0, 80, 80, 80, 00
.data hex 78, C0, 80, B8, 88, C8, 78, 00
.data hex 88, 88, 88, F8, 88, 88, 88, 00
.data hex F8, 20, 20, 20, 20, 20, F8, 00
.data hex 38, 18, 08, 08, 88, C8, 70, 00
.data hex 88, 90, A0, C0, A0, 90, 88, 00
.data hex 80, 80, 80, 80, 80, 80, F8, 00
.data hex 88, D8, A8, A8, 88, 88, 88, 00
.data hex 88, C8, A8, 98, 88, 88, 88, 00
.data hex 70, D8, 88, 88, 88, D8, 70, 00
.data hex F0, 98, 88, F0, 80, 80, 80, 00
.data hex 70, D8, 88, 88, A8, D0, 68, 00
.data hex F0, 98, 88, F0, A0, 90, 88, 00
.data hex 78, C0, 80, 70, 18, 18, F0, 00
.data hex F8, 20, 20, 20, 20, 20, 20, 00
.data hex 88, 88, 88, 88, 88, D8, 70, 00
.data hex 88, 88, 88, 88, 50, 50, 20, 00
.data hex 88, 88, 88, A8, A8, D8, 88, 00
.data hex 88, 50, 20, 20, 20, 50, 88, 00
.data hex 88, 50, 20, 20, 20, 20, 20, 00
.data hex F8, 18, 30, 60, C0, 80, F8, 00
; ASCII 91-96: punctuation
.data hex 70, 40, 40, 40, 40, 40, 70, 00
.data hex 80, 40, 40, 20, 10, 10, 08, 00
.data hex 70, 10, 10, 10, 10, 10, 70, 00
.data hex 20, 50, 88, 00, 00, 00, 00, 00
.data hex 00, 00, 00, 00, 00, 00, F8, 00
.data hex 40, 20, 10, 00, 00, 00, 00, 00
; ASCII 97-122: uppercase-style lowercase
.data hex 70, D8, 88, F8, 88, 88, 88, 00
.data hex F0, 98, 98, F0, 98, 98, F0, 00
.data hex 78, C0, 80, 80, 80, C0, 78, 00
.data hex F0, 98, 88, 88, 88, 98, F0, 00
.data hex F8, 80, 80, F0, 80, 80, F8, 00
.data hex F8, 80, 80, F0, 80, 80, 80, 00
.data hex 78, C0, 80, B8, 88, C8, 78, 00
.data hex 88, 88, 88, F8, 88, 88, 88, 00
.data hex F8, 20, 20, 20, 20, 20, F8, 00
.data hex 38, 18, 08, 08, 88, C8, 70, 00
.data hex 88, 90, A0, C0, A0, 90, 88, 00
.data hex 80, 80, 80, 80, 80, 80, F8, 00
.data hex 88, D8, A8, A8, 88, 88, 88, 00
.data hex 88, C8, A8, 98, 88, 88, 88, 00
.data hex 70, D8, 88, 88, 88, D8, 70, 00
.data hex F0, 98, 88, F0, 80, 80, 80, 00
.data hex 70, D8, 88, 88, A8, D0, 68, 00
.data hex F0, 98, 88, F0, A0, 90, 88, 00
.data hex 78, C0, 80, 70, 18, 18, F0, 00
.data hex F8, 20, 20, 20, 20, 20, 20, 00
.data hex 88, 88, 88, 88, 88, D8, 70, 00
.data hex 88, 88, 88, 88, 50, 50, 20, 00
.data hex 88, 88, 88, A8, A8, D8, 88, 00
.data hex 88, 50, 20, 20, 20, 50, 88, 00
.data hex 88, 50, 20, 20, 20, 20, 20, 00
.data hex F8, 18, 30, 60, C0, 80, F8, 00
; ASCII 123-126: punctuation
.data hex 18, 20, 20, C0, 20, 20, 18, 00
.data hex 20, 20, 20, 20, 20, 20, 20, 00
.data hex C0, 20, 20, 18, 20, 20, C0, 00
.data hex 68, 90, 00, 00, 00, 00, 00, 00
; Finnish extended letters
.data hex 50, 00, 70, 88, 88, 88, 70, 00
.data hex 50, 00, 70, 08, 78, 88, 78, 00
.data hex 20, 50, 70, 08, 78, 88, 78, 00
.data hex 50, 70, 88, 88, 88, 88, 70, 00
.data hex 50, 70, 88, 88, F8, 88, 88, 00
.data hex 20, 50, 70, 88, F8, 88, 88, 00
