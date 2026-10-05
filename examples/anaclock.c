// Seven clock styles driven by the emulator's local real-time clock.
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

/* Draw glowing digit tubes for a warm, Nixie-inspired digital face. */
void draw_nixie_time(unsigned int hours, unsigned int minutes, unsigned int seconds)
{
    int i;
    int x;
    char digits[6];

    digits[0] = '0' + hours / 10;
    digits[1] = '0' + hours % 10;
    digits[2] = '0' + minutes / 10;
    digits[3] = '0' + minutes % 10;
    digits[4] = '0' + seconds / 10;
    digits[5] = '0' + seconds % 10;

    for (i = 0; i < 6; i++)
    {
        x = 26 + i * 43;
        if (i > 1)
        {
            x = x + 18;
        }
        if (i > 3)
        {
            x = x + 18;
        }
        gfx_rect(x, 82, 34, 82, DARK_RED);
        gfx_line(x + 2, 84, x + 31, 84, ORANGE);
        gfx_line(x + 2, 161, x + 31, 161, DARK_RED);
        gfx_line(x + 2, 84, x + 2, 161, ORANGE);
        gfx_line(x + 31, 84, x + 31, 161, DARK_RED);
        draw_large_digit(x + 7, 103, digits[i] - '0', DARK_RED);
        draw_large_digit(x + 7, 102, digits[i] - '0', ORANGE);
    }
    gfx_rect(157, 112, 4, 4, ORANGE);
    gfx_rect(157, 132, 4, 4, ORANGE);
}

/* Draw a hinged split-flap card for each digit of the current time. */
void draw_flip_time(unsigned int hours, unsigned int minutes, unsigned int seconds)
{
    int i;
    int x;
    char digits[6];

    digits[0] = '0' + hours / 10;
    digits[1] = '0' + hours % 10;
    digits[2] = '0' + minutes / 10;
    digits[3] = '0' + minutes % 10;
    digits[4] = '0' + seconds / 10;
    digits[5] = '0' + seconds % 10;
    for (i = 0; i < 6; i++)
    {
        x = 26 + i * 43;
        if (i > 1)
        {
            x = x + 18;
        }
        if (i > 3)
        {
            x = x + 18;
        }
        gfx_rect(x, 84, 36, 74, GRAY);
        gfx_rect(x + 2, 86, 32, 33, BLACK);
        gfx_rect(x + 2, 121, 32, 35, DARK_GRAY);
        draw_large_digit(x + 8, 100, digits[i] - '0', WHITE);
        gfx_line(x + 2, 121, x + 33, 121, BLACK);
        gfx_line(x + 3, 123, x + 32, 123, GRAY);
    }
    gfx_rect(157, 111, 4, 4, WHITE);
    gfx_rect(157, 132, 4, 4, WHITE);
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
    int hour_angle;
    int minute_angle;
    int second_angle;

    face = 0;
    previous_keys = 0;
    while (1)
    {
        keys = gfx_keys();
        if ((keys & KEY_TAB) && !(previous_keys & KEY_TAB))
        {
            face = (face + 1) % FACE_COUNT;
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
            draw_nixie_time(hours, minutes, seconds);
        }
        else
        {
            gfx_text(132, 8, "FLIP CLOCK", WHITE);
            draw_flip_time(hours, minutes, seconds);
        }

        if (face < 5)
        {
            /* Analog hands share one RTC snapshot and retain fractional motion. */
            draw_hand(hour_angle, 31, DARK_GRAY);
            draw_hand(minute_angle, 49, DARK_GRAY);
            draw_hand(second_angle, 65, DARK_GRAY);
            draw_hand(hour_angle, 29, WHITE);
            draw_hand(minute_angle, 47, CYAN);
            draw_hand(second_angle, 65, ORANGE);
            gfx_rect(CX - 2, CY - 2, 5, 5, WHITE);
            draw_digital_time(hours, minutes, seconds);
        }
        gfx_present();
    }

    return 0;
}
