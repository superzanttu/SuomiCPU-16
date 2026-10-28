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
    LDI30 R6, 0x4301e
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
