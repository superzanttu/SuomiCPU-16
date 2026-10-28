//! Maintains the firmware's guest character-cell display.
//! Defines colored text cells, cursor movement, screen clearing, and text output.

pub const GUEST_TEXT_COLUMNS: usize = 53;
pub const GUEST_TEXT_ROWS: usize = 25;

#[derive(Clone, Copy, Debug, Default)]
pub struct GuestTextCell {
    pub character: u8,
    pub foreground: u8,
    pub background: u8,
}

pub struct GuestTextDisplay {
    pub cells: Vec<GuestTextCell>,
    pub cursor_column: usize,
    pub cursor_row: usize,
    pub foreground: u8,
    pub background: u8,
    pub enabled: bool,
}

impl GuestTextDisplay {
    pub fn new() -> Self {
        Self {
            cells: vec![
                GuestTextCell {
                    character: b' ',
                    foreground: 0,
                    background: 0,
                };
                GUEST_TEXT_COLUMNS * GUEST_TEXT_ROWS
            ],
            cursor_column: 0,
            cursor_row: 0,
            foreground: 1,
            background: 0,
            enabled: false,
        }
    }

    pub fn write_char(&mut self, character: u8, foreground: u8, background: u8) {
        let index = self.cursor_row * GUEST_TEXT_COLUMNS + self.cursor_column;
        if index < self.cells.len() {
            self.cells[index] = GuestTextCell {
                character,
                foreground,
                background,
            };
        }

        self.cursor_column += 1;
        if self.cursor_column >= GUEST_TEXT_COLUMNS {
            self.cursor_column = 0;
            self.advance_row();
        }
    }

    pub fn write_text(&mut self, text: &str, foreground: u8, background: u8) {
        for byte in text.bytes() {
            self.write_byte(byte, foreground, background);
        }
    }

    pub fn write_byte(&mut self, byte: u8, foreground: u8, background: u8) {
        self.foreground = foreground;
        self.background = background;
        match byte {
            b'\n' => self.advance_row(),
            b'\r' => self.cursor_column = 0,
            b'\x08' => {
                if self.cursor_column > 0 {
                    self.cursor_column -= 1;
                    self.write_cell(b' ');
                }
            }
            b'\t' => {
                let next_tab = (self.cursor_column / 8 + 1) * 8;
                while self.cursor_column < next_tab {
                    self.write_char(b' ', foreground, background);
                }
            }
            32..=126 => self.write_char(byte, foreground, background),
            _ => self.write_char(b'?', foreground, background),
        }
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(GUEST_TEXT_ROWS - 1);
        self.cursor_column = col.min(GUEST_TEXT_COLUMNS - 1);
    }

    pub fn clear(&mut self, background: u8) {
        for cell in &mut self.cells {
            cell.character = b' ';
            cell.foreground = self.foreground;
            cell.background = background;
        }
        self.background = background;
        self.cursor_column = 0;
        self.cursor_row = 0;
    }

    fn write_cell(&mut self, character: u8) {
        let index = self.cursor_row * GUEST_TEXT_COLUMNS + self.cursor_column;
        self.cells[index] = GuestTextCell {
            character,
            foreground: self.foreground,
            background: self.background,
        };
    }

    fn advance_row(&mut self) {
        self.cursor_row += 1;
        if self.cursor_row >= GUEST_TEXT_ROWS {
            self.scroll_up();
            self.cursor_row = GUEST_TEXT_ROWS - 1;
        }
    }

    fn scroll_up(&mut self) {
        let row_size = GUEST_TEXT_COLUMNS;
        self.cells.copy_within(row_size.., 0);
        let last_row_start = (GUEST_TEXT_ROWS - 1) * row_size;
        for cell in &mut self.cells[last_row_start..] {
            cell.character = b' ';
            cell.foreground = self.foreground;
            cell.background = self.background;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_bytes_move_cursor_and_tab_writes_spaces() {
        let mut display = GuestTextDisplay::new();
        display.write_text("ab\tc\rd", 3, 4);
        assert_eq!(display.cells[0].character, b'd');
        assert_eq!(display.cells[0].foreground, 3);
        assert_eq!(display.cells[0].background, 4);
        assert_eq!(display.cells[1].character, b'b');
        assert_eq!(display.cells[2].character, b' ');
        assert_eq!(display.cells[7].character, b' ');
        assert_eq!(display.cells[8].character, b'c');
        assert_eq!(display.cursor_column, 1);
        assert_eq!(display.cursor_row, 0);
    }

    #[test]
    fn line_feed_wrap_and_scroll_preserve_the_fixed_surface() {
        let mut display = GuestTextDisplay::new();
        display.set_cursor(GUEST_TEXT_ROWS - 1, 0);
        display.write_byte(b'x', 2, 5);
        display.write_byte(b'\n', 2, 5);
        assert_eq!(display.cursor_row, GUEST_TEXT_ROWS - 1);
        assert_eq!(display.cells[0].character, b' ');
        assert_eq!(
            display.cells[(GUEST_TEXT_ROWS - 1) * GUEST_TEXT_COLUMNS].character,
            b' '
        );
        assert_eq!(
            display.cells[(GUEST_TEXT_ROWS - 2) * GUEST_TEXT_COLUMNS].character,
            b'x'
        );
        assert_eq!(
            display.cells[(GUEST_TEXT_ROWS - 1) * GUEST_TEXT_COLUMNS].background,
            5
        );
    }

    #[test]
    fn line_feed_preserves_column_and_non_ascii_bytes_use_replacement_glyph() {
        let mut display = GuestTextDisplay::new();
        display.write_text("a\nb", 1, 0);
        assert_eq!(display.cells[0].character, b'a');
        assert_eq!(display.cells[GUEST_TEXT_COLUMNS + 1].character, b'b');
        assert_eq!(display.cursor_row, 1);
        assert_eq!(display.cursor_column, 2);
        display.write_byte(0xff, 1, 0);
        assert_eq!(display.cells[GUEST_TEXT_COLUMNS + 2].character, b'?');
    }

    #[test]
    fn backspace_blanks_the_previous_character_and_clear_resets_cursor() {
        let mut display = GuestTextDisplay::new();
        display.write_text("xy\x08", 1, 2);
        assert_eq!(display.cells[0].character, b'x');
        assert_eq!(display.cells[1].character, b' ');
        assert_eq!(display.cursor_column, 1);
        display.clear(7);
        assert_eq!(display.cursor_column, 0);
        assert_eq!(display.cursor_row, 0);
        assert!(display.cells.iter().all(|cell| cell.background == 7));
    }
}
