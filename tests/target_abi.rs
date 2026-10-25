use suomicpu16::{Cpu, assembler::assemble, c_compiler::compile_minimal_program_to_assembly};

fn run_c(source: &str) -> Cpu {
    let assembly = compile_minimal_program_to_assembly(source, "target-abi.c")
        .expect("parse C")
        .expect("compile C");
    let image = assemble(&assembly, 0).expect("assemble generated code");
    let mut cpu = Cpu::with_memory_size(1 << 20).expect("allocate CPU");
    cpu.load_program(&image.segments, image.entry_point)
        .expect("load program");
    let initial_sp = cpu.sp;
    cpu.run(1_000_000).expect("execute program");
    assert!(!cpu.running, "program exceeded instruction budget");
    assert_eq!(cpu.sp, initial_sp, "callers must restore the stack");
    cpu
}

#[test]
fn wide_integer_arithmetic_calls_and_returns_preserve_both_words() {
    let cpu = run_c(
        "long observed; unsigned long bits=305419896UL; \
         long combine(long a, unsigned int b, long c) { return a+b+c; } \
         long recur(int n, long value) { if (!n) return value; \
         return recur(n-1,value+65537L); } \
         int main(void) { long local=-70000L; \
         observed=combine(local,65535U,131072L); \
         if (observed != 126607L) return 1; \
         if (recur(4,65536L) != 327684L) return 2; \
         local*=3; local/=7; local%=1000; \
         if (local != -0L) return 3; \
         bits^=4294901760UL; bits&=16777215UL; bits|=16777216UL; \
         if (bits != 0x01CB5678UL) return 4; \
         return sizeof(65536L) == 4 && sizeof(bits+1) == 4 ? 0 : 5; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn wide_integer_conditions_comparisons_and_shift_signedness() {
    let cpu = run_c(
        "int main(void) { long negative=-65536L; unsigned long high=2147483648UL; \
         long zero=0L; \
         if (!high || !negative || zero || !(high && negative) || (zero && high)) return 1; \
         if (negative >= 0L || negative == 0 || negative != -65536L) return 2; \
         if (!(negative < 65535U) || !(high > 2147483647L)) return 3; \
         if (negative >> 16 != -1L || high >> 31 != 1UL) return 4; \
         if ((1UL << 31) != high || (~0UL) != 4294967295UL) return 5; \
         if ((negative+3L) >= (negative+4L)) return 6; \
         return (high ? negative : 1L) == -65536L ? 0 : 7; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn wide_updates_and_word_call_conversions_preserve_signedness() {
    let cpu = run_c(
        "unsigned long global=4294967295UL; \
         unsigned int word(void) { return 65535U; } \
         long widen(long value) { return value; } \
         int main(void) { long local=65535L; long old=local++; \
         if (old != 65535L || local != 65536L) return 1; \
         if (++local != 65537L || local-- != 65537L || --local != 65535L) return 2; \
         if (global++ != 4294967295UL || global != 0UL) return 3; \
         global--; if (global != 4294967295UL) return 4; \
         if (widen(word()) != 65535L || (long)word() != 65535L) return 5; \
         local+=0.75; if (local != 65535L) return 6; \
         return (int)(global=65537UL) == 1 ? 0 : 7; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn wide_integer_object_results_match_independent_rust_arithmetic() {
    for (left, right) in [
        (0u32, 1u32),
        (u32::MAX, 65_537),
        (0x8000_0000, 31),
        (0x1234_5678, 0x1020_3040),
    ] {
        for (operator, expected) in [
            ("+", left.wrapping_add(right)),
            ("-", left.wrapping_sub(right)),
            ("*", left.wrapping_mul(right)),
            ("/", left / right),
            ("%", left % right),
            ("&", left & right),
            ("|", left | right),
            ("^", left ^ right),
        ] {
            let mut cpu = run_c(&format!(
                "unsigned long observed; int main(void) {{ \
                 unsigned long a={left}UL; unsigned long b={right}UL; \
                 observed=a{operator}b; return 0; }}"
            ));
            let bytes = (0..4)
                .map(|offset| cpu.read(0x80000 + offset))
                .collect::<Vec<_>>();
            assert_eq!(bytes, expected.to_be_bytes(), "{left} {operator} {right}");
        }
    }
    for (expression, expected) in [
        ("-2147483647L/17L", (-2_147_483_647i32 / 17) as u32),
        ("-2147483647L%17L", (-2_147_483_647i32 % 17) as u32),
        ("(long)(int)65535U", u32::MAX),
        ("(long)(unsigned int)-1", 65_535),
        ("(long)(signed char)255", u32::MAX),
        ("(long)(unsigned char)-1", 255),
    ] {
        let mut cpu = run_c(&format!(
            "unsigned long observed; int main(void) {{ observed={expression}; return 0; }}"
        ));
        let bytes = (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected.to_be_bytes(), "{expression}");
    }
}

#[test]
fn wide_comparisons_and_calls_preserve_live_word_and_pointer_values() {
    let cpu = run_c(
        "long identity(long x) { return x; } \
         int compare(int *pointer, int sentinel, long value) { \
         if (value < 0L && identity(value) != 0L) *pointer=sentinel; \
         return sentinel; } \
         int main(void) { int result[1]; result[0]=0; \
         if (compare(result,12345,-65536L) != 12345) return 1; \
         return result[0] == 12345 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn widening_keeps_word_expression_side_effects_and_promoted_signedness() {
    let cpu = run_c(
        "long identity(long value) { return value; } \
         int main(void) { int word=5; unsigned int unsigned_word=65535U; \
         unsigned int values[1]; values[0]=65535U; \
         long result=(long)(word=17); \
         if (result != 17L || word != 17) return 1; \
         result=(long)word++; if (result != 17L || word != 18) return 2; \
         result=(long)++word; if (result != 19L) return 3; \
         result=identity(word ? -1 : unsigned_word); \
         if (result != 65535L) return 4; \
         result=identity(values[0]); return result == 65535L ? 0 : 5; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn expression_and_call_register_saves_do_not_undo_parameter_or_local_writes() {
    let cpu = run_c(
        "long previous(int value) { long result=(long)value++; \
         return value == 18 ? result : -1L; } \
         float single(float value) { return value; } \
         int word_identity(int value) { return value; } \
         int parameter_address(int value) { int *pointer=&value; *pointer=21; return value; } \
         int main(void) { int word=5; float rounded=(float)(word=13); \
         if (word != 13 || rounded != 13.0f) return 1; \
         rounded=single((float)word++); if (rounded != 13.0f || word != 14) return 2; \
         if (word_identity(word=23) != 23 || word != 23) return 3; \
         if (previous(17) != 17L || parameter_address(9) != 21) return 4; \
         return 0; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn wide_integer_pointer_casts_preserve_all_thirty_address_bits() {
    let mut cpu = run_c(
        "unsigned long observed; int main(void) { unsigned long address=589824UL; \
         int *pointer=(int *)address; int *next=(int *)(address+2UL); \
         *pointer=1234; *next=5678; \
         if ((long)pointer != 589824L || (unsigned long)next != 589826UL) return 1; \
         if (*pointer != 1234 || *next != 5678 || *((int *)(address+2UL)) != 5678) return 2; \
         pointer=(int *)4294967295UL; \
         observed=(unsigned long)pointer; \
         if ((unsigned long)pointer != 1073741823UL) return 3; \
         pointer=(int *)65536L; return (long)pointer == 65536L ? 0 : 4; }",
    );
    let bytes = (0..4)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, 0x3fff_ffffu32.to_be_bytes());
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn integer_to_floating_conversions_use_full_width_and_independent_ieee_results() {
    for (integer, expected) in [
        ("-2147483647L-1L", i32::MIN as f64),
        ("2147483647L", i32::MAX as f64),
        ("4294967295UL", u32::MAX as f64),
        ("16777217L", 16_777_217.0f64),
        ("-65537L", -65_537.0f64),
    ] {
        for (c_type, size) in [("float", 4), ("double", 8)] {
            let source = format!(
                "{c_type} observed; long signed_id(long x) {{ return x; }} \
                 unsigned long unsigned_id(unsigned long x) {{ return x; }} \
                 int main(void) {{ \
                 observed=({c_type})({integer}); return 0; }}"
            );
            let mut cpu = run_c(&source);
            let bytes = (0..size)
                .map(|offset| cpu.read(0x80000 + offset))
                .collect::<Vec<_>>();
            let expected = if size == 4 {
                (expected as f32).to_be_bytes().to_vec()
            } else {
                expected.to_be_bytes().to_vec()
            };
            assert_eq!(bytes, expected, "{c_type}: {integer}");
        }
    }
    let mut cpu = run_c(
        "double observed; long identity(long value) { return value; } \
         int main(void) { long local=65537L; \
         observed=identity(local*3L)-65536L; return 0; }",
    );
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, 131_075.0f64.to_be_bytes());
}

#[test]
fn floating_to_wide_integers_convert_initializers_assignments_arguments_and_returns() {
    for (expression, c_type, expected) in [
        ("-2147483648.0", "long", i32::MIN as u32),
        ("2147483647.75", "long", i32::MAX as u32),
        ("4294967295.0", "unsigned long", u32::MAX),
        ("65537.75f", "long", 65_537),
        ("-65537.75f", "long", (-65_537i32) as u32),
    ] {
        let source = format!(
            "{c_type} observed; {c_type} identity({c_type} x) {{ return x; }} \
             {c_type} converted(double x) {{ return x; }} \
             int main(void) {{ double input={expression}; \
             {c_type} local=input; observed=identity(input); \
             if (observed != local || converted(input) != local) return 1; \
             observed=({c_type})input; return 0; }}"
        );
        let mut cpu = run_c(&source);
        assert_eq!(cpu.registers[0], 0, "{expression}");
        let bytes = (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected.to_be_bytes(), "{expression}");
    }
}

#[test]
fn far_and_register_pair_calls_save_exact_big_endian_return_addresses() {
    for call in ["CALLX 0x12340", "CALLRP R0, R1"] {
        let image = assemble(
            &format!(
                ".address 0x92340\n{call}\nHALT\n\
                 .address 0x12340\nRET"
            ),
            0x92340,
        )
        .unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.registers[0] = 0x2340;
        cpu.registers[1] = 1;
        let initial_sp = cpu.sp;
        let return_pc = cpu.pc + if call.starts_with("CALLX") { 6 } else { 4 };
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x12340);
        assert_eq!(cpu.sp, initial_sp - 4);
        let saved_pc = (0..4)
            .map(|offset| cpu.read(cpu.sp + offset))
            .collect::<Vec<_>>();
        assert_eq!(saved_pc, return_pc.to_be_bytes());
        cpu.step().unwrap();
        assert_eq!(cpu.pc, return_pc);
        assert_eq!(cpu.sp, initial_sp);
        cpu.step().unwrap();
        assert!(!cpu.running);
    }
}

#[test]
fn full_width_address_aliases_and_register_pair_jump_preserve_high_bits() {
    for mnemonic in ["LDI30", "LDI19", "LDA"] {
        let image = assemble(&format!("{mnemonic} R2, 0x3F512345\nHALT"), 0).unwrap();
        let mut cpu = Cpu::new();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(8).unwrap();
        assert_eq!(cpu.registers[2], 0x3f512345);
    }
    let image = assemble(
        "LDI R0, 0x2340\nLDI R1, 9\nJMPRP R0, R1\n\
         .address 0x92340\nHALT",
        0,
    )
    .unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.run(8).unwrap();
    assert!(!cpu.running);
    assert_eq!(cpu.pc, 0x92342);
}

#[test]
fn scalar_call_baseline_preserves_nested_frames_and_stack() {
    let cpu = run_c(
        "int sum(int a, int b, int c, int d, int e) { return a+b+c+d+e; } \
         int recur(int n) { if (n == 0) return 0; \
         return sum(n, -2, 3, -4, 5) + recur(n-1); } \
         int main(void) { return recur(5); }",
    );
    assert_eq!(cpu.registers[0], 25);
}

#[test]
fn mixed_byte_parameters_use_compact_object_sizes_at_every_arity() {
    for source in [
        "int f(signed char a, unsigned char b) { return a+b; } \
         int main(void) { return f(-3,250) == 247 ? 0 : 1; }",
        "int f(signed char a, int b, unsigned char c) { return a+b+c; } \
         int main(void) { return f(-3,-700,250) == -453 ? 0 : 1; }",
        "int f(signed char a, unsigned char b, signed char c, \
         unsigned char d, int e) { return a+b+c+d+e; } \
         int main(void) { return f(-3,250,-4,249,-700) == -208 ? 0 : 1; }",
    ] {
        assert_eq!(run_c(source).registers[0], 0, "{source}");
    }
}

#[test]
fn floating_parameters_use_exact_stack_object_representations() {
    let source = "double inspect(signed char a, float b, double c, \
                  unsigned char d, int e) { return c; } \
                  int main(void) { inspect(-3,1.5f,2.5,250,4660); return 0; }";
    let assembly = compile_minimal_program_to_assembly(source, "stack-bytes.c")
        .unwrap()
        .unwrap()
        .replace(
            "__c_function_inspect:\n",
            "__c_function_inspect:\n    HALT\n",
        );
    let image = assemble(&assembly, 0).unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.run(10_000).unwrap();
    assert!(!cpu.running);
    let bytes = (4..20)
        .map(|offset| cpu.read(cpu.sp + offset))
        .collect::<Vec<_>>();
    assert_eq!(
        bytes,
        [
            0xfd, 0x3f, 0xc0, 0x00, 0x00, 0x40, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfa,
            0x12, 0x34,
        ]
    );
}

#[test]
fn mixed_floating_calls_preserve_values_and_recursive_frames() {
    for source in [
        "float sum(float a, float b) { return a+b; } \
         int main(void) { return sum(sum(1.25f,2.5f),sum(3.0f,4.0f)) \
         == 10.75f ? 0 : 1; }",
        "double mix(signed char a, float b, double c, unsigned char d, int e) \
         { return a+b+c+d+e; } \
         int main(void) { return mix(-3,1.5f,2.5,250,-7) == 244.0 ? 0 : 1; }",
        "double recurse(int n, double value) { if (n == 0) return value; \
         return recurse(n-1,value+0.5); } \
         int main(void) { return recurse(5,1.25) == 3.75 ? 0 : 1; }",
        "double load(double *p, float a, int b, double c) { return *p+a+b+c; } \
         int main(void) { double value=2.0; \
         return load(&value,1.5f,-3,4.25) == 4.75 ? 0 : 1; }",
    ] {
        assert_eq!(run_c(source).registers[0], 0, "{source}");
    }
}

#[test]
fn prototype_and_return_conversions_match_independent_ieee_results() {
    for (argument, expected) in [
        ("16777217.0", 16_777_216.0f32),
        ("1.000000059604644775390625", 1.0f32),
        ("-0.0", -0.0f32),
        ("1.401298464324817e-45", f32::from_bits(1)),
    ] {
        let source = format!(
            "float observed; \
             float narrow(double value) {{ return value; }} \
             int main(void) {{ observed=narrow({argument}); return 0; }}"
        );
        let mut cpu = run_c(&source);
        let bytes = (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected.to_be_bytes(), "{argument}");
    }
    let mut cpu = run_c(
        "double observed; double widen(float value) { return value; } \
         int main(void) { observed=widen(0.1f); return 0; }",
    );
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, (0.1f32 as f64).to_be_bytes());
    for source in [
        "float identity(float value) { return value; } \
         int main(void) { return identity(3/2) == 1.0f ? 0 : 1; }",
        "float converted(int value) { return value/2; } \
         int main(void) { return converted(-3) == -1.0f ? 0 : 1; }",
        "double converted(int value) { return value/2; } \
         int main(void) { return converted(-3) == -1.0 ? 0 : 1; }",
    ] {
        assert_eq!(run_c(source).registers[0], 0, "{source}");
    }
}

#[test]
fn wide_integer_arguments_preserve_all_object_bytes_and_unsigned_values() {
    let mut cpu = run_c(
        "double observed; \
         double combine(long a, unsigned long b, signed char c) \
         { return (double)a+(double)b+c; } \
         int main(void) { observed=combine(-65536L,4294967295UL,-3); return 0; }",
    );
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, (-65536.0 + u32::MAX as f64 - 3.0).to_be_bytes());
    let cpu = run_c(
        "int narrow(long value) { return (int)value; } \
         int main(void) { return narrow(0x123456L) == 0x3456 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn binary64_compound_parameter_updates_preserve_frame_pointer_and_mantissa() {
    let mut cpu = run_c(
        "double observed; \
         double scale(double value) { int sentinel=1234; value+=0.1; \
         if (sentinel != 1234) return -1.0; return value*2.0; } \
         int main(void) { double local=0.2; double *pointer=&local; \
         observed=scale(*pointer); return *pointer == 0.2 ? 0 : 1; }",
    );
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, ((0.2f64 + 0.1f64) * 2.0f64).to_be_bytes());
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn large_argument_lists_preserve_offsets_and_split_caller_cleanup() {
    let parameters = (0..70)
        .map(|index| format!("int a{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let arguments = (0..70)
        .map(|index| if index % 2 == 0 { index } else { -index }.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "int f({parameters}) {{ return a0+a2+a35+a69; }} \
         int main(void) {{ return f({arguments}) == -102 ? 0 : 1; }}"
    );
    assert_eq!(run_c(&source).registers[0], 0);

    let parameters = (0..20)
        .map(|index| format!("double a{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let arguments = (0..20).map(|_| "0.1").collect::<Vec<_>>().join(",");
    let source = format!(
        "double observed; double f({parameters}) {{ return a0+a19; }} \
         int main(void) {{ observed=f({arguments}); return 0; }}"
    );
    let mut cpu = run_c(&source);
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, (0.1f64 + 0.1f64).to_be_bytes());
}

#[test]
fn incompatible_function_types_are_diagnosed_before_call_lowering() {
    for declarations in [
        "float f(float a); float f(double a) { return a; }",
        "int f(char a); int f(int a) { return a; }",
        "float f(float a); double f(float a) { return a; }",
        "int f(int *a); int f(char *a) { return *a; }",
    ] {
        let source = format!("{declarations} int main(void) {{ return 0; }}");
        let error = compile_minimal_program_to_assembly(&source, "signature.c").unwrap_err();
        assert!(
            error.0.contains("conflicting function type for f"),
            "{error}"
        );
    }
    let cpu = run_c(
        "int f(int); int f(const int value) { return value; } \
         int main(void) { return f(-7); }",
    );
    assert_eq!(cpu.registers[0], -7);
}

#[test]
fn bitfields_preserve_allocation_units_neighbors_and_big_endian_bytes() {
    let mut cpu = run_c(
        "struct Flags { unsigned int low:3; unsigned int :2; unsigned int high:4; \
         signed int value:5; unsigned int :0; unsigned int final:16; }; \
         struct Flags flags = {5, 9, -7, 4660}; \
         int main(void) { \
         if (flags.low != 5 || flags.high != 9 || flags.value != -7) return 1; \
         flags.high = 3; flags.value += 2; flags.low ^= 2; flags.final |= 32768U; \
         return flags.low == 7 && flags.high == 3 && flags.value == -5 \
             && flags.final == 37428U ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let bytes = (0..6)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, [0x00, 0x67, 0x00, 0x1b, 0x92, 0x34]);
}

#[test]
fn bitfields_cover_signed_widths_unsigned_promotions_and_every_update_operator() {
    for width in 1..=16 {
        let min = -(1i32 << (width - 1));
        let max = (1u32 << width) - 1;
        let source = format!(
            "struct S {{ signed int s:{width}; unsigned int u:{width}; }} value; \
             int main(void) {{ value.s={min}; value.u={max}U; \
             if (value.s != {min} || value.u != {max}U) return 1; \
             value.s >>= 1; return value.s == {} ? 0 : 2; }}",
            min >> 1,
        );
        assert_eq!(run_c(&source).registers[0], 0, "width {width}");
    }
    let cpu = run_c(
        "struct S { unsigned int small:3; unsigned int full:16; signed int s:6; } v; \
         int main(void) { v.small=7; v.full=65535U; v.s=-12; \
         if (!(v.small > -1) || !(v.full > 1) || !((v.small-8) < 0)) return 1; \
         v.s /= 3; v.s %= 3; if (v.s != -1) return 2; \
         v.small += 4; if (v.small != 3) return 3; \
         v.small *= 2; v.small -= 1; v.small <<= 1; \
         v.small |= 4; v.small &= 6; v.small ^= 2; \
         if (v.small != 4) return 4; \
         if (v.small++ != 4 || ++v.small != 6) return 5; \
         if (v.small-- != 6 || --v.small != 4) return 6; \
         return (v.small=15) == 7 ? 0 : 7; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn bitfield_lvalues_evaluate_indices_once_and_survive_nested_calls() {
    let cpu = run_c(
        "struct S { unsigned int left:4; unsigned int right:4; }; \
         struct S values[3] = {{1,2},{3,4},{5,6}}; \
         int rhs(int n) { if (n == 0) return 2; return rhs(n-1)+1; } \
         int main(void) { int i=0; int result; \
         result=(values[i++].left=rhs(2)); \
         if (i != 1 || result != 4) return 1; \
         result=(values[i++].left+=rhs(1)); \
         if (i != 2 || result != 6) return 2; \
         result=values[i++].right++; \
         if (i != 3 || result != 6 || values[2].right != 7) return 3; \
         i=0; result=++values[i++].left; \
         return i == 1 && result == 5 && values[0].right == 2 ? 0 : 4; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "struct S { unsigned int a:4; unsigned int b:4; } v = {1, 2}; \
         int change(void) { v.b=7; return 2; } \
         int main(void) { v.a+=change(); \
         if (v.a != 3 || v.b != 7) return 1; \
         v.a+=(v.b=3); return v.a == 6 && v.b == 3 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn bitfield_automatic_objects_unions_and_recursive_frames() {
    let source = "union U { unsigned int bits:9; unsigned int word; }; \
         union U initial = {257}; \
         struct S { signed int s:5; unsigned int u:3; }; \
         int recur(int n) { struct S local = {n-4, n}; \
         struct S *p=&local; int child=0; \
         if (p->s != n-4 || p->u != n) return 200; \
         if (n) child=recur(n-1); \
         if (p->s != n-4 || p->u != n) return 100+n; return child+1; } \
         int main(void) { union U overlay = {17}; \
         struct S array[2] = {{-7,2},{3,5}}; struct S *p=array; \
         if (initial.word != 257 || overlay.word != 17) return 1; \
         overlay.bits+=3; if (overlay.word != 20) return 2; \
         p->s+=2; if (p->s != -5 || array[0].u != 2) return 3; \
         if (array[1].s != 3) return 4; return recur(5); }";
    let cpu = run_c(source);
    assert_eq!(cpu.registers[0], 6);
}

#[test]
fn bitfield_unit_boundaries_nested_members_and_volatile_accesses() {
    let mut cpu = run_c(
        "struct S { unsigned int a:12; unsigned int b:8; unsigned int :0; \
         unsigned int c:3; }; \
         struct Outer { unsigned char sentinel; struct S nested; }; \
         volatile struct Outer object = {165, {2748, 129, 5}}; \
         struct V { volatile unsigned int bits:4; volatile signed int signed_bits:5; }; \
         volatile struct V v; \
         int main(void) { object.nested.b+=1; v.bits=7; v.signed_bits=-3; \
         if (v.bits++ != 7 || v.bits != 8 || v.signed_bits != -3) return 1; \
         return object.sentinel == 165 && object.nested.a == 2748 \
             && object.nested.b == 130 && object.nested.c == 5 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(bytes, [165, 0, 0x0a, 0xbc, 0, 130, 0, 5]);
}

#[test]
fn volatile_bitfields_execute_one_unit_read_and_write_per_update() {
    let source = "struct V { volatile unsigned int low:3; volatile unsigned int high:3; }; \
         volatile struct V v = {1, 5}; \
         int main(void) { int saved; v.low=2; v.low+=1; \
         saved=v.low++; ++v.low; v.low; return saved; }";
    let assembly = compile_minimal_program_to_assembly(source, "volatile-bits.c")
        .unwrap()
        .unwrap();
    let image = assemble(&assembly, 0).unwrap();
    let load = assemble("LDW R0, R7", 0).unwrap();
    let store = assemble("STW R7, R0", 0).unwrap();
    let opcode = |image: &suomicpu16::image::ProgramImage| {
        u16::from_be_bytes([image.segments[0].data[0], image.segments[0].data[1]]) >> 11
    };
    let load_opcode = opcode(&load);
    let store_opcode = opcode(&store);
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    let initial_sp = cpu.sp;
    let (mut reads, mut writes) = (0, 0);
    for _ in 0..10_000 {
        if !cpu.running {
            break;
        }
        let instruction = u16::from_be_bytes([cpu.read(cpu.pc), cpu.read(cpu.pc + 1)]);
        let address_register = ((instruction >> 5) & 7) as usize;
        if cpu.registers[address_register] == 0x80000 {
            if instruction >> 11 == load_opcode {
                reads += 1;
            } else if instruction >> 11 == store_opcode {
                writes += 1;
            }
        }
        cpu.step().unwrap();
    }
    assert!(!cpu.running);
    assert_eq!(cpu.sp, initial_sp);
    assert_eq!(cpu.registers[0], 3);
    assert_eq!((reads, writes), (5, 4));
    assert_eq!([cpu.read(0x80000), cpu.read(0x80001)], [0, 45]);
}

#[test]
fn bitfield_address_sizeof_and_invalid_declarations_are_diagnosed() {
    for expression in ["&v.bits", "sizeof(v.bits)", "sizeof((v.bits))"] {
        let source = format!(
            "struct V {{ unsigned int bits:3; }} v; \
             int main(void) {{ {expression}; return 0; }}"
        );
        let error = compile_minimal_program_to_assembly(&source, "invalid-bits.c").unwrap_err();
        assert!(
            error.0.contains("bit-field has no address or sizeof"),
            "{error}"
        );
        assert!(error.0.contains("invalid-bits.c:1:"), "{error}");
    }
    for declaration in [
        "int named:0",
        "int bits:17",
        "int bits:-1",
        "char bits:3",
        "long bits:3",
        "int *bits:3",
        "unsigned int bits[2]:3",
    ] {
        let source = format!("struct V {{ {declaration}; }} v; int main(void) {{ return 0; }}");
        assert!(
            compile_minimal_program_to_assembly(&source, "invalid-bits.c").is_err(),
            "{declaration}"
        );
    }
    let source = "#include <stddef.h>\nstruct V { unsigned int bits:3; }; \
                  int main(void) { return offsetof(struct V, bits); }";
    assert!(compile_minimal_program_to_assembly(source, "invalid-offset.c").is_err());
    for (c_type, value) in [("double", "2.5"), ("long", "65536L")] {
        let source = format!(
            "struct V {{ unsigned int bits:3; }} v; \
             int main(void) {{ {c_type} rhs={value}; v.bits+=rhs; return 0; }}"
        );
        let error =
            compile_minimal_program_to_assembly(&source, "pending-conversion.c").unwrap_err();
        assert!(
            error
                .0
                .contains("bit-field compound assignments are not supported"),
            "{error}"
        );
    }
    for source in [
        "struct V { double real; }; int main(void) { struct V v={1.5}; return 0; }",
        "struct V { long wide; }; int main(void) { struct V v; v.wide=1234; return 0; }",
        "struct V { double real; } v; int main(void) { v.real++; return 0; }",
    ] {
        assert!(
            !matches!(
                compile_minimal_program_to_assembly(source, "pending-record-member.c"),
                Ok(Some(_))
            ),
            "{source}"
        );
    }
}

#[test]
fn bitfield_assignment_converts_floating_values_before_narrowing() {
    let cpu = run_c(
        "struct V { signed int s:5; unsigned int u:4; } global; \
         int main(void) { double real=-3.75; float single=12.5f; \
         struct V local = {real, single}; \
         global.s=real; global.u=single; \
         if (local.s != -3 || local.u != 12) return 1; \
         return global.s == -3 && global.u == 12 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn record_scalar_layout_uses_target_widths_without_enabling_word_only_accesses() {
    let cpu = run_c(
        "struct S { signed char byte; long wide; float single; double real; }; \
         union U { unsigned char byte; long wide; double real; }; \
         struct S global; union U overlay; \
         int main(void) { struct S local; \
         return sizeof(global) == 18 && sizeof(local) == 18 && sizeof(overlay) == 8 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn signed_word_comparisons_and_full_width_pointer_spills_are_consistent() {
    let cpu = run_c(
        "int main(void) { int negative=-3; int values[2]; int *p=values; \
         if (negative != 1-4 || 1-4 != negative || -1 != -1) return 1; \
         if (!(negative < 1) || !(1 > negative)) return 2; \
         if (p != values || p != &values[0] || &values[0] != p) return 3; \
         return p < p+1 && p+1 > p ? 0 : 4; }",
    );
    assert_eq!(cpu.registers[0], 0);
}
