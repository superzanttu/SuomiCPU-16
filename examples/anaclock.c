// Modern analog clock with a digital hours, minutes, and seconds display.
// The clock reads the emulator's real-time clock and runs continuously.
#include "suomi_gfx.h"

#define CX 160
#define CY 108
#define FACE_R 88

/* Sine values scaled by 100; each table step is 1/32 of a full turn. */
char sine[32] = {
    0, 20, 38, 56, 71, 83, 92, 98,
    100, 98, 92, 83, 71, 56, 38, 20,
    0, -20, -38, -56, -71, -83, -92, -98,
    -100, -98, -92, -83, -71, -56, -38, -20
};

/* Draw one hand from the center to a point at the supplied clock angle. */
void draw_hand(int angle, int length, unsigned char color)
{
    int x;
    int y;

    /* Angle zero points straight up; positive angles move clockwise. */
    x = CX + (sine[angle & 31] * length) / 100;
    y = CY - (sine[(angle + 8) & 31] * length) / 100;
    gfx_line(CX, CY, x, y, color);
}

/* Draw the dial, minute markers, and the four cardinal numerals. */
void draw_face(void)
{
    int i;
    int angle;
    int inner;
    int outer;
    int x0;
    int y0;
    int x1;
    int y1;

    /* Layered circular outline gives the dial a bright bezel. */
    for (i = 0; i < 32; i++)
    {
        angle = i;
        x0 = CX + (sine[angle] * FACE_R) / 100;
        y0 = CY - (sine[(angle + 8) & 31] * FACE_R) / 100;
        angle = (i + 1) & 31;
        x1 = CX + (sine[angle] * FACE_R) / 100;
        y1 = CY - (sine[(angle + 8) & 31] * FACE_R) / 100;
        gfx_line(x0, y0, x1, y1, BLUE);

        /* A second inner ring adds contrast around the markers. */
        x0 = CX + (sine[i] * (FACE_R - 3)) / 100;
        y0 = CY - (sine[(i + 8) & 31] * (FACE_R - 3)) / 100;
        x1 = CX + (sine[angle] * (FACE_R - 3)) / 100;
        y1 = CY - (sine[(angle + 8) & 31] * (FACE_R - 3)) / 100;
        gfx_line(x0, y0, x1, y1, DARK_GRAY);
    }

    /* The 32 markers include longer accents at the cardinal directions. */
    for (i = 0; i < 32; i++)
    {
        outer = FACE_R - 8;
        inner = FACE_R - 13;
        if ((i & 7) == 0)
        {
            inner = FACE_R - 20;
        }
        x0 = CX + (sine[i] * inner) / 100;
        y0 = CY - (sine[(i + 8) & 31] * inner) / 100;
        x1 = CX + (sine[i] * outer) / 100;
        y1 = CY - (sine[(i + 8) & 31] * outer) / 100;
        gfx_line(x0, y0, x1, y1, CYAN);
    }

    /* Minimal numerals keep the hands and time easy to read. */
    gfx_text(CX - 3, CY - 68, "12", WHITE);
    gfx_text(CX + 61, CY - 4, "3", WHITE);
    gfx_text(CX - 3, CY + 61, "6", WHITE);
    gfx_text(CX - 68, CY - 4, "9", WHITE);
}

/* Draw the live digital time beneath the analog dial. */
void draw_digital_time(unsigned int hours, unsigned int minutes, unsigned int seconds)
{
    char time_text[9];

    time_text[0] = '0' + (hours / 10);
    time_text[1] = '0' + (hours % 10);
    time_text[2] = ':';
    time_text[3] = '0' + (minutes / 10);
    time_text[4] = '0' + (minutes % 10);
    time_text[5] = ':';
    time_text[6] = '0' + (seconds / 10);
    time_text[7] = '0' + (seconds % 10);
    time_text[8] = 0;

    gfx_text(CX - 24, 216, time_text, WHITE);
}

/* Read the RTC and redraw the clock once per emulator frame. */
int main(void)
{
    unsigned int hours;
    unsigned int minutes;
    unsigned int seconds;
    unsigned int hour_angle;
    unsigned int minute_angle;
    unsigned int second_angle;

    while (1)
    {
        hours = gfx_rtc(2);
        minutes = gfx_rtc(1);
        seconds = gfx_rtc(0);

        /* Include the smaller units in the hour and minute hand positions. */
        hour_angle = (hours % 12) * 32 / 12 + minutes * 32 / 720;
        minute_angle = minutes * 32 / 60 + seconds * 32 / 3600;
        second_angle = seconds * 32 / 60;

        gfx_clear(BLACK);
        gfx_text(CX - 30, 8, "LOCAL TIME", GRAY);
        draw_face();

        /* Draw subtle shadows before each colored hand. */
        draw_hand(hour_angle, 31, DARK_GRAY);
        draw_hand(minute_angle, 49, DARK_GRAY);
        draw_hand(second_angle, 65, DARK_GRAY);
        draw_hand(hour_angle, 29, WHITE);
        draw_hand(minute_angle, 47, CYAN);
        draw_hand(second_angle, 65, ORANGE);
        gfx_rect(CX - 2, CY - 2, 5, 5, WHITE);

        draw_digital_time(hours, minutes, seconds);
        gfx_present();
    }

    return 0;
}
