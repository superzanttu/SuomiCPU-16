//! Defines address boundaries, device-bank locations, and system identification.
//! Separates usable RAM from devices and supplies the checked guest register contract.

pub const ADDRESS_SPACE_SIZE: u32 = 1 << 30;
pub const DEVICE_APERTURE_START: u32 = 0x3ff0_0000;
pub const DEVICE_BANK_SIZE: u32 = 0x1000;
pub const SYSTEM_BASE: u32 = DEVICE_APERTURE_START;
pub const INTERRUPT_BASE: u32 = SYSTEM_BASE + DEVICE_BANK_SIZE;
pub const FAULT_BASE: u32 = INTERRUPT_BASE + DEVICE_BANK_SIZE;
pub const INPUT_BASE: u32 = FAULT_BASE + DEVICE_BANK_SIZE;
pub const CLOCK_BASE: u32 = INPUT_BASE + DEVICE_BANK_SIZE;
pub const GRAPHICS_BASE: u32 = CLOCK_BASE + DEVICE_BANK_SIZE;
pub const AUDIO_BASE: u32 = GRAPHICS_BASE + DEVICE_BANK_SIZE;
pub const NETWORK_BASE: u32 = AUDIO_BASE + DEVICE_BANK_SIZE;
pub const FILESYSTEM_BASE: u32 = NETWORK_BASE + DEVICE_BANK_SIZE;
pub const CONSOLE_BASE: u32 = FILESYSTEM_BASE + DEVICE_BANK_SIZE;
pub const FIRMWARE_TEXT_BASE: u32 = CONSOLE_BASE + DEVICE_BANK_SIZE;
pub const FIRMWARE_GATEWAY_BASE: u32 = FIRMWARE_TEXT_BASE + DEVICE_BANK_SIZE;

pub const SYSTEM_MACHINE_REVISION: u32 = SYSTEM_BASE;
pub const SYSTEM_DEVICE_ABI: u32 = SYSTEM_BASE + 4;
pub const SYSTEM_RAM_BYTES: u32 = SYSTEM_BASE + 8;
pub const SYSTEM_ADDRESS_BITS: u32 = SYSTEM_BASE + 12;
pub const SYSTEM_BANKS: u32 = SYSTEM_BASE + 16;
pub const MACHINE_REVISION: u32 = 1;
pub const DEVICE_ABI: u32 = 1;
pub const IMPLEMENTED_BANKS: u32 = 1;

pub const fn usable_ram_size(backing_size: usize) -> usize {
    if backing_size < DEVICE_APERTURE_START as usize {
        backing_size
    } else {
        DEVICE_APERTURE_START as usize
    }
}

pub const fn is_device_aperture(address: u32) -> bool {
    address >= DEVICE_APERTURE_START && address < ADDRESS_SPACE_SIZE
}

pub const fn is_implemented_device_page(address: u32) -> bool {
    address >= SYSTEM_BASE && address < SYSTEM_BASE + DEVICE_BANK_SIZE
}

pub fn system_register_byte(address: u32, backing_size: usize) -> Option<u8> {
    let offset = address.checked_sub(SYSTEM_BASE)?;
    let value = match offset / 4 {
        0 => MACHINE_REVISION,
        1 => DEVICE_ABI,
        2 => usable_ram_size(backing_size) as u32,
        3 => 30,
        4 => IMPLEMENTED_BANKS,
        _ => return None,
    };
    Some(value.to_be_bytes()[(offset % 4) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aperture_is_reserved_independently_of_configured_backing() {
        for size in [
            0,
            8,
            65536,
            1 << 20,
            1 << 29,
            DEVICE_APERTURE_START as usize,
        ] {
            assert_eq!(usable_ram_size(size), size);
        }
        assert_eq!(usable_ram_size(1 << 30), 0x3ff0_0000);
        assert!(!is_device_aperture(DEVICE_APERTURE_START - 1));
        assert!(is_device_aperture(DEVICE_APERTURE_START));
        assert!(is_device_aperture(ADDRESS_SPACE_SIZE - 1));
        assert!(!is_device_aperture(ADDRESS_SPACE_SIZE));
    }

    #[test]
    fn system_registers_are_big_endian_and_reserved_offsets_are_absent() {
        for size in [1 << 16, 1 << 20, ADDRESS_SPACE_SIZE as usize] {
            for (address, value) in [
                (SYSTEM_MACHINE_REVISION, MACHINE_REVISION),
                (SYSTEM_DEVICE_ABI, DEVICE_ABI),
                (SYSTEM_RAM_BYTES, usable_ram_size(size) as u32),
                (SYSTEM_ADDRESS_BITS, 30),
                (SYSTEM_BANKS, IMPLEMENTED_BANKS),
            ] {
                for (offset, byte) in value.to_be_bytes().into_iter().enumerate() {
                    assert_eq!(
                        system_register_byte(address + offset as u32, size),
                        Some(byte)
                    );
                }
            }
        }
        for address in [
            SYSTEM_BASE - 1,
            SYSTEM_BASE + 20,
            INTERRUPT_BASE,
            ADDRESS_SPACE_SIZE,
        ] {
            assert_eq!(system_register_byte(address, 1 << 20), None);
        }
        assert!(is_implemented_device_page(SYSTEM_BASE));
        assert!(is_implemented_device_page(
            SYSTEM_BASE + DEVICE_BANK_SIZE - 1
        ));
        assert!(!is_implemented_device_page(INTERRUPT_BASE));
    }

    #[test]
    fn guest_machine_constants_match_the_host_contract() {
        let header = include_str!("../../toolchain/include/sc16_machine.h");
        let expected = [
            ("DEVICE_APERTURE_START", DEVICE_APERTURE_START),
            ("DEVICE_BANK_SIZE", DEVICE_BANK_SIZE),
            ("SYSTEM_BASE", SYSTEM_BASE),
            ("INTERRUPT_BASE", INTERRUPT_BASE),
            ("FAULT_BASE", FAULT_BASE),
            ("INPUT_BASE", INPUT_BASE),
            ("CLOCK_BASE", CLOCK_BASE),
            ("GRAPHICS_BASE", GRAPHICS_BASE),
            ("AUDIO_BASE", AUDIO_BASE),
            ("NETWORK_BASE", NETWORK_BASE),
            ("FILESYSTEM_BASE", FILESYSTEM_BASE),
            ("CONSOLE_BASE", CONSOLE_BASE),
            ("FIRMWARE_TEXT_BASE", FIRMWARE_TEXT_BASE),
            ("FIRMWARE_GATEWAY_BASE", FIRMWARE_GATEWAY_BASE),
            ("SYSTEM_MACHINE_REVISION", SYSTEM_MACHINE_REVISION),
            ("SYSTEM_DEVICE_ABI", SYSTEM_DEVICE_ABI),
            ("SYSTEM_RAM_BYTES", SYSTEM_RAM_BYTES),
            ("SYSTEM_ADDRESS_BITS", SYSTEM_ADDRESS_BITS),
            ("SYSTEM_BANKS", SYSTEM_BANKS),
        ];
        let definitions: Vec<_> = header
            .lines()
            .filter(|line| line.starts_with("#define SC16_"))
            .filter(|line| !line.starts_with("#define SC16_MACHINE_H"))
            .collect();
        assert_eq!(definitions.len(), expected.len());
        for (name, value) in expected {
            assert!(
                definitions.contains(&format!("#define SC16_{name} 0x{value:08x}UL").as_str()),
                "{name}"
            );
        }
    }
}
