use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::c_compiler::rv64::compile_to_assembly;
use suomicpu16::kiuas16::Kiuas16Machine;
use suomicpu16::kiuas16::runtime::run_headless;

fn execute(source: &str) -> (Kiuas16Machine, Vec<u8>) {
    let assembly = compile_to_assembly(source, "guest.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
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
fn invalid_and_unsupported_c_is_not_silently_compiled() {
    let cases = [
        "#include <limits.h>\nint main(void) { return 0; }",
        "#define VALUE 1\nint main(void) { return VALUE; }",
        "int main() { return 0; }",
        "int main(void) { unsigned long a; return 0; }",
        "int main(void) { static int a; return 0; }",
        "int main(void) { const int a = 1; return 0; }",
        "int main(void) { return unknown; }",
        "int main(void) { int a; int a; return 0; }",
        "int main(void) { int a[2]; return 0; }",
        "int main(void) { return 1 / 2; }",
        "int main(void) { return 1U; }",
        "int main(void) { return 2147483648; }",
        "int main(void) { return sizeof(long); }",
        "int main(void) { return; }",
        "void f(void) { return 1; } int main(void) { f(); return 0; }",
        "int f(int a) { return a; } int main(void) { return f(); }",
        "int f(void); int main(void) { return f(); }",
        "int main(void) { return f(); } int f(void) { return 1; }",
        "int f(int a); long f(int a) { return a; } int main(void) { return 0; }",
        "int main(void) { return 0; } int main(void) { return 1; }",
        "int main(void) { int f = 1; return f(); }",
        "int main(void) { return 0; unknown(); }",
        "int a; int main(void) { return a; }",
        "int main(void) { volatile unsigned char *p = 1; return 0; }",
        "int main(void) { volatile unsigned char *p = (volatile unsigned char *)1; return p + 1; }",
        "int main(void) { for (;;) {} }",
        "int main(void) { return 2 = 1; }",
        "int f(int a, int b, int c, int d, int e, int f, int g, int h, int i) { return a; } int main(void) { return 0; }",
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
