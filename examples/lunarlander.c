// Lunar Lander for the SC-16.
//
// Land the module softly on one of the flat landing pads.  Small pads pay more.
//
// Controls:
// - Left/Right (or A/D): Rotation thrusters
// - Up (or W) / Space:   Main engine (the throttle ramps up and down smoothly)
// - Menu: Up/Down changes the game mode, Left/Right the difficulty, Enter starts
// - Enter/Space: Continue after a landing or a crash
//
// Game modes:
// - Classic:   3 landers, limited fuel
// - Practice:  unlimited fuel and landers
// - Challenge: rough terrain, stronger gravity, less fuel
//
// Systems (each section below is independent):
//   1. Constants and data tables      6. Particle effects
//   2. Utilities                      7. Physics and collision
//   3. Audio                          8. Scoring
//   4. Input                          9. Rendering (scene, HUD, menu)
//   5. Terrain generation            10. Game flow and main loop
//
// Positions are kept in 1/16 pixel units and velocities in 1/256 pixel per
// frame; a small remainder keeps slow movement exact.
#include "suomi_gfx.h"

/* ===================================================================== */
/* 1. Constants and data tables                                          */
/* ===================================================================== */
#define NCOL 80          // Terrain columns (4 pixels wide each)
#define NPADS 3          // Landing pads per level
#define MAXP 36          // Particle pool size
#define NSTARS 28        // Background stars
#define NPOINTS 19       // Points in the lander model

#define ST_MENU 0        // Game states
#define ST_PLAY 1
#define ST_LANDED 2
#define ST_CRASHED 3
#define ST_OVER 4

#define M_CLASSIC 0      // Game modes
#define M_PRACTICE 1
#define M_CHALLENGE 2

#define R_NONE 0         // Crash reasons
#define R_TERRAIN 1
#define R_OFFPAD 2
#define R_TILT 3
#define R_FAST 4
#define R_SIDE 5

#define G_LIGHT 200      // Palette indexes above 11 are plain gray levels
#define G_MID 140
#define G_ROCK 110
#define G_DARK 60

#define THRUST 26        // Main engine acceleration at full throttle
#define AV_MAX 36        // Maximum angular speed (4096 units = full turn)

/* Sine table: 64 steps per turn, scaled by 64.  cos(i) = SIN[(i + 16) & 63] */
int SIN[64] = {
    0, 6, 12, 19, 24, 30, 36, 41, 45, 49, 53, 56, 59, 61, 63, 64,
    64, 64, 63, 61, 59, 56, 53, 49, 45, 41, 36, 30, 24, 19, 12, 6,
    0, -6, -12, -19, -24, -30, -36, -41, -45, -49, -53, -56, -59, -61, -63, -64,
    -64, -64, -63, -61, -59, -56, -53, -49, -45, -41, -36, -30, -24, -19, -12, -6
};

/* Lander model (pixels, angle 0 = nose up):
   0-5 hull, 6-7 window, 8-13 landing gear, 14-17 nozzle, 18 flame tip */
int lpx[NPOINTS] = {-3, 3, 5, 5, -5, -5, -2, 2, -8, 8, -10, -6, 6, 10, -2, 2, -3, 3, 0};
int lpy[NPOINTS] = {-5, -5, -2, 2, 2, -2, -3, -3, 7, 7, 7, 7, 7, 7, 2, 2, 5, 5, 6};
/* Line segments of the model; the last two are the flame */
int LA[16] = {0, 1, 2, 3, 4, 5, 6, 4, 3, 10, 12, 14, 15, 16, 16, 17};
int LB[16] = {1, 2, 3, 4, 5, 0, 7, 8, 9, 11, 13, 16, 17, 17, 18, 18};
int LCOL[16] = {1, 1, 1, 1, 1, 1, 6, 200, 200, 140, 140, 140, 140, 140, 5, 8};
int HULLPTS[8] = {0, 1, 2, 3, 4, 5, 16, 17};
int GEARPTS[6] = {8, 9, 10, 11, 12, 13};

/* Difficulty settings: Easy, Normal, Hard */
int GRAV[3] = {6, 8, 10};          // Gravity (1/256 px per frame squared)
int FUEL[3] = {1200, 900, 650};    // Starting fuel
int VLIM[3] = {260, 200, 150};     // Safe landing speed
int TLIM[3] = {160, 114, 80};      // Safe tilt (4096 units = full turn)
int PADBASE[3] = {12, 8, 5};       // Pad widths in columns (big, medium, small)
int PADMULT[3] = {2, 3, 5};        // Score multipliers

/* Jingle played after a successful landing */
int JING[6] = {523, 659, 784, 1047, 784, 1047};

/* ===================================================================== */
/* Game state                                                            */
/* ===================================================================== */
int state, mode, diff, level, lives, frame, timer;
int bg_dirty;
unsigned int score, hiscore;
int gravity, fuel, fuel_max, vlim, hlim, tlim, rough;

/* Lander: position (1/16 px), velocity, angle, throttle */
int lx16, ly16, vx, vy, remx, remy, ang, av, throttle, rcs_tick;
int pxw[NPOINTS], pyw[NPOINTS];    // Model points in screen coordinates
int reason;                        // Why the last landing failed
int landed_pad;

/* Scoring breakdown of the last landing */
int b_fuel, b_speed, b_acc, b_mult, b_total;

/* Terrain */
int th[NCOL];                      // Surface height (y) per column
int padid[NCOL];                   // 0 = rock, 1..NPADS = pad number
int pad_col[NPADS], pad_len[NPADS], pad_mul[NPADS];

/* Particles */
int ppx[MAXP], ppy[MAXP], pvx[MAXP], pvy[MAXP], plife[MAXP], pkind[MAXP];
int pnext;

/* Background */
int sx[NSTARS], sy[NSTARS], sb[NSTARS];
int earth_hw[16];

/* Audio and input state */
int engine_on, rcs_on, jing_pos, jing_wait;
unsigned int prev_keys;

/* ===================================================================== */
/* 2. Utilities                                                          */
/* ===================================================================== */
int iabs(int v) {
    if (v < 0) return -v;
    return v;
}

/* Returns a random number in 0..n-1 */
int rnd(int n) {
    unsigned int r = gfx_random();
    r = (r << 8) | gfx_random();
    return r % n;
}

/* Converts a number to text for the HUD */
void number_text(unsigned int value, char *out) {
    char tmp[6];
    int n = 0;
    int i;
    if (value == 0) { tmp[0] = '0'; n = 1; }
    while (value > 0) { tmp[n] = '0' + (value % 10); value /= 10; n++; }
    for (i = 0; i < n; i++) out[i] = tmp[n - 1 - i];
    out[n] = 0;
}

/* ===================================================================== */
/* 3. Audio (channel 0 engine, 1 thrusters/rumble, 2 effects, 3 ambience) */
/* ===================================================================== */
/* Quiet low drone that runs for the whole session */
void sfx_ambient(void) {
    gfx_sound(3, 55, 0, WAVE_TRIANGLE, 8);
}

/* Main engine roar; only talks to the sound chip when the state changes */
void sfx_engine(int on) {
    if (on == engine_on) return;
    engine_on = on;
    if (on) gfx_sound(0, 150, 0, WAVE_NOISE, 30);
    else gfx_sound(0, 0, 0, 0, 0);
}

/* Rotation thruster hiss */
void sfx_rcs(int on) {
    if (on == rcs_on) return;
    rcs_on = on;
    if (on) gfx_sound(1, 900, 0, WAVE_NOISE, 10);
    else gfx_sound(1, 0, 0, 0, 0);
}

void sfx_silence(void) {
    sfx_engine(0);
    sfx_rcs(0);
}

/* Explosion: a long noise burst plus a low rumble */
void sfx_crash(void) {
    sfx_silence();
    gfx_sound(2, 110, 55, WAVE_NOISE, 100);
    gfx_sound(1, 60, 40, WAVE_SQUARE, 60);
}

/* Touchdown thump followed by a rising jingle (played by sfx_update) */
void sfx_land(void) {
    sfx_silence();
    gfx_sound(1, 90, 6, WAVE_NOISE, 50);
    jing_pos = 0;
    jing_wait = 8;
}

/* Plays the next jingle note when it is due */
void sfx_update(void) {
    if (jing_pos < 0) return;
    if (jing_wait > 0) { jing_wait--; return; }
    if (jing_pos < 6) {
        gfx_sound(2, JING[jing_pos], 8, WAVE_SQUARE, 35);
        jing_pos++;
        jing_wait = 5;
    } else jing_pos = -1;
}

/* ===================================================================== */
/* 4. Input                                                              */
/* ===================================================================== */
/* Returns the keys that went down this frame; stores the held keys */
unsigned int read_pressed(unsigned int keys) {
    unsigned int pressed = keys & ~prev_keys;
    prev_keys = keys;
    return pressed;
}

/* ===================================================================== */
/* 5. Terrain generation                                                 */
/* ===================================================================== */
/* Ground height below a pixel x coordinate */
int ground_at(int x) {
    int c = x >> 2;
    if (c < 0) c = 0;
    if (c >= NCOL) c = NCOL - 1;
    return th[c];
}

int pad_at(int x) {
    int c = x >> 2;
    if (c < 0) c = 0;
    if (c >= NCOL) c = NCOL - 1;
    return padid[c];
}

/* Builds mountains from two sine waves plus a random walk, then flattens
   the landing pads and limits the slope so the terrain stays flyable. */
void terrain_generate(void) {
    int i, k, h, walk, ph1, ph2, amp, wmax, step, size, len, zone, start, mid, a, b;
    ph1 = rnd(64);
    ph2 = rnd(64);
    walk = 0;
    wmax = 6;
    step = 5;
    if (mode == M_CHALLENGE) { wmax = 14; step = 8; }
    for (i = 0; i < NCOL; i++) {
        walk = walk + rnd(3) - 1;
        if (walk > wmax) walk = wmax;
        if (walk < -wmax) walk = -wmax;
        amp = SIN[(i * 2 + ph1) & 63] >> 1;
        if (mode == M_CHALLENGE) amp = amp + (amp >> 1);
        h = 172 + amp + (SIN[(i * 5 + ph2) & 63] >> 3) + walk;
        if (h < 105) h = 105;
        if (h > 218) h = 218;
        th[i] = h;
        padid[i] = 0;
    }
    /* Place one pad of each size, in random order, one per screen zone */
    a = rnd(3);
    for (k = 0; k < NPADS; k++) {
        size = (k + a) % 3;
        len = PADBASE[size] + 2 - diff - (level >> 1);
        if (mode == M_CHALLENGE) len = len - 1;
        if (len < 5) len = 5;
        zone = 1 + k * 26;
        start = zone + rnd(25 - len);
        mid = th[start + (len >> 1)];
        if (mid < 130) mid = 130;
        for (i = 0; i < len; i++) {
            th[start + i] = mid;
            padid[start + i] = k + 1;
        }
        pad_col[k] = start;
        pad_len[k] = len;
        pad_mul[k] = PADMULT[size];
    }
    /* Smooth steep cliffs, leaving the pads untouched */
    for (i = 1; i < NCOL; i++) {
        if (padid[i] == 0) {
            if (th[i] > th[i - 1] + step) th[i] = th[i - 1] + step;
            if (th[i] < th[i - 1] - step) th[i] = th[i - 1] - step;
        }
    }
    for (i = NCOL - 2; i >= 0; i--) {
        if (padid[i] == 0) {
            if (th[i] > th[i + 1] + step) th[i] = th[i + 1] + step;
            if (th[i] < th[i + 1] - step) th[i] = th[i + 1] - step;
        }
    }
    for (i = 0; i < NCOL; i++) {
        if (th[i] > 222) th[i] = 222;
    }
}

/* ===================================================================== */
/* 6. Particle effects                                                   */
/* ===================================================================== */
/* kind 0 = exhaust, 1 = dust, 2 = explosion; positions in 1/16 px */
void spawn_particle(int kind, int x, int y, int pvx_, int pvy_, int life) {
    int i = pnext;
    pnext++;
    if (pnext >= MAXP) pnext = 0;
    ppx[i] = x;
    ppy[i] = y;
    pvx[i] = pvx_;
    pvy[i] = pvy_;
    plife[i] = life;
    pkind[i] = kind;
}

void clear_particles(void) {
    int i;
    for (i = 0; i < MAXP; i++) plife[i] = 0;
    pnext = 0;
}

/* Fire from the nozzle, opposite to the thrust direction */
void emit_exhaust(int s, int c) {
    int nx = ((pxw[16] + pxw[17]) >> 1) << 4;
    int ny = ((pyw[16] + pyw[17]) >> 1) << 4;
    int n = 1 + (throttle >> 2);
    int i;
    for (i = 0; i < n; i++) {
        spawn_particle(0, nx, ny,
                       ((0 - s * 48) >> 6) + (vx >> 4) + rnd(9) - 4,
                       ((c * 48) >> 6) + (vy >> 4) + rnd(9) - 4,
                       9 + rnd(8));
    }
}

/* Dust blown sideways when the engine fires close to the ground */
void emit_dust(void) {
    int gx = lx16 >> 4;
    int gy = (ground_at(gx) << 4) - 16;
    int dir = 1;
    if (rnd(2)) dir = -1;
    spawn_particle(1, (gx << 4) + rnd(64) - 32, gy, dir * (20 + rnd(30)), 0 - rnd(10), 14 + rnd(12));
}

/* Big fireball for a crash */
void explode(void) {
    int i;
    for (i = 0; i < MAXP; i++) {
        spawn_particle(2, lx16, ly16, rnd(81) - 40, rnd(61) - 45, 20 + rnd(35));
    }
}

/* Moves particles and removes the dead ones */
void update_particles(void) {
    int i, g;
    for (i = 0; i < MAXP; i++) {
        if (plife[i] > 0) {
            ppx[i] = ppx[i] + pvx[i];
            ppy[i] = ppy[i] + pvy[i];
            if (pkind[i] != 1) pvy[i] = pvy[i] + 1;
            plife[i] = plife[i] - 1;
            g = ground_at(ppx[i] >> 4) << 4;
            if (ppy[i] >= g) {
                ppy[i] = g - 16;
                pvy[i] = 0 - (pvy[i] >> 1);
                pvx[i] = pvx[i] + (pvx[i] >> 1);
                if (pkind[i] == 0) pkind[i] = 1;
            }
        }
    }
}

void draw_particles(void) {
    int i, life, color;
    for (i = 0; i < MAXP; i++) {
        life = plife[i];
        if (life > 0) {
            if (pkind[i] == 1) {
                color = G_LIGHT;
                if (life < 8) color = G_MID;
            } else if (pkind[i] == 0) {
                color = YELLOW;
                if (life < 12) color = ORANGE;
                if (life < 6) color = RED;
            } else {
                color = WHITE;
                if (life < 40) color = YELLOW;
                if (life < 28) color = ORANGE;
                if (life < 16) color = RED;
                if (life < 7) color = DARK_RED;
            }
            if (life > 6) gfx_rect(ppx[i] >> 4, ppy[i] >> 4, 2, 2, color);
            else gfx_pixel(ppx[i] >> 4, ppy[i] >> 4, color);
        }
    }
}

/* ===================================================================== */
/* 7. Physics and collision                                              */
/* ===================================================================== */
/* Rotates the lander model into screen coordinates */
void compute_points(void) {
    int idx = ang >> 6;
    int s = SIN[idx];
    int c = SIN[(idx + 16) & 63];
    int cx = lx16 >> 4, cy = ly16 >> 4;
    int i;
    lpy[18] = 6 + throttle + (frame & 1) * 2;
    for (i = 0; i < NPOINTS; i++) {
        pxw[i] = cx + ((lpx[i] * c - lpy[i] * s) >> 6);
        pyw[i] = cy + ((lpx[i] * s + lpy[i] * c) >> 6);
    }
}

/* How far a model point is below the ground (<= 0 when above it) */
int penetration(int i) {
    return pyw[i] - ground_at(pxw[i]);
}

int signed_tilt(void) {
    if (ang > 2048) return ang - 4096;
    return ang;
}

/* Distance in pixels from the lander legs to the ground under the pad */
int altitude(void) {
    int a = ground_at(lx16 >> 4) - (ly16 >> 4) - 7;
    if (a < 0) a = 0;
    return a;
}

/* Resets everything for a new landing attempt on fresh terrain */
void start_round(void) {
    bg_dirty = 1;
    gravity = GRAV[diff];
    fuel_max = FUEL[diff];
    vlim = VLIM[diff];
    hlim = (vlim * 3) >> 2;
    tlim = TLIM[diff];
    if (mode == M_CHALLENGE) {
        gravity = gravity + (gravity >> 1);
        fuel_max = fuel_max - (fuel_max >> 2);
    }
    terrain_generate();
    fuel = fuel_max;
    lx16 = (40 + rnd(240)) << 4;
    ly16 = 30 << 4;
    vx = rnd(81) - 40;
    vy = 0;
    remx = 0;
    remy = 0;
    ang = 0;
    av = 0;
    throttle = 0;
    rcs_tick = 0;
    reason = R_NONE;
    clear_particles();
    sfx_silence();
    compute_points();
    state = ST_PLAY;
}

/* Adds the landing bonuses to the score */
void score_landing(int pad, int vspeed) {
    int center = (pad_col[pad] << 2) + (pad_len[pad] << 1);
    int half = pad_len[pad] << 1;
    int dist = iabs((lx16 >> 4) - center);
    if (dist > half) dist = half;
    b_mult = pad_mul[pad];
    b_fuel = 50;
    if (mode != M_PRACTICE) b_fuel = fuel / 10;
    b_speed = 100 - (vspeed * 100) / vlim;
    if (b_speed < 0) b_speed = 0;
    b_acc = 100 - (dist * 100) / half;
    b_total = (b_fuel + b_speed + b_acc) * b_mult;
    score = score + b_total;
    if (score > hiscore) hiscore = score;
}

/* Decides between a safe landing and a crash after ground contact */
void touch_ground(void) {
    int i, hull, pen, maxpen, pad, vspeed;
    hull = 0;
    maxpen = 0;
    for (i = 0; i < 8; i++) {
        pen = penetration(HULLPTS[i]);
        if (pen >= 0) hull = 1;
    }
    for (i = 0; i < 6; i++) {
        pen = penetration(GEARPTS[i]);
        if (pen > maxpen) maxpen = pen;
    }
    vspeed = iabs(vy);
    pad = pad_at(pxw[8]);
    reason = R_NONE;
    if (hull) reason = R_TERRAIN;
    else if (pad == 0 || pad != pad_at(pxw[9])) reason = R_OFFPAD;
    else if (iabs(signed_tilt()) > tlim) reason = R_TILT;
    else if (vspeed > vlim) reason = R_FAST;
    else if (iabs(vx) > hlim) reason = R_SIDE;
    sfx_silence();
    throttle = 0;
    timer = 0;
    if (reason == R_NONE) {
        ly16 = ly16 - (maxpen << 4);
        vx = 0;
        vy = 0;
        av = 0;
        compute_points();
        landed_pad = pad - 1;
        score_landing(landed_pad, vspeed);
        sfx_land();
        for (i = 0; i < 10; i++) emit_dust();
        state = ST_LANDED;
    } else {
        explode();
        sfx_crash();
        if (mode != M_PRACTICE) lives--;
        state = ST_CRASHED;
    }
}

/* One frame of flight: controls, gravity, integration, collisions */
void update_play(unsigned int keys) {
    int rot = 0, want = 0, idx, s, c, acc, t, i;
    if (keys & KEY_LEFT) rot = rot - 1;
    if (keys & KEY_RIGHT) rot = rot + 1;
    if (keys & (KEY_UP | KEY_FIRE)) want = 1;
    if (fuel <= 0 && mode != M_PRACTICE) { want = 0; rot = 0; }

    /* Rotation thrusters: angular acceleration with damping when idle */
    if (rot != 0) {
        av = av + rot * 3;
        if (av > AV_MAX) av = AV_MAX;
        if (av < -AV_MAX) av = -AV_MAX;
        rcs_tick++;
        if (rcs_tick >= 3) {
            rcs_tick = 0;
            if (mode != M_PRACTICE) fuel--;
        }
    } else if (av > 0) {
        av = av - 2;
        if (av < 0) av = 0;
    } else if (av < 0) {
        av = av + 2;
        if (av > 0) av = 0;
    }
    ang = (ang + av) & 4095;

    /* Main engine with a smooth throttle */
    if (want) {
        if (throttle < 8) throttle++;
        if (mode != M_PRACTICE) fuel--;
    } else if (throttle > 0) {
        throttle = throttle - 2;
        if (throttle < 0) throttle = 0;
    }
    if (fuel < 0) fuel = 0;

    /* Forces: gravity pulls down, thrust acts along the nose direction */
    idx = ang >> 6;
    s = SIN[idx];
    c = SIN[(idx + 16) & 63];
    acc = (THRUST * throttle) >> 3;
    vx = vx + ((acc * s) >> 6);
    vy = vy + gravity - ((acc * c) >> 6);

    /* Integrate, keeping the sub-pixel remainder */
    t = remx + vx;
    lx16 = lx16 + (t >> 4);
    remx = t & 15;
    t = remy + vy;
    ly16 = ly16 + (t >> 4);
    remy = t & 15;

    /* Invisible walls on the left, right and top */
    if (lx16 < 160) { lx16 = 160; if (vx < 0) vx = 0; }
    if (lx16 > 4960) { lx16 = 4960; if (vx > 0) vx = 0; }
    if (ly16 < 128) { ly16 = 128; if (vy < 0) vy = 0; }

    compute_points();
    sfx_engine(throttle > 0);
    sfx_rcs(rot != 0);
    if (throttle > 0) {
        emit_exhaust(s, c);
        if (altitude() < 36) emit_dust();
    }
    if (fuel > 0 && fuel < (fuel_max >> 3) && (frame & 31) == 0 && mode != M_PRACTICE) {
        gfx_sound(2, 1000, 3, WAVE_SQUARE, 25);
    }

    /* Ground contact */
    for (i = 0; i < NPOINTS; i++) {
        if (i != 6 && i != 7 && i != 14 && i != 15 && i != 18) {
            if (penetration(i) >= 0) { touch_ground(); return; }
        }
    }
}

/* ===================================================================== */
/* 9. Rendering: stars, Earth, terrain, lander, HUD, menu, overlays      */
/* ===================================================================== */
void init_background(void) {
    int i, d, r;
    for (i = 0; i < NSTARS; i++) {
        sx[i] = rnd(320);
        sy[i] = rnd(170);
        sb[i] = rnd(3);
    }
    /* Half widths of the Earth disc, one entry per 2-pixel row */
    for (i = 0; i < 16; i++) {
        d = (i - 8) * 2 + 1;
        r = 0;
        while ((r + 1) * (r + 1) + d * d <= 256) r++;
        earth_hw[i] = r;
    }
}

void draw_stars(void) {
    int i, color;
    for (i = 0; i < NSTARS; i++) {
        color = G_MID;
        if (sb[i] == 0) color = G_DARK;
        if (sb[i] == 2) color = WHITE;
        gfx_pixel(sx[i], sy[i], color);
    }
}

/* One random star flashes per frame on top of the cached background */
void twinkle(void) {
    int i = rnd(NSTARS);
    gfx_pixel(sx[i], sy[i], WHITE);
}

/* The Earth hangs in the distance, drawn as a blue disc with continents */
void draw_earth(void) {
    int i, ex = 280, ey = 52;
    for (i = 0; i < 16; i++) {
        gfx_rect(ex - earth_hw[i], ey - 16 + i * 2, earth_hw[i] * 2, 2, BLUE);
    }
    gfx_rect(ex - 8, ey - 9, 7, 4, GREEN);
    gfx_rect(ex - 5, ey - 5, 9, 5, GREEN);
    gfx_rect(ex + 3, ey + 3, 6, 5, GREEN);
    gfx_rect(ex - 6, ey + 6, 4, 3, GREEN);
    gfx_rect(ex - 9, ey + 1, 7, 2, WHITE);
    gfx_rect(ex + 1, ey - 12, 8, 2, WHITE);
}

void draw_terrain(void) {
    int c, k;
    char lab[3];
    for (c = 0; c < NCOL; c++) gfx_rect(c << 2, th[c], 4, 240 - th[c], G_ROCK);
    for (c = 0; c < NCOL - 2; c += 2) {
        gfx_line((c << 2) + 2, th[c], ((c + 2) << 2) + 2, th[c + 2], G_LIGHT);
    }
    for (k = 0; k < NPADS; k++) {
        gfx_rect(pad_col[k] << 2, th[pad_col[k]] - 1, pad_len[k] << 2, 3, GREEN);
        lab[0] = 'x';
        lab[1] = '0' + pad_mul[k];
        lab[2] = 0;
        gfx_text((pad_col[k] << 2) + (pad_len[k] << 1) - 6, th[pad_col[k]] + 5, lab, BLACK);
    }
}

void draw_lander(void) {
    int i, n = 14;
    if (throttle > 0) n = 16;
    for (i = 0; i < n; i++) {
        gfx_line(pxw[LA[i]], pyw[LA[i]], pxw[LB[i]], pyw[LB[i]], LCOL[i]);
    }
}

/* Velocity vector drawn from the lander; green when the speed is safe */
void draw_velocity_vector(void) {
    int color = GREEN;
    int cx = lx16 >> 4, cy = ly16 >> 4;
    if (iabs(vy) > vlim || iabs(vx) > hlim) color = RED;
    gfx_line(cx, cy, cx + (vx >> 5), cy + (vy >> 5), color);
}

/* Draws "LABEL value" and returns nothing; the label is gray, the value white */
void hud_value(int x, int y, char *label, int len, int value, int color) {
    char buffer[8];
    number_text(value, buffer);
    gfx_text(x, y, label, GRAY);
    gfx_text(x + len * 6 + 2, y, buffer, color);
}

/* A speed readout with a direction arrow, red when above the safe limit */
void hud_speed(int y, char *label, int speed, int limit, char up, char down) {
    char buffer[8];
    int color = GREEN;
    int mag = iabs(speed);
    if (mag > limit) color = RED;
    number_text(mag >> 2, buffer + 1);
    buffer[0] = down;
    if (speed < 0) buffer[0] = up;
    gfx_text(4, y, label, GRAY);
    gfx_text(46, y, buffer, color);
}

void draw_hud(void) {
    int w, color, i, fq;
    char buffer[8];
    hud_value(4, 4, "SCORE", 5, score, WHITE);
    /* Fuel gauge */
    gfx_text(4, 14, "FUEL", GRAY);
    gfx_rect(32, 13, 62, 9, GRAY);
    gfx_rect(33, 14, 60, 7, BLACK);
    if (mode == M_PRACTICE) {
        gfx_rect(33, 14, 60, 7, CYAN);
        gfx_text(98, 14, "INF", CYAN);
    } else {
        fq = fuel_max >> 2;
        w = ((fuel >> 2) * 60) / fq;
        if (w > 60) w = 60;
        color = GREEN;
        if (fuel < fuel_max / 4) color = YELLOW;
        if (fuel < fuel_max / 8) color = RED;
        if (w > 0) gfx_rect(33, 14, w, 7, color);
        number_text(fuel, buffer);
        gfx_text(98, 14, buffer, WHITE);
    }
    hud_value(4, 24, "ALT", 3, altitude(), WHITE);
    hud_speed(34, "H.SPD", vx, hlim, '<', '>');
    hud_speed(44, "V.SPD", vy, vlim, '^', 'v');
    /* Throttle gauge */
    gfx_text(4, 54, "PWR", GRAY);
    gfx_rect(32, 54, 34, 7, GRAY);
    gfx_rect(33, 55, 32, 5, BLACK);
    if (throttle > 0) gfx_rect(33, 55, throttle * 4, 5, ORANGE);
    /* Level, lives and best score */
    hud_value(150, 4, "LEVEL", 5, level, WHITE);
    if (mode != M_PRACTICE) {
        for (i = 0; i < lives; i++) {
            gfx_line(212 + i * 10, 11, 215 + i * 10, 4, CYAN);
            gfx_line(215 + i * 10, 4, 218 + i * 10, 11, CYAN);
            gfx_line(212 + i * 10, 11, 218 + i * 10, 11, CYAN);
        }
    } else gfx_text(212, 4, "PRACTICE", CYAN);
}

/* Centered banner text, 6 pixels per character */
void center_text(int y, char *text, int len, int color) {
    gfx_text(160 - len * 3, y, text, color);
}

void draw_result(void) {
    char buffer[8];
    if (state == ST_LANDED) {
        center_text(70, "THE EAGLE HAS LANDED", 20, GREEN);
        hud_value(104, 86, "FUEL", 4, b_fuel, WHITE);
        hud_value(104, 96, "SPEED", 5, b_speed, WHITE);
        hud_value(104, 106, "ACCURACY", 8, b_acc, WHITE);
        hud_value(104, 116, "PAD MULTIPLIER X", 16, b_mult, YELLOW);
        hud_value(104, 128, "POINTS", 6, b_total, YELLOW);
    } else if (state == ST_CRASHED || state == ST_OVER) {
        if (timer > 25) {
            center_text(70, "THE LANDER WAS DESTROYED", 24, RED);
            if (reason == R_TERRAIN) center_text(84, "HIT THE TERRAIN", 15, WHITE);
            if (reason == R_OFFPAD) center_text(84, "MISSED THE LANDING PAD", 22, WHITE);
            if (reason == R_TILT) center_text(84, "TOO MUCH TILT", 13, WHITE);
            if (reason == R_FAST) center_text(84, "DESCENT TOO FAST", 16, WHITE);
            if (reason == R_SIDE) center_text(84, "TOO MUCH SIDEWAYS SPEED", 23, WHITE);
        }
    }
    if (state == ST_OVER && timer > 25) {
        center_text(100, "GAME OVER", 9, RED);
        number_text(score, buffer);
        center_text(112, buffer, 5, YELLOW);
    }
    if (timer > 40 && (timer & 32) == 0) {
        center_text(150, "PRESS ENTER TO CONTINUE", 23, WHITE);
    }
}

/* Draws the sky, planet, ground and everything that moves */
void draw_scene(void) {
    if (bg_dirty) {
        gfx_clear(BLACK);
        draw_stars();
        draw_earth();
        draw_terrain();
        gfx_save();
        bg_dirty = 0;
    } else {
        gfx_restore();
    }
    twinkle();
    draw_particles();
    if (state == ST_PLAY || state == ST_LANDED) {
        draw_lander();
        if (state == ST_PLAY) draw_velocity_vector();
    }
}

void draw_menu(void) {
    char *mode_name;
    char *diff_name;
    char *info;
    int len, dlen, ilen;
    mode_name = "CLASSIC";
    len = 7;
    info = "3 LANDERS, LIMITED FUEL";
    ilen = 23;
    if (mode == M_PRACTICE) {
        mode_name = "PRACTICE";
        len = 8;
        info = "UNLIMITED FUEL AND LANDERS";
        ilen = 26;
    }
    if (mode == M_CHALLENGE) {
        mode_name = "CHALLENGE";
        len = 9;
        info = "ROUGH TERRAIN, HEAVY GRAVITY";
        ilen = 28;
    }
    diff_name = "NORMAL";
    dlen = 6;
    if (diff == 0) { diff_name = "EASY"; dlen = 4; }
    if (diff == 2) { diff_name = "HARD"; dlen = 4; }
    center_text(20, "LUNAR LANDER", 12, WHITE);
    center_text(34, "SUOMICPU-16", 11, G_MID);
    center_text(76, "MODE", 4, GRAY);
    center_text(88, mode_name, len, YELLOW);
    center_text(100, info, ilen, G_MID);
    center_text(120, "DIFFICULTY", 10, GRAY);
    center_text(132, diff_name, dlen, YELLOW);
    center_text(154, "UP/DOWN: MODE   LEFT/RIGHT: DIFFICULTY", 38, CYAN);
    center_text(166, "ENTER: START", 12, GREEN);
    if (hiscore > 0) hud_value(116, 184, "BEST SCORE", 10, hiscore, WHITE);
}

/* ===================================================================== */
/* 10. Game flow and main loop                                           */
/* ===================================================================== */
void new_game(void) {
    score = 0;
    level = 1;
    lives = 3;
    start_round();
}

void enter_menu(void) {
    state = ST_MENU;
    sfx_silence();
    clear_particles();
    start_round();
    state = ST_MENU;
    ang = 0;
    ly16 = 0;
    lx16 = 0;
}

void update_menu(unsigned int pressed) {
    if (pressed & KEY_UP) { mode = mode + 2; if (mode > 2) mode = mode - 3; }
    if (pressed & KEY_DOWN) { mode++; if (mode > 2) mode = 0; }
    if (pressed & KEY_RIGHT) { diff++; if (diff > 2) diff = 0; }
    if (pressed & KEY_LEFT) { diff = diff + 2; if (diff > 2) diff = diff - 3; }
    if (pressed & (KEY_START | KEY_FIRE)) new_game();
}

/* Result screens: wait a moment, then continue or return to the menu */
void update_result(unsigned int pressed) {
    timer++;
    update_particles();
    if (timer > 40 && (pressed & (KEY_START | KEY_FIRE))) {
        if (state == ST_LANDED) { level++; start_round(); }
        else if (state == ST_CRASHED) {
            if (mode != M_PRACTICE && lives <= 0) { state = ST_OVER; timer = 0; }
            else start_round();
        } else enter_menu();
    }
    if (state == ST_CRASHED && mode != M_PRACTICE && lives <= 0 && timer > 40) {
        state = ST_OVER;
        timer = 0;
    }
}

int main(void) {
    unsigned int keys, pressed;
    mode = M_CLASSIC;
    diff = 1;
    level = 1;
    hiscore = 0;
    prev_keys = 0;
    engine_on = 0;
    rcs_on = 0;
    jing_pos = -1;
    frame = 0;
    init_background();
    sfx_ambient();
    enter_menu();
    while (1) {
        keys = gfx_keys();
        pressed = read_pressed(keys);
        if (state == ST_MENU) update_menu(pressed);
        else if (state == ST_PLAY) {
            update_play(keys);
            update_particles();
        } else update_result(pressed);
        sfx_update();
        draw_scene();
        if (state == ST_MENU) draw_menu();
        else {
            draw_hud();
            if (state != ST_PLAY) draw_result();
        }
        gfx_present();
        frame++;
    }
    return 0;
}
