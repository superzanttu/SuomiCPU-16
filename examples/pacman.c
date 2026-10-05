// Pac-Man for the SC-8.  Arrow keys / WASD steer, Enter or Space restarts after GAME OVER.
// The maze is 19x21 tiles of 8x8 pixels.  Entities move 2 pixels per frame.
#include "suomi_gfx.h"

#define MW 19
#define MH 21
#define OX 84
#define OY 28
#define TUNNEL_ROW 8
#define NG 4

char layout[400] = "####################........#........##o##.###.#.###.##o##.................##.##.#.#####.#.##.##....#...#...#....#####.#       #.########.# ##-## #.####    .  #   #  .    ####.# ##### #.########.#       #.########.# ##### #.#####........#........##.##.###.#.###.##.##o.#...........#.o###.#.#.#####.#.#.###....#...#...#....##.######.#.######.##.................##........#........####################";

char maze[672];
char dxs[4] = {-1, 1, 0, 0};
char dys[4] = {0, 0, -1, 1};
char opp[4] = {1, 0, 3, 2};

unsigned char pac_closed[8] = {0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C};
unsigned char pac_l[8] = {0x3C, 0x7E, 0x1F, 0x07, 0x07, 0x1F, 0x7E, 0x3C};
unsigned char pac_r[8] = {0x3C, 0x7E, 0xF8, 0xE0, 0xE0, 0xF8, 0x7E, 0x3C};
unsigned char pac_u[8] = {0x81, 0xC3, 0xE7, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C};
unsigned char pac_d[8] = {0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xE7, 0xC3, 0x81};
unsigned char ghost_body[8] = {0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xA5};
unsigned char ghost_eyes[8] = {0x00, 0x00, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00};
unsigned char ghost_color[4] = {RED, MAGENTA, CYAN, ORANGE};

int px;
int py;
int pdir;
int pwant;
int pmoving;
int pdx;
int pdy;
int gx[NG];
int gy[NG];
int gd[NG];
int gs[NG];
int gt[NG];
int gdx[NG];
int gdy[NG];
int pellets;
int fright;
int chain;
int tick;
int lives;
int level;
int mode;
int wait;
int hud_dirty;
unsigned int score;

int abs_i(int v)
{
    if (v < 0) {
        return -v;
    }
    return v;
}

int passable(int tx, int ty, int door)
{
    char c;
    if (ty == TUNNEL_ROW && (tx < 0 || tx >= MW)) {
        return 1;
    }
    if (tx < 0 || tx >= MW || ty < 0 || ty >= MH) {
        return 0;
    }
    c = maze[(ty << 5) + tx];
    if (c == '#') {
        return 0;
    }
    if (c == '-') {
        return door;
    }
    return 1;
}

void draw_tile(int tx, int ty)
{
    int x;
    int y;
    char c;
    x = OX + (tx << 3);
    y = OY + (ty << 3);
    if (tx < 0 || tx >= MW || ty < 0 || ty >= MH) {
        gfx_rect(x, y, 8, 8, BLACK);
        return;
    }
    c = maze[(ty << 5) + tx];
    if (c == '#') {
        gfx_rect(x, y, 8, 8, BLUE);
        gfx_rect(x + 2, y + 2, 4, 4, BLACK);
    } else {
        gfx_rect(x, y, 8, 8, BLACK);
        if (c == '.') {
            gfx_rect(x + 3, y + 3, 2, 2, WHITE);
        } else if (c == 'o') {
            gfx_rect(x + 2, y + 2, 4, 4, ORANGE);
        } else if (c == '-') {
            gfx_rect(x, y + 3, 8, 2, MAGENTA);
        }
    }
}

void draw_maze(void)
{
    int tx;
    int ty;
    gfx_clear(BLACK);
    for (ty = 0; ty < MH; ty++) {
        for (tx = 0; tx < MW; tx++) {
            draw_tile(tx, ty);
        }
    }
    hud_dirty = 1;
}

void erase_at(int x, int y)
{
    int tx;
    int ty;
    int tx0;
    int tx1;
    int ty1;
    tx0 = x >> 3;
    tx1 = (x + 7) >> 3;
    ty = y >> 3;
    ty1 = (y + 7) >> 3;
    for (; ty <= ty1; ty++) {
        for (tx = tx0; tx <= tx1; tx++) {
            draw_tile(tx, ty);
        }
    }
}

void number_text(unsigned int value, char *out)
{
    char tmp[6];
    int n;
    int i;
    n = 0;
    if (value == 0) {
        tmp[0] = '0';
        n = 1;
    }
    while (value > 0) {
        tmp[n] = '0' + (value % 10);
        value = value / 10;
        n++;
    }
    for (i = 0; i < n; i++) {
        out[i] = tmp[n - 1 - i];
    }
    out[n] = 0;
}

void draw_hud(void)
{
    char buffer[8];
    int i;
    gfx_rect(OX, 8, 152, 8, BLACK);
    gfx_text(OX, 8, "SCORE", GRAY);
    number_text(score, buffer);
    gfx_text(OX + 36, 8, buffer, WHITE);
    gfx_text(OX + 100, 8, "LEVEL", GRAY);
    number_text(level + 1, buffer);
    gfx_text(OX + 136, 8, buffer, WHITE);
    gfx_rect(OX, 204, 152, 10, BLACK);
    for (i = 0; i < lives; i++) {
        gfx_bitmap(OX + i * 10, 205, 8, 8, pac_r, YELLOW);
    }
    hud_dirty = 0;
}

void load_maze(void)
{
    int tx;
    int ty;
    int i;
    char c;
    i = 0;
    pellets = 0;
    for (ty = 0; ty < MH; ty++) {
        for (tx = 0; tx < MW; tx++) {
            c = layout[i];
            maze[(ty << 5) + tx] = c;
            if (c == '.' || c == 'o') {
                pellets++;
            }
            i++;
        }
    }
}

void reset_positions(void)
{
    int i;
    px = 72;
    py = 112;
    pdir = 0;
    pwant = 0;
    pmoving = 0;
    gx[0] = 72; gy[0] = 48; gs[0] = 1; gt[0] = 0;
    gx[1] = 72; gy[1] = 64; gs[1] = 0; gt[1] = 30;
    gx[2] = 64; gy[2] = 64; gs[2] = 0; gt[2] = 120;
    gx[3] = 80; gy[3] = 64; gs[3] = 0; gt[3] = 240;
    for (i = 0; i < NG; i++) {
        gd[i] = 0;
        gdx[i] = gx[i];
        gdy[i] = gy[i];
    }
    pdx = px;
    pdy = py;
    fright = 0;
    chain = 0;
}

void start_ready(void)
{
    draw_maze();
    reset_positions();
    mode = 0;
    wait = 70;
}

void new_game(void)
{
    score = 0;
    lives = 3;
    level = 0;
    load_maze();
    start_ready();
}

int wrap_x(int x, int d)
{
    if (d == 0 && x <= -8) {
        return MW * 8;
    }
    if (d == 1 && x >= MW * 8 + 8) {
        return -8;
    }
    return x;
}

void move_pac(unsigned int keys)
{
    int tx;
    int ty;
    int tile;
    if (keys & KEY_LEFT) {
        pwant = 0;
    } else if (keys & KEY_RIGHT) {
        pwant = 1;
    } else if (keys & KEY_UP) {
        pwant = 2;
    } else if (keys & KEY_DOWN) {
        pwant = 3;
    }
    if (pwant == opp[pdir]) {
        pdir = pwant;
        pmoving = 1;
    }
    if ((px & 7) == 0 && (py & 7) == 0) {
        tx = px >> 3;
        ty = py >> 3;
        if (passable(tx + dxs[pwant], ty + dys[pwant], 0)) {
            pdir = pwant;
            pmoving = 1;
        } else if (passable(tx + dxs[pdir], ty + dys[pdir], 0)) {
            pmoving = 1;
        } else {
            pmoving = 0;
        }
    }
    if (pmoving) {
        px = px + dxs[pdir] * 2;
        py = py + dys[pdir] * 2;
        px = wrap_x(px, pdir);
    }
    tx = (px + 4) >> 3;
    ty = (py + 4) >> 3;
    if (tx >= 0 && tx < MW) {
        tile = (ty << 5) + tx;
        if (maze[tile] == '.') {
            maze[tile] = ' ';
            score = score + 10;
            pellets--;
            hud_dirty = 1;
            draw_tile(tx, ty);
        } else if (maze[tile] == 'o') {
            maze[tile] = ' ';
            score = score + 50;
            pellets--;
            hud_dirty = 1;
            fright = 190 - level * 25;
            if (fright < 50) {
                fright = 50;
            }
            chain = 0;
            for (tile = 0; tile < NG; tile++) {
                if (gs[tile] == 1) {
                    gd[tile] = opp[gd[tile]];
                }
            }
            draw_tile(tx, ty);
        }
    }
}

void choose_dir(int i)
{
    int tx;
    int ty;
    int targx;
    int targy;
    int ptx;
    int pty;
    int d;
    int best;
    int bestd;
    int dist;
    int nx;
    int ny;
    int door;
    tx = gx[i] >> 3;
    ty = gy[i] >> 3;
    ptx = (px + 4) >> 3;
    pty = (py + 4) >> 3;
    door = 0;
    if (gs[i] == 2) {
        targx = 9;
        targy = 8;
        door = 1;
    } else if (gs[i] == 3) {
        targx = 9;
        targy = 6;
        door = 1;
    } else if (fright > 0) {
        targx = 0;
        targy = 0;
    } else if (i == 0) {
        targx = ptx;
        targy = pty;
    } else if (i == 1) {
        targx = ptx + dxs[pdir] * 4;
        targy = pty + dys[pdir] * 4;
    } else if (i == 2) {
        targx = ptx + dxs[pdir] * 2;
        targy = pty + dys[pdir] * 2;
        targx = targx + targx - (gx[0] >> 3);
        targy = targy + targy - (gy[0] >> 3);
    } else {
        targx = ptx;
        targy = pty;
        if (abs_i(tx - ptx) + abs_i(ty - pty) < 8) {
            targx = 1;
            targy = 19;
        }
    }
    best = -1;
    bestd = 30000;
    for (d = 0; d < 4; d++) {
        if (d == opp[gd[i]]) {
            continue;
        }
        nx = tx + dxs[d];
        ny = ty + dys[d];
        if (!passable(nx, ny, door)) {
            continue;
        }
        if (fright > 0 && gs[i] == 1) {
            dist = gfx_random();
        } else {
            dist = abs_i(nx - targx) + abs_i(ny - targy);
        }
        if (dist < bestd) {
            bestd = dist;
            best = d;
        }
    }
    if (best < 0) {
        best = opp[gd[i]];
    }
    gd[i] = best;
}

void move_ghost(int i)
{
    int tx;
    int ty;
    if (gs[i] == 0) {
        gt[i] = gt[i] - 1;
        if (gt[i] <= 0) {
            gs[i] = 3;
            gd[i] = 1;
        }
        return;
    }
    if (gs[i] == 1 && fright > 0 && (tick & 1)) {
        return;
    }
    if ((gx[i] & 7) == 0 && (gy[i] & 7) == 0) {
        tx = gx[i] >> 3;
        ty = gy[i] >> 3;
        if (gs[i] == 3 && tx == 9 && ty == 6) {
            gs[i] = 1;
        } else if (gs[i] == 2 && tx == 9 && ty == 8) {
            gs[i] = 3;
        }
        choose_dir(i);
    }
    gx[i] = gx[i] + dxs[gd[i]] * 2;
    gy[i] = gy[i] + dys[gd[i]] * 2;
    gx[i] = wrap_x(gx[i], gd[i]);
}

void lose_life(void)
{
    mode = 2;
    wait = 50;
}

void check_hits(void)
{
    int i;
    for (i = 0; i < NG; i++) {
        if (gs[i] == 1 || gs[i] == 3) {
            if (abs_i(gx[i] - px) < 6 && abs_i(gy[i] - py) < 6) {
                if (gs[i] == 1 && fright > 0) {
                    gs[i] = 2;
                    chain++;
                    score = score + (100 << chain);
                    hud_dirty = 1;
                } else if (mode == 1) {
                    lose_life();
                }
            }
        }
    }
}

void erase_all(void)
{
    int i;
    for (i = 0; i < NG; i++) {
        erase_at(gdx[i], gdy[i]);
    }
    erase_at(pdx, pdy);
}

void draw_entities(void)
{
    int i;
    unsigned char *bmp;
    unsigned char color;
    for (i = 0; i < NG; i++) {
        gdx[i] = gx[i];
        gdy[i] = gy[i];
        if (gs[i] != 2) {
            color = ghost_color[i];
            if (fright > 0 && gs[i] == 1) {
                color = BLUE;
                if (fright < 60 && (fright & 8)) {
                    color = WHITE;
                }
            }
            gfx_bitmap(gx[i] + OX, gy[i] + OY, 8, 8, ghost_body, color);
        }
        if (fright == 0 || gs[i] != 1) {
            gfx_bitmap(gx[i] + OX, gy[i] + OY, 8, 8, ghost_eyes, WHITE);
        }
    }
    pdx = px;
    pdy = py;
    if (mode == 2) {
        if (wait & 4) {
            return;
        }
        bmp = pac_closed;
    } else if (((tick >> 2) & 1) && pmoving) {
        bmp = pac_closed;
    } else if (pdir == 0) {
        bmp = pac_l;
    } else if (pdir == 1) {
        bmp = pac_r;
    } else if (pdir == 2) {
        bmp = pac_u;
    } else {
        bmp = pac_d;
    }
    gfx_bitmap(px + OX, py + OY, 8, 8, bmp, YELLOW);
}

void update(unsigned int keys)
{
    int i;
    int t;
    if (mode == 0) {
        if (wait > 0) {
            wait--;
        } else {
            for (t = 6; t <= 12; t++) {
                draw_tile(t, 10);
            }
            mode = 1;
        }
        return;
    }
    if (mode == 2) {
        wait--;
        if (wait <= 0) {
            lives--;
            hud_dirty = 1;
            if (lives <= 0) {
                mode = 3;
                wait = 40;
            } else {
                start_ready();
            }
        }
        return;
    }
    if (mode == 3) {
        if (wait > 0) {
            wait--;
        } else if (keys & (KEY_START | KEY_FIRE)) {
            new_game();
        }
        return;
    }
    if (fright > 0) {
        fright--;
    }
    move_pac(keys);
    for (i = 0; i < NG; i++) {
        move_ghost(i);
    }
    check_hits();
    if (pellets <= 0) {
        level++;
        load_maze();
        start_ready();
    }
}

int main(void)
{
    unsigned int keys;
    new_game();
    while (1) {
        keys = gfx_keys();
        tick++;
        update(keys);
        if (mode != 3 || wait == 40) {
            erase_all();
        }
        if (hud_dirty) {
            draw_hud();
        }
        if (mode == 3) {
            gfx_text(OX + 44, OY + 80, "GAME OVER", RED);
            gfx_text(OX + 12, OY + 96, "PRESS ENTER", WHITE);
        } else {
            draw_entities();
            if (mode == 0) {
                gfx_text(OX + 58, OY + 80, "READY!", YELLOW);
            }
        }
        gfx_present();
    }
    return 0;
}