//! Implements the guest terminal console independently of host rendering.
//! Manages bounded input/output, screen cells, scrollback, cursor state, and escape sequences.

use std::collections::{VecDeque, vec_deque};

pub const RX_CAPACITY: usize = 4096;
pub const OUTPUT_CAPACITY: usize = 65_536;
pub const COLUMNS: usize = 80;
pub const ROWS: usize = 25;
pub const SCROLLBACK_ROWS: usize = 1000;

#[derive(Clone, Debug)]
pub struct ConsoleDevice {
    receive_buffer: VecDeque<u8>,
    output: Vec<u8>,
    lines: VecDeque<Vec<u8>>,
    scrollback: VecDeque<Vec<u8>>,
    column: usize,
    row: usize,
    overrun: bool,
    control: u8,
    escape_state: u8,
    escape_private: bool,
    escape_parameters: Vec<u16>,
    saved_cursor: (usize, usize),
    cursor_visible: bool,
}

impl Default for ConsoleDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleDevice {
    pub fn new() -> Self {
        Self {
            receive_buffer: VecDeque::new(),
            output: Vec::new(),
            lines: (0..ROWS).map(|_| vec![b' '; COLUMNS]).collect(),
            scrollback: VecDeque::with_capacity(SCROLLBACK_ROWS),
            column: 0,
            row: 0,
            overrun: false,
            control: 0x03,
            escape_state: 0,
            escape_private: false,
            escape_parameters: Vec::new(),
            saved_cursor: (0, 0),
            cursor_visible: true,
        }
    }

    pub fn status(&self) -> u8 {
        u8::from(!self.receive_buffer.is_empty()) | 0x02 | (u8::from(self.overrun) << 2)
    }

    pub fn control(&self) -> u8 {
        self.control
    }

    pub fn set_control(&mut self, control: u8) {
        self.control = control & 0x03;
    }

    pub fn receive(&mut self, data: &[u8]) {
        if self.control & 0x02 == 0 {
            return;
        }
        for value in data {
            if self.receive_buffer.len() == RX_CAPACITY {
                self.overrun = true;
                break;
            }
            self.receive_buffer.push_back(*value);
        }
    }

    pub fn clear_input(&mut self) {
        self.receive_buffer.clear();
        self.overrun = false;
    }

    pub fn read_data(&mut self) -> u8 {
        let value = self.receive_buffer.pop_front().unwrap_or(0);
        if self.control & 0x01 != 0 && value != 0 {
            self.write_data(value);
        }
        value
    }

    pub fn write_data(&mut self, value: u8) {
        self.output.push(value);
        if self.output.len() > OUTPUT_CAPACITY {
            self.output.drain(..OUTPUT_CAPACITY / 4);
        }
        if self.process_escape(value) {
            return;
        }
        match value {
            0x1b => self.escape_state = 1,
            b'\n' => self.advance_line(),
            b'\r' => self.column = 0,
            8 => {
                self.column = self.column.saturating_sub(1);
                self.lines[self.row][self.column] = b' ';
            }
            b'\t' => self.column = ((self.column / 8 + 1) * 8).min(COLUMNS),
            value if value >= 32 => {
                if self.column >= COLUMNS {
                    self.advance_line();
                }
                self.lines[self.row][self.column] = value;
                self.column += 1;
            }
            _ => {}
        }
    }

    pub fn clear_overrun(&mut self) {
        self.overrun = false;
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn receive_buffer(&self) -> &VecDeque<u8> {
        &self.receive_buffer
    }

    pub fn lines(&self) -> vec_deque::Iter<'_, Vec<u8>> {
        self.lines.iter()
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.column, self.row)
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn text_snapshot(&self) -> Vec<String> {
        self.lines.iter().map(|line| decode_line(line)).collect()
    }

    pub fn scrollback_snapshot(&self) -> Vec<String> {
        self.scrollback
            .iter()
            .chain(&self.lines)
            .map(|line| decode_line(line))
            .collect()
    }

    pub fn write_text(&mut self, text: &str) {
        for value in text
            .chars()
            .map(|character| {
                if character.is_ascii() {
                    character as u8
                } else {
                    b'?'
                }
            })
            .chain(std::iter::once(b'\n'))
        {
            self.write_data(value);
        }
    }

    pub fn log_debug(&mut self, source: &str, message: &str) {
        self.write_log("DEBUG", source, message);
    }

    pub fn log_info(&mut self, source: &str, message: &str) {
        self.write_log("INFO", source, message);
    }

    pub fn log_warning(&mut self, source: &str, message: &str) {
        self.write_log("WARN", source, message);
    }

    pub fn log_error(&mut self, source: &str, message: &str) {
        self.write_log("ERROR", source, message);
    }

    fn write_log(&mut self, level: &str, source: &str, message: &str) {
        self.write_text(&format!("[{level}] {source}: {message}"));
    }

    fn process_escape(&mut self, value: u8) -> bool {
        match self.escape_state {
            0 => false,
            1 => {
                if value == b'[' {
                    self.escape_state = 2;
                    self.escape_private = false;
                    self.escape_parameters.clear();
                    self.escape_parameters.push(0);
                } else {
                    match value {
                        b'7' => self.saved_cursor = (self.column, self.row),
                        b'8' => (self.column, self.row) = self.saved_cursor,
                        _ => {}
                    }
                    self.escape_state = 0;
                }
                true
            }
            2 if value == b'?' && self.escape_parameters == [0] => {
                self.escape_private = true;
                true
            }
            2 if value.is_ascii_digit() => {
                if let Some(parameter) = self.escape_parameters.last_mut() {
                    *parameter = parameter
                        .saturating_mul(10)
                        .saturating_add(u16::from(value - b'0'));
                }
                true
            }
            2 if value == b';' => {
                if self.escape_parameters.len() < 16 {
                    self.escape_parameters.push(0);
                } else {
                    self.escape_state = 0;
                    self.escape_parameters.clear();
                }
                true
            }
            2 if (0x40..=0x7e).contains(&value) => {
                self.execute_escape(value);
                self.escape_state = 0;
                self.escape_parameters.clear();
                true
            }
            2 => true,
            _ => {
                self.escape_state = 0;
                self.escape_parameters.clear();
                true
            }
        }
    }

    fn execute_escape(&mut self, command: u8) {
        let parameter = |index: usize, default: usize| {
            self.escape_parameters
                .get(index)
                .copied()
                .filter(|value| *value != 0)
                .map(usize::from)
                .unwrap_or(default)
        };
        if self.escape_private {
            if command == b'h' || command == b'l' {
                for parameter in &self.escape_parameters {
                    if *parameter == 25 {
                        self.cursor_visible = command == b'h';
                    }
                }
            }
            return;
        }
        match command {
            b'A' => self.row = self.row.saturating_sub(parameter(0, 1)),
            b'B' => self.row = self.row.saturating_add(parameter(0, 1)).min(ROWS - 1),
            b'C' => self.column = self.column.saturating_add(parameter(0, 1)).min(COLUMNS - 1),
            b'D' => self.column = self.column.saturating_sub(parameter(0, 1)),
            b'E' => {
                self.row = self.row.saturating_add(parameter(0, 1)).min(ROWS - 1);
                self.column = 0;
            }
            b'F' => {
                self.row = self.row.saturating_sub(parameter(0, 1));
                self.column = 0;
            }
            b'G' => self.column = parameter(0, 1).saturating_sub(1).min(COLUMNS - 1),
            b'H' | b'f' => {
                self.row = parameter(0, 1).saturating_sub(1).min(ROWS - 1);
                self.column = parameter(1, 1).saturating_sub(1).min(COLUMNS - 1);
            }
            b'd' => self.row = parameter(0, 1).saturating_sub(1).min(ROWS - 1),
            b'J' => self.erase_display(parameter(0, 0)),
            b'K' => self.erase_line(parameter(0, 0)),
            b'X' => {
                let end = self.column.saturating_add(parameter(0, 1)).min(COLUMNS);
                self.lines[self.row][self.column.min(COLUMNS - 1)..end].fill(b' ');
            }
            b's' => self.saved_cursor = (self.column, self.row),
            b'u' => self.restore_cursor(),
            _ => {}
        }
    }

    fn restore_cursor(&mut self) {
        self.column = self.saved_cursor.0.min(COLUMNS - 1);
        self.row = self.saved_cursor.1.min(ROWS - 1);
    }

    fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.lines[self.row][self.column..].fill(b' ');
                for line in self.lines.iter_mut().skip(self.row + 1) {
                    line.fill(b' ');
                }
            }
            1 => {
                for line in self.lines.iter_mut().take(self.row) {
                    line.fill(b' ');
                }
                self.lines[self.row][..=self.column.min(COLUMNS - 1)].fill(b' ');
            }
            2 => {
                for line in &mut self.lines {
                    line.fill(b' ');
                }
            }
            3 => self.scrollback.clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: usize) {
        match mode {
            0 => self.lines[self.row][self.column..].fill(b' '),
            1 => self.lines[self.row][..=self.column.min(COLUMNS - 1)].fill(b' '),
            2 => self.lines[self.row].fill(b' '),
            _ => {}
        }
    }

    fn advance_line(&mut self) {
        self.column = 0;
        self.row += 1;
        if self.row >= ROWS {
            if self.scrollback.len() == SCROLLBACK_ROWS {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(self.lines.pop_front().unwrap());
            self.lines.push_back(vec![b' '; COLUMNS]);
            self.row = ROWS - 1;
        }
    }
}

fn decode_line(line: &[u8]) -> String {
    line.iter()
        .map(|value| {
            if value.is_ascii() {
                char::from(*value)
            } else {
                '\u{fffd}'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receives_fifo_input_and_respects_control_bits() {
        let mut console = ConsoleDevice::new();
        console.receive(b"AB");
        assert_eq!(console.status(), 3);
        assert_eq!(console.read_data(), b'A');
        assert_eq!(console.read_data(), b'B');
        assert_eq!(console.status(), 2);
        assert_eq!(console.output(), b"AB");

        console.set_control(0x02);
        console.receive(b"C");
        assert_eq!(console.read_data(), b'C');
        assert_eq!(console.output(), b"AB");

        console.set_control(0);
        console.receive(b"D");
        assert_eq!(console.status(), 2);
    }

    #[test]
    fn bounds_input_and_output_and_reports_input_overrun() {
        let mut console = ConsoleDevice::new();
        console.receive(&vec![b'x'; RX_CAPACITY + 1]);
        assert_eq!(console.receive_buffer().len(), RX_CAPACITY);
        assert_eq!(console.status(), 7);
        console.clear_overrun();
        assert_eq!(console.status(), 3);

        for _ in 0..OUTPUT_CAPACITY + 1 {
            console.write_data(b'x');
        }
        assert_eq!(
            console.output().len(),
            OUTPUT_CAPACITY - OUTPUT_CAPACITY / 4 + 1
        );
    }

    #[test]
    fn maintains_fixed_text_rows_and_scrollback_limit() {
        let mut console = ConsoleDevice::new();
        console.write_text("SC-16");
        let rows = console.text_snapshot();
        assert_eq!(rows.len(), ROWS);
        assert!(rows.iter().all(|row| row.len() == COLUMNS));
        assert!(rows[0].starts_with("SC-16"));

        for index in 0..SCROLLBACK_ROWS + 100 {
            console.write_text(&format!("line-{index}"));
        }
        let history = console.scrollback_snapshot();
        assert_eq!(console.scrollback_len(), SCROLLBACK_ROWS);
        assert_eq!(history.len(), SCROLLBACK_ROWS + ROWS);
        assert!(!history.iter().any(|line| line.starts_with("line-0 ")));
        assert!(history.iter().any(|line| line.starts_with("line-1099")));
    }

    #[test]
    fn handles_carriage_return_backspace_tabs_and_full_width_wrap() {
        let mut console = ConsoleDevice::new();
        console.write_text("old");
        console.write_data(b'\r');
        console.write_data(b'n');
        console.write_data(8);
        assert!(console.text_snapshot()[1].starts_with(" "));
        console.write_data(b'\t');
        console.write_data(b'x');
        assert_eq!(console.text_snapshot()[1].as_bytes()[8], b'x');

        let mut console = ConsoleDevice::new();
        for _ in 0..ROWS - 1 {
            console.write_text("");
        }
        for _ in 0..COLUMNS {
            console.write_data(b'x');
        }
        console.write_data(b'y');
        assert_eq!(console.text_snapshot().last().unwrap().as_bytes()[0], b'y');
        assert_eq!(console.scrollback_len(), 1);
    }

    #[test]
    fn handles_terminal_cursor_and_erase_sequences_without_printing_control_bytes() {
        let mut console = ConsoleDevice::new();
        console.write_data(b'a');
        console.write_data(b'b');
        console.write_data(0x1b);
        console.write_data(b'[');
        console.write_data(b'1');
        console.write_data(b'D');
        console.write_data(b'X');
        assert!(console.text_snapshot()[0].starts_with("aX"));
        assert_eq!(console.cursor_position(), (2, 0));

        for byte in b"\x1b[3;4Hxy\x1b[1K" {
            console.write_data(*byte);
        }
        assert!(console.text_snapshot()[2].starts_with("    "));
        assert_eq!(console.cursor_position(), (5, 2));
        for byte in b"\x1b[2J" {
            console.write_data(*byte);
        }
        assert!(
            console
                .text_snapshot()
                .iter()
                .all(|line| line.bytes().all(|byte| byte == b' '))
        );
        assert_eq!(console.cursor_position(), (5, 2));
        assert!(
            console
                .output()
                .windows(b"\x1b[1D".len())
                .any(|sequence| sequence == b"\x1b[1D")
        );

        for byte in b"\x1b[?25l\x1b[4;6H\x1b[s\x1b[1;1H\x1b[u\x1b[?25h" {
            console.write_data(*byte);
        }
        assert_eq!(console.cursor_position(), (5, 3));
        assert!(console.cursor_visible());
        for byte in b"\x1b[?25l" {
            console.write_data(*byte);
        }
        assert!(!console.cursor_visible());
    }

    #[test]
    fn writes_tagged_log_messages_to_the_serial_output_and_monitor() {
        let mut console = ConsoleDevice::new();
        console.log_debug("cpu", "instruction trace");
        console.log_info("emulator", "program started");
        console.log_warning("disk", "capacity nearly full");
        console.log_error("gpu", "invalid mode");

        assert_eq!(
            console.output(),
            b"[DEBUG] cpu: instruction trace\n[INFO] emulator: program started\n[WARN] disk: capacity nearly full\n[ERROR] gpu: invalid mode\n"
        );
        let lines = console.text_snapshot();
        assert!(lines[0].starts_with("[DEBUG] cpu: instruction trace"));
        assert!(lines[3].starts_with("[ERROR] gpu: invalid mode"));
    }
}
