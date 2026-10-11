# SC-16 Assembler and Emulator User Guide

## 1. Overview

SuomiCPU is a Python emulator for the SC-16, a small, byte-addressed machine with
eight general-purpose registers and a 16-bit instruction format. Assembly source
files use the `.asm` extension. The assembler converts instructions to machine
code and places instructions or data at the addresses specified in the source.

The emulator presents a BIOS startup screen before launching a program. The
BIOS selects and compiles a program from `modules/`, optionally mounts an
existing virtual data disk, and loads the resulting image into machine memory.
Disks hold application data only; they are not boot media. Once started, CPU
reset begins execution at the loaded image's entry point. The first instruction
in the source determines that entry point, regardless of the numeric order of
the image's memory segments.

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
`main.py`. It requires Python to be available as `python` on `PATH`. At startup,
the BIOS lists each subfolder in `modules/` as a module, using the folder name
as its module name. A folder with one `.c` file loads that file; a folder with
multiple `.c` files must contain `main.c`, which is loaded instead. The first
source line is shown as the module description. Folders with no `.c` file or
with multiple `.c` files but no `main.c` remain selectable and show an error
when loaded. Use the arrow keys to select a program and press Enter to load it;
hover or click a program to select it and
double-click to load it. Use the mouse wheel over the module list to scroll;
indicators show how many modules remain above or below the visible rows. Click
the data-disk line or press **D** to cycle through available data disks or no
disk. Press Escape to exit the BIOS.
The game fills its own window by default. The 80×25 console runs in a separate
window. Its **RESET** button returns to BIOS module selection; **REBOOT**
restarts the current program. Select text and press Ctrl+C to copy it; the
**COPY** button copies selected text or the visible console screen. The console
keeps a 1,000-row scrollback buffer, navigated with the mouse wheel. It logs
source and generated assembly file sizes, program image size, address span, and
emulator memory capacity/remaining memory when a program loads. Press F2 in the
emulator window to hide/show the console, and F11 to toggle fullscreen and
windowed game display. Start in fullscreen with
`python main.py --fullscreen` or
`.\run.ps1 -Fullscreen`.

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

The input file argument is optional. Without it, the BIOS compiles and loads a
selected program from `modules/`; storage disks are only for program data. Use
`--disk ID` to preselect a data disk in the BIOS. The emulator uses Pygame for
its window, display, and input.

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
  alignment directive. Base instructions occupy two bytes; extended
  instructions use two, four, or six bytes depending on their operands.

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

Base instructions occupy one 16-bit word (two bytes), stored most-significant
byte first. Extended instructions may use additional words as noted in their
descriptions. Registers are selected from `R0` through `R7`. Unless stated
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

### 32-bit register-pair arithmetic

`ADD32`, `SUB32`, `CMP32`, `AND32`, `OR32`, `XOR32`, `SHL32`, `LSR32`,
`ASR32`, `NEG32`, `MOV32`, `LD32`, `ST32`, `MUL32`, `DIV32`, `DIV32S`, `MOD32`,
and `MOD32S` operate on adjacent register pairs. The even
register is the high 16 bits and the following odd register is the low 16 bits:
for example, `R0` names the `R0:R1` pair and `R2` names `R2:R3`. Binary
arithmetic/logic operations and `MOV32` require even pair registers. Shift
counts may be an immediate from 0 to 31 or the low five bits of a register.
`LD32 Rd, Ra` loads four big-endian bytes at the 19-bit address in pair `Ra`
into pair `Rd`; `ST32 Rs, Ra` stores the value in pair `Rs` there. Both
register-pair operands must start at an even register. Memory access wraps at
the end of the 512 KiB address space and follows normal memory-mapped-device
read/write behavior.
`MUL32` writes the low 32 product bits; it sets `C` for unsigned overflow and
`V` for signed overflow. `DIV32`/`MOD32` are unsigned; the `S` forms are signed,
with division truncated toward zero and remainder following the dividend.
Division by zero yields zero for quotient or the dividend for remainder and
sets `C`; nonzero divisors clear `C`. These arithmetic instructions set `Z`
and `N` from the result.

`ADD32`/`SUB32` set `Z`, `N`, `C` and `V` for 32-bit arithmetic; `CMP32` sets
the same flags without changing its destination. `AND32`/`OR32`/`XOR32` set
`Z` and `N` and clear `C` and `V`. Shifts set `Z`, `N`, and `C` (the last bit
shifted out) and clear `V`. A zero-count shift preserves `C`. `NEG32` sets
`Z`, `N`, `C` (nonzero input), and `V` (the signed minimum value). `MOV32`
does not change flags.

These instructions use the extended `0x1A` selector in the `OP_SHR` space and
are implemented by the Python emulator. The optional JIT hands this group back
to the emulator. The `MOVSPH` miscellaneous instruction reads the high three
bits of the 19-bit stack pointer; together with `MOVSP`, it lets compiler code
form full stack addresses for 32-bit objects.

Encoding: word 1 is `0xF800 | (high-register << 8) | 0x1A`; word 2 is
`(operation << 12) | (immediate flag 0x0800) | source/count`. Operation codes
are `ADD32`=0, `SUB32`=1, `CMP32`=2, `AND32`=3, `OR32`=4, `XOR32`=5,
`SHL32`=6, `LSR32`=7, `ASR32`=8, `NEG32`=9, and `MOV32`=10. Shift immediates
use the low five bits and the immediate flag; register shift counts use the
low five bits of the selected register. `LD32`=11 and `ST32`=12. The group occupies the previously
unused `0x1A` selector, distinct from the existing `0x1B`-`0x1F` extensions.
`MUL32`=13, `DIV32`/`DIV32S`=14, and `MOD32`/`MOD32S`=15. The signed `S`
forms set the immediate/modifier flag in word 2; division and remainder
operands otherwise use an even source register pair.

### Integer multiply and divide

| Instruction | Syntax                       | Operation                                                                       |
| ----------- | ---------------------------- | ------------------------------------------------------------------------------- |
| `MUL`       | `MUL Rd, Rs` / `MUL Rd, imm` | `Rd` = low 16 bits of `Rd * source`; `C` is set if the product exceeds 16 bits. |
| `MULHU`     | `MULHU Rd, Rs` / `imm`       | `Rd` = high 16 bits of the unsigned 32-bit product.                             |
| `MULHS`     | `MULHS Rd, Rs` / `imm`       | `Rd` = high 16 bits of the signed 32-bit product.                               |
| `DIV`       | `DIV Rd, Rs` / `imm`         | `Rd = Rd / source`, unsigned.                                                   |
| `DIVS`      | `DIVS Rd, Rs` / `imm`        | `Rd = Rd / source`, signed, truncating toward zero.                             |
| `MOD`       | `MOD Rd, Rs` / `imm`         | `Rd = Rd % source`, unsigned.                                                   |
| `MODS`      | `MODS Rd, Rs` / `imm`        | `Rd = Rd % source`, signed; the sign follows the dividend.                      |

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

| Instruction                   | Syntax                     | Operation                                                                                                                |
| ----------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `SHL` `LSR` `ASR`             | `SHL Rd, n` / `SHL Rd, Rs` | Shift left, logical right, arithmetic right. Count is 0-15 (a register uses its low 4 bits). `C` = last bit shifted out. |
| `ROL` `ROR`                   | `ROL Rd, n` / `ROL Rd, Rs` | Rotate left/right by 0-15.                                                                                               |
| `ADC` `SBC`                   | `ADC Rd, Rs` / `imm`       | Add/subtract including the carry flag, for multi-word arithmetic.                                                        |
| `NEG`                         | `NEG Rd`                   | Two's complement negate.                                                                                                 |
| `TEST`                        | `TEST Rd, Rs` / `imm`      | `Rd AND source`; sets flags only.                                                                                        |
| `ADD SUB AND OR XOR CMP`      | `ADD Rd, imm`              | Immediate forms (a non-register second operand selects them).                                                            |
| `SWAP` `SEXT` `CLZ`           | `SWAP Rd`                  | Swap bytes; sign-extend the low byte; count leading zeros (all in place).                                                |
| `JMPR` `CALLR`                | `JMPR Rd`                  | Jump/call to the 16-bit address held in `Rd`.                                                                            |
| `JMPRP` `CALLRP`              | `JMPRP Rlow, Rhigh`        | Jump/call through a 19-bit pointer: low 16 bits in `Rlow`, high 3 bits in `Rhigh`.                                      |
| `PUSHF` `POPF`                | `PUSHF`                    | Push/pop the flags (Z=1, C=2, N=4, V=8).                                                                                 |
| `SETC` `CLC`                  | `SETC`                     | Set/clear the carry flag.                                                                                                |
| `SETSP`                       | `SETSP Rd`                 | Load the stack pointer from `Rd`.                                                                                        |
| `NOP`                         | `NOP`                      | Do nothing (`MOV R0, R0`).                                                                                               |
| `BNZX BNCX BNX BNNX BVX BNVX` | `BNZX label`               | Far branch if `Z`, `C`, `N`, `V` is clear (`BNZ`, `BNC`, `BNV`) or set (`BN`, `BV`); `BNNX` = `N` clear.                 |
| `BLT BGE BGT BLE`             | `BLT label`                | Signed compare branches after `CMP` (use `N` and `V`).                                                                   |
| `BHI BLS`                     | `BHI label`                | Unsigned higher / lower-or-same after `CMP`.                                                                             |

Every branch also has an `X` spelling (`BLTX`, ...); all conditional branches
here are four-byte, full-address forms. `ADD`, `SUB` and `CMP` now also set the
`N` (negative) and `V` (signed overflow) flags. `NOT` and `POP` now produce
full 16-bit values. Encoding: opcode `0x1F` with low-byte prefix `0x1F`
(ALU/misc group: `(op<<12) | 0x0800 immediate flag | operand`, ops `SHL`=0,
`LSR`=1, `ASR`=2, `ROL`=3, `ROR`=4, `ADC`=5, `SBC`=6, `NEG`=7, `TEST`=8,
`ADD`=9, `SUB`=10, `AND`=11, `OR`=12, `XOR`=13, `CMP`=14, misc=15).
Compact far control uses prefix `0xF83E` and carries the operation plus the
target's upper three bits in the prefix word. Far conditional branches use
prefix `0xF80D`, with the condition and upper address bits in that word.
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
| `CALLRP`    | `CALLRP Rlow, Rhigh`      | Call the 19-bit address in `Rlow` and the low 3 bits of `Rhigh`. |
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
`JMPX`, `BZX`, `BCX`, and `CALLX` use four bytes and encode a full 19-bit address;
use them when code or routines are outside the short-address range.
All `CALL`, `CALLX`, `CALLR`, and `CALLRP` instructions push the following
19-bit PC as three big-endian bytes; `RET` restores all 19 bits. `CALLRP` uses
the register-pair layout of C object pointers: low 16 address bits in `Rlow`
and high three bits in `Rhigh`.

The compact far-control prefix word uses bits 10-9 for the operation, bits 8-6
for address bits 18-16, and fixed selector `0x3E` in bits 5-0. Its second word
contains address bits 15-0. Far conditional branches use bits 10-7 for the
condition code, bits 6-4 for address bits 18-16, and fixed selector `0xD` in
bits 3-0; their second word also contains address bits 15-0. Both encodings
therefore cover the full 512 KiB address space in four bytes.

### Stack and interrupt instructions

| Instruction | Syntax              | Operation                                                                 |
| ----------- | ------------------- | ------------------------------------------------------------------------- |
| `PUSH`      | `PUSH Rd`           | Push a 16-bit register value to the stack.                                |
| `POP`       | `POP Rd`            | Pop a value from the stack into `Rd`; the register is masked to one byte. |
| `EI`        | `EI`                | Set the interrupt-enable bit in the interrupt control register.           |
| `DI`        | `DI`                | Clear the interrupt-enable bit.                                           |
| `RTI`       | `RTI`               | Restore the flags and 19-bit PC from the five-byte interrupt frame.       |
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
| Interrupt control (`ICR`)                |           `0x43002` |       1 byte |
| Speed register (0 = fixed, 1 = maximum)  |           `0x43003` |       1 byte |
| Held-key bitmask                         |           `0x43004` |       1 byte |
| Disk storage device                      |           `0x43100` |     16 bytes |
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

The disk storage device is accessible starting at `0x43100`. It uses JSON files
to store data blocks of 256 characters.
- Write `0x01` to `0x43100` to load a disk; the 8-character disk ID must be placed
  at `0x43101` through `0x43108`.
- Write `0x02` to `0x43100` to unload and save the current disk.
- Write `0x04` to `0x43100` to read a block; the 16-bit block pointer is at
  `0x4310A` and the RAM destination offset is at `0x4310C`. The block is copied
  to `0x20000 + offset`.
- Write `0x08` to `0x43100` to write a block; the 16-bit block pointer is at
  `0x4310A` and the RAM source offset is at `0x4310C`. Data is read from
  `0x20000 + offset`.
- Write `0x10` to `0x43100` to read the disk directory: the disk number (0-based)
  is taken from `0x4310A`, and the ID of that disk (zero terminated, empty when
  out of range) is copied to `0x20000 + offset` (offset at `0x4310C`). The number
  of disks in the `disks/` folder is returned in the word at `0x4310E`. The byte
  after the terminating zero (offset 9) is `1` if that disk is write protected.
- Write `0x20` to `0x43100` to set write protection of the loaded disk: `0` (off)
  or `1` (on) in `0x4310A`. The flag is saved immediately to the disk's JSON file
  as `"protected": true`, and writes to a protected disk are ignored. After a
  load (`0x01`) or `0x20`, the word at `0x4310E` holds the protection state.

`modules/DiskEditor/disk_editor.c` is a disk editor built on this interface: it
lists the disks, creates new ones, and lets you edit and save blocks with the
keyboard.

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

## 9. ANSI C90 compiler

[`tools/c_compiler.py`](tools/c_compiler.py) is a C90-oriented compiler, **not a
fully conforming ANSI/ISO C90 implementation**. It implements a substantial
subset of the language and common preprocessing features, but some required
C90 constructs, target type requirements, and the standard library are not
implemented. Do not assume that a program accepted by another C90 compiler
will compile or have identical behavior on SC-16.

The ABI choices targeted for full conformance, including type representations,
pointer encoding, aggregate layout, and function-call conventions, are recorded
in [`docs/c90_target_abi.md`](docs/c90_target_abi.md). The document describes
the intended target and does not imply that the current compiler implements
those choices completely.

Supported language features include:

- File-scope and block-scope declarations; function prototypes, modern
  definitions (including definitions with omitted return type, which defaults
  to `int`), and identifier-list (old-style) definitions with pre-body
  parameter declarations (undeclared old-style parameters default to `int`);
  unspecified-parameter declarations using `f()` distinct from no-argument
  prototypes using `f(void)`; recursion; and C90 declarations before statements
  within a block. Variadic prototypes and definitions accept calls with the
  required fixed arguments followed by additional arguments, applying the
  default promotions needed by the current target types. `<stdarg.h>` provides
  `va_list`, `va_start`, `va_arg`, and `va_end` for accessing variadic arguments.
- `typedef` names, including one-dimensional array typedefs, enumerations,
  structure and union declarations, member access with `.` and `->`, scalar,
  array, and aggregate initialization, and C90
  integer type specifiers including standalone `signed` and `unsigned`;
  `char` and `unsigned char` operands receive integer promotions. Global
  pointer initializers accept null pointer constant expressions and the
  compiler's SC-16 hexadecimal address-constant extension. Redeclarations with
  incompatible function parameter types are diagnosed. Unary arithmetic,
  bitwise complement, and logical operators diagnose operands that do not meet
  their C90 type constraints.
- Bounded arrays, including multidimensional arrays and initialized arrays with
  an omitted outer bound,
  integer constant expression bounds (including arithmetic, enum constants,
  casts, `sizeof` on complete object types, and conditional expressions), array parameters
  adjusted to pointers, pointers, casts, `sizeof`, and the comma and conditional
  operators.
- Blocks, expression statements, `if`/`else`, loops, `switch` with integer
  constant-expression `case` labels, `default`, `break`, `continue`, `goto`,
  labeled statements, and `return`; supported scalar arguments and return
  expressions are converted to their declared parameter or function return
  types, with incompatible pointer/scalar conversions rejected.
- Integer and floating constants, character constants, strings (including
  adjacent string-literal concatenation), assignment and compound assignment,
  increment/decrement, arithmetic, bitwise and shift operations, comparisons,
  and logical operators with short-circuit evaluation. Binary operators check
  arithmetic, integer, and scalar operand requirements and reject invalid
  pointer arithmetic and comparisons. Conditional and control-flow conditions
  must also be scalar expressions. Compound assignments use the corresponding
  arithmetic and pointer rules, including pointer scaling and floating
  conversions. Pointer subtraction returns an element difference rather than
  a byte difference. Arrays and enumerators are rejected as modifiable
  lvalues; compatible structure and union assignments copy the complete object
  representation, and fixed-parameter structure and union arguments are copied
  by value. Structure and union returns use caller-owned result storage and are
  tested with nested calls and recursion; members of a returned temporary are
  readable but are not modifiable lvalues. Duplicate storage-class
  specifiers and duplicate `const`/`volatile` qualifiers are diagnosed.
  `const` is enforced for supported non-pointer scalar and array objects,
  structure/union objects and members, including `typedef const` aliases;
  initializers and reads remain supported. Whole-record assignment is rejected
  when the record contains a const-qualified member, while assignments to its
  other mutable members remain valid.
  Scalar initializers may use nested braces around exactly one value.
- Object-like and function-like macros, `#`, `##`, `defined`, conditional
  inclusion, `#include`, `#undef`, `#error`, `#pragma`, trigraph replacement,
  and backslash-newline splicing. Quoted includes are searched relative to the
  including file; angle-bracket includes are searched in the repository's
  `lib/` directory.

The compiler's data model is target-specific and does **not** meet all the C90
minimum requirements:

- `int` and `unsigned int` are 16-bit; `char` and `unsigned char` occupy one
  byte. `short` is 16-bit, and `long`/`unsigned long` are 32-bit with the
  minimum ranges required by C90. The compiler implements pair-based storage,
  scalar arithmetic, `switch`, and function argument/return handling for these
  `long` types. The compiler supports indirect 32-bit lvalues and current
  19-bit pointer address paths. Regression tests cover high-address byte, word,
  and `long` accesses as well as indexed accesses across `0xFFFF`, but not every
  pointer conversion, arithmetic, alignment, or wraparound case. Integer
  literal types use the target's `int`, `unsigned int`, and 32-bit `long`
  candidate ranges. An explicit cast from a 32-bit integer to an object pointer
  uses its low 19 bits as the address; this is the target's implementation-
  defined conversion. For compatibility with existing programs, a file-scope
  pointer initializer also accepts an integer constant in the SC-16 19-bit
  address range as an implementation extension. Converting an object pointer
  to a 32-bit integer preserves its 19-bit address value; conversion to 16-bit
  `int` preserves the low 16 bits. These pointer/integer conversions are
  implementation-defined.
- `float`, `double`, and `long double` use the SC-16 IEEE-754 binary16 format.
  In particular, `double` does not meet C90's required minimum range or
  precision.
- Object pointers occupy three bytes for the machine's 19-bit address space.
  `size_t` is unsigned 32-bit `long`, and `ptrdiff_t` is signed 32-bit `long`;
  compatible object-pointer subtraction produces a signed 32-bit element
  difference. Pointer arithmetic and dereferencing have not yet been
  exhaustively checked for all address, alignment, and overflow cases.
  Function pointers and general function-pointer calls are not supported.
- `static`, `extern`, `auto`, `register`, and `volatile` specifiers do not have
  their full C linkage, storage-duration, or volatile-access semantics.
  The address-of constraint is enforced for local objects declared `register`,
  and `auto`/`register` file-scope declarations and storage classes on record
  members are diagnosed. Named `register` parameters in supported prototypes
  and definitions also reject address-taking.
  `const` objects, one-level const-qualified pointer targets, named function
  parameters, and one-level const-qualified pointer returns are supported for
  the implemented forms. Reads through const pointers, including casts to
  `const T *`, are allowed; writes are rejected, and assignment, function
  arguments, and return statements prevent discarding the target's const
  qualifier. Function declarations also diagnose conflicting one-level
  pointer-target qualifications. Const qualification through multiple pointer
  levels and const-qualified pointer objects are not supported; do not treat
  other accepted pointer qualifier combinations as strictly conforming.
- Global storage begins at `0x2000` (`GLOBAL_BASE` in
  [`tools/c_compiler.py`](tools/c_compiler.py)); uninitialized globals are
  zeroed. Local variables, arguments, return addresses, and temporaries use the
  machine stack.

Important language gaps include multidimensional array function-parameter
declarators beyond the supported forms, function pointers, and parts of C90's
declarator, initializer, and
constant-expression rules. The preprocessor also does not implement every
translation-phase and directive semantic required for strict C90 conformance.
Numeric `#line` directives remap subsequent diagnostic line numbers, including
preprocessor diagnostics. An optional filename remaps subsequent diagnostic
filenames and the value of `__FILE__`; without a filename, the current mapped
filename is retained.

The library is incomplete. The repository provides `<stddef.h>`, `<limits.h>`,
`<float.h>`, `<ctype.h>`, `<errno.h>`, `<assert.h>`, `<stdlib.h>`,
`<string.h>`, `<stdarg.h>`, `<locale.h>`, and `<time.h>`. `<stddef.h>` includes `offsetof`,
which supports nested struct/union member designators in constant initializers.
The string/memory
routines, C-locale character routines, and a subset of `<stdlib.h>` (`abs`,
`labs`, `atoi`, `atol`, `strtol`, `strtoul`, `rand`, `srand`, `getenv`, and
`system`) are compiled into a program when referenced. `exit(status)` and
`abort()` halt the target; `EXIT_SUCCESS` is zero and `EXIT_FAILURE` is one,
but the emulator has no host process exit-status channel and cannot perform
`atexit` callbacks or stream cleanup. `strtol` and `strtoul` support base
selection, end pointers, and `errno` range reporting using the target's 32-bit
`long` limits; tests cover signed and unsigned boundary values and overflow.
There is no target environment-variable store, so `getenv` returns `NULL`;
there is no command processor, so `system(NULL)` returns zero and commands
return -1. `<assert.h>` evaluates assertions unless `NDEBUG` is defined and
stops the target on a failed assertion; enabled assertions expand to a `void`
expression and evaluate their argument once. `RAND_MAX` is 32767; the sequence
from `rand` is implementation-defined. `<locale.h>` exposes only the `"C"` locale;
`setlocale` rejects other locale names, while `localeconv` reports the C locale
numeric and monetary conventions. `<time.h>` defines `clock_t`, `time_t`, and
`CLOCKS_PER_SEC`, and provides `clock` and `time`; since the target has no
real-time clock or processor-time counter, both report `(time_t)-1`/
`(clock_t)-1` (and `time` stores the sentinel when given a non-null pointer).
Calendar conversion, formatting, and elapsed-time functions are not
implemented. The limit and floating-point macros describe
the target's actual types; they also make C90 minimum-range incompatibilities
visible: `double` has the same binary16 precision and range as `float`, and
`long double` maps to that same format as well. `size_t` is an unsigned 32-bit
`long`, so it can represent the target's 19-bit address range; full 19-bit
pointer arithmetic and object allocation remain unimplemented. These headers
are partial: many required `<stdlib.h>` facilities and other C90 headers are
not implemented. The project's screen and graphics routines are SC-16-specific
extensions, not replacements for those standard facilities. Global
initializers must be constant expressions supported by the compiler.

### Console, BIOS, and virtual disks

The emulator starts with a BIOS selection screen. It lists C programs from
`modules/` for selection and can optionally mount any valid 8-character `*.json`
virtual disk found in the repository's `disks/` folder. The selected disk is
made available to the program; it is **not** used to load executable code. The
BIOS compiles the selected source and the emulator loads it into machine memory.
Use the mouse to hover or click a program to select it, then double-click to
load it. Click the data-disk line to cycle disks; keyboard controls remain
available.
The optional `--disk ID` command-line option preselects a data disk in the BIOS.

The BIOS information block starts at `0x43040`: bytes `0..3` are the `SC16`
signature, bytes `4..5` are the BIOS version, bytes `6..8` contain the memory
size, bytes `9..11` contain the console data-register address, bytes `12..14`
contain the disk-control address, and byte `15` is the startup status.
The status is `0` after successful initialization or disk selection and `1`
when a disk cannot be mounted.

The console is a memory-mapped UART-like device:

| Address | Register | Meaning |
| ------- | -------- | ------- |
| `0x43200` | Data | Write one output byte; read and remove one input byte. |
| `0x43201` | Status | Bit 0: input available; bit 1: output ready; bit 2: input overflow. Write bit 2 as 1 to clear overflow. |
| `0x43202` | Control | Bit 0 enables local echo; bit 1 enables host keyboard input. Both are enabled at reset. |

The game and 80×25 text console run in separate windows. The console has
**REBOOT**, **RESET**, **COPY**, and **PAUSE/CONTINUE** buttons in its top row.
**COPY** copies selected text or the visible console screen; Ctrl+C copies
selected text. The 1,000-row scrollback can be navigated with the mouse wheel.
Program loading logs source and generated assembly file sizes, image size,
address span, and available SC-16 memory. **RESET**
returns to BIOS module selection, while **REBOOT** restarts the current program.
The status line below
shows local time, uptime, mounted data disk, and loaded module. It also displays
host lifecycle messages such as emulator startup, C compilation/assembly,
program loading, disk operations, and errors. Type into the console text area
to send input; Enter and Backspace are supported. Press F2 in the emulator window
to hide/show the console. An empty console-status poll yields the current
instruction frame to the host event loop, so a C `getchar` wait does not prevent
keyboard events from being processed.

The virtual disk device retains its JSON-backed, 256-byte block format.
`0x43100` is the command register; the command bits load/unload a disk,
read/write a block, list disk IDs, and set write protection. The 16-bit block
index is at `+10`, the RAM buffer offset at `+12`, and command results are at
`+14`; `+16` reports status (`0` success, `1` operation error). Invalid IDs,
absent media, malformed images, and write-protected writes set the error
status. Disk IDs are exactly eight ASCII letters, digits, `_`, or `-`.

`<stdio.h>` currently provides console `getchar`, `putchar`, `puts`, `fgetc`,
`fputc`, `fputs`, `printf`, `fprintf`, `vprintf`, and `vfprintf`. The formatter
handles integer, character, string, and pointer conversions (`d`, `i`, `u`,
`o`, `x`, `X`, `c`, `s`, `p`, `n`, and `%%`) with field width and precision.
Unsupported conversions return `EOF`. File-backed streams and floating-point
conversions are still unimplemented; this is a useful hosted-I/O subset, not a
claim of complete C90 `stdio` conformance.

When a screen routine is referenced, the compiler links [`lib/lib_text.asm`](lib/lib_text.asm), which uses
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
`BCX`). Address 0 holds an entry stub; `0x0002`-`0x0005` hold interrupt
vectors, and the boot routine at `0x0006` calls `main`. Compiled code starts at
`0x4800`; global data starts at `0x2000`, so large code and data sections must
not overlap. Return addresses preserve the full 19-bit PC; code is no longer
restricted to addresses below `0x10000` for call/return correctness. Libraries use fixed
low-memory locations: `lib_gfx` at `0x400`, `lib_text` at `0x700`, and
`lib_cgfx` at `0x1000`. The compiler/assembler reports errors when emitted
sections overlap or stack frames exceed supported limits. The command-line
compiler writes a flat `.bin` image with address gaps zero-filled; the emulator
loads it at address zero. In [`examples/factorial.c`](examples/factorial.c),
the global `result` is stored at `0x2000` after execution.

The preprocessor implements the directives and extensions listed above, but is
not certified against the full C90 translation limits or conformance suite.
For shifts, `>>` is arithmetic for `int` and logical for unsigned types.

### C graphics library (`gfx_*`)

[`lib/suomi_gfx.h`](lib/suomi_gfx.h) declares the graphics builtins and defines color
(`RED`, `GREEN`, ...), key (`KEY_LEFT`, `KEY_FIRE`, ...) and screen-size
constants. Using any `gfx_*` function makes the compiler link
[`lib/lib_cgfx.asm`](lib/lib_cgfx.asm), which drives the graphics coprocessor, plus the
font. All drawing goes to an off-screen back buffer; `gfx_present()` copies it
to VRAM in one step and ends the current emulator frame (vsync), so there is
no flicker.

| Function                                                                                | Behavior                                                                                                                                                   |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `void gfx_clear(unsigned char color)`                                                   | Fill the back buffer.                                                                                                                                      |
| `void gfx_pixel(int x, int y, unsigned char color)`                                     | Plot one pixel.                                                                                                                                            |
| `void gfx_rect(int x, int y, int w, int h, unsigned char color)`                        | Filled rectangle.                                                                                                                                          |
| `void gfx_line(int x0, int y0, int x1, int y1, unsigned char color)`                    | Line (clipped).                                                                                                                                            |
| `void gfx_poly(int x, int y, int n, char *pts, unsigned char color)`                    | Closed outline through `n` signed-byte `(dx, dy)` pairs relative to `(x, y)`.                                                                              |
| `void gfx_sprite(int x, int y, int w, int h, unsigned char *data)`                      | Draw `w*h` palette bytes; 0 is transparent.                                                                                                                |
| `void gfx_bitmap(int x, int y, int w, int h, unsigned char *data, unsigned char color)` | 1 bit per pixel, MSB first, `(w+7)/8` bytes per row.                                                                                                       |
| `void gfx_text(int x, int y, char *text, unsigned char color)`                          | 6x8-cell text from the 5x7 font; UTF-8 `??????` and `\n` supported.                                                                                        |
| `void gfx_present(void)`                                                                | Show the back buffer and wait for the next frame.                                                                                                          |
| `unsigned int gfx_keys(void)`                                                           | Held-key bitmask.                                                                                                                                          |
| `unsigned int gfx_keys_ext(void)`                                                       | Second key byte: Q roll left, E roll right, X/Shift thrust, Z/Ctrl reverse.                                                                                |
| `unsigned int gfx_random(void)`                                                         | Random byte (0-255).                                                                                                                                       |
| `int gfx_mouse_x(void)` / `int gfx_mouse_y(void)`                                       | Mouse position in screen pixels (0-319, 0-239).                                                                                                            |
| `unsigned int gfx_mouse_buttons(void)`                                                  | Held buttons: `MOUSE_LEFT` 1, `MOUSE_RIGHT` 2 (use with position for dragging).                                                                            |
| `unsigned int gfx_mouse_events(void)`                                                   | One-frame events: press `MOUSE_LEFT_DOWN`/`RIGHT_DOWN` 1/2, double-click `MOUSE_LEFT_DOUBLE`/`RIGHT_DOUBLE` 4/8, release `MOUSE_LEFT_UP`/`RIGHT_UP` 16/32. |
| `void gfx_sound(int channel, int freq, int frames, int wave, int volume)`               | Play a tone on channel 0-3; freq 0 stops it, frames 0 loops. `WAVE_SQUARE/NOISE/TRIANGLE`.                                                                 |
| `void gfx_tilemap(unsigned char *map, int map_w, int map_h, int cam_x, int cam_y)`      | Fill the screen from a tile map (1 byte per tile = palette colour, 0 = black; 16x16 px tiles; camera in pixels; outside the map is black).                 |
| `int gfx_getchar(void)`                                                                 | Next typed ASCII character (8 backspace, 13 enter), 0 if none; consumes it.                                                                                |
| `void gfx_save(void)` / `void gfx_restore(void)`                                        | Store the back buffer as a static layer / redraw it in one call.                                                                                           |

Everything is clipped to the screen. Held keys (bitmask at `0x43004`): bit 0
left/A, 1 right/D, 2 up/W, 3 down/S, 4 Space, 5 Enter, 6 Tab. The second byte
(`0x43005`, `gfx_keys_ext()`, constants `KEYX_*`) holds bit 0 Q, 1 E, 2 X/Shift, 3 Z/Ctrl.

Speed register (`0x43003`): 0 = fixed speed (30 frames/s, 30000 instructions per frame), nonzero = maximum speed: the emulator drops the frame cap, skips most display refreshes and runs up to 300000 instructions per pass. Use it from C with `gfx_speed(1)` / `gfx_speed(0)`; time maximum-speed runs with the real-time clock (`gfx_rtc`), since frame ticks no longer match wall time. Page 9 of the showcase uses it to measure the SC-16 for 3 seconds.

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

| Function                                                               | Description                                                             |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `int net_open(char *title)`                                            | Join the group for `title`; 1 = ok.                                     |
| `void net_close(void)`                                                 | Leave the group.                                                        |
| `int net_send(unsigned char *data, int len)`                           | Broadcast 1..`NET_MAX_MSG` (48) bytes to all other instances; 1 = sent. |
| `int net_recv(unsigned char *buf)`                                     | Pop the next message into `buf`; returns its length, 0 if none.         |
| `int net_sender(void)`                                                 | Slot of the sender of the last received message.                        |
| `int net_players(void)`                                                | Live instances including this one.                                      |
| `int net_slot(void)`                                                   | Own slot 0-7, -1 when not joined.                                       |
| `int net_active(int slot)`                                             | 1 if a player occupies `slot`.                                          |
| `int net_ready(void)` / `int net_full(void)` / `int net_is_open(void)` | Slot settled / group had no free slot / joined.                         |

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
C globals start at 0x2000.

Run the showcase (9 pages; Left/Right or A/D change page, Enter runs the benchmark on page 8 and a quick speed measurement on page 9, which compares the SC-16 in MIPS with the Intel 4004, 8088, 6502, 68000, 486DX2-66, CRAY-1 and Raspberry Pi A):

    python tools/c_compiler.py examples/sc16_showcase.c -o showcase.bin
    python main.py showcase.bin

## Optional JIT acceleration

If [Numba](https://numba.pivot.org) is installed (`pip install numba`), the emulator runs the CPU core as compiled machine code, typically 5-15x faster than the pure-Python interpreter (the first start compiles and caches it, taking a few seconds). Without Numba the emulator falls back automatically to the pure-Python core. Set `SC16_NOJIT=1` to force the Python core. Both cores are verified identical by `tests/test_jit_core.py`.
