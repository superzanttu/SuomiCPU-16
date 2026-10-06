// Elite-style 3D space combat prototype for the SC-16.
// ICON
// .......##.......
// ......####......
// .....##..##.....
// ....##....##....
// ...##.####.##...
// ..##..####..##..
// .##...####...##.
// ##....####....##
// ....########....
// ...##......##...
// ..##........##..
// .##..........##.
// ##............##
// ................
// ................
// ................
//
// You fly a small fighter around a large planet with two orbiting moons,
// drifting asteroids and patrolling enemy ships. Only flying and fighting.
//
// Controls
//   Left/Right (A/D)  yaw          Up/Down (W/S)  pitch (Up = nose up)
//   Q / E             roll         X or Shift     thrust
//   Z or Ctrl         reverse      Space          laser
//   Enter / Space     restart after the ship is destroyed
//
// Architecture: like the original Elite the "camera" never moves. Every
// object is stored relative to the ship (x right, y up, z forward). Each frame
// all objects are rotated by the ship's turn and shifted by minus its velocity.
// The ship's own velocity is a vector in the same space, so it is rotated too,
// which is what gives the ship inertia when the nose turns.
//
// Sections: 1 constants and state, 2 math, 3 spawning, 4 effects and damage,
// 5 flight and physics, 6 celestial bodies, 7 asteroids, 8 enemy AI,
// 9 combat, 10 collisions, 11 rendering, 12 HUD, 13 main loop.
#include "suomi_gfx.h"

/* ---- 1. Constants and state ------------------------------------------ */

/* Slot layout of the shared object arrays. */
#define NSLOT 41
#define S_SHIP 0 /* ship velocity vector, 1/16 units per frame */
#define S_PLANET 1
#define S_MOON 2   /* two moons: slots 2 and 3 */
#define S_BASE_A 4 /* orbit plane basis vectors (rotate with the world) */
#define S_BASE_B 5
#define S_AST 6   /* six asteroid positions */
#define S_ASTV 12 /* six asteroid drift velocities */
#define S_EN 18   /* three enemy positions */
#define S_ENV 21  /* three enemy velocities */
#define S_EXP 24  /* three explosion positions */
#define S_DUST 27 /* fourteen dust particles */
#define NAST 6
#define NEN 3
#define NEXP 3
#define NDUST 14
#define NMOON 2

#define FOCAL 90
#define VCX 160
#define VCY 106
#define NEAR_Z 16
#define PLANET_R 320
#define PLAYER_R 14
#define ENEMY_R 24
#define AST_R 30
#define MAX_SPEED 320

#define YAW_MAX 8
#define PITCH_MAX 7
#define ROLL_MAX 12

/* Enemy AI states. */
#define EN_DEAD 0
#define EN_ATTACK 1
#define EN_BREAK 2
#define EN_FLEE 3
#define EN_PATROL 4

/* Object positions (x right, y up, z forward) relative to the ship. */
int ox[NSLOT];
int oy[NSLOT];
int oz[NSLOT];

/* Sine table: 32 steps per turn, scaled by 64. */
int sine[32] = {
    0, 12, 24, 36, 45, 53, 59, 63,
    64, 63, 59, 53, 45, 36, 24, 12,
    0, -12, -24, -36, -45, -53, -59, -63,
    -64, -63, -59, -53, -45, -36, -24, -12};

/* Irregular asteroid outline: eight (dx, dy) pairs, about 11 units wide. */
char rock_shape[16] = {
    0, -9, 7, -7, 11, -1, 7, 7,
    1, 10, -6, 8, -10, 3, -8, -5};

/* Ship state. */
int yaw_rate;
int pitch_rate;
int roll_rate;
int thrust_mode;
int engine_snd;
int shields;
int hull;
int heat;
int overheated;
int laser_cd;
int beam;
int hit_flash;
int shield_cd;
int hit_cool;
int dmg_flash;
int dead;
int dead_t;
int score;
int kills;
int frame;

/* Celestial bodies. */
int moon_ph[NMOON];
int moon_step[NMOON];
int moon_orbit[NMOON];
int moon_r[NMOON];

/* Asteroids. */
int ast_hp[NAST];

/* Enemies. */
int en_state[NEN];
int en_hp[NEN];
int en_cd[NEN];
int en_beam[NEN];
int en_timer[NEN];
int en_pvx[NEN];
int en_pvy[NEN];
int en_pvz[NEN];

/* Explosions. */
int exp_t[NEXP];
int exp_big[NEXP];

/* Projection results written by project(). */
int psx;
int psy;

/* Draw order scratch array. */
int order[12];

/* ---- 2. Math helpers -------------------------------------------------- */

/* Absolute value. */
int abs_i(int v)
{
    if (v < 0)
    {
        return -v;
    }
    return v;
}

/* Larger of two values. */
int max_i(int a, int b)
{
    if (a > b)
    {
        return a;
    }
    return b;
}

/* Limit v to the range lo..hi. */
int clamp_i(int v, int lo, int hi)
{
    if (v < lo)
    {
        return lo;
    }
    if (v > hi)
    {
        return hi;
    }
    return v;
}

/* Random number 0..n-1 using both halves of a 16-bit draw. */
int rnd(int n)
{
    unsigned int r;

    r = (gfx_random() << 8) + gfx_random();
    return r % n;
}

/* Cheap 3D length: the largest component plus 3/8 of the others. */
int dist3(int x, int y, int z)
{
    int ax;
    int ay;
    int az;
    int m;

    ax = abs_i(x);
    ay = abs_i(y);
    az = abs_i(z);
    m = max_i(ax, max_i(ay, az));
    return m + (((ax + ay + az - m) * 3) >> 3);
}

/* Cheap 2D length used for aiming. */
int dist2(int x, int y)
{
    int ax;
    int ay;
    int m;

    ax = abs_i(x);
    ay = abs_i(y);
    m = max_i(ax, ay);
    return m + (((ax + ay - m) * 3) >> 3);
}

/* Sine of a phase 0..511 (one turn), linearly interpolated. */
int sin_ph(int ph)
{
    int i;
    int f;
    int a;
    int b;

    i = ph >> 4;
    f = ph & 15;
    a = sine[i & 31];
    b = sine[(i + 1) & 31];
    return a + (((b - a) * f) >> 4);
}

/* Cosine of a phase 0..511. */
int cos_ph(int ph)
{
    return sin_ph(ph + 128);
}

/* Small-angle rotation term b*s/256, safe from 16-bit overflow. */
int tmul(int b, int s)
{
    if (b > 1400 || b < -1400)
    {
        return (((b >> 2) * s) + 32) >> 6;
    }
    return ((b * s) + 128) >> 8;
}

/* Divide a world offset by depth with overflow protection; result in pixels. */
int scale_div(int a, int z)
{
    int r;

    a = clamp_i(a, -3600, 3600);
    if (a > 350 || a < -350)
    {
        r = ((a >> 4) * FOCAL) / (z >> 4);
    }
    else
    {
        r = (a * FOCAL) / z;
    }
    return clamp_i(r, -400, 400);
}

/* Project a camera-space point to the screen (needs z >= NEAR_Z). */
void project(int x, int y, int z)
{
    psx = VCX + scale_div(x, z);
    psy = VCY - scale_div(y, z);
}

/* Move v toward target by at most step. */
int approach(int v, int target, int step)
{
    if (v < target)
    {
        v = v + step;
        if (v > target)
        {
            v = target;
        }
    }
    else if (v > target)
    {
        v = v - step;
        if (v < target)
        {
            v = target;
        }
    }
    return v;
}

/* Reduce |v| by about |v| / 2^sh, rounding away from zero so it reaches 0. */
int drag(int v, int sh)
{
    int mask;

    mask = (1 << sh) - 1;
    if (v > 0)
    {
        return v - ((v + mask) >> sh);
    }
    if (v < 0)
    {
        return v + (((-v) + mask) >> sh);
    }
    return 0;
}

/* Convert an unsigned value to decimal text. */
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

/* ---- 3. Spawning ------------------------------------------------------ */

/* Place slot s on a shell about 1500-1900 units away in a random direction. */
void spawn_far(int s)
{
    int axis;
    int sign;

    ox[s] = rnd(2001) - 1000;
    oy[s] = rnd(1201) - 600;
    oz[s] = rnd(2001) - 1000;
    axis = rnd(3);
    sign = rnd(2) * 2 - 1;
    if (axis == 0)
    {
        ox[s] = sign * (1500 + rnd(400));
    }
    else if (axis == 1)
    {
        oy[s] = sign * (1500 + rnd(400));
    }
    else
    {
        oz[s] = sign * (1500 + rnd(400));
    }
}

/* (Re)create asteroid i: far away when far is set, otherwise anywhere nearby. */
void spawn_asteroid(int i, int far)
{
    int s;
    int v;

    s = S_AST + i;
    v = S_ASTV + i;
    if (far)
    {
        spawn_far(s);
    }
    else
    {
        ox[s] = rnd(2401) - 1200;
        oy[s] = rnd(1401) - 700;
        oz[s] = rnd(2401) - 1200;
        if (dist3(ox[s], oy[s], oz[s]) < 350)
        {
            oz[s] = oz[s] + 700;
        }
    }
    ox[v] = rnd(49) - 24;
    oy[v] = rnd(49) - 24;
    oz[v] = rnd(49) - 24;
    ast_hp[i] = 20;
}

/* (Re)create enemy i as a patrolling ship far away. */
void spawn_enemy(int i)
{
    int v;

    spawn_far(S_EN + i);
    v = S_ENV + i;
    ox[v] = 0;
    oy[v] = 0;
    oz[v] = 0;
    en_state[i] = EN_PATROL;
    en_hp[i] = 30;
    en_cd[i] = 30 + rnd(30);
    en_beam[i] = 0;
    en_timer[i] = 0;
}

/* Reset the whole game. */
void new_game(void)
{
    int i;

    for (i = 0; i < NSLOT; i++)
    {
        ox[i] = 0;
        oy[i] = 0;
        oz[i] = 0;
    }
    ox[S_PLANET] = 250;
    oy[S_PLANET] = -120;
    oz[S_PLANET] = 1700;
    ox[S_BASE_A] = 192;
    oy[S_BASE_B] = 72;
    oz[S_BASE_B] = 178;
    moon_ph[0] = 0;
    moon_ph[1] = 256;
    moon_step[0] = 2;
    moon_step[1] = -1;
    moon_orbit[0] = 80;
    moon_orbit[1] = 120;
    moon_r[0] = 50;
    moon_r[1] = 70;
    for (i = 0; i < NAST; i++)
    {
        spawn_asteroid(i, 0);
    }
    for (i = 0; i < NEN; i++)
    {
        spawn_enemy(i);
    }
    for (i = 0; i < NEXP; i++)
    {
        exp_t[i] = 0;
    }
    for (i = 0; i < NDUST; i++)
    {
        ox[S_DUST + i] = rnd(501) - 250;
        oy[S_DUST + i] = rnd(501) - 250;
        oz[S_DUST + i] = rnd(501) - 250;
    }
    yaw_rate = 0;
    pitch_rate = 0;
    roll_rate = 0;
    thrust_mode = 0;
    shields = 100;
    hull = 100;
    heat = 0;
    overheated = 0;
    laser_cd = 0;
    beam = 0;
    hit_flash = 0;
    shield_cd = 0;
    hit_cool = 0;
    dmg_flash = 0;
    dead = 0;
    dead_t = 0;
    score = 0;
    kills = 0;
    frame = 0;
}

/* ---- 4. Effects and damage ------------------------------------------- */

/* Start an explosion at a camera-space point; big ones are larger. */
void add_explosion(int x, int y, int z, int big)
{
    int i;
    int pick;

    pick = 0;
    for (i = NEXP - 1; i >= 0; i--)
    {
        if (exp_t[i] == 0)
        {
            pick = i;
        }
    }
    exp_t[pick] = 16;
    exp_big[pick] = big;
    gfx_sound(1, 260 - big * 120, 14 + big * 14, WAVE_NOISE, 60 + big * 25);
    ox[S_EXP + pick] = x;
    oy[S_EXP + pick] = y;
    oz[S_EXP + pick] = z;
}

/* The player's ship is destroyed. */
void player_dies(void)
{
    hull = 0;
    dead = 1;
    dead_t = 0;
    add_explosion(0, 0, 80, 1);
    gfx_sound(1, 70, 70, WAVE_NOISE, 100);
    gfx_sound(0, 0, 0, 0, 0);
    engine_snd = 0;
}

/* Apply damage to the player: shields absorb first, then the hull. */
void damage_player(int n)
{
    shield_cd = 16;
    dmg_flash = 6;
    gfx_sound(3, 180, 8, WAVE_NOISE, 70);
    if (shields >= n)
    {
        shields = shields - n;
    }
    else
    {
        n = n - shields;
        shields = 0;
        hull = hull - n;
        if (hull <= 0)
        {
            player_dies();
        }
    }
}

/* Destroy enemy i; the player scores when by_player is set. */
void kill_enemy(int i, int by_player)
{
    int s;

    s = S_EN + i;
    add_explosion(ox[s], oy[s], oz[s], 1);
    en_state[i] = EN_DEAD;
    en_timer[i] = 40 + rnd(80);
    spawn_far(s);
    if (by_player)
    {
        kills++;
        score = score + 100;
    }
}

/* Damage enemy i and destroy it when its hit points run out. */
void hurt_enemy(int i, int dmg, int by_player)
{
    en_hp[i] = en_hp[i] - dmg;
    if (en_hp[i] <= 0)
    {
        kill_enemy(i, by_player);
    }
}

/* Destroy asteroid i and respawn it far away. */
void kill_asteroid(int i, int by_player)
{
    int s;

    s = S_AST + i;
    add_explosion(ox[s], oy[s], oz[s], 0);
    if (by_player)
    {
        score = score + 25;
    }
    spawn_asteroid(i, 1);
}

/* Advance timers: explosions, beams, shield regeneration and laser cooling. */
void update_effects(void)
{
    int i;

    for (i = 0; i < NEXP; i++)
    {
        if (exp_t[i] > 0)
        {
            exp_t[i] = exp_t[i] - 1;
        }
    }
    if (beam > 0)
    {
        beam--;
    }
    if (hit_flash > 0)
    {
        hit_flash--;
    }
    if (dmg_flash > 0)
    {
        dmg_flash--;
    }
    if (laser_cd > 0)
    {
        laser_cd--;
    }
    if (heat > 0)
    {
        heat--;
    }
    if (overheated && heat <= 40)
    {
        overheated = 0;
    }
    if (shield_cd > 0)
    {
        shield_cd--;
    }
    else if (shields < 100 && (frame & 1) == 0)
    {
        shields++;
    }
}

/* ---- 5. Flight controls and physics ---------------------------------- */

/* Read the keys and update turn rates, thrust, drag and the planet pull. */
void update_controls(unsigned int keys, unsigned int kx)
{
    int t;
    int i;
    int s;

    t = 0;
    if (keys & KEY_RIGHT)
    {
        t = t + YAW_MAX;
    }
    if (keys & KEY_LEFT)
    {
        t = t - YAW_MAX;
    }
    yaw_rate = approach(yaw_rate, t, 2);
    t = 0;
    if (keys & KEY_UP)
    {
        t = t + PITCH_MAX;
    }
    if (keys & KEY_DOWN)
    {
        t = t - PITCH_MAX;
    }
    pitch_rate = approach(pitch_rate, t, 2);
    t = 0;
    if (kx & KEYX_ROLL_RIGHT)
    {
        t = t + ROLL_MAX;
    }
    if (kx & KEYX_ROLL_LEFT)
    {
        t = t - ROLL_MAX;
    }
    roll_rate = approach(roll_rate, t, 3);

    thrust_mode = 0;
    if (kx & KEYX_THRUST)
    {
        oz[S_SHIP] = oz[S_SHIP] + 7;
        thrust_mode = 1;
    }
    else if (kx & KEYX_REVERSE)
    {
        oz[S_SHIP] = oz[S_SHIP] - 5;
        thrust_mode = 2;
    }
    /* Light drag along the nose, stronger sideways (flight assist). */
    /* Engine rumble loops while thrusting; restart it only when the mode changes */
    if (thrust_mode != engine_snd)
    {
        engine_snd = thrust_mode;
        if (thrust_mode == 1)
        {
            gfx_sound(0, 80, 0, WAVE_NOISE, 30);
        }
        else if (thrust_mode == 2)
        {
            gfx_sound(0, 50, 0, WAVE_NOISE, 18);
        }
        else
        {
            gfx_sound(0, 0, 0, 0, 0);
        }
    }
    ox[S_SHIP] = drag(ox[S_SHIP], 6);
    oy[S_SHIP] = drag(oy[S_SHIP], 6);
    oz[S_SHIP] = drag(oz[S_SHIP], 8);
    ox[S_SHIP] = clamp_i(ox[S_SHIP], -MAX_SPEED, MAX_SPEED);
    oy[S_SHIP] = clamp_i(oy[S_SHIP], -MAX_SPEED, MAX_SPEED);
    oz[S_SHIP] = clamp_i(oz[S_SHIP], -MAX_SPEED, MAX_SPEED);

    /* Gentle pull keeps the planet system within reach. */
    for (i = 0; i < 3; i++)
    {
        if (i == 0)
        {
            s = ox[S_PLANET];
        }
        else if (i == 1)
        {
            s = oy[S_PLANET];
        }
        else
        {
            s = oz[S_PLANET];
        }
        if (s > 3300)
        {
            if (i == 0)
            {
                ox[S_SHIP] = ox[S_SHIP] + 8;
            }
            if (i == 1)
            {
                oy[S_SHIP] = oy[S_SHIP] + 8;
            }
            if (i == 2)
            {
                oz[S_SHIP] = oz[S_SHIP] + 8;
            }
        }
        if (s < -3300)
        {
            if (i == 0)
            {
                ox[S_SHIP] = ox[S_SHIP] - 8;
            }
            if (i == 1)
            {
                oy[S_SHIP] = oy[S_SHIP] - 8;
            }
            if (i == 2)
            {
                oz[S_SHIP] = oz[S_SHIP] - 8;
            }
        }
    }
}

/* Rotate every slot by the ship's yaw, pitch and roll for this frame. */
void rotate_all(void)
{
    int i;
    int u;
    int v;

    if (yaw_rate != 0)
    {
        for (i = 0; i < NSLOT; i++)
        {
            u = ox[i];
            v = oz[i];
            u = u - tmul(v, yaw_rate);
            v = v + tmul(u, yaw_rate);
            ox[i] = u;
            oz[i] = v;
        }
    }
    if (pitch_rate != 0)
    {
        for (i = 0; i < NSLOT; i++)
        {
            u = oy[i];
            v = oz[i];
            u = u - tmul(v, pitch_rate);
            v = v + tmul(u, pitch_rate);
            oy[i] = u;
            oz[i] = v;
        }
    }
    if (roll_rate != 0)
    {
        for (i = 0; i < NSLOT; i++)
        {
            u = ox[i];
            v = oy[i];
            u = u - tmul(v, roll_rate);
            v = v + tmul(u, roll_rate);
            ox[i] = u;
            oy[i] = v;
        }
    }
}

/* Shift world objects by minus the ship velocity (and add their own drift). */
void move_world(void)
{
    int i;
    int sx;
    int sy;
    int sz;
    int a;
    int b;

    sx = ox[S_SHIP];
    sy = oy[S_SHIP];
    sz = oz[S_SHIP];
    ox[S_PLANET] = ox[S_PLANET] - ((sx + 8) >> 4);
    oy[S_PLANET] = oy[S_PLANET] - ((sy + 8) >> 4);
    oz[S_PLANET] = oz[S_PLANET] - ((sz + 8) >> 4);
    for (i = 0; i < NAST; i++)
    {
        a = S_AST + i;
        b = S_ASTV + i;
        ox[a] = ox[a] - ((sx - ox[b] + 8) >> 4);
        oy[a] = oy[a] - ((sy - oy[b] + 8) >> 4);
        oz[a] = oz[a] - ((sz - oz[b] + 8) >> 4);
    }
    for (i = 0; i < NEN; i++)
    {
        a = S_EN + i;
        b = S_ENV + i;
        ox[a] = ox[a] - ((sx - ox[b] + 8) >> 4);
        oy[a] = oy[a] - ((sy - oy[b] + 8) >> 4);
        oz[a] = oz[a] - ((sz - oz[b] + 8) >> 4);
    }
    for (i = 0; i < NEXP; i++)
    {
        a = S_EXP + i;
        ox[a] = ox[a] - ((sx + 8) >> 4);
        oy[a] = oy[a] - ((sy + 8) >> 4);
        oz[a] = oz[a] - ((sz + 8) >> 4);
    }
    for (i = 0; i < NDUST; i++)
    {
        a = S_DUST + i;
        ox[a] = ox[a] - ((sx + 8) >> 4);
        oy[a] = oy[a] - ((sy + 8) >> 4);
        oz[a] = oz[a] - ((sz + 8) >> 4);
        if (ox[a] > 250)
        {
            ox[a] = ox[a] - 500;
        }
        if (ox[a] < -250)
        {
            ox[a] = ox[a] + 500;
        }
        if (oy[a] > 250)
        {
            oy[a] = oy[a] - 500;
        }
        if (oy[a] < -250)
        {
            oy[a] = oy[a] + 500;
        }
        if (oz[a] > 250)
        {
            oz[a] = oz[a] - 500;
        }
        if (oz[a] < -250)
        {
            oz[a] = oz[a] + 500;
        }
    }
}

/* ---- 6. Celestial bodies ---------------------------------------------- */

/* Advance each moon along its orbit around the planet. */
void update_moons(void)
{
    int m;
    int c;
    int s;
    int ph;
    int o;
    int p;

    p = S_PLANET;
    for (m = 0; m < NMOON; m++)
    {
        moon_ph[m] = (moon_ph[m] + moon_step[m]) & 511;
        ph = moon_ph[m];
        c = cos_ph(ph);
        s = sin_ph(ph);
        o = moon_orbit[m];
        ox[S_MOON + m] = ox[p] + ((((c * ox[S_BASE_A]) + (s * ox[S_BASE_B])) >> 6) * o) / 30;
        oy[S_MOON + m] = oy[p] + ((((c * oy[S_BASE_A]) + (s * oy[S_BASE_B])) >> 6) * o) / 30;
        oz[S_MOON + m] = oz[p] + ((((c * oz[S_BASE_A]) + (s * oz[S_BASE_B])) >> 6) * o) / 30;
    }
}

/* ---- 7. Asteroids ------------------------------------------------------ */

/* Respawn asteroids that drifted too far away. */
void update_asteroids(void)
{
    int i;
    int s;

    for (i = 0; i < NAST; i++)
    {
        s = S_AST + i;
        if (abs_i(ox[s]) > 2600 || abs_i(oy[s]) > 2600 || abs_i(oz[s]) > 2600)
        {
            spawn_asteroid(i, 1);
        }
    }
}

/* ---- 8. Enemy AI -------------------------------------------------------- */

/* Steer enemy velocity component toward a target by at most 14 per frame. */
int steer(int v, int target)
{
    return v + clamp_i(target - v, -14, 14);
}

/* Patrol, attack, break away, flee and shoot for every enemy. */
void update_enemies(void)
{
    int i;
    int s;
    int v;
    int x;
    int y;
    int z;
    int d;
    int dd;
    int tx;
    int ty;
    int tz;
    int err;

    for (i = 0; i < NEN; i++)
    {
        s = S_EN + i;
        v = S_ENV + i;
        if (en_beam[i] > 0)
        {
            en_beam[i] = en_beam[i] - 1;
        }
        if (en_cd[i] > 0)
        {
            en_cd[i] = en_cd[i] - 1;
        }
        if (en_state[i] == EN_DEAD)
        {
            en_timer[i] = en_timer[i] - 1;
            if (en_timer[i] <= 0)
            {
                spawn_enemy(i);
            }
        }
        else
        {
            x = ox[s];
            y = oy[s];
            z = oz[s];
            d = dist3(x, y, z);
            dd = d >> 3;
            if (dd < 1)
            {
                dd = 1;
            }
            /* Direction toward the player scaled to a speed of about 120. */
            tx = -((x >> 3) * 120) / dd;
            ty = -((y >> 3) * 120) / dd;
            tz = -((z >> 3) * 120) / dd;
            if (en_hp[i] <= 10 && en_state[i] != EN_FLEE)
            {
                en_state[i] = EN_FLEE;
            }
            if (en_state[i] == EN_PATROL)
            {
                en_timer[i] = en_timer[i] - 1;
                if (en_timer[i] <= 0)
                {
                    en_timer[i] = 60 + rnd(60);
                    en_pvx[i] = rnd(141) - 70;
                    en_pvy[i] = rnd(141) - 70;
                    en_pvz[i] = rnd(141) - 70;
                }
                tx = en_pvx[i];
                ty = en_pvy[i];
                tz = en_pvz[i];
                if (d < 1100)
                {
                    en_state[i] = EN_ATTACK;
                }
            }
            else if (en_state[i] == EN_ATTACK)
            {
                if (d < 170)
                {
                    en_state[i] = EN_BREAK;
                    en_timer[i] = 24;
                    ox[v] = ox[v] + rnd(121) - 60;
                    oy[v] = oy[v] + rnd(121) - 60;
                }
                else if (d > 1600)
                {
                    en_state[i] = EN_PATROL;
                    en_timer[i] = 0;
                }
            }
            else if (en_state[i] == EN_BREAK)
            {
                tx = -tx;
                ty = -ty;
                tz = -tz;
                en_timer[i] = en_timer[i] - 1;
                if (en_timer[i] <= 0)
                {
                    en_state[i] = EN_ATTACK;
                }
            }
            else
            {
                tx = -tx;
                ty = -ty;
                tz = -tz;
                if (d > 2000)
                {
                    en_state[i] = EN_DEAD;
                    en_timer[i] = 60 + rnd(60);
                    en_hp[i] = 30;
                }
            }
            ox[v] = steer(ox[v], tx);
            oy[v] = steer(oy[v], ty);
            oz[v] = steer(oz[v], tz);

            /* Fire when pointed roughly at the player. */
            if (en_state[i] == EN_ATTACK && d < 800 && en_cd[i] == 0)
            {
                err = abs_i(tx - ox[v]) + abs_i(ty - oy[v]) + abs_i(tz - oz[v]);
                if (err < 60)
                {
                    en_cd[i] = 18 + rnd(20);
                    en_beam[i] = 2;
                    gfx_sound(3, 420, 4, WAVE_SQUARE, 25);
                    if (rnd(100) < 55 && !dead)
                    {
                        damage_player(5);
                    }
                }
            }
        }
    }
}

/* ---- 9. Player combat --------------------------------------------------- */

/* Fire the hitscan laser along the nose and apply damage to what it strikes. */
void laser_hit(void)
{
    int i;
    int s;
    int z;
    int best;
    int kind;
    int bestz;
    int lat;

    best = -1;
    kind = 0;
    bestz = 1600;
    for (i = 0; i < NEN; i++)
    {
        s = S_EN + i;
        z = oz[s];
        if (en_state[i] != EN_DEAD && z > 0 && z < bestz)
        {
            lat = dist2(ox[s], oy[s]);
            if (lat < ENEMY_R + 4 + (z >> 5))
            {
                best = i;
                kind = 1;
                bestz = z;
            }
        }
    }
    for (i = 0; i < NAST; i++)
    {
        s = S_AST + i;
        z = oz[s];
        if (z > 0 && z < bestz)
        {
            lat = dist2(ox[s], oy[s]);
            if (lat < AST_R + (z >> 5))
            {
                best = i;
                kind = 2;
                bestz = z;
            }
        }
    }
    for (i = 0; i < NMOON; i++)
    {
        s = S_MOON + i;
        z = oz[s];
        if (z > 0 && z < bestz)
        {
            lat = dist2(ox[s], oy[s]);
            if (lat < moon_r[i])
            {
                best = i;
                kind = 3;
                bestz = z;
            }
        }
    }
    z = oz[S_PLANET];
    if (z > 0 && z < bestz && dist2(ox[S_PLANET], oy[S_PLANET]) < PLANET_R)
    {
        best = 0;
        kind = 4;
        bestz = z;
    }
    if (kind != 0)
    {
        hit_flash = 3;
        if (kind == 1)
        {
            hurt_enemy(best, 10, 1);
            if (en_state[best] == EN_PATROL)
            {
                en_state[best] = EN_ATTACK;
            }
        }
        if (kind == 2)
        {
            ast_hp[best] = ast_hp[best] - 10;
            if (ast_hp[best] <= 0)
            {
                kill_asteroid(best, 1);
            }
        }
    }
}

/* Handle the fire button: cooldown, heat and overheating. */
void update_combat(unsigned int keys)
{
    if (heat >= 100)
    {
        if (!overheated)
        {
            gfx_sound(2, 220, 20, WAVE_SQUARE, 40);
        }
        overheated = 1;
    }
    if ((keys & KEY_FIRE) && !overheated && laser_cd == 0)
    {
        heat = heat + 9;
        laser_cd = 3;
        beam = 2;
        gfx_sound(2, 1500, 3, WAVE_SQUARE, 35);
        laser_hit();
    }
}

/* ---- 10. Collisions ------------------------------------------------------ */

/* True when slot s is within r of the ship. */
int within(int s, int r)
{
    int x;
    int y;
    int z;

    x = ox[s];
    y = oy[s];
    z = oz[s];
    if (x > r || x < -r || y > r || y < -r || z > r || z < -r)
    {
        return 0;
    }
    return dist3(x, y, z) < r;
}

/* True when slots a and b are within r of each other. */
int near_pair(int a, int b, int r)
{
    int x;
    int y;
    int z;

    x = ox[a] - ox[b];
    y = oy[a] - oy[b];
    z = oz[a] - oz[b];
    if (x > r || x < -r || y > r || y < -r || z > r || z < -r)
    {
        return 0;
    }
    return dist3(x, y, z) < r;
}

/* Bounce the ship away from slot s (velocity pointing away at about 192). */
void bounce(int s)
{
    int dd;

    dd = dist3(ox[s], oy[s], oz[s]) >> 3;
    if (dd < 1)
    {
        dd = 1;
    }
    ox[S_SHIP] = -((ox[s] >> 3) * 192) / dd;
    oy[S_SHIP] = -((oy[s] >> 3) * 192) / dd;
    oz[S_SHIP] = -((oz[s] >> 3) * 192) / dd;
}

/* Test the player and enemies against every solid object. */
void update_collisions(void)
{
    int i;
    int j;
    int speed;

    if (hit_cool > 0)
    {
        hit_cool--;
    }
    speed = dist3(ox[S_SHIP], oy[S_SHIP], oz[S_SHIP]);
    for (i = 0; i < NAST; i++)
    {
        if (within(S_AST + i, AST_R + PLAYER_R))
        {
            if (hit_cool == 0)
            {
                damage_player(6 + speed / 48);
                hit_cool = 12;
            }
            kill_asteroid(i, 0);
        }
    }
    for (i = 0; i < NMOON; i++)
    {
        if (within(S_MOON + i, moon_r[i] + PLAYER_R))
        {
            bounce(S_MOON + i);
            if (hit_cool == 0)
            {
                damage_player(12 + speed / 24);
                hit_cool = 12;
            }
        }
    }
    if (within(S_PLANET, PLANET_R + PLAYER_R))
    {
        bounce(S_PLANET);
        if (hit_cool == 0)
        {
            damage_player(20 + speed / 20);
            hit_cool = 12;
        }
    }
    for (i = 0; i < NEN; i++)
    {
        if (en_state[i] == EN_DEAD)
        {
            continue;
        }
        if (within(S_EN + i, ENEMY_R + PLAYER_R))
        {
            bounce(S_EN + i);
            if (hit_cool == 0)
            {
                damage_player(10);
                hit_cool = 12;
            }
            hurt_enemy(i, 15, 0);
            if (en_state[i] == EN_DEAD)
            {
                continue;
            }
        }
        for (j = 0; j < NAST; j++)
        {
            if (en_state[i] != EN_DEAD && near_pair(S_EN + i, S_AST + j, ENEMY_R + AST_R))
            {
                kill_asteroid(j, 0);
                kill_enemy(i, 0);
            }
        }
        for (j = 0; j < NMOON; j++)
        {
            if (en_state[i] != EN_DEAD && near_pair(S_EN + i, S_MOON + j, ENEMY_R + moon_r[j]))
            {
                kill_enemy(i, 0);
            }
        }
        if (en_state[i] != EN_DEAD && near_pair(S_EN + i, S_PLANET, ENEMY_R + PLANET_R))
        {
            kill_enemy(i, 0);
        }
    }
    for (i = 0; i < NEN; i++)
    {
        for (j = i + 1; j < NEN; j++)
        {
            if (en_state[i] != EN_DEAD && en_state[j] != EN_DEAD && near_pair(S_EN + i, S_EN + j, ENEMY_R * 2))
            {
                hurt_enemy(i, 20, 0);
                hurt_enemy(j, 20, 0);
            }
        }
    }
}

/* ---- 11. Rendering -------------------------------------------------------- */

/* Filled disc made of horizontal bands that alternate between two colours. */
void draw_disc(int cx, int cy, int r, int c1, int c2)
{
    int j;
    int y0;
    int y1;
    int hw;
    int hh;
    int col;

    if (r < 2)
    {
        gfx_rect(cx - 1, cy - 1, 2, 2, c1);
        return;
    }
    if (r > 320)
    {
        r = 320;
    }
    for (j = 0; j < 8; j++)
    {
        y0 = (r * sine[j]) >> 6;
        y1 = (r * sine[j + 1]) >> 6;
        hw = (r * sine[8 - j]) >> 6;
        hh = y1 - y0 + 1;
        col = c1;
        if (j & 1)
        {
            col = c2;
        }
        gfx_rect(cx - hw, cy + y0, hw * 2, hh, col);
        gfx_rect(cx - hw, cy - y1, hw * 2, hh, col);
    }
}

/* Draw asteroid i as a scaled rocky outline. */
void draw_rock(int i)
{
    int s;
    int r;
    int j;
    char pts[16];

    s = S_AST + i;
    r = scale_div(AST_R, oz[s]);
    project(ox[s], oy[s], oz[s]);
    if (r < 3)
    {
        gfx_rect(psx - 1, psy - 1, 2, 2, GRAY);
    }
    else if (r > 26)
    {
        draw_disc(psx, psy, r, GRAY, DARK_GRAY);
    }
    else
    {
        for (j = 0; j < 16; j++)
        {
            pts[j] = (rock_shape[j] * r) / 11;
            if ((i & 1) && (j & 1) == 0)
            {
                pts[j] = -pts[j];
            }
        }
        gfx_poly(psx, psy, 8, pts, GRAY);
    }
}

/* Draw a bracket around a target; green when the laser would hit it. */
void draw_bracket(int cx, int cy, int b, int col)
{
    gfx_line(cx - b, cy - b, cx - b + 4, cy - b, col);
    gfx_line(cx - b, cy - b, cx - b, cy - b + 4, col);
    gfx_line(cx + b, cy - b, cx + b - 4, cy - b, col);
    gfx_line(cx + b, cy - b, cx + b, cy - b + 4, col);
    gfx_line(cx - b, cy + b, cx - b + 4, cy + b, col);
    gfx_line(cx - b, cy + b, cx - b, cy + b - 4, col);
    gfx_line(cx + b, cy + b, cx + b - 4, cy + b, col);
    gfx_line(cx + b, cy + b, cx + b, cy + b - 4, col);
}

/* Draw enemy i as a dart pointing along its velocity, plus lock bracket and fire. */
void draw_enemy(int i)
{
    int s;
    int v;
    int x;
    int y;
    int z;
    int rp;
    int cx;
    int cy;
    int nx;
    int ny;
    int tx;
    int ty;
    int w;
    int col;
    int vx;
    int vy;
    int vz;

    s = S_EN + i;
    v = S_ENV + i;
    x = ox[s];
    y = oy[s];
    z = oz[s];
    vx = ox[v] >> 2;
    vy = oy[v] >> 2;
    vz = oz[v] >> 2;
    rp = scale_div(ENEMY_R, z);
    project(x, y, z);
    cx = psx;
    cy = psy;
    if (rp < 3 || z + vz < NEAR_Z || z - vz < NEAR_Z)
    {
        gfx_rect(cx - rp, cy - rp, rp * 2 + 1, rp * 2 + 1, RED);
    }
    else
    {
        project(x + vx, y + vy, z + vz);
        nx = psx;
        ny = psy;
        project(x - vx, y - vy, z - vz);
        tx = psx;
        ty = psy;
        w = rp + (rp >> 1);
        col = RED;
        if (en_state[i] == EN_FLEE)
        {
            col = ORANGE;
        }
        gfx_line(nx, ny, tx - w, ty, col);
        gfx_line(nx, ny, tx + w, ty, col);
        gfx_line(tx - w, ty, tx + w, ty, col);
        gfx_line(nx, ny, tx, ty, col);
        gfx_rect(tx - 1, ty - 1, 3, 3, YELLOW);
    }
    if (z < 1600)
    {
        col = DARK_RED;
        if (dist2(x, y) < ENEMY_R + 4 + (z >> 5))
        {
            col = GREEN;
        }
        draw_bracket(cx, cy, max_i(rp + 5, 8), col);
    }
    if (en_beam[i] > 0)
    {
        gfx_line(cx, cy, VCX + rnd(61) - 30, 192, ORANGE);
    }
}

/* Radial burst of rays: used for explosions and the hit spark. */
void draw_burst(int cx, int cy, int pr, int col, int phase)
{
    int k;
    int a;
    int dx;
    int dy;

    for (k = 0; k < 8; k++)
    {
        a = k * 4 + phase;
        dx = (sine[(a + 8) & 31] * pr) >> 6;
        dy = (sine[a & 31] * pr) >> 6;
        gfx_line(cx + (dx >> 1), cy + (dy >> 1), cx + dx, cy + dy, col);
    }
}

/* Colour of an explosion by age: bright yellow to dark red. */
int exp_color(int t)
{
    if (t > 10)
    {
        return YELLOW;
    }
    if (t > 5)
    {
        return ORANGE;
    }
    return RED;
}

/* Draw explosion i as an expanding ring of rays. */
void draw_explosion(int i)
{
    int s;
    int t;
    int z;
    int sz;
    int pr;

    t = exp_t[i];
    s = S_EXP + i;
    z = oz[s];
    if (t == 0 || z < NEAR_Z)
    {
        return;
    }
    sz = (17 - t) * 6;
    if (exp_big[i])
    {
        sz = (17 - t) * 10;
    }
    pr = clamp_i(scale_div(sz, z), 2, 90);
    project(ox[s], oy[s], z);
    draw_burst(psx, psy, pr, exp_color(t), (i & 1) * 2);
    if (t > 8)
    {
        gfx_rect(psx - (pr >> 2), psy - (pr >> 2), (pr >> 1) + 1, (pr >> 1) + 1, WHITE);
    }
}

/* Draw one sorted object: planet, moon, asteroid or enemy. */
void draw_item(int s)
{
    int z;
    int r;

    z = oz[s];
    if (z < NEAR_Z)
    {
        return;
    }
    if (s == S_PLANET)
    {
        project(ox[s], oy[s], z);
        draw_disc(psx, psy, scale_div(PLANET_R, z), BLUE, CYAN);
    }
    else if (s < S_BASE_A)
    {
        r = s - S_MOON;
        project(ox[s], oy[s], z);
        draw_disc(psx, psy, scale_div(moon_r[r], z), GRAY, WHITE);
    }
    else if (s < S_ASTV)
    {
        draw_rock(s - S_AST);
    }
    else
    {
        draw_enemy(s - S_EN);
    }
}

/* Draw dust particles as streaks that show the ship's motion. */
void draw_dust(void)
{
    int i;
    int s;
    int x;
    int y;
    int z;
    int x0;
    int y0;

    for (i = 0; i < NDUST; i++)
    {
        s = S_DUST + i;
        x = ox[s];
        y = oy[s];
        z = oz[s];
        if (z >= NEAR_Z)
        {
            project(x, y, z);
            x0 = psx;
            y0 = psy;
            project(x + (ox[S_SHIP] >> 3), y + (oy[S_SHIP] >> 3), z + (oz[S_SHIP] >> 3));
            if (z + (oz[S_SHIP] >> 3) >= NEAR_Z)
            {
                gfx_line(x0, y0, psx, psy, DARK_GRAY);
            }
            gfx_pixel(x0, y0, WHITE);
        }
    }
}

/* Depth-sort all solid objects (far to near) and draw them. */
void draw_world(void)
{
    int n;
    int i;
    int j;
    int t;

    n = 0;
    order[n] = S_PLANET;
    n++;
    for (i = 0; i < NMOON; i++)
    {
        order[n] = S_MOON + i;
        n++;
    }
    for (i = 0; i < NAST; i++)
    {
        order[n] = S_AST + i;
        n++;
    }
    for (i = 0; i < NEN; i++)
    {
        if (en_state[i] != EN_DEAD)
        {
            order[n] = S_EN + i;
            n++;
        }
    }
    for (i = 1; i < n; i++)
    {
        t = order[i];
        j = i - 1;
        while (j >= 0 && oz[order[j]] < oz[t])
        {
            order[j + 1] = order[j];
            j--;
        }
        order[j + 1] = t;
    }
    for (i = 0; i < n; i++)
    {
        draw_item(order[i]);
    }
}

/* Cockpit furniture: crosshair, canopy struts and laser beams. */
void draw_cockpit(void)
{
    gfx_line(VCX - 10, VCY, VCX - 4, VCY, GREEN);
    gfx_line(VCX + 4, VCY, VCX + 10, VCY, GREEN);
    gfx_line(VCX, VCY - 10, VCX, VCY - 4, GREEN);
    gfx_line(VCX, VCY + 4, VCX, VCY + 10, GREEN);
    gfx_line(0, 150, 70, 191, DARK_GRAY);
    gfx_line(319, 150, 249, 191, DARK_GRAY);
    if (beam > 0)
    {
        gfx_line(60, 191, VCX, VCY, RED);
        gfx_line(260, 191, VCX, VCY, RED);
    }
    if (hit_flash > 0)
    {
        draw_burst(VCX, VCY, 10 + hit_flash * 3, YELLOW, 0);
    }
}

/* Arrow at the screen edge pointing to the nearest enemy that is off-screen. */
void draw_pointer(void)
{
    int i;
    int s;
    int best;
    int bd;
    int d;
    int dx;
    int dy;
    int ax;
    int ay;
    int px;
    int py;

    best = -1;
    bd = 32000;
    for (i = 0; i < NEN; i++)
    {
        s = S_EN + i;
        if (en_state[i] != EN_DEAD)
        {
            d = dist3(ox[s], oy[s], oz[s]);
            if (d < bd)
            {
                bd = d;
                best = i;
            }
        }
    }
    if (best < 0)
    {
        return;
    }
    s = S_EN + best;
    if (oz[s] >= NEAR_Z)
    {
        project(ox[s], oy[s], oz[s]);
        if (psx > 10 && psx < 310 && psy > 26 && psy < 186)
        {
            return;
        }
        dx = (psx - VCX) >> 2;
        dy = (psy - VCY) >> 2;
    }
    else
    {
        dx = ox[s] >> 4;
        dy = -(oy[s] >> 4);
        dx = clamp_i(dx, -150, 150) >> 2;
        dy = clamp_i(dy, -150, 150) >> 2;
    }
    if (dx == 0 && dy == 0)
    {
        dy = 20;
    }
    ax = abs_i(dx);
    ay = abs_i(dy);
    if (ax * 80 > ay * 150)
    {
        py = (dy * 150) / ax;
        px = 150;
        if (dx < 0)
        {
            px = -150;
        }
    }
    else
    {
        px = (dx * 80) / ay;
        py = 80;
        if (dy < 0)
        {
            py = -80;
        }
    }
    gfx_rect(VCX + px - 4, VCY + py - 4, 9, 9, RED);
    gfx_rect(VCX + px - 2, VCY + py - 2, 5, 5, YELLOW);
}

/* Compose the 3D view. */
void draw_scene(void)
{
    int i;

    gfx_clear(BLACK);
    draw_dust();
    draw_world();
    for (i = 0; i < NEXP; i++)
    {
        draw_explosion(i);
    }
    draw_cockpit();
    if (!dead)
    {
        draw_pointer();
    }
    if (dead)
    {
        draw_burst(VCX, VCY, clamp_i(dead_t * 4, 4, 110), exp_color(16 - (dead_t >> 2)), dead_t & 3);
    }
}

/* ---- 12. HUD ------------------------------------------------------------ */

/* Horizontal gauge: frame, then a bar filled in proportion to value/max. */
void draw_bar(int x, int y, int w, int value, int max, int col)
{
    int fill;

    gfx_rect(x, y, w, 7, DARK_GRAY);
    gfx_rect(x + 1, y + 1, w - 2, 5, BLACK);
    if (value < 0)
    {
        value = 0;
    }
    fill = (value * (w - 2)) / max;
    if (fill > 0)
    {
        gfx_rect(x + 1, y + 1, fill, 5, col);
    }
}

/* Plot one radar contact: x/z on the ellipse and a stick for height. */
void draw_blip(int s, int col, int big)
{
    int bx;
    int by;
    int hh;

    bx = VCX + clamp_i(ox[s] / 40, -50, 50);
    by = 217 - clamp_i(oz[s] / 100, -18, 18);
    hh = clamp_i(oy[s] / 60, -8, 8);
    gfx_line(bx, by, bx, by - hh, DARK_GRAY);
    gfx_rect(bx - big, by - hh - big, big * 2 + 1, big * 2 + 1, col);
}

/* Elite-style elliptical 3D radar showing nearby targets. */
void draw_radar(void)
{
    int k;
    int x0;
    int y0;
    int x1;
    int y1;
    int i;

    x0 = VCX + 52;
    y0 = 217;
    for (k = 1; k <= 32; k++)
    {
        x1 = VCX + ((52 * sine[(k + 8) & 31]) >> 6);
        y1 = 217 - ((19 * sine[k & 31]) >> 6);
        gfx_line(x0, y0, x1, y1, DARK_GRAY);
        x0 = x1;
        y0 = y1;
    }
    gfx_line(VCX, 198, VCX, 236, DARK_GRAY);
    draw_blip(S_PLANET, BLUE, 2);
    for (i = 0; i < NMOON; i++)
    {
        draw_blip(S_MOON + i, WHITE, 1);
    }
    for (i = 0; i < NAST; i++)
    {
        draw_blip(S_AST + i, GRAY, 0);
    }
    for (i = 0; i < NEN; i++)
    {
        if (en_state[i] != EN_DEAD)
        {
            draw_blip(S_EN + i, RED, 1);
        }
    }
    gfx_rect(VCX - 1, 216, 3, 3, CYAN);
}

/* Instrument panels, gauges, readouts and messages. */
void draw_hud(void)
{
    char text[8];
    int alt;

    gfx_rect(0, 0, 320, 20, BLACK);
    gfx_rect(0, 192, 320, 48, BLACK);
    gfx_line(0, 20, 319, 20, DARK_GRAY);
    gfx_line(0, 192, 319, 192, DARK_GRAY);

    gfx_text(8, 6, "ELITE DEMO // MK-I", CYAN);
    alt = dist3(ox[S_PLANET], oy[S_PLANET], oz[S_PLANET]) - PLANET_R;
    if (alt < 0)
    {
        alt = 0;
    }
    gfx_text(226, 6, "ALT", GRAY);
    number_text(alt, text);
    gfx_text(250, 6, text, WHITE);

    gfx_text(6, 196, "SHLD", GRAY);
    draw_bar(36, 196, 64, shields, 100, GREEN);
    gfx_text(6, 206, "HULL", GRAY);
    draw_bar(36, 206, 64, hull, 100, YELLOW);
    gfx_text(6, 216, "HEAT", GRAY);
    if (overheated)
    {
        draw_bar(36, 216, 64, heat, 100, RED);
    }
    else
    {
        draw_bar(36, 216, 64, heat, 100, ORANGE);
    }
    gfx_text(6, 226, "SPD", GRAY);
    draw_bar(36, 226, 64, dist3(ox[S_SHIP], oy[S_SHIP], oz[S_SHIP]), MAX_SPEED, CYAN);

    draw_radar();

    gfx_text(218, 196, "KILLS", GRAY);
    number_text(kills, text);
    gfx_text(260, 196, text, WHITE);
    gfx_text(218, 206, "SCORE", GRAY);
    number_text(score, text);
    gfx_text(260, 206, text, WHITE);
    if (thrust_mode == 1)
    {
        gfx_text(218, 218, "THRUST", GREEN);
    }
    else if (thrust_mode == 2)
    {
        gfx_text(218, 218, "REVERSE", YELLOW);
    }
    else
    {
        gfx_text(218, 218, "COAST", GRAY);
    }
    if (oz[S_SHIP] > 0)
    {
        gfx_text(218, 228, "FWD", GRAY);
    }
    else if (oz[S_SHIP] < 0)
    {
        gfx_text(218, 228, "BACK", GRAY);
    }

    if (!dead)
    {
        if (overheated)
        {
            gfx_text(124, 176, "LASER OVERHEAT", RED);
        }
        else if (shields == 0 && (frame & 8))
        {
            gfx_text(124, 176, "SHIELDS DOWN", RED);
        }
        else if (hull < 30 && (frame & 8))
        {
            gfx_text(124, 176, "HULL CRITICAL", RED);
        }
        if (frame < 240)
        {
            gfx_text(60, 26, "ARROWS PITCH/YAW   Q/E ROLL   SPACE FIRE", GRAY);
            gfx_text(60, 36, "X/SHIFT THRUST     Z/CTRL REVERSE", GRAY);
        }
    }
    else
    {
        gfx_text(116, 150, "SHIP DESTROYED", RED);
        if (dead_t > 45)
        {
            gfx_text(104, 162, "SPACE/ENTER: RESTART", WHITE);
        }
    }
}

/* ---- 13. Main loop -------------------------------------------------------- */

/* One frame: input, simulation, drawing and presenting. */
int main(void)
{
    unsigned int keys;
    unsigned int kx;

    new_game();
    while (1)
    {
        keys = gfx_keys();
        kx = gfx_keys_ext();
        if (dead)
        {
            dead_t++;
            update_effects();
            if (dead_t > 45 && (keys & (KEY_FIRE | KEY_START)))
            {
                new_game();
            }
        }
        else
        {
            update_controls(keys, kx);
            rotate_all();
            move_world();
            update_moons();
            update_asteroids();
            update_enemies();
            update_combat(keys);
            update_collisions();
            update_effects();
        }
        draw_scene();
        draw_hud();
        gfx_present();
        frame++;
    }
    return 0;
}
