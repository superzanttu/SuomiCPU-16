# SC-16 Assembler and Emulator User Guide

## 1. Overview

SuomiCPU is a Python emulator for the SC-16, a small, byte-addressed machine with
eight general-purpose registers and a 16-bit instruction format. Assembly source
files use the `.asm` extension. The assembler converts instructions to machine
code and places instructions or data at the addresses specified in the source.

The emulator has no BIOS. On reset, it starts executing at the assembly image's
entry point. The first instruction in the source determines that entry point,
regardless of the numeric order of the image's memory segments.

## 2. Running the emulator

Run a program from the repository root:

```console
python main.py examples/example.asm
```

On Windows, run the PowerShell launcher with a source or binary path:

```powershell
.\run.ps1 .\examples\factorial.c
.\run.ps1 .\examples\asteroids.c
```

`run.ps1` changes to the repository directory and starts the emulator through
`main.py`. It requires Python to be available as `python` on `PATH`. Without a
program path, SC-16 opens SC-launcher. The launcher lists `.c` files in
`modules/`, using each filename as its module name and the first source line as
its description. Click a module to select it, then click **LOAD MODULE**; the
16x16 outlined square is reserved for a module icon. Use the mouse wheel to
scroll through the list. Press Escape to close the launcher.

Additional examples are in [`examples/commands.asm`](examples/commands.asm)
and [`examples/directives.asm`](examples/directives.asm):

```console
python main.py examples/commands.asm
python main.py examples/directives.asm
```

Compile and immediately run a C source file, or create a flat binary image:

```console
python main.py examples/factorial.c
python tools/c_compiler.py examples/factorial.c
python tools/c_compiler.py examples/factorial.c -S -o examples/factorial.asm
python main.py examples/factorial.bin
```

The compiler defaults to an output file with the input name and a `.bin`
extension. `-S` emits readable SC-16 assembly instead. The emulator accepts
`.asm`, `.c`, and flat `.bin` inputs.

The input file argument is optional. With no file, the launcher starts instead
of opening an empty emulator. The emulator and launcher use Pygame for their
window, display, and input.

## 3. Assembly source syntax

- Mnemonics and directives are case-insensitive. Register names are `R0` through
  `R7`.
- Labels start with a letter or underscore and may then contain letters, digits,
  and underscores. Labels are case-sensitive and end in a colon:

  ```asm
  loop:
      INC R0
      JMP loop
  ```

- A label can share a line with an instruction or directive:

  ```asm
  message: .data string "Hello"
  ```

- `;` and `#` start comments outside quoted strings.
- Numeric instruction operands use Python-style integer prefixes: decimal
  (`25`), hexadecimal (`0x19`), octal (`0o31`), or binary (`0b11001`).
- There is no implicit `CALL`, `RET`, `NOP`, expression evaluation, or data
  alignment directive. Every instruction has a two-byte encoding, except a
  wide `LDI`, which expands to two instructions.

## 4. Directives

### `.address`

Set the current **byte address** for subsequent instructions, data, and labels:

```asm
.address 0x0100
entry:
    LDI R0, 25
    HALT

.address 0x0200
scratch:
    .data hex 00, 00, 00
```

Use as many `.address` directives as needed. Each takes one numeric address;
labels are not accepted as `.address` arguments. Moving the address does not
emit bytes. Sections need not be written in address order. The assembler returns
separate memory segments for disjoint ranges and rejects overlapping output.

Valid addresses are `0` through `0x7FFFF`, the emulator's 512 KiB memory range.
The entry point is the address of the first instruction encountered in source
order. If there are no instructions, it is the first emitted data address; if
the source emits nothing, it is the initial address `0`.

### `.data`

Emit one or more bytes at the current address. Each numeric value must fit in a
byte (`0x00` through `0xFF`). Values can be separated by spaces or commas.

#### Hexadecimal

The `hex` format treats unprefixed tokens as hexadecimal. An optional `0x`
prefix is allowed:

```asm
.data hex 53, 0x43, FF
```

This emits bytes `0x53`, `0x43`, and `0xFF`.

#### Octal

The `oct` format treats unprefixed tokens as octal. An optional `0o` prefix is
allowed:

```asm
.data oct 123, 0o377
```

This emits bytes `0x53` and `0xFF`.

#### Binary

The `bin` format treats unprefixed tokens as binary. An optional `0b` prefix is
allowed:

```asm
.data bin 01010011, 0b01000011
```

This emits bytes `0x53` and `0x43`.

#### Strings

The `string` format accepts one or more quoted strings. Strings are encoded as
UTF-8 and emitted exactly as given; no null terminator is added. Python-style
quoted escapes such as `\n` and `\t` are interpreted:

```asm
.data string "Hello", " world!\n"
```

This emits the UTF-8 bytes of `Hello world!` followed by a newline byte.

### `.include`

Insert another assembler source file at the current point:

```asm
.include "fonts/font5x7.asm"
```

The path is relative to the file containing the `.include`, so the same include
works when the top-level source is assembled from another working directory.
Included sources are assembled inline: their `.address` directives change the
current address, and their labels share the global label namespace with the
including source. Nested includes are supported. Circular includes, missing
files, and invalid paths are reported as assembly errors. Use unique labels in
shared include files; including a file twice that defines the same label
produces a duplicate-label error.

## 5. Instruction reference

All instructions occupy one 16-bit word (two bytes), stored most-significant
byte first. Registers are selected from `R0` through `R7`. Unless stated
otherwise, instructions do not change the condition flags.

### Program and immediate instructions

| Instruction | Syntax                | Operation                                                                                                                                                          |
| ----------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `HALT`      | `HALT`                | Stop execution.                                                                                                                                                    |
| `LDI`       | `LDI Rd, value`       | Load an immediate value into `Rd`. Values from `0` to `255` use one word. Values from `256` to `65535` expand to `LDI` (low byte) followed by `LDI_H` (high byte). |
| `LDI_H`     | `LDI_H Rd, high_byte` | Replace the high byte of `Rd`, preserving its low byte. `high_byte` must be `0` to `255`.                                                                          |

For example:

```asm
LDI R0, 0x34
LDI_H R0, 0x12       ; R0 becomes 0x1234
; Equivalent one-line pseudo-instruction:
LDI R1, 0x1234
```

The assembler accepts `LDI R1, 0x1234` as a pseudo-instruction and emits the
same pair of machine instructions.

### Memory and register instructions

| Instruction | Syntax            | Operation                                                        |
| ----------- | ----------------- | ---------------------------------------------------------------- |
| `LD`        | `LD Rd, Ra`       | Load one byte from the address in `Ra` into `Rd`.                |
| `ST`        | `ST Ra, Rs`       | Store the low byte of `Rs` at the address in `Ra`.               |
| `LDW`       | `LDW Rd, Ra`      | Load a big-endian 16-bit word from the address in `Ra`.          |
| `STW`       | `STW Ra, Rs`      | Store a big-endian 16-bit word from `Rs` at the address in `Ra`. |
| `LDWS`      | `LDWS Rd, offset` | Load a word from `SP + offset`; offset is 0-255.                 |
| `STWS`      | `STWS Rs, offset` | Store a word at `SP + offset`; offset is 0-255.                  |
| `MOV`       | `MOV Rd, Rs`      | Copy the register value from `Rs` to `Rd`.                       |

Example:

```asm
LDI R0, 0x2000
LDI R1, 0x2A
ST R0, R1
LD R2, R0
MOV R3, R2
```

`LD` and `ST` access byte-sized memory. `ST`'s first operand is the address
register; its second operand is the value register.

### Arithmetic and logic

| Instruction | Syntax                   | Operation                                                                     |
| ----------- | ------------------------ | ----------------------------------------------------------------------------- |
| `ADD`       | `ADD Rd, Rs`             | `Rd = Rd + Rs`; sets `Z` and `C`.                                             |
| `SUB`       | `SUB Rd, Rs`             | `Rd = Rd - Rs`; sets `Z` and `C`.                                             |
| `AND`       | `AND Rd, Rs`             | `Rd = Rd & Rs`.                                                               |
| `OR`        | `OR Rd, Rs`              | `Rd = Rd \| Rs`.                                                              |
| `XOR`       | `XOR Rd, Rs`             | `Rd = Rd ^ Rs`.                                                               |
| `NOT`       | `NOT Rd` or `NOT Rd, Rs` | `Rd = ~Rd` or `Rd = ~Rs`, limited to the low eight bits.                      |
| `INC`       | `INC Rd`                 | Increment `Rd` by one.                                                        |
| `DEC`       | `DEC Rd`                 | Decrement `Rd` by one.                                                        |
| `CMP`       | `CMP Ra, Rb`             | Set `Z` and `C` as if subtracting `Rb` from `Ra`, without storing the result. |
| `SHR`       | `SHR Rd`                 | Logical right shift of the low 16 bits of `Rd` by one.                        |

For two-register arithmetic/logical operations the first operand is also the
destination. `CMP` uses its two operands only as inputs.

The original byte-oriented instructions retain their existing behavior:
register values are Python integers, `LDI` may load up to 16 bits, arithmetic
does not automatically mask its result, and byte stores and `POP` retain only a
byte. `SHR` explicitly operates on a 16-bit word. The C compiler emits masks
where needed to implement its 16-bit integer semantics.

`ADD` sets `Z` when its result is zero and `C` when the result exceeds `255`.
`SUB` sets `Z` when its result is zero and `C` when the result is negative
(borrow). `CMP` sets `Z` when the operands are equal and `C` when the first
operand is less than the second. `AND`, `OR`, `XOR`, `NOT`, `INC`, and `DEC` do
not update the flags in the current emulator.

### Integer multiply and divide

| Instruction | Syntax                         | Operation                                                                       |
| ----------- | ------------------------------ | ------------------------------------------------------------------------------- |
| `MUL`       | `MUL Rd, Rs` / `MUL Rd, imm`   | `Rd` = low 16 bits of `Rd * source`; `C` is set if the product exceeds 16 bits. |
| `MULHU`     | `MULHU Rd, Rs` / `imm`         | `Rd` = high 16 bits of the unsigned 32-bit product.                             |
| `MULHS`     | `MULHS Rd, Rs` / `imm`         | `Rd` = high 16 bits of the signed 32-bit product.                               |
| `DIV`       | `DIV Rd, Rs` / `imm`           | `Rd = Rd / source`, unsigned.                                                   |
| `DIVS`      | `DIVS Rd, Rs` / `imm`          | `Rd = Rd / source`, signed, truncating toward zero.                             |
| `MOD`       | `MOD Rd, Rs` / `imm`           | `Rd = Rd % source`, unsigned.                                                   |
| `MODS`      | `MODS Rd, Rs` / `imm`          | `Rd = Rd % source`, signed; the sign follows the dividend.                      |

The source is a register (`R0`-`R7`) or a 16-bit immediate (decimal, `0x` hex,
or negative). Operands are taken as 16-bit values and the result is always a
clean 16-bit value. `Z` is set when the result is zero. Dividing by zero gives a
quotient of `0` or a remainder equal to the dividend, and sets `C`; otherwise
the division instructions clear `C`.

Encoding: these share the `0x1F` opcode with the extension group. The first
word is `0xF800 | (Rd << 8) | 0x1E`; the second word is
`(operation << 12) | (immediate flag 0x0800) | Rs` with operations
`MUL`=0, `MULHU`=1, `MULHS`=2, `DIV`=3, `DIVS`=4, `MOD`=5, `MODS`=6. With an
immediate, a third word holds the 16-bit value. The register form is four
bytes and the immediate form six bytes. The C compiler emits `MUL`, `DIV`/`DIVS`
and `MOD`/`MODS` directly for `*`, `/` and `%`; there are no software routines.

### Extended ALU, miscellaneous and conditional-branch instructions

| Instruction | Syntax | Operation |
| ----------- | ------ | --------- |
| `SHL` `LSR` `ASR` | `SHL Rd, n` / `SHL Rd, Rs` | Shift left, logical right, arithmetic right. Count is 0-15 (a register uses its low 4 bits). `C` = last bit shifted out. |
| `ROL` `ROR` | `ROL Rd, n` / `ROL Rd, Rs` | Rotate left/right by 0-15. |
| `ADC` `SBC` | `ADC Rd, Rs` / `imm` | Add/subtract including the carry flag, for multi-word arithmetic. |
| `NEG` | `NEG Rd` | Two's complement negate. |
| `TEST` | `TEST Rd, Rs` / `imm` | `Rd AND source`; sets flags only. |
| `ADD SUB AND OR XOR CMP` | `ADD Rd, imm` | Immediate forms (a non-register second operand selects them). |
| `SWAP` `SEXT` `CLZ` | `SWAP Rd` | Swap bytes; sign-extend the low byte; count leading zeros (all in place). |
| `JMPR` `CALLR` | `JMPR Rd` | Jump/call to the 19-bit address held in `Rd`. |
| `PUSHF` `POPF` | `PUSHF` | Push/pop the flags (Z=1, C=2, N=4, V=8). |
| `SETC` `CLC` | `SETC` | Set/clear the carry flag. |
| `SETSP` | `SETSP Rd` | Load the stack pointer from `Rd`. |
| `NOP` | `NOP` | Do nothing (`MOV R0, R0`). |
| `BNZX BNCX BNX BNNX BVX BNVX` | `BNZX label` | Far branch if `Z`, `C`, `N`, `V` is clear (`BNZ`, `BNC`, `BNV`) or set (`BN`, `BV`); `BNNX` = `N` clear. |
| `BLT BGE BGT BLE` | `BLT label` | Signed compare branches after `CMP` (use `N` and `V`). |
| `BHI BLS` | `BHI label` | Unsigned higher / lower-or-same after `CMP`. |

Every branch also has an `X` spelling (`BLTX`, ...); all conditional branches
here are 6-byte, full-address forms. `ADD`, `SUB` and `CMP` now also set the
`N` (negative) and `V` (signed overflow) flags. `NOT` and `POP` now produce
full 16-bit values. Encoding: opcode `0x1F` with low-byte prefix `0x1F`
(ALU/misc group: `(op<<12) | 0x0800 immediate flag | operand`, ops `SHL`=0,
`LSR`=1, `ASR`=2, `ROL`=3, `ROR`=4, `ADC`=5, `SBC`=6, `NEG`=7, `TEST`=8,
`ADD`=9, `SUB`=10, `AND`=11, `OR`=12, `XOR`=13, `CMP`=14, misc=15) or `0x1B`
(conditional far branch; condition in the prefix word, then a 24-bit target).
### Branches

| Instruction | Syntax                    | Operation                                               |
| ----------- | ------------------------- | ------------------------------------------------------- |
| `JMP`       | `JMP address_or_label`    | Unconditionally set the PC to an absolute byte address. |
| `JZ`        | `JZ Rn, address_or_label` | Jump to an absolute byte address if `Rn` is zero.       |
| `BZ`        | `BZ address_or_label`     | Jump if the `Z` condition flag is set.                  |
| `BC`        | `BC address_or_label`     | Jump if the `C` condition flag is set.                  |
| `CALL`      | `CALL address_or_label`   | Push the return PC and call an absolute byte address.   |
| `JMPX`      | `JMPX address_or_label`   | Unconditionally jump to a full 19-bit byte address.     |
| `BZX`       | `BZX address_or_label`    | Jump to a full 19-bit address if `Z` is set.            |
| `BCX`       | `BCX address_or_label`    | Jump to a full 19-bit address if `C` is set.            |
| `CALLX`     | `CALLX address_or_label`  | Call a full 19-bit byte address.                        |
| `RET`       | `RET`                     | Pop the return PC from the stack.                       |

Example:

```asm
    LDI R0, 0
    JZ R0, is_zero
    JMP finished
is_zero:
    LDI R1, 1
finished:
    HALT
```

`JMP`, `BZ`, `BC`, and `CALL` encode an 11-bit address (`0` to `0x7FF`); `JZ`
encodes an 8-bit target (`0` to `0xFF`). These are absolute **byte** addresses,
not instruction indexes or offsets. `JZ` tests the named register directly; it
does not test the `Z` flag. `BZ` and `BC` test the `Z` and `C` flags,
respectively, commonly after `SUB` or `CMP`.
`JMPX`, `BZX`, `BCX`, and `CALLX` use six bytes and encode a full 19-bit address;
use them when code or routines are outside the short-address range.

### Stack and interrupt instructions

| Instruction | Syntax              | Operation                                                                 |
| ----------- | ------------------- | ------------------------------------------------------------------------- |
| `PUSH`      | `PUSH Rd`           | Push a 16-bit register value to the stack.                                |
| `POP`       | `POP Rd`            | Pop a value from the stack into `Rd`; the register is masked to one byte. |
| `EI`        | `EI`                | Set the interrupt-enable bit in the interrupt control register.           |
| `DI`        | `DI`                | Clear the interrupt-enable bit.                                           |
| `RTI`       | `RTI`               | Restore `Z` and the PC from the interrupt stack frame.                    |
| `ADJSP`     | `ADJSP signed_byte` | Add a signed -128 to 127 byte adjustment to the stack pointer.            |

The stack pointer starts at `0x2FFFF`. `PUSH` decrements the pointer by two and
stores a big-endian word; `POP` reads that word and advances the pointer by two.
`CALL`/`RET` use the same stack. The `POP` instruction loads only the low byte
into a general-purpose register, whereas `RET` and `RTI` use the full popped
word. `LDW`/`STW` use big-endian byte order. `LDWS`/`STWS` address words from
the current stack pointer; `ADJSP` reserves or releases a small stack region.
Interrupt handling attempts to save the PC and `Z` flag, but not `C`.
The interrupt vectors are two-byte absolute addresses stored at `0x0002`
(keyboard) and `0x0004` (RTC).

Example vector setup:

```asm
.address 0x0002
.data hex 00, 70
.address 0x0004
.data hex 00, 70
.address 0x0070
interrupt_handler:
    RTI
```

Interrupt input and timing are emulator-specific. The emulated RTC sets its
interrupt-pending bit on each display-loop update, and the emulator checks
interrupts after each instruction. Set up vectors and an `RTI` handler before
enabling interrupts.

## 6. Instruction encoding and execution

- Memory is byte-addressed. The PC points to a byte address and advances by two
  for each 16-bit instruction.
- The instruction word has a five-bit opcode in bits 15-11. Other bits hold a
  destination register, source registers, or an immediate/address field,
  depending on the instruction.
- Instruction words are stored big-endian: the high byte is at the instruction
  address and the low byte is at the next address.
- The assembler's `.data` directives emit literal bytes and do not apply
  instruction endianness or alignment.
- There is no ROM protection: assembled segments are copied into the same
  writable memory used by the CPU.

## 7. Emulator memory and devices

The backing memory is `2**19` bytes (`0x80000`, or 512 KiB). CPU read/write
addresses are masked with `0x7FFFF`, so out-of-range accesses wrap into that
memory. Program-image loading is stricter and rejects segments that extend past
the end of the backing memory.

The emulator declares these addresses, which now map to distinct backing
memory ranges:

| Name                                     |       Address range |         Size |
| ---------------------------------------- | ------------------: | -----------: |
| Flash                                    | `0x00000`-`0x1FFFF` |      128 KiB |
| RAM and stack                            | `0x20000`-`0x2FFFF` |       64 KiB |
| VRAM                                     | `0x30000`-`0x42BFF` | 76,800 bytes |
| Keyboard                                 |           `0x43000` |       1 byte |
| Speed register (0 = fixed, 1 = maximum)  |           `0x43003` |       1 byte |
| Held-key bitmask                         |           `0x43004` |       1 byte |
| Interrupt control (`ICR`)                |           `0x43002` |       1 byte |
| RTC seconds/minutes/hours                | `0x43010`-`0x43012` |      3 bytes |
| Graphics coprocessor registers           | `0x43020`-`0x4302E` |     15 bytes |
| Back buffer (coprocessor drawing target) | `0x44000`-`0x56BFF` | 76,800 bytes |

The display is 320 by 240 pixels. Each VRAM byte indexes a 256-entry palette:
entries 0-11 are named colors (0 black, 1 white, 2 red, 3 green, 4 blue,
5 yellow, 6 cyan, 7 magenta, 8 orange, 9 gray, 10 dark gray, 11 dark red), 25 is
pure blue, and the rest are grayscale. VRAM is a live view of CPU memory, so byte writes starting at
`0x30000 + y * 320 + x` change the pixel at `(x, y)` and are shown at the next
display refresh. The emulator renders the indexed framebuffer directly rather
than converting every pixel in Python. It executes up to 30,000 instructions per
display frame (capped at 60 frames per second), so long drawing routines can
finish without requiring a separate screen update for every CPU instruction.
When the CPU halts, the window remains open with the final frame displayed;
press any key to close it, or close the window using its title-bar control.
Keyboard key presses write the character byte to the keyboard address and mark
a keyboard interrupt pending. The RTC writes seconds, minutes, and hours to its
three declared byte addresses.

## 8. Bitmap font

[`fonts/font5x7.asm`](fonts/font5x7.asm) contains an original 5x7 bitmap font
for every printable ASCII character (`0x20`-`0x7E`) plus `ÃƒÆ’Ã‚Â¶ÃƒÆ’Ã‚Â¤ÃƒÆ’Ã‚Â¥ÃƒÆ’Ã¢â‚¬â€œÃƒÆ’Ã¢â‚¬Å¾ÃƒÆ’Ã¢â‚¬Â¦`. Include it
with:

```asm
.include "fonts/font5x7.asm"
```

It places `font5x7` at `0x4400`, clear of the graphics mask table and scratch
memory. The 95 ASCII glyphs are ordered by character
code, starting with space. The six Finnish letters follow in the order
`ÃƒÆ’Ã‚Â¶`, `ÃƒÆ’Ã‚Â¤`, `ÃƒÆ’Ã‚Â¥`, `ÃƒÆ’Ã¢â‚¬â€œ`, `ÃƒÆ’Ã¢â‚¬Å¾`, `ÃƒÆ’Ã¢â‚¬Â¦`. Each glyph occupies eight bytes, so ASCII glyph
index `character_code - 0x20` starts at `0x4400 + index * 8`; the Finnish
glyphs have indexes 95-100. Each byte describes one horizontal row:
bits 7 through 3 are the five visible pixels (left to right), bit 2 is the
blank sixth column, and bits 1 and 0 are unused. The eighth row is blank. Thus,
the glyph bitmap is 5x7 inside a 6x8 cell. For `GFX_TEXT`, glyph ID is the ASCII
code minus 31 (`1` for space, `95` for `~`), and IDs 96-101 select the Finnish
letters in the order above; `0` terminates the text. `SCREEN_PRINT` accepts
ordinary ASCII and UTF-8 strings containing those Finnish letters. The reusable
graphics include provides a renderer that uses this table and a small mask
table to unpack the pixels.

## 9. Minimal C89 compiler

[`tools/c_compiler.py`](tools/c_compiler.py) implements a small, dependency-free C89 subset
compiler; it is not a complete C89 implementation. Supported types are signed
`int`, `unsigned int`, `char`, `unsigned char`, `float`, `void`, pointers to
supported object types, and fixed-size arrays. It accepts global and local
variables, function prototypes and definitions, recursion, calls, and C89-style
declarations at the start of each block.

Supported statements are blocks, expression statements, `if`/`else`, `while`,
`do`/`while`, `for`, `switch`, `break`, `continue`, and `return`. `switch`
accepts constant `case` labels and one optional `default`; cases fall through
until a `break`. Expressions include numeric and character constants, string
literals, scalar and pointer assignment, compound assignment,
prefix/postfix increment and decrement, unary `+`, `-`, `!`, `~`, arithmetic
`+`, `-`, `*`, `/`, `%`, bitwise `&`, `|`, `^`, comparisons, and short-circuit
`&&` and `||`.

Integer and pointer details:

- `int` and `unsigned int` are 16-bit values. Arithmetic wraps modulo 65536;
  signed comparisons and division apply to `int`, unsigned comparisons and
  division apply to `unsigned int`. Signed division truncates toward zero and
  signed remainder has the dividend's sign.
- `char` array elements and string data occupy one byte. Integer and floating
  objects occupy two bytes. Floating-point values use IEEE-754 binary16.
- Pointers occupy three bytes, matching the CPU's 19-bit address space.
  Pointer arithmetic and array indexing scale by the element size.
- Global storage begins at `0xE800` (`GLOBAL_BASE` in
  [`tools/c_compiler.py`](tools/c_compiler.py)); uninitialized globals are zeroed. Local
  variables, arguments, return addresses, and temporaries use the machine stack.

The subset excludes storage-class and type qualifiers, structures, unions,
enums, `typedef`, function-like macros, `goto`, casts, and most of the C standard
library. Global initializers must be constant expressions supported by the
compiler.

There is no `printf` or separate console device. When a screen routine is
referenced, the compiler links [`lib/lib_text.asm`](lib/lib_text.asm), which uses
[`fonts/font5x7.asm`](fonts/font5x7.asm) to provide a 53-column by 30-row text
grid of 6x8 cells:

| Routine             | C signature                                                                           | Behavior                                                                                                                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SCREEN_CLEAR`      | `void SCREEN_CLEAR(void)`                                                             | Clear the display and reset the cursor to `(0, 0)`.                                                                                                                                                             |
| `SCREEN_SET_CURSOR` | `void SCREEN_SET_CURSOR(unsigned char column, unsigned char row)`                     | Set the zero-based text cursor.                                                                                                                                                                                 |
| `SCREEN_GET_CURSOR` | `int SCREEN_GET_CURSOR(void)`                                                         | Return `(row << 8) \| column`.                                                                                                                                                                                  |
| `SCREEN_PUTCHAR`    | `void SCREEN_PUTCHAR(char ch, unsigned char color)`                                   | Draw one character; newline/carriage return advance to the next row.                                                                                                                                            |
| `SCREEN_PRINT`      | `void SCREEN_PRINT(char *text, unsigned char color)`                                  | Print a zero-terminated string.                                                                                                                                                                                 |
| `SCREEN_INPUT`      | `unsigned int SCREEN_INPUT(char *buffer, unsigned int capacity, unsigned char color)` | Read and echo a line, NUL-terminate it, and return its character count. `capacity` is the maximum character count, so reserve at least `capacity + 1` bytes for the buffer. Enter finishes and Backspace edits. |

Screen text input accepts printable ASCII and Latin-1 keys, echoes the entered
glyphs, and uses Enter as the line terminator. The library reserves scratch bytes near `0x4300`;
do not use that region for application data while a screen routine is running.
See [`examples/text_demo.c`](examples/text_demo.c) for a C example.

Generated C code uses the far control instructions (`JMPX`, `CALLX`, `BZX`,
`BCX`). Address 0 holds a small entry stub (`JMP` to a boot routine at `0x0006` that runs `CALLX main; HALT`; `0x0002`-`0x0005` hold the interrupt vectors); compiled code
starts at `0x4800` (above the font and library scratch memory) and must stay\nbelow `0xE800`, where globals begin (and below `0x10000`), because `CALLX` pushes a 16-bit return address. Libraries
live below that: `lib_gfx` at `0x400`, `lib_text` at `0x700`, and `lib_cgfx`
at `0x1000`. The compiler reports assembler errors when generated code or
stack frames exceed supported limits. The command-line compiler writes a flat `.bin` image
with address gaps zero-filled; the emulator loads it at address zero. In
[`examples/factorial.c`](examples/factorial.c), the global `result` should
contain 123 at `0xE800` after execution.

### Preprocessor, shifts and arrays

A minimal preprocessor handles object-like `#define NAME tokens` (replaced as
tokens, may nest) and `#include "file"` or `#include <file>` (searched next to
the source file, then next to the compiler). Function-like macros and other
directives are errors. Binary `<<` and `>>` are supported: `>>` is arithmetic
for `int` and logical for unsigned types. `x[i]++` and casts are not supported;
write `x[i] = x[i] + 1`.

### C graphics library (`gfx_*`)

[`lib/suomi_gfx.h`](lib/suomi_gfx.h) declares the graphics builtins and defines color
(`RED`, `GREEN`, ...), key (`KEY_LEFT`, `KEY_FIRE`, ...) and screen-size
constants. Using any `gfx_*` function makes the compiler link
[`lib/lib_cgfx.asm`](lib/lib_cgfx.asm), which drives the graphics coprocessor, plus the
font. All drawing goes to an off-screen back buffer; `gfx_present()` copies it
to VRAM in one step and ends the current emulator frame (vsync), so there is
no flicker.

| Function                                                                                | Behavior                                                                      |
| --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `void gfx_clear(unsigned char color)`                                                   | Fill the back buffer.                                                         |
| `void gfx_pixel(int x, int y, unsigned char color)`                                     | Plot one pixel.                                                               |
| `void gfx_rect(int x, int y, int w, int h, unsigned char color)`                        | Filled rectangle.                                                             |
| `void gfx_line(int x0, int y0, int x1, int y1, unsigned char color)`                    | Line (clipped).                                                               |
| `void gfx_poly(int x, int y, int n, char *pts, unsigned char color)`                    | Closed outline through `n` signed-byte `(dx, dy)` pairs relative to `(x, y)`. |
| `void gfx_sprite(int x, int y, int w, int h, unsigned char *data)`                      | Draw `w*h` palette bytes; 0 is transparent.                                   |
| `void gfx_bitmap(int x, int y, int w, int h, unsigned char *data, unsigned char color)` | 1 bit per pixel, MSB first, `(w+7)/8` bytes per row.                          |
| `void gfx_text(int x, int y, char *text, unsigned char color)`                          | 6x8-cell text from the 5x7 font; UTF-8 `??????` and `\n` supported.           |
| `void gfx_present(void)`                                                                | Show the back buffer and wait for the next frame.                             |
| `unsigned int gfx_keys(void)`                                                           | Held-key bitmask.                                                             |
| `unsigned int gfx_keys_ext(void)`                                                       | Second key byte: Q roll left, E roll right, X/Shift thrust, Z/Ctrl reverse.   |
| `unsigned int gfx_random(void)`                                                         | Random byte (0-255).                                                          |
| `int gfx_mouse_x(void)` / `int gfx_mouse_y(void)`                                       | Mouse position in screen pixels (0-319, 0-239).                               |
| `unsigned int gfx_mouse_buttons(void)`                                                  | Held buttons: `MOUSE_LEFT` 1, `MOUSE_RIGHT` 2 (use with position for dragging). |
| `unsigned int gfx_mouse_events(void)`                                                   | One-frame events: press `MOUSE_LEFT_DOWN`/`RIGHT_DOWN` 1/2, double-click `MOUSE_LEFT_DOUBLE`/`RIGHT_DOUBLE` 4/8, release `MOUSE_LEFT_UP`/`RIGHT_UP` 16/32. |
| `void gfx_sound(int channel, int freq, int frames, int wave, int volume)`                | Play a tone on channel 0-3; freq 0 stops it, frames 0 loops. `WAVE_SQUARE/NOISE/TRIANGLE`. |
| `void gfx_tilemap(unsigned char *map, int map_w, int map_h, int cam_x, int cam_y)` | Fill the screen from a tile map (1 byte per tile = palette colour, 0 = black; 16x16 px tiles; camera in pixels; outside the map is black). |
| `int gfx_getchar(void)` | Next typed ASCII character (8 backspace, 13 enter), 0 if none; consumes it. |
| `void gfx_save(void)` / `void gfx_restore(void)`                                        | Store the back buffer as a static layer / redraw it in one call.              |

Everything is clipped to the screen. Held keys (bitmask at `0x43004`): bit 0
left/A, 1 right/D, 2 up/W, 3 down/S, 4 Space, 5 Enter, 6 Tab. The second byte
(`0x43005`, `gfx_keys_ext()`, constants `KEYX_*`) holds bit 0 Q, 1 E, 2 X/Shift, 3 Z/Ctrl.

Speed register (`0x43003`): 0 = fixed speed (60 frames/s, 30000 instructions per frame), nonzero = maximum speed: the emulator drops the frame cap, skips most display refreshes and runs up to 300000 instructions per pass. Use it from C with `gfx_speed(1)` / `gfx_speed(0)`; time maximum-speed runs with the real-time clock (`gfx_rtc`), since frame ticks no longer match wall time. Page 9 of the showcase uses it to measure the SC-16 for 3 seconds.

Mouse registers (base `0x43008`): `+0` X word, `+2` Y word, `+4` held buttons, `+5` events.
A double click is a second press of the same button within 400 ms and 4 pixels; it raises
the press bit and the double-click bit together. Drag and drop = button held while the
position changes; the release event marks the drop.

Coprocessor registers (base `0x43020`, 16-bit values big-endian and signed):
`+0` CMD (writing it runs the command), `+1` COLOR, `+2` X, `+4` Y, `+6` W,
`+8` H, `+10` SRC word, `+12` SRC high byte, `+14` RESULT. Commands: 1 clear,
2 pixel, 3 rect, 4 line (W,H are the end point), 5 sprite, 6 bitmap, 7 text
(NUL-terminated string at SRC), 8 present, 9 random (to RESULT), 10 polygon
(W is the point count), 13 sound (COLOR channel, X Hz, Y frames, W wave, H volume %),
14 save back buffer, 15 restore it, 16-20 networking (below), 21 tile map (X,Y camera, W,H map size, SRC map). Assembly programs can use it directly.

### LAN games (`net_*`)

Games can talk to other emulator instances on the local network without any server program:
every instance is a peer and finds the others with small UDP broadcast packets (port 47016,
change with `--net-port`; all players must use the same port). Instances only see peers that
called `net_open` with the **same title** (up to 16 characters), so "pacman" ignores "elite".
At most 8 instances share a title; each gets a slot 0-7 (settled about 0.6 s after joining, `net_ready()`;
an already settled player keeps its slot against a newcomer). Peers that vanish time out after about 3.5 s; `net_close()` leaves immediately.
Broadcasts are not delivered back to the sender. Delivery is UDP: messages can be lost or reordered,
so send state continuously rather than relying on single events.

| Function | Description |
|---|---|
| `int net_open(char *title)` | Join the group for `title`; 1 = ok. |
| `void net_close(void)` | Leave the group. |
| `int net_send(unsigned char *data, int len)` | Broadcast 1..`NET_MAX_MSG` (48) bytes to all other instances; 1 = sent. |
| `int net_recv(unsigned char *buf)` | Pop the next message into `buf`; returns its length, 0 if none. |
| `int net_sender(void)` | Slot of the sender of the last received message. |
| `int net_players(void)` | Live instances including this one. |
| `int net_slot(void)` | Own slot 0-7, -1 when not joined. |
| `int net_active(int slot)` | 1 if a player occupies `slot`. |
| `int net_ready(void)` / `int net_full(void)` / `int net_is_open(void)` | Slot settled / group had no free slot / joined. |

Windows may need to allow Python through the firewall for private networks.
[`examples/net_game_demo.c`](examples/net_game_demo.c) shows the pattern.

### Games

[`examples/asteroids.c`](examples/asteroids.c) and
[`examples/space_invaders.c`](examples/space_invaders.c) are complete playable
games with score, lives, and restart. [`examples/pacman.c`](examples/pacman.c)
is a 19x21-tile maze game with four ghosts, power pellets, a wrap-around tunnel,
lives, and levels. Build and run any of them from the repository root:

```
python tools/c_compiler.py examples/asteroids.c -o asteroids.bin
python main.py asteroids.bin
```

[`examples/factorial.c`](examples/factorial.c) and
[`examples/text_demo.c`](examples/text_demo.c) demonstrate C recursion and text
input/output. [`examples/sc16_showcase.c`](examples/sc16_showcase.c) is an
eight-page tour of the SC-16's CPU, memory, graphics, text, devices, and
benchmark features.

All games and the showcase play sound effects through `gfx_sound` (lasers, explosions,
engine rumble, pellet and ghost sounds, page and benchmark beeps); sound is
synthesized by the emulator and needs a working pygame audio device.

Controls: Left/Right (A/D) steer or move; Up (W) thrusts in Asteroids; Space
fires; Enter or Space restarts after GAME OVER. In Pac-Man, all four directions
steer. The showcase uses Left/Right (A/D) to change pages and Enter to run the
benchmark on page 8. Game run time is roughly 15,000-25,000 emulated
instructions per frame.

## 10. Examples

- [`examples/example.asm`](examples/example.asm) draws a letter `H` into the emulated display
  memory.
- [`examples/commands.asm`](examples/commands.asm) contains at least one
  example of every supported instruction mnemonic. `EI`, `DI`, and `RTI` are
  placed after `HALT` so they are assembled but not executed by this tour.
- [`examples/directives.asm`](examples/directives.asm) demonstrates `.address`
  and all `.data` formats across multiple memory locations.
- [`examples/gfx_demo.asm`](examples/gfx_demo.asm) demonstrates plot, line,
  rectangle, sprite, and text routines.
- [`examples/factorial.c`](examples/factorial.c) demonstrates the C compiler,
  recursive calls, local variables, arithmetic, and a `for` loop.
- [`examples/anaclock.c`](examples/anaclock.c) displays the emulator RTC as a
  live clock with five classic analog faces, a Nixie-tube simulation, and a
  split-flap flip clock. Each analog face has its own hand design, Nixie digits
  dim and re-ignite when they change, and flip cards animate each change.
  Press Tab to cycle through the seven faces. Run it with
  `python main.py examples/anaclock.c`.
- [`examples/elitedemo.c`](examples/elitedemo.c) is a first-person Elite-style
  space-combat prototype around a planet with two orbiting moons, drifting
  asteroids and patrolling enemy fighters that dogfight and shoot back. Flight is
  Newtonian-inspired: the ship keeps its velocity when it turns. Arrows pitch/yaw
  (Up = nose up), Q/E roll, X or Shift thrust, Z or Ctrl reverse, Space fires a
  heat-limited laser. It has regenerating shields, a hull, collisions with every
  body, a 3D radar and a HUD. It needs roughly 100,000 instructions per frame.
  Run it with `python main.py examples/elitedemo.c`.
- [`examples/lunarlander.c`](examples/lunarlander.c) is a Lunar Lander game with
  gravity, inertia, limited fuel, procedural terrain with x2/x3/x5 landing pads,
  exhaust, dust and explosion particles, a starfield with the Earth, a HUD with
  altitude, velocity and fuel, and synthesized sound (engine, RCS, ambient drone,
  landing jingle, crash). Modes: Classic, Practice (unlimited fuel) and Challenge
  (rough terrain, stronger gravity), each with three difficulty levels. Space fires
  the main engine, Left/Right (A/D) rotate; in the menu Up/Down picks the mode,
  Left/Right the difficulty and Enter starts. Score = (fuel + speed bonus +
  accuracy bonus) x pad multiplier. Run it with `python main.py examples/lunarlander.c`.
- [`examples/paint.c`](examples/paint.c) is an MS Paint style drawing program driven by the
  mouse: pen, eraser, spray, line, rectangle and filled rectangle with live shape previews,
  four brush sizes and a 16-color palette. Left button draws with the foreground color, right
  button with the background color; double-click a palette color to fill the canvas and
  double-click CLR to clear it. Images cannot be saved. Run it with `python main.py examples/paint.c`.
- [`examples/net_game_demo.c`](examples/net_game_demo.c) is a LAN demo: start it in up to 8
  emulator instances on one network; each player moves a coloured square (arrows) and Space
  broadcasts a ping ring to all others. Run it with `python main.py examples/net_game_demo.c`.
- [`examples/spacecave.c`](examples/spacecave.c) (SpaceCave) is an Asteroids-style LAN dogfight
  for up to 8 pilots inside a procedurally generated cave with auto-repairing defence turrets.
  Start it in one emulator per player (`python main.py examples/spacecave.c`), type a nickname and
  press Enter. Left/Right rotate, Up thrust, Space fire, hold Tab for the full scoreboard. The
  lowest network slot acts as the authoritative host: clients only send their keys, the host simulates
  and broadcasts ships, bullets, turrets and scores, and the cave is rebuilt from a broadcast seed.
  A disconnected pilot's score stays on the board (slot reserved) for 10 s; a destroyed ship
  respawns after 10 s at a safe random spot. If the host leaves, the next slot takes over.
  In the menu, Left/Right switches between **Classic** (pure dogfight) and **Mission** mode; clients
  follow the host's mode. Mission mode adds one target per map (type chosen by the map seed):
  a *beacon* (fly into it, +150), a *reactor* (shoot it down, +300, rebuilt elsewhere) or six
  *crystals* (+40 each, +140 for the last), plus four roaming drones (+25 when shot; they chase
  pilots in line of sight and damage them on contact). An edge marker points to the nearest target.
- [`examples/text_demo.c`](examples/text_demo.c) demonstrates C text output,
  cursor positioning, buffered keyboard input, and printing.
- [`examples/asteroids.c`](examples/asteroids.c) and
  [`examples/space_invaders.c`](examples/space_invaders.c) are playable C games.
- [`examples/pacman.c`](examples/pacman.c) is a tile-based maze game with ghost
  AI, power pellets, and tunnel wrapping.
- [`examples/sc16_showcase.c`](examples/sc16_showcase.c) presents an interactive
  eight-page SC-16 feature tour and benchmark. The playable games are written in C
  and use the graphics API; their controls are described in the Games section.

These examples document the current assembler and emulator behavior. Register
arithmetic is not strictly limited to eight bits, and the CPU's `LDI` pair loads
16-bit values; these are implementation details rather than guarantees of a
conventional 8-bit processor.

## 11. Reusable graphics library

[`lib/lib_gfx.asm`](lib/lib_gfx.asm) provides assembly-callable routines.
Include it after the main code (its routines occupy addresses starting at
`0x0400`):

```asm
.address 0x0100
start:
    LDI R0, 20
    LDI R1, 20
    LDI R2, 25
    CALL GFX_PLOT
    HALT
.include "lib_gfx.asm"
```

`CALL`/`RET` and the full-width conditional branches support reusable routines.
The graphics library calling conventions are:

| Routine      | Inputs                                                     | Effect                                                                            |
| ------------ | ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `GFX_PLOT`   | `R0=x`, `R1=y`, `R2=color`                                 | Write one palette index at a pixel. Preserves `R0`-`R3`; clobbers `R4`-`R7`.      |
| `GFX_CLEAR`  | none                                                       | Clear all 320x240 pixels to color 0. Clobbers all registers.                      |
| `GFX_HLINE`  | `R0=x`, `R1=y`, `R2=length`, `R3=color`                    | Draw a horizontal line. Length must be 1-255. Clobbers `R0`, `R2`, and `R4`-`R7`. |
| `GFX_RECT`   | `R0=x`, `R1=y`, `R2=width`, `R3=height`, `R4=color`        | Draw a filled rectangle. Width and height must be 1-255. Clobbers all registers.  |
| `GFX_SPRITE` | `R0=x`, `R1=y`, `R2=width`, `R3=height`, `R4=data address` | Draw row-major palette bytes; zero is transparent. Clobbers all registers.        |
| `GFX_TEXT`   | `R0=x`, `R1=y`, `R2=text-data address`, `R3=color`         | Draw 6x8 cells using the font include. Clobbers all registers.                    |

Coordinates are pixels from the top-left. Drawing beyond the screen is not
clipped. Pixel and sprite colors are palette indexes. A sprite is a sequence of
`width * height` bytes; `.data hex` is suitable for small sprites. Text data
consists of glyph IDs: printable ASCII uses its character code minus 31, IDs
96-101 select `ÃƒÆ’Ã‚Â¶ÃƒÆ’Ã‚Â¤ÃƒÆ’Ã‚Â¥ÃƒÆ’Ã¢â‚¬â€œÃƒÆ’Ã¢â‚¬Å¾ÃƒÆ’Ã¢â‚¬Â¦`, and `0` terminates the text. `GFX_TEXT` accepts a full
19-bit text pointer. Its x/y inputs are stored as bytes and therefore should be in the
range 0-255. Its scratch bytes at `0x4300`-`0x4306` are reserved while it runs.
Include the glyph data so it is loaded at `0x4400`; the mask table is supplied
by `lib_gfx.asm` at `0x4200`.

These routines are simple reference implementations rather than optimized
renderers. In particular, `GFX_CLEAR`, `GFX_RECT`, `GFX_SPRITE`, and `GFX_TEXT`
execute many instructions per pixel. Use them sparingly in timing-sensitive
loops. Code, graphics scratch data, and the stack must not overlap.

## SC-16 showcase and timing/interrupt additions

GPU commands 11 and 12 (in addition to 1-10):

| Cmd | Name  | Effect                                                                |
| --- | ----- | --------------------------------------------------------------------- |
| 11  | TICKS | Frame counter written big-endian to RESULT (+14/+15)                  |
| 12  | RTC   | RTC register selected by COLOR (0 sec, 1 min, 2 hour) to RESULT (+14) |

C builtins: `gfx_ticks()`, `gfx_rtc(field)`, `gfx_irq_init()` (executes EI),
`gfx_irq_ticks()` (timer interrupt count, word at 0x4340) and
`gfx_irq_keys()` (key interrupt count, word at 0x4342).

Interrupt handlers live in `lib_cgfx.asm` at fixed addresses: `irq_timer` at
0x0010 (vector 0x0004) and `irq_key` at 0x0030 (vector 0x0002). The C entry stub
is `JMP __c_boot` at 0 with the boot code at 6. `RTI` restores both Z and C flags.
C globals start at 0xE800.

Run the showcase (9 pages; Left/Right or A/D change page, Enter runs the benchmark on page 8 and a quick speed measurement on page 9, which compares the SC-16 in MIPS with the Intel 4004, 8088, 6502, 68000, 486DX2-66, CRAY-1 and Raspberry Pi A):

    python tools/c_compiler.py examples/sc16_showcase.c -o showcase.bin
    python main.py showcase.bin

## Optional JIT acceleration

If [Numba](https://numba.pivot.org) is installed (`pip install numba`), the emulator runs the CPU core as compiled machine code, typically 5-15x faster than the pure-Python interpreter (the first start compiles and caches it, taking a few seconds). Without Numba the emulator falls back automatically to the pure-Python core. Set `SC16_NOJIT=1` to force the Python core. Both cores are verified identical by `tests/test_jit_core.py`.
