// No imports needed for this current implementation

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
}

impl GuestTextDisplay {
    pub fn new() -> Self {
        Self {
            cells: vec![GuestTextCell {
                character: b' ',
                foreground: 0,
                background: 0,
            }; GUEST_TEXT_COLUMNS * GUEST_TEXT_ROWS],
            cursor_column: 0,
            cursor_row: 0,
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
            let char_byte = if (32..=126).contains(&byte) { byte } else { b'?' };
            self.write_char(char_byte, foreground, background);
        }
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(GUEST_TEXT_ROWS - 1);
        self.cursor_column = col.min(GUEST_TEXT_COLUMNS - 1);
    }

    pub fn clear(&mut self, background: u8) {
        for cell in &mut self.cells {
            cell.character = b' ';
            cell.background = background;
        }
        self.cursor_column = 0;
        self.cursor_row = 0;
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
        for row in 0..GUEST_TEXT_ROWS - 1 {
            let start = row * row_size;
            let end = start + row_size;
            let next_start = end;
            let next_end = next_start + row_size;
            
            let mut source = Vec::with_capacity(row_size);
            source.extend_from_slice(&self.cells[next_start..next_end]);
            self.cells[start..end].copy_from_slice(&source);
        }
        let last_row_start = (GUEST_TEXT_ROWS - 1) * row_size;
        for cell in &mut self.cells[last_row_start..] {
            cell.character = b' ';
        }
    }
}
