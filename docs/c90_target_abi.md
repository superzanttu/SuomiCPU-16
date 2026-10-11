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
| object pointer | 19-bit byte address, stored in 3 bytes |
| function pointer | 19-bit byte address, stored in 3 bytes |

Bytes are 8 bits. Multi-byte integer and floating representations are
big-endian. Integer signedness and ranges follow the representations above;
`long` therefore provides the minimum range required by C90. `float` and
`double` are separate types even though current compiler code aliases them.
The floating-point format decisions meet the C90 minimum precision and range
requirements.

Pointer representations use the low 16 address bits in the first two bytes and
the remaining three address bits in the third byte. Pointer arithmetic is
defined over the flat byte-addressed machine space but must still obey C90's
same-array object rules. The compiler must not silently truncate addresses or
offsets.

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
occupies two bytes. At function entry, the first named parameter begins at
`SP + 2`, followed by subsequent parameters in declaration order; locals are
allocated below the entry stack position.

16-bit integer results use `R0`; 32-bit integer results use `R0:R1`, with the
high word in the even register. Pointer results use `R0` for the low 16 address
bits and the low three bits of `R1` for the high address bits. Once implemented,
32-bit floating results use `R0:R1` and 64-bit floating results use
`R0:R1:R2:R3`, from most- to least-significant word. Arguments use their
in-memory object representations on the stack, including the 3-byte pointer
representation.

Structure and union parameters are passed as copies of their complete object
representations. Structure and union returns use a caller-provided result
address as an implicit first argument; this pointer is removed with the
ordinary arguments by the caller. This convention must remain reentrant and
must not use shared global scratch storage.

## Hosted execution environment

The hosted implementation uses the emulator's byte-addressable 512 KiB
address space. Device-mapped addresses retain their documented SC-16 behavior.
The implementation must define and test which C standard-library stream and
device facilities are available, error behavior for unavailable host resources,
and process termination/status behavior. Screen/graphics APIs remain
implementation extensions and do not replace required C90 library interfaces.

## Conformance gates

The current code does not yet satisfy this target. In particular, binary16 is
still used for all floating types, some pointer paths use 16-bit addressing,
function pointers and indirect calls are incomplete, aggregate values are not
passed or returned, and the hosted library is partial.

Do not remove the compiler's nonconformance disclaimer until all of the
following pass:

1. Compile-time and runtime ABI tests verify widths, ranges, endianness,
   alignment, aggregate offsets, pointer encoding, and all scalar/aggregate
   calling conventions.
2. Emulator, JIT, and assembler execute identical tests for every C-visible
   memory, control-flow, and floating-point opcode.
3. An independent C90 conformance suite passes, with each remaining failure
   categorized as a documented implementation-defined/unspecified behavior
   or fixed as a defect.
4. Every required hosted C90 header and library interface is present and has
   behavior and boundary tests.
