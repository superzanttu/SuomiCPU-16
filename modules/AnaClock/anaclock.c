// Seven clock styles driven by the emulator's local real-time clock.
// ICON
// ................
// .....######.....
// ...##......##...
// ..#..........#..
// .#............#.
// #......##......#
// #.....###......#
// #......##......#
// #......##......#
// #......##......#
// #......##......#
// #......##......#
// .#............#.
// ..#..........#..
// ...##......##...
// .....######.....
// Press Tab to cycle through five analog faces, Nixie tubes, and a flip clock.
#include "suomi_gfx.h"

#define CX 160
#define CY 106
#define FACE_R 82
#define FACE_COUNT 7

/* Sine values scaled by 100; each table step is 1/32 of a full turn. */
char sine[32] = {
    0, 20, 38, 56, 71, 83, 92, 98,
    100, 98, 92, 83, 71, 56, 38, 20,
    0, -20, -38, -56, -71, -83, -92, -98,
    -100, -98, -92, -83, -71, -56, -38, -20
};

/* Five-by-seven bitmaps scaled up for the digital clock digits. */
char digit_font[70] = {
    14, 17, 19, 21, 25, 17, 14,
    4, 12, 4, 4, 4, 4, 14,
    14, 17, 1, 2, 4, 8, 31,
    30, 1, 1, 14, 1, 1, 30,
    2, 6, 10, 18, 31, 2, 2,
    31, 16, 16, 30, 1, 1, 30,
    6, 8, 16, 30, 17, 17, 14,
    31, 1, 2, 4, 8, 8, 8,
    14, 17, 17, 14, 17, 17, 14,
    14, 17, 17, 15, 1, 2, 12
};

/* Draw a large 5x7 bitmap digit using filled pixels. */
void draw_large_digit(int x, int y, int value, unsigned char color)
{
    int row;
    int column;

    for (row = 0; row < 7; row++)
    {
        for (column = 0; column < 5; column++)
        {
            if (digit_font[value * 7 + row] & (16 >> column))
            {
                gfx_rect(x + column * 4, y + row * 6, 4, 5, color);
            }
        }
    }
}

/* Convert a fractional clock angle and radius to a point on the dial. */
void clock_point(int angle_units, int radius, int *x, int *y)
{
    int angle;
    int fraction;
    int x0;
    int y0;
    int x1;
    int y1;
    /* Angles use 60 subdivisions per sine-table step. */
    angle = angle_units / 60;
    fraction = angle_units % 60;
    x0 = (sine[angle & 31] * radius) / 100;
    y0 = (sine[(angle + 8) & 31] * radius) / 100;
    x1 = (sine[(angle + 1) & 31] * radius) / 100;
    y1 = (sine[(angle + 9) & 31] * radius) / 100;
    *x = CX + x0 + ((x1 - x0) * fraction) / 60;
    *y = CY - y0 - ((y1 - y0) * fraction) / 60;
}

/* Draw a hand at a fractional position between adjacent sine-table entries. */
void draw_hand(int angle_units, int length, unsigned char color)
{
    int x;
    int y;

    clock_point(angle_units, length, &x, &y);
    gfx_line(CX, CY, x, y, color);
}

/* Trace a circular bezel using the available line primitive. */
void draw_ring(int radius, unsigned char color)
{
    int i;
    int x0;
    int y0;
    int x1;
    int y1;

    for (i = 0; i < 32; i++)
    {
        clock_point(i * 60, radius, &x0, &y0);
        clock_point((i + 1) * 60, radius, &x1, &y1);
        gfx_line(x0, y0, x1, y1, color);
    }
}

/* Put Arabic hour numerals around the dial for traditional clock faces. */
void draw_arabic_numbers(void)
{
    gfx_text(CX - 3, CY - 60, "12", WHITE);
    gfx_text(CX + 24, CY - 51, "1", WHITE);
    gfx_text(CX + 43, CY - 32, "2", WHITE);
    gfx_text(CX + 51, CY - 4, "3", WHITE);
    gfx_text(CX + 43, CY + 21, "4", WHITE);
    gfx_text(CX + 24, CY + 40, "5", WHITE);
    gfx_text(CX - 3, CY + 47, "6", WHITE);
    gfx_text(CX - 30, CY + 40, "7", WHITE);
    gfx_text(CX - 49, CY + 21, "8", WHITE);
    gfx_text(CX - 56, CY - 4, "9", WHITE);
    gfx_text(CX - 49, CY - 32, "10", WHITE);
    gfx_text(CX - 30, CY - 51, "11", WHITE);
}

/* Draw one of five traditional dial layouts around the shared clock hands. */
void draw_analog_face(int style)
{
    int i;
    int inner;
    int x0;
    int y0;
    int x1;
    int y1;

    if (style == 0)
    {
        draw_ring(FACE_R, BLUE);
        draw_ring(FACE_R - 3, DARK_GRAY);
        for (i = 0; i < 60; i++)
        {
            inner = FACE_R - 11;
            if (i % 5 == 0)
            {
                inner = FACE_R - 19;
            }
            clock_point(i * 32, inner, &x0, &y0);
            clock_point(i * 32, FACE_R - 5, &x1, &y1);
            gfx_line(x0, y0, x1, y1, CYAN);
        }
        draw_arabic_numbers();
    }
    else if (style == 1)
    {
        draw_ring(FACE_R, YELLOW);
        draw_ring(FACE_R - 5, DARK_GRAY);
        for (i = 0; i < 12; i++)
        {
            clock_point(i * 160, FACE_R - 8, &x0, &y0);
            clock_point(i * 160, FACE_R - 22, &x1, &y1);
            gfx_line(x0, y0, x1, y1, YELLOW);
        }
        gfx_text(CX - 9, CY - 59, "XII", WHITE);
        gfx_text(CX + 46, CY - 4, "III", WHITE);
        gfx_text(CX - 6, CY + 47, "VI", WHITE);
        gfx_text(CX - 61, CY - 4, "IX", WHITE);
    }
    else if (style == 2)
    {
        draw_ring(FACE_R, WHITE);
        draw_ring(FACE_R - 7, GRAY);
        for (i = 0; i < 60; i++)
        {
            inner = FACE_R - 9;
            if (i % 5 == 0)
            {
                inner = FACE_R - 20;
            }
            clock_point(i * 32, inner, &x0, &y0);
            clock_point(i * 32, FACE_R - 4, &x1, &y1);
            if (i % 5 == 0)
                gfx_line(x0, y0, x1, y1, WHITE);
            else
                gfx_line(x0, y0, x1, y1, GRAY);
        }
        gfx_text(CX - 3, CY - 57, "12", WHITE);
        gfx_text(CX + 50, CY - 4, "3", WHITE);
        gfx_text(CX - 3, CY + 45, "6", WHITE);
        gfx_text(CX - 57, CY - 4, "9", WHITE);
    }
    else if (style == 3)
    {
        draw_ring(FACE_R, GRAY);
        draw_ring(FACE_R - 3, DARK_GRAY);
        clock_point(0, 40, &x0, &y0);
        clock_point(480, 40, &x1, &y1);
        gfx_line(x0, y0, x1, y1, YELLOW);
        clock_point(960, 40, &x0, &y0);
        gfx_line(x1, y1, x0, y0, YELLOW);
        clock_point(1440, 40, &x1, &y1);
        gfx_line(x0, y0, x1, y1, YELLOW);
        clock_point(0, 40, &x0, &y0);
        gfx_line(x1, y1, x0, y0, YELLOW);
        for (i = 0; i < 12; i++)
        {
            clock_point(i * 160, FACE_R - 15, &x0, &y0);
            if (i % 3 == 0)
                gfx_rect(x0 - 2, y0 - 2, 5, 5, YELLOW);
            else
                gfx_rect(x0 - 2, y0 - 2, 5, 5, CYAN);
        }
    }
    else
    {
        draw_ring(FACE_R, GREEN);
        draw_ring(FACE_R - 2, DARK_GRAY);
        for (i = 0; i < 60; i++)
        {
            inner = FACE_R - 10;
            if (i % 5 == 0)
            {
                inner = FACE_R - 24;
            }
            clock_point(i * 32, inner, &x0, &y0);
            clock_point(i * 32, FACE_R - 5, &x1, &y1);
            if (i % 5 == 0)
                gfx_line(x0, y0, x1, y1, GREEN);
            else
                gfx_line(x0, y0, x1, y1, DARK_GRAY);
        }
        draw_arabic_numbers();
    }
}

/* Animation state for the six time digits (10 means a blank tube or card). */
char digit_new[6];
char digit_old[6];
char digit_anim[6];

/* Return the left edge of a digit slot, leaving gaps for the colons. */
int digit_x(int i)
{
    int x;

    x = 26 + i * 43;
    if (i > 1)
    {
        x = x + 18;
    }
    if (i > 3)
    {
        x = x + 18;
    }
    return x;
}

/* Advance per-digit animations; reset restarts them (used on face change). */
void update_digits(unsigned int hours, unsigned int minutes,
                   unsigned int seconds, int reset)
{
    int i;
    char value[6];

    value[0] = hours / 10;
    value[1] = hours % 10;
    value[2] = minutes / 10;
    value[3] = minutes % 10;
    value[4] = seconds / 10;
    value[5] = seconds % 10;
    for (i = 0; i < 6; i++)
    {
        if (digit_anim[i] > 0)
        {
            digit_anim[i] = digit_anim[i] + 1;
            if (digit_anim[i] > 8)
            {
                digit_anim[i] = 0;
            }
        }
        if (reset)
        {
            digit_old[i] = 10;
            digit_new[i] = value[i];
            digit_anim[i] = 1;
        }
        else if (digit_new[i] != value[i])
        {
            digit_old[i] = digit_new[i];
            digit_new[i] = value[i];
            digit_anim[i] = 1;
        }
    }
}

/* Draw only some rows of a large digit; blank values draw nothing. */
void draw_digit_rows(int x, int y, int value, unsigned char color,
                     int first_row, int last_row)
{
    int row;
    int column;

    if (value > 9)
    {
        return;
    }
    for (row = first_row; row <= last_row; row++)
    {
        for (column = 0; column < 5; column++)
        {
            if (digit_font[value * 7 + row] & (16 >> column))
            {
                gfx_rect(x + column * 4, y + row * 6, 4, 5, color);
            }
        }
    }
}

/* Draw one Nixie tube; its cathode digit dims, goes dark, then re-ignites. */
void draw_nixie_tube(int x, int old, int cur, int anim)
{
    unsigned char edge;
    int shown;
    unsigned char color;

    shown = cur;
    color = ORANGE;
    edge = ORANGE;
    if (anim == 1 || anim == 2)
    {
        shown = old;
        color = DARK_RED;
    }
    else if (anim == 3)
    {
        shown = 10;
        edge = DARK_GRAY;
    }
    else if (anim == 4)
    {
        color = DARK_RED;
    }
    else if (anim == 5 || anim == 7)
    {
        color = YELLOW;
    }
    gfx_rect(x, 82, 34, 82, DARK_RED);
    gfx_line(x + 2, 84, x + 31, 84, edge);
    gfx_line(x + 2, 161, x + 31, 161, DARK_RED);
    gfx_line(x + 2, 84, x + 2, 161, edge);
    gfx_line(x + 31, 84, x + 31, 161, DARK_RED);
    draw_digit_rows(x + 7, 103, shown, DARK_RED, 0, 6);
    draw_digit_rows(x + 7, 102, shown, color, 0, 6);
}

/* Draw glowing digit tubes for a warm, Nixie-inspired digital face. */
void draw_nixie_time(void)
{
    int i;

    for (i = 0; i < 6; i++)
    {
        draw_nixie_tube(digit_x(i), digit_old[i], digit_new[i], digit_anim[i]);
    }
    gfx_rect(157, 112, 4, 4, ORANGE);
    gfx_rect(157, 132, 4, 4, ORANGE);
}

/* Draw one split-flap card; t is the flip phase (0 = settled, 1-4 = flipping). */
void draw_flip_card(int x, int old, int cur, int t)
{
    gfx_rect(x, 84, 36, 74, GRAY);
    gfx_rect(x + 2, 86, 32, 33, BLACK);
    gfx_rect(x + 2, 121, 32, 35, DARK_GRAY);
    if (t == 0 || t == 4)
    {
        draw_digit_rows(x + 8, 100, cur, WHITE, 0, 6);
    }
    else
    {
        draw_digit_rows(x + 8, 100, cur, WHITE, 0, 3);
        draw_digit_rows(x + 8, 100, old, WHITE, 3, 6);
    }
    if (t == 1)
    {
        gfx_rect(x + 2, 99, 32, 22, BLACK);
        draw_digit_rows(x + 8, 100, old, WHITE, 1, 3);
        gfx_line(x + 2, 99, x + 33, 99, GRAY);
    }
    else if (t == 2)
    {
        gfx_rect(x + 2, 111, 32, 10, BLACK);
        draw_digit_rows(x + 8, 100, old, WHITE, 3, 3);
        gfx_line(x + 2, 111, x + 33, 111, GRAY);
    }
    else if (t == 3)
    {
        gfx_rect(x + 2, 121, 32, 10, DARK_GRAY);
        draw_digit_rows(x + 8, 100, cur, WHITE, 3, 4);
        gfx_line(x + 2, 131, x + 33, 131, GRAY);
    }
    gfx_line(x + 2, 121, x + 33, 121, BLACK);
    gfx_line(x + 3, 123, x + 32, 123, GRAY);
}

/* Draw the six flip cards, each flipping when its own digit changes. */
void draw_flip_time(void)
{
    int i;
    int t;

    for (i = 0; i < 6; i++)
    {
        t = 0;
        if (digit_anim[i] > 0)
        {
            t = (digit_anim[i] + 1) / 2;
        }
        draw_flip_card(digit_x(i), digit_old[i], digit_new[i], t);
    }
    gfx_rect(157, 111, 4, 4, WHITE);
    gfx_rect(157, 132, 4, 4, WHITE);
}

/* Draw a hand three pixels wide. */
void draw_thick_hand(int angle, int length, unsigned char color)
{
    int x;
    int y;

    clock_point(angle, length, &x, &y);
    gfx_line(CX, CY, x, y, color);
    gfx_line(CX + 1, CY, x + 1, y, color);
    gfx_line(CX, CY + 1, x, y + 1, color);
}

/* Draw a small square ornament along a hand (or its tail with angle + 960). */
void draw_marker(int angle, int radius, int size, unsigned char color)
{
    int x;
    int y;

    clock_point(angle, radius, &x, &y);
    gfx_rect(x - size, y - size, size * 2 + 1, size * 2 + 1, color);
}

/* Draw an open diamond-shaped skeleton hand. */
void draw_skeleton_hand(int angle, int length, unsigned char color)
{
    int tip_x;
    int tip_y;
    int mid_x;
    int mid_y;
    int side_x;
    int side_y;

    clock_point(angle, length, &tip_x, &tip_y);
    clock_point(angle, length * 35 / 100, &mid_x, &mid_y);
    clock_point(angle + 480, 5, &side_x, &side_y);
    side_x = side_x - CX;
    side_y = side_y - CY;
    gfx_line(CX, CY, mid_x + side_x, mid_y + side_y, color);
    gfx_line(mid_x + side_x, mid_y + side_y, tip_x, tip_y, color);
    gfx_line(tip_x, tip_y, mid_x - side_x, mid_y - side_y, color);
    gfx_line(mid_x - side_x, mid_y - side_y, CX, CY, color);
}

/* Draw the hand set that belongs to the selected analog face. */
void draw_hands(int style, int hour_angle, int minute_angle, int second_angle)
{
    if (style == 0)
    {
        /* Classic: baton hands with a long second-hand tail. */
        draw_thick_hand(hour_angle, 29, WHITE);
        draw_thick_hand(minute_angle, 47, CYAN);
        draw_hand(second_angle, 65, ORANGE);
        draw_hand(second_angle + 960, 14, ORANGE);
    }
    else if (style == 1)
    {
        /* Roman: slim hands with diamond ornaments and a ringed second hand. */
        draw_hand(hour_angle, 29, YELLOW);
        draw_marker(hour_angle, 21, 2, YELLOW);
        draw_hand(minute_angle, 47, WHITE);
        draw_marker(minute_angle, 38, 2, WHITE);
        draw_hand(second_angle, 65, RED);
        draw_marker(second_angle + 960, 13, 2, RED);
    }
    else if (style == 2)
    {
        /* Railway: bold bars and a lollipop second hand. */
        draw_thick_hand(hour_angle, 30, WHITE);
        draw_thick_hand(hour_angle, 24, GRAY);
        draw_thick_hand(minute_angle, 50, WHITE);
        draw_hand(second_angle, 56, RED);
        draw_marker(second_angle, 60, 3, RED);
    }
    else if (style == 3)
    {
        /* Art Deco: open skeleton hands with a slender cyan second hand. */
        draw_skeleton_hand(hour_angle, 30, YELLOW);
        draw_skeleton_hand(minute_angle, 50, YELLOW);
        draw_hand(second_angle, 66, CYAN);
        draw_marker(second_angle, 46, 1, CYAN);
    }
    else
    {
        /* Pilot: broad hands with luminous blocks and a white-tipped second hand. */
        draw_thick_hand(hour_angle, 27, GREEN);
        draw_marker(hour_angle, 20, 3, WHITE);
        draw_hand(minute_angle, 49, GREEN);
        draw_hand(minute_angle + 0, 48, GREEN);
        draw_marker(minute_angle, 38, 2, WHITE);
        draw_marker(minute_angle, 46, 1, WHITE);
        draw_hand(second_angle, 64, ORANGE);
        draw_marker(second_angle, 62, 1, WHITE);
        draw_hand(second_angle + 960, 16, ORANGE);
    }
    gfx_rect(CX - 2, CY - 2, 5, 5, WHITE);
}
/* Keep a small HH:MM:SS readout on the analog face styles. */
void draw_digital_time(unsigned int hours, unsigned int minutes, unsigned int seconds)
{
    char time_text[9];

    time_text[0] = '0' + hours / 10;
    time_text[1] = '0' + hours % 10;
    time_text[2] = ':';
    time_text[3] = '0' + minutes / 10;
    time_text[4] = '0' + minutes % 10;
    time_text[5] = ':';
    time_text[6] = '0' + seconds / 10;
    time_text[7] = '0' + seconds % 10;
    time_text[8] = 0;
    gfx_text(CX - 24, 216, time_text, WHITE);
}

/* Read the RTC and redraw the clock once per emulator frame. */
int main(void)
{
    unsigned int hours;
    unsigned int minutes;
    unsigned int seconds;
    unsigned int seconds_before;
    unsigned int keys;
    unsigned int previous_keys;
    int face;
    int reset_digits;
    int hour_angle;
    int minute_angle;
    int second_angle;

    face = 0;
    previous_keys = 0;
    reset_digits = 1;
    while (1)
    {
        keys = gfx_keys();
        if ((keys & KEY_TAB) && !(previous_keys & KEY_TAB))
        {
            face = (face + 1) % FACE_COUNT;
            reset_digits = 1;
        }
        previous_keys = keys;

        /* Retry if the RTC ticks while its three fields are being read. */
        seconds_before = 0;
        seconds = 1;
        while (seconds != seconds_before)
        {
            seconds_before = gfx_rtc(0);
            minutes = gfx_rtc(1);
            hours = gfx_rtc(2);
            seconds = gfx_rtc(0);
        }

        /* Angles use 60 subdivisions per sine-table step for smooth motion. */
        hour_angle = (hours % 12) * 160 +
                     (minutes * 8) / 3 + (seconds * 8) / 180;
        minute_angle = minutes * 32 + seconds * 32 / 60;
        second_angle = seconds * 32;

        update_digits(hours, minutes, seconds, reset_digits);
        reset_digits = 0;
        gfx_clear(BLACK);
        gfx_text(8, 8, "TAB: NEXT FACE", GRAY);
        if (face == 0)
        {
            gfx_text(132, 8, "CLASSIC", CYAN);
            draw_analog_face(0);
        }
        else if (face == 1)
        {
            gfx_text(132, 8, "ROMAN", YELLOW);
            draw_analog_face(1);
        }
        else if (face == 2)
        {
            gfx_text(132, 8, "RAILWAY", WHITE);
            draw_analog_face(2);
        }
        else if (face == 3)
        {
            gfx_text(132, 8, "ART DECO", YELLOW);
            draw_analog_face(3);
        }
        else if (face == 4)
        {
            gfx_text(132, 8, "PILOT", GREEN);
            draw_analog_face(4);
        }
        else if (face == 5)
        {
            gfx_text(132, 8, "NIXIE TUBES", ORANGE);
            draw_nixie_time();
        }
        else
        {
            gfx_text(132, 8, "FLIP CLOCK", WHITE);
            draw_flip_time();
        }

        if (face < 5)
        {
            /* Analog hands share one RTC snapshot and retain fractional motion. */
            draw_hands(face, hour_angle, minute_angle, second_angle);
            draw_digital_time(hours, minutes, seconds);
        }
        gfx_present();
    }

    return 0;
}
