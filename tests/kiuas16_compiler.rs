use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::c_compiler::rv64::compile_to_assembly;
use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{FileSectorMedia, Kiuas16Machine};

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
fn compiled_text_literals_character_buffers_and_uart_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_text.c");
    let assembly = compile_to_assembly(source, "text.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 128]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"Kiuas16: ROM ready\n\x80\xff\n");
    assert_eq!(machine.core().register(10), Some(19));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 24];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    for (address, expected) in
        addresses
            .chunks_exact(8)
            .zip([&b"\x80\xff\0"[..], &b"\0"[..], &b"OK\0\0"[..]])
    {
        let address = u64::from_le_bytes(address.try_into().unwrap());
        let mut bytes = vec![0; expected.len()];
        machine.bus_mut().read(address, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    let mut table_address = [0; 8];
    machine
        .bus_mut()
        .read(0x80000018, &mut table_address)
        .unwrap();
    let mut entries = [0; 32];
    machine
        .bus_mut()
        .read(u64::from_le_bytes(table_address), &mut entries)
        .unwrap();
    assert_eq!(&entries[24..32], &[0; 8]);
    for (entry, expected) in
        entries[..24]
            .chunks_exact(8)
            .zip([&b"Kiuas16: \0"[..], &b"ROM \0"[..], &b"ready\n\0"[..]])
    {
        let mut bytes = vec![0; expected.len()];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(entry.try_into().unwrap()), &mut bytes)
            .unwrap();
        assert_eq!(bytes, expected);
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
fn character_promotions_aliases_updates_and_volatile_widths_execute() {
    let cases = [
        ("char c = 255; return c + 1;", 256),
        ("signed char c = 255; return c;", -1),
        ("signed char c = 128; return c;", -128),
        ("signed char c = -128; return c / 2;", -64),
        ("signed char c = -1; return c < 1U;", 0),
        ("signed char c = -128; return c >> 1;", -64),
        ("signed char c = 127; int old = c++; return old + c;", -1),
        ("signed char c = -128; return --c;", 127),
        ("signed char c = 127; return c += 1;", -128),
        ("char c = 255; return ++c;", 0),
        (
            "signed char c = 0; signed char *p = &c; *p = 255; return c;",
            -1,
        ),
        ("char c = 0; char *p = &c; *p = 255; return c;", 255),
        (
            "return sizeof(char) + sizeof(signed char) + sizeof(char *);",
            10,
        ),
        ("signed char c = -1; return sizeof(c + c);", 4),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
    let (mut machine, _) = execute(
        "int main(void) {
            volatile signed char *s = (volatile signed char *)0x80000000UL;
            volatile char *u = (volatile char *)0x80000001UL;
            *s = 255; *u = 128;
            if (*s != -1 || *u != 128) return 1;
            *s += 2; *u += 127;
            return *s + *u;
        }",
    );
    assert_eq!(machine.core().register(10), Some(256));
    let mut bytes = [0; 3];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 255, 0]);
}

#[test]
fn string_array_initializers_preserve_escapes_embedded_zero_and_padding() {
    let source = r#"
        char global[] = {"A\0B"};
        signed char signed_bytes[] = {"\x80\377"};
        unsigned char unsigned_bytes[2] = "\200\377";
        int main(void) {
            char local[] = {"a" "b"};
            char exact[2] = "CD";
            char padded[5] = {"E"};
            char empty[] = "";
            if (sizeof(global) != 4 || sizeof(local) != 3 || sizeof(empty) != 1) return 1;
            if (sizeof(signed_bytes) != 3 || sizeof(unsigned_bytes) != 2) return 2;
            if (sizeof("A\0B") != 4 || global[1] || global[2] != 'B' || global[3]) return 3;
            if (signed_bytes[0] != -128 || signed_bytes[1] != -1 || signed_bytes[2]) return 4;
            if (unsigned_bytes[0] != 128 || unsigned_bytes[1] != 255) return 5;
            if (local[0] != 'a' || local[1] != 'b' || local[2] || empty[0]) return 6;
            if (exact[0] != 'C' || exact[1] != 'D' || padded[0] != 'E' || padded[4]) return 7;
            char bound[sizeof("XY")] = "XY";
            if (sizeof(bound) != 3 || bound[2]) return 8;
            return 47;
        }"#;
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(47));
}

#[test]
fn string_addresses_have_static_lifetime_and_use_final_layout() {
    let source = r#"
        char *saved = "ABC" + 1;
        volatile char *qualified = &"ABC"[2];
        char *choose = 0 ? (char *)0 : "Z";
        char *get(int depth) {
            char scratch[16] = "scratch";
            if (depth) return get(depth - 1);
            return "ABC";
        }
        int main(void) {
            char *p = get(5);
            char overwrite[16] = "overwrite";
            if (p[0] != 'A' || p[1] != 'B' || p[2] != 'C' || p[3]) return 1;
            if (*saved != 'B' || *qualified != 'C' || *choose != 'Z') return 2;
            if (sizeof(*p) != 1 || sizeof(p) != 8 || sizeof("ABC") != 4) return 3;
            if ("ABC"[1] != 'B' || *&"ABC"[2] != 'C') return 4;
            return 49;
        }"#;
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(49));
}

#[test]
fn character_abi_interoperates_with_independent_register_and_stack_calls() {
    let source = "
        signed char echo(signed char value) { return value; }
        char plain(char value) { return value; }
        signed char probe(signed char a, char b, unsigned char c,
                          int d, int e, int f, int g, int h, signed char i, char j) {
            return 0;
        }
        signed char caller(void) {
            return probe(-128, 255, 255, 0, 0, 0, 0, 0, echo(-1), 128);
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "character-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lcharacter_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lcharacter_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, 255
         call echo
         mv s2, a0
         li a0, -1
         call plain
         mv s3, a0
         call caller
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lcharacter_bad
         li t0, -128
         bne a0, t0, .Lcharacter_bad
         li t0, 255
         bne a1, t0, .Lcharacter_bad
         bne a2, t0, .Lcharacter_bad
         ld t1, 0(sp)
         li t0, -1
         bne t1, t0, .Lcharacter_bad
         ld t1, 8(sp)
         li t0, 128
         bne t1, t0, .Lcharacter_bad
         li a0, -2
         ret
         .Lcharacter_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(255));
    assert_eq!(machine.core().register(20), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_character_and_string_uses_diagnose_without_output_overwrite() {
    for source in [
        r#"int main(void) { char a[2] = "ABC"; return 0; }"#,
        r#"char a[2] = "ABC"; int main(void) { return 0; }"#,
        r#"int main(void) { int a[4] = "ABC"; return 0; }"#,
        r#"int a[4] = "ABC"; int main(void) { return 0; }"#,
        r#"int main(void) { unsigned char *p = "ABC"; return 0; }"#,
        r#"int main(void) { signed char *p = "ABC"; return 0; }"#,
        r#"int main(void) { char *p = 1; return 0; }"#,
        r#"int main(void) { char *p = "ABC"; signed char *s = p; return 0; }"#,
        r#"int main(void) { volatile char *p = "ABC"; char *q = p; return 0; }"#,
        r#"int main(void) { "ABC" = "DEF"; return 0; }"#,
        r#"int main(void) { ++"ABC"; return 0; }"#,
        r#"char *p = &"ABC"; int main(void) { return 0; }"#,
        r#"char full[65536]; char *p = "X"; int main(void) { return 0; }"#,
        "char a; signed char a; int main(void) { return 0; }",
        "int main(void) { volatile char c; return 0; }",
        r#"int main(void) { return L"ABC"[0]; }"#,
    ] {
        let error = compile_to_assembly(source, "text-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("text-invalid.c"), "{source}: {error}");
    }
    let oversized = format!(
        "char *p = \"{}\"; int main(void) {{ return 0; }}",
        "A".repeat(65536)
    );
    let error = compile_to_assembly(&oversized, "text-invalid.c")
        .unwrap_err()
        .to_string();
    assert!(error.contains("65536-byte RAM budget"), "{error}");
    let files = Files::new(r#"int main(void) { char a[2] = "ABC"; return 0; }"#);
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
    let oversized_rom = format!(
        "char *p = \"{}\"; int main(void) {{ return p[0]; }}",
        "A".repeat(61440)
    );
    let files = Files::new(&oversized_rom);
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    let build = files.compile();
    assert!(!build.status.success());
    assert!(
        String::from_utf8_lossy(&build.stderr).contains("ROM"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
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
        "int *p = 1; int main(void) { return 0; }",
        "int n; unsigned int *p = &n; int main(void) { return 0; }",
        "int n; long *p = &n; int main(void) { return 0; }",
        "int n; int *p = &n; int *q = p; int main(void) { return 0; }",
        "int n; int *p = &n; int **q = &p; int main(void) { return sizeof(&q); }",
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
        "int n; int **p[2] = {(int **)&n}; int main(void) { return 0; }",
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
        "int ***pointer; int main(void) { return 0; }",
        "volatile int state; int main(void) { return 0; }",
        "int **table[2]; int main(void) { return 0; }",
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
        "int **a[2];",
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
        "int main(void) { int **p; return (long)&p; }",
        "int main(void) { return sizeof(void); }",
        "void f(void) { return; } int main(void) { return sizeof(f()); }",
        "int main(void) { return sizeof(missing); }",
        "int main(void) { return sizeof(++1); }",
        "int main(void) { return sizeof(float); }",
        "int main(void) { int n; volatile int *v = &n; int *p = v; return 0; }",
        "int main(void) { int n; unsigned int *p = &n; return 0; }",
        "int main(void) { int n; long *p = &n; return 0; }",
        "int main(void) { int *p = 1; return 0; }",
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
fn const_pointer_reads_preserve_all_integer_widths_and_qualifier_spellings() {
    for (ty, value, expected) in [
        ("char", "255", 255u64),
        ("signed char", "255", u64::MAX),
        ("unsigned char", "255", 255),
        ("short", "65535", u64::MAX),
        ("unsigned short", "65535", 65535),
        ("int", "-7", (-7i64) as u64),
        ("unsigned int", "0x80000001U", 0x80000001),
        ("long", "-4294967297L", (-4294967297i64) as u64),
        ("unsigned long", "0xfedcba9876543210UL", 0xfedcba9876543210),
    ] {
        for qualified in [
            format!("const {ty}"),
            format!("{ty} const"),
            format!("const volatile {ty}"),
            format!("volatile {ty} const"),
        ] {
            let source = format!(
                "{ty} object[] = {{ [1] = {value} }};
                 {qualified} *view = object + 1;
                 unsigned long read({qualified} *p) {{ return *p; }}
                 int main(void) {{
                     {qualified} *local = view;
                     {qualified} *table[] = {{ [1] = local }};
                     if (table[0] || &*local != view || &table[1][0] != local) return 1;
                     if (sizeof(local) != 8 || sizeof(*local) != sizeof({ty})) return 2;
                     *(volatile unsigned long *)0x80000000UL = read(table[1]);
                     *(volatile unsigned long *)0x80000008UL = (unsigned long)object;
                     return local - object;
                 }}"
            );
            let (mut machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(1), "{source}");
            let mut report = [0; 16];
            machine.bus_mut().read(0x80000000, &mut report).unwrap();
            assert_eq!(
                u64::from_le_bytes(report[..8].try_into().unwrap()),
                expected,
                "{source}"
            );
            let size = match ty {
                "char" | "signed char" | "unsigned char" => 1,
                "short" | "unsigned short" => 2,
                "int" | "unsigned int" => 4,
                _ => 8,
            };
            let mut bytes = vec![0; size * 2];
            machine
                .bus_mut()
                .read(
                    u64::from_le_bytes(report[8..].try_into().unwrap()),
                    &mut bytes,
                )
                .unwrap();
            assert_eq!(&bytes[..size], vec![0; size]);
            assert_eq!(&bytes[size..], &expected.to_le_bytes()[..size]);
        }
    }
}

#[test]
fn const_pointer_tables_aliases_and_generic_qualification_unions_execute() {
    let (machine, output) = execute(
        "typedef const char *text;
         typedef text *texts;
         typedef const volatile char *read_only_mmio;
         typedef volatile char *writable_mmio;
         char bytes[] = \"AB\";
         text table[] = { [2] = bytes + 1, [0] = \"X\" };
         text choose(int n, text a, text b) { return n ? a : b; }
         text ninth(int a,int b,int c,int d,int e,int f,int g,int h,text p) { return p; }
         int main(void) {
             texts cursor = table;
             text p = choose(0, 0, ninth(1,2,3,4,5,6,7,8,cursor[2]));
             const void *generic = p;
             const char *q = generic;
             volatile char *writable = bytes;
             read_only_mmio combined = 1 ? p : writable;
             const volatile void *both = 0 ? generic : writable;
             const volatile char *roundtrip = both;
             writable_mmio alias = writable;
             *alias = 'C';
             cursor[1] = q;
             ++cursor;
             if (cursor - table != 1 || &*cursor != &table[1]) return 1;
             if (*cursor != p || combined != q || *roundtrip != 'C') return 2;
             if (choose(1, 0, p) != 0 || (0 ? p : 0) != 0) return 3;
             *(volatile unsigned char *)0x10000000UL = *table[0];
             *(volatile unsigned char *)0x10000000UL = *cursor[0];
             *(volatile unsigned char *)0x10000000UL = *roundtrip;
             char *explicit_view = (char *)p;
             *explicit_view = 'D';
             return *p;
         }",
    );
    assert_eq!(output, b"XBC");
    assert_eq!(machine.core().register(10), Some(b'D' as u64));
}

#[test]
fn const_volatile_views_skip_unevaluated_reads_and_read_uart_once() {
    let source =
        "int calls;
         const volatile unsigned char *uart(void) { ++calls; return (const volatile unsigned char *)0x10000000UL; }
         int main(void) {
             sizeof(*uart());
             0 && *uart();
             1 || *uart();
             const volatile unsigned char *p = 1 ? uart() : 0;
             unsigned char a = *p++;
             --p;
             unsigned char b = *p;
             *(volatile unsigned char *)0x10000000UL = a;
             *(volatile unsigned char *)0x10000000UL = b;
             return calls;
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "const-uart.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"ABC", &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"AB");
    assert_eq!(machine.core().register(10), Some(1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut next = [0; 1];
    machine.bus_mut().read(0x10000000, &mut next).unwrap();
    assert_eq!(next, *b"C");
}

#[test]
fn const_atomic_loads_execute_without_changing_objects_or_neighbor_bytes() {
    for (ty, value, expected, size) in [
        ("int", "0x80000001U", (-2147483647i64) as u64, 4),
        ("unsigned int", "0x80000001U", 0x80000001, 4),
        ("long", "-4294967297L", (-4294967297i64) as u64, 8),
        (
            "unsigned long",
            "0xfedcba9876543210UL",
            0xfedcba9876543210,
            8,
        ),
    ] {
        let source = format!(
            "{ty} object = {value};
             int main(void) {{
                 const volatile {ty} *p = &object;
                 {ty} old = __atomic_load_n(p, 2);
                 if (old != object) return 1;
                 *(volatile unsigned long *)0x80000000UL = old;
                 *(volatile unsigned long *)0x80000008UL = object;
                 {ty} *mutable = &object;
                 __atomic_store_n(mutable, 7, 3);
                 if (__atomic_load_n(p, 5) != 7) return 2;
                 return object;
             }}"
        );
        let rom =
            assemble_boot_rom(&compile_to_assembly(&source, "const-atomic.c").unwrap()).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 16]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(7));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut bytes = [0; 16];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        assert_eq!(&bytes[..8], &expected.to_le_bytes());
        assert_eq!(&bytes[8..], &expected.to_le_bytes());
        machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
        assert_eq!(&bytes[..size], &7u64.to_le_bytes()[..size]);
        assert_eq!(&bytes[size..8], vec![0; 8 - size]);
        assert_eq!(&bytes[8..], &[0xa5; 8]);
    }
}

#[test]
fn const_pointer_recursive_stack_views_and_two_level_returns_execute() {
    let (machine, _) = execute(
        "const int **pass(const int **p) { return p; }
         const int *read(const int **p) { return *p; }
         int recurse(int n, const int *parent) {
             int local = n;
             const int *view = &local;
             const int **table = pass(&view);
             int result = *read(table) + (parent ? *parent : 0);
             if (n) result += recurse(n - 1, view);
             if (view != &local || *view != n) return 99;
             return result;
         }
         int main(void) { return recurse(4, 0); }",
    );
    assert_eq!(machine.core().register(10), Some(20));
}

#[test]
fn const_pointer_register_and_stack_abi_interoperate_with_independent_caller() {
    let mut assembly = compile_to_assembly(
        "const int *probe(int a,int b,int c,int d,int e,int f,int g,int h,const int *p) {
             if (a + b + c + d + e + f + g + h != 36 || *p != -17) return 0;
             return p + 1;
         }
         int main(void) { return 0; }",
        "const-abi.c",
    )
    .unwrap()
    .replacen("call main\n", "call .Lconst_entry\n", 1);
    assembly.push_str(
        ".Lconst_entry:
         addi sp, sp, -32
         sd ra, 24(sp)
         li s0, 123
         li s1, 456
         li t0, 1
         slli t0, t0, 31
         sd t0, 0(sp)
         li t1, -17
         sw t1, 0(t0)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call probe
         ld ra, 24(sp)
         addi sp, sp, 32
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 0x80000004)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn const_writes_qualifier_loss_and_unsupported_objects_preserve_existing_rom() {
    for body in [
        "*p = 1;",
        "p[0] = 1;",
        "*p += 1;",
        "++*p;",
        "(*p)++;",
        "p[0]--;",
        "*(&*p) = 1;",
        "*(&p[0]) = 1;",
        "int *q = p;",
        "int *q = 0; q = p;",
        "consume(p);",
        "int *q = 1 ? p : &object;",
        "int *q = 0 ? p : 0;",
        "void *q = p;",
        "const void *v = p; int *q = v;",
        "volatile int *q = p;",
        "const int **q = &mutable;",
        "int **q = &p;",
        "const volatile int **q = &p;",
        "int **q = 1 ? &p : &mutable;",
        "**(&p) = 1;",
        "sizeof((*p)++);",
        "if (0) *p = 1;",
        "__atomic_store_n(p, 1, 3);",
        "__atomic_exchange_n(p, 1, 2);",
        "const int value = 1;",
        "const int a[2] = {1};",
        "int *const q = &object;",
        "sizeof(int *const);",
        "(int *const)&object;",
        "typedef int *ip; const ip q = &object;",
        "typedef int *ip; sizeof(const ip);",
        "typedef int *ip; (const ip)&object;",
        "typedef const int *cp; volatile cp q = p;",
        "const void *v = p; *v;",
        "const void *v = p; v++;",
    ] {
        let source = format!(
            "int object;
             int consume(int *p) {{ return *p; }}
             int main(void) {{
                 int *mutable = &object;
                 const int *p = mutable;
                 {body}
                 return 0;
             }}"
        );
        let diagnostic = compile_to_assembly(&source, "const.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("const.c:"), "{body}: {diagnostic}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "int object; const int *p = &object; int *bad(void) { return p; } int main(void) { return 0; }",
        "int object; const int *p = &object; int *q = p; int main(void) { return 0; }",
        "int object; const int *p = &object; int *a[] = { [0] = p }; int main(void) { return 0; }",
        "const int object = 1; int main(void) { return 0; }",
        "typedef int *ip; const ip q; int main(void) { return 0; }",
        "typedef const int *cp; typedef const cp bad; int main(void) { return 0; }",
        "const int *p; extern int *p; int main(void) { return 0; }",
        "int f(const int *p); int f(int *p) { return 0; } int main(void) { return 0; }",
    ] {
        assert!(
            compile_to_assembly(source, "const-global.c").is_err(),
            "{source}"
        );
    }
}

#[test]
fn designated_integer_arrays_preserve_widths_sparse_zeroes_and_inferred_bounds() {
    for (ty, size, last) in [
        ("char", 1, 35u64),
        ("signed char", 1, 35),
        ("unsigned char", 1, 35),
        ("short", 2, 291),
        ("unsigned short", 2, 291),
        ("int", 4, 291),
        ("unsigned int", 4, 291),
        ("long", 8, 291),
        ("unsigned long", 8, 291),
    ] {
        let source = format!(
            "{ty} global[] = {{ [4] = 0x123, [1] = {{7}}, 9 }};
             int main(void) {{
                 {ty} local[] = {{ [4] = 0x123, [1] = {{7}}, 9 }};
                 *(volatile unsigned long *)0x80000000UL = (unsigned long)global;
                 *(volatile unsigned long *)0x80000008UL = (unsigned long)local;
                 return sizeof(global) + sizeof(local);
             }}"
        );
        let rom = assemble_boot_rom(&compile_to_assembly(&source, "sparse.c").unwrap()).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
        machine.bus_mut().write(0x8000fc00, &[0xa5; 1024]).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0, "{ty}: {:?}", report.last_trap);
        assert!(report.waiting > 0);
        assert!(output.is_empty());
        assert_eq!(machine.core().register(10), Some(10 * size as u64));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut addresses = [0; 16];
        machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
        let expected: Vec<_> = [0u64, 7, 9, 0, last]
            .iter()
            .flat_map(|number| number.to_le_bytes()[..size].to_vec())
            .collect();
        for address in addresses.chunks_exact(8) {
            let address = u64::from_le_bytes(address.try_into().unwrap());
            let mut actual = vec![0; expected.len()];
            machine.bus_mut().read(address, &mut actual).unwrap();
            assert_eq!(actual, expected, "{ty}");
        }
        let padded = expected.len().div_ceil(8) * 8;
        let mut neighbor = [0; 8];
        machine
            .bus_mut()
            .read(0x80010000 + padded as u64, &mut neighbor)
            .unwrap();
        assert_eq!(neighbor, [0xa5; 8], "{ty}");
    }
}

#[test]
fn designated_pointer_tables_resolve_addresses_null_holes_and_stack_arguments() {
    let (mut machine, output) = execute(
        "static char bytes[] = \"AB\";
         static char *table[] = { [3] = \"X\", [0] = bytes + 1, 0 };
         char *ninth(int a,int b,int c,int d,int e,int f,int g,int h,char *p) { return p; }
         int main(void) {
             char *local[4] = { [2] = ninth(1,2,3,4,5,6,7,8,table[3]), [0] = bytes };
             void *generic[] = { [2] = local[2], [0] = bytes };
             if (sizeof(table) != 32 || sizeof(generic) != 24) return 1;
             if (table[1] || table[2] || local[1] || local[3] || generic[1]) return 2;
             if (generic[0] != bytes || generic[2] != local[2]) return 3;
             *(volatile unsigned long *)0x80000000UL = (unsigned long)table;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)(bytes + 1);
             *(volatile unsigned char *)0x10000000UL = *table[0];
             *(volatile unsigned char *)0x10000000UL = *local[0];
             *(volatile unsigned char *)0x10000000UL = *local[2];
             return 42;
         }",
    );
    assert_eq!(output, b"BAX");
    assert_eq!(machine.core().register(10), Some(42));
    let mut addresses = [0; 16];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let mut entries = [0; 32];
    machine
        .bus_mut()
        .read(
            u64::from_le_bytes(addresses[..8].try_into().unwrap()),
            &mut entries,
        )
        .unwrap();
    assert_eq!(&entries[..8], &addresses[8..]);
    assert_eq!(&entries[8..24], &[0; 16]);
    let mut text = [0; 2];
    machine
        .bus_mut()
        .read(
            u64::from_le_bytes(entries[24..].try_into().unwrap()),
            &mut text,
        )
        .unwrap();
    assert_eq!(text, *b"X\0");
}

#[test]
fn designated_initializers_evaluate_once_in_source_order_across_recursive_calls() {
    let (machine, output) = execute(
        "int mark(int n) { *(volatile unsigned char *)0x10000000UL = n; return n; }
         int recurse(int n) {
             int values[3] = { [2] = n, [0] = n ? recurse(n - 1) : 0 };
             return values[0] + values[1] + values[2];
         }
         int main(void) {
             int values[] = { [4] = mark(65), [0] = mark(66), mark(67) };
             int total = 0;
             for (int i = 0; i < 2; ++i) {
                 int again[3] = { [2] = mark(68 + i), [0] = recurse(4) };
                 total += again[0] + again[1] + again[2];
             }
             if (sizeof(values) != 20 || values[2] || values[3]) return 1;
             return total + values[0] + values[1] + values[4];
         }",
    );
    assert_eq!(output, b"ABCDE");
    assert_eq!(machine.core().register(10), Some(355));
}

#[test]
fn designated_indices_use_lp64_constants_aliases_enumerators_and_continuations() {
    let (machine, _) = execute(
        "typedef unsigned long index_t;
         enum { LAST = 7 };
         unsigned char global[] = { [LAST] = 255, [sizeof(long) - 6] = 17, 18,
             [(index_t)-1 + 1] = 19 };
         int main(void) {
             int local[] = { [(unsigned char)258] = 3, [1 ? 4 : 1/0] = 5, 6,
                 [0 && 1/0] = 7 };
             if (sizeof(global) != 8 || sizeof(local) != 24) return 1;
             if (global[1] || global[4] || global[5] || global[6]) return 2;
             if (local[1] || local[3]) return 3;
             return global[0] + global[2] + global[3] + global[7]
                 + local[0] + local[2] + local[4] + local[5];
         }",
    );
    assert_eq!(machine.core().register(10), Some(330));
}

#[test]
fn designated_pointer_arrays_preserve_pointee_widths_and_volatile_qualification() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        for qualifier in ["", "volatile "] {
            let source = format!(
                "{ty} object[2] = {{ [1] = 42 }};
                 {qualifier}{ty} *table[] = {{ [2] = object + 1 }};
                 int main(void) {{
                     {qualifier}{ty} *local[] = {{ [2] = table[2] }};
                     if (table[0] || table[1] || local[0] || local[1]) return 1;
                     if (sizeof(table) != 24 || sizeof(local) != 24) return 2;
                     *local[2] += 1;
                     return object[1] + *table[2];
                 }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(86), "{source}");
        }
    }
}

#[test]
fn designated_array_limits_execute_and_invalid_forms_preserve_existing_rom() {
    let (machine, _) = execute_with_budget(
        "unsigned char full[] = { [57343] = 99 };
         unsigned char tail[8192];
         int main(void) { return sizeof(full) == 57344 && full[0] == 0 && full[57343] == 99 && tail[8191] == 0; }",
        1000000,
    );
    assert_eq!(machine.core().register(10), Some(1));
    let (machine, _) = execute(
        "int main(void) { unsigned char full[] = { [959] = 99 }; return sizeof(full) == 960 && full[0] == 0 && full[959] == 99; }",
    );
    assert_eq!(machine.core().register(10), Some(1));
    for source in [
        "int a[3] = { [3] = 1 }; int main(void) { return 0; }",
        "int a[3] = { [2] = 1, 2 }; int main(void) { return 0; }",
        "int a[] = { [-1] = 1 }; int main(void) { return 0; }",
        "int a[] = { [1 / 0] = 1 }; int main(void) { return 0; }",
        "int a[] = { [65536] = 1 }; int main(void) { return 0; }",
        "int a[] = { [0xffffffffffffffffUL] = 1 }; int main(void) { return 0; }",
        "int a[] = { [2] = 1, [2] = 2 }; int main(void) { return 0; }",
        "int a[] = { [2] = 1, [1] = 2, 3 }; int main(void) { return 0; }",
        "int a[] = { [0][1] = 1 }; int main(void) { return 0; }",
        "int a[] = { .field = 1 }; int main(void) { return 0; }",
        "int a[] = { [0] = { [0] = 1 } }; int main(void) { return 0; }",
        "int a[] = { [0] 1 }; int main(void) { return 0; }",
        "int a[] = { [0] = }; int main(void) { return 0; }",
        "int a = { [0] = 1 }; int main(void) { return 0; }",
        "int *a[] = { [0] = 1 }; int main(void) { return 0; }",
        "int **a[] = { [0] = 0 }; int main(void) { return 0; }",
        "int a[1][1] = { [0] = {1} }; int main(void) { return 0; }",
        "int a[] = { [0] = unknown }; int main(void) { return 0; }",
        "int a[] = { [0] = 1, [0] = unknown }; int main(void) { return 0; }",
        "long a[] = { [8192] = 1 }; int main(void) { return 0; }",
        "int main(void) { int n = 0; int a[] = { [n] = 1 }; return 0; }",
        "int f(void) { return 0; } int main(void) { int a[] = { [f()] = 1 }; return 0; }",
        "int f(void) { return 0; } int main(void) { int a[] = { [1 ? 0 : f()] = 1 }; return 0; }",
        "int main(void) { int n = 0; int a[] = { [1 ? 0 : n] = 1 }; return 0; }",
        "int main(void) { int n = 0; int a[] = { [sizeof(n)] = 1 }; return 0; }",
        "int main(void) { unsigned char a[] = { [960] = 1 }; return 0; }",
        "int main(void) { if (0) { int a[1] = { [1] = 1 }; } return 0; }",
        "int main(void) { int a[] = { [0] = 1, [0] = 2 }; return 0; }",
        "int main(void) { volatile int *a[] = { [0] = (volatile long *)0 }; return 0; }",
    ] {
        let diagnostic = compile_to_assembly(source, "designator.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("designator.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty(), "{source}");
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    let files = Files::new(
        "unsigned char full[] = { [65535] = 99 }; int main(void) { return full[65535]; }",
    );
    std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(
        std::fs::read(files.0.join("rom.bin")).unwrap(),
        b"previous ROM"
    );
}

#[test]
fn null_pointer_constants_convert_all_supported_pointer_categories() {
    for ty in [
        "char *",
        "signed char *",
        "unsigned char *",
        "short *",
        "unsigned short *",
        "int *",
        "unsigned int *",
        "long *",
        "unsigned long *",
        "void *",
        "volatile int *",
        "volatile void *",
        "int **",
        "void **",
        "volatile int **",
        "const int *",
        "const volatile int *",
        "const void *",
        "const volatile void *",
        "const int **",
        "const volatile int **",
        "_Bool *",
        "volatile _Bool *",
        "const _Bool *",
        "const volatile _Bool *",
        "_Bool **",
        "const _Bool **",
    ] {
        for zero in [
            "0",
            "0L",
            "0UL",
            "1 - 1",
            "sizeof(long) - 8",
            "(int)0x100000000UL",
            "(void *)0",
        ] {
            let source = format!(
                "{ty} global = {zero};
                 {ty} pass({ty} p) {{ return p; }}
                 {ty} empty(void) {{ return {zero}; }}
                 int main(void) {{
                     {ty} p = {zero};
                     if (p != 0 || global != 0 || pass({zero}) != 0 || empty() != 0) return 99;
                     p = {zero};
                     return (p == {zero}) + ({zero} == p) + (p == 0);
                 }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(3), "{source}");
        }
    }
}

#[test]
fn null_conditionals_preserve_object_pointer_types_and_skip_calls() {
    let (machine, _) = execute(
        "static int calls;
         int *choose(int *p) { ++calls; return p; }
         int main(void) {
             int n = 7;
             int *p = 1 ? &n : 0;
             int *q = 0 ? 0 : &n;
             if (*(1 ? p : (void *)0) != 7) return 99;
             if (*(0 ? (void *)0 : q) != 7) return 98;
             volatile int *v = 1 ? &n : 0UL;
             int **table = 0 ? &p : 0;
             if (table != 0 || *v != 7) return 97;
             p = 0 ? choose(&n) : 0;
             q = 1 ? 0 : choose(&n);
             if (p || q || calls) return 96;
             return sizeof(*(1 ? &n : (void *)0)) + (p == (void *)0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let (machine, _) = execute(
        "static int calls;
         int *empty(void) { ++calls; return 0; }
         int accepts(int a,int b,int c,int d,int e,int f,int g,int h,int *p) {
             return a+b+c+d+e+f+g+h + (p == 0);
         }
         int main(void) {
             if (empty() != 0 || 0 != empty()) return 99;
             return accepts(1,2,3,4,5,6,7,8,0) + calls;
         }",
    );
    assert_eq!(machine.core().register(10), Some(39));
}

#[test]
fn explicit_null_table_entries_and_conditional_addresses_survive_poisoned_startup() {
    let source = "
        enum { NIL = 0 };
        int n;
        int *table[6] = { NIL, 1 ? &n : 0, 0 ? &n : 0, (void *)0, 0UL };
        int main(void) {
            int *local[3] = { 0, &n, (void *)0 };
            if (local[0] || local[2] || local[1] != &n) return 99;
            *table[1] = 7;
            return n;
        }";
    let assembly = compile_to_assembly(source, "null-table.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(7));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 64];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &[0; 8]);
    assert_eq!(&bytes[8..16], &0x80010030u64.to_le_bytes());
    assert_eq!(&bytes[16..48], &[0; 32]);
    assert_eq!(&bytes[48..56], &[7, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[56..64], &[0xa5; 8]);
}

#[test]
fn null_pointer_runtime_values_nonzero_constants_and_invalid_uses_preserve_output() {
    let bodies = [
        "int *p = 1;",
        "int *p = -1;",
        "int *p = 0x100000000UL;",
        "int zero = 0; int *p = zero;",
        "int *p = 0; int zero = 0; p = zero;",
        "int *p = 0; return p == 1;",
        "int *p = 0; int zero = 0; return p == zero;",
        "int *p = 0; return p < 0;",
        "int *p = 0; return 0 >= p;",
        "int *p = 0; p += 0; return p;",
        "int *p = 0; return 1 ? p : 1;",
        "int *p = 0; int zero = 0; p = 1 ? p : zero;",
        "int zero = 0; int *p = 0 && zero;",
        "int *p = 0 && main();",
        "int *p = 1 ? 0 : main();",
        "int *p = (int *)0; int value = p;",
        "int *p = 1 / 0;",
        "int *p = 0; int **q = &p; volatile int **r = 1 ? q : 0;",
    ];
    for body in bodies {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn goto_forward_backward_jumps_and_recursive_cleanup_execute() {
    let (machine, output) = execute(
        "static int calls;
         static int sum(int n) {
             int result = n;
             if (n == 0) goto cleanup;
             result += sum(n - 1);
             goto cleanup;
             result = 99;
         cleanup:
             ++calls;
             return result;
         }
         int main(void) {
             int i = 0;
             int result = 0;
             goto test;
         again:
             result += ++i;
         test:
             if (i < 4) goto again;
             goto cleanup;
             result = 99;
         cleanup:
             *(volatile unsigned char *)0x10000000UL = result;
             return result + sum(4) + calls;
         }",
    );
    assert_eq!(output, [10]);
    assert_eq!(machine.core().register(10), Some(25));
}

#[test]
fn goto_nested_scopes_switch_loops_and_label_namespace_execute() {
    let (machine, output) = execute(
        "int destination = 7;
         typedef int finish;
         enum { AGAIN = 2 };
         int main(void) {
             int total = 0;
             int destination = 3;
             for (int i = 0; i < 3; ++i) {
                 switch (i) {
                 case 0:
                     goto emit;
                 case 1:
                     goto nested;
                 default:
                     goto finish;
                 }
             emit:
                 *(volatile unsigned char *)0x10000000UL = i;
                 ++total;
                 continue;
             }
         nested:
             { int destination = 11;
               total += destination;
               goto AGAIN;
             }
         finish:
             return 99;
         AGAIN:
             goto destination;
         destination:
             return total + destination;
         }",
    );
    assert_eq!(output, [0]);
    assert_eq!(machine.core().register(10), Some(15));
    let (machine, _) = execute(
        "int main(void) {
             int result = 0;
             goto inner;
             { int n = 99;
             inner:
                 n = 7;
                 result = n;
                 goto end;
             }
         end:
             return result;
         }",
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn goto_reentering_scope_reexecutes_initialization_and_skips_side_effects() {
    let (machine, output) = execute(
        "static int calls;
         int initialize(void) { ++calls; return calls; }
         int main(void) {
             int total = 0;
             int rounds = 0;
         restart:
             {
                 int local = initialize();
                 int array[2] = {local, 3};
                 total += array[0] + array[1];
                 if (++rounds < 3) goto restart;
             }
             goto done;
             initialize();
             *(volatile unsigned char *)0x10000000UL = 99;
         done:
             return total + calls;
         }",
    );
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(18));
}

#[test]
fn goto_labels_are_function_local_and_cleanup_restores_stack_calls() {
    let (machine, _) = execute(
        "static int f(int a,int b,int c,int d,int e,int f,int g,int h,int i,int j) {
             int result = a+b+c+d+e+f+g+h+i+j;
             goto out;
             result = 99;
         out: return result;
         }
         static int g(int n) {
             int result = f(1,2,3,4,5,6,7,8,9,10);
             if (n) result += g(n - 1);
             goto out;
         out: return result;
         }
         int main(void) { goto out; out: return g(2); }",
    );
    assert_eq!(machine.core().register(10), Some(165));
}

#[test]
fn invalid_goto_labels_scopes_and_unevaluated_paths_preserve_rom() {
    let cases = [
        "int main(void) { goto missing; return 0; }",
        "int main(void) { return 0; goto missing; }",
        "int main(void) { if (0) goto missing; return 0; }",
        "int main(void) { while (0) { goto missing; } return 0; }",
        "int main(void) { switch (0) { case 1: goto missing; } return 0; }",
        "int main(void) { same: ; same: return 0; }",
        "int main(void) { { same: ; } { same: ; } return 0; }",
        "int main(void) { if (0) { same: ; } same: return 0; }",
        "int f(void) { target: return 1; } int main(void) { goto target; }",
        "int main(void) { goto target; target: { int hidden = 7; } return hidden; }",
        "int main(void) { goto target; target: int n = 7; return n; }",
        "int main(void) { goto target; target: break; }",
        "int main(void) { target: continue; }",
        "int main(void) { int n = 2; goto target; int a[n]; target: return 0; }",
        "int main(void) { goto * (int *)0x1000UL; }",
        "int main(void) { return &&target; target: return 0; }",
        "int main(void) { goto 123; }",
        "int main(void) { goto default; }",
        "int main(void) { goto if; }",
        "int main(void) { default: return 0; }",
        "int main(void) { typedef int same; same: ; same: return 0; }",
    ];
    for source in cases {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn pointer_tables_finalize_symbolic_entries_and_zero_fill_poisoned_ram() {
    let source = "
        int target;
        int later;
        static int *links[4] = { &target, &later, (int *)0 };
        int **cursor = links + 1;
        int **direct = &links[0];
        int *empty[3];
        int target = 7;
        int main(void) {
            if (links[2] || links[3] || empty[0] || empty[2]) return 99;
            if (cursor - links != 1 || direct != links) return 98;
            **cursor = 11;
            **direct += 3;
            volatile unsigned long *report = (volatile unsigned long *)0x80000000UL;
            report[0] = (unsigned long)links;
            report[1] = (unsigned long)empty;
            report[2] = (unsigned long)&target;
            report[3] = (unsigned long)&later;
            return target + later;
        }";
    let assembly = compile_to_assembly(source, "table-layout.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 112]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(21));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 32];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let addresses: Vec<_> = addresses
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010010, 0x80010038, 0x80010030, 0x80010050]);
    let mut bytes = [0; 96];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[0..8], &0x80010018u64.to_le_bytes());
    assert_eq!(&bytes[8..16], &0x80010010u64.to_le_bytes());
    assert_eq!(&bytes[16..24], &0x80010030u64.to_le_bytes());
    assert_eq!(&bytes[24..32], &0x80010050u64.to_le_bytes());
    assert_eq!(&bytes[32..48], &[0; 16]);
    assert_eq!(&bytes[48..56], &[10, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[56..80], &[0; 24]);
    assert_eq!(&bytes[80..88], &[11, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[88..96], &[0xa5; 8]);
}

#[test]
fn pointer_table_elements_support_all_integer_widths_and_volatile_pointees() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        for qualifier in ["", "volatile "] {
            let source = format!(
                "int main(void) {{
                    {ty} values[3] = {{ 1, 2, 3 }};
                    {qualifier}{ty} *table[4] = {{ values, values + 1, values + 2 }};
                    {qualifier}{ty} **p = table;
                    **p = 7;
                    ++p;
                    *p += 1;
                    **p = 9;
                    p[-1] = values + 1;
                    if (table[3] || sizeof(table) != 32 || sizeof(*p) != 8) return 99;
                    return values[0] + values[1] + values[2] + (p - table);
                }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(19), "{source}");
        }
    }
    let (machine, _) = execute(
        "int main(void) {
             int n = 4;
             void *table[3] = { &n, (void *)0 };
             void **p = table;
             int *value = *p;
             *value = 8;
             *p = &n;
             if (p[1] || p[2]) return 99;
             return *(int *)*p;
         }",
    );
    assert_eq!(machine.core().register(10), Some(8));
}

#[test]
fn automatic_pointer_tables_recursive_calls_and_element_updates_execute() {
    let (machine, _) = execute(
        "typedef int *item;
         typedef int **items;
         static int calls;
         int *choose(int *p) { ++calls; return p; }
         int recurse(int n) {
             int value = n;
             item table[3] = { choose(&value), (int *)0 };
             items cursor = table;
             int *old = (*cursor)++;
             --*cursor;
             if (old != &value || table[1] || table[2]) return 99;
             int result = n ? recurse(n - 1) : 0;
             return **cursor + result;
         }
         int main(void) { return recurse(4) + calls; }",
    );
    assert_eq!(machine.core().register(10), Some(15));
    let (machine, _) = execute(
        "int main(void) {
             int n = 1;
             int *p = &n;
             int **q = &p;
             int m = 7;
             *q = &m;
             **q += 2;
             return n + m;
         }",
    );
    assert_eq!(machine.core().register(10), Some(10));
}

#[test]
fn pointer_table_arguments_and_returns_preserve_eight_byte_stack_slots() {
    let (machine, _) = execute(
        "int **pass(int a, int b, int c, int d, int e, int f, int g, int h, int **table) {
             if (a+b+c+d+e+f+g+h != 36) return (int **)0;
             return table + 1;
         }
         int main(void) {
             int values[2] = { 3, 9 };
             int *table[] = { values, values + 1 };
             int **p = pass(1,2,3,4,5,6,7,8,table);
             return **p + (p - table);
         }",
    );
    assert_eq!(machine.core().register(10), Some(10));
    let (machine, _) = execute(
        "int n;
         int *p = &n;
         int **q = &p;
         int n = 3;
         int main(void) {
             int m = 11;
             **q += 4;
             *q = &m;
             return n + *p;
         }",
    );
    assert_eq!(machine.core().register(10), Some(18));
}

#[test]
fn invalid_pointer_table_types_initializers_qualifications_and_capacity_preserve_rom() {
    let (machine, _) = execute(
        "int main(void) {
             int *table[120] = { (int *)0 };
             return sizeof(table) + (table[119] == (int *)0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(961));
    let (machine, _) = execute_with_budget(
        "int *table[8192];
         int main(void) {
             if (table[0] || table[8191]) return 99;
             return sizeof(table);
         }",
        400000,
    );
    assert_eq!(machine.core().register(10), Some(65536));
    let sources = [
        "int *table[2] = {1}; int main(void) { return 0; }",
        "int n; unsigned int *table[] = {&n}; int main(void) { return 0; }",
        "int n; int *p = &n; int *table[] = {p}; int main(void) { return 0; }",
        "int *table[] = {&later}; int later; int main(void) { return 0; }",
        "int *table[1] = {(int *)0, (int *)0}; int main(void) { return 0; }",
        "int *table[2] = \"AB\"; int main(void) { return 0; }",
        "int *table[2] = {}; int main(void) { return 0; }",
        "int ***table; int main(void) { return 0; }",
        "int **table[2]; int main(void) { return 0; }",
        "int *table[8193]; int main(void) { return 0; }",
        "int main(void) { int *table[121]; return 0; }",
        "int main(void) { int n; int *table[120]; return 0; }",
        "int main(void) { int *table[2]; table++; return 0; }",
        "int main(void) { int *table[2]; return sizeof(&table); }",
        "int main(void) { int *table[2]; int *other[2]; table = other; return 0; }",
        "int main(void) { int n; int *p = &n; volatile int **q = &p; return 0; }",
        "int main(void) { volatile int *p = (volatile int *)0; int **q = &p; return 0; }",
        "int main(void) { int n; int *p = &n; void **q = &p; return 0; }",
        "int main(void) { int **p = (int **)0; int *q = p; return 0; }",
        "int main(void) { int **p = (int **)0; return sizeof(&p); }",
    ];
    for source in sources {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn boolean_conversions_promotions_and_scalar_updates_execute() {
    for (body, expected) in [
        ("_Bool b = 256; return b;", 1u64),
        ("_Bool b = -1; return b;", 1),
        ("_Bool b = 0x8000000000000000UL; return b;", 1),
        ("_Bool b = 0; return b;", 0),
        ("_Bool b = (int *)0; return b;", 0),
        ("_Bool b = (int *)0x80000000UL; return b;", 1),
        ("int n = 0; _Bool b = &n; return b;", 1),
        ("_Bool b = 1; return b + b;", 2),
        ("_Bool b = 1; return -b;", u64::MAX),
        ("_Bool b = 1; return sizeof(b + b);", 4),
        (
            "_Bool b = 1; return sizeof(b) + sizeof(_Bool) + sizeof(_Bool *);",
            10,
        ),
        ("_Bool b = 1; return (1 ? b : 5) + (0 ? b : 7U);", 8),
        ("_Bool b = 0; return b++ + b;", 1),
        ("_Bool b = 1; return b++ + b;", 2),
        ("_Bool b = 1; return b-- + b;", 1),
        ("_Bool b = 0; return --b;", 1),
        ("_Bool b = 1; b += 256; return b;", 1),
        ("_Bool b = 1; b -= 1; return b;", 0),
        ("_Bool b = 1; b ^= 1; return b;", 0),
        ("_Bool b = 1; b <<= 8; return b;", 1),
        ("_Bool b = 1; b >>= 1; return b;", 0),
        ("_Bool b = 1; b *= 0; return b;", 0),
        ("_Bool b = 1; b %= 1; return b;", 0),
        ("_Bool b = 1; b = 512; return b;", 1),
        ("_Bool b = 1; return ~b;", (-2i64) as u64),
        ("return (_Bool)(unsigned char)256;", 0),
        ("return (_Bool)256;", 1),
        ("return (_Bool)0xffffffffffffffffUL == 1;", 1),
    ] {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(machine.core().register(10), Some(expected), "{body}");
    }
}

#[test]
fn boolean_arrays_globals_and_qualified_views_preserve_exact_byte_storage() {
    let source = "typedef _Bool flag;
         flag initialized[] = { [4] = 256, [0] = -1, 0, 0x8000000000000000UL };
         flag zero[5];
         const flag *view = initialized;
         int main(void) {
             flag local[] = { [4] = 256, [0] = -1, 0, 0x8000000000000000UL };
             volatile flag *p = (volatile flag *)0x80000020UL;
             p[0] = 256; p[1] = 0; p[2] = -1; p[3] = 1;
             if (p[0]++ != 1 || p[1]-- != 0) return 1;
             p[2] -= 1;
             p[3] <<= 8;
             const volatile flag *read = p;
             if (read[0] != 1 || read[1] != 1 || read[2] != 0 || read[3] != 1) return 2;
             flag *table[] = { [1] = local };
             const flag **cursor = &view;
             if (**cursor != 1 || table[1][4] != 1 || read - p != 0) return 3;
             if (sizeof(initialized) != 5 || sizeof(local) != 5 || sizeof(flag) != 1) return 4;
             *(volatile unsigned long *)0x80000000UL = (unsigned long)initialized;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)zero;
             *(volatile unsigned long *)0x80000010UL = (unsigned long)local;
             return local[0] + local[1] + local[2] + local[3] + local[4];
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bool-bytes.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    machine.bus_mut().write(0x80000020, &[0xa5; 8]).unwrap();
    machine.bus_mut().write(0x8000fc00, &[0xa5; 1024]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(3));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 24];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    for (address, expected) in addresses.chunks_exact(8).zip([
        &[1u8, 0, 1, 0, 1][..],
        &[0u8; 5][..],
        &[1u8, 0, 1, 0, 1][..],
    ]) {
        let mut bytes = [0; 5];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(address.try_into().unwrap()), &mut bytes)
            .unwrap();
        assert_eq!(bytes, expected);
    }
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000020, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 1, 0, 1, 0xa5, 0xa5, 0xa5, 0xa5]);
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 0, 1, 0, 1, 0, 0, 0]);
    machine.bus_mut().read(0x80010010, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 8]);
    machine.bus_mut().read(0x80010018, &mut bytes).unwrap();
    assert_eq!(bytes, [0xa5; 8]);
}

#[test]
fn boolean_volatile_input_conversion_reads_each_uart_byte_once() {
    let source =
        "static int calls;
         volatile unsigned char *address(void) { ++calls; return (volatile unsigned char *)0x10000000UL; }
         int main(void) {
             sizeof((_Bool)*address());
             0 && (_Bool)*address();
             _Bool a = *address();
             _Bool b = *address();
             *(volatile unsigned char *)0x10000000UL = a;
             *(volatile unsigned char *)0x10000000UL = b;
             return calls;
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bool-uart.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[0, 128, 255], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, [0, 1]);
    assert_eq!(machine.core().register(10), Some(2));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut next = [0; 1];
    machine.bus_mut().read(0x10000000, &mut next).unwrap();
    assert_eq!(next, [255]);
}

#[test]
fn boolean_pointer_conversion_calls_recursion_and_side_effects_execute_once() {
    let (machine, output) = execute(
        "static int calls;
         static int object;
         int *address(void) { ++calls; *(volatile unsigned char *)0x10000000UL = 'P'; return &object; }
         _Bool accept(_Bool flag) { return flag; }
         _Bool recurse(int n, _Bool value) {
             if (n) return recurse(n - 1, value);
             return value;
         }
         _Bool ninth(int a,int b,int c,int d,int e,int f,int g,int h,_Bool value) {
             return value && a + b + c + d + e + f + g + h == 36;
         }
         _Bool pointer_result(void) { return address(); }
         int main(void) {
             _Bool value = accept(address());
             if (!value || !pointer_result() || calls != 2) return 1;
             if (!recurse(4, 0x8000000000000000UL)) return 2;
             if (!ninth(1,2,3,4,5,6,7,8,address()) || calls != 3) return 3;
             if (accept((const volatile void *)0)) return 4;
             sizeof((_Bool)address());
             0 && accept(address());
             if (calls != 3) return 5;
             value = 0;
             value += pointer_result();
             return value + calls;
         }"
    );
    assert_eq!(output, b"PPPP");
    assert_eq!(machine.core().register(10), Some(5));
}

#[test]
fn boolean_constant_indices_enumerators_and_switch_promotions_execute() {
    let (machine, _) = execute(
        "enum { TRUE_VALUE = (_Bool)0x100000000UL, FALSE_VALUE = (_Bool)0 };
         _Bool global = (_Bool)-1;
         int numbers[(_Bool)256 + 1] = { [(_Bool)256] = 42 };
         int main(void) {
             _Bool flag = global;
             switch (flag) { case 0: return 99; case 1: break; default: return 98; }
             if (TRUE_VALUE != 1 || FALSE_VALUE != 0 || sizeof(numbers) != 8) return 97;
             _Bool flags[] = { 0, 256, -2 };
             int total = 0;
             for (int i = 0; i < 3; ++i) total += flags[i];
             return numbers[TRUE_VALUE] + total;
         }",
    );
    assert_eq!(machine.core().register(10), Some(44));
}

#[test]
fn boolean_scalar_register_stack_abi_interoperates_with_independent_assembly() {
    let mut assembly = compile_to_assembly(
        "_Bool probe(_Bool a,int b,int c,int d,int e,int f,int g,int h,_Bool last) {
             return a && last && b + c + d + e + f + g + h == 35;
         }
         int main(void) { return probe(256,2,3,4,5,6,7,8,0x100000000UL); }",
        "bool-abi.c",
    )
    .unwrap();
    assembly = assembly.replacen("\nprobe:\n", "\n.Lunused_bool_probe:\n", 1);
    assembly.push_str(
        "probe:
         andi t0, sp, 15
         bne t0, zero, .Lbool_bad
         li t0, 1
         bne a0, t0, .Lbool_bad
         ld t1, 0(sp)
         bne t1, t0, .Lbool_bad
         li a0, 1
         ret
         .Lbool_bad:
         li a0, 0
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut assembly = compile_to_assembly(
        "_Bool probe(_Bool a,int b,int c,int d,int e,int f,int g,int h,_Bool last) {
             return a && last && b + c + d + e + f + g + h == 35;
         }
         int main(void) { return 0; }",
        "bool-caller.c",
    )
    .unwrap()
    .replacen("call main\n", "call .Lbool_entry\n", 1);
    assembly.push_str(
        ".Lbool_entry:
         addi sp, sp, -32
         sd ra, 24(sp)
         li s0, 123
         li s1, 456
         li t0, 1
         sd t0, 0(sp)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call probe
         ld ra, 24(sp)
         addi sp, sp, 32
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 1)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn boolean_invalid_types_qualifications_and_atomic_widths_preserve_rom() {
    for source in [
        "unsigned _Bool object; int main(void) { return 0; }",
        "signed _Bool object; int main(void) { return 0; }",
        "long _Bool object; int main(void) { return 0; }",
        "const _Bool object = 1; int main(void) { return 0; }",
        "volatile _Bool object; int main(void) { return 0; }",
        "int main(void) { _Bool a[2] = \"AB\"; return 0; }",
        "int main(void) { _Bool *p = (int *)0; return 0; }",
        "int main(void) { int *p = (_Bool *)0; return 0; }",
        "int main(void) { _Bool *p = (const _Bool *)0; return 0; }",
        "int main(void) { const _Bool *p = (const _Bool *)0; *p = 1; return 0; }",
        "int main(void) { const _Bool *p = (const _Bool *)0; ++*p; return 0; }",
        "int main(void) { _Bool *p = (_Bool *)0; const _Bool **q = &p; return 0; }",
        "int main(void) { _Bool b = 1; int *p = b; return 0; }",
        "void f(void) {} int main(void) { _Bool b = f(); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_load_n(&b,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_exchange_n(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_fetch_or(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_fetch_add(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; switch(b) { case 1: break; case (_Bool)256: break; } return 0; }",
    ] {
        let diagnostic = compile_to_assembly(source, "bool-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("bool-invalid.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn compiled_atomic_bitmap_allocator_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_bitmap.c");
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bitmap.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(59));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(output, b"B\x09\x09\n");
    let mut bytes = [0; 28];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let expected: Vec<_> = 9u64
        .to_le_bytes()
        .into_iter()
        .chain(0x8000000000000000u64.to_le_bytes())
        .chain(9u32.to_le_bytes())
        .chain(17u32.to_le_bytes())
        .chain(42u32.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
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
fn atomic_fetch_arithmetic_widths_wrapping_old_values_and_neighbors_execute() {
    for (ty, width, signed) in [
        ("int", 4, true),
        ("unsigned int", 4, false),
        ("long", 8, true),
        ("unsigned long", 8, false),
    ] {
        let mask = if width == 4 {
            u32::MAX as u64
        } else {
            u64::MAX
        };
        for old in [mask, mask >> 1, 1u64 << (width * 8 - 1)] {
            for (operation, operand) in [("add", 1u64), ("sub", 1), ("sub", 1u64 << 63)] {
                for order in 0..=5 {
                    let expected = if operation == "add" {
                        old.wrapping_add(operand) & mask
                    } else {
                        old.wrapping_sub(operand) & mask
                    };
                    let returned = if width == 4 && signed {
                        old as u32 as i32 as i64 as u64
                    } else {
                        old
                    };
                    let source = format!(
                        "int main(void) {{
                             volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                             *p = 0x{old:x}UL;
                             {ty} prior = __atomic_fetch_{operation}(p, 0x{operand:x}UL, {order});
                             *(volatile unsigned long *)0x80000020UL = prior;
                             return 7;
                         }}"
                    );
                    let rom = assemble_boot_rom(&compile_to_assembly(&source, "fetch.c").unwrap())
                        .unwrap();
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
                    machine.bus_mut().write(0x80000000, &[0xa5; 48]).unwrap();
                    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(10), Some(7));
                    assert_eq!(machine.core().register(2), Some(0x80010000));
                    assert_eq!(machine.core().register(8), Some(0));
                    let mut bytes = [0; 24];
                    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                    assert_eq!(&bytes[..8], &[0xa5; 8]);
                    assert_eq!(
                        &bytes[8..8 + width],
                        &expected.to_le_bytes()[..width],
                        "{source}"
                    );
                    assert!(bytes[8 + width..].iter().all(|byte| *byte == 0xa5));
                    let mut actual = [0; 8];
                    machine.bus_mut().read(0x80000020, &mut actual).unwrap();
                    assert_eq!(u64::from_le_bytes(actual), returned, "{source}");
                }
            }
        }
    }
}

#[test]
fn atomic_fetch_arithmetic_emits_exact_width_ordering_and_fence_words() {
    for width in [4, 8] {
        for operation in ["add", "sub"] {
            for order in 0..=5 {
                let ty = if width == 4 {
                    "unsigned int"
                } else {
                    "unsigned long"
                };
                let source = format!(
                    "enum {{ ORDER = {order} }};
                     int main(void) {{ {ty} object = 7;
                         {ty} old = __atomic_fetch_{operation}(&object, 9, ORDER);
                         return old == 7 && object == ({ty})(7 - 9 + {offset});
                     }}",
                    offset = if operation == "add" { 18 } else { 0 }
                );
                let rom = assemble_boot_rom(&compile_to_assembly(&source, "fetch-bits.c").unwrap())
                    .unwrap();
                let words: Vec<_> = rom
                    .chunks_exact(4)
                    .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                    .collect();
                let atomics: Vec<_> = words
                    .iter()
                    .enumerate()
                    .filter(|(_, word)| **word & 0x7f == 0x2f)
                    .collect();
                assert_eq!(atomics.len(), 1, "{source}");
                let (index, word) = atomics[0];
                assert_eq!(word >> 27, 0);
                assert_eq!((word >> 12) & 7, if width == 4 { 2 } else { 3 });
                assert_eq!((word >> 7) & 31, 10);
                assert_eq!((word >> 20) & 31, 10);
                assert_eq!(
                    (word >> 25) & 3,
                    match order {
                        0 => 0,
                        1 | 2 => 2,
                        3 => 1,
                        _ => 3,
                    }
                );
                assert_eq!(
                    words.iter().filter(|word| **word == 0x0ff0000f).count(),
                    if order == 5 { 2 } else { 0 }
                );
                if order == 5 {
                    assert_eq!(words[index - 1], 0x0ff0000f);
                    assert_eq!(words[index + 1], 0x0ff0000f);
                }
                let (machine, _) = execute(&source);
                assert_eq!(machine.core().register(10), Some(1));
            }
        }
    }
}

#[test]
fn atomic_fetch_arithmetic_evaluates_operands_once_across_nested_stack_calls() {
    let (mut machine, output) = execute(
        "static unsigned long counter;
         static int calls;
         unsigned long *address(void) {
             ++calls;
             *(volatile unsigned char *)0x10000000UL = 'P';
             return &counter;
         }
         unsigned long ninth(long a,long b,long c,long d,long e,long f,long g,long h,unsigned long value) {
             ++calls;
             *(volatile unsigned char *)0x10000000UL = 'V';
             return value;
         }
         unsigned long operand(void) { return ninth(1,2,3,4,5,6,7,8,5); }
         unsigned long recurse(int n) {
             unsigned long old = __atomic_fetch_add(&counter, 1, 2);
             if (n) return old + recurse(n - 1);
             return old;
         }
         int main(void) {
             if (__atomic_fetch_add(address(), operand(), 4) != 0) return 1;
             if (__atomic_fetch_sub(address(), operand(), 5) != 5) return 2;
             if (counter || calls != 4) return 3;
             if (recurse(4) != 10 || counter != 5) return 4;
             unsigned int local = 0x80000001U;
             unsigned int *table[] = { [1] = &local };
             unsigned int old = __atomic_fetch_add(table[1], (signed char)-1, 0);
             if (old != 0x80000001U || local != 0x80000000U) return 5;
             *(volatile unsigned long *)0x80000000UL = counter;
             return __atomic_fetch_sub(&local, 0x100000000UL, 3) == 0x80000000U;
         }"
    );
    assert_eq!(output, b"PVPV");
    assert_eq!(machine.core().register(10), Some(1));
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, 5u64.to_le_bytes());
}

#[test]
fn atomic_fetch_ticket_counter_uses_old_values_for_slots_and_publication() {
    let (machine, _) = execute(
        "static unsigned int next;
         static unsigned int published;
         static unsigned int entries[4];
         int publish(unsigned int value) {
             unsigned int ticket = __atomic_fetch_add(&next, 1, 0);
             if (ticket >= 4) {
                 __atomic_fetch_sub(&next, 1, 0);
                 return 0;
             }
             entries[ticket] = value;
             __atomic_fetch_add(&published, 1, 3);
             return 1;
         }
         int main(void) {
             if (!publish(11) || !publish(22) || !publish(33) || !publish(44)) return 1;
             if (publish(99) || next != 4) return 2;
             if (__atomic_load_n(&published, 2) != 4) return 3;
             if (entries[0] != 11 || entries[1] != 22 || entries[2] != 33 || entries[3] != 44) return 4;
             return __atomic_fetch_sub(&published, 4, 4);
         }"
    );
    assert_eq!(machine.core().register(10), Some(4));
}

#[test]
fn atomic_fetch_bitwise_widths_orders_old_values_and_neighbor_bytes_execute() {
    for (ty, size, signed) in [
        ("int", 4, true),
        ("unsigned int", 4, false),
        ("long", 8, true),
        ("unsigned long", 8, false),
    ] {
        let mask = if size == 4 { u32::MAX as u64 } else { u64::MAX };
        for (operation, funct5) in [("and", 12u32), ("or", 8), ("xor", 4)] {
            for (old, operand) in [
                (mask, 0u64),
                (0, mask),
                (0xfedcba9880000001, 0x1234567887654321),
                (0x8000000000000000, u64::MAX),
            ] {
                for order in 0..=5 {
                    let old = old & mask;
                    let expected = match operation {
                        "and" => old & operand,
                        "or" => old | operand,
                        _ => old ^ operand,
                    } & mask;
                    let returned = if size == 4 && signed {
                        old as u32 as i32 as i64 as u64
                    } else {
                        old
                    };
                    let source = format!(
                        "int main(void) {{
                             volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                             *p = 0x{old:x}UL;
                             {ty} prior = __atomic_fetch_{operation}(p, 0x{operand:x}UL, {order});
                             *(volatile unsigned long *)0x80000020UL = prior;
                             return 7;
                         }}"
                    );
                    let rom = assemble_boot_rom(&compile_to_assembly(&source, "bits.c").unwrap())
                        .unwrap();
                    let words: Vec<_> = rom
                        .chunks_exact(4)
                        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                        .collect();
                    let atomics: Vec<_> = words
                        .iter()
                        .enumerate()
                        .filter(|(_, word)| **word & 0x7f == 0x2f)
                        .collect();
                    assert_eq!(atomics.len(), 1);
                    let (index, word) = atomics[0];
                    assert_eq!(word >> 27, funct5);
                    assert_eq!((word >> 12) & 7, if size == 4 { 2 } else { 3 });
                    assert_eq!((word >> 7) & 31, 10);
                    assert_eq!((word >> 20) & 31, 10);
                    assert_eq!(
                        (word >> 25) & 3,
                        match order {
                            0 => 0,
                            1 | 2 => 2,
                            3 => 1,
                            _ => 3,
                        }
                    );
                    assert_eq!(
                        words.iter().filter(|word| **word == 0x0ff0000f).count(),
                        if order == 5 { 2 } else { 0 }
                    );
                    if order == 5 {
                        assert_eq!(words[index - 1], 0x0ff0000f);
                        assert_eq!(words[index + 1], 0x0ff0000f);
                    }
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
                    machine.bus_mut().write(0x80000000, &[0xa5; 48]).unwrap();
                    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(10), Some(7));
                    assert_eq!(machine.core().register(2), Some(0x80010000));
                    assert_eq!(machine.core().register(8), Some(0));
                    let mut bytes = [0; 24];
                    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                    assert_eq!(&bytes[..8], &[0xa5; 8]);
                    assert_eq!(
                        &bytes[8..8 + size],
                        &expected.to_le_bytes()[..size],
                        "{source}"
                    );
                    assert!(bytes[8 + size..].iter().all(|byte| *byte == 0xa5));
                    let mut actual = [0; 8];
                    machine.bus_mut().read(0x80000020, &mut actual).unwrap();
                    assert_eq!(u64::from_le_bytes(actual), returned, "{source}");
                }
            }
        }
    }
}

#[test]
fn atomic_fetch_bitwise_nested_stack_operands_tables_and_recursive_updates_execute() {
    let (machine, output) = execute(
        "typedef unsigned long word;
         static word object;
         static int calls;
         word *address(void) {
             ++calls; *(volatile unsigned char *)0x10000000UL = 'P'; return &object;
         }
         word ninth(long a,long b,long c,long d,long e,long f,long g,long h,word value) {
             ++calls; *(volatile unsigned char *)0x10000000UL = 'V'; return value;
         }
         word operand(void) { return ninth(1,2,3,4,5,6,7,8,0x8000000000000001UL); }
         word recurse(int n) {
             word old = __atomic_fetch_or(&object, 1UL << n, 2);
             if (n) return old ^ recurse(n - 1);
             return old;
         }
         int main(void) {
             if (__atomic_fetch_or(address(), operand(), 4) != 0) return 1;
             if (__atomic_fetch_xor(address(), operand(), 5) != 0x8000000000000001UL) return 2;
             if (__atomic_fetch_and(address(), operand(), 0) != 0 || calls != 6) return 3;
             if (recurse(3) != 10 || object != 15) return 4;
             unsigned int local = 0x80000001U;
             unsigned int *table[] = { [1] = &local };
             if (__atomic_fetch_and(table[1], (signed char)-2, 3) != 0x80000001U) return 5;
             if (local != 0x80000000U) return 6;
             if (__atomic_fetch_or(table[1], 0x100000001UL, 1) != 0x80000000U) return 7;
             return __atomic_fetch_xor(table[1], 0x80000000U, 2) == 0x80000001U && local == 1;
         }",
    );
    assert_eq!(output, b"PVPVPV");
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn atomic_fetch_invalid_inputs_and_reserved_names_preserve_rom() {
    for operation in ["add", "sub", "and", "or", "xor"] {
        let builtin = format!("__atomic_fetch_{operation}");
        let mut sources: Vec<_> = [
            format!("{builtin}((char *)0, 1, 0);"),
            format!("{builtin}((short *)0, 1, 0);"),
            format!("{builtin}((void *)0, 1, 0);"),
            format!("{builtin}((int **)0, 1, 0);"),
            format!("{builtin}((const int *)0, 1, 0);"),
            format!("{builtin}((const volatile long *)0, 1, 0);"),
            format!("{builtin}(1, 1, 0);"),
            format!("{builtin}((int *)0, (void *)0, 0);"),
            format!("{builtin}((int *)0, 1, 6);"),
            format!("{builtin}((int *)0, 1, -1);"),
            format!("int order = 0; {builtin}((int *)0, 1, order);"),
            format!("{builtin}((int *)0, 1, 1 / 0);"),
            format!("{builtin}((int *)0, 1);"),
            format!("{builtin}((int *)0, 1, 0, 0);"),
            format!("if (0) {builtin}((const int *)0, 1, 0);"),
            format!("int {builtin} = 0;"),
        ]
        .iter()
        .map(|body| format!("int main(void) {{ {body} return 0; }}"))
        .collect();
        sources.extend([
            format!("int {builtin}; int main(void) {{ return 0; }}"),
            format!("int {builtin}(void) {{ return 0; }} int main(void) {{ return 0; }}"),
            format!("typedef int {builtin}; int main(void) {{ return 0; }}"),
            format!("enum {{ {builtin} = 0 }}; int main(void) {{ return 0; }}"),
            format!("int f(int {builtin}) {{ return 0; }} int main(void) {{ return 0; }}"),
        ]);
        for source in sources {
            let diagnostic = compile_to_assembly(&source, "fetch-invalid.c")
                .unwrap_err()
                .to_string();
            assert!(
                diagnostic.contains("fetch-invalid.c:"),
                "{source}: {diagnostic}"
            );
            let files = Files::new(&source);
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.compile();
            assert!(!build.status.success(), "{source}");
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn atomic_integer_widths_orders_and_exact_neighbor_bytes_execute() {
    for ty in ["int", "unsigned int", "long", "unsigned long"] {
        for order in 0..=5 {
            let source = format!(
                "int main(void) {{
                    volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                    *p = -1;
                    {ty} old = __atomic_exchange_n(p, 7, {order});
                    if (old != ({ty})-1 || *p != 7) return 99;
                    __atomic_store_n(p, -2, 3);
                    if (__atomic_load_n(p, 2) != ({ty})-2) return 98;
                    return 7;
                }}"
            );
            let assembly = compile_to_assembly(&source, "atomic.c").unwrap();
            let rom = assemble_boot_rom(&assembly).unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
            machine.bus_mut().write(0x80000000, &[0xa5; 24]).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
            assert!(report.waiting > 0);
            assert_eq!(machine.core().register(10), Some(7), "{source}");
            assert_eq!(machine.core().register(2), Some(0x80010000));
            assert_eq!(machine.core().register(8), Some(0));
            let mut bytes = [0; 24];
            machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
            let width = if ty.contains("long") { 8 } else { 4 };
            assert_eq!(&bytes[..8], &[0xa5; 8]);
            assert_eq!(&bytes[8..8 + width], &(-2i64).to_le_bytes()[..width]);
            assert!(bytes[8 + width..].iter().all(|byte| *byte == 0xa5));
        }
    }
}

#[test]
fn atomic_operands_nested_calls_and_load_store_orders_execute() {
    let source = "
        static unsigned long object;
        static int calls;
        unsigned long *address(void) { ++calls; return &object; }
        unsigned long value(void) { ++calls; return 0xfedcba9876543210UL; }
        unsigned long exchange(void) { return __atomic_exchange_n(address(), value(), 4); }
        int main(void) {
            if (exchange() || calls != 2) return 99;
            if (__atomic_load_n(&object, 1) != 0xfedcba9876543210UL) return 98;
            __atomic_store_n(address(), value(), 0);
            if (calls != 4) return 97;
            __atomic_store_n(&object, 7, 5);
            if (__atomic_load_n(&object, 5) != 7) return 96;
            __atomic_store_n(&object, 8, 3);
            return __atomic_load_n(&object, 0);
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(8));
    let assembly = compile_to_assembly(source, "orders.c").unwrap();
    assert!(assembly.contains("amoswap.d.aqrl a0, a0, (t0)"));
    assert!(assembly.contains("lr.d.aq a0, (t0)"));
    assert!(assembly.contains("amoswap.d zero, a0, (t0)"));
    assert!(assembly.contains("amoswap.d.rl zero, a0, (t0)"));
    assert!(assembly.contains("fence\nlr.d.aq a0, (t0)\nfence"));
    assert!(assembly.contains("fence\namoswap.d.aqrl zero, a0, (t0)\nfence"));
    let (machine, _) = execute(
        "int main(void) {
             int local = -1;
             if (__atomic_exchange_n(&local, 5, 2) != -1) return 99;
             return __atomic_load_n(&local, 0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let (machine, _) = execute(
        "static unsigned int lock = 1;
         static unsigned long payload;
         int publish(unsigned long value) {
             if (__atomic_exchange_n(&lock, 1, 2)) return 0;
             payload = value;
             __atomic_store_n(&lock, 0, 3);
             return 1;
         }
         int main(void) {
             if (publish(99) || payload) return 99;
             __atomic_store_n(&lock, 0, 3);
             if (!publish(42)) return 98;
             if (__atomic_load_n(&lock, 2)) return 97;
             return payload;
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn atomic_memory_orders_emit_exact_rv64_ordering_bits_and_fences() {
    for (operation, orders) in [
        ("load", &[0u32, 1, 2, 5][..]),
        ("store", &[0u32, 3, 5][..]),
        ("exchange", &[0u32, 1, 2, 3, 4, 5][..]),
    ] {
        for order in orders {
            let argument = if operation == "load" { "" } else { "9," };
            let source = format!(
                "enum {{ ORDER = {order} }};
                 int main(void) {{
                     unsigned long object = 7;
                     __atomic_{operation}_n(&object, {argument} ORDER);
                     return object;
                 }}"
            );
            let assembly = compile_to_assembly(&source, "atomic-bits.c").unwrap();
            let rom = assemble_boot_rom(&assembly).unwrap();
            let words: Vec<_> = rom
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            let atomics: Vec<_> = words
                .iter()
                .enumerate()
                .filter(|(_, word)| **word & 0x7f == 0x2f)
                .collect();
            assert_eq!(atomics.len(), 1, "{source}");
            let (index, word) = atomics[0];
            assert_eq!(word >> 27, if operation == "load" { 2 } else { 1 });
            assert_eq!((word >> 12) & 7, 3);
            let expected_order = match order {
                0 => 0,
                1 | 2 => 2,
                3 => 1,
                _ if operation == "load" => 2,
                _ => 3,
            };
            assert_eq!((word >> 25) & 3, expected_order, "{source}");
            let fences = words.iter().filter(|word| **word == 0x0ff0000f).count();
            assert_eq!(fences, if *order == 5 { 2 } else { 0 }, "{source}");
            if *order == 5 {
                assert_eq!(words[index - 1], 0x0ff0000f);
                assert_eq!(words[index + 1], 0x0ff0000f);
            }
            let (machine, _) = execute(&source);
            assert_eq!(
                machine.core().register(10),
                Some(if operation == "load" { 7 } else { 9 })
            );
        }
    }
}

#[test]
fn atomic_misaligned_and_nonram_addresses_trap_in_guest() {
    for expression in [
        "__atomic_load_n((int *)0x80000001UL, 0)",
        "__atomic_exchange_n((long *)0x80000004UL, 1, 2)",
        "__atomic_store_n((int *)0x80000001UL, 1, 3)",
        "__atomic_load_n((int *)0x10000000UL, 0)",
        "__atomic_exchange_n((long *)0x1000UL, 1, 0)",
        "__atomic_fetch_add((int *)0x80000001UL, 1, 0)",
        "__atomic_fetch_sub((long *)0x80000004UL, 1, 0)",
        "__atomic_fetch_add((int *)0x10000000UL, 1, 0)",
        "__atomic_fetch_sub((long *)0x1000UL, 1, 0)",
        "__atomic_fetch_and((int *)0x80000001UL, 1, 0)",
        "__atomic_fetch_or((long *)0x80000004UL, 1, 0)",
        "__atomic_fetch_xor((int *)0x10000000UL, 1, 0)",
        "__atomic_fetch_and((long *)0x1000UL, 1, 0)",
    ] {
        let source = format!("int main(void) {{ {expression}; return 0; }}");
        let assembly = compile_to_assembly(&source, "trap.c").unwrap();
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 100, &[], &mut Vec::new()).unwrap();
        assert!(report.traps > 0, "{source}");
        assert!(report.last_trap.is_some());
    }
}

#[test]
fn invalid_atomic_types_orders_arguments_and_reserved_names_preserve_rom() {
    let bodies = [
        "__atomic_load_n((char *)0x80000000UL, 0);",
        "__atomic_store_n((short *)0x80000000UL, 1, 0);",
        "__atomic_exchange_n((void *)0x80000000UL, 1, 0);",
        "__atomic_load_n(1, 0);",
        "__atomic_load_n((int *)0x80000000UL, 3);",
        "__atomic_load_n((int *)0x80000000UL, 4);",
        "__atomic_store_n((int *)0x80000000UL, 1, 1);",
        "__atomic_store_n((int *)0x80000000UL, 1, 2);",
        "__atomic_store_n((int *)0x80000000UL, 1, 4);",
        "__atomic_exchange_n((int *)0x80000000UL, 1, 6);",
        "__atomic_exchange_n((int *)0x80000000UL, 1, -1);",
        "int order = 0; __atomic_load_n((int *)0x80000000UL, order);",
        "__atomic_load_n((int *)0x80000000UL, 1 / 0);",
        "__atomic_load_n((int *)0x80000000UL, (void *)0);",
        "__atomic_store_n((int *)0x80000000UL, (void *)0, 0);",
        "return __atomic_store_n((int *)0x80000000UL, 1, 0);",
        "__atomic_load_n((int *)0x80000000UL);",
        "__atomic_exchange_n((int *)0x80000000UL, 0);",
        "__atomic_store_n((int *)0x80000000UL, 0, 0, 0);",
        "int __atomic_load_n = 0;",
    ];
    let mut sources: Vec<_> = bodies
        .iter()
        .map(|body| format!("int main(void) {{ {body} return 0; }}"))
        .collect();
    sources.extend([
        "int __atomic_load_n; int main(void) { return 0; }".to_owned(),
        "int __atomic_store_n(void) { return 0; } int main(void) { return 0; }".to_owned(),
        "typedef int __atomic_load_n; int main(void) { return 0; }".to_owned(),
        "enum { __atomic_exchange_n = 0 }; int main(void) { return 0; }".to_owned(),
    ]);
    for source in sources {
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn file_linkage_forward_declarations_private_recursion_and_poisoned_storage_execute() {
    let source = "
        typedef unsigned short u16;
        static u16 private[3] = { 1, 2, 3 };
        extern u16 private[3];
        static unsigned long visits;
        extern unsigned long visits;
        static u16 *cursor = private + 1;
        extern u16 *cursor;
        extern unsigned long shared;
        static int step(int n);
        extern int step(int n);
        int main(void) {
            if (visits || shared != 7) return 99;
            int sum = step(3);
            volatile unsigned long *report = (volatile unsigned long *)0x80000000UL;
            report[0] = (unsigned long)private;
            report[1] = (unsigned long)&visits;
            report[2] = (unsigned long)cursor;
            report[3] = (unsigned long)&shared;
            return sum + *cursor + visits + shared;
        }
        int step(int n) {
            if (!n) return 0;
            ++visits;
            *cursor += n;
            return n + step(n - 1);
        }
        static int step(int n);
        extern unsigned long shared = 7;
        unsigned long shared;
        extern unsigned long shared;
        static unsigned long visits;
    ";
    let assembly = compile_to_assembly(source, "linkage.c").unwrap();
    assert!(assembly.contains(".Lrv64_internal_step:"));
    assert!(assembly.contains("call .Lrv64_internal_step"));
    assert!(!assembly.lines().any(|line| line == "step:"));
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(24));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 32];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let addresses: Vec<_> = addresses
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010008, 0x80010018, 0x8001000a, 0x80010010]);
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[0..8], &0x8001000au64.to_le_bytes());
    assert_eq!(&bytes[8..16], &[1, 0, 8, 0, 3, 0, 0, 0]);
    assert_eq!(&bytes[16..24], &7u64.to_le_bytes());
    assert_eq!(&bytes[24..32], &3u64.to_le_bytes());
    assert_eq!(&bytes[32..40], &[0xa5; 8]);
}

#[test]
fn file_linkage_multi_declarators_tentative_data_and_aliases_execute() {
    let (machine, _) = execute(
        "typedef unsigned long word;
         extern word left, right;
         static unsigned char first = 255, second;
         extern unsigned char first, second;
         static word twice(word n);
         word twice(word n);
         word twice(word n) { return n + n; }
         int main(void) {
             if (left != 4 || right || second) return 99;
             right = twice(left);
             second = first;
             return right + second;
         }
         word left = 4, right;
         word left;
         extern word right;
         static unsigned char second;
         extern word twice(word n);",
    );
    assert_eq!(machine.core().register(10), Some(263));
}

#[test]
fn extern_declarations_do_not_allocate_duplicate_global_storage() {
    let (mut machine, _) = execute_with_budget(
        "extern unsigned char data[65536];
         extern unsigned char data[65536];
         int main(void) {
             data[0] = 17;
             data[65535] = 29;
             return sizeof(data) + data[0] + data[65535];
         }
         unsigned char data[65536];
         extern unsigned char data[65536];",
        400000,
    );
    assert_eq!(machine.core().register(10), Some(65582));
    let mut first = [0; 2];
    let mut last = [0; 2];
    machine.bus_mut().read(0x80010000, &mut first).unwrap();
    machine.bus_mut().read(0x8001fffe, &mut last).unwrap();
    assert_eq!(first, [17, 0]);
    assert_eq!(last, [0, 29]);
}

#[test]
fn file_linkage_external_function_symbols_remain_abi_callable() {
    let (machine, _) = execute(
        "extern unsigned int identity(unsigned int value);
         unsigned int identity(unsigned int value) { return value; }
         extern unsigned int identity(unsigned int value);
         extern int main(void);
         int main(void) { return identity(4294967295U) == 4294967295U; }",
    );
    assert_eq!(machine.core().register(10), Some(1));
    let assembly = compile_to_assembly(
        "extern int f(void); int f(void) { return 3; }
         int main(void) { return f(); }",
        "external.c",
    )
    .unwrap();
    assert!(assembly.lines().any(|line| line == "f:"));
    assert!(assembly.lines().any(|line| line == "call f"));
}

#[test]
fn invalid_file_linkage_declarations_visibility_and_definitions_preserve_output() {
    let cases = [
        "extern int missing; int main(void) { return 0; }",
        "extern int missing; int main(void) { return missing; }",
        "static int missing(void); int main(void) { return 0; }",
        "extern int missing(void); int main(void) { return missing(); }",
        "extern int x; static int x; int main(void) { return 0; }",
        "int x; static int x; int main(void) { return 0; }",
        "static int x; int x; int main(void) { return 0; }",
        "static int x; extern int x; int x; int main(void) { return 0; }",
        "extern int f(void); static int f(void) { return 0; } int main(void) { return f(); }",
        "int f(void); static int f(void) { return 0; } int main(void) { return f(); }",
        "static int x; extern long x; int main(void) { return 0; }",
        "extern int x; long x; int main(void) { return 0; }",
        "extern int a[2]; int a[3]; int main(void) { return 0; }",
        "extern int x = 1; int x = 2; int main(void) { return 0; }",
        "static int f(void); extern long f(void); int main(void) { return 0; }",
        "static int main(void) { return 0; }",
        "extern int main(void); static int main(void) { return 0; }",
        "static int x; extern int x(void); int main(void) { return 0; }",
        "extern int x; int main(void) { return 0; } int x(void) { return 1; }",
        "typedef int number; extern int number; int main(void) { return 0; }",
        "enum { VALUE = 1 }; extern int VALUE; int main(void) { return 0; }",
        "static int; int main(void) { return 0; }",
        "extern int; int main(void) { return 0; }",
        "static extern int x; int main(void) { return 0; }",
        "static static int x; int main(void) { return 0; }",
        "extern int a[]; int a[2]; int main(void) { return 0; }",
        "int main(void) { static int count; return 0; }",
        "int main(void) { extern int x; return 0; } int x;",
        "int f(static int n) { return n; } int main(void) { return 0; }",
        "int main(void) { return x; } extern int x; int x;",
        "int main(void) { return f(); } static int f(void) { return 1; }",
    ];
    for source in cases {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM",
            "{source}"
        );
    }
}

#[test]
fn invalid_and_unsupported_c_is_not_silently_compiled() {
    let cases = [
        "#include <limits.h>\nint main(void) { return 0; }",
        "#define VALUE 1\nint main(void) { return VALUE; }",
        "int main() { return 0; }",
        "int main(void) { unsigned long long a; return 0; }",
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
        "int ***a; int main(void) { return 0; }",
        "int main(void) { volatile unsigned char *p = 1; return 0; }",
        "int main(void) { volatile unsigned char *p = (volatile unsigned char *)1; return p + p; }",
        "int main(void) { switch ((void *)0) { case 1: return 0; } }",
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
fn switch_promotions_lp64_case_conversion_and_constant_expressions_execute() {
    let cases = [
        ("signed char n = -1;", "255", "-1"),
        ("unsigned char n = 255;", "-1", "255"),
        ("char n = 255;", "-1", "255"),
        ("short n = -32768;", "32768", "-32768"),
        ("unsigned short n = 65535;", "-1", "65535"),
        ("int n = -2147483647 - 1;", "2147483647", "0x80000000U"),
        ("unsigned int n = 4294967295U;", "0", "-1"),
        ("long n = -1;", "4294967295U", "18446744073709551615UL"),
        ("unsigned long n = 0x8000000000000000UL;", "0", "1UL << 63"),
        ("int n = 0;", "1", "0x100000000UL"),
        ("unsigned int n = 0;", "1", "0x100000000UL"),
        ("long n = 0x100000000L;", "0", "0x100000000UL"),
    ];
    for (declaration, miss, hit) in cases {
        let source = format!(
            "int main(void) {{ {declaration}
                switch (n) {{ case {miss}: return 1; case {hit}: return 7; default: return 9; }}
            }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(7), "{source}");
    }
    let (machine, _) = execute(
        "typedef unsigned long u64;
         enum { BASE = 3, COMMAND = BASE + 1 };
         int main(void) {
             switch ((u64)COMMAND) {
             case 0 && (1 / 0): return 1;
             case sizeof(u64) / 2: return 7;
             default: return 9;
             }
         }",
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn switch_nested_break_continue_and_fallthrough_execute() {
    let (machine, output) = execute(
        "void put(unsigned char c) { *(volatile unsigned char *)0x10000000 = c; }
         int main(void) {
             int tails = 0;
             for (int i = 0; i < 6; ++i) {
                 switch (i) {
                 case 0: continue;
                 case 1:
                     while (1) {
                         switch (9) { default: put('A'); break; }
                         break;
                     }
                     break;
                 case 2:
                     switch (i) { case 2: put('B'); break; default: return 99; }
                 case 3: put('C'); continue;
                 default: put('D'); break;
                 }
                 put('E');
                 ++tails;
             }
             return tails;
         }",
    );
    assert_eq!(output, b"AEBCCDEDE");
    assert_eq!(machine.core().register(10), Some(3));
}

#[test]
fn switch_selector_side_effects_defaults_empty_cases_and_scopes_execute() {
    let (machine, output) = execute(
        "int calls;
         int select(void) { ++calls; return 2; }
         int main(void) {
             int n = 10;
             switch (select()) {
             case 1:
             case 2: { int n = 5; *(volatile unsigned char *)0x10000000 = n; }
             default: n += 3;
             case 4: n += 4; break;
             }
             switch (n++) {}
             switch (99) { case 1: n = 0; }
             switch (2) { default: n += 2; case 1: n += 3; break; case 2: n += 7; }
             switch (99) { default: n += 2; case 1: n += 3; break; case 2: n = 0; }
             volatile unsigned short *p = (volatile unsigned short *)0x80000000;
             *p = 65535;
             switch ((*p)++) { case 65535: break; default: return 99; }
             return n + calls + *p;
         }",
    );
    assert_eq!(output, [5]);
    assert_eq!(machine.core().register(10), Some(31));
    let mut bytes = [0; 2];
    let mut machine = machine;
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [0, 0]);
    let (machine, output) = execute(
        "int main(void) {
             int n = 10;
             switch (0) {
             case 0: ; int n = 3;
             case 1: n += 4; *(volatile unsigned char *)0x10000000 = n; break;
             }
             return n;
         }",
    );
    assert_eq!(output, [7]);
    assert_eq!(machine.core().register(10), Some(10));
}

#[test]
fn switch_volatile_uart_selector_is_read_once_before_dispatch() {
    let assembly = compile_to_assembly(
        "int main(void) {
             volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
             switch (*uart) {
             case 0: return 99;
             case 'X': *uart = 'A'; break;
             default: return 98;
             }
             *uart = *uart;
             return 7;
         }",
        "uart-switch.c",
    )
    .unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"XY", &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"AY");
    assert_eq!(machine.core().register(10), Some(7));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn switch_recursive_returns_and_loop_continue_targets_execute() {
    let (machine, _) = execute(
        "int factorial(int n) {
             switch (n) { case 0: case 1: return 1; default: return n * factorial(n - 1); }
         }
         int main(void) {
             int i = 0;
             int count = 0;
             do {
                 ++i;
                 switch (i) { case 1: continue; default: ++count; }
             } while (i < 3);
             while (i < 5) {
                 ++i;
                 switch (i) { case 4: continue; default: ++count; }
             }
             return factorial(5) + count;
         }",
    );
    assert_eq!(machine.core().register(10), Some(123));
}

#[test]
fn invalid_switch_cases_selectors_scopes_and_control_targets_diagnose() {
    let cases = [
        "switch ((void *)0) {}",
        "switch (1) { case (int *)0: break; }",
        "int n = 1; switch (n) { case n: break; }",
        "int n = 1; switch (n) { case n++: break; }",
        "switch (1) { case main(): break; }",
        "switch (1) { case 1 / 0: break; }",
        "switch (1) { case 1 << 32: break; }",
        "switch (1) { case 2147483647 + 1: break; }",
        "switch (1) { case 1: break; case 1: break; }",
        "switch (0U) { case -1: break; case 4294967295U: break; }",
        "switch (0) { case 0: break; case 0x100000000UL: break; }",
        "switch (0L) { case -1: break; case 18446744073709551615UL: break; }",
        "switch (1) { default: break; default: break; }",
        "switch (1) { case 1: continue; }",
        "break;",
        "continue;",
        "case 1: return 0;",
        "default: return 0;",
        "switch (1) { case 1: { int hidden = 1; } case 2: return hidden; }",
        "switch (1) { case 1: { int hidden = 1; } } return hidden;",
        "switch (1) { case 1: ; int hidden = 1; } return hidden;",
        "switch (1) { case 1: ; int n = 1; case 2: ; int n = 2; }",
        "switch (1) { case 1: if (1) { case 2: break; } }",
    ];
    for body in cases {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM",
            "{source}"
        );
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
fn enum_constants_use_lp64_evaluation_and_signed_int_runtime_types() {
    let (machine, _) = execute(
        "typedef unsigned short u16;
         enum {
             ZERO,
             ONE,
             MINIMUM = -2147483647 - 1,
             AFTER_MINIMUM,
             MAXIMUM = 2147483647,
             WRAPPED = (0xffffffffU + 1U),
             WIDTH = sizeof(unsigned long),
             SHIFT = 1UL << 20,
             MASK = SHIFT | 7,
             NEGATIVE = -7,
             NARROW = (u16)65538,
             SHORT_CIRCUIT = 1 ? NARROW : 1 / 0,
             LOGICAL = 0 && (1 / 0),
         };
         enum { NEXT = ONE + 2, FINAL };
         int main(void) {
             if (ZERO || ONE != 1 || NEXT != 3 || FINAL != 4) return 1;
             if (MINIMUM != (-2147483647 - 1) || AFTER_MINIMUM != -2147483647) return 2;
             if (MAXIMUM != 2147483647 || WRAPPED || WIDTH != 8) return 3;
             if (MASK != 1048583 || NEGATIVE != -7 || NARROW != 2 || SHORT_CIRCUIT != 2) return 4;
             if (LOGICAL || sizeof(MINIMUM) != 4 || sizeof(WIDTH) != 4 || sizeof(MAXIMUM) != 4) return 5;
             if (MINIMUM < 0L == 0 || MINIMUM < 1U) return 6;
             return 73;
         }",
    );
    assert_eq!(machine.core().register(10), Some(73));
}

#[test]
fn enum_bounds_initializers_and_pointer_offsets_execute_without_storage_objects() {
    let source = "
        enum { LENGTH = 4, FIRST = -1, SECOND = 32768, OFFSET = 2 };
        unsigned short data[LENGTH] = {FIRST, SECOND, LENGTH};
        unsigned short *cursor = data + OFFSET;
        int values[LENGTH] = {FIRST, SECOND};
        int sum(int n) { if (n) return LENGTH + sum(n - 1); return OFFSET; }
        int main(void) {
            unsigned char local[LENGTH] = {LENGTH, OFFSET};
            if (sizeof(data) != 8 || sizeof(values) != 16 || sizeof(local) != 4) return 1;
            if (data[0] != 65535 || data[1] != 32768 || *cursor != 4 || data[3]) return 2;
            if (values[0] != -1 || values[1] != 32768 || local[1] != 2 || local[3]) return 3;
            *(volatile unsigned long *)0x80000000UL = (unsigned long)data;
            *(volatile unsigned long *)0x80000008UL = (unsigned long)cursor;
            return sum(3);
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(14));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &0x80010008_u64.to_le_bytes());
    assert_eq!(&bytes[8..], &0x8001000c_u64.to_le_bytes());
    let mut data = [0; 8];
    machine.bus_mut().read(0x80010008, &mut data).unwrap();
    assert_eq!(data, [255, 255, 0, 128, 4, 0, 0, 0]);
}

#[test]
fn unsupported_enum_forms_ranges_visibility_and_name_collisions_diagnose() {
    for source in [
        "enum { BAD = 2147483648 }; int main(void) { return 0; }",
        "enum { BAD = 0xffffffffU }; int main(void) { return 0; }",
        "enum { BAD = -2147483649L }; int main(void) { return 0; }",
        "enum { MAX = 2147483647, BAD }; int main(void) { return 0; }",
        "enum { BAD = 1 / 0 }; int main(void) { return 0; }",
        "enum { BAD = 2147483647 + 1 }; int main(void) { return 0; }",
        "enum { BAD = 1 << 32 }; int main(void) { return 0; }",
        "enum { BAD = (void *)0 }; int main(void) { return 0; }",
        "enum { BAD = FUTURE, FUTURE = 7 }; int main(void) { return 0; }",
        "int n; enum { BAD = n }; int main(void) { return 0; }",
        "int f(void) { return 1; } enum { BAD = f() }; int main(void) { return 0; }",
        "enum { }; int main(void) { return 0; }",
        "enum { SAME, SAME }; int main(void) { return 0; }",
        "enum tagged { VALUE }; int main(void) { return 0; }",
        "int main(void) { return sizeof(enum missing); }",
        "int main(void) { return (enum missing)1; }",
        "int f(enum missing n) { return n; } int main(void) { return 0; }",
        "enum { VALUE } object; int main(void) { return 0; }",
        "typedef enum { VALUE } name; int main(void) { return 0; }",
        "int main(void) { enum { VALUE }; return 0; }",
        "enum { VALUE }; int VALUE; int main(void) { return 0; }",
        "int VALUE; enum { VALUE }; int main(void) { return 0; }",
        "enum { VALUE }; int VALUE(void) { return 1; } int main(void) { return 0; }",
        "typedef int VALUE; enum { VALUE }; int main(void) { return 0; }",
        "enum { VALUE }; int main(void) { int VALUE = 1; return VALUE; }",
        "enum { VALUE }; int f(int VALUE) { return VALUE; } int main(void) { return 0; }",
        "int main(void) { return FUTURE; } enum { FUTURE = 7 };",
        "enum { VALUE }; int main(void) { VALUE = 7; return 0; }",
        "enum { VALUE }; int main(void) { return (long)&VALUE; }",
    ] {
        let error = compile_to_assembly(source, "enum-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("enum-invalid.c"), "{source}: {error}");
    }
    let files = Files::new("enum { BAD = 0xffffffffU }; int main(void) { return 0; }");
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
}

#[test]
fn scalar_typedefs_preserve_lp64_widths_promotions_and_array_bounds() {
    let (machine, _) = execute(
        "typedef unsigned char u8;
         typedef unsigned short u16;
         typedef unsigned int u32;
         typedef unsigned long u64;
         typedef signed short i16;
         typedef u64 address;
         typedef void nothing;
         typedef void *buffer;
         typedef u16 *half_pointer;
         u16 seed[] = {65535, 7};
         half_pointer cursor = seed;
         address high = 0xfedcba9876543210UL;
         nothing set(half_pointer p, u16 value) { *p = value; }
         i16 recurse(i16 n, u8 depth) {
             if (depth) return recurse(n + 1, depth - 1);
             return n;
         }
         int main(void) {
             u8 a = 255; u16 b = 65535; u32 c = 0xffffffffU;
             address d = high; i16 negative = 65535;
             u16 local[sizeof(address)] = {7};
             buffer opaque = local;
             half_pointer back = opaque;
             if (sizeof(u8) != 1 || sizeof(u16) != 2 || sizeof(u32) != 4) return 1;
             if (sizeof(address) != 8 || sizeof(buffer) != 8 || sizeof(local) != 16) return 2;
             if (a + 1 != 256 || b + 1 != 65536 || c + 1U || negative != -1) return 3;
             if (d != 0xfedcba9876543210UL || back != local || back[7]) return 4;
             set(cursor, (u16)65538);
             if (seed[0] != 2 || seed[1] != 7 || recurse(32766, 3) != -32767) return 5;
             volatile u16 *v = back;
             *(volatile u16 *)v = 11;
             if (back[0] != 11 || sizeof(volatile u16 *) != 8) return 6;
             return 67;
         }",
    );
    assert_eq!(machine.core().register(10), Some(67));
}

#[test]
fn alias_and_underlying_function_signatures_and_global_declarations_are_compatible() {
    let (machine, _) = execute(
        "typedef unsigned long word;
         typedef word *word_pointer;
         word total;
         unsigned long total = 9;
         word sum(word a, word b);
         unsigned long sum(unsigned long a, unsigned long b) { return a + b; }
         word_pointer address(word_pointer p) { return p; }
         int main(void) {
             word local = sum(total, 4UL);
             word_pointer p = address(&local);
             if (*p != 13 || sizeof(*p) != 8) return 1;
             return 71;
         }",
    );
    assert_eq!(machine.core().register(10), Some(71));
}

#[test]
fn unsupported_typedef_targets_and_namespace_conflicts_diagnose() {
    for source in [
        "typedef float real; int main(void) { return 0; }",
        "typedef double real; int main(void) { return 0; }",
        "typedef unsigned long long wide; int main(void) { return 0; }",
        "typedef int ***nested; int main(void) { return 0; }",
        "typedef int array[2]; int main(void) { return 0; }",
        "typedef int initialized = 7; int main(void) { return 0; }",
        "typedef int (*callback)(int); int main(void) { return 0; }",
        "typedef int word; typedef long word; int main(void) { return 0; }",
        "typedef int word; int word; int main(void) { return 0; }",
        "int word; typedef int word; int main(void) { return 0; }",
        "typedef int word; int main(void) { int word = 7; return word; }",
        "typedef int word; int f(int word) { return word; } int main(void) { return 0; }",
        "typedef int word; int word(void) { return 1; } int main(void) { return 0; }",
        "int word(void) { return 1; } typedef int word; int main(void) { return 0; }",
        "typedef volatile int object; int main(void) { return 0; }",
        "typedef int *pointer; int main(void) { volatile pointer p; return 0; }",
        "typedef int *pointer; int main(void) { sizeof(volatile pointer); return 0; }",
        "int main(void) { typedef int word; return 0; }",
        "int main(void) { word n; return 0; } typedef int word;",
        "typedef int word; int main(void) { word *p = (long *)0; return 0; }",
    ] {
        let error = compile_to_assembly(source, "aliases-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("aliases-invalid.c"), "{source}: {error}");
    }
    let files = Files::new("typedef float hidden; int main(void) { return 0; }");
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
}

#[test]
fn generic_object_pointers_round_trip_supported_types_and_qualifications() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        let source = format!(
            "void *echo(void *p) {{ return p; }}
             volatile void *qualified(volatile void *p) {{ return p; }}
             int main(void) {{
                 {ty} value = 7;
                 {ty} *p = &value;
                 void *opaque = p;
                 {ty} *back = echo(opaque);
                 volatile {ty} *v = qualified(p);
                 volatile void *q = v;
                 volatile {ty} *again = q;
                 if (opaque != p || p != opaque || back != p || again != v) return 1;
                 if (q != opaque || opaque != q || !q || !opaque) return 2;
                 *back = 9; *again += 2;
                 if (value != 11 || sizeof(opaque) != 8 || sizeof(q) != 8) return 3;
                 {ty} *explicit = ({ty} *)(void *)v;
                 if (explicit != p || *explicit != 11) return 4;
                 return 59;
             }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(59), "{ty}");
    }
}

#[test]
fn generic_global_addresses_resolve_final_layout_and_preserve_pointer_bytes() {
    let source = "
        unsigned short data[3];
        void *a = &data[1];
        volatile void *b = &data[2];
        void *c = (void *)0xfedcba9876543210UL;
        void *d = (void *)0;
        void *e;
        void *f = 1 ? &data[0] : (void *)0;
        unsigned short data[3] = {7, 11, 13};
        void *echo(void *p) { return p; }
        int main(void) {
            unsigned short *p = a;
            volatile unsigned short *q = b;
            if (*p != 11 || *q != 13 || d || e) return 1;
            if (a != &data[1] || b != &data[2] || f != data) return 2;
            if ((unsigned long)c != 0xfedcba9876543210UL) return 3;
            e = echo(data + 2);
            q = e;
            *q += 1;
            if (data[2] != 14) return 4;
            *(volatile unsigned long *)0x80000000UL = (unsigned long)data;
            return 61;
        }";
    let assembly = compile_to_assembly(source, "generic-globals.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 56]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(61));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 56];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected: Vec<u8> = [
        0x80010022_u64,
        0x80010024,
        0xfedcba9876543210,
        0,
        u64::from_le_bytes([7, 0, 11, 0, 14, 0, 0, 0]),
        0x80010020,
        0x80010024,
    ]
    .iter()
    .flat_map(|n| n.to_le_bytes())
    .collect();
    assert_eq!(bytes.as_slice(), expected);
}

#[test]
fn generic_conditionals_merge_qualifiers_and_skip_unselected_calls() {
    let (machine, output) = execute(
        "int n = 7;
         short s = 11;
         int calls;
         void *pick(void *p) { ++calls; return p; }
         int main(void) {
             int *p = &n;
             volatile int *v = p;
             void *opaque = p;
             volatile void *a = 1 ? opaque : v;
             volatile void *b = 0 ? v : opaque;
             if (a != p || b != p) return 1;
             volatile int *back = a;
             if (*back != 7) return 2;
             void *chosen = 1 ? pick(&s) : pick(p);
             short *q = chosen;
             if (*q != 11 || calls != 1) return 3;
             if ((0 ? opaque : p) != p || (1 ? p : opaque) != p) return 4;
             volatile void *qualified = v;
             if ((0 ? opaque : qualified) != p) return 5;
             *(volatile unsigned char *)0x10000000UL = calls;
             return 63;
         }",
    );
    assert_eq!(machine.core().register(10), Some(63));
    assert_eq!(output, [1]);
}

#[test]
fn generic_pointer_abi_interoperates_with_independent_register_and_stack_calls() {
    let source = "
        void *echo(void *p) { return p; }
        volatile void *probe(void *a, int b, int c, int d, int e, int f,
                             int g, int h, volatile void *i) { return i; }
        volatile void *caller(void) {
            return probe((void *)0xfedcba9876543210UL, 0, 0, 0, 0, 0, 0, 0,
                         echo((void *)0x80000000UL));
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "generic-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lgeneric_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lgeneric_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, -1
         call echo
         mv s2, a0
         call caller
         mv s3, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lgeneric_bad
         srli t0, a0, 32
         li t1, -19088744
         slli t1, t1, 32
         srli t1, t1, 32
         bne t0, t1, .Lgeneric_bad
         slli t0, a0, 32
         srli t0, t0, 32
         li t1, 1985229328
         bne t0, t1, .Lgeneric_bad
         ld t0, 0(sp)
         li t1, 1
         slli t1, t1, 31
         bne t0, t1, .Lgeneric_bad
         mv a0, t0
         ret
         .Lgeneric_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(0x80000000));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_generic_pointer_operations_and_qualification_removal_diagnose() {
    for body in [
        "void *p = (void *)0; p++;",
        "void *p = (void *)0; --p;",
        "void *p = (void *)0; p += 1;",
        "void *p = (void *)0; p -= 1;",
        "void *p = (void *)0; p + 1;",
        "void *p = (void *)0; 1 + p;",
        "void *p = (void *)0; p - p;",
        "void *p = (void *)0; p[0];",
        "void *p = (void *)0; *p;",
        "void *p = (void *)0; sizeof(*p);",
        "void *p = (void *)0; p < p;",
        "void *p = (void *)0; p >= p;",
        "void *p = (void *)0; p();",
        "void **p = (void **)0; &p;",
        "void ***p;",
        "void **a[2];",
        "void *p = 1;",
        "volatile int *p = (volatile int *)0; void *q = p;",
        "volatile void *p = (volatile void *)0; int *q = p;",
        "volatile void *p = (volatile void *)0; void *q = p;",
        "int *p = (int *)0; volatile void *v = p; void *q = 1 ? p : v;",
        "void *p = (void *)0; volatile int *v = (volatile int *)0; void *q = 0 ? p : v;",
        "int *p = (int *)0; long *q = (long *)0; void *v = 1 ? p : q;",
        "void *p = (void *)0; int *q = (int *)0; p < q;",
        "void *p = (void *)0; return p;",
    ] {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let error = compile_to_assembly(&source, "generic-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("generic-invalid.c"), "{body}: {error}");
    }
}

#[test]
fn halfword_storage_guest_preserves_descriptor_neighbors_and_ring_entries() {
    let source = include_str!("fixtures/kiuas16_storage.c");
    let rom = assemble_boot_rom(&compile_to_assembly(source, "storage.c").unwrap()).unwrap();
    let files = Files::new(source);
    let mut disk = vec![0; 1024];
    disk[512..].fill(b'R');
    let path = files.0.join("disk.raw");
    for writable in [false, true] {
        std::fs::write(&path, &disk).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom.clone(), 0x20000).unwrap();
        machine
            .bus_mut()
            .attach_root_block(Box::new(FileSectorMedia::open(&path, !writable).unwrap()))
            .unwrap();
        machine.bus_mut().write(0x80000000, &[0xa5; 64]).unwrap();
        machine.bus_mut().write(0x80000100, &[0xa5; 20]).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 20000, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0, "{:?}", report.last_trap);
        assert!(report.waiting > 0);
        assert_eq!(output, [b'C', 0, b'R', u8::from(!writable), 0, 1, 3, 0]);
        assert_eq!(machine.core().register(10), Some(0));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut descriptors = [0; 64];
        machine
            .bus_mut()
            .read(0x80000000, &mut descriptors)
            .unwrap();
        let mut expected = Vec::new();
        for (address, length, flags, next) in [
            (0x80000300_u64, 16_u32, 1_u16, 2_u16),
            (0x80000400, 512, 1, 2),
            (0x80000600, 1, 2, 0),
        ] {
            expected.extend_from_slice(&address.to_le_bytes());
            expected.extend_from_slice(&length.to_le_bytes());
            expected.extend_from_slice(&flags.to_le_bytes());
            expected.extend_from_slice(&next.to_le_bytes());
        }
        expected.extend_from_slice(&[0xa5; 16]);
        assert_eq!(descriptors.as_slice(), expected);
        let mut available = [0; 20];
        machine.bus_mut().read(0x80000100, &mut available).unwrap();
        assert_eq!(&available[..10], &[0, 0, 3, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&available[10..], &[0xa5; 10]);
        let mut used = [0; 4];
        machine.bus_mut().read(0x80000200, &mut used).unwrap();
        assert_eq!(used, [0, 0, 3, 0]);
        let mut copy_addresses = [0; 16];
        machine
            .bus_mut()
            .read(0x80000700, &mut copy_addresses)
            .unwrap();
        let snapshot = u64::from_le_bytes(copy_addresses[..8].try_into().unwrap());
        assert_eq!(
            u64::from_le_bytes(copy_addresses[8..].try_into().unwrap()),
            0x80000400
        );
        let mut copied = [0; 16];
        machine.bus_mut().read(snapshot, &mut copied).unwrap();
        assert_eq!(&copied[..1], b"W");
        assert_eq!(&copied[1..], &[b'R'; 15]);
        let mut buffer = [0; 512];
        machine.bus_mut().read(0x80000400, &mut buffer).unwrap();
        assert_eq!(buffer[0], b'W');
        assert_eq!(&buffer[1..], &[b'R'; 511]);
        drop(machine);
        let mut expected_disk = disk.clone();
        if writable {
            expected_disk[512] = b'W';
        }
        assert_eq!(std::fs::read(&path).unwrap(), expected_disk);
    }
}

#[test]
fn halfword_promotions_narrowing_aliases_and_updates_execute() {
    for (body, expected) in [
        ("short n = 65535; return n;", -1),
        ("unsigned short n = -1; return n + 1;", 65536),
        ("short n = 32768; return n;", -32768),
        ("short n = -32768; return n / 2;", -16384),
        ("short n = -1; return n < 1U;", 0),
        ("short n = -2; return n >> 1;", -1),
        ("unsigned short n = 32768; return n >> 1;", 16384),
        ("unsigned short n = 256; return n * n == 65536;", 1),
        ("short n = 32767; int old = n++; return old + n;", -1),
        ("short n = -32768; return --n;", 32767),
        ("short n = 32767; return n += 1;", -32768),
        ("unsigned short n = 65535; return ++n;", 0),
        ("unsigned short n = 32768; return n <<= 1;", 0),
        ("short n = 0; short *p = &n; *p = 65535; return n;", -1),
        (
            "unsigned short n = 0; unsigned short *p = &n; *p = -1; return n;",
            65535,
        ),
        (
            "return sizeof(short) + sizeof(unsigned short) + sizeof(short *);",
            12,
        ),
        ("unsigned short n = 1; return sizeof(n + n);", 4),
    ] {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn halfword_arrays_globals_static_addresses_and_volatile_bytes_execute() {
    let (mut machine, _) = execute(
        "short signed_values[] = {32768, 65535, 32767};
         unsigned short unsigned_values[4] = {65536, -1, 0x1234};
         short *signed_cursor = signed_values + 1;
         volatile unsigned short *cursor = &unsigned_values[2];
         int main(void) {
             short local[3] = {65535, 32768};
             short *p = local + 2;
             if (sizeof(signed_values) != 6 || sizeof(unsigned_values) != 8) return 1;
             if (signed_values[0] != -32768 || *signed_cursor != -1) return 2;
             if (unsigned_values[0] || unsigned_values[1] != 65535 || unsigned_values[3]) return 3;
             if (p - local != 2 || p[-1] != -32768 || *p || local[0] != -1) return 4;
             if ((unsigned long)cursor != (unsigned long)unsigned_values + 4UL) return 5;
             *cursor += 1;
             volatile short *s = (volatile short *)0x80000002UL;
             volatile unsigned short *u = (volatile unsigned short *)0x80000004UL;
             *(volatile unsigned long *)0x80000000UL = 0xa5a5a5a5a5a5a5a5UL;
             *s = 0x8000; *u = 0xffff;
             if (*s != -32768 || *u != 65535) return 6;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)signed_values;
             *(volatile unsigned long *)0x80000010UL = (unsigned long)unsigned_values;
             return 53;
         }",
    );
    assert_eq!(machine.core().register(10), Some(53));
    let mut bytes = [0; 24];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &[0xa5, 0xa5, 0, 0x80, 0xff, 0xff, 0xa5, 0xa5]);
    for (address, expected) in bytes[8..].chunks_exact(8).zip([
        [0, 0x80, 0xff, 0xff, 0xff, 0x7f, 0, 0],
        [0, 0, 0xff, 0xff, 0x35, 0x12, 0, 0],
    ]) {
        let mut data = [0; 8];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(address.try_into().unwrap()), &mut data)
            .unwrap();
        assert_eq!(data, expected);
    }
    for bound in ["(unsigned short)65538", "sizeof(short)", "sizeof(\"X\")"] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ short a[{bound}] = {{7}}; return sizeof(a); }}"
        ));
        assert_eq!(machine.core().register(10), Some(4));
    }
}

#[test]
fn halfword_updates_survive_nested_calls_and_qualification_conversions() {
    let (machine, output) = execute(
        "short values[3] = {32767, -32768, 7};
         int calls;
         short *target(void) { ++calls; return values; }
         int operand(void) { ++calls; return 1; }
         short recur(short n, unsigned short depth) {
             if (depth) return recur(n + 1, depth - 1);
             return n;
         }
         int main(void) {
             volatile short *v = values;
             short *p = (short *)v;
             if (p != v || (1 ? p : v) != p || v - p) return 1;
             short old = (*target())++;
             if (old != 32767 || values[0] != -32768 || calls != 1) return 2;
             *target() += operand();
             if (values[0] != -32767 || calls != 3) return 3;
             if (recur(32766, 3) != -32767) return 4;
             signed short int s = -1;
             unsigned short int u = 65535;
             volatile signed short int *sv = &s;
             volatile unsigned short int *uv = &u;
             if (*sv != -1 || *uv != 65535) return 5;
             *(volatile unsigned char *)0x10000000UL = calls;
             return 57;
         }",
    );
    assert_eq!(machine.core().register(10), Some(57));
    assert_eq!(output, [3]);
}

#[test]
fn halfword_abi_matches_independent_register_and_stack_partners() {
    let source = "
        short echo(short n) { return n; }
        unsigned short echo_unsigned(unsigned short n) { return n; }
        short probe(short a, unsigned short b, int c, int d, int e, int f,
                    int g, int h, short i, unsigned short j) { return 0; }
        short caller(void) {
            return probe(-32768, 65535, 0, 0, 0, 0, 0, 0, echo(-1), 32768);
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "halfword-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lhalfword_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lhalfword_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, 65535
         call echo
         mv s2, a0
         li a0, -1
         call echo_unsigned
         mv s3, a0
         call caller
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lhalfword_bad
         li t0, -32768
         bne a0, t0, .Lhalfword_bad
         li t0, 65535
         bne a1, t0, .Lhalfword_bad
         ld t1, 0(sp)
         li t0, -1
         bne t1, t0, .Lhalfword_bad
         ld t1, 8(sp)
         li t0, 32768
         bne t1, t0, .Lhalfword_bad
         li a0, -2
         ret
         .Lhalfword_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(65535));
    assert_eq!(machine.core().register(20), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_halfword_types_initializers_and_qualifications_diagnose() {
    for source in [
        "int main(void) { unsigned short *p = (short *)0; return 0; }",
        "int main(void) { short *p = (int *)0; return 0; }",
        "int main(void) { short *p = 1; return 0; }",
        "int main(void) { volatile short *v = (volatile short *)0; short *p = v; return 0; }",
        "int main(void) { volatile unsigned short *v = (volatile unsigned short *)0; unsigned short *p = v; return 0; }",
        "int main(void) { short ***p; return 0; }",
        "int main(void) { short **a[2]; return 0; }",
        "int main(void) { short a[481]; return 0; }",
        "short a[32769]; int main(void) { return 0; }",
        "short a; unsigned short a; int main(void) { return 0; }",
        "int main(void) { volatile short a; return 0; }",
        "int main(void) { short a[2] = \"AB\"; return 0; }",
    ] {
        let error = compile_to_assembly(source, "halfword-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("halfword-invalid.c"), "{source}: {error}");
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
