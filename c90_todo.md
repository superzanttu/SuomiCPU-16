# ANSI C90 Compatibility TODO

This roadmap divides the remaining work into three categories:

- **A — SuomiCPU-16 platform required:** target ABI, architecture, ISA, and
  execution-engine work needed to make the C implementation correct on
  SuomiCPU-16.
- **B — Basic C game programming:** compiler and runtime work that directly
  helps users write and run practical, ordinary C games on this platform.
- **C — Other C90 work:** remaining language completeness, hosted-standard
  library coverage, limits, independent conformance testing, and release
  certification.

These categories prioritize and explain the tasks; **all three are needed for
full C90 compatibility**. Tasks may depend on items in another category.

**The compiler is not currently a fully conforming C90 implementation.** Keep
that disclaimer in [`user_guide.md`](user_guide.md) until the release gates at
the end of this document pass. The ABI target is recorded in
[`docs/c90_target_abi.md`](docs/c90_target_abi.md); it contains intended
choices as well as existing behavior, so verify implementation before treating
an ABI choice as delivered.

## Current baseline

- The compiler implements a substantial C90-oriented subset, but the user
  guide and ABI specification explicitly state that it is not yet conforming.
- Existing implementation and regression coverage include 32-bit `long`,
  30-bit object-pointer accesses, aggregate assignment and by-value argument
  copies, caller-owned structure/union returns, and supported one-level
  `const` pointer-target qualification.
- The emulator provides 1 GiB of byte-addressable memory, and far control flow,
  return addresses, stack-pointer operations, and pointer accesses support
  30-bit addresses. Compiler global/static data occupies
  `0x80000..0xFFFFF`; `LDI30` loads full addresses. Regression coverage
  exercises the C90 minimum 65,535-byte object size, but this does not establish
  all C90 translation limits.
- `float`, `double`, and `long double` still use binary16; `double` therefore
  does not satisfy C90 minimum range and precision requirements.
- Function pointers and indirect calls, bit-fields, full linkage/storage
  semantics, complete hosted standard-library facilities, and independent
  conformance validation remain incomplete.
- Repository regression tests do not substitute for an independent C90
  conformance suite.

## A — SuomiCPU-16 platform required

These tasks define and verify the target-specific representations and
execution behavior. Add machine instructions only when existing instructions
or correct runtime helpers cannot meet the ABI requirements.

### A0. Implement the 30-bit control-flow ABI

- [x] **Implement full-address calls, returns, and compact far control**
  - Store return PCs as four big-endian bytes; `RET` and `RTI` restore all
    30 address bits.
  - Set the first function parameter at `SP + 4` and update assembly-library
    argument offsets to match.
  - Encode direct far control and conditional branches in six bytes, and add
    register-pair indirect `JMPRP` / `CALLRP` for full 30-bit addresses.
  - Add `LDI30` (with `LDI19`/`LDA` compatibility aliases) so assembler and
    compiler-generated addresses preserve all 30 bits; verify assembler,
    emulator, JIT, ABI offsets, and high addresses.

### A1. Define C90 floating-point representations

- [ ] **Define C90 floating-point representations** (`c90-float-model`)
  - [x] Select and document IEEE-754 binary32 `float`, binary64 `double` and
    `long double`, their sizes, byte order, alignment, literal conversion,
    rounding, subnormals, signed zero, infinities, NaNs, and exception policy
    in `docs/c90_target_abi.md`.
  - [ ] Implement this model consistently in compiler types, object layout,
    literal conversion, and the execution backend.
  - [ ] Update `float.h` limits and type sizes only with the implementation
    that makes those limits true. Until then, the existing header continues to
    report the measured binary16 behavior rather than advertising the future
    ABI.
  - [ ] Add compile-time and runtime boundary tests; claim C90 minimum range
    and precision only when verified.

### A2. Provide required floating-point machine operations

- [ ] **Provide required floating-point machine operations** (`c90-float-isa`)
  - Compare binary32/binary64 needs with the existing ISA, runtime helpers,
    emulator, JIT, and assembler.
  - Reuse instructions or helpers where they correctly implement the
    semantics; add opcodes only when necessary.
  - Cover arithmetic, comparison, conversion, rounding, and required
    status/exception behavior.
  - Keep encoding, assembler output, emulator, and JIT behavior consistent.
  - **Depends on:** A1 (`c90-float-model`).

### A3. Validate floating-point execution and ABI

- [ ] **Validate floating-point ABI behavior** (`c90-float-validation`)
  - Test exponent and precision boundaries, subnormals, signed zero,
    infinities, NaNs, rounding, casts, constants, calls, and returns.
  - Compare emulator and JIT results with an independent reference.
  - **Depends on:** B1 (`c90-float-backend`).

### A4. Validate the complete SuomiCPU-16 C ABI

- [ ] **Validate the complete target ABI** (`c90-abi-conformance`)
  - Verify widths, ranges, representations, endianness, alignment, aggregate
    layout, object/function pointers, conversions, stack frames, and scalar
    and aggregate calling conventions.
  - Exercise relevant C-visible memory, control-flow, and floating-point
    behavior across assembler, emulator, and JIT.
  - Update `docs/c90_target_abi.md` only from verified behavior.
  - **Depends on:** A3, B2 (`c90-indirect-calls`), C1
    (`c90-pointer-qualifiers`), C3 (`c90-bitfield-codegen`), B4
    (`c90-linkage-storage`), and B5 (`c90-initializers`).

## B — Basic C game programming

These tasks prioritize practical programs such as small games: ordinary
functions and declarations, numeric calculations, mutable game state,
initialization, and useful runtime facilities. They do not replace the full
C90 requirements listed under C.

### B1. Implement C90 floating-point compiler support

- [ ] **Implement floating-point code generation** (`c90-float-backend`)
  - Represent `float`, `double`, and `long double` distinctly in parsing,
    type checking, constant evaluation, and code generation.
  - Implement literals, constants, conversions, usual arithmetic conversions,
    arithmetic, comparisons, assignments, and conditional expressions.
  - Implement arguments, returns, variadic promotions, and `sizeof` according
    to the target ABI.
  - **Depends on:** A1 (`c90-float-model`) and A2 (`c90-float-isa`).

### B2. Parse and call through function pointers

- [ ] **Parse C90 function-pointer declarators**
  (`c90-function-pointer-types`)
  - Implement nested and abstract declarators for function types and function
    pointers, including typedef names, arrays of pointers, valid pointers to
    arrays, and functions returning pointers.
  - Preserve prototypes, variadic status, and type compatibility rather than
    flattening semantically distinct types.
  - Add parser, diagnostic, and compatibility tests.

- [ ] **Implement function-pointer conversions and calls**
  (`c90-indirect-calls`)
  - Implement function-to-pointer conversion, address-of functions, pointer
    assignment/conversions, equality, function-pointer parameters and
    returns, and indirect calls.
  - Define and test call ABI, stack layout, argument order, results, and
    register clobbers.
  - Diagnose invalid conversions between object and function pointers.
  - **Depends on:** B2a (`c90-function-pointer-types`). Verify the completed
    ABI in A4 (`c90-abi-conformance`).

### B3. Complete declarations needed for real programs

- [ ] **Complete C90 declarations** (`c90-declarators`, existing tracker item)
  - Close gaps in nested and abstract declarators, array/function parameter
    adjustment, compatible declarations/definitions, and old-style parameter
    metadata.
  - Preserve supported structure/union assignment, by-value argument copies,
    and caller-owned returns.
  - Diagnose unsupported or invalid forms rather than silently miscompiling.
  - **Depends on:** existing `c90-type-abi` work and B2a.

### B4. Implement linkage and storage duration

- [ ] **Implement C90 linkage and storage duration** (`c90-linkage-storage`)
  - Implement or explicitly diagnose `static`, `extern`, `auto`, `register`,
    and `volatile` at file and block scope.
  - Cover internal/external/no linkage, tentative definitions, compatible
    declarations, multiple definitions, and static local lifetime and
    initialization.
  - Enforce register address-taking rules and define volatile accesses,
    including memory-mapped device behavior.

### B5. Complete initializers for game state and data

- [ ] **Complete C90 initializer semantics** (`c90-initializers`)
  - Implement scalar, array, structure, and union initialization, including
    brace elision, nested aggregates, string-literal arrays, omitted outer
    bounds, and zero filling.
  - Enforce automatic versus static initialization and static
    constant-expression rules.
  - Reject designated-initializer extensions unless deliberately documented.
  - **Depends on:** B4 (`c90-linkage-storage`).

### B6. Provide a practical game-programming runtime subset

- [ ] **Define and test the SC-16 game C runtime subset**
  (`sc16-game-c-runtime`)
  - Identify the standard C routines most useful to ordinary game programs,
    such as math, string/memory, random-number, and allocation facilities;
    record which are already usable and which are missing.
  - Specify and test interaction with existing SC-16 graphics, screen, input,
    and timing extensions without presenting those extensions as standard
    C90 interfaces.
  - Document limitations and failure behavior. This subset is a practical
    milestone, not a substitute for Category C's complete hosted library.
  - **Depends on:** B3 (`c90-declarators`) and B5
    (`c90-initializers`).

## C — Other C90 compatibility work

These tasks close the remaining language and hosted-environment requirements
that go beyond the practical game-programming milestone. They are still
required before claiming full C90 compatibility.

### C1. Complete pointer qualifiers

- [ ] **Complete C90 pointer qualifier semantics** (`c90-pointer-qualifiers`)
  - Track `const` and `volatile` at every pointer level through declarations,
    typedefs, casts, conditional/comma expressions, members, prototypes,
    arguments, and returns.
  - Distinguish a const-qualified pointer object from a pointer to const data.
  - Reject qualifier loss and writes through const-qualified lvalues.
  - Add positive/negative tests for multi-level pointers and all expression
    and declaration paths.
  - **Depends on:** B2a (`c90-function-pointer-types`).

### C2. Define bit-field layout

- [ ] **Define C90 bit-field allocation rules** (`c90-bitfield-layout`)
  - Specify allocation-unit size, bit order, signedness, alignment, packing,
    boundary crossing, unnamed fields, and zero-width fields.
  - Define interactions with `sizeof`, unions, record padding, and
    `offsetof` constraints.
  - Document implementation-defined choices in `docs/c90_target_abi.md` and
    add layout tests.

### C3. Implement bit-field accesses

- [ ] **Implement C90 bit-field accesses** (`c90-bitfield-codegen`)
  - Parse and validate fields and allocate them using C2's ABI rules.
  - Generate signed/unsigned loads and read/modify/write stores.
  - Cover assignment, compound assignment, increment/decrement, promotions,
    initialization, unions, and volatile fields.
  - Diagnose invalid base types and widths.
  - **Depends on:** C2 (`c90-bitfield-layout`).

### C4. Complete preprocessing semantics

- [ ] **Complete C90 preprocessing semantics** (`c90-preprocessor`)
  - Audit preprocessing translation phases and directives against C90.
  - Implement macro argument prescan, stringification, token pasting,
    rescanning, and `defined` behavior.
  - Verify conditional-expression integer evaluation, include behavior,
    diagnostics, line mapping, and trigraph/splice ordering.
  - Add tests independent of code generation.

### C5. Enforce C90 translation limits

- [ ] **Enforce C90 translation limits and constraints**
  (`c90-translation-limits`)
  - Audit limits for identifiers, nested blocks/declarations, parameters and
    arguments, macro/include nesting, expressions, strings, records, and
    source lines.
  - Support standard-mandated minimums or produce clear diagnostics beyond
    target/resource limits.
  - Test at and immediately beyond each required minimum.

### C6. Implement all required C90 headers

- [ ] **Implement all required C90 headers** (`c90-required-headers`)
  - Partial implementation now includes the console-only `<stdio.h>` interface
    and a `printf` integer/text formatter; C90 `gets` is present, but floating
    conversions and file-backed streams remain outstanding. `<setjmp.h>` now
    provides `jmp_buf`, `setjmp`, and `longjmp`; tests cover restored control
    flow, zero-value conversion, and unwinding a nested call. `<signal.h>`
    remains missing because signal-handler dispatch depends on function-pointer
    support. The exposed `stdio.h` limits now meet the C90 minimum values for
    `BUFSIZ`, `FOPEN_MAX`, and `TMP_MAX`; this does not imply file-backed stream
    support, which remains outstanding.
  - `<math.h>` now declares the C90 math interface and links software
    approximations. These use the target's binary16 arithmetic and do not meet
    C90 `double` range, precision, or exceptional-value requirements.
  - The emulator now supplies a text console, BIOS device descriptor and
    startup data-disk selection. This does not load programs from disk:
    programs selected by the BIOS still come from `modules/`.
  - Audit all required headers: `assert.h`, `ctype.h`, `errno.h`, `float.h`,
    `limits.h`, `locale.h`, `math.h`, `setjmp.h`, `signal.h`, `stdarg.h`,
    `stddef.h`, `stdio.h`, `stdlib.h`, `string.h`, and `time.h`.
  - Add missing required declarations, macros, and types.
  - Verify standalone use, repeated inclusion where permitted, and varied
    combinations/order.

### C7. Define hosted execution behavior

- [ ] **Define C90 hosted execution behavior** (`c90-hosted-runtime`)
  - Specify standard stream/device mapping, file operations or explicit
    capability policy, buffering, EOF/error indicators, environment and
    command behavior, and process termination/status.
  - Surface unavailable resources and errors through documented C interfaces.
  - Add tests for available and unavailable services.
  - **Depends on:** C6 (`c90-required-headers`).

### C8. Complete the required C90 library

- [ ] **Complete required C90 library functions** (`c90-library-functions`)
  - Implement required functions and macros from C90 headers, including
    remaining math, stdio, signal, stdlib, string, and time
    facilities. `strerror` now has a message for every errno macro defined by
    this implementation, with runtime coverage for each mapping and the
    unknown-error fallback.
  - The current `<math.h>` implementation is only an approximate target
    subset; validate all domains, ranges, errors, and accuracy after the
    floating-point ABI is upgraded.
  - Specify boundary behavior, errors, `errno` effects, and hosted-resource
    limitations.
  - Keep SC-16 screen/graphics routines documented as extensions, not
    replacements for standard interfaces.
  - **Depends on:** C6 (`c90-required-headers`) and C7
    (`c90-hosted-runtime`).

### C9. Integrate an independent conformance suite

- [ ] **Integrate an independent C90 conformance suite**
  (`c90-independent-suite`)
  - Select a suite and review its license and harness requirements.
  - Add a repeatable runner for SC-16 and preserve categorized output.
  - Keep the suite independent of compiler implementation internals.

### C10. Run conformance tests and resolve findings

- [ ] **Validate C90 conformance** (`c90-conformance-tests`)
  - Exercise the language, ABI, preprocessing, translation limits, and hosted
    library with the independent suite.
  - Classify failures as implementation defects, documented
    implementation-defined/unspecified behavior, unsupported hosted
    facilities, or invalid test assumptions.
  - Fix defects, add regressions, and publish repeatable emulator/JIT results.
  - **Depends on:** A4 (`c90-abi-conformance`), B3
    (`c90-declarators`), C4 (`c90-preprocessor`), C5
    (`c90-translation-limits`), C8 (`c90-library-functions`), and C9
    (`c90-independent-suite`).

### C11. Close compatibility release gates

- [ ] **Close C90 compatibility release gates** (`c90-release-gate`)
  - Run repository tests and the independent conformance suite.
  - Verify ABI and assembler/emulator/JIT parity, translation limits,
    required headers/library behavior, and documentation.
  - Remove the nonconformance disclaimer only if the C90 requirements are
    satisfied and independent evidence supports the claim.
  - **Depends on:** A4, C5, C8, and C10.

## Completion criteria

Do not describe the compiler as fully C90-compatible until all of these are
true:

1. Required language constraints, declarators, conversions, initialization,
   preprocessing, and translation limits are implemented and tested.
2. The data model meets C90 minimum ranges and precision; scalar, pointer,
   aggregate, and function-call ABI behavior is verified.
3. Required hosted headers and library facilities have documented and tested
   behavior, including errors and unavailable resources.
4. Assembler, emulator, and JIT agree on generated programs and C-visible
   machine behavior.
5. An independent C90 conformance suite has been run; failures are resolved
   or correctly classified, and implementation-defined/unspecified behavior
   is documented.
6. The user guide, ABI document, and this checklist accurately describe the
   implementation and the evidence supporting any conformance claim.
