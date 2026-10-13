# SC-16 C90 Target ABI

This document records the target ABI decisions for completing hosted ANSI C90
support. It is a specification target, not a claim that the current compiler
implements every item. The compiler remains nonconforming until the
implementation and independent conformance tests meet the acceptance criteria
below.

## Data model

| C type | Target representation |
| --- | --- |
| `char`, `signed char`, `unsigned char` | 8 bits; plain `char` is signed |
| `short`, `unsigned short` | 16 bits |
| `int`, `unsigned int` | 16 bits |
| `long`, `unsigned long` | 32 bits |
| `float` | IEEE-754 binary32 |
| `double` | IEEE-754 binary64 |
| `long double` | IEEE-754 binary64 |
| object pointer | 30-bit byte address, stored in 4 bytes |
| function pointer | 30-bit byte address, stored in 4 bytes |

Bytes are 8 bits. Multi-byte integer and floating representations are
big-endian. Integer signedness and ranges follow the representations above;
`long` therefore provides the minimum range required by C90. `float` and
`double` are separate types even though current compiler code aliases them.
The floating-point format decisions meet the C90 minimum precision and range
requirements.

Pointer objects store the low 16 address bits in the first two bytes, followed
by the high 14 bits in the next two bytes; each word is big-endian. LDP/STP
access this representation. Pointer arithmetic is defined over the flat
byte-addressed machine space but must still obey C90's same-array object rules.
The compiler must not silently truncate addresses or offsets. Explicit
pointer-to-integer conversions preserve the complete address in a 32-bit `long`
and the low 16 bits in a 16-bit `int`. Explicit integer-to-pointer conversions
use the low 30 bits. These conversion choices are implementation-defined.

## Alignment and aggregates

The machine is byte-addressable and permits unaligned loads and stores, so this
ABI imposes no minimum address alignment on objects. Arrays have contiguous
row-major storage with no padding between elements.

For compatibility with the current layout, a non-union structure member whose
size exceeds one byte starts at the next even offset relative to the beginning
of the structure. Other members use the current offset. The size of every
structure or union is rounded up to an even number of bytes. Union members
start at offset zero. These are implementation-defined padding choices, not
hardware alignment requirements, and must be reflected identically by
`sizeof`, `offsetof`, initializer emission, address calculations, and runtime
accesses.

There is no bit-field allocation-unit rule yet; before claiming conformance,
the compiler must implement C90 bit-field declarations and document allocation
order, unit size, and boundary behavior as implementation-defined choices.

## Function calls

Arguments are evaluated and pushed right-to-left. The stack grows toward
lower addresses. The caller removes arguments after return. The return address
occupies four bytes and stores the 30-bit PC in big-endian order. At function
entry, the first named parameter begins at `SP + 4`, followed by subsequent
parameters in declaration order; locals are allocated below the entry stack
position.

16-bit integer results use `R0`; 32-bit integer results use `R0:R1`, with the
high word in the even register. Pointer results use `R0` as a numeric 30-bit
address. Indirect function-pointer calls and jumps split this address between
`CALLRP Rlow, Rhigh` and `JMPRP Rlow, Rhigh`: the low 16 bits are in `Rlow` and
the high 14 bits in `Rhigh`. Once implemented,
32-bit floating results use `R0:R1` and 64-bit floating results use
`R0:R1:R2:R3`, from most- to least-significant word. Arguments use their
in-memory object representations on the stack, including the 4-byte pointer
representation.

Structure and union parameters are passed as copies of their complete object
representations; this caller-side copy convention is implemented for fixed
parameters. Structure and union assignment also copies complete object
representations. Structure and union returns use a caller-provided result
address as an implicit first argument; this pointer is removed with the
ordinary arguments by the caller. The compiler reserves per-call result
temporaries in the caller's stack frame, so recursive calls do not share
scratch storage. Runtime regressions cover nested return/argument expressions
and recursive returns.

## Hosted execution environment

The hosted implementation uses the emulator's byte-addressable 1 GiB
address space. Startup firmware exposes a descriptor at `0x43040`, an
interactive data-disk selector, and a memory-mapped console at `0x43200`.
Selected modules are still loaded into memory by the emulator; virtual disks
store program data only. The console currently supports a subset of standard
text streams and formatted output. File-backed streams, floating-point format
conversions, and several required C90 I/O facilities remain unimplemented.
Device-mapped addresses retain their documented SC-16 behavior. Compiler
global/static data is currently placed in `0x80000..0xFFFFF`, providing 512 KiB
of storage; generated addresses use `LDI30` so this region is not truncated to
16 bits. The BIOS descriptor at `0x43040` is 20 bytes: it stores the memory
size in bytes `6..9`, the console data address in `10..12`, the disk-control
address in `13..15`, and startup status at byte `19`. This allocation does not
reserve additional runtime heap or imply that every C90 translation-limit
requirement has been met. Screen and graphics APIs remain implementation
extensions and do not replace required C90 library interfaces.

## Conformance gates

The current code does not yet satisfy this target. In particular, binary16 is
still used for all floating types, function pointers and indirect calls are
incomplete, and the hosted library is partial. Aggregate assignment, fixed-
parameter copies, and caller-owned record returns are implemented, but other
declarator and ABI requirements remain incomplete. Regression tests now cover
30-bit pointer encodings, explicit 32-bit-integer-to-pointer casts, high-address
byte/word/long accesses, and indexed accesses crossing `0xFFFF`; these cases do
not establish every pointer conversion, arithmetic, alignment, or wraparound
behavior.

Do not remove the compiler's nonconformance disclaimer until all of the
following pass:

1. Compile-time and runtime ABI tests verify widths, ranges, endianness,
   alignment, aggregate offsets, pointer encoding, and all scalar/aggregate
   calling conventions.
2. Emulator, JIT, and assembler execute identical tests for every C-visible
   memory, control-flow, and floating-point opcode. Calls and returns,
   including indirect calls, preserve all 30 PC bits; interrupt frames contain
   the four-byte PC and two-byte flags value.
3. An independent C90 conformance suite passes, with each remaining failure
   categorized as a documented implementation-defined/unspecified behavior
   or fixed as a defect.
4. Every required hosted C90 header and library interface is present and has
   behavior and boundary tests.
