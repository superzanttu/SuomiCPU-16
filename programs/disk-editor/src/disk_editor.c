#include "suomi_gfx.h"
#include "disk.h"

#define COLS 34
#define TX 104
#define TY 44
#define ANIM 30
#define MAX_FILES 16
#define MAX_NAME 32
#define EDIT_SIZE 256

volatile unsigned char *ram = 0x40000;
char root_path[1] = "";
char files[MAX_FILES][MAX_NAME + 1];
char label[MAX_NAME + 1];
char num[6];
char line[COLS + 1];
int ndirs, top, sel, loaded, file_length, cur, dirty, led, press, anim_t, act;
char *status;

void copy_name(char *destination, const char *source) {
    int i;
    i = 0;
    while (source[i] && i < MAX_NAME) {
        destination[i] = source[i];
        i++;
    }
    destination[i] = 0;
}

int same_name(const char *left, const char *right) {
    int i;
    i = 0;
    while (left[i] && right[i] && left[i] == right[i]) i++;
    return left[i] == right[i];
}

void put_num(int value) {
    int i;
    i = 4;
    num[5] = 0;
    while (i >= 0) {
        num[i] = 48 + value % 10;
        value = value / 10;
        i--;
    }
}

void refresh_dir(void) {
    int i, j, count, length, is_directory;
    volatile char *scratch;
    scratch = (volatile char *)(ram + 0x200);
    ndirs = 0;
    count = folder_fs_list(0, root_path, scratch, 256);
    if (count < 0) {
        status = "FILESYSTEM UNAVAILABLE";
        return;
    }
    for (i = 0; i < count && ndirs < MAX_FILES; i++) {
        if (folder_fs_list(i, root_path, scratch, 256) < 0) {
            status = "FILE LIST FAILED";
            return;
        }
        length = 0;
        while (length < MAX_NAME && scratch[length]) length++;
        is_directory = scratch[length + 1];
        if (length == 0 || length > MAX_NAME || is_directory) continue;
        for (j = 0; j < length; j++) files[ndirs][j] = scratch[j];
        files[ndirs][length] = 0;
        ndirs++;
    }
    if (sel >= ndirs) sel = ndirs - 1;
    if (sel < 0) sel = 0;
}

void do_eject(void) {
    loaded = 0;
    dirty = 0;
    file_length = 0;
    label[0] = 0;
    status = "FILE CLOSED";
}

void load_file(int index) {
    unsigned long size;
    int length;
    if (index < 0 || index >= ndirs) {
        status = "NO FILE SELECTED";
        return;
    }
    copy_name(label, files[index]);
    length = disk_read_file(label, 0, ram, EDIT_SIZE, 0);
    if (length < 0) {
        loaded = 0;
        status = "FILE READ FAILED";
        return;
    }
    size = folder_fs_read32(FOLDER_FS_REGISTERS,
                            FOLDER_FS_FILE_SIZE_OFFSET);
    if (size > EDIT_SIZE) {
        loaded = 0;
        status = "FILE TOO LARGE (256 BYTE LIMIT)";
        return;
    }
    loaded = 1;
    file_length = length;
    cur = 0;
    dirty = 0;
    status = "FILE LOADED";
}

void do_load(void) {
    if (ndirs == 0) {
        status = "NO FILES - PRESS NEW";
        return;
    }
    load_file(sel);
}

int exists(const char *name) {
    int i;
    for (i = 0; i < ndirs; i++) {
        if (same_name(files[i], name)) return 1;
    }
    return 0;
}

void do_new(void) {
    int n, i;
    for (n = 1; n < 10000; n++) {
        put_num(n);
        label[0] = 'E';
        label[1] = 'D';
        label[2] = 'I';
        label[3] = 'T';
        for (i = 0; i < 4; i++) label[4 + i] = num[1 + i];
        label[8] = '.';
        label[9] = 'T';
        label[10] = 'X';
        label[11] = 'T';
        label[12] = 0;
        if (!exists(label)) break;
    }
    if (n == 10000) {
        status = "NO FREE EDIT FILE NAMES";
        return;
    }
    ram[0] = 0;
    if (folder_fs_write(label, ram, 0) < 0) {
        status = "FILE CREATE FAILED";
        return;
    }
    refresh_dir();
    for (i = 0; i < ndirs; i++) {
        if (same_name(files[i], label)) sel = i;
    }
    loaded = 1;
    file_length = 0;
    cur = 0;
    dirty = 0;
    status = "NEW FILE CREATED";
}

void do_save(void) {
    int result;
    if (!loaded) {
        status = "NO FILE OPEN";
        return;
    }
    if (!dirty) {
        status = "NOTHING TO SAVE";
        return;
    }
    result = folder_fs_write(label, ram, file_length);
    if (result != file_length) {
        status = "WRITE FAILED (PROTECTED OR FULL)";
        return;
    }
    dirty = 0;
    status = "FILE SAVED";
}

void do_read(void) {
    int current;
    current = sel;
    if (loaded) {
        int i;
        for (i = 0; i < ndirs; i++) {
            if (same_name(files[i], label)) current = i;
        }
    }
    load_file(current);
}

void goto_file(int index) {
    if (dirty) {
        status = "SAVE OR READ FIRST";
        return;
    }
    if (index < 0) index = 0;
    if (index >= ndirs) index = ndirs - 1;
    if (ndirs == 0) {
        status = "NO FILES";
        return;
    }
    sel = index;
    load_file(sel);
}

void do_clear(void) {
    int i;
    if (!loaded) return;
    for (i = 0; i < EDIT_SIZE; i++) ram[i] = ' ';
    file_length = 0;
    cur = 0;
    dirty = 1;
    status = "BUFFER CLEARED";
}

void type_char(int character) {
    if (!loaded) return;
    if (character == 8) {
        if (cur > 0) {
            cur--;
            ram[cur] = ' ';
            dirty = 1;
        }
    } else if (character == 13 || character == 10) {
        if (cur < EDIT_SIZE) {
            ram[cur] = 10;
            cur++;
            if (cur > file_length) file_length = cur;
            dirty = 1;
        }
    } else if (character >= 32 && character < 127 && cur < EDIT_SIZE) {
        ram[cur] = character;
        cur++;
        if (cur > file_length) file_length = cur;
        dirty = 1;
    }
}

int inside(int mx, int my, int x, int y, int w, int h) {
    if (mx < x || mx >= x + w) return 0;
    if (my < y || my >= y + h) return 0;
    return 1;
}

void off_button(int x, int y, int w, char *name) {
    gfx_rect(x, y, w, 16, DARK_GRAY);
    gfx_text(x + 5, y + 4, name, GRAY);
}

void button(int x, int y, int w, int idx, char *name, unsigned char color) {
    int off;
    off = 0;
    if (press == idx) off = 1;
    gfx_rect(x, y, w, 16, DARK_GRAY);
    gfx_rect(x + 1, y + off, w - 2, 14, color);
    gfx_text(x + 5, y + 4 + off, name, WHITE);
}

void draw_list(void) {
    int i, row;
    unsigned char color;
    gfx_text(8, 24, "FILES:", YELLOW);
    for (row = 0; row < 5; row++) {
        i = top + row;
        gfx_rect(8, 34 + row * 12, 60, 11, BLACK);
        if (i < ndirs) {
            color = GRAY;
            if (i == sel) {
                gfx_rect(8, 34 + row * 12, 60, 11, BLUE);
                color = WHITE;
            }
            gfx_text(11, 36 + row * 12, files[i], color);
        }
    }
}

void draw_text_area(void) {
    int row, column, index;
    gfx_rect(TX - 3, TY - 3, COLS * 6 + 6, 86, BLACK);
    for (row = 0; row < 8; row++) {
        for (column = 0; column < COLS; column++) {
            index = row * COLS + column;
            line[column] = ' ';
            if (index < file_length && loaded) line[column] = ram[index];
            if (line[column] < 32) line[column] = '.';
        }
        line[COLS] = 0;
        gfx_text(TX, TY + row * 10, line, GREEN);
    }
    if (loaded) {
        gfx_rect(TX + (cur % COLS) * 6, TY + (cur / COLS) * 10 + 8, 6, 2,
                 YELLOW);
    }
}

void draw_screen(void) {
    gfx_clear(BLUE);
    gfx_rect(0, 0, 320, 16, WHITE);
    gfx_text(8, 4, "SC-16 FILE EDITOR", BLUE);
    gfx_rect(4, 18, 316, 218, GRAY);
    draw_list();
    button(72, 34, 16, 10, "^", DARK_GRAY);
    button(72, 78, 16, 11, "v", DARK_GRAY);
    if (loaded) {
        off_button(8, 100, 38, "LOAD");
        button(50, 100, 42, 1, "CLOSE", ORANGE);
        off_button(8, 120, 38, "NEW");
    } else {
        if (ndirs > 0) button(8, 100, 38, 0, "LOAD", BLUE);
        else off_button(8, 100, 38, "LOAD");
        button(8, 120, 38, 2, "NEW", GREEN);
        off_button(50, 100, 42, "CLOSE");
    }
    gfx_text(104, 24, "FILE:", BLACK);
    gfx_text(140, 24, label, BLUE);
    if (!loaded) gfx_text(140, 24, "--------", DARK_GRAY);
    gfx_text(200, 24, "BYTES:", BLACK);
    put_num(file_length);
    gfx_text(242, 24, num, BLUE);
    if (dirty) gfx_text(278, 24, "UNSAVED", DARK_RED);
    draw_text_area();

    if (loaded && !dirty) {
        button(104, 134, 38, 3, "PREV", BLUE);
        button(146, 134, 38, 4, "NEXT", BLUE);
    } else {
        off_button(104, 134, 38, "PREV");
        off_button(146, 134, 38, "NEXT");
    }
    if (loaded) button(188, 134, 38, 5, "READ", GREEN);
    else off_button(188, 134, 38, "READ");
    if (loaded) button(272, 134, 38, 7, "CLR", ORANGE);
    else off_button(272, 134, 38, "CLR");
    if (dirty) button(230, 134, 38, 6, "SAVE", DARK_RED);
    else off_button(230, 134, 38, "SAVE");

    gfx_text(8, 152, "STATUS:", BLACK);
    gfx_text(56, 152, status, DARK_RED);
    gfx_rect(8, 168, 304, 66, DARK_GRAY);
    gfx_rect(8, 168, 304, 2, WHITE);
    gfx_text(16, 184, "MOUNTED FOLDER DISK", WHITE);
    gfx_text(16, 202, "EDIT TEXT FILES UP TO 256 BYTES", GRAY);
    gfx_line(gfx_mouse_x(), gfx_mouse_y(), gfx_mouse_x() + 6,
             gfx_mouse_y() + 8, YELLOW);
    gfx_line(gfx_mouse_x(), gfx_mouse_y(), gfx_mouse_x(),
             gfx_mouse_y() + 10, YELLOW);
}

int which_button(int mx, int my) {
    if (inside(mx, my, 8, 100, 38, 16)) return 0;
    if (inside(mx, my, 50, 100, 42, 16)) return 1;
    if (inside(mx, my, 8, 120, 38, 16)) return 2;
    if (inside(mx, my, 104, 134, 38, 16)) return 3;
    if (inside(mx, my, 146, 134, 38, 16)) return 4;
    if (inside(mx, my, 188, 134, 38, 16)) return 5;
    if (inside(mx, my, 230, 134, 38, 16)) return 6;
    if (inside(mx, my, 272, 134, 38, 16)) return 7;
    if (inside(mx, my, 72, 34, 16, 16)) return 10;
    if (inside(mx, my, 72, 78, 16, 16)) return 11;
    return -1;
}

void start_act(int action) {
    act = action;
    anim_t = ANIM;
    if (action >= 3 && action <= 6)
        gfx_sound(0, 110, ANIM, WAVE_NOISE, 12);
}

void finish_act(void) {
    if (act == 3) goto_file(sel - 1);
    if (act == 4) goto_file(sel + 1);
    if (act == 5) do_read();
    if (act == 6) do_save();
    gfx_sound(1, 400, 3, WAVE_SQUARE, 20);
}

void click(int mx, int my) {
    int index;
    if (anim_t > 0) return;
    if (press == 0 && !loaded) do_load();
    if (press == 1 && loaded) do_eject();
    if (press == 2 && !loaded) do_new();
    if (press == 3 && loaded && !dirty) start_act(3);
    if (press == 4 && loaded && !dirty) start_act(4);
    if (press == 5 && loaded) start_act(5);
    if (press == 6 && loaded && dirty) start_act(6);
    if (press == 7 && loaded) do_clear();
    if (press == 10 && top > 0) top--;
    if (press == 11 && top + 5 < ndirs) top++;
    if (inside(mx, my, 8, 34, 60, 60) && !loaded) {
        index = top + (my - 34) / 12;
        if (index < ndirs) sel = index;
    }
    if (inside(mx, my, TX, TY, COLS * 6, 80) && loaded) {
        index = (my - TY) / 10 * COLS + (mx - TX) / 6;
        if (index > EDIT_SIZE) index = EDIT_SIZE;
        cur = index;
    }
}

int main(void) {
    int character, mx, my, down, was_down, event;
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_layer_reset(1 << LAYER_BACKGROUND);
    was_down = 0;
    status = "PICK A FILE OR PRESS NEW";
    refresh_dir();
    while (1) {
        mx = gfx_mouse_x();
        my = gfx_mouse_y();
        down = gfx_mouse_buttons() & MOUSE_LEFT;
        event = gfx_mouse_events() & MOUSE_LEFT_DOWN;
        press = -1;
        if ((down || event) && anim_t == 0)
            press = which_button(mx, my);
        if (event || (down && !was_down)) click(mx, my);
        was_down = down;
        character = gfx_getchar();
        while (character) {
            if (anim_t == 0) type_char(character);
            character = gfx_getchar();
        }
        if (anim_t > 0) {
            anim_t--;
            if (anim_t == 0) finish_act();
        }
        if (led > 0) led--;
        draw_screen();
        gfx_present();
    }
    return 0;
}
