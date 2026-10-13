// SC-16 showcase: a tour of the machine, written in C for the SC-16 itself.
// ICON
// ################
// #..............#
// #..##########..#
// #..#........#..#
// #..#..####..#..#
// #..#..####..#..#
// #..#........#..#
// #..##########..#
// #..............#
// #....##..##....#
// #....##..##....#
// #....##..##....#
// #....##..##....#
// #....######....#
// #..............#
// ################
// Left/Right (or A/D) change page.  Press Enter on the benchmark/comparison pages.
// Pages: title, CPU, memory, graphics, animation, text/font, devices, benchmark,
//        speed comparison, mouse/input, tile map/layers, LAN, 3D ship,
//        maximum-speed sprite load test.
// All timing uses the machine's own clocks: gfx_ticks() (emulated frames; 30/s
// at fixed speed) and gfx_rtc() (the real-time clock registers).
#include "suomi_gfx.h"

#define PAGES 14
#define NMACH 8
#define SC16_ROW 4
#define CMP_SECS 3
#define NTEST 7
#define DUR 30
#define BENCH_ADDS_PER_BLOCK 32
/* Compiled bench_block(0) executes 533 SC-16 instructions for 32 additions. */
#define BENCH_INSTRUCTIONS_PER_BLOCK 533
#define FONT_ROM 0x4400
#define TILE_MAP_W 40
#define TILE_MAP_H 20
#define NETWORK_PAGE 11
#define STRESS_PAGE 13
#define SHIP_VERTICES 27
#define SHIP_FACES 44

char sintab[32] = {0, 20, 38, 56, 71, 83, 92, 98, 100, 98, 92, 83, 71, 56, 38, 20,
                   0, -20, -38, -56, -71, -83, -92, -98, -100, -98, -92, -83, -71, -56, -38, -20};
unsigned char rainbow[7] = {RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, MAGENTA};
unsigned char logo[35] = {0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E,
                          0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E,
                          0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00,
                          0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E,
                          0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E};
unsigned char bit5[5] = {16, 8, 4, 2, 1};
unsigned char bit8[8] = {128, 64, 32, 16, 8, 4, 2, 1};
unsigned char ball[8] = {0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C};
unsigned char smiley[64] = {
    0, 0, 5, 5, 5, 5, 0, 0,
    0, 5, 5, 5, 5, 5, 5, 0,
    5, 5, 1, 5, 5, 1, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5,
    5, 2, 5, 5, 5, 5, 2, 5,
    5, 5, 2, 2, 2, 2, 5, 5,
    0, 5, 5, 5, 5, 5, 5, 0,
    0, 0, 5, 5, 5, 5, 0, 0};
char tri[6] = {0, -18, 16, 14, -16, 14};
char star5[10] = {0, -20, 6, -6, 20, -6, 10, 4, 12, 20};

int cvx[8] = {-36, 36, 36, -36, -36, 36, 36, -36};
int cvy[8] = {-36, -36, 36, 36, -36, -36, 36, 36};
int cvz[8] = {-36, -36, -36, -36, 36, 36, 36, 36};
int cea[12] = {0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3};
int ceb[12] = {1, 2, 3, 0, 5, 6, 7, 4, 4, 5, 6, 7};
int cpx[8];
int cpy[8];

char logo_text[8] = "SC-16 !";
int starx[40];
int stary[40];
int ballx[3] = {20, 140, 250};
int bally[3] = {170, 190, 150};
int balldx[3] = {2, -3, 1};
int balldy[3] = {1, 2, -2};

unsigned int hist[16];
unsigned int hist_total;
unsigned int rng_last;

unsigned int bench_blocks[NTEST];
unsigned int bench_kops[NTEST];
unsigned int bench_scale[NTEST] = {1, 1, 1, 1, 1, 1, 1};
int bench_state;
int cmp_state;
unsigned int mach_tenths[NMACH] = {1, 3, 4, 7, 18, 540, 1600, 8750};
int bench_index;
unsigned int bench_t0;
unsigned int bench_ticks;
unsigned int bench_host_start;
unsigned int bench_host_secs;
unsigned int bench_sink;
unsigned int bench_mark;
unsigned char bench_mem[64];
unsigned int cmp_kops;
unsigned int cmp_mips_tenths;
unsigned char bench_prev_speed;
unsigned char bench_prev_vsync;
unsigned char tile_map[TILE_MAP_W * TILE_MAP_H];
int tile_ready;
int mouse_last_char;
int network_attempted;
int network_joined;
int network_tx_ok;
int network_x;
int network_y;
int network_px[NET_MAX_PLAYERS];
int network_py[NET_MAX_PLAYERS];
unsigned char network_message[NET_MAX_MSG];
unsigned char network_colors[NET_MAX_PLAYERS] = {RED, GREEN, BLUE, YELLOW, CYAN, MAGENTA, ORANGE, WHITE};
int ship_x[SHIP_VERTICES] = {
    0, -13, -58, -28, -18, 0, 18, 28, 58, 13,
    0, -13, -58, -28, -18, 0, 18, 28, 58, 13,
    0, 0, -8, 8, 6, -6, 0};
int ship_y[SHIP_VERTICES] = {
    -52, -12, 16, 9, 38, 29, 38, 9, 16, -12,
    -52, -12, 16, 9, 38, 29, 38, 9, 16, -12,
    0, 0, -12, -12, 5, 5, -7};
int ship_z[SHIP_VERTICES] = {
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    -8, -8, -8, -8, -8, -8, -8, -8, -8, -8,
    8, -8, 8, 8, 8, 8, 22};
unsigned char ship_fa[SHIP_FACES];
unsigned char ship_fb[SHIP_FACES];
unsigned char ship_fc[SHIP_FACES];
int ship_scene[GPU3D_HEADER_WORDS +
               SHIP_VERTICES * GPU3D_VERTEX_WORDS +
               SHIP_FACES * GPU3D_FACE_WORDS];
int ship_face_count;
int stress_active;
int stress_sprite_count;
int stress_fps;
int stress_slow_mode;
unsigned int stress_last_ticks;
unsigned int stress_last_second;
unsigned char stress_previous_speed;
unsigned char stress_previous_vsync;

unsigned int host_seconds(void);

int page;
int frame;
unsigned int keys;
unsigned int prev_keys;
unsigned int pressed;
char nb[8];

int sinv(int a)
{
    return sintab[a & 31];
}

int cosv(int a)
{
    return sintab[(a + 8) & 31];
}

int ship_sinv(int phase)
{
    int index;
    int fraction;
    int next;
    index = (phase >> 2) & 31;
    fraction = phase & 3;
    next = (index + 1) & 31;
    return (sintab[index] * (4 - fraction) +
            sintab[next] * fraction) / 4;
}

int ship_cosv(int phase)
{
    return ship_sinv(phase + 32);
}

int pick(int cond, int a, int b)
{
    if (cond)
    {
        return a;
    }
    return b;
}

char *bit_text(int cond)
{
    if (cond)
    {
        return "1";
    }
    return "0";
}

void num_text(unsigned int value, char *out)
{
    char tmp[6];
    int n;
    int i;
    n = 0;
    if (value == 0)
    {
        tmp[0] = '0';
        n = 1;
    }
    while (value > 0)
    {
        tmp[n] = '0' + (value % 10);
        value = value / 10;
        n++;
    }
    for (i = 0; i < n; i++)
    {
        out[i] = tmp[n - 1 - i];
    }
    out[n] = 0;
}

void pnum(int x, int y, unsigned int v, unsigned char c)
{
    num_text(v, nb);
    gfx_text(x, y, nb, c);
}

void pad2(int x, int y, unsigned int v, unsigned char c)
{
    nb[0] = '0' + v / 10;
    nb[1] = '0' + v % 10;
    nb[2] = 0;
    gfx_text(x, y, nb, c);
}

void hex_text(int x, int y, unsigned int v, int digits, unsigned char c)
{
    int i;
    int d;
    for (i = 0; i < digits; i++)
    {
        d = (v >> ((digits - 1 - i) << 2)) & 15;
        if (d < 10)
        {
            nb[i] = '0' + d;
        }
        else
        {
            nb[i] = 'A' + d - 10;
        }
    }
    nb[digits] = 0;
    gfx_text(x, y, nb, c);
}

int text_len(char *s)
{
    int n;
    n = 0;
    while (s[n])
    {
        n++;
    }
    return n;
}

void ctext(int y, char *s, unsigned char c)
{
    gfx_text((SCREEN_W - text_len(s) * 6) >> 1, y, s, c);
}

void panel(int x, int y, int w, int h, unsigned char c)
{
    gfx_line(x, y, x + w, y, c);
    gfx_line(x + w, y, x + w, y + h, c);
    gfx_line(x + w, y + h, x, y + h, c);
    gfx_line(x, y + h, x, y, c);
}

void chrome(char *title)
{
    int i;
    gfx_rect(0, 0, 320, 14, BLUE);
    gfx_text(6, 3, title, WHITE);
    gfx_text(278, 3, "SC-16", YELLOW);
    gfx_line(0, 14, 319, 14, CYAN);
    gfx_line(0, 226, 319, 226, DARK_GRAY);
    gfx_text(6, 231, "< > PAGE", GRAY);
    gfx_text(96, 231, "UP", DARK_GRAY);
    pnum(114, 231, gfx_ticks() / 60, GRAY);
    gfx_text(150, 231, "S", DARK_GRAY);
    for (i = 0; i < PAGES; i++)
    {
        if (i == page)
        {
            gfx_rect(212 + i * 8, 232, 7, 6, YELLOW);
        }
        else
        {
            gfx_rect(212 + i * 8, 232, 7, 6, DARK_GRAY);
        }
    }
}

// ---------------------------------------------------------------- page 1
void page_title(void)
{
    int g;
    int r;
    int c;
    int x;
    int y;
    int i;
    unsigned char b;
    for (i = 0; i < 7; i++)
    {
        y = 22 + i * 3 + ((sinv(frame / 2 + i * 2) * 3) / 100) * 2;
        gfx_rect(0, y, 320, 2, rainbow[(i + frame / 6) % 7]);
        y = 200 + i * 3 + ((sinv(frame / 2 + i * 2 + 16) * 3) / 100) * 2;
        gfx_rect(0, y, 320, 2, rainbow[(i + 7 - (frame / 6) % 7) % 7]);
    }
    gfx_rect(0, 40, 320, 100, BLACK);
    for (g = 0; g < 5; g++)
    {
        for (r = 0; r < 7; r++)
        {
            b = logo[g * 7 + r];
            for (c = 0; c < 5; c++)
            {
                if (b & bit5[c])
                {
                    x = 73 + g * 36 + c * 6;
                    y = 48 + r * 6 + (sinv(frame / 2 + g * 3 + c) * 4) / 100;
                    gfx_rect(x, y, 5, 5, rainbow[(r + g + frame / 5) % 7]);
                }
            }
        }
    }
    ctext(100, "A COMPLETE 8-REGISTER COMPUTER", WHITE);
    ctext(112, "CPU - ASSEMBLER - C COMPILER - GRAPHICS", CYAN);
    ctext(124, "EVERYTHING YOU SEE RUNS ON THE SC-16", YELLOW);
    gfx_text(112, 150, "CLOCK", GRAY);
    pad2(154, 150, gfx_rtc(2), WHITE);
    gfx_text(166, 150, ":", pick((frame >> 4) & 1, WHITE, DARK_GRAY));
    pad2(172, 150, gfx_rtc(1), WHITE);
    gfx_text(184, 150, ":", pick((frame >> 4) & 1, WHITE, DARK_GRAY));
    pad2(190, 150, gfx_rtc(0), WHITE);
    if ((frame >> 5) & 1)
    {
        ctext(172, "PRESS RIGHT ARROW TO BEGIN THE TOUR", GREEN);
    }
}

// ---------------------------------------------------------------- page 2
void group(int x, int y, char *name, char *ops, unsigned char c)
{
    gfx_rect(x, y, 3, 16, c);
    gfx_text(x + 7, y, name, c);
    gfx_text(x + 7, y + 9, ops, GRAY);
}

void page_cpu(void)
{
    int i;
    int x;
    int y;
    gfx_text(8, 20, "ARCHITECTURE", YELLOW);
    gfx_text(8, 32, "DATA", GRAY);
    gfx_text(56, 32, "16-BIT REGISTERS, BIG-ENDIAN", WHITE);
    gfx_text(8, 42, "ADDRESS", GRAY);
    gfx_text(56, 42, "19-BIT = 512 KIB, BYTE ADDRESSED", WHITE);
    gfx_text(8, 52, "INSTR", GRAY);
    gfx_text(56, 52, "FIXED 16-BIT WORD (FAR MEM 6B, BRANCH 4B)", WHITE);
    gfx_text(8, 62, "STACK", GRAY);
    gfx_text(56, 62, "GROWS DOWN FROM 0x2FFFF", WHITE);
    gfx_text(8, 72, "FLAGS", GRAY);
    gfx_text(56, 72, "Z (ZERO)  C (CARRY/BORROW)", WHITE);
    gfx_text(8, 82, "IRQ", GRAY);
    gfx_text(56, 82, "KEYBOARD AND RTC, VECTORS 2 AND 4", WHITE);

    gfx_text(8, 98, "REGISTERS", YELLOW);
    for (i = 0; i < 8; i++)
    {
        x = 8 + i * 38;
        panel(x, 110, 34, 16, GREEN);
        gfx_text(x + 3, 114, "R", GREEN);
        pnum(x + 9, 114, i, GREEN);
        hex_text(x + 16, 114, (frame * (i + 1)) & 255, 2, DARK_GRAY);
    }
    panel(8, 130, 70, 14, CYAN);
    gfx_text(12, 133, "PC", CYAN);
    hex_text(30, 133, 0x4800 + ((frame << 1) & 0xFE), 4, WHITE);
    panel(84, 130, 70, 14, MAGENTA);
    gfx_text(88, 133, "SP", MAGENTA);
    hex_text(106, 133, 0xFFFF - ((frame << 1) & 0xFE), 4, WHITE);
    panel(160, 130, 30, 14, ORANGE);
    gfx_text(164, 133, "Z", ORANGE);
    gfx_text(176, 133, bit_text((frame >> 4) & 1), WHITE);
    panel(196, 130, 30, 14, ORANGE);
    gfx_text(200, 133, "C", ORANGE);
    gfx_text(212, 133, bit_text((frame >> 5) & 1), WHITE);

    gfx_text(8, 152, "INSTRUCTION WORD", YELLOW);
    gfx_rect(8, 164, 50, 14, RED);
    gfx_rect(58, 164, 130, 14, BLUE);
    gfx_text(12, 167, "OPCODE", WHITE);
    gfx_text(62, 167, "REG / IMM / ADDR", WHITE);
    gfx_text(8, 180, "BIT 15", DARK_GRAY);
    gfx_text(164, 180, "BIT 0", DARK_GRAY);
    gfx_text(200, 167, "5-BIT OPCODE", GRAY);

    group(8, 190, "MOVE", "LDI LD ST LDW STW MOV", GREEN);
    group(166, 190, "ALU", "ADD SUB INC DEC LOGIC SHR", YELLOW);
    group(8, 208, "FLOW", "JMP JZ BZ BC CALL RET +X", CYAN);
    group(166, 208, "STACK/IRQ", "PUSH POP EI DI RTI ADJSP", MAGENTA);
}

// ---------------------------------------------------------------- page 3
void seg(int a, int b, unsigned char c)
{
    gfx_rect(32 + a, 40, b - a, 22, c);
}

void maprow(int y, unsigned char c, char *range, char *name, char *size)
{
    gfx_rect(8, y, 6, 6, c);
    gfx_text(20, y - 1, range, WHITE);
    gfx_text(104, y - 1, name, c);
    gfx_text(236, y - 1, size, GRAY);
}

void page_memory(void)
{
    gfx_text(8, 20, "MEMORY MAP - 512 KIB", YELLOW);
    seg(0, 128, GREEN);
    seg(128, 160, ORANGE);
    seg(160, 198, YELLOW);
    seg(134, 136, MAGENTA);
    seg(198, 236, CYAN);
    seg(236, 256, DARK_GRAY);
    panel(31, 39, 257, 23, WHITE);
    gfx_text(26, 66, "0", GRAY);
    gfx_text(266, 66, "7FFFF", GRAY);
    maprow(86, GREEN, "00000-3FFFF", "FLASH / PROGRAM", "256 KIB");
    maprow(98, ORANGE, "40000-4FFFF", "LOW RAM", "64 KIB");
    maprow(110, YELLOW, "50000-757FF", "VRAM 640X480X4", "150 KIB");
    maprow(122, MAGENTA, "43000-43036", "KEYS RTC GPU LAYERS", "I/O");
    maprow(134, CYAN, "GPU INTERNAL", "TILES SPRITES 3D HUD", "PRIVATE");
    maprow(146, DARK_GRAY, "75800-7FFFF", "FREE", "42 KIB");
    gfx_text(8, 166, "C PROGRAM LAYOUT", YELLOW);
    gfx_text(8, 178, "0000 ENTRY STUB   0400 LIB_GFX   0700 LIB_TEXT", GRAY);
    gfx_text(8, 188, "1000 LIB_CGFX     4400 FONT 5X7  4800 CODE", GRAY);
    gfx_text(8, 198, "80000 GLOBALS    STACK 3FFFFFFF DOWN", GRAY);
    gfx_text(8, 212, "SPEED 43003  KEYS 43004  MOUSE 43008  RTC 43010", CYAN);
    gfx_rect(32 + ((frame * 3) % 255), 36, 3, 3, WHITE);
}

// ---------------------------------------------------------------- page 4
void page_graphics(void)
{
    int i;
    int x;
    int y;
    int a;
    gfx_text(8, 20, "320X240 - 256 COLOR PALETTE - DOUBLE BUFFERED", YELLOW);
    for (i = 0; i < 12; i++)
    {
        gfx_rect(8 + i * 14, 32, 12, 12, i);
    }
    for (i = 0; i < 16; i++)
    {
        gfx_rect(8 + i * 8, 48, 8, 6, 128 + i * 8);
    }
    gfx_text(190, 34, "12 NAMED COLORS", GRAY);
    gfx_text(190, 46, "GRAYSCALE RAMP", GRAY);

    panel(8, 62, 96, 70, DARK_GRAY);
    gfx_text(12, 65, "LINES", CYAN);
    a = frame / 2;
    for (i = 0; i < 16; i++)
    {
        gfx_line(56, 100, 56 + (cosv(i * 2 + a) * 22) / 100 * 2, 100 + (sinv(i * 2 + a) * 22) / 100, rainbow[i % 7]);
    }
    panel(112, 62, 96, 70, DARK_GRAY);
    gfx_text(116, 65, "RECTANGLES", CYAN);
    for (i = 0; i < 6; i++)
    {
        gfx_rect(122 + i * 12, 82 + ((sinv(frame / 2 + i * 3) * 16) / 100), 10, 22, rainbow[i]);
    }
    panel(216, 62, 96, 70, DARK_GRAY);
    gfx_text(220, 65, "POLYGONS", CYAN);
    gfx_poly(240, 106, 3, tri, YELLOW);
    gfx_poly(284, 104, 5, star5, MAGENTA);

    panel(8, 140, 96, 78, DARK_GRAY);
    gfx_text(12, 143, "SPRITES", CYAN);
    for (i = 0; i < 4; i++)
    {
        gfx_sprite(16 + i * 20, 160 + ((i & 1) << 3) + ((sinv(frame / 2 + i * 4) * 4) / 100), 8, 8, smiley);
    }
    gfx_text(12, 190, "PALETTE BYTES,", GRAY);
    gfx_text(12, 200, "0 = TRANSPARENT", GRAY);
    panel(112, 140, 96, 78, DARK_GRAY);
    gfx_text(116, 143, "1-BIT BITMAPS", CYAN);
    for (i = 0; i < 7; i++)
    {
        gfx_bitmap(120 + i * 12, 164, 8, 8, ball, rainbow[(i + frame / 8) % 7]);
    }
    gfx_text(116, 190, "ANY COLOR, MSB", GRAY);
    gfx_text(116, 200, "FIRST, CLIPPED", GRAY);
    panel(216, 140, 96, 78, DARK_GRAY);
    gfx_text(220, 143, "TEXT", CYAN);
    gfx_text(220, 160, "5X7 FONT", WHITE);
    gfx_text(220, 170, "6X8 CELLS", YELLOW);
    gfx_text(220, 180, "53 X 30 CHARS", GREEN);
    gfx_text(220, 190, "OEAAEA: öäå", MAGENTA);
    gfx_text(220, 204, "22 GPU COMMANDS", GRAY);
}

// ---------------------------------------------------------------- page 5
void page_animation(void)
{
    int i;
    int a;
    int b;
    int x;
    int y;
    int z;
    int x1;
    int z1;
    int y1;
    int z2;
    int s;
    for (i = 0; i < 40; i++)
    {
        starx[i] = starx[i] - (1 + (i % 3));
        if (starx[i] < 0)
        {
            starx[i] = 319;
            stary[i] = 18 + gfx_random() % 205;
        }
        gfx_pixel(starx[i], stary[i], pick(i % 3 == 0, WHITE, pick(i % 3 == 1, GRAY, DARK_GRAY)));
    }
    a = frame / 2;
    b = frame / 3;
    for (i = 0; i < 8; i++)
    {
        x = cvx[i];
        y = cvy[i];
        z = cvz[i];
        x1 = (x * cosv(a) - z * sinv(a)) / 100;
        z1 = (x * sinv(a) + z * cosv(a)) / 100;
        y1 = (y * cosv(b) - z1 * sinv(b)) / 100;
        z2 = (y * sinv(b) + z1 * cosv(b)) / 100;
        s = 180 + z2;
        cpx[i] = 160 + (x1 * 150) / s;
        cpy[i] = 118 + (y1 * 150) / s;
    }
    for (i = 0; i < 12; i++)
    {
        gfx_line(cpx[cea[i]], cpy[cea[i]], cpx[ceb[i]], cpy[ceb[i]], rainbow[i % 7]);
    }
    for (i = 0; i < 3; i++)
    {
        ballx[i] = ballx[i] + balldx[i];
        bally[i] = bally[i] + balldy[i];
        if (ballx[i] < 2 || ballx[i] > 310)
        {
            balldx[i] = -balldx[i];
            gfx_sound(1, 300 + i * 200, 3, WAVE_TRIANGLE, 30);
        }
        if (bally[i] < 20 || bally[i] > 214)
        {
            balldy[i] = -balldy[i];
            gfx_sound(1, 300 + i * 200, 3, WAVE_TRIANGLE, 30);
        }
        gfx_bitmap(ballx[i], bally[i], 8, 8, ball, rainbow[(i * 2 + 1)]);
    }
    gfx_text(8, 20, "SOFTWARE 3D + STARFIELD + SPRITES", YELLOW);
    gfx_text(8, 214, "ALL MATH IS 16-BIT INTEGER", GRAY);
}

// ---------------------------------------------------------------- page 6
void page_text(void)
{
    int i;
    int row;
    int col;
    int idx;
    unsigned char *glyph;
    unsigned char bits;
    char line[50];
    gfx_text(8, 20, "BITMAP FONT - ALL PRINTABLE ASCII + OEAA", YELLOW);
    for (row = 0; row < 3; row++)
    {
        for (i = 0; i < 32; i++)
        {
            idx = 32 + row * 32 + i;
            if (idx > 126)
            {
                line[i] = ' ';
            }
            else
            {
                line[i] = idx;
            }
        }
        line[32] = 0;
        gfx_text(8, 34 + row * 10, line, rainbow[(row * 2 + frame / 20) % 7]);
    }
    gfx_text(8, 68, "åäö ÅÄÖ  PACK MY BOX WITH FIVE DOZEN JUGS", WHITE);
    gfx_text(8, 84, "GLYPH ROM AT 0x4400, 8 BYTES PER GLYPH:", GRAY);
    gfx_text(8, 94, "BITS 7-3 = PIXELS, BIT 2 = BLANK COLUMN", GRAY);
    idx = (frame / 30) % 6;
    glyph = (unsigned char *)FONT_ROM + (logo_text[idx] - 32) * 8;
    for (row = 0; row < 7; row++)
    {
        bits = glyph[row];
        for (col = 0; col < 6; col++)
        {
            if (bits & bit8[col])
            {
                gfx_rect(16 + col * 10, 112 + row * 10, 9, 9, rainbow[(row + frame / 6) % 7]);
            }
            else
            {
                gfx_rect(16 + col * 10, 112 + row * 10, 9, 9, 10);
            }
        }
    }
    for (row = 0; row < 8; row++)
    {
        hex_text(120, 112 + row * 9, glyph[row], 2, pick(row < 7, GREEN, DARK_GRAY));
        for (col = 0; col < 8; col++)
        {
            if (glyph[row] & bit8[col])
            {
                gfx_rect(150 + col * 4, 113 + row * 9, 3, 6, GREEN);
            }
            else
            {
                gfx_rect(150 + col * 4, 113 + row * 9, 3, 6, DARK_GRAY);
            }
        }
    }
    gfx_text(200, 112, "WRITTEN TO", GRAY);
    gfx_text(200, 122, "THE GPU TEXT", GRAY);
    gfx_text(200, 132, "COMMAND (7)", GRAY);
    gfx_text(200, 152, "NUL-TERMINATED", WHITE);
    gfx_text(200, 162, "UTF-8, NEWLINE", WHITE);
    gfx_text(200, 172, "AND COLOR", WHITE);
    gfx_text(200, 192, "TEXT LIBRARY ALSO:", CYAN);
    gfx_text(200, 202, "CURSOR + INPUT", CYAN);
}

// ---------------------------------------------------------------- page 7
void page_devices(void)
{
    int i;
    int x;
    int y;
    unsigned int r;
    unsigned int m;
    unsigned int hmax;
    gfx_text(8, 20, "REAL-TIME CLOCK 0x43010", YELLOW);
    panel(8, 32, 150, 40, DARK_GRAY);
    pad2(20, 40, gfx_rtc(2), WHITE);
    gfx_text(32, 40, ":", GRAY);
    pad2(38, 40, gfx_rtc(1), WHITE);
    gfx_text(50, 40, ":", GRAY);
    pad2(56, 40, gfx_rtc(0), WHITE);
    gfx_text(88, 40, "HH:MM:SS", GRAY);
    gfx_text(16, 56, "REGS", GRAY);
    hex_text(52, 56, gfx_rtc(0), 2, GREEN);
    hex_text(70, 56, gfx_rtc(1), 2, GREEN);
    hex_text(88, 56, gfx_rtc(2), 2, GREEN);
    gfx_text(112, 56, "S M H", DARK_GRAY);
    gfx_text(170, 20, "INTERNAL TIMER", YELLOW);
    panel(170, 32, 142, 40, DARK_GRAY);
    gfx_text(178, 38, "FRAMES", GRAY);
    pnum(226, 38, gfx_ticks(), WHITE);
    gfx_text(178, 50, "SECONDS", GRAY);
    pnum(226, 50, gfx_ticks() / 60, GREEN);
    gfx_text(178, 60, "IRQ4", GRAY);
    pnum(226, 60, gfx_irq_ticks(), CYAN);

    gfx_text(8, 80, "KEYBOARD 0x43004", YELLOW);
    panel(8, 92, 150, 38, DARK_GRAY);
    gfx_rect(16, 98, 22, 10, pick(keys & KEY_LEFT, GREEN, DARK_GRAY));
    gfx_rect(42, 98, 22, 10, pick(keys & KEY_RIGHT, GREEN, DARK_GRAY));
    gfx_rect(68, 98, 22, 10, pick(keys & KEY_UP, GREEN, DARK_GRAY));
    gfx_rect(94, 98, 22, 10, pick(keys & KEY_DOWN, GREEN, DARK_GRAY));
    gfx_rect(120, 98, 30, 10, pick(keys & KEY_FIRE, GREEN, DARK_GRAY));
    gfx_text(18, 112, "L    R    U    D    FIRE", GRAY);
    gfx_text(16, 121, "MASK", GRAY);
    for (i = 0; i < 6; i++)
    {
        gfx_text(52 + i * 8, 121, bit_text(keys & (32 >> i)), WHITE);
    }
    gfx_text(106, 121, "IRQ2", GRAY);
    pnum(134, 121, gfx_irq_keys(), CYAN);

    gfx_text(170, 80, "RANDOM - GPU CMD 9", YELLOW);
    panel(170, 92, 142, 38, DARK_GRAY);
    for (i = 0; i < 40; i++)
    {
        x = 172 + (gfx_random() % 136);
        y = 94 + (gfx_random() % 34);
        gfx_rect(x, y, 2, 2, 2 + gfx_random() % 7);
    }

    gfx_text(8, 138, "DISTRIBUTION OF 16 BUCKETS (UNIFORM = FLAT)", YELLOW);
    for (i = 0; i < 40; i++)
    {
        r = gfx_random() >> 4;
        hist[r] = hist[r] + 1;
        hist_total = hist_total + 1;
    }
    if (hist_total >= 16000)
    {
        for (i = 0; i < 16; i++)
        {
            hist[i] = hist[i] / 2;
        }
        hist_total = hist_total / 2;
    }
    m = hist_total / 16 + 1;
    panel(8, 150, 304, 62, DARK_GRAY);
    for (i = 0; i < 16; i++)
    {
        y = (hist[i] * 30) / m;
        if (y > 58)
        {
            y = 58;
        }
        gfx_rect(16 + i * 18, 210 - y, 14, y, rainbow[i % 7]);
    }
    gfx_text(8, 216, "SAMPLES", GRAY);
    pnum(56, 216, hist_total, WHITE);
}

// ---------------------------------------------------------------- page 10
void page_mouse_input(void)
{
    int mx;
    int my;
    int buttons;
    int events;
    int wheel;
    int ch;
    mx = gfx_mouse_x();
    my = gfx_mouse_y();
    buttons = gfx_mouse_buttons();
    events = gfx_mouse_events();
    wheel = gfx_mouse_wheel();
    ch = gfx_getchar();
    if (ch > 0)
    {
        mouse_last_char = ch;
    }

    gfx_text(8, 20, "MOUSE, EXTENDED KEYS AND TEXT INPUT", YELLOW);
    gfx_text(8, 38, "POSITION", GRAY);
    gfx_text(76, 38, "X", CYAN);
    pnum(88, 38, mx, WHITE);
    gfx_text(130, 38, "Y", CYAN);
    pnum(142, 38, my, WHITE);
    gfx_text(8, 52, "BUTTONS", GRAY);
    gfx_text(76, 52, "LEFT", buttons & MOUSE_LEFT ? GREEN : DARK_GRAY);
    gfx_text(124, 52, "RIGHT", buttons & MOUSE_RIGHT ? GREEN : DARK_GRAY);
    gfx_text(8, 66, "EVENTS", GRAY);
    hex_text(76, 66, events, 2, WHITE);
    gfx_text(104, 66, "DOWN / DOUBLE / UP", DARK_GRAY);
    gfx_text(8, 80, "WHEEL", GRAY);
    gfx_text(76, 80, wheel < 0 ? "-" : "+", CYAN);
    pnum(88, 80, wheel < 0 ? -wheel : wheel, WHITE);
    gfx_text(8, 104, "EXT KEYS (Q/E ROLL, X THRUST, Z REVERSE)", YELLOW);
    hex_text(8, 118, gfx_keys_ext(), 2, WHITE);
    gfx_text(32, 118, "BITS 0-3", GRAY);
    gfx_text(8, 146, "TYPE A KEY - LAST INPUT BYTE:", YELLOW);
    if (mouse_last_char > 0)
    {
        hex_text(190, 146, mouse_last_char, 2, GREEN);
        if (mouse_last_char >= 32 && mouse_last_char <= 126)
        {
            nb[0] = mouse_last_char;
            nb[1] = 0;
            gfx_text(208, 146, nb, WHITE);
        }
        else if (mouse_last_char == 13)
        {
            gfx_text(208, 146, "ENTER", WHITE);
        }
        else if (mouse_last_char == 8)
        {
            gfx_text(208, 146, "BACKSPACE", WHITE);
        }
    }
    else
    {
        gfx_text(190, 146, "--", DARK_GRAY);
    }
    gfx_text(8, 184, "LEFT/RIGHT CLICK, DOUBLE-CLICK AND WHEEL", GRAY);
    gfx_text(8, 198, "MOUSE COORDINATES ARE SCREEN PIXELS", GRAY);
    gfx_line(mx - 5, my, mx + 5, my, WHITE);
    gfx_line(mx, my - 5, mx, my + 5, WHITE);
}

// ---------------------------------------------------------------- page 11
void page_tiles(void)
{
    int x;
    int y;
    int tile;
    if (!tile_ready)
    {
        for (y = 0; y < TILE_MAP_H; y++)
        {
            for (x = 0; x < TILE_MAP_W; x++)
            {
                if (y >= 16)
                {
                    tile = ((x + y) % 4 == 0) ? GREEN : DARK_GRAY;
                }
                else if ((x * 3 + y * 5) % 13 == 0)
                {
                    tile = CYAN;
                }
                else if ((x + y) % 3 == 0)
                {
                    tile = BLUE;
                }
                else
                {
                    tile = 25;
                }
                tile_map[y * TILE_MAP_W + x] = tile;
            }
        }
        gfx_tilemap(tile_map, TILE_MAP_W, TILE_MAP_H, 64, 32);
        gfx_sprite_upload(0, 8, 8, smiley);
        tile_ready = 1;
    }
    else
    {
        gfx_tile_camera(64, 32);
    }
    gfx_sprite_move(0, 150 + sinv(frame / 2) / 4,
                    118 + cosv(frame / 3) / 4, SPRITE_VISIBLE);
    gfx_layer_select(LAYER_HUD);
    gfx_clear(BLACK);
    gfx_text(8, 6, "TILE + SPRITE GPU ENGINES", YELLOW);
    gfx_text(8, 19, "TILEMAP AND SPRITE DATA STAY IN GPU MEMORY", WHITE);
    gfx_text(8, 214, "MAP 40X20 TILES - CAMERA (64,32)", CYAN);
    gfx_layer_select(LAYER_BACKGROUND);
}

// ---------------------------------------------------------------- page 12
void network_receive(void)
{
    int n;
    int sender;
    n = net_recv(network_message);
    while (n > 0)
    {
        sender = net_sender();
        if (n == 3 && sender >= 0 && sender < NET_MAX_PLAYERS)
        {
            network_px[sender] = (network_message[0] << 8) | network_message[1];
            network_py[sender] = network_message[2];
        }
        n = net_recv(network_message);
    }
}

void page_network(void)
{
    int i;
    int me;
    int keys_now;
    gfx_text(8, 20, "LOCAL NETWORK - UDP BROADCAST PEERS", YELLOW);
    if (!network_joined)
    {
        gfx_text(8, 42, "COULD NOT JOIN NETWORK GROUP", RED);
        gfx_text(8, 62, "OPEN THE SHOWCASE ON ANOTHER SC-16", GRAY);
        return;
    }
    if (net_full())
    {
        gfx_text(8, 42, "GROUP FULL - NO PLAYER SLOT AVAILABLE", RED);
        return;
    }
    me = net_slot();
    gfx_text(8, 40, net_ready() ? "CONNECTED" : "ASSIGNING PLAYER SLOT...", net_ready() ? GREEN : ORANGE);
    gfx_text(8, 54, "PLAYERS", GRAY);
    pnum(68, 54, net_players(), WHITE);
    gfx_text(104, 54, "MY SLOT", GRAY);
    if (me >= 0)
    {
        pnum(158, 54, me, CYAN);
    }
    if (net_ready() && me >= 0)
    {
        keys_now = gfx_keys();
        if (keys_now & KEY_LEFT) network_x = network_x - 2;
        if (keys_now & KEY_RIGHT) network_x = network_x + 2;
        if (keys_now & KEY_UP) network_y = network_y - 2;
        if (keys_now & KEY_DOWN) network_y = network_y + 2;
        if (network_x < 8) network_x = 8;
        if (network_x > 304) network_x = 304;
        if (network_y < 82) network_y = 82;
        if (network_y > 210) network_y = 210;
        network_message[0] = network_x >> 8;
        network_message[1] = network_x & 255;
        network_message[2] = network_y;
        network_tx_ok = net_send(network_message, 3);
        network_receive();
        for (i = 0; i < NET_MAX_PLAYERS; i++)
        {
            if (net_active(i))
            {
                gfx_rect(204 + i * 13, 54, 9, 8, network_colors[i]);
                if (i == me)
                {
                    gfx_rect(network_x, network_y, 8, 8, network_colors[i]);
                }
                else
                {
                    gfx_rect(network_px[i], network_py[i], 8, 8, network_colors[i]);
                }
            }
            else
            {
                gfx_rect(204 + i * 13, 54, 9, 8, DARK_GRAY);
            }
        }
        gfx_text(8, 70, "ARROWS MOVE YOUR SQUARE; OTHER INSTANCES ARE SHARED", GRAY);
        gfx_text(8, 222, network_tx_ok ? "POSITION BROADCAST OK" : "WAITING TO BROADCAST", network_tx_ok ? GREEN : ORANGE);
    }
    else
    {
        for (i = 0; i < NET_MAX_PLAYERS; i++)
        {
            gfx_rect(204 + i * 13, 54, 9, 8, net_active(i) ? network_colors[i] : DARK_GRAY);
        }
        gfx_text(8, 70, "WAITING FOR NETWORK SLOT ASSIGNMENT", GRAY);
    }
}

// ---------------------------------------------------------------- page 13
void ship_add_face(int a, int b, int c)
{
    if (ship_face_count < SHIP_FACES)
    {
        ship_fa[ship_face_count] = a;
        ship_fb[ship_face_count] = b;
        ship_fc[ship_face_count] = c;
        ship_face_count++;
    }
}

void ship_init(void)
{
    int i;
    int next;
    int offset;
    ship_face_count = 0;
    for (i = 0; i < 10; i++)
    {
        next = (i + 1) % 10;
        ship_add_face(i, 20, next);
        ship_add_face(i + 10, next + 10, 21);
        ship_add_face(i, next, next + 10);
        ship_add_face(i, next + 10, i + 10);
    }
    ship_add_face(22, 23, 26);
    ship_add_face(23, 24, 26);
    ship_add_face(24, 25, 26);
    ship_add_face(25, 22, 26);
    ship_scene[0] = SHIP_VERTICES;
    ship_scene[1] = ship_face_count;
    ship_scene[8] = GPU3D_FIXED(100);
    ship_scene[9] = GPU3D_FILL | GPU3D_WIREFRAME;
    offset = GPU3D_HEADER_WORDS;
    for (i = 0; i < SHIP_VERTICES; i++)
    {
        ship_scene[offset++] = GPU3D_FIXED(ship_x[i]);
        ship_scene[offset++] = GPU3D_FIXED(ship_y[i]);
        ship_scene[offset++] = GPU3D_FIXED(ship_z[i]);
    }
    for (i = 0; i < ship_face_count; i++)
    {
        ship_scene[offset++] = ship_fa[i];
        ship_scene[offset++] = ship_fb[i];
        ship_scene[offset++] = ship_fc[i];
        ship_scene[offset++] = i >= 40 ? ORANGE : (i & 1 ? CYAN : BLUE);
    }
}

void page_ship(void)
{
    int i;
    int flight_phase;
    int flight_x;
    int flight_y;
    int flight_depth;
    flight_phase = frame / 2;
    flight_x = (ship_sinv(flight_phase) * 70) / 100;
    flight_y = (ship_sinv(flight_phase + 32) * 32) / 100;
    flight_depth = (ship_sinv(flight_phase + 16) * 28) / 100;
    ship_scene[2] = (flight_phase * 8 + 12) & 255;
    ship_scene[3] = (flight_phase * 8 + 32) & 255;
    ship_scene[4] = (256 - flight_phase * 3) & 255;
    ship_scene[5] = GPU3D_FIXED(flight_x);
    ship_scene[6] = GPU3D_FIXED(flight_y);
    ship_scene[7] = GPU3D_FIXED(96 + flight_depth / 2);
    for (i = 0; i < 56; i++)
    {
        int px;
        int py;
        px = (i * 47 + (frame & 255) * 3) % 320;
        py = 24 + (i * 37) % 188;
        gfx_pixel(px, py, (i % 4 == 0) ? WHITE : DARK_GRAY);
    }
    gfx3d_draw(ship_scene);
    gfx_layer_select(LAYER_HUD);
    gfx_text(8, 20, "GPU 3D FLIGHT - PAINTER-SORTED TRIANGLES", YELLOW);
    gfx_text(8, 204, "FIXED 8.8 TRANSFORMS + HIDDEN-LINE REMOVAL", WHITE);
    gfx_text(8, 216, "ROTATION, PERSPECTIVE AND FLAT SHADING IN GPU", GRAY);
}

// ---------------------------------------------------------------- page 14
void page_sprite_load(void)
{
    int i;
    int x;
    int y;
    unsigned int now;
    unsigned int elapsed;
    unsigned int ticks;
    int seconds;

    now = host_seconds();
    elapsed = (now + 3600 - stress_last_second) % 3600;
    if (elapsed > 0)
    {
        ticks = gfx_ticks() - stress_last_ticks;
        stress_fps = ticks / elapsed;
        if (stress_fps < 30)
        {
            stress_slow_mode = 1;
        }
        else if (!stress_slow_mode || stress_fps > 30)
        {
            seconds = elapsed * (stress_slow_mode ? 1 : 10);
            stress_sprite_count = stress_sprite_count + seconds;
        }
        stress_last_ticks = gfx_ticks();
        stress_last_second = now;
    }

    gfx_text(8, 20, "30 FPS SPRITE LOAD TEST", YELLOW);
    gfx_text(8, 38, "SPRITES", GRAY);
    pnum(68, 38, stress_sprite_count, WHITE);
    gfx_text(142, 38, "FRAME RATE", GRAY);
    pnum(244, 38, stress_fps, CYAN);
    gfx_text(280, 38, "FPS", CYAN);
    for (i = 0; i < stress_sprite_count; i++)
    {
        int phase;
        phase = frame + i * 2;
        x = 8 + ((i * 8 + frame + (ship_cosv(phase) * 12) / 100 + 304) % 304);
        y = 120 + (ship_sinv(phase) * 58) / 100;
        gfx_sprite(x, y, 8, 8, smiley);
    }
    if (stress_slow_mode && stress_fps <= 30)
    {
        gfx_text(8, 216, "LOW FPS - WAITING TO RECOVER", YELLOW);
    }
    else if (stress_slow_mode)
    {
        gfx_text(8, 216, "ADDING ONE SPRITE PER SECOND", GREEN);
    }
    else
    {
        gfx_text(8, 216, "ADDING 10 SPRITES PER SECOND", GREEN);
    }
}

// ---------------------------------------------------------------- page 8
unsigned int fibo(int n)
{
    if (n < 2)
    {
        return n;
    }
    return fibo(n - 1) + fibo(n - 2);
}

void bench_block(int test, unsigned int blk)
{
    int i;
    unsigned int x;
    x = bench_sink;
    if (test == 0)
    {
        for (i = 0; i < 32; i++)
        {
            x = x + i;
        }
    }
    else if (test == 1)
    {
        for (i = 0; i < 8; i++)
        {
            x = x * i + 3;
        }
    }
    else if (test == 2)
    {
        for (i = 0; i < 4; i++)
        {
            x = (x + 9999) / (i + 3);
        }
    }
    else if (test == 3)
    {
        for (i = 0; i < 32; i++)
        {
            bench_mem[i] = i + blk;
        }
        for (i = 0; i < 32; i++)
        {
            x = x + bench_mem[i];
        }
    }
    else if (test == 4)
    {
        x = x + fibo(6);
    }
    else if (test == 5)
    {
        gfx_rect(blk & 127, 100, 10, 10, GREEN);
    }
    else
    {
        gfx_sprite(blk & 127, 120, 8, 8, smiley);
    }
    bench_sink = x;
}

int bench_ops(int test)
{
    if (test == 0)
    {
        return 32;
    }
    if (test == 1)
    {
        return 8;
    }
    if (test == 2)
    {
        return 4;
    }
    if (test == 3)
    {
        return 64;
    }
    if (test == 4)
    {
        return 25;
    }
    if (test == 5)
    {
        return 100;
    }
    return 64;
}

void bench_run(int test)
{
    unsigned int blocks;
    unsigned int acc;
    unsigned int k;
    unsigned int t;
    int ops;
    ops = bench_ops(test);
    blocks = 0;
    acc = 0;
    k = 0;
    t = gfx_ticks();
    while (gfx_ticks() == t)
    {
    }
    t = gfx_ticks();
    while (gfx_ticks() - t < DUR)
    {
        bench_block(test, blocks);
        blocks++;
        acc = acc + ops;
        if (acc >= 1000)
        {
            acc = acc - 1000;
            k++;
        }
    }
    bench_blocks[test] = blocks;
    bench_kops[test] = k * (60 / DUR) + (acc * (60 / DUR)) / 1000;
}

unsigned int host_seconds(void)
{
    return gfx_rtc(1) * 60 + gfx_rtc(0);
}

/* Unthrottled benchmark: the speed register is set to maximum, and the
   run is timed against the real-time clock (3 whole seconds). */
void bench_max(void)
{
    unsigned int blocks;
    unsigned int acc;
    unsigned int k;
    unsigned int s0;
    unsigned int s;
    int j;
    int ops;
    int secs;
    int last;
    unsigned int ksum;
    unsigned int remainder;
    ops = bench_ops(0);
    blocks = 0;
    acc = 0;
    k = 0;
    ksum = 0;
    remainder = 0;
    last = 0;
    bench_prev_speed = *((unsigned char *)0x43003);
    bench_prev_vsync = *((unsigned char *)0x43013);
    gfx_speed(SPEED_MAX);
    gfx_vsync(0);
    s0 = host_seconds();
    while (host_seconds() == s0)
    {
    }
    s0 = host_seconds();
    secs = 0;
    while (secs < CMP_SECS)
    {
        for (j = 0; j < 8; j++)
        {
            bench_block(0, blocks);
            blocks++;
            acc = acc + ops;
            if (acc >= 1000)
            {
                acc = acc - 1000;
                k++;
            }
        }
        s = host_seconds();
        secs = (s + 3600 - s0) % 3600;
        if (secs != last)
        {
            ksum = ksum + k / CMP_SECS;
            remainder = remainder + k % CMP_SECS;
            if (remainder >= CMP_SECS)
            {
                ksum = ksum + remainder / CMP_SECS;
                remainder = remainder % CMP_SECS;
            }
            k = 0;
            last = secs;
        }
    }
    gfx_speed(bench_prev_speed);
    gfx_vsync(bench_prev_vsync);
    cmp_kops = ksum;
}

unsigned int kops_to_mips_tenths(unsigned int kops)
{
    unsigned int scale;
    unsigned int whole;
    unsigned int remainder;
    scale = BENCH_ADDS_PER_BLOCK * 100;
    whole = (kops / scale) * BENCH_INSTRUCTIONS_PER_BLOCK;
    remainder = (kops % scale) * BENCH_INSTRUCTIONS_PER_BLOCK;
    return whole + (remainder + scale / 2) / scale;
}

char *bench_name(int test)
{
    if (test == 0)
    {
        return "INTEGER ADD LOOP";
    }
    if (test == 1)
    {
        return "16-BIT MULTIPLY";
    }
    if (test == 2)
    {
        return "16-BIT DIVIDE";
    }
    if (test == 3)
    {
        return "MEMORY READ+WRITE";
    }
    if (test == 4)
    {
        return "RECURSIVE CALLS";
    }
    if (test == 5)
    {
        return "GPU RECT FILL";
    }
    return "GPU SPRITE BLIT";
}

char *bench_unit(int test)
{
    if (test == 0)
    {
        return "K ITER/S";
    }
    if (test == 1)
    {
        return "K MUL/S";
    }
    if (test == 2)
    {
        return "K DIV/S";
    }
    if (test == 3)
    {
        return "K BYTES/S";
    }
    if (test == 4)
    {
        return "K CALLS/S";
    }
    if (test == 5)
    {
        return "K PIXELS/S";
    }
    return "K PIXELS/S";
}

void page_bench(void)
{
    int i;
    int w;
    unsigned int t;
    gfx_text(8, 20, "BENCHMARK - TIMED WITH THE SC-16'S OWN FRAME CLOCK", YELLOW);
    for (i = 0; i < NTEST; i++)
    {
        gfx_text(8, 36 + i * 20, bench_name(i), WHITE);
        if (bench_state == 0 || i >= bench_index)
        {
            if (bench_state == 1 && i == bench_index)
            {
                gfx_text(150, 36 + i * 20, "RUNNING...", pick((frame >> 3) & 1, YELLOW, ORANGE));
            }
            else
            {
                gfx_text(150, 36 + i * 20, "-", DARK_GRAY);
            }
        }
        else
        {
            w = bench_kops[i] / bench_scale[i];
            if (w > 100)
            {
                w = 100;
            }
            if (w < 1)
            {
                w = 1;
            }
            gfx_rect(150, 36 + i * 20, w, 6, rainbow[i]);
            pnum(150, 44 + i * 20 - 1, bench_kops[i], WHITE);
            gfx_text(150 + 6 * 7, 44 + i * 20 - 1, bench_unit(i), GRAY);
        }
    }
    gfx_line(8, 180, 311, 180, DARK_GRAY);
    if (bench_state == 2)
    {
        gfx_text(8, 186, "SC-16 MARK", CYAN);
        pnum(80, 186, bench_mark, YELLOW);
        gfx_text(8, 196, "FRAMES", GRAY);
        pnum(80, 196, bench_ticks, WHITE);
        gfx_text(8, 206, "HOST SEC", GRAY);
        pnum(80, 206, bench_host_secs, WHITE);
        gfx_text(150, 186, "EMULATION SPEED", GRAY);
        if (bench_host_secs > 0)
        {
            t = bench_ticks / bench_host_secs;
            pnum(150, 196, t, GREEN);
            gfx_text(150 + 6 * 3, 196, "FRAMES/S", GRAY);
            pnum(150, 206, (t * 100) / 60, GREEN);
            gfx_text(150 + 6 * 3, 206, "% OF REAL TIME", GRAY);
        }
        else
        {
            gfx_text(150, 196, "UNDER 1 SECOND (RTC TICK)", GRAY);
        }
    }
    else if (bench_state == 0)
    {
        if ((frame >> 4) & 1)
        {
            ctext(196, "PRESS ENTER TO RUN THE BENCHMARK", GREEN);
        }
    }
    else
    {
        ctext(196, "BENCHMARK RUNNING - PLEASE WAIT", ORANGE);
    }
}

char *mach_name(int i)
{
    if (i == 0)
    {
        return "INTEL 4004 1971";
    }
    if (i == 1)
    {
        return "IBM PC 8088 1981";
    }
    if (i == 2)
    {
        return "C64 6502 1982";
    }
    if (i == 3)
    {
        return "AMIGA 68000 1985";
    }
    if (i == SC16_ROW)
    {
        return "SC-16 (THIS)";
    }
    if (i == 5)
    {
        return "486DX2-66 1992";
    }
    if (i == 6)
    {
        return "CRAY-1 1976";
    }
    return "RASPBERRY PI A";
}

// Bar length on a log2 scale (11 px per doubling, 3 fractional steps)
int log_bar(unsigned int v)
{
    int b;
    b = 0;
    while ((v >> (b + 1)) > 0)
    {
        b++;
    }
    if (b < 2)
    {
        return 4 + v * 4;
    }
    return 12 + b * 11 + ((v >> (b - 2)) & 3) * 3;
}

void tenths_text(int x, int y, unsigned int v, unsigned char c)
{
    pnum(x, y, v / 10, c);
    x = x + text_len(nb) * 6;
    gfx_text(x, y, ".", c);
    pnum(x + 6, y, v % 10, c);
}

void page_compare(void)
{
    int i;
    int y;
    unsigned int sc;
    unsigned char c;
    sc = mach_tenths[SC16_ROW];
    if (cmp_state == 2)
    {
        sc = cmp_mips_tenths;
    }
    gfx_text(8, 20, "SPEED IN MIPS (MILLION INSTRUCTIONS/S)", YELLOW);
    for (i = 0; i < NMACH; i++)
    {
        y = 34 + i * 14;
        c = WHITE;
        if (i == SC16_ROW)
        {
            c = CYAN;
        }
        gfx_text(8, y, mach_name(i), c);
        if (i == SC16_ROW)
        {
            tenths_text(116, y, sc, c);
            gfx_rect(160, y, log_bar(sc), 8, c);
        }
        else
        {
            tenths_text(116, y, mach_tenths[i], GRAY);
            gfx_rect(160, y, log_bar(mach_tenths[i]), 8, rainbow[i % 7]);
        }
    }
    gfx_line(8, 150, 311, 150, DARK_GRAY);
    gfx_text(8, 156, "CRAY-1 IS", GRAY);
    gfx_text(8, 166, "RASPBERRY PI A IS", GRAY);
    gfx_text(8, 176, "486DX2-66 IS", GRAY);
    if (sc > 0)
    {
        pnum(116, 156, mach_tenths[6] / sc, GREEN);
        gfx_text(116 + text_len(nb) * 6 + 6, 156, "X FASTER", GRAY);
        pnum(116, 166, mach_tenths[7] / sc, GREEN);
        gfx_text(116 + text_len(nb) * 6 + 6, 166, "X FASTER", GRAY);
        pnum(116, 176, mach_tenths[5] / sc, GREEN);
        gfx_text(116 + text_len(nb) * 6 + 6, 176, "X FASTER", GRAY);
    }
    else
    {
        gfx_text(116, 156, "--", GRAY);
        gfx_text(116, 166, "--", GRAY);
        gfx_text(116, 176, "--", GRAY);
        gfx_text(134, 156, "NO RATE", DARK_GRAY);
        gfx_text(134, 166, "NO RATE", DARK_GRAY);
        gfx_text(134, 176, "NO RATE", DARK_GRAY);
    }
    gfx_text(8, 188, "LOG-SCALE BARS. OTHERS: APPROX. PUBLISHED", DARK_GRAY);
    gfx_text(8, 197, cmp_state == 2 ? "SC-16: 32-ADD LOOP MEASURED AT MAX SPEED" : "SC-16: APPROXIMATE UNTIL MEASURED", DARK_GRAY);
    if (cmp_state == 1)
    {
        ctext(210, "MEASURING AT MAX SPEED - 3 S", ORANGE);
    }
    else if (cmp_state == 2)
    {
        ctext(210, "MEASURED - ENTER TO REPEAT", GREEN);
    }
    else if ((frame >> 4) & 1)
    {
        ctext(210, "ESTIMATE - PRESS ENTER TO MEASURE", GREEN);
    }
}

void bench_step(void)
{
    int i;
    if (cmp_state == 1)
    {
        bench_max();
        cmp_mips_tenths = kops_to_mips_tenths(cmp_kops);
        cmp_state = 2;
        gfx_sound(2, 1320, 30, WAVE_TRIANGLE, 50);
    }
    if (bench_state == 1)
    {
        if (bench_index == 0)
        {
            bench_t0 = gfx_ticks();
            bench_host_start = host_seconds();
        }
        bench_run(bench_index);
        bench_index = bench_index + 1;
        if (bench_index >= NTEST)
        {
            bench_ticks = gfx_ticks() - bench_t0;
            bench_host_secs = (host_seconds() + 3600 - bench_host_start) % 3600;
            bench_mark = 0;
            for (i = 0; i < NTEST; i++)
            {
                bench_mark = bench_mark + bench_blocks[i] * (60 / DUR);
            }
            bench_state = 2;
            gfx_sound(2, 1320, 30, WAVE_TRIANGLE, 50);
        }
    }
}

int main(void)
{
    int i;
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_irq_init();
    for (i = 0; i < 40; i++)
    {
        starx[i] = gfx_random() + (gfx_random() & 63);
        stary[i] = 18 + gfx_random() % 205;
    }
    network_x = 160;
    network_y = 120;
    for (i = 0; i < NET_MAX_PLAYERS; i++)
    {
        network_px[i] = 20 + i * 24;
        network_py[i] = 120;
    }
    ship_init();
    page = 0;
    while (1)
    {
        keys = gfx_keys();
        pressed = keys & ~prev_keys;
        prev_keys = keys;
        if (bench_state != 1 && cmp_state != 1)
        {
            if (pressed & KEY_RIGHT)
            {
                page = (page + 1) % PAGES;
            }
            if (pressed & KEY_LEFT)
            {
                page = (page + PAGES - 1) % PAGES;
            }
            if (pressed & (KEY_LEFT | KEY_RIGHT))
            {
                gfx_sound(2, 400 + page * 70, 5, WAVE_SQUARE, 30);
            }
            if (page == 6 && (pressed & (KEY_UP | KEY_DOWN | KEY_FIRE)))
            {
                gfx_sound(3, 1000, 4, WAVE_TRIANGLE, 40);
            }
            if (page == 7 && (pressed & KEY_START))
            {
                bench_state = 1;
                bench_index = 0;
                gfx_sound(2, 880, 10, WAVE_SQUARE, 40);
            }
            if (page == 8 && (pressed & KEY_START))
            {
                cmp_state = 1;
                gfx_sound(2, 880, 10, WAVE_SQUARE, 40);
            }
        }
        if (page == STRESS_PAGE && !stress_active)
        {
            stress_previous_speed = *((unsigned char *)0x43003);
            stress_previous_vsync = *((unsigned char *)0x43013);
            gfx_speed(SPEED_MAX);
            gfx_vsync(0);
            stress_active = 1;
            stress_sprite_count = 1;
            stress_fps = 0;
            stress_slow_mode = 0;
            stress_last_ticks = gfx_ticks();
            stress_last_second = host_seconds();
        }
        else if (page != STRESS_PAGE && stress_active)
        {
            gfx_speed(stress_previous_speed);
            gfx_vsync(stress_previous_vsync);
            stress_active = 0;
        }
        if (page == NETWORK_PAGE && !network_attempted)
        {
            network_attempted = 1;
            network_joined = net_open("sc16showcase");
        }
        else if (page != NETWORK_PAGE && network_attempted)
        {
            if (network_joined)
            {
                net_close();
            }
            network_attempted = 0;
            network_joined = 0;
        }
        frame++;
        if (page == 10)
        {
            gfx_layer_visibility((1 << LAYER_BACKGROUND) |
                                 (1 << LAYER_TILES) |
                                 (1 << LAYER_SPRITES) |
                                 (1 << LAYER_HUD));
        }
        else if (page == 12)
        {
            gfx_layer_visibility((1 << LAYER_BACKGROUND) | (1 << LAYER_3D) |
                                 (1 << LAYER_HUD));
            gfx_layer_select(LAYER_HUD);
            gfx_clear(BLACK);
        }
        else
        {
            gfx_layer_visibility(1 << LAYER_BACKGROUND);
        }
        gfx_layer_select(LAYER_BACKGROUND);
        gfx_clear(BLACK);
        if (page == 0)
        {
            page_title();
            chrome("WELCOME");
        }
        else if (page == 1)
        {
            page_cpu();
            chrome("CPU ARCHITECTURE");
        }
        else if (page == 2)
        {
            page_memory();
            chrome("MEMORY MAP");
        }
        else if (page == 3)
        {
            page_graphics();
            chrome("GRAPHICS");
        }
        else if (page == 4)
        {
            page_animation();
            chrome("REAL-TIME ANIMATION");
        }
        else if (page == 5)
        {
            page_text();
            chrome("TEXT AND FONT");
        }
        else if (page == 6)
        {
            page_devices();
            chrome("DEVICES: RTC, KEYBOARD, RANDOM");
        }
        else if (page == 7)
        {
            page_bench();
            chrome("BENCHMARK");
        }
        else if (page == 8)
        {
            page_compare();
            chrome("SPEED COMPARISON");
        }
        else if (page == 9)
        {
            page_mouse_input();
            chrome("MOUSE AND INPUT");
        }
        else if (page == 10)
        {
            page_tiles();
            chrome("TILE MAP AND LAYERS");
        }
        else if (page == NETWORK_PAGE)
        {
            page_network();
            chrome("LAN NETWORKING");
        }
        else if (page == 12)
        {
            page_ship();
            chrome("3D SHIP FLIGHT");
        }
        else
        {
            page_sprite_load();
            chrome("SPRITE LOAD TEST");
        }
        if (page == 12)
            gfx_layer_select(LAYER_BACKGROUND);
        gfx_present();
        bench_step();
    }
    return 0;
}
