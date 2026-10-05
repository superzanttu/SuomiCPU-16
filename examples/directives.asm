; Demonstrates multiple address changes and all .data formats.
; Load with: python SuomiCPU.py examples/directives.asm

.address 0x0100
start:
    LDI R0, 0x20
    JMP done
    HALT
done:
    HALT

.address 0x0200
hex_bytes:
    .data hex 0x53, 43, FF

.address 0x0210
octal_bytes:
    .data oct 123, 377

.address 0x0220
binary_bytes:
    .data bin 01010011, 01000011

.address 0x0230
message:
    .data string "SC-16", " ready!\n"
