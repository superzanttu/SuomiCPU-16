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
        }
    }

    pub fn reset(&mut self, preserve_history: bool) {
        self.receive_buffer.clear();
        self.overrun = false;
        self.control = 0x03;
        if !preserve_history {
            self.output.clear();
            self.lines = (0..ROWS).map(|_| vec![b' '; COLUMNS]).collect();
            self.scrollback.clear();
            self.column = 0;
            self.row = 0;
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
        match value {
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
    fn reset_can_preserve_or_clear_history_and_output() {
        let mut console = ConsoleDevice::new();
        console.write_text("previous");
        console.receive(b"pending");
        console.reset(true);
        assert!(console.receive_buffer().is_empty());
        assert_eq!(console.control(), 3);
        assert!(
            console
                .scrollback_snapshot()
                .iter()
                .any(|line| line.contains("previous"))
        );
        assert!(!console.output().is_empty());

        console.reset(false);
        assert!(console.output().is_empty());
        assert!(
            !console
                .scrollback_snapshot()
                .iter()
                .any(|line| line.contains("previous"))
        );
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
