//! Exposes the SC-16 emulator, assembler, C compiler, and shared tooling.
//! Declares module locations and re-exports the main CPU types and constants.

#[path = "emulator/accounts.rs"]
pub mod accounts;
#[path = "assembler/mod.rs"]
pub mod assembler;
#[path = "emulator/bios_settings.rs"]
pub mod bios_settings;
#[path = "c_compiler/mod.rs"]
pub mod c_compiler;
#[path = "emulator/console.rs"]
pub mod console;
#[path = "emulator/console_font.rs"]
pub mod console_font;
#[path = "emulator/cpu.rs"]
pub mod cpu;
#[path = "emulator/disk.rs"]
pub mod disk;
#[path = "emulator/guest_text.rs"]
pub mod guest_text;
#[path = "emulator/image.rs"]
pub mod image;
#[path = "emulator/interrupts.rs"]
pub mod interrupts;
#[path = "emulator/isa.rs"]
pub mod isa;
pub mod kiuas16;
pub mod kiuas16_abi;
#[path = "emulator/machine.rs"]
pub mod machine;
#[path = "emulator/mmu.rs"]
pub mod mmu;
#[path = "emulator/net.rs"]
pub mod net;
#[path = "emulator/runtime.rs"]
pub mod runtime;
#[path = "emulator/sc_net.rs"]
pub mod sc_net;
#[path = "emulator/scheduler.rs"]
pub mod scheduler;
#[path = "emulator/syscall.rs"]
pub mod syscall;

pub use cpu::{ADDRESS_MASK, Cpu, CpuError, Flags, MEMORY_SIZE, MemorySegment};

#[path = "tools/programs.rs"]
pub mod programs;
