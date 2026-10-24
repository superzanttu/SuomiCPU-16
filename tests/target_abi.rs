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
