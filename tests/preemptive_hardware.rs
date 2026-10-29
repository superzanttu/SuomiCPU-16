//! Verifies hardware support for a guest-owned preemptive kernel.
//! Executes guest timer scheduling and SVC handling without host tasks, checking
//! isolated mappings, translated user entry, and register/stack preservation.

use std::fmt::Write;

use suomicpu16::{Cpu, CpuError, assembler::assemble, cpu::PrivilegeMode};

fn data(source: &mut String, address: u32, bytes: &[u8]) {
    write!(source, "\n.address {address:#x}\n.data hex ").unwrap();
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            source.push_str(", ");
        }
        write!(source, "{byte:02x}").unwrap();
    }
    source.push('\n');
}

#[test]
fn supervisor_executes_reads_from_the_high_system_bus_with_small_ram() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { SYSTEM_BASE };
        let source = format!(
            "LDI30 R2, {base}\nLDW R0, R2\n\
             LDI30 R2, {}\nLDW R1, R2\n\
             LDI30 R2, {}\nLDW R3, R2\nHALT",
            base + 2,
            base + 8
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.memory[..20].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            cpu.memory[0x3004..0x3008].copy_from_slice(&0x100bu32.to_be_bytes());
            cpu.memory[0x3024..0x3028].copy_from_slice(&(SYSTEM_BASE | 3).to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(16).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 0);
        assert_eq!(cpu.registers[1], i64::from(MACHINE_REVISION));
        assert_eq!(cpu.registers[3], 1);
        assert_eq!(cpu.current_task(), None);
        assert!(cpu.memory[..20].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn high_pit_mask_latches_and_unmask_delivers_to_an_unmanaged_guest_kernel() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { INTERRUPT_BASE };
        let source = format!(
            "LDI30 R6, {}\nLDI R0, 0\nST R6, R0\n\
             LDI30 R6, {}\nLDI R0, 1\nSTW R6, R0\n\
             EI\nINC R1\nLDI30 R6, {}\nST R6, R0\nHALT\n\
             .address 0x1100\nINC R5\n\
             LDI30 R6, {}\nLDI R0, 0\nSTW R6, R0\n\
             LDI30 R6, {}\nLDI R0, 16\nST R6, R0\nRTI",
            base + 6,
            base + 8,
            base + 6,
            base + 8,
            base + 5,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            suomicpu16::interrupts::IDT_VECTOR_PIT.into(),
            suomicpu16::interrupts::IdtDescriptor {
                handler: 0x1100,
                present: true,
                privilege: 0,
                gate_type: suomicpu16::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            for (page, entry) in [(1, 0x100bu32), (9, INTERRUPT_BASE | 3), (15, 0xf003)] {
                let offset = 0x3000 + page * 4;
                cpu.memory[offset..offset + 4].copy_from_slice(&entry.to_be_bytes());
            }
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(100).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[1], 1);
        assert_eq!(cpu.registers[5], 1);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.read(INTERRUPT_PENDING), 0);
        assert_eq!(cpu.read_u16(INTERRUPT_PIT_PERIOD), 0);
    }
}

#[test]
fn guest_fault_handler_reads_the_high_fault_bank_after_a_denied_device_alias() {
    use suomicpu16::machine::*;

    let image = assemble(
        "LDI30 R2, 0x9000\nLD R0, R2\nHALT\n\
         .address 0x1100\nLDI R2, 0\nLDI R3, 0xa000\nLD32 R4, R2\n\
         LDI30 R2, 0xa004\nLDW R0, R2\n\
         LDI30 R2, 0xa006\nLD R1, R2\nHALT",
        0x1000,
    )
    .unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.load_idt(0x800).unwrap();
    cpu.set_idt_descriptor(
        2,
        suomicpu16::interrupts::IdtDescriptor {
            handler: 0x1100,
            present: true,
            privilege: 0,
            gate_type: suomicpu16::interrupts::GateType::Interrupt,
        },
    )
    .unwrap();
    cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
    for (page, entry) in [
        (1, 0x100fu32),
        (7, 0x7007),
        (9, INTERRUPT_BASE | 0xf),
        (10, FAULT_BASE | 3),
        (15, 0xf003),
    ] {
        let offset = 0x3000 + page * 4;
        cpu.memory[offset..offset + 4].copy_from_slice(&entry.to_be_bytes());
    }
    cpu.enable_mmu(0x2000).unwrap();
    cpu.enter_user_mode(0x1000, 0x8000).unwrap();
    cpu.run(32).unwrap();
    assert!(!cpu.running);
    assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
    assert_eq!(cpu.current_task(), None);
    assert_eq!(cpu.registers[4], 0);
    assert_eq!(cpu.registers[5], 0x9000);
    assert_eq!(cpu.registers[0], 0x0104);
    assert_eq!(cpu.registers[1], 1);
    assert_eq!(cpu.last_page_fault().unwrap().virtual_address, 0x9000);
}

#[test]
fn supervisor_executes_clock_snapshots_without_ram_backing_or_host_tasks() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { CLOCK_BASE };
        let source = format!(
            "LDI30 R2, {base}\nLDI R0, 3\nST R2, R0\n\
             LDI R2, {}\nLDI R3, {}\nLD32 R4, R2\n\
             LD32 R6, R2\n\
             LDI30 R2, {}\nLDW R0, R2\n\
             LDI30 R2, {}\nLDW R1, R2\nHALT",
            (base + 16) >> 16,
            (base + 16) & 0xffff,
            base + 8,
            base + 12,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.update_rtc(23 * 3600 + 59 * 60 + 58);
        cpu.frame_count = 0x1234;
        cpu.memory[0x4000..0x4014].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            cpu.memory[0x3004..0x3008].copy_from_slice(&0x100bu32.to_be_bytes());
            cpu.memory[0x3024..0x3028].copy_from_slice(&(CLOCK_BASE | 3).to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.registers[0], 0x1234);
        assert_eq!(cpu.registers[1], 0x3a3b);
        assert_eq!(cpu.registers[4..6], cpu.registers[6..8]);
        assert_ne!(cpu.registers[4] | cpu.registers[5], 0);
        assert!(cpu.memory[0x4000..0x4014].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn supervisor_consumes_high_input_events_with_small_ram_and_translated_aliases() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { INPUT_BASE };
        let source = format!(
            "LDI30 R2, {base}\nLD R0, R2\nLD R1, R2\n\
             LDI R2, {}\nLDI R3, {}\nLD32 R4, R2\n\
             LDI30 R2, {}\nLDW R6, R2\n\
             LDI30 R2, {}\nLDW R7, R2\n\
             LDI30 R2, {}\nLD R3, R2\nHALT",
            (base + 4) >> 16,
            (base + 4) & 0xffff,
            base + 8,
            base + 10,
            base + 13,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.signal_keyboard_event(b'Z');
        cpu.set_key(0x12ab34, true);
        cpu.set_mouse_position(600, 400);
        cpu.mouse_button(1, true, 600, 400, 100);
        cpu.memory[0x3000..0x3010].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x500fu32.to_be_bytes());
            cpu.memory[0x5004..0x5008].copy_from_slice(&0x100bu32.to_be_bytes());
            cpu.memory[0x5024..0x5028].copy_from_slice(&(INPUT_BASE | 3).to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.registers[0], i64::from(b'Z'));
        assert_eq!(cpu.registers[1], 0);
        assert_eq!(cpu.registers[3], 2);
        assert_eq!(cpu.registers[4], 0x12);
        assert_eq!(cpu.registers[5], 0xab34);
        assert_eq!(cpu.registers[6], 319);
        assert_eq!(cpu.registers[7], 239);
        assert_eq!(cpu.read(INPUT_KEY), 0);
        assert_eq!(cpu.read(INPUT_MOUSE_EVENTS), 0);
        assert!(cpu.memory[0x3000..0x3010].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn supervisor_executes_high_console_and_firmware_text_with_small_ram() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let console = if translated { 0x9000 } else { CONSOLE_BASE };
        let text = if translated {
            0xa000
        } else {
            FIRMWARE_TEXT_BASE
        };
        let source = format!(
            "LDI30 R2, {}\nLD R1, R2\n\
             LDI30 R2, {console}\nLD R0, R2\n\
             LDI R3, 33\nST R2, R3\n\
             LDI30 R2, {}\nLDI R3, 2\nST R2, R3\n\
             LDI30 R2, {}\nLDI R3, 24\nST R2, R3\n\
             LDI30 R2, {}\nLDI R3, 52\nST R2, R3\n\
             LDI30 R2, {text}\nLDI R3, 9\nST R2, R3\n\
             LDI R3, 65\nST R2, R3\n\
             LDI30 R2, {}\nLD R4, R2\n\
             LDI30 R2, {}\nLD R5, R2\nHALT",
            console + 1,
            text + 3,
            text + 4,
            text + 5,
            text + 4,
            text + 5,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.receive_console_input(b"K");
        cpu.memory[0x9000..0x9008].fill(0xa5);
        cpu.memory[0xa000..0xa008].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            for (page, entry) in [
                (1, 0x100bu32),
                (9, CONSOLE_BASE | 3),
                (10, FIRMWARE_TEXT_BASE | 3),
            ] {
                let offset = 0x3000 + page * 4;
                cpu.memory[offset..offset + 4].copy_from_slice(&entry.to_be_bytes());
            }
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.registers[0], i64::from(b'K'));
        assert_eq!(cpu.registers[1], 3);
        assert_eq!(cpu.registers[4], 24);
        assert_eq!(cpu.registers[5], 4);
        assert_eq!(cpu.console.output(), b"K!");
        assert!(cpu.console.receive_buffer().is_empty());
        assert!(cpu.guest_text.enabled);
        let index = 24 * suomicpu16::guest_text::GUEST_TEXT_COLUMNS + 3;
        assert_eq!(cpu.guest_text.cells[index].character, b'A');
        assert!(cpu.memory[0x9000..0x9008].iter().all(|byte| *byte == 0xa5));
        assert!(cpu.memory[0xa000..0xa008].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn supervisor_executes_high_audio_commands_with_small_ram_and_translated_aliases() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { AUDIO_BASE };
        let source = format!(
            "LDI30 R2, {}\nLDI R0, 3\nST R2, R0\n\
             LDI30 R2, {}\nLDI R0, 440\nSTW R2, R0\n\
             LDI30 R2, {}\nLDI R0, 60\nSTW R2, R0\n\
             LDI30 R2, {}\nLDI R0, 2\nSTW R2, R0\n\
             LDI30 R2, {}\nLDI R0, 75\nSTW R2, R0\n\
             LDI30 R2, {base}\nLDI R0, 1\nST R2, R0\n\
             LDI30 R2, {}\nLD R4, R2\n\
             LDI30 R2, {}\nLD R5, R2\nHALT",
            base + 2,
            base + 4,
            base + 6,
            base + 8,
            base + 10,
            base + 1,
            base + 12,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.memory[0x6000..0x6010].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            cpu.memory[0x3004..0x3008].copy_from_slice(&0x100bu32.to_be_bytes());
            cpu.memory[0x3024..0x3028].copy_from_slice(&(AUDIO_BASE | 3).to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.registers[4], 0);
        assert_eq!(cpu.registers[5], 8);
        assert_eq!(
            cpu.take_sound_commands(),
            [suomicpu16::cpu::GpuSoundCommand {
                channel: 3,
                frequency: 440,
                frames: 60,
                waveform: 2,
                volume: 75,
            }]
        );
        assert!(cpu.take_sound_commands().is_empty());
        assert_eq!(cpu.read(AUDIO_PENDING), 0);
        assert!(cpu.memory[0x6000..0x6010].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn supervisor_executes_high_network_commands_with_small_ram_and_translated_aliases() {
    use suomicpu16::machine::*;

    for translated in [false, true] {
        let base = if translated { 0x9000 } else { NETWORK_BASE };
        let source = format!(
            "LDI30 R2, {}\nLDI R0, 49\nST R2, R0\n\
             LDI30 R2, {base}\nLDI R0, 3\nST R2, R0\n\
             LDI30 R2, {}\nLD R4, R2\n\
             LDI30 R2, {}\nLDI R0, 1\nST R2, R0\n\
             LDI30 R2, {base}\nLDI R0, 5\nST R2, R0\n\
             LDI30 R2, {}\nLDW R5, R2\n\
             LDI30 R2, {}\nLD R6, R2\nHALT",
            base + 2,
            base + 1,
            base + 3,
            base + 6,
            base + 1,
        );
        let image = assemble(&source, 0x1000).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.memory[0x7000..0x7080].fill(0xa5);
        if translated {
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            cpu.memory[0x3004..0x3008].copy_from_slice(&0x100bu32.to_be_bytes());
            cpu.memory[0x3024..0x3028].copy_from_slice(&(NETWORK_BASE | 3).to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
        }
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.registers[4], 2);
        assert_eq!(cpu.registers[5], 65535);
        assert_eq!(cpu.registers[6], 0);
        assert!(cpu.memory[0x7000..0x7080].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn unmanaged_svc_always_enters_the_guest_kernel() {
    for abi_register in [0, 1, 77] {
        let image = assemble("SVC\nHALT\n.address 0x300\nINC R5\nRTI", 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            0x80,
            suomicpu16::interrupts::IdtDescriptor {
                handler: 0x300,
                present: true,
                privilege: 3,
                gate_type: suomicpu16::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
        cpu.enter_user_mode(0x100, 0x7000).unwrap();
        cpu.registers[0] = 0x1234;
        cpu.registers[7] = abi_register;

        cpu.step().unwrap();

        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.pc, 0x300, "R7 = {abi_register}");
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.registers[0], 0x1234);
        cpu.run(2).unwrap();
        assert_eq!(cpu.pc, 0x102);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.registers[5], 1);
        assert_eq!(cpu.registers[7], abi_register);
    }
}

#[test]
fn translated_user_entry_obeys_page_permissions_in_both_execution_modes() {
    use suomicpu16::mmu::{MemoryAccess, PageFault, PageFaultReason};

    for managed in [false, true] {
        for (flags, reason) in [
            (0xfu32, None),
            (7, Some(PageFaultReason::NotExecutable)),
            (0xb, Some(PageFaultReason::SupervisorOnly)),
        ] {
            let image = assemble("HALT", 0x4000).unwrap();
            let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
            cpu.memory[0x3014..0x3018].copy_from_slice(&(0x4000 | flags).to_be_bytes());
            cpu.memory[0x301c..0x3020].copy_from_slice(&0xa007u32.to_be_bytes());
            cpu.enable_mmu(0x2000).unwrap();
            let result = if managed {
                cpu.create_user_task(0x5000, 0x8000, 0xf000, Some(0x2000))
                    .and_then(|task| cpu.start_task(task))
            } else {
                cpu.enter_user_mode(0x5000, 0x8000)
            };
            if let Some(reason) = reason {
                assert_eq!(
                    result,
                    Err(CpuError::PageFault(PageFault {
                        virtual_address: 0x5000,
                        access: MemoryAccess::Fetch,
                        reason,
                    }))
                );
                assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
                assert_eq!(cpu.pc, 0x4000);
                assert_eq!(cpu.current_task(), None);
            } else {
                result.unwrap();
                cpu.step().unwrap();
                assert!(!cpu.running);
                assert_eq!(cpu.pc, 0x5002);
                assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
            }
        }
    }
}

fn translated_kernel_stack(second_page_flags: u32) -> Cpu {
    let image = assemble(
        "PUSH R4\nSVC\nPOP R4\nRTI\n.address 0x1100\nINC R5\nRTI\n.address 0x5000\nSVC\nHALT",
        0x1000,
    )
    .unwrap();
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.load_idt(0x800).unwrap();
    cpu.set_idt_descriptor(
        128,
        suomicpu16::interrupts::IdtDescriptor {
            handler: 0x1000,
            present: true,
            privilege: 3,
            gate_type: suomicpu16::interrupts::GateType::Interrupt,
        },
    )
    .unwrap();
    cpu.memory[0x2000..0x2004].copy_from_slice(&0x300fu32.to_be_bytes());
    for (page, physical, flags) in [
        (1, 0x1000, 0xb),
        (5, 0x5000, 0xd),
        (7, 0x7000, 7),
        (9, 0x20000, 3),
        (10, 0x30000, second_page_flags),
    ] {
        let entry = 0x3000 + page * 4;
        cpu.memory[entry..entry + 4].copy_from_slice(&(physical | flags).to_be_bytes());
    }
    cpu.enable_mmu(0x2000).unwrap();
    cpu.sp = 0xa008;
    cpu.enter_user_mode(0x5000, 0x8000).unwrap();
    cpu.registers[4] = 0x1234;
    cpu
}

#[test]
fn translated_kernel_stack_preserves_cross_page_and_nested_event_frames() {
    let mut cpu = translated_kernel_stack(3);
    cpu.memory[0x9fe0..0xa008].fill(0xa5);

    cpu.step().unwrap();

    assert_eq!(cpu.sp, 0x9ff8);
    assert_eq!(&cpu.memory[0x20ff8..0x20ffc], &0x5002u32.to_be_bytes());
    assert_eq!(&cpu.memory[0x30000..0x30004], &0x8000u32.to_be_bytes());
    cpu.set_idt_descriptor(
        128,
        suomicpu16::interrupts::IdtDescriptor {
            handler: 0x1100,
            present: true,
            privilege: 3,
            gate_type: suomicpu16::interrupts::GateType::Interrupt,
        },
    )
    .unwrap();
    cpu.run(6).unwrap();

    assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
    assert_eq!(cpu.pc, 0x5002);
    assert_eq!(cpu.sp, 0x8000);
    assert_eq!(cpu.kernel_sp, 0xa008);
    assert_eq!(cpu.registers[4], 0x1234);
    assert_eq!(cpu.registers[5], 1);
    assert!(cpu.memory[0x9fe0..0xa008].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn event_frame_validation_prevents_partial_writes() {
    for flags in [0, 1] {
        let mut cpu = translated_kernel_stack(flags);
        cpu.memory[0x20ff8..0x21000].fill(0xa5);
        cpu.memory[0x30000..0x30008].fill(0x5a);

        assert_eq!(cpu.step(), Err(CpuError::DoubleFault), "flags {flags}");
        assert!(!cpu.running);
        assert_eq!(cpu.sp, 0x8000);
        assert_eq!(cpu.kernel_sp, 0xa008);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert!(
            cpu.memory[0x20ff8..0x21000]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        assert!(
            cpu.memory[0x30000..0x30008]
                .iter()
                .all(|byte| *byte == 0x5a)
        );
    }
}

#[test]
fn event_frames_reject_device_backing_aliases_before_reading_or_writing() {
    for physical in [0x43000u32, 0x50000, 0x75000] {
        let mut cpu = translated_kernel_stack(3);
        cpu.memory[0x3028..0x302c].copy_from_slice(&(physical | 3).to_be_bytes());
        cpu.memory[0x20ff8..0x21000].fill(0xa5);
        cpu.memory[physical as usize..physical as usize + 8].fill(0x5a);
        assert_eq!(cpu.step(), Err(CpuError::DoubleFault));
        assert!(!cpu.running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.sp, 0x8000);
        assert_eq!(cpu.kernel_sp, 0xa008);
        assert!(
            cpu.memory[0x20ff8..0x21000]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        assert!(
            cpu.memory[physical as usize..physical as usize + 8]
                .iter()
                .all(|byte| *byte == 0x5a)
        );

        let mut cpu = translated_kernel_stack(3);
        cpu.step().unwrap();
        cpu.memory[0x3028..0x302c].copy_from_slice(&(physical | 3).to_be_bytes());
        cpu.pc = 0x1102;
        let registers = cpu.registers;
        let flags = cpu.flags;
        assert_eq!(cpu.step(), Err(CpuError::DoubleFault));
        assert!(!cpu.running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.sp, 0x9ff8);
        assert_eq!(cpu.kernel_sp, 0x9ff8);
        assert_eq!(cpu.registers, registers);
        assert_eq!(cpu.flags, flags);
    }
}

#[test]
fn rti_rejects_reserved_aperture_targets_even_when_user_mapped() {
    let aperture = suomicpu16::machine::DEVICE_APERTURE_START;
    for (offset, value) in [(0, aperture), (8, aperture + 0x1000)] {
        let mut cpu = translated_kernel_stack(3);
        cpu.step().unwrap();
        cpu.memory[0x27fc..0x2800].copy_from_slice(&0x400fu32.to_be_bytes());
        cpu.memory[0x4400..0x4404].copy_from_slice(&0x500fu32.to_be_bytes());
        assert_eq!(
            suomicpu16::mmu::translate(
                &cpu.memory,
                0x2000,
                aperture,
                suomicpu16::mmu::MemoryAccess::Fetch,
                true
            )
            .unwrap(),
            0x5000
        );
        let physical = if offset == 0 { 0x20ff8 } else { 0x30000 };
        cpu.memory[physical..physical + 4].copy_from_slice(&value.to_be_bytes());
        cpu.pc = 0x1102;
        let registers = cpu.registers;
        let flags = cpu.flags;
        assert_eq!(cpu.step(), Err(CpuError::DoubleFault));
        assert!(!cpu.running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.sp, 0x9ff8);
        assert_eq!(cpu.registers, registers);
        assert_eq!(cpu.flags, flags);
    }
}

#[test]
fn rti_rejects_invalid_user_targets_before_restoring_context() {
    for (offset, value) in [(0, 0x6000u32), (8, 0x6000)] {
        let mut cpu = translated_kernel_stack(3);
        cpu.step().unwrap();
        let physical = if offset == 0 { 0x20ff8 } else { 0x30000 };
        cpu.memory[physical..physical + 4].copy_from_slice(&value.to_be_bytes());
        cpu.pc = 0x1102;
        let sp = cpu.sp;
        let flags = cpu.flags;

        assert_eq!(cpu.step(), Err(CpuError::DoubleFault));
        assert!(!cpu.running);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.sp, sp);
        assert_eq!(cpu.flags, flags);
    }
}

#[test]
fn guest_kernel_preempts_and_switches_isolated_address_spaces_without_host_tasks() {
    let mut source = String::from(
        r#"
    LDI R0, 0
    LDI R1, 0x800
    LIDT R0
    LDI R0, 0
    SETSPH R0
    LDI R0, 0xb000
    SETSP R0
    LDI30 R6, 0x3ff01008
    LDI R0, 128
    STW R6, R0
    LDI R4, 0
    LDI R5, 0x2000
    MMUON R4
    LDI R0, 0
    LDI R1, 0x5000
    LDI R2, 0
    LDI R3, 0x9000
    EI
    ENTERU R0, R2

.address 0x1400
timer_handler:
    PUSH R0
    PUSH R1
    PUSH R2
    PUSH R3
    PUSH R4
    PUSH R5
    PUSH R6
    PUSH R7
    LDI R2, 0x1810
    LDW R3, R2
    ADD R3, 1
    STW R2, R3
    MOVSP R0
    LDI R2, 0x1800
    LD R1, R2
    CMP R1, 0
    BNZ switch_from_b
    LDI R1, 1
    ST R2, R1
    LDI R2, 0x1802
    STW R2, R0
    LDI R2, 0x1804
    LDW R0, R2
    LDI R4, 0
    LDI R5, 0x2800
    MMUSWITCH R4
    SETSP R0
    JMPX restore
switch_from_b:
    LDI R1, 0
    ST R2, R1
    LDI R2, 0x1804
    STW R2, R0
    LDI R2, 0x1802
    LDW R0, R2
    LDI R4, 0
    LDI R5, 0x2000
    MMUSWITCH R4
    SETSP R0
restore:
    POP R7
    POP R6
    POP R5
    POP R4
    POP R3
    POP R2
    POP R1
    POP R0
    RTI

.address 0x1600
svc_handler:
    PUSH R0
    PUSH R1
    LDI R0, 0x1812
    LDW R1, R0
    ADD R1, 1
    STW R0, R1
    POP R1
    POP R0
    RTI
.address 0x1700
unexpected_fault:
    HALT
"#,
    );
    for (vector, handler, privilege) in [
        (0u32, 0x1700u32, 0u8),
        (1, 0x1700, 0),
        (2, 0x1700, 0),
        (34, 0x1400, 0),
        (128, 0x1600, 3),
    ] {
        let mut descriptor = [0; 8];
        descriptor[..4].copy_from_slice(&handler.to_be_bytes());
        descriptor[4] = 0x80 | (privilege << 1);
        data(&mut source, 0x800 + vector * 8, &descriptor);
    }
    data(&mut source, 0x1800, &[0, 0, 0, 0, 0xbf, 0xe0]);
    for (root, table, code, private, stack) in [
        (0x2000, 0x3000u32, 0x20000u32, 0x70000u32, 0x60000u32),
        (0x2800, 0x4000u32, 0x30000, 0x71000, 0x61000),
    ] {
        data(&mut source, root, &(table | 0xf).to_be_bytes());
        for page in [0, 1, 2, 3, 4, 10, 11] {
            data(
                &mut source,
                table + page * 4,
                &((page << 12) | 0xb).to_be_bytes(),
            );
        }
        for (page, physical, flags) in [(5, code, 0xd), (6, private, 7), (8, stack, 7)] {
            data(
                &mut source,
                table + page * 4,
                &(physical | flags).to_be_bytes(),
            );
        }
    }
    let mut context = [0u8; 32];
    context[16..20].copy_from_slice(&0x5000u32.to_be_bytes());
    context[22..24].copy_from_slice(&3u16.to_be_bytes());
    context[24..28].copy_from_slice(&0x9000u32.to_be_bytes());
    context[28..30].copy_from_slice(&34u16.to_be_bytes());
    context[30..32].copy_from_slice(&1u16.to_be_bytes());
    data(&mut source, 0xbfe0, &context);
    let mut image = assemble(&source, 0x1000).unwrap();
    for (physical, sentinel) in [(0x20000, 0x1111), (0x30000, 0x2222)] {
        let user_source = format!(
            r#"
    LDI R0, 0
    LDI R2, 0x6000
    LDI R4, {sentinel}
    LDI R7, 77
loop:
    LDI R3, {sentinel}
    CMP R4, R3
    BNZ corrupt
    PUSH R4
    POP R5
    CMP R5, R4
    BNZ corrupt
    ADD R0, 1
    STW R2, R0
    LDI R3, 0x6002
    STW R3, R4
    SVC
    JMPX loop
corrupt:
    LDI R0, 0xffff
    STW R2, R0
    HALT
"#
        );
        let user = assemble(&user_source, 0x5000).unwrap();
        for mut segment in user.segments {
            segment.address = physical + (segment.address - 0x5000);
            image.segments.push(segment);
        }
    }
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();

    cpu.run(100_000).unwrap();

    assert!(cpu.running, "guest kernel/task stopped at {:#x}", cpu.pc);
    assert!(cpu.is_mmu_enabled());
    assert_eq!(cpu.current_task(), None);
    let word = |address| u16::from_be_bytes([cpu.memory[address], cpu.memory[address + 1]]);
    assert!(
        word(0x1810) > 100,
        "guest timer handler did not switch tasks"
    );
    assert!(word(0x1812) > 100, "guest SVC handler did not run");
    for (private, sentinel) in [(0x70000, 0x1111), (0x71000, 0x2222)] {
        let progress = word(private);
        assert!(
            progress > 100 && progress < 0xffff,
            "invalid progress: {progress}"
        );
        assert_eq!(word(private + 2), sentinel);
    }
    assert_eq!(word(0x60ffe), 0x1111);
    assert_eq!(word(0x61ffe), 0x2222);
}
