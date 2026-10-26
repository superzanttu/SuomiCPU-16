use suomicpu16::{Cpu, assembler::assemble, c_compiler::compile_minimal_program_to_assembly};

fn run_c(source: &str) -> Cpu {
    run_c_with_assembly(source, "")
}

fn run_c_with_assembly(source: &str, linked_assembly: &str) -> Cpu {
    let mut assembly = compile_minimal_program_to_assembly(source, "target-abi.c")
        .expect("parse C")
        .expect("compile C");
    assembly.push_str(linked_assembly);
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
fn nested_record_member_assignment_copies_full_record_contents() {
    let cpu = run_c(
        "struct Position { int x; long y; }; \
         struct Building { struct Position position; }; \
         struct State { struct Building buildings[1]; }; \
         struct Robot { struct Position position; }; \
         struct State state_object; struct State *state; \
         struct Robot robot_object; struct Robot *robot; \
         int main(void) { state=&state_object; robot=&robot_object; \
         state->buildings[0].position.x=1234; \
         state->buildings[0].position.y=-123456L; \
         robot->position=state->buildings[0].position; \
         return robot->position.x == 1234 && \
         robot->position.y == -123456L ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn record_assignment_copies_padding_and_member_bytes() {
    let cpu = run_c(
        "struct Padded { unsigned char byte; int word; }; \
         struct Padded source; struct Padded destination; \
         int main(void) { unsigned char *source_bytes=(unsigned char *)&source; \
         unsigned char *destination_bytes=(unsigned char *)&destination; \
         source.byte=7; source.word=4660; source_bytes[1]=165; \
         destination=source; \
         return sizeof(source)==4 && destination_bytes[1]==165 && \
         destination.byte==7 && destination.word==4660 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn pointer_subtraction_returns_a_signed_wide_element_difference() {
    for (difference, expected) in [
        ("&state_object.items[2]-state_object.items", 2i32),
        ("&state_object.items[0]-&state_object.items[2]", -2i32),
    ] {
        let mut cpu = run_c(&format!(
            "struct Item {{ int value; long tag; }}; \
             struct State {{ struct Item items[3]; }}; \
             long observed; struct State state_object; \
             int main(void) {{ observed={difference}; \
             return observed == {expected}L && \
             (int)({difference}) == {expected} ? 0 : 1; }}"
        ));
        let bytes = (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected.to_be_bytes(), "{difference}");
        assert_eq!(cpu.registers[0], 0);
    }
}

#[test]
fn pointer_subtraction_with_different_element_sizes_is_rejected() {
    let assembly = compile_minimal_program_to_assembly(
        "int main(void) { int *narrow; long *wide; return narrow-wide; }",
        "pointer-difference.c",
    )
    .expect("parse C");
    assert!(assembly.is_none());
}

#[test]
fn long_pointer_arithmetic_advances_by_four_byte_elements() {
    let cpu = run_c(
        "long values[2]; long *first; long *second; \
         unsigned long unsigned_values[2]; unsigned long *unsigned_pointer; \
         long distance; \
         int main(void) { first=values; second=first+1; \
         *first=123456L; second[0]=-654321L; \
         if (*first != 123456L || second[0] != -654321L) return 1; \
         unsigned_pointer=unsigned_values; unsigned_pointer[1]=4294967295UL; \
         if (unsigned_pointer[1] != 4294967295UL) return 2; \
         distance=second-first; \
         if (distance != 1L) return 3; \
         return second == &values[1] ? 0 : 4; }",
    );
    assert_eq!(cpu.registers[0], 0);
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
fn floating_to_byte_integer_casts_truncate_before_byte_normalization() {
    let cpu = run_c(
        "int main(void) { signed char positive=(signed char)123.75f; \
         signed char negative=(signed char)-127.75; \
         unsigned char maximum=(unsigned char)255.75f; \
         if (positive != 123 || negative != -127 || maximum != 255) return 1; \
         return (signed char)0.99 == 0 && (unsigned char)17.99 == 17 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn floating_pointer_casts_are_rejected_with_a_diagnostic() {
    for source in [
        "int main(void) { int *pointer=(int *)1.5; return 0; }",
        "int main(void) { double value=(double)(int *)0; return 0; }",
    ] {
        let error = compile_minimal_program_to_assembly(source, "invalid-float-pointer-cast.c")
            .unwrap_err();
        assert!(
            error
                .0
                .contains("conversion between floating-point and pointer types"),
            "{error}"
        );
    }
}

#[test]
fn mixed_floating_pointer_operations_are_rejected() {
    for source in [
        "int main(void) { int *pointer=0; double value=pointer+1.5; return 0; }",
        "int main(void) { int *pointer=0; return pointer<1.5; }",
        "int main(void) { int *pointer=0; double value=1.5; pointer=value; return 0; }",
        "int main(void) { int *pointer=0; double value=1.5; return (pointer ? value : pointer); }",
        "int *pointer(void) { return 0; } \
         int main(void) { double value=pointer()+1.5; return 0; }",
        "float *pointer(void) { return 0; } \
         int main(void) { double value=pointer()+1.5; return 0; }",
        "int main(void) { double value=1.5+\"text\"; return 0; }",
    ] {
        let error =
            compile_minimal_program_to_assembly(source, "invalid-float-pointer-op.c").unwrap_err();
        assert!(
            error.0.contains("floating-point and pointer types"),
            "{error}"
        );
    }
}

#[test]
fn floating_operands_are_rejected_for_integer_only_operators() {
    for source in [
        "int main(void) { double value=5.0%2; return 0; }",
        "int main(void) { double value=5.0&2; return 0; }",
        "int main(void) { double value=5.0<<2; return 0; }",
        "int main(void) { double value=5.0; value%=2; return 0; }",
        "int main(void) { double value=5.0; return ~value; }",
    ] {
        let error =
            compile_minimal_program_to_assembly(source, "invalid-float-integer-op.c").unwrap_err();
        assert!(
            error.0.contains("operator requires integer operands"),
            "{error}"
        );
    }
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
fn mixed_precision_and_unsigned_binary64_conversions_match_ieee_references() {
    let source = "double observed_real; float observed_single; \
                  int main(void) { unsigned long maximum=4294967295UL; \
                  double promoted=0.1f+0.1; \
                  float rounded=(float)(16777216.0+1.0); \
                  double widened=(double)rounded; \
                  double unsigned_real=(double)maximum; \
                  unsigned long restored=(unsigned long)unsigned_real; \
                  unsigned int word=(unsigned int)65535.75; \
                  unsigned int single_word=(unsigned int)65535.75f; \
                  float word_mix=0.5f+1U; double wide_mix=0.5f+1UL; \
                  long truncated=(long)-3.75; \
                  unsigned int comparison=(wide_mix == 1.5); \
                  if (promoted != (double)0.1f+0.1 || \
                      widened != 16777216.0 || \
                      unsigned_real != 4294967295.0 || \
                      restored != maximum || word != 65535U || \
                      single_word != 65535U || word_mix != 1.5f || \
                      wide_mix != 1.5 || truncated != -3L || \
                      comparison != 1U) return 1; \
                  if (!promoted || !widened || !unsigned_real || !wide_mix) return 2; \
                  observed_real=promoted; observed_single=rounded; return 0; }";
    let mut cpu = run_c(source);
    assert_eq!(cpu.registers[0], 0);
    let real_bytes = (0..8)
        .map(|offset| cpu.read(0x80000 + offset))
        .collect::<Vec<_>>();
    let single_bytes = (0..4)
        .map(|offset| cpu.read(0x80008 + offset))
        .collect::<Vec<_>>();
    assert_eq!(real_bytes, ((0.1f32 as f64) + 0.1f64).to_be_bytes());
    assert_eq!(single_bytes, 16_777_216.0f32.to_be_bytes());
}

#[test]
fn automatic_floating_arrays_initialize_from_runtime_expressions() {
    let cpu = run_c(
        "int main(void) { int seed=4; \
         float singles[2]={1.25f, seed++ + 0.5f}; \
         double doubles[2]={seed + 0.25, singles[1] + 0.125}; \
         if (seed != 5 || singles[0] != 1.25f || singles[1] != 4.5f) return 1; \
         if (doubles[0] != 5.25 || doubles[1] != 4.625) return 2; \
         return sizeof(singles)==8 && sizeof(doubles)==16 ? 0 : 3; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn floating_pointer_dereferences_support_load_store_and_compound_updates() {
    let mut cpu = run_c(
        "float observed_single; double observed_real; \
         int main(void) { float single=1.5f; double real=2.25; \
         float *single_pointer=&single; double *real_pointer=&real; \
         *single_pointer=2.5f; *real_pointer=3.75; \
         if (*single_pointer != 2.5f || *real_pointer != 3.75) return 1; \
         *single_pointer+=0.25f; *real_pointer*=2.0; \
         if (*single_pointer != 2.75f || *real_pointer != 7.5) return 2; \
         if ((*single_pointer)++ != 2.75f || *single_pointer != 3.75f) return 3; \
         if (++(*real_pointer) != 8.5 || *real_pointer != 8.5) return 4; \
         observed_single=*single_pointer; observed_real=*real_pointer; return 0; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0x80000..0x80004)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        3.75f32.to_be_bytes()
    );
    assert_eq!(
        (0x80004..0x8000c)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        8.5f64.to_be_bytes()
    );
}

#[test]
fn automatic_nested_record_initializers_evaluate_runtime_expressions_in_order() {
    let cpu = run_c(
        "struct Inner { int first; int second; }; \
         struct Outer { int before; struct Inner nested; int after; }; \
         int main(void) { int source=3; \
         struct Outer value={source++, {source++, source+4}}; \
         if (value.before != 3 || value.nested.first != 4 || \
             value.nested.second != 9 || value.after != 0 || source != 5) return 1; \
         return 0; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn automatic_multidimensional_record_arrays_initialize_nested_elements() {
    let cpu = run_c(
        "struct Pair { int left; int right; }; \
         int main(void) { int next=1; \
         struct Pair grid[2][2]={{{next++,next++},{next++,next++}}, \
                                  {{next++,next++}}}; \
         if (grid[0][0].left != 1 || grid[0][0].right != 2 || \
             grid[0][1].left != 3 || grid[0][1].right != 4 || \
             grid[1][0].left != 5 || grid[1][0].right != 6 || \
             grid[1][1].left != 0 || grid[1][1].right != 0 || next != 7) return 1; \
         return 0; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn unsupported_block_storage_classes_are_diagnosed() {
    for (declaration, storage_class) in [
        ("static int value=1;", "static"),
        ("extern int value;", "extern"),
    ] {
        let source = format!("int main(void) {{ {declaration} return 0; }}");
        let error = compile_minimal_program_to_assembly(&source, "storage.c").unwrap_err();
        assert!(
            error.0.contains(&format!(
                "block-scope {storage_class} declarations are not supported"
            )),
            "{error}"
        );
    }
}

#[test]
fn const_qualified_record_member_writes_are_rejected() {
    let cpu = run_c(
        "struct Flags { const unsigned int ready:1; }; \
         int main(void) { struct Flags flags={1}; return flags.ready ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "struct Value { const float number; }; \
         int main(void) { struct Value value={2.5f}; return value.number == 2.5f ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "struct Value { const int number; }; \
         int main(void) { struct Value value={6}; return value.number == 6 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "const int value=1; int main(void) { int value=2; value=3; \
         return value == 3 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);

    for source in [
        "struct Flags { const unsigned int ready:1; }; \
         int main(void) { struct Flags flags={0}; flags.ready=1; return 0; }",
        "struct Flags { const unsigned int ready:1; }; \
         int main(void) { struct Flags flags={0}; flags.ready+=1; return 0; }",
        "struct Flags { const unsigned int ready:1; }; \
         int main(void) { struct Flags flags={0}; flags.ready++; return 0; }",
        "struct Flags { const int ready; }; \
         int main(void) { struct Flags flags={0}; flags.ready=1; return 0; }",
        "struct Flags { const float ready; }; \
         int main(void) { struct Flags flags={0.0f}; flags.ready=1.0f; return 0; }",
    ] {
        assert!(
            compile_minimal_program_to_assembly(source, "const-member.c")
                .unwrap()
                .is_none(),
            "compiler accepted a write to a const-qualified member: {source}"
        );
    }
}

#[test]
fn writes_through_pointers_to_const_objects_are_rejected() {
    let cpu = run_c(
        "int main(void) { int value=1; const int *pointer=&value; \
         return *pointer == 1 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "int read_value(const int *pointer) { return *pointer; } \
         int main(void) { int value=9; return read_value(&value) == 9 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "const int *preserve(int *pointer) { return pointer; } \
         int main(void) { int value=6; const int *result=preserve(&value); \
         return *result == 6 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "const int *pointer; int main(void) { int value=7; pointer=&value; \
         return *pointer == 7 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "int main(void) { int value=4; int *source=&value; const int *target; \
         target=source; return *target == 4 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    let cpu = run_c(
        "int main(void) { const int value=4; const int *pointer=&value; \
         return *pointer == 4 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);

    for write in ["*pointer=2", "*pointer+=1", "(*pointer)++"] {
        let source = format!(
            "int main(void) {{ int value=1; const int *pointer=&value; {write}; return 0; }}"
        );
        assert!(
            compile_minimal_program_to_assembly(&source, "const-pointer.c")
                .unwrap()
                .is_none(),
            "compiler accepted a write through a pointer to const: {write}"
        );
    }
    for source in [
        "int write_value(const int *pointer) { *pointer=2; return 0; } \
         int main(void) { int value=1; return write_value(&value); }",
        "const int *pointer; int main(void) { int value=1; pointer=&value; \
         *pointer=2; return 0; }",
        "int main(void) { int value=1; const int *source=&value; \
         int *target=&value; target=source; return 0; }",
        "int read_value(int *pointer) { return *pointer; } \
         int main(void) { int value=1; const int *source=&value; \
         return read_value(source); }",
        "int *drop_const(const int *pointer) { return pointer; } \
         int main(void) { int value=1; const int *source=&value; \
         return drop_const(source) == &value ? 0 : 1; }",
        "int main(void) { int value=1; const int *source=&value; int *target; \
         target = 1 ? source : &value; return 0; }",
        "int main(void) { int value=1; const int *source=&value; int *target; \
         target = (0, source); return 0; }",
        "int main(void) { int value=1; const int *source=&value; int *target; \
         target = (int *)source; return 0; }",
        "int main(void) { const int **source=0; int **target=0; \
         target=source; return 0; }",
        "int main(void) { int value=1; const int *source=&value; \
         *(1 ? source : &value)=2; return 0; }",
        "int *drop_const(const int *pointer) { return (int *)pointer; } \
         int main(void) { int value=1; const int *source=&value; \
         return drop_const(source) == &value ? 0 : 1; }",
        "const int *preserve(int *pointer) { return pointer; } \
         int take(int *pointer) { return *pointer; } \
         int main(void) { int value=1; return take(1 ? preserve(&value) : &value); }",
        "int main(void) { const int value=1; int *pointer=&value; return 0; }",
        "const int value=1; int main(void) { int *pointer=&value; return 0; }",
        "int main(void) { const int values[2]={1,2}; int *pointer=&values[0]; return 0; }",
        "int take(int *pointer) { return *pointer; } \
         int main(void) { const int value=1; return take(&value); }",
        "int *escape(void) { const int value=1; return &value; } \
         int main(void) { return escape() != 0; }",
    ] {
        assert!(
            compile_minimal_program_to_assembly(source, "const-pointer.c")
                .unwrap()
                .is_none(),
            "compiler accepted a const-pointer violation: {source}"
        );
    }
}

#[test]
fn const_qualified_pointer_objects_are_diagnosed() {
    for source in [
        "int * const pointer;",
        "int main(void) { int * const pointer; return 0; }",
    ] {
        let error =
            compile_minimal_program_to_assembly(source, "const-pointer-object.c").unwrap_err();
        assert!(
            error
                .0
                .contains("qualifiers on pointer objects are not supported"),
            "{error}"
        );
    }
}

#[test]
fn const_qualified_scalar_and_array_writes_are_rejected() {
    let cpu = run_c(
        "const int global_value=3; const int global_array[2]={4,5}; \
         int read_value(const int parameter) { return parameter; } \
         int main(void) { const int local_value=6; const int local_array[2]={7,8}; \
         return global_value + global_array[1] + local_value + local_array[0] \
         + read_value(1) == 22 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);

    for source in [
        "int main(void) { const int value=1; value=2; return 0; }",
        "int main(void) { const int value=1; value+=1; return 0; }",
        "int main(void) { const int value=1; value++; return 0; }",
        "const int value=1; int main(void) { value=2; return 0; }",
        "const int values[2]={1,2}; int main(void) { values[0]=3; return 0; }",
        "int main(void) { const int values[2]={1,2}; values[1]++; return 0; }",
        "int update(const int value) { value=2; return value; } \
         int main(void) { return update(1); }",
    ] {
        assert!(
            compile_minimal_program_to_assembly(source, "const-object.c")
                .unwrap()
                .is_none(),
            "compiler accepted a write to a const-qualified object: {source}"
        );
    }
}

#[test]
fn floating_scalar_prefix_and_postfix_updates_preserve_old_and_new_values() {
    let source = "float global_single=1.25f; double global_double=4.5; \
                  int main(void) { float single=1.5f; double real=2.25; \
                  float old_single=single++; \
                  if (old_single != 1.5f || single != 2.5f) return 1; \
                  if (++single != 3.5f || single != 3.5f) return 2; \
                  if (single-- != 3.5f || single != 2.5f) return 3; \
                  if (--single != 1.5f || single != 1.5f) return 4; \
                  double old_real=real--; \
                  if (old_real != 2.25 || real != 1.25) return 5; \
                  if (++real != 2.25 || real != 2.25) return 6; \
                  if (real++ != 2.25 || real != 3.25) return 7; \
                  if (--real != 2.25 || real != 2.25) return 8; \
                  if (global_single++ != 1.25f || global_single != 2.25f) return 9; \
                  if (--global_double != 3.5 || global_double != 3.5) return 10; \
                  return 0; }";
    let mut cpu = run_c(source);
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>(),
        2.25f32.to_be_bytes()
    );
    assert_eq!(
        (0..8)
            .map(|offset| cpu.read(0x80004 + offset))
            .collect::<Vec<_>>(),
        3.5f64.to_be_bytes()
    );
}

#[test]
fn floating_array_element_updates_preserve_old_and_new_values() {
    let source = "float singles[2]={1.25f,2.5f}; \
                  double doubles[2]={3.5,4.75}; \
                  int main(void) { int index=1; \
                  float old_single=singles[index]++; \
                  if (old_single != 2.5f || singles[index] != 3.5f) return 1; \
                  if (++singles[0] != 2.25f || singles[0] != 2.25f) return 2; \
                  double old_double=doubles[index]--; \
                  if (old_double != 4.75 || doubles[index] != 3.75) return 3; \
                  if (--doubles[0] != 2.5 || doubles[0] != 2.5) return 4; \
                  return 0; }";
    let mut cpu = run_c(source);
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>(),
        2.25f32.to_be_bytes()
    );
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80004 + offset))
            .collect::<Vec<_>>(),
        3.5f32.to_be_bytes()
    );
    assert_eq!(
        (0..8)
            .map(|offset| cpu.read(0x80008 + offset))
            .collect::<Vec<_>>(),
        2.5f64.to_be_bytes()
    );
    assert_eq!(
        (0..8)
            .map(|offset| cpu.read(0x80010 + offset))
            .collect::<Vec<_>>(),
        3.75f64.to_be_bytes()
    );
}

#[test]
fn variadic_calls_apply_default_promotions_and_stack_object_layout() {
    let source = "struct Pair { unsigned int first; unsigned int second; }; \
                  unsigned int datum; \
                  struct Pair pair={0xABCDU,0x1234U}; \
                  int probe(int marker, ...); \
                  int main(void) { signed char negative=-2; \
                  unsigned char positive=250; \
                  return probe(7,negative,positive,1.5f,&datum,pair,0x12345678UL); }";
    let linked_assembly = ".address 0x70000\n\
                           __c_function_probe:\n\
                               LDWS R4, 4\n    LDI R5, 7\n    CMP R4, R5\n\
                               BNZX __probe_fail_1\n\
                               LDWS R4, 6\n    LDI R5, 0xFFFE\n    CMP R4, R5\n\
                               BNZX __probe_fail_2\n\
                               LDWS R4, 8\n    LDI R5, 250\n    CMP R4, R5\n\
                               BNZX __probe_fail_3\n\
                               LDWS R4, 10\n    LDI R5, 0x3FF8\n    CMP R4, R5\n\
                               BNZX __probe_fail_4\n\
                               LDWS R4, 12\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __probe_fail_5\n\
                               LDWS R4, 14\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __probe_fail_6\n\
                               LDWS R4, 16\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __probe_fail_7\n\
                               LDWS R4, 18\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __probe_fail_8\n\
                               LDWS R4, 20\n    LDI R5, 8\n    CMP R4, R5\n\
                               BNZX __probe_fail_9\n\
                               LDWS R4, 22\n    LDI R5, 0xABCD\n    CMP R4, R5\n\
                               BNZX __probe_fail_10\n\
                               LDWS R4, 24\n    LDI R5, 0x1234\n    CMP R4, R5\n\
                               BNZX __probe_fail_11\n\
                               LDWS R4, 26\n    LDI R5, 0x1234\n    CMP R4, R5\n\
                               BNZX __probe_fail_12\n\
                               LDWS R4, 28\n    LDI R5, 0x5678\n    CMP R4, R5\n\
                               BNZX __probe_fail_13\n\
                               LDI R0, 0\n    RET\n\
                           __probe_fail_1:\n    LDI R0, 1\n    RET\n\
                           __probe_fail_2:\n    LDI R0, 2\n    RET\n\
                           __probe_fail_3:\n    LDI R0, 3\n    RET\n\
                           __probe_fail_4:\n    LDI R0, 4\n    RET\n\
                           __probe_fail_5:\n    LDI R0, 5\n    RET\n\
                           __probe_fail_6:\n    LDI R0, 6\n    RET\n\
                           __probe_fail_7:\n    LDI R0, 7\n    RET\n\
                           __probe_fail_8:\n    LDI R0, 8\n    RET\n\
                           __probe_fail_9:\n    LDI R0, 9\n    RET\n\
                           __probe_fail_10:\n    LDI R0, 10\n    RET\n\
                           __probe_fail_11:\n    LDI R0, 11\n    RET\n\
                           __probe_fail_12:\n    LDI R0, 12\n    RET\n\
                           __probe_fail_13:\n    LDI R0, 13\n    RET\n";
    let cpu = run_c_with_assembly(source, linked_assembly);
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn unprototyped_calls_apply_default_promotions_and_stack_object_layout() {
    let source = "struct Pair { unsigned int first; unsigned int second; }; \
                  unsigned int datum; \
                  struct Pair pair={0xABCDU,0x1234U}; \
                  int probe(); \
                  int main(void) { signed char negative=-2; \
                  unsigned char positive=250; \
                  return probe(negative,positive,1.5f,&datum,pair,0x12345678UL); }";
    let linked_assembly = ".address 0x70000\n\
                           __c_function_probe:\n\
                               LDWS R4, 4\n    LDI R5, 0xFFFE\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_1\n\
                               LDWS R4, 6\n    LDI R5, 250\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_2\n\
                               LDWS R4, 8\n    LDI R5, 0x3FF8\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_3\n\
                               LDWS R4, 10\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_4\n\
                               LDWS R4, 12\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_5\n\
                               LDWS R4, 14\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_6\n\
                               LDWS R4, 16\n    LDI R5, 0\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_7\n\
                               LDWS R4, 18\n    LDI R5, 8\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_8\n\
                               LDWS R4, 20\n    LDI R5, 0xABCD\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_9\n\
                               LDWS R4, 22\n    LDI R5, 0x1234\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_10\n\
                               LDWS R4, 24\n    LDI R5, 0x1234\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_11\n\
                               LDWS R4, 26\n    LDI R5, 0x5678\n    CMP R4, R5\n\
                               BNZX __unprototyped_fail_12\n\
                               LDI R0, 0\n    RET\n\
                           __unprototyped_fail_1:\n    LDI R0, 1\n    RET\n\
                           __unprototyped_fail_2:\n    LDI R0, 2\n    RET\n\
                           __unprototyped_fail_3:\n    LDI R0, 3\n    RET\n\
                           __unprototyped_fail_4:\n    LDI R0, 4\n    RET\n\
                           __unprototyped_fail_5:\n    LDI R0, 5\n    RET\n\
                           __unprototyped_fail_6:\n    LDI R0, 6\n    RET\n\
                           __unprototyped_fail_7:\n    LDI R0, 7\n    RET\n\
                           __unprototyped_fail_8:\n    LDI R0, 8\n    RET\n\
                           __unprototyped_fail_9:\n    LDI R0, 9\n    RET\n\
                           __unprototyped_fail_10:\n    LDI R0, 10\n    RET\n\
                           __unprototyped_fail_11:\n    LDI R0, 11\n    RET\n\
                           __unprototyped_fail_12:\n    LDI R0, 12\n    RET\n";
    let cpu = run_c_with_assembly(source, linked_assembly);
    assert_eq!(cpu.registers[0], 0);
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
fn floating_truth_conversions_treat_nan_as_true_and_both_zeros_as_false() {
    let cpu = run_c(
        "int main(void) { \
         float single_nan=0.0f/0.0f; double double_nan=0.0/0.0; \
         float single_zero=-0.0f; double double_zero=-0.0; \
         if (!single_nan || !double_nan) return 1; \
         if (single_zero || double_zero) return 2; \
         if (!(single_nan && double_nan)) return 3; \
         if (single_zero || double_zero) return 4; \
         return (single_nan ? 0 : 5) + (double_nan ? 0 : 6); }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn implicit_binary64_to_binary32_assignments_convert_and_round() {
    let mut cpu = run_c(
        "struct Holder { float item; }; float observed; \
         double exact_step(void) { return 16777217.0; } \
         int main(void) { double exact=16777217.0; float value=1.0f; \
         float mixed=1.0f+2.0; float values[2]={1.0f, 1.0f}; int index=0; \
         float pointed=1.0f; float *pointer=&pointed; \
         struct Holder holder={1.0f}; \
         value=exact; observed=exact+0.5; \
         if (value != 16777216.0f || observed != 16777218.0f || mixed != 3.0f) return 1; \
         value=index++ ? exact : 16777217.0; \
         if (value != 16777216.0f || index != 1) return 2; \
         value=1.0f; value+=exact; \
         if (value != 16777218.0f) return 3; \
         value=1.0f; value+=exact_step(); \
         if (value != 16777218.0f) return 4; \
         values[index++]+=exact; \
         if (values[1] != 16777218.0f || index != 2) return 5; \
         holder.item+=exact; \
         if (holder.item != 16777218.0f) return 6; \
         *pointer+=exact; \
         if (pointed != 16777218.0f) return 7; \
         return 0; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>(),
        16_777_218.0f32.to_be_bytes()
    );
}

#[test]
fn binary64_compound_operators_narrow_for_all_float_lvalue_kinds() {
    let cpu = run_c(
        "struct Holder { float item; }; \
         float step(void) { return 0.5f; } \
         int main(void) { double two=2.0; unsigned long wide=16777217UL; \
         float wide_value=1.0f; float value=8.0f; \
         float values[1]={8.0f}; float pointed=8.0f; \
         float *pointer=&pointed; struct Holder holder={8.0f}; \
         wide_value+=wide; \
         value+=two; value-=two; value*=two; value/=two; \
         value+=step(); value-=step(); \
         values[0]+=two; values[0]-=two; values[0]*=two; values[0]/=two; \
         holder.item+=two; holder.item-=two; holder.item*=two; holder.item/=two; \
         *pointer+=two; *pointer-=two; *pointer*=two; *pointer/=two; \
         return wide_value!=16777216.0f || value!=8.0f || values[0]!=8.0f || \
         holder.item!=8.0f || pointed!=8.0f; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn binary32_rhs_promotes_for_binary64_compound_lvalues() {
    let cpu = run_c(
        "struct Holder { double item; }; \
         float next_step(void) { return 0.1f; } \
         int main(void) { float step=0.1f; \
         double value=1.0; double values[1]={1.0}; \
         double pointed=1.0; double *pointer=&pointed; \
         struct Holder holder={1.0}; double all_ops=4.0; \
         value+=step; values[0]+=step; holder.item+=step; *pointer+=step; \
         all_ops+=step; all_ops-=step; all_ops*=step; all_ops/=step; \
         if (value!=1.0+(double)step || values[0]!=value || \
         holder.item!=value || pointed!=value || all_ops!=4.0) return 1; \
         value+=next_step(); \
         return value==1.0+(double)step+(double)next_step() ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn binary64_compound_lvalues_preserve_addresses_across_function_calls() {
    let cpu = run_c(
        "struct Holder { double item; }; \
         double next_step(void) { return 0.125; } \
         int main(void) { double value=1.0; double values[1]={1.0}; \
         double pointed=1.0; double *pointer=&pointed; \
         struct Holder holder={1.0}; \
         value+=next_step(); values[0]+=next_step(); \
         holder.item+=next_step(); *pointer+=next_step(); \
         return value!=1.125 || values[0]!=value || \
         holder.item!=value || pointed!=value; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn pointer_return_types_survive_call_expression_analysis() {
    let mut cpu = run_c(
        "long object; long *get_object(void) { return &object; } \
         int main(void) { *get_object()=123456L; \
         return *get_object()==123456L ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>(),
        123_456i32.to_be_bytes()
    );
}

#[test]
fn array_parameters_adjust_to_pointers_and_execute() {
    let cpu = run_c(
        "int total(int values[3]); \
         int total(int values[4]) { return values[0]+values[1]+values[2]; } \
         int main(void) { int values[3]={11,22,33}; \
         return total(values)==66 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn sizeof_function_and_array_pointers_uses_target_pointer_size() {
    let cpu = run_c(
        "int main(void) { return sizeof(int (*)(int)) == 4 && \
         sizeof(int (*)[3]) == 4 ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn returned_float_pointers_support_dereference_load_store_and_compound_update() {
    let mut cpu = run_c(
        "float object; float *get_object(void) { return &object; } \
         int main(void) { *get_object()=1.5f; *get_object()+=0.25f; \
         return *get_object()==1.75f ? 0 : 1; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0..4)
            .map(|offset| cpu.read(0x80000 + offset))
            .collect::<Vec<_>>(),
        1.75f32.to_be_bytes()
    );
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
fn aggregate_arguments_use_object_bytes_and_recursive_copies() {
    let cpu = run_c(
        "struct Pair { signed char left; int right; }; \
         struct Outer { unsigned char prefix; struct Pair nested; }; \
         union Overlay { int word; unsigned char byte; }; \
         int combine(int before, struct Pair pair, unsigned char after) { \
         pair.left++; pair.right+=before; \
         return pair.left+pair.right+after; } \
         int inspect(struct Outer outer, union Overlay overlay, int after) { \
         outer.nested.right++; overlay.word+=after; \
         return outer.prefix+outer.nested.right+overlay.word; } \
         int recurse(int depth, struct Pair pair) { \
         if (!depth) return pair.left+pair.right; \
         pair.right+=depth; return recurse(depth-1,pair)+pair.right; } \
         int main(void) { struct Pair pair={-3,1000}; \
         struct Outer outer={9,{-3,1000}}; union Overlay overlay={1200}; \
         if (combine(7,pair,250) != 1255) return 1; \
         if (pair.left != -3 || pair.right != 1000) return 2; \
         if (inspect(outer,overlay,7) != 2217) return 3; \
         if (outer.nested.right != 1000 || overlay.word != 1200) return 4; \
         return recurse(3,pair) == 4017 ? 0 : 5; }",
    );
    assert_eq!(cpu.registers[0], 0);
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
fn no_prototype_declarations_merge_with_promotion_compatible_prototypes() {
    for (source, expected) in [
        (
            "int f(); int f(int); int f(int value) { return value; } \
             int main(void) { return f(-9); }",
            -9,
        ),
        (
            "int f(int); int f(); int f(int value) { return value; } \
             int main(void) { return f(13); }",
            13,
        ),
        (
            "int f(); int f(double); int f(double value) { return value == 1.5 ? 7 : 1; } \
             int main(void) { return f(1.5f) == 7 ? 0 : 1; }",
            0,
        ),
        (
            "int f(double); int f(); int f(double value) { return value == 2.5 ? 8 : 1; } \
             int main(void) { return f(2.5f) == 8 ? 0 : 1; }",
            0,
        ),
    ] {
        let cpu = run_c(source);
        assert_eq!(cpu.registers[0], expected);
    }

    for declarations in [
        "int f(); int f(char);",
        "int f(); int f(float);",
        "int f(); int f(int, ...);",
        "int f(); int f(unsigned char);",
    ] {
        let source = format!("{declarations} int main(void) {{ return 0; }}");
        let error = compile_minimal_program_to_assembly(&source, "prototype-compat.c").unwrap_err();
        assert!(
            error.0.contains("conflicting function type for f"),
            "{error}"
        );
    }
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
}

#[test]
fn bitfield_assignment_converts_floating_values_before_narrowing() {
    let cpu = run_c(
        "struct V { signed int s:5; unsigned int u:4; } global={1,2}; \
         int main(void) { double real=-3.75; float single=12.5f; \
         struct V local = {real, single}; \
         global.s=real; global.u=single; \
         if (local.s != -3 || local.u != 12) return 1; \
         return global.s == -3 && global.u == 12 ? 0 : 2; }",
    );
    assert_eq!(cpu.registers[0], 0);
}

#[test]
fn bitfield_compound_assignments_convert_floating_and_wide_values_before_narrowing() {
    let source = "struct V { signed int s:5; unsigned int u:4; } global; \
         struct V records[2]; int index_calls; \
         int next_index(void) { index_calls++; return 1; } \
         int main(void) { struct V local={1,2}; \
         struct V *pointer=&global; double real=4.75; long wide=65538L; \
         local.s+=real; local.u+=real; \
         global.s=1; global.u=2; \
         if ((pointer->s+=wide)!=3 || (pointer->u+=wide)!=4) return 2; \
         records[1].s=1; \
         if ((records[next_index()].s+=wide)!=3 || index_calls!=1) return 5; \
         if (local.s != 5 || local.u != 6) return 1; \
         if (global.s != 3 || global.u != 4) return 4; \
         local.s-=2.5; local.u*=2.5; \
         if (local.s != 2 || local.u != 15) return 3; \
         return 0; }";
    let cpu = run_c(source);
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
fn floating_record_members_load_store_and_update_their_object_bytes() {
    let mut cpu = run_c(
        "struct Inner { float single; double real; }; \
         struct Outer { int tag; struct Inner inner; }; \
         struct Outer global; \
         int main(void) { struct Outer local={0,{1.5f,-3.5}}; \
         struct Inner *pointer=&local.inner; \
         if (pointer != &local.inner) return 8; \
         if (local.inner.single != 1.5f || local.inner.real != -3.5) return 13; \
         local.inner.single+=2.25f; if (pointer != &local.inner) return 10; \
         if (local.inner.single++ != 3.75f) return 1; \
         if (pointer != &local.inner) return 11; \
         local.inner.real=-3.5; local.inner.real+=1.25; \
         if (pointer != &local.inner) return 12; \
         if (pointer->single != 4.75f) return 2; \
         pointer->single+=0.25f; if (pointer->single != 5.0f) return 3; \
         pointer->real++; \
         global.inner.single=pointer->single; global.inner.real=pointer->real; \
         if (pointer->real != -1.25) return 4; \
         return global.inner.single == 5.0f && global.inner.real == -1.25 ? 0 : 7; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0x80002..0x80006)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        [0x40, 0xA0, 0x00, 0x00]
    );
    assert_eq!(
        (0x80006..0x8000E)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        [0xBF, 0xF4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn floating_arrays_in_records_support_indexed_updates_and_stores() {
    let mut cpu = run_c(
        "struct Samples { float singles[2]; double reals[2]; }; \
         struct Samples global; \
         int main(void) { struct Samples local; struct Samples *pointer=&local; \
         pointer->singles[0]=1.5f; local.singles[1]=2.5f; \
         pointer->reals[0]=-1.25; local.reals[1]=0.5; \
         pointer->singles[1]+=1.25f; \
         if (local.singles[0]++ != 1.5f || local.singles[1] != 3.75f) return 1; \
         local.reals[0]-=0.75; \
         if (local.reals[1]++ != 0.5 || local.reals[0] != -2.0) return 2; \
         global.singles[0]=local.singles[1]; global.reals[1]=local.reals[0]; \
         return global.singles[0] == 3.75f && global.reals[1] == -2.0 ? 0 : 3; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0x80000..0x80004)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        [0x40, 0x70, 0x00, 0x00]
    );
    assert_eq!(
        (0x80010..0x80018)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn wide_integer_record_members_load_store_and_update_all_bytes() {
    let mut cpu = run_c(
        "struct Pair { long signed_value; unsigned long unsigned_value; long samples[2]; }; \
         struct Wrapper { int tag; struct Pair pair; }; \
         struct Wrapper global; \
         int main(void) { struct Wrapper local; struct Pair *pointer=&local.pair; \
         local.pair.signed_value=-65537L; \
         if (pointer->signed_value != -65537L) return 1; \
         pointer->signed_value+=70000L; \
         if (pointer->signed_value != 4463L || pointer->signed_value++ != 4463L) return 2; \
         if (local.pair.signed_value != 4464L) return 3; \
         pointer->unsigned_value=0xFEDCBA98UL; \
         pointer->unsigned_value-=0x10000000UL; \
         if (local.pair.unsigned_value != 0xEEDCBA98UL) return 4; \
         local.pair.samples[1]=-3L; local.pair.samples[1]+=8L; \
         if (pointer->samples[1] != 5L) return 6; \
         global.pair.signed_value=local.pair.signed_value; \
         global.pair.unsigned_value=pointer->unsigned_value; \
         global.pair.samples[1]=local.pair.samples[1]; \
         return global.pair.signed_value == 4464L && \
                global.pair.unsigned_value == 0xEEDCBA98UL && \
                global.pair.samples[1] == 5L ? 0 : 5; }",
    );
    assert_eq!(cpu.registers[0], 0);
    assert_eq!(
        (0x80002..0x80006)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        4464i32.to_be_bytes()
    );
    assert_eq!(
        (0x80006..0x8000A)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        0xEEDC_BA98u32.to_be_bytes()
    );
    assert_eq!(
        (0x8000E..0x80012)
            .map(|address| cpu.read(address))
            .collect::<Vec<_>>(),
        5i32.to_be_bytes()
    );
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
