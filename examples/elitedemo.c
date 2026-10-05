// Simple Elite-style space combat demo for the SC-16.
// Left/Right turn; Up increases throttle; Down decreases throttle; Space fires.
// The ship accelerates gradually and keeps drifting when you change direction.
#include "suomi_gfx.h"

#define VIEW_TOP 30
#define VIEW_BOTTOM 190
#define VIEW_CX 160
#define VIEW_CY 105
#define FOCAL 72
#define HAZARDS 5
#define ENEMIES 3
#define STARS 12

/* Sine values scaled by 100; one table step is 1/32 of a full turn. */
char sine[32] = {
    0, 20, 38, 56, 71, 83, 92, 98,
    100, 98, 92, 83, 71, 56, 38, 20,
    0, -20, -38, -56, -71, -83, -92, -98,
    -100, -98, -92, -83, -71, -56, -38, -20
};

/* Irregular outline used for distant asteroids. */
char rock_shape[16] = {
    0, -9, 7, -7, 11, -1, 7, 7,
    1, 10, -6, 8, -10, 3, -8, -5
};

/* State for the forward-moving hazards; the planet and moon share a depth. */
int hz_x[HAZARDS];
int hz_y[HAZARDS];
int hz_z[HAZARDS];
int hz_kind[HAZARDS];
int hz_phase[HAZARDS];
int hz_speed[HAZARDS];

/* Enemies move toward the player until hit or passed. */
int enemy_x[ENEMIES];
int enemy_y[ENEMIES];
int enemy_z[ENEMIES];
int enemy_active[ENEMIES];

/* Stars are stored in world space so they drift toward the cockpit. */
int star_x[STARS];
int star_y[STARS];
int star_z[STARS];

/* Fixed-point ship position and velocity, heading, and engine throttle. */
int ship_x;
int ship_y;
int ship_vx;
int ship_vz;
int yaw;
int throttle;
int shields;
int laser_heat;
int laser_cooldown;
int shield_timer;
int flash_timer;
int frame;
int game_over;
unsigned int score;

/* Return the absolute value of a signed integer. */
int abs_i(int value)
{
    if (value < 0)
    {
        return -value;
    }
    return value;
}

/* Convert an unsigned value to decimal text for the compact HUD. */
void number_text(unsigned int value, char *out)
{
    char digits[6];
    int count;
    int i;

    count = 0;
    if (value == 0)
    {
        digits[count] = '0';
        count++;
    }
    while (value > 0)
    {
        digits[count] = '0' + (value % 10);
        value = value / 10;
        count++;
    }
    for (i = 0; i < count; i++)
    {
        out[i] = digits[count - i - 1];
    }
    out[count] = 0;
}

/* Rotate world-space offsets into the ship's current view direction. */
int view_depth(int world_x, int depth)
{
    int offset_x;
    int relative_depth;

    offset_x = world_x - ship_x;
    relative_depth = (offset_x * sine[yaw] + depth * sine[(yaw + 8) & 31]) / 100;
    if (relative_depth < 20)
    {
        relative_depth = 20;
    }
    return relative_depth;
}

/* Project horizontal position using perspective and the ship's current heading. */
int project_x(int world_x, int depth)
{
    int offset_x;
    int side;
    int relative_depth;

    offset_x = world_x - ship_x;
    side = (offset_x * sine[(yaw + 8) & 31] - depth * sine[yaw]) / 100;
    relative_depth = view_depth(world_x, depth);
    return VIEW_CX + (side * FOCAL) / relative_depth;
}

/* Project vertical position with perspective relative to the ship. */
int project_y(int world_y, int world_x, int depth)
{
    int vertical;
    int relative_depth;

    vertical = world_y - ship_y;
    relative_depth = view_depth(world_x, depth);
    return VIEW_CY - (vertical * FOCAL) / relative_depth;
}

/* Place a hazard at a new random position far ahead of the ship. */
void reset_hazard(int i, int depth)
{
    hz_x[i] = (gfx_random() % 181) - 90;
    hz_y[i] = (gfx_random() % 101) - 50;
    hz_z[i] = depth;
    hz_phase[i] = gfx_random() & 31;
    hz_speed[i] = 1 + (gfx_random() & 1);
}

/* Create a new enemy fighter at a random point in the forward view. */
void reset_enemy(int i, int depth)
{
    enemy_x[i] = (gfx_random() % 161) - 80;
    enemy_y[i] = (gfx_random() % 81) - 40;
    enemy_z[i] = depth;
    enemy_active[i] = 1;
}

/* Initialize the asteroid field, planet system, enemies, and player systems. */
void new_game(void)
{
    int i;

    ship_x = 0;
    ship_y = 0;
    ship_vx = 0;
    ship_vz = 0;
    yaw = 0;
    throttle = 3;
    shields = 100;
    laser_heat = 0;
    laser_cooldown = 0;
    shield_timer = 0;
    flash_timer = 0;
    frame = 0;
    game_over = 0;
    score = 0;

    reset_hazard(0, 150);
    reset_hazard(1, 220);
    reset_hazard(2, 190);
    reset_hazard(3, 190);
    reset_hazard(4, 270);
    hz_kind[0] = 0;
    hz_kind[1] = 0;
    hz_kind[2] = 2;
    hz_kind[3] = 1;
    hz_kind[4] = 0;

    for (i = 0; i < ENEMIES; i++)
    {
        reset_enemy(i, 120 + i * 60);
    }
    for (i = 0; i < STARS; i++)
    {
        star_x[i] = (gfx_random() % 181) - 90;
        star_y[i] = (gfx_random() % 101) - 50;
        star_z[i] = 35 + (gfx_random() % 220);
    }
}

/* Turn the ship, adjust throttle, accelerate with inertia, and handle firing. */
void update_player(unsigned int keys)
{
    int i;
    int x;
    int y;
    int hit;

    /* Turn at a measured rate rather than snapping the view to each key press. */
    if ((frame & 3) == 0)
    {
        if (keys & KEY_LEFT)
        {
            yaw = (yaw + 31) & 31;
        }
        if (keys & KEY_RIGHT)
        {
            yaw = (yaw + 1) & 31;
        }
    }

    /* Throttle changes a little per key hold, independently of velocity. */
    if ((frame & 7) == 0)
    {
        if ((keys & KEY_UP) && throttle < 8)
        {
            throttle++;
        }
        if ((keys & KEY_DOWN) && throttle > 0)
        {
            throttle--;
        }
    }
    /* Acceleration is applied along the nose. Positions and velocities use
       sixteenths to retain sub-unit motion; velocity continues through turns. */
    ship_vx = ship_vx + (sine[yaw] * throttle) / 10;
    ship_vz = ship_vz + (sine[(yaw + 8) & 31] * throttle) / 10;
    if (ship_vx > 256)
    {
        ship_vx = 256;
    }
    if (ship_vx < -256)
    {
        ship_vx = -256;
    }
    if (ship_vz > 256)
    {
        ship_vz = 256;
    }
    if (ship_vz < -256)
    {
        ship_vz = -256;
    }
    ship_x = ship_x + ship_vx / 16;
    /* Drag gradually reduces drift without instantly stopping the ship. */
    ship_vx = (ship_vx * 15) / 16;
    ship_vz = (ship_vz * 15) / 16;
    if (ship_x < -110)
    {
        ship_x = -110;
    }
    if (ship_x > 110)
    {
        ship_x = 110;
    }

    if (laser_heat > 0 && (frame & 1) == 0)
    {
        laser_heat--;
    }
    if (laser_cooldown > 0)
    {
        laser_cooldown--;
    }
    if (shields < 100)
    {
        shield_timer++;
        if (shield_timer >= 4)
        {
            shields++;
            shield_timer = 0;
        }
    }
    if (flash_timer > 0)
    {
        flash_timer--;
    }

    /* A forward laser only hits a fighter aligned with the central reticle. */
    if ((keys & KEY_FIRE) && laser_cooldown == 0 && laser_heat < 96)
    {
        hit = -1;
        for (i = 0; i < ENEMIES; i++)
        {
            if (enemy_active[i] && enemy_z[i] > 24)
            {
                x = project_x(enemy_x[i], enemy_z[i]);
                y = project_y(enemy_y[i], enemy_x[i], enemy_z[i]);
                if (abs_i(x - VIEW_CX) < 8 && abs_i(y - VIEW_CY) < 8)
                {
                    if (hit < 0 || enemy_z[i] < enemy_z[hit])
                    {
                        hit = i;
                    }
                }
            }
        }
        if (hit >= 0)
        {
            enemy_active[hit] = 0;
            score = score + 100;
            reset_enemy(hit, 180 + (gfx_random() & 127));
        }
        laser_heat = laser_heat + 18;
        if (laser_heat > 100)
        {
            laser_heat = 100;
        }
        laser_cooldown = 8;
        flash_timer = 2;
    }
}

/* Advance hazards and enemies, applying shield damage on close passes. */
void update_world(void)
{
    int i;
    int impact;

    for (i = 0; i < HAZARDS; i++)
    {
        if (i == 3)
        {
            /* Keep the moon at the planet's depth while it orbits the planet. */
            hz_z[i] = hz_z[2];
            hz_phase[i] = (hz_phase[i] + 1) & 31;
            hz_x[i] = hz_x[2] + (sine[hz_phase[i]] * 28) / 100;
            hz_y[i] = hz_y[2] + (sine[(hz_phase[i] + 8) & 31] * 18) / 100;
        }
        else
        {
            hz_z[i] = hz_z[i] - hz_speed[i] - ship_vz / 64;
            if (hz_kind[i] == 0)
            {
                /* Asteroids drift sideways as they approach. */
                hz_phase[i] = (hz_phase[i] + 1) & 31;
                hz_x[i] = hz_x[i] + sine[hz_phase[i]] / 25;
            }
            if (hz_z[i] < 28)
            {
                if (i == 2)
                {
                    reset_hazard(2, 210 + (gfx_random() & 63));
                    hz_kind[2] = 2;
                    hz_z[3] = hz_z[2];
                    hz_x[3] = hz_x[2];
                    hz_y[3] = hz_y[2];
                }
                else
                {
                    reset_hazard(i, 190 + (gfx_random() & 127));
                }
            }
        }
    }

    /* Check impact only when a hazard reaches the near-field collision plane. */
    for (i = 0; i < HAZARDS; i++)
    {
        if (hz_z[i] <= 34)
        {
            impact = 7;
            if (hz_kind[i] == 1)
            {
                impact = 9;
            }
            else if (hz_kind[i] == 2)
            {
                impact = 18;
            }
            if (abs_i(hz_x[i] - ship_x) < impact &&
                abs_i(hz_y[i] - ship_y) < impact)
            {
                shields = shields - 24;
                if (shields < 0)
                {
                    shields = 0;
                }
                flash_timer = 12;
                if (i == 2)
                {
                    reset_hazard(2, 210 + (gfx_random() & 63));
                    hz_kind[2] = 2;
                    hz_z[3] = hz_z[2];
                }
                else if (i == 3)
                {
                    /* Kick the moon to the far side of its orbit after impact. */
                    hz_phase[3] = (hz_phase[3] + 16) & 31;
                }
                else
                {
                    reset_hazard(i, 190 + (gfx_random() & 127));
                }
            }
        }
    }

    for (i = 0; i < ENEMIES; i++)
    {
        if (enemy_active[i])
        {
            enemy_z[i] = enemy_z[i] - 2 - ship_vz / 64;
            if (enemy_z[i] < 26)
            {
                if (abs_i(enemy_x[i] - ship_x) < 10 &&
                    abs_i(enemy_y[i] - ship_y) < 8)
                {
                    shields = shields - 14;
                    if (shields < 0)
                    {
                        shields = 0;
                    }
                    flash_timer = 10;
                }
                reset_enemy(i, 150 + (gfx_random() & 127));
            }
        }
    }

    if (shields == 0)
    {
        game_over = 1;
    }
}

/* Draw a restrained horizon and a few converging lines for depth cues. */
void draw_space_grid(void)
{
    gfx_line(24, VIEW_CY, 296, VIEW_CY, DARK_GRAY);
    gfx_line(24, VIEW_BOTTOM, VIEW_CX, VIEW_CY, DARK_GRAY);
    gfx_line(296, VIEW_BOTTOM, VIEW_CX, VIEW_CY, DARK_GRAY);
}

/* Draw moving stars as small perspective points in the flight path. */
void draw_stars(void)
{
    int i;
    int x;
    int y;
    int size;

    for (i = 0; i < STARS; i++)
    {
        star_z[i] = star_z[i] - 1 - ship_vz / 64;
        if (star_z[i] < 24)
        {
            star_x[i] = (gfx_random() % 181) - 90;
            star_y[i] = (gfx_random() % 101) - 50;
            star_z[i] = 220 + (gfx_random() & 63);
        }
        x = project_x(star_x[i], star_z[i]);
        y = project_y(star_y[i], star_x[i], star_z[i]);
        size = 1;
        if (star_z[i] < 80)
        {
            size = 2;
        }
        if (x > 15 && x < 304 && y > VIEW_TOP && y < VIEW_BOTTOM)
        {
            gfx_rect(x, y, size, size, CYAN);
        }
    }
}

/* Draw an asteroid as a scaled wireframe shape. */
void draw_asteroid(int i)
{
    int x;
    int y;
    int size;
    int j;
    char points[16];

    x = project_x(hz_x[i], hz_z[i]);
    y = project_y(hz_y[i], hz_x[i], hz_z[i]);
    size = 60 / hz_z[i];
    if (size < 1)
    {
        size = 1;
    }
    if (size > 4)
    {
        size = 4;
    }
    for (j = 0; j < 16; j++)
    {
        points[j] = rock_shape[j] * size;
    }
    gfx_poly(x, y, 8, points, GRAY);
}

/* Draw the planet and orbiting moon at their current projected positions. */
void draw_planet_system(void)
{
    int x;
    int y;
    int moon_x;
    int moon_y;
    int radius;
    int i;
    char orb[32];
    char moon[16];

    x = project_x(hz_x[2], hz_z[2]);
    y = project_y(hz_y[2], hz_x[2], hz_z[2]);
    radius = 900 / hz_z[2];
    if (radius < 3)
    {
        radius = 3;
    }
    if (radius > 24)
    {
        radius = 24;
    }
    for (i = 0; i < 16; i++)
    {
        orb[i * 2] = (sine[(i * 2 + 8) & 31] * radius) / 100;
        orb[i * 2 + 1] = (sine[i * 2] * radius) / 100;
    }
    for (i = 0; i < 8; i++)
    {
        moon[i * 2] = (sine[(i * 4 + 8) & 31] * 3) / 100;
        moon[i * 2 + 1] = (sine[i * 4] * 3) / 100;
    }
    gfx_poly(x, y, 16, orb, BLUE);
    gfx_line(x - radius / 2, y, x + radius / 2, y, DARK_GRAY);

    moon_x = project_x(hz_x[3], hz_z[3]);
    moon_y = project_y(hz_y[3], hz_x[3], hz_z[3]);
    gfx_poly(moon_x, moon_y, 8, moon, WHITE);
}

/* Draw an enemy as a small pointed fighter silhouette. */
void draw_enemy(int i)
{
    int x;
    int y;
    int size;

    x = project_x(enemy_x[i], enemy_z[i]);
    y = project_y(enemy_y[i], enemy_x[i], enemy_z[i]);
    size = 240 / enemy_z[i];
    if (size < 2)
    {
        size = 2;
    }
    if (size > 12)
    {
        size = 12;
    }
    gfx_line(x, y - size, x - size, y + size / 2, RED);
    gfx_line(x, y - size, x + size, y + size / 2, RED);
    gfx_line(x - size, y + size / 2, x + size, y + size / 2, ORANGE);
    gfx_line(x - size / 2, y, x + size / 2, y, YELLOW);
}

/* Render a compact targeting reticle and the player's ship silhouette. */
void draw_cockpit(void)
{
    int pulse;

    pulse = 5;
    if (flash_timer > 0)
    {
        pulse = 8;
    }
    gfx_line(VIEW_CX - pulse, VIEW_CY, VIEW_CX - 2, VIEW_CY, GREEN);
    gfx_line(VIEW_CX + 2, VIEW_CY, VIEW_CX + pulse, VIEW_CY, GREEN);
    gfx_line(VIEW_CX, VIEW_CY - pulse, VIEW_CX, VIEW_CY - 2, GREEN);
    gfx_line(VIEW_CX, VIEW_CY + 2, VIEW_CX, VIEW_CY + pulse, GREEN);

    /* Ship nose and wings frame the bottom edge of the forward view. */
    gfx_line(160, 192, 142, 211, CYAN);
    gfx_line(160, 192, 178, 211, CYAN);
    gfx_line(142, 211, 160, 205, CYAN);
    gfx_line(178, 211, 160, 205, CYAN);
}

/* Draw score, shield/heat meters, and contextual status messages. */
void draw_hud(void)
{
    char text[8];
    int width;

    gfx_text(8, 7, "ELITE // DEEP SPACE", CYAN);
    gfx_text(218, 7, "MK-I FIGHTER", GRAY);

    gfx_text(8, 220, "SHLD", GRAY);
    gfx_rect(38, 220, 82, 8, DARK_GRAY);
    width = shields;
    if (width > 100)
    {
        width = 100;
    }
    gfx_rect(39, 221, width * 80 / 100, 6, GREEN);

    gfx_text(129, 220, "HEAT", GRAY);
    gfx_rect(157, 220, 68, 8, DARK_GRAY);
    width = laser_heat * 66 / 100;
    if (width > 0)
    {
        gfx_rect(158, 221, width, 6, ORANGE);
    }
    gfx_text(235, 220, "THR", GRAY);
    gfx_rect(260, 220, 48, 8, DARK_GRAY);
    if (throttle > 0)
    {
        gfx_rect(261, 221, throttle * 6, 6, CYAN);
    }

    gfx_text(8, 231, "SCORE", GRAY);
    number_text(score, text);
    gfx_text(44, 231, text, WHITE);
    gfx_text(108, 231, "SPD", GRAY);
    number_text(abs_i(ship_vz) / 16, text);
    gfx_text(132, 231, text, WHITE);
    if (laser_heat >= 96)
    {
        gfx_text(190, 231, "LASER HOT", RED);
    }
    else
    {
        gfx_text(190, 231, "SPACE: FIRE", GRAY);
    }
}

/* Draw one complete frame of stars, hazards, enemies, cockpit, and HUD. */
void draw_scene(void)
{
    int i;

    gfx_clear(BLACK);
    draw_space_grid();
    draw_stars();
    for (i = 0; i < HAZARDS; i++)
    {
        if (hz_kind[i] == 0)
        {
            draw_asteroid(i);
        }
    }
    draw_planet_system();
    for (i = 0; i < ENEMIES; i++)
    {
        if (enemy_active[i])
        {
            draw_enemy(i);
        }
    }
    draw_cockpit();
    draw_hud();

    if (flash_timer > 0 && (frame & 2))
    {
        gfx_text(119, 184, "SHIELDS HIT", RED);
    }
    if (game_over)
    {
        gfx_rect(68, 90, 184, 54, BLACK);
        gfx_text(126, 99, "SHIP LOST", RED);
        gfx_text(91, 116, "ENTER TO RESTART", WHITE);
    }
}

/* Run the controls, simulation, and rendering once per emulated frame. */
int main(void)
{
    unsigned int keys;

    new_game();
    while (1)
    {
        keys = gfx_keys();
        if (game_over)
        {
            if (keys & (KEY_START | KEY_FIRE))
            {
                new_game();
            }
        }
        else
        {
            update_player(keys);
            update_world();
        }
        draw_scene();
        gfx_present();
        frame++;
    }
    return 0;
}
