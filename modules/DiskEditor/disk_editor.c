/* Disk editor: pick a disk from disks/, then edit and save its 256-character blocks. */
#include "suomi_gfx.h"

#define COLS 34
#define TX 104
#define TY 44
#define ANIM 30

unsigned char *dk = 0x43100;      /* control byte, id at +1..+8 */
unsigned int *dw = 0x43100;       /* dw[5] pointer, dw[6] RAM offset, dw[7] disk count */
char *ram = 0x20000;              /* edit buffer at 0, directory scratch at 0x200 */

char dirs[144];                   /* up to 16 disk ids, 9 bytes each */
char dprot[16];                   /* write-protect flag per listed disk */
char label[9] = "        ";
char num[6];
char line[COLS + 1];
int ndirs, top, sel, loaded, block, cur, dirty, led, press, anim_t, act, prot;
char *status;

void put_num(int v) {
    int i;
    i = 4;
    num[5] = 0;
    while (i >= 0) {
        num[i] = 48 + v % 10;
        v = v / 10;
        i--;
    }
}

void disk_cmd(unsigned char cmd) {
    int i;
    for (i = 0; i < 8; i++) {
        dk[1 + i] = label[i];
    }
    dk[0] = cmd;
    led = 6;
}

void refresh_dir(void) {
    int i, j;
    dw[5] = 0;
    dw[6] = 0x200;
    disk_cmd(0x10);
    ndirs = dw[7];
    if (ndirs > 16) ndirs = 16;
    for (i = 0; i < ndirs; i++) {
        dw[5] = i;
        dw[6] = 0x200;
        disk_cmd(0x10);
        for (j = 0; j < 9; j++) {
            dirs[i * 9 + j] = ram[0x200 + j];
        }
        dprot[i] = ram[0x209];
    }
    if (sel >= ndirs) sel = ndirs - 1;
    if (sel < 0) sel = 0;
}

void read_block(void) {
    dw[5] = block;
    dw[6] = 0;
    disk_cmd(4);
    ram[256] = 0;
    dirty = 0;
}

void do_eject(void) {
    if (loaded) {
        disk_cmd(2);
        loaded = 0;
        prot = 0;
        refresh_dir();
        status = "DISK EJECTED";
    }
}

void load_named(void) {
    do_eject();
    disk_cmd(1);
    prot = dw[7];
    loaded = 1;
    block = 0;
    cur = 0;
    read_block();
    refresh_dir();
    status = "DISK LOADED";
}

void do_load(void) {
    int i;
    if (ndirs == 0) {
        status = "NO DISKS - PRESS NEW";
        return;
    }
    for (i = 0; i < 8; i++) {
        label[i] = dirs[sel * 9 + i];
    }
    load_named();
}

int exists(void) {
    int i, j, same;
    for (i = 0; i < ndirs; i++) {
        same = 1;
        for (j = 0; j < 8; j++) {
            if (dirs[i * 9 + j] != label[j]) same = 0;
        }
        if (same) return 1;
    }
    return 0;
}

void do_new(void) {
    int n, i;
    for (n = 1; n < 10000; n++) {
        put_num(n);
        label[0] = 'D';
        label[1] = 'I';
        label[2] = 'S';
        label[3] = 'K';
        for (i = 0; i < 4; i++) {
            label[4 + i] = num[1 + i];
        }
        if (!exists()) {
            n = 10000;
        }
    }
    load_named();
    status = "NEW DISK CREATED";
}

void do_save(void) {
    if (!loaded) {
        status = "NO DISK LOADED";
        return;
    }
    if (prot) {
        status = "DISK WRITE PROTECTED";
        return;
    }
    if (!dirty) {
        status = "NOTHING TO SAVE";
        return;
    }
    dw[5] = block;
    dw[6] = 0;
    disk_cmd(8);
    dirty = 0;
    status = "BLOCK SAVED";
}

void do_read(void) {
    if (!loaded) {
        status = "NO DISK LOADED";
        return;
    }
    read_block();
    status = "BLOCK READ";
}

void goto_block(int b) {
    if (!loaded) {
        status = "NO DISK LOADED";
        return;
    }
    if (dirty) {
        status = "SAVE OR READ FIRST";
        return;
    }
    if (b < 0) b = 0;
    block = b;
    cur = 0;
    read_block();
    status = "BLOCK READ";
}

void do_clear(void) {
    int i;
    if (!loaded || prot) return;
    for (i = 0; i < 256; i++) {
        ram[i] = ' ';
    }
    cur = 0;
    dirty = 1;
    status = "BUFFER CLEARED";
}

void type_char(int c) {
    if (!loaded) return;
    if (prot) {
        if (c >= 32 || c == 8) status = "DISK WRITE PROTECTED";
        return;
    }
    if (c == 8) {
        if (cur > 0) cur--;
        ram[cur] = ' ';
        dirty = 1;
    } else if (c == 13) {
        cur = cur + COLS - cur % COLS;
        if (cur > 255) cur = 255;
    } else if (c >= 32 && c < 127) {
        ram[cur] = c;
        if (cur < 255) cur++;
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
    unsigned char c;
    gfx_text(8, 24, "DISKS:", YELLOW);
    for (row = 0; row < 5; row++) {
        i = top + row;
        gfx_rect(8, 34 + row * 12, 60, 11, BLACK);
        if (i < ndirs) {
            c = GRAY;
            if (i == sel) {
                gfx_rect(8, 34 + row * 12, 60, 11, BLUE);
                c = WHITE;
            }
            line[0] = 0;
            gfx_text(11, 36 + row * 12, dirs + i * 9, c);
        }
    }
}

void draw_text_area(void) {
    int r, k, i;
    gfx_rect(TX - 3, TY - 3, COLS * 6 + 6, 86, BLACK);
    for (r = 0; r < 8; r++) {
        for (k = 0; k < COLS; k++) {
            i = r * COLS + k;
            line[k] = ' ';
            if (i < 256 && loaded) line[k] = ram[i];
            if (line[k] < 32) line[k] = '.';
        }
        line[COLS] = 0;
        gfx_text(TX, TY + r * 10, line, GREEN);
    }
    if (loaded) {
        gfx_rect(TX + (cur % COLS) * 6, TY + (cur / COLS) * 10 + 8, 6, 2, YELLOW);
    }
}

void draw_drive(void) {
    int vis, prog, pm;
    char *nm;
    vis = 0;
    if (!loaded) vis = 30;
    if (anim_t > 0) {
        if (act == 0 || act == 2) vis = 30 * anim_t / ANIM;
        if (act == 1) vis = 30 - 30 * anim_t / ANIM;
    }
    nm = label;
    if (!loaded || (anim_t > 0 && (act == 0 || act == 2))) {
        nm = dirs + sel * 9;
        if (act == 2 && anim_t > 0) nm = "NEW DISK";
    }
    gfx_rect(8, 168, 304, 66, DARK_GRAY);
    gfx_rect(8, 168, 304, 2, WHITE);
    gfx_text(16, 176, "DF0:", WHITE);
    if (vis > 0) {
        gfx_rect(76, 210 - vis, 128, vis, BLUE);
        if (vis >= 14) {
            gfx_rect(96, 212 - vis, 88, 11, WHITE);
            gfx_text(110, 214 - vis, nm, BLACK);
        }
    }
    if (vis < 30) {
        pm = prot;
        if (act == 0 && anim_t > 0) pm = dprot[sel];
        if (act == 2 && anim_t > 0) pm = 0;
        if (pm) gfx_rect(196, 210 - vis + 2, 6, 6, RED);
    }
    gfx_rect(70, 210, 140, 8, BLACK);
    gfx_rect(290, 212, 10, 5, GRAY);
    if (anim_t > 0 || led) {
        if (act == 6) gfx_rect(290, 212, 10, 5, RED);
        else gfx_rect(290, 212, 10, 5, GREEN);
    }
    if (anim_t > 0) {
        prog = 60 * (ANIM - anim_t) / ANIM;
        if (act == 0 || act == 2) gfx_text(226, 176, "LOADING", YELLOW);
        if (act == 1) gfx_text(226, 176, "EJECTING", YELLOW);
        if (act == 6) gfx_text(226, 176, "WRITING", YELLOW);
        if (act >= 3 && act <= 5) gfx_text(226, 176, "READING", YELLOW);
        gfx_rect(226, 190, 66, 8, BLACK);
        if (prog > 0) gfx_rect(226, 190, prog, 8, GREEN);
        if (act >= 3 && act <= 6) {
            gfx_rect(226, 203, 66, 2, GRAY);
            gfx_rect(226 + prog, 200, 6, 8, YELLOW);
        }
    }
}

void draw_screen(void) {
    gfx_clear(BLUE);
    gfx_rect(0, 0, 320, 16, WHITE);
    gfx_text(8, 4, "SC-16 DISK EDITOR", BLUE);
    gfx_rect(4, 18, 308 + 8, 218, GRAY);
    draw_list();
    button(72, 34, 16, 10, "^", DARK_GRAY);
    button(72, 78, 16, 11, "v", DARK_GRAY);
    if (loaded) {
        off_button(8, 100, 38, "LOAD");
        off_button(8, 120, 38, "NEW");
    } else {
        if (ndirs > 0) button(8, 100, 38, 0, "LOAD", BLUE);
        else off_button(8, 100, 38, "LOAD");
        button(8, 120, 38, 2, "NEW", GREEN);
    }
    if (loaded && !dirty) {
        button(50, 100, 42, 1, "EJECT", ORANGE);
        if (prot) button(50, 120, 42, 8, "PROT", DARK_RED);
        else button(50, 120, 42, 8, "PROT", BLUE);
    } else {
        off_button(50, 100, 42, "EJECT");
        off_button(50, 120, 42, "PROT");
    }

    gfx_text(104, 24, "DISK:", BLACK);
    gfx_text(140, 24, label, BLUE);
    if (!loaded) gfx_text(140, 24, "--------", DARK_GRAY);
    gfx_text(200, 24, "BLOCK:", BLACK);
    put_num(block);
    gfx_text(242, 24, num, BLUE);
    if (dirty) gfx_text(278, 24, "UNSAVED", DARK_RED);
    draw_text_area();

    if (dirty || !loaded) {
        off_button(104, 134, 38, "PREV");
        off_button(146, 134, 38, "NEXT");
    } else {
        button(104, 134, 38, 3, "PREV", BLUE);
        button(146, 134, 38, 4, "NEXT", BLUE);
    }
    if (loaded && dirty) {
        button(188, 134, 38, 5, "READ", GREEN);
    } else {
        off_button(188, 134, 38, "READ");
    }
    if (loaded && !prot) {
        button(272, 134, 38, 7, "CLR", ORANGE);
    } else {
        off_button(272, 134, 38, "CLR");
    }
    if (dirty && loaded && !prot) {
        button(230, 134, 38, 6, "SAVE", DARK_RED);
    } else {
        off_button(230, 134, 38, "SAVE");
    }

    gfx_text(8, 152, "STATUS:", BLACK);
    gfx_text(56, 152, status, DARK_RED);
    draw_drive();
    gfx_line(gfx_mouse_x(), gfx_mouse_y(), gfx_mouse_x() + 6, gfx_mouse_y() + 8, YELLOW);
    gfx_line(gfx_mouse_x(), gfx_mouse_y(), gfx_mouse_x(), gfx_mouse_y() + 10, YELLOW);
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
    if (inside(mx, my, 50, 120, 42, 16)) return 8;
    if (inside(mx, my, 72, 34, 16, 16)) return 10;
    if (inside(mx, my, 72, 78, 16, 16)) return 11;
    return -1;
}

void start_act(int a) {
    act = a;
    anim_t = ANIM;
    if (a >= 3 && a <= 6) gfx_sound(0, 110, ANIM, WAVE_NOISE, 12);
}

void finish_act(void) {
    if (act == 0) do_load();
    if (act == 1) do_eject();
    if (act == 2) do_new();
    if (act == 3) goto_block(block - 1);
    if (act == 4) goto_block(block + 1);
    if (act == 5) do_read();
    if (act == 6) do_save();
    gfx_sound(1, 400, 3, WAVE_SQUARE, 20);
}

void click(int mx, int my) {
    int i;
    if (anim_t > 0) return;
    if (press == 1 && !loaded) return;
    if (press >= 3 && press <= 7 && !loaded) return;
    if (press == 6 && !dirty) return;
    if (press == 5 && !dirty) return;
    if (press == 7 && prot) return;
    if (press == 6 && prot) return;
    if ((press == 0 || press == 2) && loaded) return;
    if (press == 0 && ndirs == 0) return;
    if ((press == 1 || press == 8) && dirty) {
        status = "SAVE OR READ FIRST";
        return;
    }
    if (press == 8) {
        if (!loaded) return;
        dw[5] = !prot;
        disk_cmd(0x20);
        prot = dw[7];
        if (prot) status = "WRITE PROTECT ON";
        else status = "WRITE PROTECT OFF";
        return;
    }
    if (press >= 0 && press <= 6) start_act(press);
    if (press == 7) do_clear();
    if (press == 10 && top > 0) top--;
    if (press == 11 && top + 5 < ndirs) top++;
    if (inside(mx, my, 8, 34, 60, 60)) {
        i = top + (my - 34) / 12;
        if (i < ndirs) sel = i;
    }
    if (inside(mx, my, TX, TY, COLS * 6, 80) && loaded) {
        i = (my - TY) / 10 * COLS + (mx - TX) / 6;
        if (i > 255) i = 255;
        cur = i;
    }
}

int main(void) {
    int c, mx, my, down, was_down, ev;
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_layer_reset(1 << LAYER_BACKGROUND);
    was_down = 0;
    status = "PICK A DISK OR PRESS NEW";
    ram[0] = 0;
    refresh_dir();
    while (1) {
        mx = gfx_mouse_x();
        my = gfx_mouse_y();
        down = gfx_mouse_buttons() & MOUSE_LEFT;
        ev = gfx_mouse_events() & MOUSE_LEFT_DOWN;
        press = -1;
        if ((down || ev) && anim_t == 0) press = which_button(mx, my);
        if (ev || (down && !was_down)) click(mx, my);
        was_down = down;
        c = gfx_getchar();
        while (c) {
            if (anim_t == 0) type_char(c);
            c = gfx_getchar();
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
