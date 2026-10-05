// SC-16 showcase: a tour of the machine, written in C for the SC-16 itself.
// Left/Right (or A/D) change page.  On the benchmark page press Enter to run it.
// Pages: 1 title, 2 CPU, 3 memory map, 4 graphics, 5 animation, 6 text/font,
//        7 devices (RTC, keyboard, random numbers), 8 benchmark.
// All timing uses the machine's own clocks: gfx_ticks() (frames, 60 per second
// of emulated time) and gfx_rtc() (the real-time clock registers).
#include "suomi_gfx.h"

#define PAGES 8
#define NTEST 7
#define DUR 30
#define FONT_ROM 0x4400

char sintab[32] = {0, 20, 38, 56, 71, 83, 92, 98, 100, 98, 92, 83, 71, 56, 38, 20,
                   0, -20, -38, -56, -71, -83, -92, -98, -100, -98, -92, -83, -71, -56, -38, -20};
unsigned char rainbow[7] = {RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, MAGENTA};
unsigned char logo[28] = {0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E,
                          0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E,
                          0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00,
                          0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E};
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
int bench_index;
unsigned int bench_t0;
unsigned int bench_ticks;
unsigned int bench_host_start;
unsigned int bench_host_secs;
unsigned int bench_sink;
unsigned int bench_mark;
unsigned char bench_mem[64];

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
            gfx_rect(236 + i * 10, 232, 8, 6, YELLOW);
        }
        else
        {
            gfx_rect(236 + i * 10, 232, 8, 6, DARK_GRAY);
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
    for (g = 0; g < 4; g++)
    {
        for (r = 0; r < 7; r++)
        {
            b = logo[g * 7 + r];
            for (c = 0; c < 5; c++)
            {
                if (b & bit5[c])
                {
                    x = 91 + g * 36 + c * 6;
                    y = 48 + r * 6 + ((sinv(frame / 2 + g * 3 + c) * 2) / 100);
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
    gfx_text(56, 52, "FIXED 16-BIT WORD (FAR OPS 6 BYTES)", WHITE);
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
    gfx_rect(32 + (a >> 7), 40, (b - a) >> 7, 22, c);
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
    seg(0, 1024, GREEN);
    seg(1024, 1536, ORANGE);
    seg(1536, 2112, YELLOW);
    seg(2112, 2176, MAGENTA);
    seg(2176, 2784, CYAN);
    gfx_rect(32 + 2784 / 128, 40, 2048 - 2784 / 128, 22, DARK_GRAY);
    panel(31, 39, 257, 23, WHITE);
    gfx_text(26, 66, "0", GRAY);
    gfx_text(266, 66, "7FFFF", GRAY);
    maprow(86, GREEN, "00000-1FFFF", "FLASH / PROGRAM", "128 KIB");
    maprow(98, ORANGE, "20000-2FFFF", "RAM AND STACK", "64 KIB");
    maprow(110, YELLOW, "30000-42BFF", "VRAM 320X240X8", "75 KIB");
    maprow(122, MAGENTA, "43000-4302E", "KEYBOARD RTC GPU REGS", "I/O");
    maprow(134, CYAN, "44000-56BFF", "GPU BACK BUFFER", "75 KIB");
    maprow(146, DARK_GRAY, "57000-7FFFF", "FREE", "164 KIB");
    gfx_text(8, 166, "C PROGRAM LAYOUT", YELLOW);
    gfx_text(8, 178, "0000 ENTRY STUB   0400 LIB_GFX   0700 LIB_TEXT", GRAY);
    gfx_text(8, 188, "1000 LIB_CGFX     4400 FONT 5X7  4800 CODE", GRAY);
    gfx_text(8, 198, "E800 GLOBALS      STACK 2FFFF DOWN", GRAY);
    gfx_text(8, 212, "KEYS 43004   RTC 43010-12   GPU 43020-2E", CYAN);
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
    gfx_text(220, 204, "12 GPU COMMANDS", GRAY);
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
    glyph = FONT_ROM + (logo_text[idx] - 32) * 8;
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

void bench_step(void)
{
    int i;
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
    gfx_irq_init();
    for (i = 0; i < 40; i++)
    {
        starx[i] = gfx_random() + (gfx_random() & 63);
        stary[i] = 18 + gfx_random() % 205;
    }
    page = 0;
    while (1)
    {
        keys = gfx_keys();
        pressed = keys & ~prev_keys;
        prev_keys = keys;
        if (bench_state != 1)
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
        }
        frame++;
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
        else
        {
            page_bench();
            chrome("BENCHMARK");
        }
        gfx_present();
        bench_step();
    }
    return 0;
}
