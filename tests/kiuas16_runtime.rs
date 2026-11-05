use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use suomicpu16::kiuas16::kiulu::*;
use suomicpu16::kiuas16::runtime::{RunError, run_headless};
use suomicpu16::kiuas16::{
    BOOT_ROM_BYTES, FileSectorMedia, Kiuas16Machine, RAM, ROOT_BLOCK, SectorMedia, UART,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Files {
    directory: PathBuf,
}

impl Files {
    fn new(rom: &[u8]) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "kiuas16-runtime-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("rom.bin"), rom).unwrap();
        let mut disk = vec![0; 1536];
        disk[512..1024].fill(b'R');
        std::fs::write(directory.join("disk.raw"), disk).unwrap();
        std::fs::write(
            directory.join("input.bin"),
            b"serial input beyond FIFO capacity",
        )
        .unwrap();
        Self { directory }
    }

    fn command(&self, steps: u64) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"));
        command
            .arg("--rom")
            .arg(self.directory.join("rom.bin"))
            .arg("--steps")
            .arg(steps.to_string());
        command
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        for name in ["rom.bin", "disk.raw", "input.bin"] {
            std::fs::remove_file(self.directory.join(name)).unwrap();
        }
        std::fs::remove_dir(&self.directory).unwrap();
    }
}

struct Rom {
    code: Vec<u32>,
}

impl Rom {
    fn new() -> Self {
        Self { code: Vec::new() }
    }

    fn immediate(&mut self, rd: u32, rs: u32, function: u32, value: i32) {
        assert!((-2048..2048).contains(&value));
        self.code
            .push(((value as u32 & 0xfff) << 20) | rs << 15 | function << 12 | rd << 7 | 0x13);
    }

    fn constant(&mut self, rd: u32, value: u32) {
        let upper = value.wrapping_add(0x800) & 0xfffff000;
        self.code.push(upper | rd << 7 | 0x37);
        let lower = (value << 20) as i32 >> 20;
        self.immediate(rd, rd, 0, lower);
        if upper & 0x8000_0000 != 0 {
            self.immediate(rd, rd, 1, 32);
            self.immediate(rd, rd, 5, 32);
        }
    }

    fn store(&mut self, rs: u32, base: u32, offset: u32, width: u32) {
        assert!(offset < 2048);
        self.code.push(
            (offset >> 5) << 25 | rs << 20 | base << 15 | width << 12 | (offset & 31) << 7 | 0x23,
        );
    }

    fn load(&mut self, rd: u32, base: u32, offset: u32, width: u32) {
        assert!(offset < 2048);
        self.code
            .push(offset << 20 | base << 15 | width << 12 | rd << 7 | 0x03);
    }

    fn ram(&mut self, offset: u32, width: u32, value: u32) {
        self.constant(7, value);
        self.store(7, 6, offset, width);
    }

    fn mmio(&mut self, offset: u64, value: u32) {
        self.constant(7, value);
        self.store(7, 5, offset.try_into().unwrap(), 2);
    }

    fn descriptor(&mut self, index: u32, address: u32, length: u32, flags: u32) {
        self.ram(index * 16, 3, address);
        self.ram(index * 16 + 8, 2, length);
        self.ram(index * 16 + 12, 2, flags);
    }

    fn initialize_block(&mut self) {
        self.constant(5, ROOT_BLOCK.start as u32);
        self.constant(6, RAM.start as u32);
        self.constant(8, UART.start as u32);
        self.mmio(MMIO_STATUS, 3);
        self.mmio(MMIO_DRIVER_FEATURES_SEL, 1);
        self.mmio(MMIO_DRIVER_FEATURES, 1);
        self.mmio(MMIO_DRIVER_FEATURES_SEL, 0);
        self.mmio(MMIO_DRIVER_FEATURES, 1 << 9);
        self.mmio(MMIO_STATUS, 11);
        self.mmio(MMIO_QUEUE_NUM, 8);
        self.mmio(MMIO_QUEUE_DESC_LOW, RAM.start as u32);
        self.mmio(MMIO_QUEUE_DRIVER_LOW, RAM.start as u32 + 0x100);
        self.mmio(MMIO_QUEUE_DEVICE_LOW, RAM.start as u32 + 0x200);
        self.mmio(MMIO_QUEUE_READY, 1);
        self.mmio(MMIO_STATUS, 15);
    }

    fn block() -> Self {
        let mut rom = Self::new();
        rom.initialize_block();
        rom.descriptor(0, RAM.start as u32 + 0x300, 16, 0x10001);
        rom.descriptor(1, RAM.start as u32 + 0x400, 512, 0x20003);
        rom.descriptor(2, RAM.start as u32 + 0x600, 1, 2);
        rom.ram(0x308, 3, 1);
        rom.ram(0x600, 0, 0xff);
        rom.ram(0x102, 1, 1);
        rom.mmio(MMIO_QUEUE_NOTIFY, 0);
        rom.load(7, 6, 0x400, 4);
        rom.store(7, 8, 0, 0);
        rom.load(7, 6, 0x600, 4);
        rom.store(7, 8, 0, 0);
        rom
    }

    fn finish(mut self) -> (Vec<u8>, u64) {
        self.code.push(0x10500073);
        let steps = self.code.len() as u64 + 8;
        let mut rom = vec![0; BOOT_ROM_BYTES];
        for (index, word) in self.code.iter().enumerate() {
            rom[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        (rom, steps)
    }
}

#[test]
fn rom_owns_negotiation_queues_read_write_flush_irq_and_reset() {
    let mut guest = Rom::block();
    guest.ram(0x300, 2, 1);
    guest.ram(0x308, 3, 2);
    guest.ram(0x400, 0, b'W' as u32);
    guest.ram(28, 2, 0x20001);
    guest.ram(0x102, 1, 2);
    guest.mmio(MMIO_QUEUE_NOTIFY, 0);
    guest.load(7, 6, 0x600, 4);
    guest.store(7, 8, 0, 0);
    guest.ram(0x300, 2, 4);
    guest.ram(12, 2, 0x20001);
    guest.ram(0x102, 1, 3);
    guest.mmio(MMIO_QUEUE_NOTIFY, 0);
    guest.load(7, 6, 0x600, 4);
    guest.store(7, 8, 0, 0);
    guest.load(7, 6, 0x202, 5);
    guest.store(7, 8, 0, 0);
    guest.load(7, 5, MMIO_INTERRUPT_STATUS as u32, 2);
    guest.store(7, 8, 0, 0);
    guest.mmio(MMIO_INTERRUPT_ACK, 1);
    guest.load(7, 5, MMIO_INTERRUPT_STATUS as u32, 2);
    guest.store(7, 8, 0, 0);
    guest.mmio(MMIO_STATUS, 0);
    guest.load(7, 5, MMIO_QUEUE_READY as u32, 2);
    guest.store(7, 8, 0, 0);
    let (rom, steps) = guest.finish();
    let files = Files::new(&rom);
    let disk = files.directory.join("disk.raw");
    let result = files
        .command(steps)
        .arg("--root")
        .arg(&disk)
        .arg("--root-writable")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, [b'R', 0, 0, 0, 3, 1, 0, 0]);
    let diagnostic = String::from_utf8(result.stderr).unwrap();
    assert!(diagnostic.contains("Step budget exhausted:"));
    assert!(diagnostic.contains("traps=0"));
    let persisted = std::fs::read(disk).unwrap();
    assert_eq!(&persisted[..512], &[0; 512]);
    assert_eq!(&persisted[512..1024], &[b'R'; 512]);
    assert_eq!(persisted[1024], b'W');
    assert_eq!(&persisted[1025..], &[b'R'; 511]);
}

#[test]
fn default_media_is_read_only_and_guest_write_gets_ioerr() {
    let mut guest = Rom::block();
    guest.ram(0x300, 2, 1);
    guest.ram(0x400, 0, b'W' as u32);
    guest.ram(28, 2, 0x20001);
    guest.ram(0x102, 1, 2);
    guest.mmio(MMIO_QUEUE_NOTIFY, 0);
    guest.load(7, 6, 0x600, 4);
    guest.store(7, 8, 0, 0);
    let (rom, steps) = guest.finish();
    let files = Files::new(&rom);
    let disk = files.directory.join("disk.raw");
    let before = std::fs::read(&disk).unwrap();
    let result = files
        .command(steps)
        .arg("--root")
        .arg(&disk)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, [b'R', 0, 1]);
    assert_eq!(std::fs::read(disk).unwrap(), before);
}

#[test]
fn rom_loads_executable_sector_and_payload_reinitializes_the_used_controller() {
    let mut payload = Rom::new();
    payload.constant(5, ROOT_BLOCK.start as u32);
    payload.constant(8, UART.start as u32);
    payload.constant(7, b'K' as u32);
    payload.store(7, 8, 0, 0);
    payload.code.push(0xf14023f3);
    payload.store(7, 8, 0, 0);
    payload.mmio(MMIO_STATUS, 0);
    payload.load(7, 5, MMIO_INTERRUPT_STATUS as u32, 2);
    payload.store(7, 8, 0, 0);
    payload.load(7, 5, MMIO_QUEUE_READY as u32, 2);
    payload.store(7, 8, 0, 0);
    payload.initialize_block();
    payload.ram(16, 3, RAM.start as u32 + 0x700);
    payload.ram(0x308, 3, 2);
    payload.ram(0x202, 1, 0);
    payload.ram(0x600, 0, 0xff);
    payload.mmio(MMIO_QUEUE_NOTIFY, 0);
    payload.load(7, 6, 0x700, 4);
    payload.store(7, 8, 0, 0);
    payload.load(7, 6, 0x600, 4);
    payload.store(7, 8, 0, 0);
    payload.load(7, 6, 0x202, 5);
    payload.store(7, 8, 0, 0);
    payload.code.push(0x10500073);
    let payload: Vec<u8> = payload
        .code
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert!(payload.len() <= 512);

    let mut firmware = Rom::block();
    firmware.immediate(7, 6, 0, 0x400);
    firmware.code.push(0x00038067);
    let (rom, steps) = firmware.finish();
    let files = Files::new(&rom);
    let disk = files.directory.join("disk.raw");
    let mut sectors = vec![0; 1536];
    sectors[512..512 + payload.len()].copy_from_slice(&payload);
    sectors[1024..].fill(b'S');
    std::fs::write(&disk, &sectors).unwrap();
    let result = files
        .command(steps + 200)
        .arg("--root")
        .arg(&disk)
        .arg("--deny-traps")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, [payload[0], 0, b'K', 0, 0, 0, b'S', 0, 1]);
    assert_eq!(std::fs::read(disk).unwrap(), sectors);
}

#[test]
fn serial_script_larger_than_fifo_round_trips_without_overrun() {
    let mut guest = Rom::new();
    guest.constant(8, UART.start as u32);
    guest.load(7, 8, 0, 4);
    guest.store(7, 8, 0, 0);
    guest.code.push(0xff9ff06f);
    let (rom, _) = guest.finish();
    let files = Files::new(&rom);
    let input = files.directory.join("input.bin");
    let expected = std::fs::read(&input).unwrap();
    let result = files
        .command(2 + expected.len() as u64 * 3)
        .arg("--serial-input")
        .arg(input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, expected);
}

#[test]
fn runtime_preserves_uart_backpressure_and_reports_exact_virtual_steps() {
    let mut guest = Rom::new();
    guest.constant(8, UART.start as u32);
    guest.code.push(0x10500073);
    let (rom, _) = guest.finish();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 10, b"abc", &mut Vec::new()).unwrap();
    assert_eq!(report.steps, 10);
    assert_eq!(report.retired, 3);
    assert_eq!(report.waiting, 7);
    assert_eq!(report.input_accepted, 1);
    assert_eq!(report.output_bytes, 0);
    assert_eq!(machine.virtual_time(), 10);
    let mut byte = [0];
    machine.bus_mut().read(UART.start + 5, &mut byte).unwrap();
    assert_eq!(byte[0] & 2, 0);
    machine.bus_mut().read(UART.start, &mut byte).unwrap();
    assert_eq!(byte, [b'a']);
    assert_eq!(machine.bus().uart_input_capacity(), 1);
    assert!(matches!(
        run_headless(&mut machine, 0, b"", &mut Vec::new()),
        Err(RunError::InvalidStepBudget)
    ));
    assert_eq!(machine.virtual_time(), 10);
}

struct BrokenOutput;

impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected serial failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("injected flush failure"))
    }
}

#[test]
fn serial_write_and_flush_failures_are_explicit() {
    let mut guest = Rom::new();
    guest.constant(8, UART.start as u32);
    guest.constant(7, b'x' as u32);
    guest.store(7, 8, 0, 0);
    let (rom, steps) = guest.finish();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom.clone(), 4096).unwrap();
    let result = run_headless(&mut machine, steps, b"", &mut BrokenOutput);
    assert!(matches!(result, Err(RunError::Output(error)) if error.to_string().contains("serial")));
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let result = run_headless(&mut machine, 1, b"", &mut BrokenOutput);
    assert!(matches!(result, Err(RunError::Output(error)) if error.to_string().contains("flush")));
}

struct FailedMedia;

impl SectorMedia for FailedMedia {
    fn sector_count(&self) -> u64 {
        3
    }

    fn read_only(&self) -> bool {
        false
    }

    fn read_sectors(&mut self, _: u64, _: &mut [u8]) -> io::Result<()> {
        Err(io::Error::other("injected media failure"))
    }

    fn write_sectors(&mut self, _: u64, _: &[u8]) -> io::Result<()> {
        unreachable!()
    }

    fn flush(&mut self) -> io::Result<()> {
        unreachable!()
    }
}

#[test]
fn backend_failure_is_guest_ioerr_and_explicit_runtime_error() {
    let (rom, steps) = Rom::block().finish();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    machine
        .bus_mut()
        .attach_root_block(Box::new(FailedMedia))
        .unwrap();
    let result = run_headless(&mut machine, steps, b"", &mut Vec::new());
    assert!(
        matches!(result, Err(RunError::Media(error)) if error.to_string().contains("injected"))
    );
    let mut status = [0];
    machine
        .bus_mut()
        .read(RAM.start + 0x600, &mut status)
        .unwrap();
    assert_eq!(status, [1]);
}

#[test]
fn cli_rejects_bad_rom_media_and_incomplete_serial_without_false_success() {
    let (rom, steps) = Rom::new().finish();
    let files = Files::new(&rom);
    let result = files
        .command(steps)
        .arg("--serial-input")
        .arg(files.directory.join("input.bin"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("before all serial input"));
    let result = files
        .command(steps)
        .arg("--root")
        .arg(files.directory.join("missing.raw"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    std::fs::write(files.directory.join("disk.raw"), [0; 513]).unwrap();
    let result = files
        .command(steps)
        .arg("--root")
        .arg(files.directory.join("disk.raw"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("sector-aligned"));
    std::fs::write(files.directory.join("rom.bin"), [0; 8]).unwrap();
    let result = files.command(1).output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ROM must contain exactly"));
}

#[test]
fn strict_cli_rejects_traps_and_reports_the_cause() {
    let mut guest = Rom::new();
    guest.code.push(0xffff_ffff);
    let (rom, _) = guest.finish();
    let files = Files::new(&rom);
    let result = files.command(2).arg("--deny-traps").output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&result.stderr);
    assert!(diagnostic.contains("traps=2"));
    assert!(diagnostic.contains("synchronous trap(s)"));
    assert!(diagnostic.contains("InstructionAccessFault"));
}

#[test]
fn handled_guest_traps_resume_without_host_syscall_interpretation() {
    let mut guest = Rom::new();
    guest.constant(7, 0x1020);
    guest.code.push(0x30539073);
    guest.code.push(0x00000073);
    guest.code.push(0x10500073);
    while guest.code.len() < 8 {
        guest.code.push(0x00000013);
    }
    guest
        .code
        .extend([0x341023f3, 0x00438393, 0x34139073, 0x30200073]);
    let (rom, _) = guest.finish();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom.clone(), 4096).unwrap();
    let report = run_headless(&mut machine, 15, b"", &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 1);
    assert_eq!(report.retired, 8);
    assert_eq!(report.waiting, 6);
    assert_eq!(report.last_trap.unwrap().pc, 0x100c);
    let files = Files::new(&rom);
    let result = files.command(15).output().unwrap();
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&result.stderr);
    assert!(diagnostic.contains("traps=1"));
    assert!(diagnostic.contains("EnvironmentCallFromMachine"));
}

#[test]
fn media_handles_close_after_runtime_execution() {
    let (rom, steps) = Rom::block().finish();
    let files = Files::new(&rom);
    let disk = files.directory.join("disk.raw");
    {
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        machine
            .bus_mut()
            .attach_root_block(Box::new(FileSectorMedia::open(&disk, false).unwrap()))
            .unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, steps, b"", &mut output).unwrap();
        assert_eq!(output, [b'R', 0]);
        assert_eq!(report.traps, 0);
        assert_eq!(report.steps, report.retired + report.waiting);
    }
    std::fs::rename(&disk, files.directory.join("closed.raw")).unwrap();
    std::fs::rename(files.directory.join("closed.raw"), disk).unwrap();
}
