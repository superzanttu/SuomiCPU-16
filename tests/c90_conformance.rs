//! Exercises the supported C90 subset through compilation and CPU execution.
//! Checks language behavior and consistency between compiler images and assembled output.

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
fn guest_c_machine_header_reads_the_supervisor_system_bank() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned long *revision = (volatile unsigned long *)SC16_SYSTEM_MACHINE_REVISION;\n\
                 volatile unsigned long *ram = (volatile unsigned long *)SC16_SYSTEM_RAM_BYTES;\n\
                 volatile unsigned long *bits = (volatile unsigned long *)SC16_SYSTEM_ADDRESS_BITS;\n\
                 volatile unsigned long *banks = (volatile unsigned long *)SC16_SYSTEM_BANKS;\n\
                 if (*revision != 1UL) return 11;\n\
                 if (*ram != 1048576UL) return 12;\n\
                 if (*bits != 30UL) return 13;\n\
                 if (*banks != 4095UL) return 14;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_draws_through_the_high_graphics_bank() {
    let assembly = compile_minimal_program_to_assembly(
        "#include <sc16_machine.h>\n\
         int main(void) {\n\
             volatile unsigned char *command = (volatile unsigned char *)SC16_GRAPHICS_COMMAND;\n\
             volatile unsigned char *status = (volatile unsigned char *)SC16_GRAPHICS_STATUS;\n\
             volatile unsigned char *operation = (volatile unsigned char *)SC16_GRAPHICS_OPERATION;\n\
             volatile unsigned char *color = (volatile unsigned char *)SC16_GRAPHICS_COLOR;\n\
             volatile unsigned char *mode = (volatile unsigned char *)SC16_GRAPHICS_MODE;\n\
             volatile unsigned int *x = (volatile unsigned int *)SC16_GRAPHICS_X;\n\
             volatile unsigned int *y = (volatile unsigned int *)SC16_GRAPHICS_Y;\n\
             volatile unsigned int *result = (volatile unsigned int *)SC16_GRAPHICS_RESULT;\n\
             *operation = SC16_GRAPHICS_OP_PIXEL;\n\
             *color = 23; *x = 5; *y = 7; *command = 1;\n\
             if (*status != 0 || *result != 1) return 11;\n\
             *operation = SC16_GRAPHICS_OP_MODE; *mode = 1; *command = 1;\n\
             if (*status != 0) return 12;\n\
             *mode = 0; *command = 1;\n\
             *x = 319; *y = 239; *operation = SC16_GRAPHICS_OP_RECT;\n\
             *color = 99; *command = 1;\n\
             if (*status != SC16_GRAPHICS_STATUS_INVALID_REQUEST || *result != 0) return 13;\n\
             return 1;\n\
         }",
        "high-graphics.c",
    )
    .unwrap()
    .unwrap();
    let image = assemble(&assembly, 0).unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.run(1_000_000).unwrap();
    assert!(!cpu.running);
    assert_eq!(cpu.registers[0], 1);
    let framebuffer = cpu.composited_framebuffer();
    assert_eq!(framebuffer[7 * 320 + 5], 23);
    assert_eq!(framebuffer[239 * 320 + 319], 0);
}

#[test]
fn guest_c_controls_the_high_pit_and_reads_fault_status() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned int *period = (volatile unsigned int *)SC16_INTERRUPT_PIT_PERIOD;\n\
                 volatile unsigned char *enable = (volatile unsigned char *)SC16_INTERRUPT_PIT_ENABLE;\n\
                 volatile unsigned char *valid = (volatile unsigned char *)SC16_FAULT_VALID;\n\
                 volatile unsigned long *address = (volatile unsigned long *)SC16_FAULT_ADDRESS;\n\
                 *enable = 0;\n\
                 *period = 60000;\n\
                 if (*period != 60000) return 11;\n\
                 if (*enable != 0) return 12;\n\
                 if (*valid != 0 || *address != 0UL) return 13;\n\
                 *period = 0;\n\
                 *enable = 1;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_captures_stable_clock_and_random_registers() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned char *capture = (volatile unsigned char *)SC16_CLOCK_CAPTURE;\n\
                 volatile unsigned long *random = (volatile unsigned long *)SC16_CLOCK_RANDOM;\n\
                 volatile unsigned long *time = (volatile unsigned long *)SC16_CLOCK_MONOTONIC_MILLIS;\n\
                 unsigned long first;\n\
                 unsigned long stamp;\n\
                 *capture = 3;\n\
                 first = *random;\n\
                 stamp = *time;\n\
                 if (first == 0UL || *random != first) return 11;\n\
                 *capture = 2;\n\
                 if (*random == first || *time != stamp) return 12;\n\
                 if (*capture != 0) return 13;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_reads_the_high_input_bank() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned char *key = (volatile unsigned char *)SC16_INPUT_KEY;\n\
                 volatile unsigned long *keys = (volatile unsigned long *)SC16_INPUT_KEYS;\n\
                 volatile unsigned int *x = (volatile unsigned int *)SC16_INPUT_MOUSE_X;\n\
                 volatile unsigned char *events = (volatile unsigned char *)SC16_INPUT_MOUSE_EVENTS;\n\
                 if (*key != 0 || *key != 0) return 11;\n\
                 if (*keys != 0UL || *x != 0 || *events != 0) return 12;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_writes_high_console_and_firmware_text_devices() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned char *data = (volatile unsigned char *)SC16_CONSOLE_DATA;\n\
                 volatile unsigned char *control = (volatile unsigned char *)SC16_CONSOLE_CONTROL;\n\
                 volatile unsigned char *text = (volatile unsigned char *)SC16_FIRMWARE_TEXT_DATA;\n\
                 volatile unsigned char *row = (volatile unsigned char *)SC16_FIRMWARE_TEXT_ROW;\n\
                 volatile unsigned char *col = (volatile unsigned char *)SC16_FIRMWARE_TEXT_COLUMN;\n\
                 volatile unsigned char *cmd = (volatile unsigned char *)SC16_FIRMWARE_TEXT_CONTROL;\n\
                 volatile unsigned char *enabled = (volatile unsigned char *)SC16_FIRMWARE_TEXT_ENABLED;\n\
                 *control = 254;\n\
                 *data = 65;\n\
                 if (*control != 2) return 11;\n\
                 *cmd = 2;\n\
                 *row = 24;\n\
                 *col = 52;\n\
                 *text = 9;\n\
                 if (*row != 24 || *col != 3 || *enabled != 1) return 12;\n\
                 *text = 66;\n\
                 if (*col != 4) return 13;\n\
                 *cmd = 1;\n\
                 if (*row != 0 || *col != 0) return 14;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_submits_and_stops_high_audio_commands() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned char *command = (volatile unsigned char *)SC16_AUDIO_COMMAND;\n\
                 volatile unsigned char *status = (volatile unsigned char *)SC16_AUDIO_STATUS;\n\
                 volatile unsigned char *channel = (volatile unsigned char *)SC16_AUDIO_CHANNEL;\n\
                 volatile unsigned int *frequency = (volatile unsigned int *)SC16_AUDIO_FREQUENCY;\n\
                 volatile unsigned int *wave = (volatile unsigned int *)SC16_AUDIO_WAVEFORM;\n\
                 volatile unsigned char *pending = (volatile unsigned char *)SC16_AUDIO_PENDING;\n\
                 *channel = 3;\n\
                 *frequency = 440;\n\
                 *wave = 2;\n\
                 *command = 1;\n\
                 if (*status != 0 || *pending != 8 || *frequency != 440) return 11;\n\
                 *wave = 3;\n\
                 *command = 1;\n\
                 if (*status != 3 || *pending != 8) return 12;\n\
                 *command = 2;\n\
                 if (*status != 0 || *command != 0) return 13;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_uses_bounded_high_network_staging_and_explicit_status() {
    assert_eq!(
        run_c(
            "#include <sc16_machine.h>\n\
             int main(void) {\n\
                 volatile unsigned char *command = (volatile unsigned char *)SC16_NETWORK_COMMAND;\n\
                 volatile unsigned char *status = (volatile unsigned char *)SC16_NETWORK_STATUS;\n\
                 volatile unsigned char *length = (volatile unsigned char *)SC16_NETWORK_LENGTH;\n\
                 volatile unsigned char *query = (volatile unsigned char *)SC16_NETWORK_QUERY;\n\
                 volatile unsigned int *argument = (volatile unsigned int *)SC16_NETWORK_ARGUMENT;\n\
                 volatile unsigned int *result = (volatile unsigned int *)SC16_NETWORK_RESULT;\n\
                 volatile unsigned char *title = (volatile unsigned char *)SC16_NETWORK_TITLE;\n\
                 volatile unsigned char *tx = (volatile unsigned char *)SC16_NETWORK_TRANSMIT;\n\
                 volatile unsigned char *rx = (volatile unsigned char *)SC16_NETWORK_RECEIVE;\n\
                 *command = 1;\n\
                 if (*status != 3 || *result != 0) return 11;\n\
                 title[15] = 65;\n\
                 tx[47] = 123;\n\
                 *length = 49;\n\
                 *command = 3;\n\
                 if (*status != 2 || tx[47] != 123 || title[15] != 65) return 12;\n\
                 *length = 48;\n\
                 *command = 3;\n\
                 if (*status != 4) return 13;\n\
                 *query = 1;\n\
                 *command = 5;\n\
                 if (*status != 0 || *result != 65535U || *command != 0) return 14;\n\
                 *query = 2;\n\
                 *argument = 65535U;\n\
                 *command = 5;\n\
                 if (*status != 0 || *result != 0 || *argument != 65535U) return 15;\n\
                 *command = 4;\n\
                 if (*status != 4 || rx[47] != 0) return 16;\n\
                 *command = 2;\n\
                 if (*status != 0) return 17;\n\
                 return 1;\n\
             }"
        ),
        1
    );
}

#[test]
fn guest_c_reads_writes_and_lists_files_through_high_filesystem_bank() {
    let root = std::env::temp_dir().join(format!(
        "sc16-c90-filesystem-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let disk = root.join("DISK0001");
    std::fs::create_dir_all(&disk).unwrap();
    std::fs::write(
        disk.join("disk.json"),
        r#"{"version":1,"id":"DISK0001","label":"Test","capacity_bytes":65536,"read_only":false,"protected_files":["protected.txt"]}"#,
    )
    .unwrap();
    std::fs::write(disk.join("file.txt"), b"abc").unwrap();
    std::fs::write(disk.join("protected.txt"), b"fixed").unwrap();
    let assembly = compile_minimal_program_to_assembly(
        "#include <sc16_machine.h>\n\
         int main(void) {\n\
             volatile unsigned char *command = (volatile unsigned char *)SC16_FILESYSTEM_COMMAND;\n\
             volatile unsigned char *status = (volatile unsigned char *)SC16_FILESYSTEM_STATUS;\n\
             volatile unsigned char *operation = (volatile unsigned char *)SC16_FILESYSTEM_OPERATION;\n\
             volatile unsigned int *length = (volatile unsigned int *)SC16_FILESYSTEM_LENGTH;\n\
             volatile unsigned int *result = (volatile unsigned int *)SC16_FILESYSTEM_RESULT;\n\
             volatile unsigned int *outlen = (volatile unsigned int *)SC16_FILESYSTEM_OUTPUT_LENGTH;\n\
             volatile unsigned char *path = (volatile unsigned char *)SC16_FILESYSTEM_PATH;\n\
             volatile unsigned char *data = (volatile unsigned char *)SC16_FILESYSTEM_DATA;\n\
             path[0] = 102; path[1] = 105; path[2] = 108; path[3] = 101;\n\
             path[4] = 46; path[5] = 116; path[6] = 120; path[7] = 116; path[8] = 0;\n\
             *operation = 0x42; *length = 3; *command = 1;\n\
             if (*status != 0 || *result != 3 || *outlen != 3) return 11;\n\
             if (data[0] != 97 || data[1] != 98 || data[2] != 99) return 12;\n\
             *operation = 0x43; data[0] = 88; data[1] = 89; data[2] = 90;\n\
             *command = 1;\n\
             if (*status != 0 || *result != 3) return 13;\n\
             *operation = 0x42; *command = 1;\n\
             if (*status != 0 || data[0] != 88 || data[1] != 89 || data[2] != 90) return 14;\n\
             *operation = 0x41; for (int i = 0; i < 128; i++) path[i] = 0; *command = 1;\n\
             if (*status != 0 || *result != 2 || data[0] != 102) return 15;\n\
             *operation = 0x47; path[0] = 112; path[1] = 114; path[2] = 111;\n\
             path[3] = 116; path[4] = 101; path[5] = 99; path[6] = 116;\n\
             path[7] = 101; path[8] = 100; path[9] = 46; path[10] = 116;\n\
             path[11] = 120; path[12] = 116; path[13] = 0; *command = 1;\n\
             if (*status != 0 || *result != 5 || data[0] != 0) return 16;\n\
             *operation = 0x43; *length = 1; data[0] = 88; *command = 1;\n\
             if (*status != 4 || *result != 0) return 17;\n\
             return 1;\n\
         }",
        "high-filesystem.c",
    )
    .unwrap()
    .unwrap();
    let image = assemble(&assembly, 0).unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.mount_folder_disk(&disk).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.run(1_000_000).unwrap();
    assert!(!cpu.running);
    assert_eq!(cpu.registers[0], 1);
    assert_eq!(std::fs::read(disk.join("file.txt")).unwrap(), b"XYZ");
    assert_eq!(std::fs::read(disk.join("protected.txt")).unwrap(), b"fixed");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn guest_c_reads_firmware_disks_through_bounded_inline_responses() {
    let root = std::env::temp_dir().join(format!(
        "sc16-c90-firmware-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("app")).unwrap();
    let assembly = compile_minimal_program_to_assembly(
        "#include <sc16_machine.h>\n\
         int main(void) {\n\
             volatile unsigned char *command = (volatile unsigned char *)SC16_FIRMWARE_COMMAND;\n\
             volatile unsigned char *status = (volatile unsigned char *)SC16_FIRMWARE_STATUS;\n\
             volatile unsigned int *index = (volatile unsigned int *)SC16_FIRMWARE_INDEX;\n\
             volatile unsigned int *count = (volatile unsigned int *)SC16_FIRMWARE_DISK_COUNT;\n\
             volatile unsigned int *length = (volatile unsigned int *)SC16_FIRMWARE_OUTPUT_LENGTH;\n\
             volatile unsigned char *output = (volatile unsigned char *)SC16_FIRMWARE_OUTPUT;\n\
             *command = 1;\n\
             if (*status != 0 || *count != 1 || *index != 0) return 11;\n\
             *command = 2;\n\
             if (*status != 0 || *length != 3 || output[0] != 97 || output[3] != 0) return 12;\n\
             *index = 65535U;\n\
             *command = 2;\n\
             if (*status != 1 || *length != 0 || output[0] != 97) return 13;\n\
             *index = 0;\n\
             *command = 2;\n\
             if (*status != 0 || *length != 3 || *command != 0) return 14;\n\
             return 1;\n\
         }",
        "high-firmware.c",
    )
    .unwrap()
    .unwrap();
    let image = assemble(&assembly, 0).unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.configure_disk(root.clone(), root.join("missing"))
        .unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.run(1_000_000).unwrap();
    assert!(!cpu.running);
    assert_eq!(cpu.registers[0], 1);
    std::fs::remove_dir_all(root).unwrap();
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
fn c90_two_word_call_arguments_use_documented_stack_abi() {
    let source = "int combine(int left, int right) { return left * 100 + right; } \
                 int main(void) { return combine(12, 34); }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-call-abi.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let combine_start = assembly
        .find("__c_function_combine:\n")
        .expect("combine function should be emitted");
    let main_start = assembly
        .find("__c_function_main:\n")
        .expect("main function should be emitted");
    let combine = &assembly[combine_start..main_start];
    let main = &assembly[main_start..];
    let call_start = main
        .find("CALLX __c_function_combine\n")
        .expect("combine call should be emitted");

    let left_load = combine
        .find("LDWS R1, 4\n")
        .expect("left parameter should load from SP + 4");
    let right_load = combine
        .find("LDWS R2, 6\n")
        .expect("right parameter should load from SP + 6");
    assert!(left_load < right_load, "{combine}");
    assert!(
        main[..call_start]
            .contains("LDI R4, 0x0022\n    PUSH R4\n    LDI R4, 0x000C\n    PUSH R4\n"),
        "{}",
        &main[..call_start]
    );
    assert!(main[call_start..].contains("ADJSP 4\n"), "{main}");
    assert_eq!(run_c(source), 1234);
}

#[test]
fn c90_three_word_call_arguments_use_documented_stack_abi() {
    let source = "int combine(int first, int second, int third) { \
                 return first * 100 + second * 10 + third; } \
                 int main(void) { if (combine(1, 2, 3) != 123) return 1; \
                 return combine(1, 2, -3); }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-call-abi.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let combine_start = assembly
        .find("__c_function_combine:\n")
        .expect("combine function should be emitted");
    let main_start = assembly
        .find("__c_function_main:\n")
        .expect("main function should be emitted");
    let combine = &assembly[combine_start..main_start];
    let main = &assembly[main_start..];
    let call_start = main
        .find("CALLX __c_function_combine\n")
        .expect("combine call should be emitted");

    for (register, offset) in [(1, 4), (2, 6), (3, 8)] {
        assert!(
            combine.contains(&format!("LDWS R{register}, {offset}\n")),
            "{combine}"
        );
    }
    assert!(
        main[..call_start]
            .contains("LDI R4, 0x0003\n    PUSH R4\n    LDI R4, 0x0002\n    PUSH R4\n    LDI R4, 0x0001\n    PUSH R4\n"),
        "{}",
        &main[..call_start]
    );
    assert!(main[call_start..].contains("ADJSP 6\n"), "{main}");
    assert_eq!(run_c(source), 117);
}

#[test]
fn c90_four_word_call_arguments_use_documented_stack_abi() {
    let source = "int check(int first, int second, int third, int fourth) { \
                 if (first != -1) return 1; if (second != 2) return 2; \
                 if (third != -3) return 3; if (fourth != 4) return 4; \
                 return first; } \
                 int main(void) { return check(-1, 2, -3, 4); }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-call-abi.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let combine_start = assembly
        .find("__c_function_check:\n")
        .expect("check function should be emitted");
    let main_start = assembly
        .find("__c_function_main:\n")
        .expect("main function should be emitted");
    let combine = &assembly[combine_start..main_start];
    let main = &assembly[main_start..];
    let call_start = main
        .find("CALLX __c_function_check\n")
        .expect("check call should be emitted");

    for (register, offset) in [(1, 4), (2, 6), (3, 8)] {
        assert!(
            combine.contains(&format!("LDWS R{register}, {offset}\n")),
            "{combine}"
        );
    }
    let register_save_end = main[..call_start]
        .find("PUSH R6\n")
        .map(|index| index + "PUSH R6\n".len())
        .expect("caller should save its registers before evaluating arguments");
    assert_eq!(
        main[register_save_end..call_start]
            .matches("PUSH R4\n")
            .count(),
        4,
        "{}",
        &main[register_save_end..call_start]
    );
    assert!(main[call_start..].contains("ADJSP 8\n"), "{main}");
    assert_eq!(run_c(source), -1);
}

#[test]
fn c90_five_word_call_arguments_use_documented_stack_abi() {
    let source = "int check(int first, int second, int third, int fourth, int fifth) { \
                 if (first != -1) return 1; if (second != 2) return 2; \
                 if (third != -3) return 3; if (fourth != 4) return 4; \
                 if (fifth != -5) return 5; return first; } \
                 int main(void) { return check(-1, 2, -3, 4, -5); }";
    let assembly = compile_minimal_program_to_assembly(source, "c90-call-abi.c")
        .expect("C source should parse")
        .expect("C source should compile");
    let check_start = assembly
        .find("__c_function_check:\n")
        .expect("check function should be emitted");
    let main_start = assembly
        .find("__c_function_main:\n")
        .expect("main function should be emitted");
    let check = &assembly[check_start..main_start];
    let main = &assembly[main_start..];
    let call_start = main
        .find("CALLX __c_function_check\n")
        .expect("check call should be emitted");

    for (register, offset) in [(1, 4), (2, 6), (3, 8)] {
        assert!(
            check.contains(&format!("LDWS R{register}, {offset}\n")),
            "{check}"
        );
    }
    let register_save_end = main[..call_start]
        .find("PUSH R6\n")
        .map(|index| index + "PUSH R6\n".len())
        .expect("caller should save its registers before evaluating arguments");
    assert_eq!(
        main[register_save_end..call_start]
            .matches("PUSH R4\n")
            .count(),
        5,
        "{}",
        &main[register_save_end..call_start]
    );
    assert!(main[call_start..].contains("ADJSP 10\n"), "{main}");
    assert_eq!(run_c(source), -1);
}

#[test]
fn c90_signed_char_call_argument_uses_documented_stack_abi() {
    let source = "int identity(signed char value) { return value; } \
                 int unsigned_identity(unsigned char value) { return value; } \
                 int main(void) { if (identity(-3) != -3) return 1; \
                 return unsigned_identity(250) == 250 ? 0 : 2; }";
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

    assert!(identity.contains("LD R1, R7\n"), "{identity}");
    assert!(
        main.contains("ADJSP -1\n    MOVSP R7\n    ST R7, R4\n"),
        "{main}"
    );
    assert!(
        main.contains("CALLX __c_function_identity\n    ADJSP 1\n"),
        "{main}"
    );
    assert!(
        main.contains("CALLX __c_function_unsigned_identity\n    ADJSP 1\n"),
        "{main}"
    );
    assert_eq!(run_c(source), 0);
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
fn c90_bit_field_layout_and_access() {
    let source = "struct Flags { unsigned int ready : 1; unsigned int mode : 3; \
                  signed int signed_value : 4; unsigned int : 0; \
                  unsigned int after_zero : 2; }; \
                  union Overlay { unsigned int low : 4; unsigned int high : 9; \
                  unsigned char byte; }; \
                  struct Flags flags; union Overlay overlay; \
                  int main(void) { return sizeof(flags) * 100 + sizeof(overlay); }";
    assert_eq!(run_c(source), 602, "bit-field aggregate sizes");

    let access = "struct Flags { unsigned int ready : 1; }; \
                  int main(void) { struct Flags flags = { 1 }; \
                  return flags.ready; }";
    assert_eq!(
        run_c(access),
        1,
        "automatic bit-field initialization and access"
    );
    assert_eq!(
        run_c(
            "typedef struct { int x; int y; } Point; \
               int main(void) { Point point = { 2, 5 }; return point.x; }"
        ),
        2,
        "automatic record initialization and access"
    );
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
        "int main(void) { return sizeof(short); }",
    ] {
        match compile_minimal_program(source, "unsupported-c90.c") {
            Ok(None) | Err(_) => {}
            Ok(Some(_)) => panic!("unsupported C90 facility was accepted: {source}"),
        }
    }
}
