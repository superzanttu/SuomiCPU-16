// Space Invaders for the SC-16.
// ICON
// ................
// ...##......##...
// ....##....##....
// ...##########...
// ..##.######.##..
// .##############.
// ##..##.##.##..##
// ##..##.##.##..##
// ################
// .##..######..##.
// ..##........##..
// .##..........##.
// ##............##
// ................
// ................
// ................
// Controls:
// - Left/Right (or A/D): Move ship
// - Space: Fire bullet
// - Enter: Restart after GAME OVER or clearing all waves.
#include "suomi_gfx.h"

/* Constants */
#define COLS 8
#define ROWS 4
#define COUNT 32
#define MAXS 3
#define CELL_W 24
#define CELL_H 18
#define PLAYER_Y 214

/* Sprite data (8x8 bitmaps) */
unsigned char inv_a0[8] = {0x18, 0x3C, 0x7E, 0xDB, 0xFF, 0x24, 0x5A, 0xA5};
unsigned char inv_a1[8] = {0x18, 0x3C, 0x7E, 0xDB, 0xFF, 0x5A, 0x81, 0x42};
unsigned char inv_b0[8] = {0x42, 0x24, 0x7E, 0xDB, 0xFF, 0xFF, 0xA5, 0x24};
unsigned char inv_b1[8] = {0x42, 0x24, 0xFF, 0xDB, 0xFF, 0x7E, 0x24, 0x42};
unsigned char inv_c0[8] = {0x3C, 0x7E, 0xFF, 0x99, 0xFF, 0x66, 0xC3, 0x81};
unsigned char inv_c1[8] = {0x3C, 0x7E, 0xFF, 0x99, 0xFF, 0x66, 0x42, 0xC3};
unsigned char ship_bmp[8] = {0x10, 0x38, 0x38, 0x7C, 0xFE, 0xFE, 0xFE, 0x00};
unsigned char boom_bmp[8] = {0x91, 0x4A, 0x24, 0x81, 0x81, 0x24, 0x4A, 0x91};

/* Game State */
unsigned char alive[COUNT];
int ox;            // Invaders formation X offset
int oy;            // Invaders formation Y offset
int dir;           // Movement direction (positive: right, negative: left)
int px;            // Player X position
int bullet_x;      // Player's bullet X
int bullet_y;      // Player's bullet Y
int shot_x[MAXS];  // Enemy bullets X
int shot_y[MAXS];  // Enemy bullets Y
int left_count;    // Number of remaining invaders
int move_timer;    // Timer for formation movement
int move_delay;    // Delay between formation movements
int anim;          // Animation frame toggle
int cooldown;      // Player fire cooldown
int lives;         // Player remaining lives
int wave;          // Current wave number
int march;         // Step counter for the four-note march sound
int over;          // Game over flag
int wait;          // Delay before restart is allowed
int boom_x;        // Explosion X position
int boom_y;        // Explosion Y position
int boom_timer;    // Explosion duration timer
int invuln;        // Player invulnerability timer
unsigned int score; // Current score

/* Initializes a new wave of invaders */
void set_wave(void)
{
    int i;
    for (i = 0; i < COUNT; i++)
    {
        alive[i] = 1;
    }
    for (i = 0; i < MAXS; i++)
    {
        shot_y[i] = -1;
    }
    left_count = COUNT;
    ox = 16;
    oy = 28 + (wave & 3) * 6;
    dir = 2;
    move_timer = 0;
    move_delay = 24 - (wave & 3) * 3;
    bullet_y = -1;
}

/* Resets the game to starting state */
void new_game(void)
{
    score = 0;
    lives = 3;
    wave = 0;
    over = 0;
    wait = 0;
    px = 150;
    boom_timer = 0;
    invuln = 0;
    set_wave();
}

/* Converts an unsigned integer to a string */
void number_text(unsigned int value, char *out)
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

/* Finds the lowest active invader in a given column */
int lowest_in_column(int c)
{
    int r;
    for (r = ROWS - 1; r >= 0; r--)
    {
        if (alive[r * COLS + c])
        {
            return r;
        }
    }
    return -1;
}

/* Randomly selects an invader to fire a shot */
void invader_fire(void)
{
    int s;
    int c;
    int r;
    c = gfx_random() & 7;
    r = lowest_in_column(c);
    if (r < 0)
    {
        return;
    }
    for (s = 0; s < MAXS; s++)
    {
        if (shot_y[s] < 0)
        {
            shot_x[s] = ox + c * CELL_W + 8;
            shot_y[s] = oy + r * CELL_H + 8;
            return;
        }
    }
}

/* Moves the invader formation and checks for game over */
void step_formation(void)
{
    int r;
    int c;
    int minc;
    int maxc;
    int low;
    minc = COLS;
    maxc = -1;
    low = -1;
    for (c = 0; c < COLS; c++)
    {
        r = lowest_in_column(c);
        if (r >= 0)
        {
            if (c < minc)
            {
                minc = c;
            }
            maxc = c;
            if (r > low)
            {
                low = r;
            }
        }
    }
    if (maxc < 0)
    {
        return;
    }
    anim = 1 - anim;
    if ((dir > 0 && ox + maxc * CELL_W + 8 + dir > 312) || (dir < 0 && ox + minc * CELL_W + dir < 8))
    {
        dir = -dir;
        oy = oy + 8;
        if (oy + low * CELL_H + 8 >= PLAYER_Y - 4)
        {
            over = 1;
            wait = 60;
        }
    }
    else
    {
        ox = ox + dir;
    }
}

/* Handles killing an invader and updating score */
void hit_invader(int index)
{
    int r;
    alive[index] = 0;
    left_count--;
    gfx_sound(1, 450, 8, WAVE_NOISE, 60);
    r = index >> 3;
    score += (ROWS - r) * 10;
    if (left_count == 8)
    {
        move_delay = move_delay / 2 + 1;
    }
}

/* Handles player movement and firing */
void update_player(unsigned int keys)
{
    if ((keys & KEY_LEFT) && px > 8)
    {
        px = px - 2;
    }
    if ((keys & KEY_RIGHT) && px < 296)
    {
        px = px + 2;
    }
    if (cooldown > 0)
    {
        cooldown--;
    }
    if ((keys & KEY_FIRE) && bullet_y < 0 && cooldown == 0)
    {
        bullet_x = px + 4;
        bullet_y = PLAYER_Y - 4;
        cooldown = 10;
        gfx_sound(2, 1200, 4, WAVE_SQUARE, 35);
    }
    if (invuln > 0)
    {
        invuln--;
    }
}

/* Updates player bullet position and checks for hits */
void update_bullet(void)
{
    int r;
    int c;
    int index;
    int hx;
    int hy;
    if (bullet_y < 0)
    {
        return;
    }
    bullet_y = bullet_y - 5;
    if (bullet_y < 12)
    {
        bullet_y = -1;
        return;
    }
    r = (bullet_y - oy) / CELL_H;
    c = (bullet_x - ox) / CELL_W;
    if (bullet_y >= oy && bullet_x >= ox && r < ROWS && c < COLS)
    {
        index = r * COLS + c;
        hx = ox + c * CELL_W;
        hy = oy + r * CELL_H;
        if (alive[index] && bullet_x < hx + 16 && bullet_y < hy + 8)
        {
            hit_invader(index);
            boom_x = hx;
            boom_y = hy;
            boom_timer = 8;
            bullet_y = -1;
        }
    }
}

/* Updates enemy bullet positions and checks for player hits */
void update_shots(void)
{
    int s;
    for (s = 0; s < MAXS; s++)
    {
        if (shot_y[s] >= 0)
        {
            shot_y[s] = shot_y[s] + 3;
            if (shot_y[s] > 232)
            {
                shot_y[s] = -1;
            }
            else if (invuln == 0 && shot_y[s] >= PLAYER_Y && shot_y[s] < PLAYER_Y + 8 && shot_x[s] >= px && shot_x[s] < px + 8)
            {
                shot_y[s] = -1;
                lives--;
                invuln = 60;
                gfx_sound(1, 120, 40, WAVE_NOISE, 90);
                if (lives == 0)
                {
                    over = 1;
                    wait = 60;
                    gfx_sound(3, 70, 70, WAVE_TRIANGLE, 70);
                }
            }
        }
    }
}

/* Main game logic update loop */
void update(unsigned int keys)
{
    update_player(keys);
    update_bullet();
    update_shots();
    move_timer++;
    if (move_timer >= move_delay)
    {
        move_timer = 0;
        step_formation();
        /* Classic descending four-note heartbeat */
        march = (march + 1) & 3;
        gfx_sound(0, 110 - march * 10, 5, WAVE_SQUARE, 40);
    }
    if ((gfx_random() & 63) == 0)
    {
        invader_fire();
    }
    if (boom_timer > 0)
    {
        boom_timer--;
    }
    if (left_count == 0 && over == 0)
    {
        wave++;
        score += 100;
        gfx_sound(3, 880, 14, WAVE_TRIANGLE, 50);
        set_wave();
    }
}

/* Draws the current wave of invaders */
void draw_invaders(void)
{
    int r;
    int c;
    unsigned char *shape;
    unsigned char color;
    for (r = 0; r < ROWS; r++)
    {
        color = GREEN;
        shape = inv_a0;
        if (r == 0)
        {
            color = MAGENTA;
            if (anim)
            {
                shape = inv_c1;
            }
            else
            {
                shape = inv_c0;
            }
        }
        else if (r < 3)
        {
            color = CYAN;
            if (anim)
            {
                shape = inv_b1;
            }
            else
            {
                shape = inv_b0;
            }
        }
        else
        {
            if (anim)
            {
                shape = inv_a1;
            }
        }
        for (c = 0; c < COLS; c++)
        {
            if (alive[r * COLS + c])
            {
                gfx_bitmap(ox + c * CELL_W + 4, oy + r * CELL_H, 8, 8, shape, color);
            }
        }
    }
}

/* Renders the game frame */
void draw(void)
{
    char buffer[8];
    int s;
    int i;
    gfx_clear(BLACK);
    draw_invaders();
    if (boom_timer > 0)
    {
        gfx_bitmap(boom_x + 4, boom_y, 8, 8, boom_bmp, YELLOW);
    }
    if (over == 0 && (invuln & 4) == 0)
    {
        gfx_bitmap(px, PLAYER_Y, 8, 8, ship_bmp, WHITE);
    }
    if (bullet_y >= 0)
    {
        gfx_rect(bullet_x, bullet_y, 1, 4, WHITE);
    }
    for (s = 0; s < MAXS; s++)
    {
        if (shot_y[s] >= 0)
        {
            gfx_rect(shot_x[s], shot_y[s], 2, 4, RED);
        }
    }
    gfx_line(0, 226, 319, 226, GREEN);
    number_text(score, buffer);
    gfx_text(4, 4, "SCORE", GRAY);
    gfx_text(40, 4, buffer, WHITE);
    gfx_text(120, 4, "WAVE", GRAY);
    number_text(wave + 1, buffer);
    gfx_text(150, 4, buffer, WHITE);
    for (i = 0; i < lives; i++)
    {
        gfx_bitmap(300 - i * 10, 4, 8, 8, ship_bmp, CYAN);
    }
    if (over)
    {
        gfx_text(124, 108, "GAME OVER", RED);
        gfx_text(100, 124, "PRESS ENTER TO RESTART", WHITE);
    }
}

int main(void)
{
    unsigned int keys;
    new_game();
    while (1)
    {
        keys = gfx_keys();
        if (over)
        {
            if (wait > 0)
            {
                wait--;
            }
            else if (keys & (KEY_START | KEY_FIRE))
            {
                new_game();
            }
        }
        else
        {
            update(keys);
        }
        draw();
        gfx_present();
    }
    return 0;
}
