use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::c_compiler::rv64::compile_to_assembly;
use suomicpu16::kiuas16::Kiuas16Machine;
use suomicpu16::kiuas16::runtime::run_headless;

fn execute(source: &str) -> (Kiuas16Machine, Vec<u8>) {
    execute_with_budget(source, 10000)
}

fn execute_with_budget(source: &str, steps: u64) -> (Kiuas16Machine, Vec<u8>) {
    let assembly = compile_to_assembly(source, "guest.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, steps, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}\n{assembly}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    (machine, output)
}

#[test]
fn compiled_c_recursion_eight_arguments_scopes_and_uart_execute() {
    let (machine, output) = execute(include_str!("fixtures/kiuas16_uart.c"));
    assert_eq!(output, b"KK=\n");
    assert_eq!(machine.core().register(10), Some(120));
}

#[test]
fn persistent_global_cursors_resolve_after_layout_and_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_cursors.c");
    let assembly = compile_to_assembly(source, "cursors.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(machine.core().register(10), Some(13));
    assert_eq!(output, [b'R', 6, 15, b'\n']);
    let mut bytes = [0; 56];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected_words = [
        0x80010028_u64,
        0x80010027,
        0x80010027,
        0x10000000,
        u64::from_le_bytes([0, 10, 11, 12, 13, 14, 15, 0]),
        6,
        0,
    ];
    let expected: Vec<u8> = expected_words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
    let mut results = [0; 40];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let expected: Vec<u8> = [
        0x80010020_u64,
        0x80010027,
        0x80010027,
        0x80010028,
        0x80010028,
    ]
    .iter()
    .flat_map(|word| word.to_le_bytes())
    .collect();
    assert_eq!(results.as_slice(), expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn global_pointer_initializers_resolve_final_data_and_bss_object_addresses() {
    let source = "
        int words[4];
        int *a = &words[2];
        int *b = words + 4;
        int *c = &words[3] - 2;
        int *d = 1 + words;
        volatile int *e = &words[0];
        int *f = (int *)(volatile int *)&words[1];
        int *g = sizeof(int) == 4 ? &words[3] : (int *)0;
        int *h = (int *)0;
        int *i;
        unsigned long longs[3];
        unsigned long *j = longs + 2;
        unsigned int value = 17;
        unsigned int *k = &value;
        unsigned char bytes[9] = {7};
        unsigned char *l = bytes + 8;
        int words[4] = {1, 2, 3, 4};
        int main(void) {
            if (a != &words[2] || *a != 3 || b - words != 4) return 1;
            if (c != words + 1 || d != c || f != c) return 2;
            if (e != words || g != &words[3] || *g != 4) return 3;
            if (h || i || sizeof(h) != 8) return 4;
            if ((unsigned long)j != (unsigned long)longs + 16UL) return 5;
            if (*k != 17 || l != &bytes[8] || *l != 0) return 6;
            *a = -7;
            if (words[2] != -7) return 7;
            return 37;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(37));
}

#[test]
fn global_pointer_updates_and_assignments_preserve_addresses_across_nested_calls() {
    let source = "
        long values[4] = {3, 5, 7, 11};
        long *cursor = values;
        long *saved;
        int depth;
        int next(void) { ++depth; return depth; }
        long *set(long *p) { saved = p; return p; }
        int shadow(long *cursor) { return *cursor; }
        int main(void) {
            if (*cursor++ != 3 || cursor != values + 1) return 1;
            if (*++cursor != 7 || *cursor-- != 7 || *--cursor != 3) return 2;
            cursor += next();
            if (*cursor != 5 || depth != 1) return 3;
            cursor = set(values + next());
            if (cursor != values + 2 || saved != cursor || depth != 2) return 4;
            cursor -= 2;
            if (*cursor != 3 || shadow(values + 3) != 11) return 5;
            if ((unsigned long)cursor != (unsigned long)values) return 6;
            return 41;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(41));
}

#[test]
fn explicit_integer_pointer_initializers_preserve_full_width_and_qualification() {
    let source = "
        unsigned char *high = (unsigned char *)0xfedcba9876543210UL;
        unsigned int *word = (unsigned int *)(unsigned int)0x80000000U;
        long *negative = (long *)-1;
        unsigned long *wide = (unsigned long *)0x10000000000UL;
        volatile unsigned long *qualified = (volatile unsigned long *)0x80000000UL;
        int *null = (int *)0;
        int main(void) {
            if ((unsigned long)high != 0xfedcba9876543210UL) return 1;
            if ((unsigned long)word != 0x80000000UL) return 2;
            if ((unsigned long)negative != 0xffffffffffffffffUL) return 3;
            if ((unsigned long)wide != 0x10000000000UL) return 4;
            *qualified = 0x1122334455667788UL;
            if (null || !qualified) return 5;
            return 43;
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(43));
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, 0x1122334455667788_u64.to_le_bytes());
    for (offset, number) in [
        (0, 0xfedcba9876543210_u64),
        (8, u64::MAX),
        (16, 0),
        (24, 0x80000000),
        (32, 0x10000000000),
        (40, 0x80000000),
    ] {
        machine
            .bus_mut()
            .read(0x80010000 + offset, &mut bytes)
            .unwrap();
        assert_eq!(bytes, number.to_le_bytes());
    }
}

#[test]
fn tentative_pointer_definitions_and_zero_storage_survive_poisoned_ram() {
    let source = "
        unsigned long data[2];
        unsigned long *cursor;
        unsigned long *cursor = data + 1;
        unsigned long *cursor;
        unsigned long *empty;
        int main(void) {
            if (empty || cursor != data + 1 || *cursor != 0) return 1;
            *cursor = 77;
            return data[1];
        }";
    let assembly = compile_to_assembly(source, "pointer-bss.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 40]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(77));
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected: Vec<u8> = [0x80010010_u64, 0, 77, 0, 0xa5a5a5a5a5a5a5a5]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
}

#[test]
fn invalid_static_pointer_initializers_and_pointer_object_addresses_diagnose() {
    for source in [
        "int *p = 0; int main(void) { return 0; }",
        "int n; unsigned int *p = &n; int main(void) { return 0; }",
        "int n; long *p = &n; int main(void) { return 0; }",
        "int n; int *p = &n; int *q = p; int main(void) { return 0; }",
        "int n; int *p = &n; int main(void) { return sizeof(&p); }",
        "int *p = &later; int later; int main(void) { return 0; }",
        "int n; int *p = &n; int *p = &n; int main(void) { return 0; }",
        "int *p; long *p; int main(void) { return 0; }",
        "int n; int *p = &n; int main(void) { return p(); }",
        "int *p = (int *)(1 / 0); int main(void) { return 0; }",
        "int n; int *p = &n + (1 / 0); int main(void) { return 0; }",
        "int n; int *p = n ? &n : (int *)0; int main(void) { return 0; }",
        "int n; int *p = (int *)(n + 1); int main(void) { return 0; }",
        "int n; volatile int *p = &n; int *q = p; int main(void) { return 0; }",
        "int n; int *p = (int *)(long)&n; int main(void) { return 0; }",
        "int n; int *p[2] = {&n}; int main(void) { return 0; }",
    ] {
        let error = compile_to_assembly(source, "pointer-static-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("pointer-static-invalid.c:"),
            "{source}: {error}"
        );
    }
}

#[test]
fn global_data_and_zero_storage_are_initialized_by_guest_startup() {
    let source = include_str!("fixtures/kiuas16_globals.c");
    let assembly = compile_to_assembly(source, "globals.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(machine.core().register(10), Some(148));
    assert_eq!(output, [b'G', 148, 4, 17, b'\n']);
    let mut results = [0; 32];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let addresses: Vec<u64> = results
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010008, 0x80010010, 0x80010000, 0x80010018]);
    let expected: [&[u8]; 4] = [
        &148_u64.to_le_bytes(),
        &4_u32.to_le_bytes(),
        &[7, 11, 13, 17],
        &[7, 11, 13, 17, 0, 0, 0, 0],
    ];
    for (address, expected) in addresses.iter().zip(expected) {
        assert!((0x80010000..0x80020000).contains(address));
        assert_eq!(address % 8, 0);
        let mut bytes = vec![0; expected.len()];
        machine.bus_mut().read(*address, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    let mut padding_and_guard = [0; 8];
    machine
        .bus_mut()
        .read(0x80010020, &mut padding_and_guard)
        .unwrap();
    assert_eq!(padding_and_guard, [0xa5; 8]);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn global_constant_initializers_preserve_lp64_types_and_exact_object_bytes() {
    let source = "
        unsigned char a = {511};
        int b = -7;
        unsigned int c = 0x80000001U;
        long d = -4294967297L;
        unsigned long e = 0xfedcba9876543210UL;
        int f[3] = {{-1}, 0x80000000U};
        unsigned char g[] = {256, 511, 2};
        unsigned long h = sizeof(h) + sizeof(f);
        unsigned long i = (0xffffffffU + 2U) + (0 ? 1 / 0 : 7UL);
        int main(void) {
            if (a != 255 || b != -7 || c != 0x80000001U) return 1;
            if (d != -4294967297L || e != 0xfedcba9876543210UL) return 2;
            if (f[0] != -1 || f[1] != -2147483647 - 1 || f[2] != 0) return 3;
            if (g[0] != 0 || g[1] != 255 || g[2] != 2 || sizeof(g) != 3) return 4;
            if (h != 20UL || i != 8UL) return 5;
            return 29;
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(29));
    let mut bytes = [0; 80];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let mut expected = [0; 80];
    expected[0] = 255;
    expected[8..12].copy_from_slice(&(-7_i32).to_le_bytes());
    expected[16..20].copy_from_slice(&0x80000001_u32.to_le_bytes());
    expected[24..32].copy_from_slice(&(-4294967297_i64).to_le_bytes());
    expected[32..40].copy_from_slice(&0xfedcba9876543210_u64.to_le_bytes());
    expected[40..44].copy_from_slice(&(-1_i32).to_le_bytes());
    expected[44..48].copy_from_slice(&0x80000000_u32.to_le_bytes());
    expected[56..59].copy_from_slice(&[0, 255, 2]);
    expected[64..72].copy_from_slice(&20_u64.to_le_bytes());
    expected[72..80].copy_from_slice(&8_u64.to_le_bytes());
    assert_eq!(bytes, expected);
}

#[test]
fn tentative_globals_persist_across_calls_and_local_shadowing() {
    let source = "
        int count;
        int count;
        int count = 3;
        int count;
        unsigned long values[3];
        unsigned long values[3];
        void add(int *target) { *target += 2; values[1] += 5; }
        int step(int depth) {
            add(&count);
            if (depth > 0) step(depth - 1);
            return count;
        }
        int shadow(int count) {
            count += 1;
            { int values[2] = {5}; if (sizeof(values) != 8) return 0; }
            return count + sizeof(values);
        }
        int main(void) {
            if (step(3) != 11 || values[1] != 20UL) return 1;
            if (shadow(7) != 32 || count != 11 || sizeof(values) != 24) return 2;
            if (count++ != 11 || ++count != 13) return 3;
            return count;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(13));
}

#[test]
fn global_data_only_and_bss_only_startup_do_not_depend_on_clean_ram() {
    for (declaration, check, expected) in [
        (
            "unsigned long state = 0xfedcba9876543210UL;",
            "return state == 0xfedcba9876543210UL;",
            1,
        ),
        ("unsigned long state;", "return state == 0;", 1),
        (
            "unsigned char state[9] = {7};",
            "return state[0] == 7 && state[1] == 0 && state[8] == 0;",
            1,
        ),
        (
            "unsigned char state[9];",
            "return state[0] == 0 && state[8] == 0;",
            1,
        ),
    ] {
        let source = format!("{declaration} int main(void) {{ {check} }}");
        let assembly = compile_to_assembly(&source, "startup.c").unwrap();
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 32]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{declaration}");
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(expected), "{declaration}");
        let mut untouched = [0; 8];
        machine.bus_mut().read(0x80010010, &mut untouched).unwrap();
        assert_eq!(untouched, [0xa5; 8]);
    }
}

#[test]
fn global_zero_storage_capacity_is_independent_of_stack_slot_limits() {
    let source = "
        unsigned char buffer[65536];
        int main(void) {
            if (buffer[0] != 0 || buffer[65535] != 0) return 1;
            buffer[0] = 7;
            buffer[65535] = 11;
            return buffer[0] + buffer[65535] + (sizeof(buffer) == 65536UL);
        }";
    let assembly = compile_to_assembly(source, "bss-capacity.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
    let report = run_headless(&mut machine, 300000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(19));
    let mut bytes = [0; 65536];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(bytes[0], 7);
    assert_eq!(bytes[65535], 11);
    assert_eq!(&bytes[1..65535], &[0; 65534]);
}

#[test]
fn invalid_global_definitions_initializers_visibility_and_capacity_diagnose() {
    for source in [
        "int a = 1; int a = 2; int main(void) { return 0; }",
        "int a; long a; int main(void) { return 0; }",
        "int a[2]; int a[3]; int main(void) { return 0; }",
        "int a[2]; int a; int main(void) { return 0; }",
        "int f; int f(void) { return 1; } int main(void) { return 0; }",
        "int f(void) { return 1; } int f; int main(void) { return 0; }",
        "int main; int main(void) { return 0; }",
        "int _start; int main(void) { return 0; }",
        "int **pointer; int main(void) { return 0; }",
        "volatile int state; int main(void) { return 0; }",
        "int *table[2]; int main(void) { return 0; }",
        "int a[2][3]; int main(void) { return 0; }",
        "int a = 1 / 0; int main(void) { return 0; }",
        "int a = 2147483647 + 1; int main(void) { return 0; }",
        "int a; int b = a; int main(void) { return 0; }",
        "int a = a + 1; int main(void) { return 0; }",
        "int a = missing; int main(void) { return 0; }",
        "int f(void) { return 1; } int a = f(); int main(void) { return 0; }",
        "int a; long b = (long)&a; int main(void) { return 0; }",
        "int a[2] = {1, 2, 3}; int main(void) { return 0; }",
        "int a[2] = {}; int main(void) { return 0; }",
        "int a[2] = 1; int main(void) { return 0; }",
        "int a = {1, 2}; int main(void) { return 0; }",
        "int main(void) { return later; } int later;",
        "int main(void) { return sizeof(later); } int later[3];",
        "int object; int main(void) { return object(); }",
        "unsigned char a[65537]; int main(void) { return 0; }",
        "unsigned char a[65536]; int b; int main(void) { return 0; }",
        "unsigned char a[65529]; unsigned char b; int main(void) { return 0; }",
        "unsigned long a[18446744073709551615UL]; int main(void) { return 0; }",
    ] {
        let error = compile_to_assembly(source, "globals-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("globals-invalid.c:"), "{source}: {error}");
    }
}

#[test]
fn oversized_global_rom_image_is_rejected_without_overwriting_output() {
    let source = "unsigned char bytes[65536] = {7}; int main(void) { return bytes[0]; }";
    let assembly = compile_to_assembly(source, "large-data.c").unwrap();
    assert!(assemble_boot_rom(&assembly).is_err());
    let files = Files::new(source);
    std::fs::write(files.0.join("rom.bin"), b"keep").unwrap();
    let build = files.compile();
    assert!(!build.status.success());
    assert!(!build.stderr.is_empty());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
}

#[test]
fn automatic_array_copy_executes_through_production_tools_with_exact_stack_bytes() {
    let source = include_str!("fixtures/kiuas16_arrays.c");
    let (mut machine, output) = execute_with_budget(source, 100000);
    assert_eq!(output, [b'A', 0x80, 0x7f, b'\n']);
    assert_eq!(machine.core().register(10), Some(256));
    let mut results = [0; 32];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let words: Vec<u64> = results
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(&words[2..], &[32640, 12]);
    assert_ne!(words[0], words[1]);
    let expected: Vec<u8> = (0..256).map(|i| i as u8).collect();
    for address in &words[..2] {
        assert!((0x8000fc00..0x80010000).contains(address));
        let mut bytes = [0; 256];
        machine.bus_mut().read(*address, &mut bytes).unwrap();
        assert_eq!(bytes.as_slice(), expected);
    }

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn array_initializers_use_contiguous_declared_widths_and_zero_fill() {
    let source = "
            int next(int *counter) { return ++*counter; }
            int main(void) {
                long before = 0x1234567887654321L;
                int counter = 0;
                unsigned char bytes[9] = {256, 511, {258}};
                int signed_words[4] = {-1, -2147483647 - 1};
                unsigned int words[] = {0xffffffffU, 0x80000000U,};
                long longs[3] = {-4294967297L, 0x1234567887654321L};
                unsigned long wide[2] = {0xffffffffffffffffUL};
                int calls[] = {next(&counter), next(&counter), next(&counter)};
                long after = 0x7777777766666666L;
                if (sizeof(bytes) != 9 || sizeof(words) != 8 || sizeof(longs) != 24) return 1;
                if (bytes[0] != 0 || bytes[1] != 255 || bytes[2] != 2) return 2;
                for (int i = 3; i < 9; ++i) if (bytes[i] != 0) return 3;
                if (signed_words[0] != -1 || signed_words[1] != -2147483647 - 1) return 4;
                if (signed_words[2] != 0 || signed_words[3] != 0) return 5;
                if (words[0] != 0xffffffffU || words[1] != 0x80000000U) return 6;
                if (longs[0] != -4294967297L || longs[1] != before || longs[2] != 0) return 7;
                if (wide[0] != 0xffffffffffffffffUL || wide[1] != 0) return 8;
                if (counter != 3 || calls[0] != 1 || calls[1] != 2 || calls[2] != 3) return 9;
                if (after != 0x7777777766666666L || before != 0x1234567887654321L) return 10;
                if ((unsigned long)longs % 8 != 0 || (unsigned long)words % 4 != 0) return 11;
                unsigned char *p = &bytes[0];
                if (bytes != p || &bytes[8] - bytes != 8L || sizeof(bytes + 0) != 8) return 12;
                return 31;
            }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(31));
}

#[test]
fn recursive_array_objects_and_shadowed_lengths_remain_independent() {
    let source = "
            int recurse(int depth) {
                int values[3] = {depth, depth + 1};
                int result = depth > 0 ? recurse(depth - 1) : 0;
                return result + values[0] + values[1] + values[2];
            }
            int length(unsigned char bytes[7]) { return sizeof(bytes); }
            int main(void) {
                unsigned char bytes[13] = {0};
                unsigned long count = sizeof(bytes);
                {
                    unsigned long bytes[2] = {9};
                    if (sizeof(bytes) != 16 || bytes[1] != 0) return 1;
                    count += sizeof(bytes);
                }
                if (sizeof(bytes) != 13 || count != 29 || length(bytes) != 8) return 2;
                int n = 0;
                while (n < 3) {
                    int reset[2] = {n};
                    if (reset[1] != 0) return 3;
                    reset[1] = 77;
                    ++n;
                }
                return recurse(4);
            }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(25));
}

#[test]
fn array_bounds_use_lp64_types_instead_of_legacy_constant_rules() {
    for (bound, expected) in [
        ("3", 3),
        ("2 + 3 * 4", 14),
        ("sizeof(long)", 8),
        ("sizeof(unsigned char) + sizeof(int *)", 9),
        ("(0xffffffffU + 2U)", 1),
        ("(0xffffffffU >> 31)", 1),
        ("(0xffffffffffffffffUL >> 63)", 1),
        ("(1UL << 40) / (1UL << 39)", 2),
        ("((unsigned char)258)", 2),
        ("(-7L / 3L + 5)", 3),
        ("(-7L % 3L + 5)", 4),
        ("(~0UL & 7)", 7),
        ("((0xffffffffU < 0L) ? 9 : 2)", 2),
        ("((0xffffffffU < -1L) ? 9 : 2)", 2),
        ("((0xffffffffU < -1) ? 9 : 2)", 2),
        ("(0 ? 1 / 0 : 3)", 3),
        ("((0 && 1 / 0) + 2)", 2),
        ("((1 || 1 / 0) + 2)", 3),
        ("(9 & 7)", 1),
        ("(9 ^ 7)", 14),
        ("(8 | 7)", 15),
        ("(!0 + 1)", 2),
        ("(-8 >> 2) + 5", 3),
    ] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ unsigned char bytes[{bound}]; return sizeof(bytes); }}"
        ));
        assert_eq!(machine.core().register(10), Some(expected), "{bound}");
    }
}

#[test]
fn array_capacity_counts_bytes_and_rejects_oversized_frames() {
    for (ty, length, size) in [
        ("unsigned char", 960, 960),
        ("int", 240, 960),
        ("unsigned int", 240, 960),
        ("long", 120, 960),
        ("unsigned long", 120, 960),
    ] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ {ty} a[{length}]; a[0] = 7; a[{length}-1] = 11;
                    return a[0] + a[{length}-1] + sizeof(a); }}"
        ));
        assert_eq!(machine.core().register(10), Some(size + 18));
        let error = compile_to_assembly(
            &format!("int main(void) {{ {ty} a[{length}+1]; return 0; }}"),
            "array-capacity.c",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("120 local/parameter slots"), "{error}");
    }
    let (machine, _) = execute_with_budget(
        "int main(void) { unsigned char a[960] = {7}; return a[0] + a[959]; }",
        20000,
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn invalid_array_declarations_initializers_and_uses_diagnose() {
    for body in [
        "int a[0];",
        "int a[-1];",
        "int a[2][3];",
        "int *a[2];",
        "int a[2] = {1, 2, 3};",
        "int a[2] = 1;",
        "int a[2] = {};",
        "int a[2] = {{1, 2}};",
        "int a[];",
        "int a[] = {};",
        "int a[2] = {1, missing};",
        "int a[2]; int b[2]; a = b;",
        "int a[2]; ++a;",
        "int a[2]; a++;",
        "int a[2]; a += 1;",
        "int a[2]; sizeof(&a);",
        "int n = 2; int a[n];",
        "unsigned char a[1 << 32];",
        "unsigned char a[1UL << 64];",
        "unsigned char a[1 << -1];",
        "unsigned char a[(-1 << 1) + 3];",
        "unsigned char a[1 / 0];",
        "unsigned char a[1 % 0];",
        "unsigned char a[2147483647 + 1];",
        "unsigned char a[(-2147483647 - 1) / -1];",
        "unsigned char a[(-2147483647 - 1) % -1 + 1];",
        "unsigned char a[(-9223372036854775807L - 1L) / -1L];",
        "unsigned char a[(-9223372036854775807L - 1L) % -1L + 1];",
        "unsigned long a[18446744073709551615UL];",
    ] {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let error = compile_to_assembly(&source, "array-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("array-invalid.c:"), "{body}: {error}");
    }
}

#[test]
fn addressable_stack_objects_survive_recursion_and_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_objects.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [b'S', 10, 255, b'\n']);
    assert_eq!(machine.core().register(10), Some(14));
    let mut bytes = [0; 9];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &10_u64.to_le_bytes());
    assert_eq!(bytes[8], 255);

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn aliased_scalar_locals_use_declared_width_after_pointer_writes() {
    let source = "
        int set(int *s, unsigned int *u, unsigned char *b, long *l, unsigned long *w) {
            *s = -7;
            *u = 0x80000001U;
            *b = 255;
            *l = -4294967297L;
            *w = 0xfedcba9876543210UL;
            return 0;
        }
        int main(void) {
            int s = 1;
            unsigned int u = 0;
            unsigned char b = 0;
            long l = 0;
            unsigned long w = 0;
            set(&s, &u, &b, &l, &w);
            if (s != -7 || u != 0x80000001U || b != 255) return 1;
            if (l != -4294967297L || w != 0xfedcba9876543210UL) return 2;
            if (s++ != -7 || ++s != -5) return 3;
            if (u++ != 0x80000001U || ++u != 0x80000003U) return 4;
            if (b++ != 255 || ++b != 1) return 5;
            if (l++ != -4294967297L || ++l != -4294967295L) return 6;
            if (w++ != 0xfedcba9876543210UL || ++w != 0xfedcba9876543212UL) return 7;
            int *p = &s;
            *p = 11;
            if (&*p != p || *&s != 11) return 8;
            return s;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(11));
}

#[test]
fn address_of_subscript_evaluates_once_without_loading_the_object() {
    let source = "
        unsigned long *base(int *count) {
            ++*count;
            return (unsigned long *)0x80000000UL;
        }
        int index(int *count) { ++*count; return 3; }
        int main(void) {
            int count = 0;
            unsigned long *p = &base(&count)[index(&count)];
            if (count != 2 || (unsigned long)p != 0x80000018UL) return 1;
            unsigned long *null = (unsigned long *)0;
            if (&*null != null) return 2;
            volatile unsigned int *unmapped = (volatile unsigned int *)0x12345000;
            if (&*unmapped != unmapped) return 3;
            return 17;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(17));
}

#[test]
fn lp64_sizeof_is_unsigned_long_and_never_evaluates_its_operand() {
    let source = "
        int side(int *n) { ++*n; return 1; }
        void nothing(void) { return; }
        int main(void) {
            int n = 0;
            int *p = &n;
            unsigned long sizes = sizeof(unsigned char) + sizeof(int)
                + sizeof(unsigned int) + sizeof(long) + sizeof(unsigned long)
                + sizeof(int *) + sizeof(volatile unsigned long *);
            if (sizes != 41UL || sizeof(n) != 4UL || sizeof(p) != 8UL) return 1;
            if (sizeof(++n) != 4 || sizeof(side(&n)) != 4) return 2;
            if (sizeof(*((long *)0)) != 8 || sizeof(*p++) != 4) return 3;
            if (sizeof(&n) != 8 || sizeof((unsigned char)1) != 1) return 4;
            if (sizeof((unsigned char)1 + (unsigned char)2) != 4) return 5;
            if (sizeof(1) - 5 > 0UL && n == 0 && p == &n) return 23;
            return 6;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(23));
}

#[test]
fn addressable_parameters_and_volatile_qualification_work_across_calls() {
    let source = "
        int set(volatile int *p) { *p = -3; return *p; }
        int args(int a, int b, int c, int d, int e, int f, int g, int h, int ninth) {
            set(&a);
            set(&ninth);
            return a + ninth + b + c + d + e + f + g + h;
        }
        int main(void) {
            int n = 0;
            int *p = &n;
            volatile int *v = p;
            if (v != p || p != v || v - p != 0L) return 1;
            if ((1 ? v : p) != p || (0 ? p : v) != v) return 2;
            int *explicit = (int *)v;
            if (explicit != p || (volatile int *)p != v) return 3;
            if (args(99, 2, 3, 4, 5, 6, 7, 8, 99) != 29) return 4;
            return set(p) == -3 && n == -3;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn invalid_address_of_sizeof_and_pointer_qualifications_diagnose() {
    let cases = [
        "int main(void) { return (long)&1; }",
        "int main(void) { int n = 0; return (long)&(n + 1); }",
        "int main(void) { int n = 0; return (long)&n++; }",
        "int main(void) { int *p; return (long)&p; }",
        "int main(void) { return sizeof(void); }",
        "void f(void) { return; } int main(void) { return sizeof(f()); }",
        "int main(void) { return sizeof(missing); }",
        "int main(void) { return sizeof(++1); }",
        "int main(void) { return sizeof(short); }",
        "int main(void) { int n; volatile int *v = &n; int *p = v; return 0; }",
        "int main(void) { int n; unsigned int *p = &n; return 0; }",
        "int main(void) { int n; long *p = &n; return 0; }",
        "int main(void) { int *p = 0; return 0; }",
    ];
    for source in cases {
        let error = compile_to_assembly(source, "objects-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("objects-invalid.c"), "{source}: {error}");
    }
}

#[test]
fn typed_pointer_offsets_differences_comparisons_and_casts_execute() {
    let cases = [
        ("return (unsigned long)(p + 3) == 0x8000000cUL;", 1),
        ("return (unsigned long)(3 + p) == 0x8000000cUL;", 1),
        ("return (unsigned long)(p - 3) == 0x7ffffff4UL;", 1),
        ("return (unsigned long)(p + -2) == 0x7ffffff8UL;", 1),
        ("return (unsigned long)(p - -2) == 0x80000008UL;", 1),
        ("return (p + 7) - p;", 7),
        ("return p - (p + 7);", -7),
        ("return p == p + 0;", 1),
        ("return p != p + 1;", 1),
        ("return p < p + 1;", 1),
        ("return p <= p;", 1),
        ("return p > p - 1;", 1),
        ("return p >= p;", 1),
        ("if (p) return 7; return 0;", 7),
        ("return !p;", 0),
        ("return !(volatile int *)0;", 1),
        ("return p == (volatile int *)0;", 0),
        ("return (unsigned int)p == 0x80000000U;", 1),
        ("return (long)p == 0x80000000L;", 1),
        ("return (unsigned char)(p + 3);", 12),
        (
            "return (unsigned long)(p + 4294967295U) == 0x47ffffffcUL;",
            1,
        ),
        ("return (p + 4294967295U) - p == 4294967295L;", 1),
        ("return (volatile int *)0x8000000000000000UL > p;", 1),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!(
            "int main(void) {{ volatile int *p = (volatile int *)0x80000000UL; {body} }}"
        ));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn pointer_indexing_uses_element_width_and_evaluates_operands_once() {
    let source = "
        volatile long *base(volatile unsigned int *count) {
            *count = *count + 1;
            return (volatile long *)0x80000010UL;
        }
        int index(volatile unsigned int *count) {
            *count = *count + 1;
            return -1;
        }
        int main(void) {
            volatile unsigned int *count = (volatile unsigned int *)0x80000000UL;
            volatile long *p = (volatile long *)0x80000010UL;
            volatile unsigned char *b = (volatile unsigned char *)0x80000020UL;
            *count = 0;
            base(count)[index(count)] = 0x1234567887654321L;
            if (*count != 2) return 1;
            if (p[-1] != 0x1234567887654321L) return 2;
            1[b] = 258;
            if (*(b + 1) != 2) return 3;
            b[0] = 255;
            if ((b + 2) - b != 2L) return 4;
            return b[0] + 1[b];
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(257));
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &2_u32.to_le_bytes());
    assert_eq!(&bytes[8..16], &0x1234567887654321_u64.to_le_bytes());
    assert_eq!(&bytes[32..34], &[255, 2]);
    assert_eq!(&bytes[16..32], &[0; 16]);
}

#[test]
fn independently_assembled_caller_checks_pointer_width_stride_and_difference() {
    let source = "
        volatile unsigned long *advance(volatile unsigned long *p, int offset) {
            return p + offset;
        }
        long distance(volatile unsigned long *p, volatile unsigned long *q) {
            return p - q;
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "pointer-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lpointer_entry\n", 1);
    assembly.push_str(
        ".Lpointer_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li s0, 77
         li a0, 1
         slli a0, a0, 40
         addi a0, a0, 64
         li a1, -3
         call advance
         mv s2, a0
         li a1, 1
         slli a1, a1, 40
         addi a1, a1, 64
         call distance
         mv s3, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(77));
    assert_eq!(machine.core().register(18), Some((1_u64 << 40) + 40));
    assert_eq!(machine.core().register(19), Some((-3_i64) as u64));
}

#[test]
fn compiled_pointer_buffer_copy_reports_checksum_and_exact_memory() {
    let source = include_str!("fixtures/kiuas16_memory.c");
    let (mut machine, output) = execute_with_budget(source, 100000);
    assert_eq!(output, [b'P', 0x80, 0x7f, b'\n']);
    assert_eq!(machine.core().register(10), Some(256));
    let mut source_bytes = [0; 256];
    let mut copied_bytes = [0; 256];
    let mut words = [0; 8];
    machine
        .bus_mut()
        .read(0x80000000, &mut source_bytes)
        .unwrap();
    machine
        .bus_mut()
        .read(0x80000200, &mut copied_bytes)
        .unwrap();
    machine.bus_mut().read(0x80000400, &mut words).unwrap();
    let expected: Vec<u8> = (0..256).map(|i| i as u8).collect();
    assert_eq!(source_bytes.as_slice(), expected);
    assert_eq!(copied_bytes, source_bytes);
    assert_eq!(&words[..4], &32640_u32.to_le_bytes());
    assert_eq!(&words[4..], &256_u32.to_le_bytes());

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn invalid_pointer_operations_have_source_located_diagnostics() {
    for body in [
        "return p + p;",
        "return 1 - p;",
        "return p * 2;",
        "return p >> 1;",
        "return p - b;",
        "return p == b;",
        "return p < 1;",
        "return p[ b ];",
        "return 1[2];",
        "p = b; return 0;",
        "p = 1; return 0;",
        "int n = p; return n;",
        "return (p + 1) = p;",
        "return &p;",
    ] {
        let source = format!(
            "int main(void) {{
                volatile int *p = (volatile int *)0x80000000UL;
                volatile unsigned char *b = (volatile unsigned char *)0x80000020UL;
                {body}
            }}"
        );
        let diagnostic = compile_to_assembly(&source, "pointer.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("pointer.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn typed_integer_promotions_conversions_and_signed_comparisons_execute() {
    let cases = [
        ("return -1 < 0;", 1),
        ("return -1L > 0L;", 0),
        ("return 4L >= 4;", 1),
        ("return 5 != 5L;", 0),
        ("return 5 == 5L;", 1),
        ("return ~0;", -1),
        ("return !0L;", 1),
        ("return !7L;", 0),
        ("unsigned char a = 255; return a + 1;", 256),
        ("unsigned char a = 258; return a;", 2),
        ("long a = 2147483647L + 1L; return a > 2147483647L;", 1),
        ("long a = 2147483647L + 1L; return (int)a < 0;", 1),
        ("return (3 & 1) | (4 ^ 2);", 7),
        ("int a = 7; { int a = 8; a = a + 2; } return a;", 7),
        ("int a = 3; return a = 9;", 9),
        ("return +7 * -3;", -21),
        ("if (0) return 1; else return 2;", 2),
        ("int a = 1; while (0) { a = 9; } return a;", 1),
        ("unsigned char a = 255; return -a;", -255),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn compound_assignments_and_increment_results_execute() {
    let cases = [
        ("int n = 3; return n++ + 10;", 13),
        ("int n = 3; return ++n + 10;", 14),
        ("int n = 3; int a = n--; return a * 10 + n;", 32),
        ("int n = 3; int a = --n; return a * 10 + n;", 22),
        ("unsigned char n = 255; int old = n++; return old + n;", 255),
        ("unsigned char n = 0; return --n;", 255),
        ("unsigned int n = 0xffffffffU; return ++n;", 0),
        ("unsigned char n = 250; return n += 10;", 4),
        ("int n = -17; return n /= 5;", -3),
        ("int n = -17; return n %= 5;", -2),
        ("unsigned int n = 0xffffffffU; return (n >>= 31);", 1),
        ("int n = -4; return n >>= 1UL;", -2),
        ("unsigned char n = 128; return n <<= 1;", 0),
        ("long n = -1; return (n += 2U);", 1),
        (
            "unsigned int n = 0xffffffffU; return (n /= 2UL) == 2147483647U;",
            1,
        ),
        (
            "unsigned long n = 0xffffffffffffffffUL; return (n %= 16UL);",
            15,
        ),
        ("int n = -17; return (n /= 5U) == 858993455U;", 1),
        (
            "int a = 1; int b = 2; a += (b *= 3); return a * 10 + b;",
            76,
        ),
        (
            "int n = 7; n *= 3; n -= 2; n &= 15; n ^= 3; return n |= 8;",
            8,
        ),
        (
            "volatile int *p = (volatile int *)0x80000000UL; volatile int *old = p++; return p - old;",
            1,
        ),
        (
            "volatile long *p = (volatile long *)0x80000010UL; --p; p += 3; p -= 1; return (unsigned long)p == 0x80000018UL;",
            1,
        ),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn compound_destinations_and_rhs_are_evaluated_once_across_calls() {
    let (mut machine, output) = execute(
        "volatile unsigned char *target(void) {
             *(volatile unsigned char *)0x10000000 = 65;
             return (volatile unsigned char *)0x80000000UL;
         }
         int rhs(void) {
             *(volatile unsigned char *)0x10000000 = 66;
             return 10;
         }
         int main(void) {
             volatile unsigned char *p = (volatile unsigned char *)0x80000000UL;
             *p = 250;
             int a = (*target() += rhs());
             int b = (*target())++;
             int c = ++(*target());
             return a * 100 + b * 10 + c;
         }",
    );
    assert_eq!(output, b"ABAA");
    assert_eq!(machine.core().register(10), Some(446));
    let mut bytes = [0; 2];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [6, 0]);
    let (machine, _) = execute(
        "int main(void) {
             volatile int *p = (volatile int *)0x80000000UL;
             p[0] = 4; p[1] = 7;
             int i = 0;
             p[i++] += 3;
             if (i != 1) return 1;
             if (p[0] != 7) return 2;
             *p++ += 5;
             return *p + p[-1];
         }",
    );
    assert_eq!(machine.core().register(10), Some(19));
}

#[test]
fn update_guest_executes_through_production_cli_and_preserves_memory() {
    let source = include_str!("fixtures/kiuas16_updates.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [138]);
    assert_eq!(machine.core().register(10), Some(138));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000100, &mut bytes).unwrap();
    let mut expected = std::array::from_fn::<_, 16, _>(|i| i as u8);
    expected[3] = 21;
    assert_eq!(bytes, expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, [138]);
}

#[test]
fn invalid_update_operands_diagnose() {
    for body in [
        "return 1++;",
        "return ++1;",
        "return (1 + 2) += 3;",
        "return p *= 2;",
        "return p += p;",
        "return p <<= 1;",
        "return p -= p;",
        "return n += p;",
        "return f()++;",
        "return ++f();",
        "return n += f();",
    ] {
        let source = format!(
            "void f(void) {{}} int main(void) {{
                volatile int *p = (volatile int *)0x80000000UL;
                int n = 1; {body}
            }}"
        );
        let diagnostic = compile_to_assembly(&source, "update.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("update.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn volatile_update_reads_uart_data_only_once() {
    let source = "
        int main(void) {
            volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
            *uart += 0;
            *uart = *uart;
            return 0;
        }";
    let assembly = compile_to_assembly(source, "uart-update.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"XY", &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"XY");
}

#[test]
fn nested_and_recursive_stack_calls_execute_through_production_tools() {
    let source = include_str!("fixtures/kiuas16_stack.c");
    let (machine, output) = execute(source);
    assert_eq!(machine.core().register(10), Some(139));
    assert_eq!(output, [139]);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, [139]);
}

#[test]
fn independent_caller_supplies_mixed_width_stack_parameters() {
    let source = "
        long stack(long a, long b, long c, long d, long e, long f, long g, long h,
                   unsigned int i, int j, unsigned char k, volatile long *p) {
            if (a+b+c+d+e+f+g+h != 36L) return 1;
            if (i != 4294967295L) return 2;
            if (j != -17L) return 3;
            if (k != 2) return 4;
            if ((unsigned long)p != 0x10000000000UL) return 5;
            return 77;
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "stack-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lstack_entry\n", 1);
    assembly.push_str(
        ".Lstack_entry:
         addi sp, sp, -48
         sd ra, 32(sp)
         li s0, 123
         li s1, 456
         li t0, -1
         sd t0, 0(sp)
         li t0, -17
         sd t0, 8(sp)
         li t0, 2
         sd t0, 16(sp)
         li t0, 1
         slli t0, t0, 40
         sd t0, 24(sp)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call stack
         ld ra, 32(sp)
         addi sp, sp, 48
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 77)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn independent_callee_checks_stack_offsets_extension_and_alignment() {
    for count in [9, 10, 11, 12, 31, 32] {
        let parameters = (0..count)
            .map(|i| format!("long p{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = (0..count)
            .map(|i| format!("twice({})", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "long probe({parameters}) {{ return 0; }}
             long twice(long n) {{ return n * 2; }}
             int main(void) {{ return 3 + probe({arguments}); }}"
        );
        let mut assembly = compile_to_assembly(&source, "stack.c").unwrap().replacen(
            "\nprobe:\n",
            "\n.Lunused_stack_probe:\n",
            1,
        );
        assembly.push_str("probe:\nandi t0, sp, 15\nbne t0, zero, .Lstack_bad\n");
        for i in 0..count {
            assembly.push_str(&format!("li t0, {}\n", (i + 1) * 2));
            if i < 8 {
                assembly.push_str(&format!("bne a{i}, t0, .Lstack_bad\n"));
            } else {
                assembly.push_str(&format!(
                    "ld t1, {}(sp)\nbne t1, t0, .Lstack_bad\n",
                    (i - 8) * 8
                ));
            }
        }
        assembly.push_str("li a0, 99\nret\n.Lstack_bad:\nli a0, -99\nret\n");
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{count}");
        assert!(report.waiting > 0, "{count}");
        assert_eq!(machine.core().register(10), Some(102), "{count}");
        assert_eq!(machine.core().register(2), Some(0x80010000), "{count}");
    }

    let source = "
        unsigned int probe(long a, long b, long c, long d, long e, long f, long g, long h, unsigned int i) { return 0; }
        int main(void) { return probe(1,2,3,4,5,6,7,8,0x80000000U) == 0xffffffffU; }";
    let mut assembly = compile_to_assembly(source, "extension.c")
        .unwrap()
        .replacen("\nprobe:\n", "\n.Lunused_extension:\n", 1);
    assembly.push_str("probe:\nld t0, 0(sp)\nli t1, -2147483648\nbne t0, t1, .Lextension_bad\nli a0, -1\nret\n.Lextension_bad:\nli a0, 0\nret\n");
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn scalar_parameter_limit_is_explicit_and_maximum_executes() {
    let parameters = (0..32)
        .map(|i| format!("long p{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (0..32)
        .map(|i| format!("{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let (machine, _) = execute(&format!(
        "long last({parameters}) {{ return p31 + p8; }}
         int main(void) {{ return last({arguments}); }}"
    ));
    assert_eq!(machine.core().register(10), Some(39));
    let diagnostic = compile_to_assembly(
        &format!(
            "long last({parameters}, long extra) {{ return extra; }}
         int main(void) {{ return 0; }}"
        ),
        "limit.c",
    )
    .unwrap_err()
    .to_string();
    assert!(diagnostic.contains("32 scalar parameters"), "{diagnostic}");
}

#[test]
fn logical_and_conditional_expressions_sequence_and_skip_side_effects() {
    let cases = [
        ("return 0 && (1 / 0);", 0),
        ("return 7 && 9;", 1),
        ("return -1L || 0;", 1),
        ("return 0UL || 0;", 0),
        ("return (volatile int *)0 && *(volatile int *)0;", 0),
        ("return (volatile int *)1 || *(volatile int *)0;", 1),
        ("int n = 0; int a = 0 && (n = 8); return n + a;", 0),
        ("int n = 0; int a = 1 || (n = 8); return n + a;", 1),
        (
            "int n = 0; int a = (n = 2) && (n = n + 3); return n + a;",
            6,
        ),
        ("int n = 0; int a = (n = 0) || (n = 7); return n + a;", 8),
        ("int n = 0; return (0 ? (n = 3) : (n = 9)) + n;", 18),
        ("int n = 0; return (1 ? (n = 3) : (n = 9)) + n;", 6),
        ("int n = 0; int a = (n = n + 1) ? 5 : 6; return n + a;", 6),
        ("return 1 ? 2 : 0 ? 3 : 4;", 2),
        ("return 0 ? 2 : 0 ? 3 : 4;", 4),
        ("return 0 ? 1 : 7 && 3;", 1),
        ("return (1 ? -1 : 1U) > 0;", 1),
        ("return (0 ? 1U : -1L) < 0;", 1),
        (
            "return (1 ? (unsigned char)255 : (unsigned char)1) + 1;",
            256,
        ),
        (
            "return *(0 ? (volatile int *)0 : (volatile int *)0x80000000UL);",
            0,
        ),
        ("return ((volatile int *)0 ? 1 : 4);", 4),
        ("return 3 * (0 || 9) + (1 && 2);", 4),
        ("return 1 + (0 ? 2L : 4294967295U) == 4294967296L;", 1),
        ("int n = 0; return (1 || (0 && (n = 9))) + n;", 1),
        ("int n = 0; return (0 && (1 || (n = 9))) + n;", 0),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }

    let (machine, output) = execute(
        "int mark(int n) { *(volatile unsigned char *)0x10000000 = n; return n; }
         void emit(int n) { *(volatile unsigned char *)0x10000000 = n; }
         int main(void) {
             int a = mark(65) && mark(66);
             int b = mark(67) || mark(68);
             int c = mark(69) ? mark(70) : mark(71);
             0 ? emit(72) : emit(73);
             return a + b + c;
         }",
    );
    assert_eq!(output, b"ABCEFI");
    assert_eq!(machine.core().register(10), Some(72));
}

#[test]
fn structured_loops_target_continue_and_break_and_preserve_scope() {
    let cases = [
        (
            "int n = 0; for (;;) { n = n + 1; if (n == 3) break; } return n;",
            3,
        ),
        ("int n = 0; do { n = n + 1; } while (0); return n;", 1),
        (
            "int n = 0; do { n = n + 1; continue; n = 9; } while (n < 3); return n;",
            3,
        ),
        (
            "int n = 0; while (n < 3) { n = n + 1; continue; n = 9; } return n;",
            3,
        ),
        ("int n = 7; do { break; n = 9; } while (1); return n;", 7),
        (
            "int i; int n = 0; for (i = 0; i < 5; i = i + 1) { if (i == 2) continue; n = n + i; } return n;",
            8,
        ),
        (
            "int n = 0; for (int i = 0; i < 3; i = i + 1) { n = n + i; } return n;",
            3,
        ),
        (
            "int i = 7; for (int i = 0; i < 2; i = i + 1) {} return i;",
            7,
        ),
        (
            "int i = 7; for (int i = 0; i < 2; i = i + 1) { int i = 9; } return i;",
            7,
        ),
        ("int i; for (i = 0; 0; i = 9) {} return i;", 0),
        (
            "int i = 0; int n = 0; for (; i < 3;) { i = i + 1; n = n + i; } return n;",
            6,
        ),
        (
            "int i = 0; int n = 0; while ((i = i + 1) < 4) { for (;;) { n = n + 1; break; } continue; } return n;",
            3,
        ),
        (
            "int n = 0; for (int i = 0; i < 2; i = i + 1) { for (int j = 0; j < 3; j = j + 1) { if (j == 1) continue; n = n + 1; } } return n;",
            4,
        ),
        ("for (;;) { return 12; }", 12),
        (
            "int n = 0; int updates = 0; for (; (n = n + 1) < 4; updates = updates + 1) { continue; } return n * 10 + updates;",
            43,
        ),
        ("int n = 0; for (; 1; n = 9) { break; } return n;", 0),
        (
            "int n = 0; do { continue; } while ((n = n + 1) < 3); return n;",
            3,
        ),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(machine.core().register(10), Some(expected), "{body}");
    }
}

#[test]
fn control_flow_errors_diagnose_even_in_unevaluated_paths() {
    for body in [
        "break;",
        "continue;",
        "if (0) break;",
        "while (0) {} break;",
        "for (int i = 0; i < 1; i = i + 1) {} return i;",
        "return 0 && unknown;",
        "return 1 || unknown;",
        "return 1 ? 2 : unknown;",
        "return 0 ? unknown : 2;",
        "return 1 ? 1 : (volatile int *)0;",
        "return 1 ? (volatile int *)0 : (volatile long *)0;",
        "return f() && 1;",
        "return 1 ? f() : 1;",
        "return f() ? 1 : 2;",
        "for (; f();) {}",
        "do {} while (f());",
        "for (int i = 0; i < 1; unknown()) {}",
        "for (int i = 0; i < 1; hidden = 9) { int hidden = 0; }",
        "while (0) { unknown(); }",
        "for (; 0;) { unknown(); }",
    ] {
        let source = format!("void f(void) {{}} int main(void) {{ {body} }}");
        let diagnostic = compile_to_assembly(&source, "control.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("control.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn compiled_polling_and_nested_loop_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_control.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [b'L', 18, b'\n']);
    assert_eq!(machine.core().register(10), Some(18));
    let mut status = [0; 4];
    machine.bus_mut().read(0x80000000, &mut status).unwrap();
    assert_eq!(status, 1_u32.to_le_bytes());
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn invalid_and_unsupported_c_is_not_silently_compiled() {
    let cases = [
        "#include <limits.h>\nint main(void) { return 0; }",
        "#define VALUE 1\nint main(void) { return VALUE; }",
        "int main() { return 0; }",
        "int main(void) { unsigned short a; return 0; }",
        "int main(void) { static int a; return 0; }",
        "int main(void) { const int a = 1; return 0; }",
        "int main(void) { return unknown; }",
        "int main(void) { int a; int a; return 0; }",
        "int main(void) { int a[2][3]; return 0; }",
        "int main(void) { return 1, 2; }",
        "int main(void) { return 1ULL; }",
        "int main(void) { return 18446744073709551616UL; }",
        "int main(void) { return; }",
        "void f(void) { return 1; } int main(void) { f(); return 0; }",
        "int f(int a) { return a; } int main(void) { return f(); }",
        "int f(void); int main(void) { return f(); }",
        "int main(void) { return f(); } int f(void) { return 1; }",
        "int f(int a); long f(int a) { return a; } int main(void) { return 0; }",
        "int main(void) { return 0; } int main(void) { return 1; }",
        "int main(void) { int f = 1; return f(); }",
        "int main(void) { return 0; unknown(); }",
        "int **a; int main(void) { return 0; }",
        "int main(void) { volatile unsigned char *p = 1; return 0; }",
        "int main(void) { volatile unsigned char *p = (volatile unsigned char *)1; return p + p; }",
        "int main(void) { switch (1) { case 1: return 0; } }",
        "int main(void) { return 2 = 1; }",
        "int main(void) { int a = 1; volatile int *p = (volatile int *)0x10000000; p = a; return 0; }",
    ];
    for source in cases {
        let error = compile_to_assembly(source, "invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid.c"), "{source}: {error}");
    }
}

#[test]
fn lp64_unsigned_literals_conversions_and_arithmetic_execute() {
    let cases = [
        ("return 2147483648 > 0;", 1),
        ("return 4294967295 == 4294967295L;", 1),
        ("return 4294967295U > 2147483647;", 1),
        ("return 0xffffffff == -1;", 1),
        ("return 037777777777 == 4294967295U;", 1),
        ("return 0x8000000000000000 > 0;", 1),
        ("return 18446744073709551615UL > 0L;", 1),
        ("return 18446744073709551615U == -1L;", 1),
        ("return 18446744073709551615 == -1L;", 1),
        ("return 0xffffffffffffffffL == -1L;", 1),
        ("return 0XfEdCbA9876543210uL == 18364758544493064720UL;", 1),
        ("return (unsigned int)-1L == 4294967295U;", 1),
        ("return (unsigned long)-1 == 18446744073709551615UL;", 1),
        ("return (long)4294967295U == 4294967295L;", 1),
        ("return -1L < 1U;", 1),
        ("return -1 < 1U;", 0),
        ("return -1L < 1UL;", 0),
        ("return 4294967295U + 1U == 0U;", 1),
        ("return 0UL - 1UL == 18446744073709551615UL;", 1),
        ("return -4294967295U == 1U;", 1),
        ("return ~0U == 4294967295U;", 1),
        ("return ~0UL == 18446744073709551615UL;", 1),
        ("return 0x80000000U * 2U == 0;", 1),
        ("return (unsigned char)0xffffffffffffffffUL;", 255),
        ("return (int)0xffffffffUL;", -1),
        (
            "return 0x8000000000000000UL / 2UL == 0x4000000000000000UL;",
            1,
        ),
        ("return 0xffffffffffffffffUL % 16UL;", 15),
        ("return 4294967295U / 2U == 2147483647;", 1),
        ("return 4294967295U % 2U;", 1),
        ("return -17 / 5;", -3),
        ("return -17 % 5;", -2),
        ("return -17L / 5U;", -3),
        ("return -17L % 5U;", -2),
        ("return 1U << 31 == 0x80000000U;", 1),
        ("return 0x80000000U >> 31;", 1),
        ("return -4 >> 1;", -2),
        ("return -4L >> 1;", -2),
        ("return 0xffffffffffffffffUL >> 63;", 1),
        ("return 1UL << 63 == 0x8000000000000000UL;", 1),
        ("return 1U << 31UL == 0x80000000U;", 1),
        ("unsigned char a = 255; return a << 1;", 510),
        ("return -1 < 4294967295U;", 0),
        ("return -1 <= 4294967295U;", 1),
        ("return 0xffffffffffffffffUL >= 1;", 1),
        ("return 0xffffffffffffffffUL != 0;", 1),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn unsigned_volatile_words_doublewords_and_nested_calls_execute() {
    let (mut machine, output) = execute(
        "unsigned long combine(unsigned int low, unsigned long high) {
                 return high | low;
             }
             unsigned int narrow(unsigned long value) { return value; }
             int main(void) {
                 volatile unsigned int *word = (volatile unsigned int *)0x80000000UL;
                 volatile unsigned long *wide = (volatile unsigned long *)0x80000008UL;
                 *word = 0xffffffffU;
                 *wide = combine(narrow(0x12345678ffffffffUL), 0xabcdef0100000000UL);
                 if (*word != 4294967295L) return 1;
                 if (*wide != 0xabcdef01ffffffffUL) return 2;
                 *(volatile unsigned char *)0x10000000 = (*wide >> 32) & 255U;
                 return *word / 0x10000000U;
             }",
    );
    assert_eq!(output, [1]);
    assert_eq!(machine.core().register(10), Some(15));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &u32::MAX.to_le_bytes());
    assert_eq!(&bytes[4..8], &[0; 4]);
    assert_eq!(&bytes[8..], &0xabcdef01ffffffff_u64.to_le_bytes());
}

#[test]
fn wide_literal_materialization_matches_independent_host_values() {
    let cases = [
        ("0UL", 0),
        ("2147483647L", 0x7fffffff),
        ("2147483648", 0x80000000),
        ("0xffffffffU", 0xffffffff),
        ("0x100000000UL", 0x100000000),
        ("0x1234567887654321UL", 0x1234567887654321),
        ("0x7ffffffffffff7ffUL", 0x7ffffffffffff7ff),
        ("0x7ffffffffffff800UL", 0x7ffffffffffff800),
        ("9223372036854775807L", 0x7fffffffffffffff),
        ("9223372036854775808", 0x8000000000000000),
        ("0x8000000000000800UL", 0x8000000000000800),
        ("0xaaaaaaaaaaaaaaaaUL", 0xaaaaaaaaaaaaaaaa),
        ("0xffffffff7fffffffUL", 0xffffffff7fffffff),
        ("0xffffffff80000000UL", 0xffffffff80000000),
        ("0xffffffffffffffffUL", u64::MAX),
    ];
    for (literal, expected) in cases {
        let source = format!(
            "unsigned long wide(void) {{ return {literal}; }}
             int main(void) {{ return 0; }}"
        );
        let assembly = compile_to_assembly(&source, "literal.c").unwrap().replacen(
            "call main\n",
            "call wide\n",
            1,
        );
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 1000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{literal}");
        assert!(report.waiting > 0, "{literal}");
        assert_eq!(machine.core().register(10), Some(expected), "{literal}");
    }
}

#[test]
fn unsigned_integer_abi_interoperates_with_independent_assembly() {
    let source = "
            unsigned int echo(unsigned int value) { return value; }
            unsigned int probe(unsigned int a, unsigned int b) { return 0; }
            unsigned int call_probe(void) { return probe(0x80000000U, echo(0xffffffffU)); }
            int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "unsigned-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lunsigned_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lunsigned_entry:
             addi sp, sp, -16
             sd ra, 0(sp)
             li a0, -1
             call echo
             mv s2, a0
             call call_probe
             mv s3, a0
             ld ra, 0(sp)
             addi sp, sp, 16
             ret
             probe:
             andi t0, sp, 15
             bne t0, zero, .Lunsigned_bad
             li t0, -2147483648
             bne a0, t0, .Lunsigned_bad
             li t0, -1
             bne a1, t0, .Lunsigned_bad
             li a0, -2
             ret
             .Lunsigned_bad:
             li a0, 7
             ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
}

#[test]
fn invalid_lp64_integer_literals_diagnose_without_truncation() {
    for literal in [
        "18446744073709551616UL",
        "0x10000000000000000",
        "1ULL",
        "1UU",
        "1LL",
        "09",
    ] {
        let source = format!("int main(void) {{ return {literal}; }}");
        let error = compile_to_assembly(&source, "literal.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("literal.c"), "{literal}: {error}");
    }
}
#[test]
fn declared_forward_calls_and_eight_byte_local_slot_limits_are_explicit() {
    let (machine, _) = execute(
        "long next(long value);
             int main(void) { return (int)next(4L); }
             long next(long value) { return value + 1L; }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let declarations: String = (0..120)
        .map(|i| format!("long value{i} = {i}L;\n"))
        .collect();
    let (machine, _) = execute(&format!(
        "int main(void) {{ {declarations} return (int)value119; }}"
    ));
    assert_eq!(machine.core().register(10), Some(119));
    let error = compile_to_assembly(
        &format!("int main(void) {{ {declarations} long extra; return 0; }}"),
        "limits.c",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("120 local/parameter slots"), "{error}");
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);

impl Files {
    fn new(source: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "kiuas16-compiler-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("guest.c"), source).unwrap();
        Self(directory)
    }

    fn compile(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_kiuas16-cc"))
            .arg(self.0.join("guest.c"))
            .arg(self.0.join("rom.bin"))
            .output()
            .unwrap()
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn c_source_to_production_rom_cli_executes_guest_storage_without_host_queue_setup() {
    let files = Files::new(include_str!("fixtures/kiuas16_storage.c"));
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    let mut disk = vec![0; 1024];
    disk[512..].fill(b'R');
    std::fs::write(files.0.join("disk.raw"), &disk).unwrap();
    for writable in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"));
        command
            .arg("--rom")
            .arg(files.0.join("rom.bin"))
            .arg("--root")
            .arg(files.0.join("disk.raw"))
            .args(["--steps", "20000", "--deny-traps"]);
        if writable {
            command.arg("--root-writable");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            result.stdout,
            [b'C', 0, b'R', u8::from(!writable), 0, 1, 3, 0]
        );
        let mut expected = disk.clone();
        if writable {
            expected[512] = b'W';
        }
        assert_eq!(std::fs::read(files.0.join("disk.raw")).unwrap(), expected);
    }
}

#[test]
fn compiler_cli_preserves_source_and_existing_output_on_invalid_input() {
    let files = Files::new("int main(void) { return missing; }");
    let source = files.0.join("guest.c");
    let original = std::fs::read(&source).unwrap();
    std::fs::write(files.0.join("rom.bin"), b"keep").unwrap();
    let result = files.compile();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("guest.c:1"));
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
    let result = Command::new(env!("CARGO_BIN_EXE_kiuas16-cc"))
        .arg(&source)
        .arg(&source)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overwrite"));
    assert_eq!(std::fs::read(&source).unwrap(), original);

    let oversized = format!("int main(void) {{ {} return 0; }}", "1 + 2;\n".repeat(3000));
    std::fs::write(&source, oversized).unwrap();
    let result = files.compile();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("exceeds boot ROM size"));
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
}

#[test]
fn volatile_loads_store_widths_and_assignment_results_execute() {
    let (machine, _) = execute(
        "long ram(void) { return 2147483647L + 1L; }
             int main(void) {
                 volatile long *wide = (volatile long *)ram();
                 volatile int *word = (volatile int *)(ram() + 8L);
                 volatile unsigned char *byte = (volatile unsigned char *)(ram() + 12L);
                 *wide = 2147483647L + 2L;
                 *word = -17;
                 *byte = 258;
                 if (*wide != 2147483647L + 2L) return 1;
                 if (*word != -17) return 2;
                 if (*byte != 2) return 3;
                 return *byte = 259;
             }",
    );
    assert_eq!(machine.core().register(10), Some(3));
}

#[test]
fn independently_assembled_caller_checks_compiled_parameter_and_saved_register_rules() {
    let source = "
        long weighted(long a, long b, long c, long d, long e, long f, long g, long h) {
            return a + b*2L + c*3L + d*4L + e*5L + f*6L + g*7L + h*8L;
        }
        long sign(int value) { return value; }
        unsigned char clip(int value) { return value; }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "abi.c").unwrap().replacen(
        "call main\n",
        "call .Labi_entry\n",
        1,
    );
    assembly.push_str(
        ".Labi_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li s0, 77
         li s1, 88
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call weighted
         mv s2, a0
         li a0, -17
         call sign
         mv s3, a0
         li a0, 258
         call clip
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    for (register, value) in [
        (2, 0x80010000),
        (8, 77),
        (9, 88),
        (18, 204),
        (19, (-17_i64) as u64),
        (20, 2),
    ] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn independently_assembled_callee_checks_compiled_call_alignment_and_arguments() {
    let source = "
        long probe(long a, long b, long c, long d, long e, long f, long g, long h) { return 0L; }
        long twice(long a) { return a*2L; }
        int main(void) { return (int)probe(1L, 2L, 3L, 4L, 5L, 6L, 7L, twice(4L)); }";
    let mut assembly = compile_to_assembly(source, "abi.c").unwrap().replacen(
        "\nprobe:\n",
        "\n.Lunused_probe:\n",
        1,
    );
    assembly.push_str(
        "probe:
         andi t0, sp, 15
         bne t0, zero, .Lbad_alignment
         add a0, a0, a1
         add a0, a0, a2
         add a0, a0, a3
         add a0, a0, a4
         add a0, a0, a5
         add a0, a0, a6
         add a0, a0, a7
         ret
         .Lbad_alignment:
         li a0, -1
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert_eq!(machine.core().register(10), Some(36));
    assert_eq!(machine.core().register(2), Some(0x80010000));
}
