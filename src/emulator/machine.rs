//! Defines address boundaries, register contracts, and typed device state.
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
pub const INPUT_KEY: u32 = INPUT_BASE;
pub const INPUT_KEYS: u32 = INPUT_BASE + 4;
pub const INPUT_MOUSE_X: u32 = INPUT_BASE + 8;
pub const INPUT_MOUSE_Y: u32 = INPUT_BASE + 10;
pub const INPUT_MOUSE_BUTTONS: u32 = INPUT_BASE + 12;
pub const INPUT_MOUSE_EVENTS: u32 = INPUT_BASE + 13;
pub const INPUT_MOUSE_WHEEL: u32 = INPUT_BASE + 14;
pub const CLOCK_CAPTURE: u32 = CLOCK_BASE;
pub const CLOCK_MONOTONIC_MILLIS: u32 = CLOCK_BASE + 4;
pub const CLOCK_FRAMES: u32 = CLOCK_BASE + 8;
pub const CLOCK_RTC: u32 = CLOCK_BASE + 12;
pub const CLOCK_RANDOM: u32 = CLOCK_BASE + 16;
pub const AUDIO_COMMAND: u32 = AUDIO_BASE;
pub const AUDIO_STATUS: u32 = AUDIO_BASE + 1;
pub const AUDIO_CHANNEL: u32 = AUDIO_BASE + 2;
pub const AUDIO_FREQUENCY: u32 = AUDIO_BASE + 4;
pub const AUDIO_FRAMES: u32 = AUDIO_BASE + 6;
pub const AUDIO_WAVEFORM: u32 = AUDIO_BASE + 8;
pub const AUDIO_VOLUME: u32 = AUDIO_BASE + 10;
pub const AUDIO_PENDING: u32 = AUDIO_BASE + 12;
pub const NETWORK_COMMAND: u32 = NETWORK_BASE;
pub const NETWORK_STATUS: u32 = NETWORK_BASE + 1;
pub const NETWORK_LENGTH: u32 = NETWORK_BASE + 2;
pub const NETWORK_QUERY: u32 = NETWORK_BASE + 3;
pub const NETWORK_ARGUMENT: u32 = NETWORK_BASE + 4;
pub const NETWORK_RESULT: u32 = NETWORK_BASE + 6;
pub const NETWORK_TITLE: u32 = NETWORK_BASE + 0x10;
pub const NETWORK_TRANSMIT: u32 = NETWORK_BASE + 0x20;
pub const NETWORK_RECEIVE: u32 = NETWORK_BASE + 0x50;
pub const CONSOLE_DATA: u32 = CONSOLE_BASE;
pub const CONSOLE_STATUS: u32 = CONSOLE_BASE + 1;
pub const CONSOLE_CONTROL: u32 = CONSOLE_BASE + 2;
pub const FIRMWARE_TEXT_DATA: u32 = FIRMWARE_TEXT_BASE;
pub const FIRMWARE_TEXT_FOREGROUND: u32 = FIRMWARE_TEXT_BASE + 1;
pub const FIRMWARE_TEXT_BACKGROUND: u32 = FIRMWARE_TEXT_BASE + 2;
pub const FIRMWARE_TEXT_CONTROL: u32 = FIRMWARE_TEXT_BASE + 3;
pub const FIRMWARE_TEXT_ROW: u32 = FIRMWARE_TEXT_BASE + 4;
pub const FIRMWARE_TEXT_COLUMN: u32 = FIRMWARE_TEXT_BASE + 5;
pub const FIRMWARE_TEXT_ENABLED: u32 = FIRMWARE_TEXT_BASE + 6;
pub const FIRMWARE_COMMAND: u32 = FIRMWARE_GATEWAY_BASE;
pub const FIRMWARE_STATUS: u32 = FIRMWARE_GATEWAY_BASE + 1;
pub const FIRMWARE_INDEX: u32 = FIRMWARE_GATEWAY_BASE + 2;
pub const FIRMWARE_DISK_COUNT: u32 = FIRMWARE_GATEWAY_BASE + 4;
pub const FIRMWARE_STARTUP: u32 = FIRMWARE_GATEWAY_BASE + 6;
pub const FIRMWARE_OUTPUT_LENGTH: u32 = FIRMWARE_GATEWAY_BASE + 8;
pub const FIRMWARE_SETTING_COUNT: u32 = FIRMWARE_GATEWAY_BASE + 14;
pub const FIRMWARE_NETWORK_NAME: u32 = FIRMWARE_GATEWAY_BASE + 16;
pub const FIRMWARE_SETTING: u32 = FIRMWARE_GATEWAY_BASE + 48;
pub const FIRMWARE_OUTPUT: u32 = FIRMWARE_GATEWAY_BASE + 0x100;
pub const FIRMWARE_REQUEST_BYTES: usize = 90;
pub const MACHINE_REVISION: u32 = 1;
pub const DEVICE_ABI: u32 = 1;
pub const IMPLEMENTED_BANKS: u32 = 0xedf;

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
    (address >= SYSTEM_BASE && address < CLOCK_BASE + DEVICE_BANK_SIZE)
        || (address >= AUDIO_BASE && address < NETWORK_BASE + DEVICE_BANK_SIZE)
        || (address >= CONSOLE_BASE && address < FIRMWARE_GATEWAY_BASE + DEVICE_BANK_SIZE)
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
        INPUT_KEY => Some(false),
        address if (INPUT_KEYS..INPUT_MOUSE_WHEEL + 2).contains(&address) => Some(false),
        CLOCK_CAPTURE => Some(true),
        AUDIO_COMMAND | AUDIO_CHANNEL => Some(true),
        AUDIO_STATUS | AUDIO_PENDING => Some(false),
        address if (AUDIO_FREQUENCY..AUDIO_PENDING).contains(&address) => Some(true),
        NETWORK_COMMAND | NETWORK_LENGTH | NETWORK_QUERY => Some(true),
        NETWORK_STATUS => Some(false),
        address if (NETWORK_ARGUMENT..NETWORK_RESULT).contains(&address) => Some(true),
        address
            if (NETWORK_RESULT..NETWORK_RESULT + 2).contains(&address)
                || (NETWORK_RECEIVE..NETWORK_RECEIVE + 48).contains(&address) =>
        {
            Some(false)
        }
        address if (NETWORK_TITLE..NETWORK_TRANSMIT + 48).contains(&address) => Some(true),
        CONSOLE_DATA..=CONSOLE_CONTROL => Some(true),
        FIRMWARE_TEXT_DATA..=FIRMWARE_TEXT_COLUMN => Some(true),
        FIRMWARE_TEXT_ENABLED => Some(false),
        FIRMWARE_COMMAND | FIRMWARE_STARTUP => Some(true),
        FIRMWARE_STATUS | FIRMWARE_SETTING_COUNT => Some(false),
        address
            if (FIRMWARE_INDEX..FIRMWARE_INDEX + 2).contains(&address)
                || (FIRMWARE_NETWORK_NAME..FIRMWARE_NETWORK_NAME + 17).contains(&address)
                || (FIRMWARE_SETTING..FIRMWARE_SETTING + 42).contains(&address) =>
        {
            Some(true)
        }
        address
            if (FIRMWARE_DISK_COUNT..FIRMWARE_DISK_COUNT + 2).contains(&address)
                || (FIRMWARE_OUTPUT_LENGTH..FIRMWARE_OUTPUT_LENGTH + 2).contains(&address)
                || (FIRMWARE_OUTPUT..FIRMWARE_OUTPUT + 512).contains(&address) =>
        {
            Some(false)
        }
        address
            if (CLOCK_MONOTONIC_MILLIS..CLOCK_MONOTONIC_MILLIS + 4).contains(&address)
                || (CLOCK_FRAMES..CLOCK_FRAMES + 2).contains(&address)
                || (CLOCK_RTC..CLOCK_RTC + 3).contains(&address)
                || (CLOCK_RANDOM..CLOCK_RANDOM + 4).contains(&address) =>
        {
            Some(false)
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct FirmwareGateway {
    registers: [u8; FIRMWARE_REQUEST_BYTES],
    output: [u8; 512],
}

impl Default for FirmwareGateway {
    fn default() -> Self {
        Self {
            registers: [0; FIRMWARE_REQUEST_BYTES],
            output: [0; 512],
        }
    }
}

impl FirmwareGateway {
    pub fn read(&self, address: u32) -> Option<u8> {
        device_register_writable(address)?;
        if (FIRMWARE_GATEWAY_BASE..FIRMWARE_GATEWAY_BASE + FIRMWARE_REQUEST_BYTES as u32)
            .contains(&address)
        {
            if address == FIRMWARE_COMMAND {
                Some(0)
            } else {
                Some(self.registers[(address - FIRMWARE_GATEWAY_BASE) as usize])
            }
        } else if (FIRMWARE_OUTPUT..FIRMWARE_OUTPUT + 512).contains(&address) {
            Some(self.output[(address - FIRMWARE_OUTPUT) as usize])
        } else {
            None
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        assert!(
            address != FIRMWARE_COMMAND
                && (FIRMWARE_GATEWAY_BASE..FIRMWARE_GATEWAY_BASE + FIRMWARE_REQUEST_BYTES as u32)
                    .contains(&address)
                && device_register_writable(address) == Some(true),
            "host write to an invalid firmware staging register"
        );
        self.registers[(address - FIRMWARE_GATEWAY_BASE) as usize] = value;
    }

    pub fn request(&self) -> [u8; FIRMWARE_REQUEST_BYTES] {
        self.registers
    }

    pub fn complete(
        &mut self,
        mut request: [u8; FIRMWARE_REQUEST_BYTES],
        output: Option<&str>,
    ) -> Result<(), String> {
        let length = output.map_or(0, |text| text.len());
        if length >= self.output.len() {
            self.fail();
            return Err("firmware response exceeds 511 bytes".to_owned());
        }
        self.output.fill(0);
        if let Some(output) = output {
            self.output[..length].copy_from_slice(output.as_bytes());
        }
        request[1] = 0;
        request[8..10].copy_from_slice(&(length as u16).to_be_bytes());
        self.registers = request;
        Ok(())
    }

    pub fn fail(&mut self) {
        self.registers[1] = 1;
        self.registers[8..10].fill(0);
    }
}

#[derive(Clone, Debug)]
pub struct NetworkDevice {
    length: u8,
    query: u8,
    argument: [u8; 2],
    title: [u8; 16],
    transmit: [u8; 48],
    receive: [u8; 48],
    result: u16,
    status: u8,
}

impl Default for NetworkDevice {
    fn default() -> Self {
        Self {
            length: 0,
            query: 0,
            argument: [0; 2],
            title: [0; 16],
            transmit: [0; 48],
            receive: [0; 48],
            result: 0,
            status: 0,
        }
    }
}

impl NetworkDevice {
    pub fn read(&self, address: u32) -> Option<u8> {
        match address {
            NETWORK_COMMAND => Some(0),
            NETWORK_STATUS => Some(self.status),
            NETWORK_LENGTH => Some(self.length),
            NETWORK_QUERY => Some(self.query),
            address if (NETWORK_ARGUMENT..NETWORK_RESULT).contains(&address) => {
                Some(self.argument[(address - NETWORK_ARGUMENT) as usize])
            }
            address if (NETWORK_RESULT..NETWORK_RESULT + 2).contains(&address) => {
                Some(self.result.to_be_bytes()[(address - NETWORK_RESULT) as usize])
            }
            address if (NETWORK_TITLE..NETWORK_TRANSMIT).contains(&address) => {
                Some(self.title[(address - NETWORK_TITLE) as usize])
            }
            address if (NETWORK_TRANSMIT..NETWORK_RECEIVE).contains(&address) => {
                Some(self.transmit[(address - NETWORK_TRANSMIT) as usize])
            }
            address if (NETWORK_RECEIVE..NETWORK_RECEIVE + 48).contains(&address) => {
                Some(self.receive[(address - NETWORK_RECEIVE) as usize])
            }
            _ => None,
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        match address {
            NETWORK_LENGTH => self.length = value,
            NETWORK_QUERY => self.query = value,
            address if (NETWORK_ARGUMENT..NETWORK_RESULT).contains(&address) => {
                self.argument[(address - NETWORK_ARGUMENT) as usize] = value;
            }
            address if (NETWORK_TITLE..NETWORK_TRANSMIT).contains(&address) => {
                self.title[(address - NETWORK_TITLE) as usize] = value;
            }
            address if (NETWORK_TRANSMIT..NETWORK_RECEIVE).contains(&address) => {
                self.transmit[(address - NETWORK_TRANSMIT) as usize] = value;
            }
            _ => panic!("host write to a read-only or unimplemented network staging register"),
        }
    }

    pub fn command(&mut self, operation: u8, node: &mut crate::net::NetNode) -> Result<(), String> {
        self.result = 0;
        self.status = match operation {
            1 if self.title[0] == 0 => 3,
            3 if usize::from(self.length) > self.transmit.len() => 2,
            5 if self.query > 6 => 7,
            1..=5 => 0,
            _ => 1,
        };
        if self.status != 0 {
            return Ok(());
        }
        node.poll();
        match operation {
            1 => {
                let length = self.title.iter().position(|byte| *byte == 0).unwrap_or(16);
                match node.open(&self.title[..length]) {
                    Ok(opened) => self.result = u16::from(opened),
                    Err(error) => {
                        self.status = 5;
                        return Err(error);
                    }
                }
            }
            2 => node.close(),
            3 => match node.try_send(&self.transmit[..usize::from(self.length)]) {
                Ok(true) => self.result = 1,
                Ok(false) => self.status = 4,
                Err(error) => {
                    self.status = 5;
                    return Err(error);
                }
            },
            4 => {
                if !node.is_open() {
                    self.status = 4;
                } else if let Some((_, data)) = node.receive() {
                    self.receive.fill(0);
                    self.receive[..data.len()].copy_from_slice(&data);
                    self.result = data.len() as u16;
                } else {
                    self.status = 6;
                }
            }
            5 => {
                self.result = node
                    .query(self.query, u16::from_be_bytes(self.argument))
                    .expect("validated network query");
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundCommand {
    pub channel: u8,
    pub frequency: u16,
    pub frames: u16,
    pub waveform: u16,
    pub volume: u16,
}

#[derive(Clone, Debug, Default)]
pub struct AudioDevice {
    channel: u8,
    parameters: [u8; 8],
    status: u8,
    pending: [Option<SoundCommand>; 4],
}

impl AudioDevice {
    pub fn read(&self, address: u32) -> Option<u8> {
        match address {
            AUDIO_COMMAND => Some(0),
            AUDIO_STATUS => Some(self.status),
            AUDIO_CHANNEL => Some(self.channel),
            AUDIO_PENDING => Some(
                self.pending
                    .iter()
                    .enumerate()
                    .fold(0, |mask, (channel, command)| {
                        mask | (u8::from(command.is_some()) << channel)
                    }),
            ),
            address if (AUDIO_FREQUENCY..AUDIO_PENDING).contains(&address) => {
                Some(self.parameters[(address - AUDIO_FREQUENCY) as usize])
            }
            _ => None,
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        match address {
            AUDIO_CHANNEL => self.channel = value,
            address if (AUDIO_FREQUENCY..AUDIO_PENDING).contains(&address) => {
                self.parameters[(address - AUDIO_FREQUENCY) as usize] = value;
            }
            AUDIO_COMMAND => self.commit(value),
            _ => panic!("host write to a read-only or unimplemented audio register"),
        }
    }

    fn commit(&mut self, operation: u8) {
        let word =
            |offset| u16::from_be_bytes([self.parameters[offset], self.parameters[offset + 1]]);
        let command = SoundCommand {
            channel: self.channel,
            frequency: word(0),
            frames: word(2),
            waveform: word(4),
            volume: word(6),
        };
        self.status = if !matches!(operation, 1 | 2) {
            1
        } else if command.channel >= 4 {
            2
        } else if operation == 1 && command.waveform > 2 {
            3
        } else if operation == 1 && command.volume > 100 {
            4
        } else {
            if operation == 2 {
                self.submit(SoundCommand {
                    frequency: 0,
                    frames: 0,
                    waveform: 0,
                    volume: 0,
                    ..command
                });
            } else {
                self.submit(command);
            }
            0
        };
    }

    pub fn submit(&mut self, command: SoundCommand) {
        self.pending[usize::from(command.channel)] = Some(command);
    }

    pub fn take_commands(&mut self) -> Vec<SoundCommand> {
        self.pending.iter_mut().filter_map(Option::take).collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct InputDevice {
    pub key: u8,
    pub keys: u32,
    pub mouse: [u8; 8],
    last_mouse_click: [Option<(u64, u16, u16)>; 2],
}

impl InputDevice {
    pub fn read(&mut self, address: u32) -> Option<u8> {
        match address {
            INPUT_KEY => Some(std::mem::take(&mut self.key)),
            address if (INPUT_KEYS..INPUT_KEYS + 4).contains(&address) => {
                Some((self.keys & 0x00ff_ffff).to_be_bytes()[(address - INPUT_KEYS) as usize])
            }
            INPUT_MOUSE_EVENTS => Some(std::mem::take(&mut self.mouse[5])),
            address if (INPUT_MOUSE_X..INPUT_MOUSE_X + 8).contains(&address) => {
                Some(self.mouse[(address - INPUT_MOUSE_X) as usize])
            }
            _ => None,
        }
    }

    pub fn set_key(&mut self, mask: u32, pressed: bool) {
        let mask = mask & 0x00ff_ffff;
        if pressed {
            self.keys |= mask;
        } else {
            self.keys &= !mask;
        }
    }

    pub fn mouse_button(&mut self, button: usize, down: bool, x: u16, y: u16, now_ms: u64) {
        if button > 1 {
            return;
        }
        let bit = 1 << button;
        if !down {
            self.mouse[4] &= !bit;
            self.mouse[5] |= bit << 4;
            return;
        }
        self.mouse[4] |= bit;
        self.mouse[5] |= bit;
        let double =
            self.last_mouse_click[button].is_some_and(|(previous_ms, previous_x, previous_y)| {
                now_ms.saturating_sub(previous_ms) <= 400
                    && x.abs_diff(previous_x) <= 4
                    && y.abs_diff(previous_y) <= 4
            });
        if double {
            self.mouse[5] |= bit << 2;
            self.last_mouse_click[button] = None;
        } else {
            self.last_mouse_click[button] = Some((now_ms, x, y));
        }
    }

    pub fn add_wheel(&mut self, delta: i16) {
        let wheel = i16::from_be_bytes([self.mouse[6], self.mouse[7]]);
        self.mouse[6..8].copy_from_slice(&wheel.saturating_add(delta).to_be_bytes());
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClockDevice {
    pub rtc: [u8; 3],
    milliseconds: u32,
    frames: u16,
    captured_rtc: [u8; 3],
    random: u32,
}

impl ClockDevice {
    pub fn capture_time(&mut self, milliseconds: u32, frames: u16) {
        self.milliseconds = milliseconds & (ADDRESS_SPACE_SIZE - 1);
        self.frames = frames;
        self.captured_rtc = self.rtc;
    }

    pub fn capture_random(&mut self, random: u32) {
        self.random = random;
    }

    pub fn read(&self, address: u32) -> Option<u8> {
        match address {
            CLOCK_CAPTURE => Some(0),
            address if (CLOCK_MONOTONIC_MILLIS..CLOCK_MONOTONIC_MILLIS + 4).contains(&address) => {
                Some(self.milliseconds.to_be_bytes()[(address - CLOCK_MONOTONIC_MILLIS) as usize])
            }
            address if (CLOCK_FRAMES..CLOCK_FRAMES + 2).contains(&address) => {
                Some(self.frames.to_be_bytes()[(address - CLOCK_FRAMES) as usize])
            }
            address if (CLOCK_RTC..CLOCK_RTC + 3).contains(&address) => {
                Some(self.captured_rtc[(address - CLOCK_RTC) as usize])
            }
            address if (CLOCK_RANDOM..CLOCK_RANDOM + 4).contains(&address) => {
                Some(self.random.to_be_bytes()[(address - CLOCK_RANDOM) as usize])
            }
            _ => None,
        }
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
        assert!(is_implemented_device_page(INPUT_BASE));
        assert!(is_implemented_device_page(CLOCK_BASE));
        assert!(!is_implemented_device_page(GRAPHICS_BASE));
    }

    #[test]
    fn implemented_bank_mask_matches_every_aperture_page_boundary() {
        for page in 0..(ADDRESS_SPACE_SIZE - DEVICE_APERTURE_START) / DEVICE_BANK_SIZE {
            let base = DEVICE_APERTURE_START + page * DEVICE_BANK_SIZE;
            let implemented = page < 32 && IMPLEMENTED_BANKS & (1 << page) != 0;
            assert_eq!(is_implemented_device_page(base), implemented);
            assert_eq!(
                is_implemented_device_page(base + DEVICE_BANK_SIZE - 1),
                implemented
            );
        }
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
            ("INPUT_KEY", INPUT_KEY),
            ("INPUT_KEYS", INPUT_KEYS),
            ("INPUT_MOUSE_X", INPUT_MOUSE_X),
            ("INPUT_MOUSE_Y", INPUT_MOUSE_Y),
            ("INPUT_MOUSE_BUTTONS", INPUT_MOUSE_BUTTONS),
            ("INPUT_MOUSE_EVENTS", INPUT_MOUSE_EVENTS),
            ("INPUT_MOUSE_WHEEL", INPUT_MOUSE_WHEEL),
            ("CLOCK_CAPTURE", CLOCK_CAPTURE),
            ("CLOCK_MONOTONIC_MILLIS", CLOCK_MONOTONIC_MILLIS),
            ("CLOCK_FRAMES", CLOCK_FRAMES),
            ("CLOCK_RTC", CLOCK_RTC),
            ("CLOCK_RANDOM", CLOCK_RANDOM),
            ("AUDIO_COMMAND", AUDIO_COMMAND),
            ("AUDIO_STATUS", AUDIO_STATUS),
            ("AUDIO_CHANNEL", AUDIO_CHANNEL),
            ("AUDIO_FREQUENCY", AUDIO_FREQUENCY),
            ("AUDIO_FRAMES", AUDIO_FRAMES),
            ("AUDIO_WAVEFORM", AUDIO_WAVEFORM),
            ("AUDIO_VOLUME", AUDIO_VOLUME),
            ("AUDIO_PENDING", AUDIO_PENDING),
            ("NETWORK_COMMAND", NETWORK_COMMAND),
            ("NETWORK_STATUS", NETWORK_STATUS),
            ("NETWORK_LENGTH", NETWORK_LENGTH),
            ("NETWORK_QUERY", NETWORK_QUERY),
            ("NETWORK_ARGUMENT", NETWORK_ARGUMENT),
            ("NETWORK_RESULT", NETWORK_RESULT),
            ("NETWORK_TITLE", NETWORK_TITLE),
            ("NETWORK_TRANSMIT", NETWORK_TRANSMIT),
            ("NETWORK_RECEIVE", NETWORK_RECEIVE),
            ("CONSOLE_DATA", CONSOLE_DATA),
            ("CONSOLE_STATUS", CONSOLE_STATUS),
            ("CONSOLE_CONTROL", CONSOLE_CONTROL),
            ("FIRMWARE_TEXT_DATA", FIRMWARE_TEXT_DATA),
            ("FIRMWARE_TEXT_FOREGROUND", FIRMWARE_TEXT_FOREGROUND),
            ("FIRMWARE_TEXT_BACKGROUND", FIRMWARE_TEXT_BACKGROUND),
            ("FIRMWARE_TEXT_CONTROL", FIRMWARE_TEXT_CONTROL),
            ("FIRMWARE_TEXT_ROW", FIRMWARE_TEXT_ROW),
            ("FIRMWARE_TEXT_COLUMN", FIRMWARE_TEXT_COLUMN),
            ("FIRMWARE_TEXT_ENABLED", FIRMWARE_TEXT_ENABLED),
            ("FIRMWARE_COMMAND", FIRMWARE_COMMAND),
            ("FIRMWARE_STATUS", FIRMWARE_STATUS),
            ("FIRMWARE_INDEX", FIRMWARE_INDEX),
            ("FIRMWARE_DISK_COUNT", FIRMWARE_DISK_COUNT),
            ("FIRMWARE_STARTUP", FIRMWARE_STARTUP),
            ("FIRMWARE_OUTPUT_LENGTH", FIRMWARE_OUTPUT_LENGTH),
            ("FIRMWARE_SETTING_COUNT", FIRMWARE_SETTING_COUNT),
            ("FIRMWARE_NETWORK_NAME", FIRMWARE_NETWORK_NAME),
            ("FIRMWARE_SETTING", FIRMWARE_SETTING),
            ("FIRMWARE_OUTPUT", FIRMWARE_OUTPUT),
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
