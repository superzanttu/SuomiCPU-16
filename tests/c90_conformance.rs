use suomicpu16::{
    Cpu,
    assembler::assemble,
    c_compiler::{compile_minimal_program, compile_minimal_program_to_assembly},
};

fn run_c(source: &str) -> i64 {
    let assembly = compile_minimal_program_to_assembly(source, "c90-conformance.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let image = assemble(&assembly, 0).expect("compiler-generated assembly should assemble");
    let mut cpu = Cpu::with_memory_size(1 << 20).expect("CPU allocation");
    cpu.load_program(&image.segments, image.entry_point)
        .expect("load compiled program");
    cpu.run(1_000_000).expect("execute compiled program");
    assert!(!cpu.running, "compiled program did not terminate");
    cpu.registers[0]
}

#[test]
fn c_compiler_image_matches_independently_assembled_output() {
    let source = "int main(void) { return 40 + 2; }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-conformance.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let assembled = assemble(&assembly, 0).expect("compiler-generated assembly should assemble");
    let compiled = compile_minimal_program(source, "c90-conformance.c")
        .expect("C source should parse")
        .expect("C source should compile");
    assert_eq!(assembled.entry_point, compiled.entry_point);
    assert_eq!(assembled.segments, compiled.segments);
}

#[test]
fn c90_integer_expression_and_control_flow_cases() {
    for (source, expected) in [
        ("int main(void) { return 3 + 4 * 5 - (8 >> 1); }", 19),
        (
            "int main(void) { int value = 0; if (0 && (value = 7)) return 1; \
             if (1 || (value = 9)) return value; return 2; }",
            0,
        ),
        (
            "int main(void) { int i; int sum = 0; for (i = 0; i < 6; i++) { \
             if (i == 2) continue; if (i == 5) break; sum += i; } return sum; }",
            8,
        ),
        (
            "int main(void) { int value = 0; switch (3) { case 1: value = 10; \
             break; case 2: value = 20; case 3: value += 3; break; \
             default: value = 99; } return value; }",
            3,
        ),
        (
            "int main(void) { int value = 0; again: value++; \
             if (value < 4) goto again; return value; }",
            4,
        ),
        (
            "int main(void) { int i = 0; int sum = 0; do { sum += ++i; } \
             while (i < 4); return sum; }",
            10,
        ),
    ] {
        assert_eq!(run_c(source), expected, "{source}");
    }
}

#[test]
fn c90_function_calls_preserve_computed_arguments_and_frames() {
    for (source, expected) in [
        (
            "int identity(int value) { return value; } \
             int main(void) { return identity(70 - 72); }",
            -2,
        ),
        (
            "int sum(int a, int b, int c) { return a * 100 + b * 10 + c; } \
             int main(void) { return sum(10 - 1, 20 + 2, 3 * 4); }",
            1132,
        ),
        (
            "int recurse(int value) { if (value < 2) return 1; \
             return value * recurse(value - 1); } \
             int main(void) { return recurse(5); }",
            120,
        ),
        (
            "int helper(int a, int b, int c, int d) { return a + b + c + d; } \
             int main(void) { return helper(1, 2, 3, 4); }",
            10,
        ),
    ] {
        assert_eq!(run_c(source), expected, "{source}");
    }
}

#[test]
fn c90_scalar_call_argument_uses_documented_stack_abi() {
    let source = "int identity(int value) { return value; } \
                 int main(void) { return identity(42); }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-call-abi.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let identity_start = assembly
        .find("__c_function_identity:\n")
        .expect("identity function should be emitted");
    let main_start = assembly
        .find("__c_function_main:\n")
        .expect("main function should be emitted");
    let identity = &assembly[identity_start..main_start];
    let main = &assembly[main_start..];

    assert!(identity.contains("LDWS R1, 4\n"), "{identity}");
    assert!(
        main.contains("PUSH R4\n    CALLX __c_function_identity\n"),
        "{main}"
    );
    assert_eq!(run_c(source), 42);
}

#[test]
fn c90_object_layout_arrays_and_pointers() {
    for (source, expected) in [
        (
            "int main(void) { return sizeof(char) * 1000 + sizeof(int) * 100 + \
             sizeof(long) * 10 + sizeof(int *); }",
            1244,
        ),
        (
            "int main(void) { int values[3] = { 4, 7, 11 }; \
             int *cursor = values; cursor++; return *cursor; }",
            7,
        ),
        (
            "int main(void) { int values[2][3] = { { 1, 2, 3 }, { 4, 5, 6 } }; \
             return values[1][2]; }",
            6,
        ),
    ] {
        assert_eq!(run_c(source), expected, "{source}");
    }
}

#[test]
fn c90_binary64_runtime_arithmetic_conversions_and_calls() {
    let source = "double scale(double value) { value += 0.5; return value * 2.0; } \
                 int main(void) { double values[2]; double *pointer = values; \
                 pointer[0] = 1.25; pointer[1] = pointer[0] + 1.25; \
                 if (pointer[1] * 2.0 != 5.0) return 10; \
                 if (pointer[0] + 1 != 2.25) return 15; \
                 if ((int)2.75 != 2) return 20; \
                 if (scale(1.25) != 3.5) return 30; return 0; }";
    assert_eq!(run_c(source), 0, "binary64 runtime execution");
}

#[test]
fn c90_binary32_and_binary64_convert_32_bit_integer_values() {
    let source = "float observed; \
                 int main(void) { long signed_value = 65536L; \
                 unsigned long unsigned_value = 131072UL; \
                 float signed_single = signed_value; \
                 double signed_real = signed_value; \
                 float unsigned_single = unsigned_value; \
                 double unsigned_real = unsigned_value; \
                 observed = unsigned_single; \
                 if (signed_single != 65536.0f) return 10; \
                 if (signed_real != 65536.0) return 20; \
                 if (unsigned_single != 131072.0f) return 30; \
                 if (unsigned_real != 131072.0) return 40; return 0; }";
    assert_eq!(run_c(source), 0, "32-bit integer to float conversions");
}

#[test]
fn c90_bit_field_layout_and_unsupported_access() {
    let source = "struct Flags { unsigned int ready : 1; unsigned int mode : 3; \
                  signed int signed_value : 4; unsigned int : 0; \
                  unsigned int after_zero : 2; }; \
                  union Overlay { unsigned int low : 4; unsigned int high : 9; \
                  unsigned char byte; }; \
                  struct Flags flags; union Overlay overlay; \
                  int main(void) { return sizeof(flags) * 100 + sizeof(overlay); }";
    assert_eq!(run_c(source), 602, "bit-field aggregate sizes");

    let unsupported_access = "struct Flags { unsigned int ready : 1; }; \
                              int main(void) { struct Flags flags; \
                              return flags.ready; }";
    assert!(matches!(
        compile_minimal_program(unsupported_access, "bit-field-access.c"),
        Ok(None) | Err(_)
    ));
}

#[test]
fn c90_preprocessing_typedef_enum_and_header_macros() {
    let source = "\
        #include <limits.h>\n\
        #include <stdint.h>\n\
        #define JOIN(a, b) a ## b\n\
        #define VALUE(x) ((x) + 2)\n\
        typedef unsigned char Byte;\n\
        enum { START = 3, END = START + 4 };\n\
        int JOIN(result, _value) = VALUE(END);\n\
        int main(void) { Byte value = UINT8_MAX; \
            return CHAR_BIT + value + result_value; }\n";
    assert_eq!(run_c(source), 272, "preprocessor/typedef/enum source");
    assert_eq!(
        run_c("#include <limits.h>\nint main(void) { return CHAR_BIT * 100 + UCHAR_MAX; }"),
        1055,
        "C90 limits macros"
    );
}

#[test]
fn implemented_c90_headers_compile_and_run_standalone() {
    for header in [
        "assert.h", "ctype.h", "errno.h", "float.h", "limits.h", "locale.h", "math.h", "stdarg.h",
        "stddef.h", "stdio.h", "stdlib.h", "string.h", "time.h",
    ] {
        let source = format!("#include <{header}>\nint main(void) {{ return 0; }}");
        let image = compile_minimal_program(&source, "c90-header.c")
            .unwrap_or_else(|error| panic!("{header}: {error}"))
            .unwrap_or_else(|| panic!("{header}: standalone header did not compile"));
        let mut cpu = Cpu::with_memory_size(1 << 20).expect("CPU allocation");
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap_or_else(|error| panic!("{header}: {error}"));
        cpu.run(1_000_000)
            .unwrap_or_else(|error| panic!("{header}: {error}"));
        assert!(!cpu.running, "{header}: empty program did not terminate");
    }
}

#[test]
fn known_unimplemented_c90_headers_are_rejected_explicitly() {
    for (header, expected_diagnostic) in [
        (
            "setjmp.h",
            "array and initialized typedefs are not supported",
        ),
        ("signal.h", "cannot find include file"),
    ] {
        let source = format!("#include <{header}>\nint main(void) {{ return 0; }}");
        match compile_minimal_program(&source, "c90-header.c") {
            Err(error) => assert!(
                error.0.contains(expected_diagnostic),
                "{header}: unexpected diagnostic: {error}"
            ),
            Ok(_) => panic!("{header}: unsupported header unexpectedly compiled"),
        }
    }
}

#[test]
fn unsupported_c90_facilities_are_not_silently_miscompiled() {
    for source in [
        "int (*callback)(int); int main(void) { return 0; }",
        "typedef struct { int x; int y; } Point; \
         int main(void) { Point point = { 2, 5 }; return point.x; }",
        "int main(void) { return sizeof(short); }",
    ] {
        match compile_minimal_program(source, "unsupported-c90.c") {
            Ok(None) | Err(_) => {}
            Ok(Some(_)) => panic!("unsupported C90 facility was accepted: {source}"),
        }
    }
}
