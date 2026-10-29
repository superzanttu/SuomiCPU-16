//! Defines address boundaries, device banks, system identification, and interrupt state.
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
pub const INTERRUPT_ENABLE: u32 = INTERRUPT_BASE;
pub const INTERRUPT_PENDING: u32 = INTERRUPT_BASE + 4;
pub const INTERRUPT_ACK: u32 = INTERRUPT_BASE + 5;
pub const INTERRUPT_PIT_ENABLE: u32 = INTERRUPT_BASE + 6;
pub const INTERRUPT_PIT_PERIOD: u32 = INTERRUPT_BASE + 8;
pub const INTERRUPT_PIT_COUNTDOWN: u32 = INTERRUPT_BASE + 12;
pub const FAULT_ADDRESS: u32 = FAULT_BASE;
pub const FAULT_INFO: u32 = FAULT_BASE + 4;
pub const FAULT_VALID: u32 = FAULT_BASE + 6;
pub const MACHINE_REVISION: u32 = 1;
pub const DEVICE_ABI: u32 = 1;
pub const IMPLEMENTED_BANKS: u32 = 7;

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
    address >= SYSTEM_BASE && address < FAULT_BASE + DEVICE_BANK_SIZE
}

pub fn device_register_writable(address: u32) -> Option<bool> {
    match address {
        address if (SYSTEM_BASE..SYSTEM_BANKS + 4).contains(&address) => Some(false),
        INTERRUPT_ENABLE | INTERRUPT_ACK | INTERRUPT_PIT_ENABLE => Some(true),
        INTERRUPT_PENDING => Some(false),
        address if (INTERRUPT_PIT_PERIOD..INTERRUPT_PIT_PERIOD + 2).contains(&address) => {
            Some(true)
        }
        address if (INTERRUPT_PIT_COUNTDOWN..INTERRUPT_PIT_COUNTDOWN + 2).contains(&address) => {
            Some(false)
        }
        FAULT_ADDRESS..=FAULT_VALID => Some(false),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct InterruptController {
    pub control: u8,
    pub pit_enabled: bool,
    pub period: u16,
    pub countdown: u16,
    period_latch: u16,
}

impl Default for InterruptController {
    fn default() -> Self {
        Self {
            control: 0,
            pit_enabled: true,
            period: 0,
            countdown: 0,
            period_latch: 0,
        }
    }
}

impl InterruptController {
    pub fn read(&self, address: u32) -> Option<u8> {
        match address {
            INTERRUPT_ENABLE => Some(self.control & 1),
            INTERRUPT_PENDING => Some(self.control & 0x1a),
            INTERRUPT_ACK => Some(0),
            INTERRUPT_PIT_ENABLE => Some(u8::from(self.pit_enabled)),
            address if (INTERRUPT_PIT_PERIOD..INTERRUPT_PIT_PERIOD + 2).contains(&address) => {
                Some(self.period.to_be_bytes()[(address - INTERRUPT_PIT_PERIOD) as usize])
            }
            address
                if (INTERRUPT_PIT_COUNTDOWN..INTERRUPT_PIT_COUNTDOWN + 2).contains(&address) =>
            {
                Some(self.countdown.to_be_bytes()[(address - INTERRUPT_PIT_COUNTDOWN) as usize])
            }
            _ => None,
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        match address {
            INTERRUPT_ENABLE => self.control = (self.control & !1) | (value & 1),
            INTERRUPT_ACK => self.control &= !(value & 0x1a),
            INTERRUPT_PIT_ENABLE => self.pit_enabled = value & 1 != 0,
            INTERRUPT_PIT_PERIOD => {
                self.period_latch = (u16::from(value) << 8) | (self.period_latch & 0xff);
            }
            address if address == INTERRUPT_PIT_PERIOD + 1 => {
                self.period_latch = (self.period_latch & 0xff00) | u16::from(value);
                self.period = self.period_latch;
                self.countdown = self.period;
            }
            _ => panic!("host write to a read-only or unimplemented interrupt register"),
        }
    }

    pub fn write_legacy_period_byte(&mut self, low: bool, value: u8) {
        self.period = if low {
            (self.period & 0xff00) | u16::from(value)
        } else {
            (self.period & 0xff) | (u16::from(value) << 8)
        };
        self.period_latch = self.period;
        self.countdown = self.period;
    }

    pub fn tick(&mut self) {
        if self.period == 0 {
            self.countdown = 0;
        } else if self.countdown <= 1 {
            self.countdown = self.period;
            self.control |= 0x10;
        } else {
            self.countdown -= 1;
        }
    }
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
        assert!(is_implemented_device_page(INTERRUPT_BASE));
        assert!(is_implemented_device_page(FAULT_BASE));
        assert!(!is_implemented_device_page(INPUT_BASE));
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
            ("INTERRUPT_ENABLE", INTERRUPT_ENABLE),
            ("INTERRUPT_PENDING", INTERRUPT_PENDING),
            ("INTERRUPT_ACK", INTERRUPT_ACK),
            ("INTERRUPT_PIT_ENABLE", INTERRUPT_PIT_ENABLE),
            ("INTERRUPT_PIT_PERIOD", INTERRUPT_PIT_PERIOD),
            ("INTERRUPT_PIT_COUNTDOWN", INTERRUPT_PIT_COUNTDOWN),
            ("FAULT_ADDRESS", FAULT_ADDRESS),
            ("FAULT_INFO", FAULT_INFO),
            ("FAULT_VALID", FAULT_VALID),
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
