// SpaceCave - LAN multiplayer Asteroids dogfight inside a procedurally generated cave.
//
// Up to 8 players on the local network (start the emulator once per player):
//     python main.py examples/spacecave.c
//
// Controls: Left/Right rotate, Up thrust, Space fire, hold Tab for the full scoreboard.
// Menu: type a nickname (A-Z, 0-9, space), Enter joins, Tab shows the controls.
//
// Architecture (server-authoritative, no dedicated server program):
//   * The player with the lowest network slot is the host. The host alone simulates
//     physics, bullets, turrets, scoring and respawns. Everybody else only sends its
//     pressed keys and renders the snapshots the host broadcasts, so a modified client
//     cannot teleport, heal itself or award itself points.
//   * If the host leaves, the next lowest slot takes over using the state it mirrored.
//   * The cave is generated from a 16-bit seed that the host broadcasts, so every
//     instance builds an identical map without sending the map itself.
//   * Players are keyed by a table index 0-7 (the colour). A player who disconnects
//     keeps the table entry (score visible, slot reserved) for 10 seconds.
//
// Sections: 1 constants/state, 2 helpers, 3 cave generation, 4 particles/audio/events,
// 5 physics + collisions, 6 combat, 7 turret AI, 8 player management, 9 networking,
// 10 rendering (HUD/UI), 11 main loop.
#include "suomi_gfx.h"

/* --- 1. Constants and state --- */
#define MW 64
#define MH 48
#define WORLD_W 1024
#define WORLD_H 768
#define MAXP 8
#define MAXB 20
#define MAXT 12
#define MAXPART 40
#define HP_MAX 100
#define AMMO_MAX 30
#define TURRET_HP 60
#define RESPAWN_FRAMES 600
#define GONE_FRAMES 600
#define C_EDGE 13
#define C_ROCK 14

#define M_NAME 1
#define M_INPUT 2
#define M_SHIP 3
#define M_BULLET 4
#define M_TURRET 5
#define M_WORLD 6
#define M_EVENT 7
#define M_PLAYER 8

#define EV_KILL 1
#define EV_TURRET 2
#define EV_IMPACT 3
#define EV_TFIRE 4
#define EV_FIRE 5
#define EV_CRASH 6

int dxT[32] = {
    0, 2, 3, 4, 6, 7, 7, 8, 8, 8, 7, 7, 6, 4, 3, 2,
    0, -2, -3, -4, -6, -7, -7, -8, -8, -8, -7, -7, -6, -4, -3, -2
};
int dyT[32] = {
    -8, -8, -7, -7, -6, -4, -3, -2, 0, 2, 3, 4, 6, 7, 7, 8,
    8, 8, 7, 7, 6, 4, 3, 2, 0, -2, -3, -4, -6, -7, -7, -8
};
int pcol[8] = {3, 6, 5, 7, 8, 1, 12, 4};     // Player colours (no red: red = turrets)

unsigned char cave[3072];                    // Tile map: 0 empty, else rock colour
unsigned char chx[12], chy[12], chr[12];     // Chamber centres/radius in tiles
unsigned int rs;                             // Random state
unsigned int cave_seed;
int cave_ok;

// Player table (index = colour). pused: 0 free, 1 playing, 2 disconnected (grace period)
unsigned char pused[8], psid[8], pnet[8], php[8], pang[8], pthr[8], pammo[8];
unsigned char pcool[8], pinv[8], pkeys[8], pkills[8], pdeaths[8], pstale[8], pbump[8];
int px[8], py[8], pvx[8], pvy[8], psx[8], psy[8];
int pscore[8], prt[8], pgone[8], pecho[8], palive[8];
char names[72];

int bx[20], by[20], bvx[20], bvy[20];
unsigned char blife[20], bown[20];           // bown: 0-7 player, 8 turret

int tx[12], ty[12], ttimer[12];
unsigned char thp[12], taim[12], tcool[12], tboost[12];
int nt;

int qx[40], qy[40], qvx[40], qvy[40];        // Particles in 1/4 pixel units
unsigned char qlife[40], qcol[40];

unsigned char fk[4], fv[4];                  // Kill feed
int ftime[4];

unsigned char ibuf[48], obuf[48];
char tname[9];
char myname[9];
int name_len;

int state, frame, prev_keys, show_help;
int me, mysid, myslot, hnet, nready, i_am_host, was_host, joined_sent;
int nact[8];
int camx, camy, ping, thr_snd, was_alive;

/* --- 2. Helpers --- */
int iabs(int v) {
    if (v < 0) return -v;
    return v;
}

int sgn(int v) {
    if (v > 0) return 1;
    if (v < 0) return -1;
    return 0;
}

int rnd(unsigned int n) {
    rs = rs * 25173 + 13849;
    return (rs >> 3) % n;
}

int drag(int v) {
    if (v > 0) return v - ((v + 31) >> 5);
    return v + (((-v) + 31) >> 5);
}

void number_text(unsigned int value, char *out) {
    char tmp[6];
    int n = 0;
    int i;
    if (value == 0) { tmp[0] = '0'; n = 1; }
    while (value > 0) { tmp[n] = '0' + (value % 10); value /= 10; n++; }
    for (i = 0; i < n; i++) out[i] = tmp[n - 1 - i];
    out[n] = 0;
}

int slen(char *s) {
    int n = 0;
    while (s[n]) n++;
    return n;
}

void put16(int at, int v) {
    obuf[at] = (v >> 8) & 255;
    obuf[at + 1] = v & 255;
}

int get16(int at) {
    return (ibuf[at] << 8) | ibuf[at + 1];
}

void add_score(int t, int v) {
    pscore[t] = pscore[t] + v;
    if (pscore[t] < 0) pscore[t] = 0;
}

/* --- 3. Procedural cave generation --- */
int wall(int x, int y) {
    if (x < 0 || y < 0 || x >= WORLD_W || y >= WORLD_H) return 1;
    return cave[((y >> 4) << 6) + (x >> 4)];
}

void carve(int x, int y) {
    if (x > 0 && x < MW - 1 && y > 0 && y < MH - 1) cave[(y << 6) + x] = 0;
}

void carve_disc(int cx, int cy, int r) {
    int x, y;
    for (y = cy - r; y <= cy + r; y++) {
        for (x = cx - r; x <= cx + r; x++) {
            if ((x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r + r) carve(x, y);
        }
    }
}

/* Meandering tunnel between two tiles; narrow tunnels are 2 tiles wide, wide ones 3 tiles */
void tunnel(int x0, int y0, int x1, int y1, int wide) {
    int dx, dy, guard, along_x;
    guard = 200;
    while ((x0 != x1 || y0 != y1) && guard > 0) {
        dx = x1 - x0;
        dy = y1 - y0;
        along_x = iabs(dx) > iabs(dy);
        if (dx == 0) along_x = 0;
        if (dy == 0) along_x = 1;
        if (dx != 0 && dy != 0 && rnd(4) == 0) along_x = 1 - along_x;
        if (along_x) x0 = x0 + sgn(dx);
        else y0 = y0 + sgn(dy);
        carve(x0, y0);
        carve(x0 + 1, y0);
        carve(x0, y0 + 1);
        carve(x0 + 1, y0 + 1);
        if (wide) {
            carve(x0 + 2, y0);
            carve(x0 + 2, y0 + 1);
            carve(x0, y0 + 2);
            carve(x0 + 1, y0 + 2);
            carve(x0 + 2, y0 + 2);
        }
        guard--;
    }
}

/* Rock touching open space becomes the bright edge colour */
void colour_rock(void) {
    int x, y, i, edge;
    for (y = 0; y < MH; y++) {
        for (x = 0; x < MW; x++) {
            i = (y << 6) + x;
            if (cave[i] != 0) {
                edge = 0;
                if (x > 0 && cave[i - 1] == 0) edge = 1;
                if (x < MW - 1 && cave[i + 1] == 0) edge = 1;
                if (y > 0 && cave[i - 64] == 0) edge = 1;
                if (y < MH - 1 && cave[i + 64] == 0) edge = 1;
                if (edge) cave[i] = C_EDGE;
                else cave[i] = C_ROCK;
            }
        }
    }
}

/* Twelve chambers on a 4x3 grid joined by tunnels: guaranteed connected, with big
   open arenas (radius 5-6), small chambers, narrow passages and a few shortcuts */
void gen_cave(unsigned int seed) {
    int r, c, i, k, a;
    rs = seed;
    cave_seed = seed;
    for (i = 0; i < 3072; i++) cave[i] = 1;
    for (r = 0; r < 3; r++) {
        for (c = 0; c < 4; c++) {
            i = r * 4 + c;
            chx[i] = c * 16 + 8 + rnd(5) - 2;
            chy[i] = r * 16 + 8 + rnd(5) - 2;
            chr[i] = 3 + rnd(4);
            carve_disc(chx[i], chy[i], chr[i]);
        }
    }
    for (r = 0; r < 3; r++) {
        for (c = 0; c < 4; c++) {
            i = r * 4 + c;
            if (c < 3) tunnel(chx[i], chy[i], chx[i + 1], chy[i + 1], rnd(2));
            if (r < 2) tunnel(chx[i], chy[i], chx[i + 4], chy[i + 4], rnd(2));
        }
    }
    for (k = 0; k < 4; k++) {
        a = rnd(8);
        if ((a & 3) < 3) tunnel(chx[a], chy[a], chx[a + 5], chy[a + 5], 0);
    }
    for (i = 0; i < 12; i++) {
        if (chr[i] >= 5) {
            cave[(chy[i] << 6) + chx[i] - 2] = 1;
            cave[(chy[i] << 6) + chx[i] + 2] = 1;
        }
    }
    colour_rock();
}

/* Turrets sit on the rim of chambers; derived from the same seed, so they are identical
   on every instance (only their state is networked) */
void place_turrets(void) {
    int i, k, a, x, y;
    nt = 0;
    for (i = 0; i < 12; i++) {
        if (nt < MAXT && rnd(4) != 0) {
            for (k = 0; k < 10; k++) {
                a = rnd(32);
                x = chx[i] + ((dxT[a] * (chr[i] - 1)) >> 3);
                y = chy[i] + ((dyT[a] * (chr[i] - 1)) >> 3);
                if (cave[(y << 6) + x] == 0) {
                    tx[nt] = x * 16 + 8;
                    ty[nt] = y * 16 + 8;
                    thp[nt] = TURRET_HP;
                    taim[nt] = rnd(32);
                    tcool[nt] = 30 + rnd(40);
                    tboost[nt] = 0;
                    ttimer[nt] = 0;
                    nt++;
                    break;
                }
            }
        }
    }
}

void reset_mirror(void) {
    int i;
    for (i = 0; i < MAXP; i++) { pused[i] = 0; php[i] = 0; pstale[i] = 0; }
    for (i = 0; i < MAXB; i++) blife[i] = 0;
    for (i = 0; i < MAXPART; i++) qlife[i] = 0;
}

void make_world(unsigned int seed) {
    gfx_clear(BLACK);
    gfx_text(100, 110, "GENERATING CAVE...", YELLOW);
    gfx_present();
    gen_cave(seed);
    place_turrets();
    reset_mirror();
    cave_ok = 1;
    rs = seed ^ gfx_ticks() ^ 0x5A5A;
}

/* --- 4. Particles, kill feed, audio and events --- */
void part(int x, int y, int vx, int vy, int life, int col) {
    int i;
    for (i = 0; i < MAXPART; i++) {
        if (qlife[i] == 0) {
            qx[i] = x << 2; qy[i] = y << 2; qvx[i] = vx; qvy[i] = vy;
            qlife[i] = life; qcol[i] = col;
            return;
        }
    }
}

void burst(int x, int y, int n, int speed, int c1, int c2) {
    int k, a, sp;
    for (k = 0; k < n; k++) {
        a = rnd(32);
        sp = 1 + rnd(speed);
        if (k & 1) part(x, y, (dxT[a] * sp) >> 1, (dyT[a] * sp) >> 1, 10 + rnd(20), c1);
        else part(x, y, (dxT[a] * sp) >> 1, (dyT[a] * sp) >> 1, 10 + rnd(20), c2);
    }
}

void update_particles(void) {
    int i;
    for (i = 0; i < MAXPART; i++) {
        if (qlife[i] > 0) {
            qx[i] = qx[i] + qvx[i];
            qy[i] = qy[i] + qvy[i];
            qlife[i] = qlife[i] - 1;
        }
    }
    for (i = 0; i < 4; i++) {
        if (ftime[i] > 0) ftime[i] = ftime[i] - 1;
    }
}

int near_view(int x, int y) {
    return iabs(x - camx - 160) < 260 && iabs(y - camy - 120) < 220;
}

void feed_add(int killer, int victim) {
    int i;
    for (i = 3; i > 0; i--) { fk[i] = fk[i - 1]; fv[i] = fv[i - 1]; ftime[i] = ftime[i - 1]; }
    fk[0] = killer; fv[0] = victim; ftime[0] = 360;
}

/* Applies an event on every instance: particles, sound and kill feed */
void handle_event(int kind, int a, int b, int x, int y) {
    int near;
    near = near_view(x, y);
    if (kind == EV_KILL) {
        burst(x, y, 28, 5, pcol[a & 7], WHITE);
        burst(x, y, 10, 3, ORANGE, YELLOW);
        if (near) gfx_sound(2, 110, 40, WAVE_NOISE, 80);
        feed_add(b, a);
    } else if (kind == EV_TURRET) {
        burst(x, y, 24, 4, RED, ORANGE);
        if (near) gfx_sound(2, 150, 30, WAVE_NOISE, 70);
        feed_add(b, 8);
    } else if (kind == EV_IMPACT) {
        burst(x, y, 5, 2, ORANGE, WHITE);
        if (near) gfx_sound(2, 380, 4, WAVE_NOISE, 30);
    } else if (kind == EV_CRASH) {
        burst(x, y, 7, 3, YELLOW, GRAY);
        if (near) gfx_sound(2, 220, 8, WAVE_NOISE, 45);
    } else if (kind == EV_TFIRE) {
        if (near) gfx_sound(3, 330, 7, WAVE_TRIANGLE, 30);
    } else if (kind == EV_FIRE) {
        if (near) {
            if (a == me) gfx_sound(1, 900, 5, WAVE_SQUARE, 30);
            else gfx_sound(1, 700, 5, WAVE_SQUARE, 15);
        }
    }
}

/* --- 5. Physics and collisions (host) --- */
/* A ship is a 8x8 box; turrets (alive or wrecked) are solid obstacles too */
int hit(int x, int y) {
    int k;
    if (wall(x - 4, y - 4) || wall(x + 4, y - 4) || wall(x - 4, y + 4) || wall(x + 4, y + 4)) return 1;
    for (k = 0; k < nt; k++) {
        if (iabs(x - tx[k]) < 9 && iabs(y - ty[k]) < 9) return 1;
    }
    return 0;
}

int los(int x0, int y0, int x1, int y1) {
    int n, k, dx, dy;
    dx = x1 - x0;
    dy = y1 - y0;
    n = (iabs(dx) + iabs(dy)) >> 3;
    for (k = 1; k < n; k++) {
        if (wall(x0 + dx * k / n, y0 + dy * k / n)) return 0;
    }
    return 1;
}

/* Reports an event to everybody (host only) and applies it locally */
void event(int kind, int a, int b, int x, int y) {
    obuf[0] = M_EVENT; obuf[1] = kind; obuf[2] = a; obuf[3] = b;
    put16(4, x);
    put16(6, y);
    net_send(obuf, 8);
    handle_event(kind, a, b, x, y);
}

/* --- 6. Combat (host) --- */
/* Damage a ship. killer: 0-7 player, 8 turret, 9 terrain. */
void hurt(int t, int amount, int killer) {
    if (php[t] == 0 || pinv[t] > 0) return;
    if (php[t] > amount) {
        php[t] = php[t] - amount;
        return;
    }
    php[t] = 0;
    prt[t] = RESPAWN_FRAMES;
    pthr[t] = 0;
    pdeaths[t] = pdeaths[t] + 1;
    if (killer < 8 && killer != t) {
        pkills[killer] = pkills[killer] + 1;
        add_score(killer, 100);
    } else if (killer != 8) {
        add_score(t, -10);
        killer = 9;
    }
    event(EV_KILL, t, killer, px[t], py[t]);
}

int add_bullet(int x, int y, int vx, int vy, int life, int owner) {
    int i;
    for (i = 0; i < MAXB; i++) {
        if (blife[i] == 0) {
            bx[i] = x; by[i] = y; bvx[i] = vx; bvy[i] = vy;
            blife[i] = life; bown[i] = owner;
            return 1;
        }
    }
    return 0;
}

void destroy_turret(int k, int owner) {
    thp[k] = 0;
    ttimer[k] = 600 + rnd(1200);              // Auto-repair in 10-30 seconds
    if (owner < 8) add_score(owner, 50);
    event(EV_TURRET, k, owner, tx[k], ty[k]);
}

void sim_bullets(void) {
    int i, t, k, done;
    for (i = 0; i < MAXB; i++) {
        if (blife[i] == 0) continue;
        bx[i] = bx[i] + bvx[i];
        by[i] = by[i] + bvy[i];
        blife[i] = blife[i] - 1;
        done = 0;
        if (wall(bx[i], by[i])) {
            event(EV_IMPACT, 0, 0, bx[i], by[i]);
            done = 1;
        }
        for (t = 0; t < MAXP && !done; t++) {
            if (php[t] > 0 && pinv[t] == 0 && t != bown[i]
                && iabs(bx[i] - px[t]) < 6 && iabs(by[i] - py[t]) < 6) {
                event(EV_IMPACT, 0, 0, bx[i], by[i]);
                hurt(t, 20, bown[i]);
                done = 1;
            }
        }
        for (k = 0; k < nt && !done && bown[i] < 8; k++) {
            if (thp[k] > 0 && iabs(bx[i] - tx[k]) < 8 && iabs(by[i] - ty[k]) < 8) {
                event(EV_IMPACT, 0, 0, bx[i], by[i]);
                if (thp[k] > 20) thp[k] = thp[k] - 20;
                else destroy_turret(k, bown[i]);
                done = 1;
            }
        }
        if (done) blife[i] = 0;
    }
}

/* Ship vs obstacle: bounce and take damage proportional to the impact speed */
void crash(int t, int speed) {
    if (speed > 96) {
        event(EV_CRASH, t, 0, px[t], py[t]);
        add_score(t, -1);
        hurt(t, speed >> 5, 9);
    }
}

/* Random spawn point: open 3x3 tiles, away from other ships and from every turret */
void spawn_ship(int t) {
    int tries, x, y, k, ok, dx, dy, minp, mint;
    for (tries = 0; tries < 400; tries++) {
        x = 2 + rnd(MW - 4);
        y = 2 + rnd(MH - 4);
        minp = 160;
        mint = 120;
        if (tries >= 250) { minp = 90; mint = 70; }
        if (tries >= 380) { minp = 0; mint = 0; }
        ok = 1;
        for (dy = -1; dy <= 1; dy++) {
            for (dx = -1; dx <= 1; dx++) {
                if (cave[((y + dy) << 6) + x + dx] != 0) ok = 0;
            }
        }
        for (k = 0; k < MAXP && ok; k++) {
            if (k != t && php[k] > 0 && iabs(px[k] - x * 16 - 8) + iabs(py[k] - y * 16 - 8) < minp) ok = 0;
        }
        for (k = 0; k < nt && ok; k++) {
            if (iabs(tx[k] - x * 16 - 8) + iabs(ty[k] - y * 16 - 8) < mint) ok = 0;
        }
        if (ok) break;
    }
    px[t] = x * 16 + 8;
    py[t] = y * 16 + 8;
    pvx[t] = 0; pvy[t] = 0; psx[t] = 0; psy[t] = 0;
    pang[t] = rnd(32);
    php[t] = HP_MAX;
    pammo[t] = AMMO_MAX;
    pcool[t] = 0; pbump[t] = 0;
    pinv[t] = 120;
    palive[t] = 0;
    pthr[t] = 0;
    prt[t] = 0;
}

void sim_ship(int t) {
    int keys, a, s, d, nx, ny;
    if (php[t] == 0) {
        if (prt[t] > 0) {
            prt[t] = prt[t] - 1;
            if (prt[t] == 0) spawn_ship(t);
        }
        return;
    }
    keys = pkeys[t];
    a = pang[t];
    if ((frame & 1) == 0) {
        if (keys & KEY_LEFT) a = (a + 31) & 31;
        if (keys & KEY_RIGHT) a = (a + 1) & 31;
        pang[t] = a;
    }
    pthr[t] = 0;
    if (keys & KEY_UP) {
        pvx[t] = pvx[t] + dxT[a] * 3;
        pvy[t] = pvy[t] + dyT[a] * 3;
        pthr[t] = 1;
    }
    pvx[t] = drag(pvx[t]);
    pvy[t] = drag(pvy[t]);

    s = psx[t] + pvx[t];
    d = s >> 8;
    nx = px[t] + d;
    if (hit(nx, py[t])) {
        crash(t, iabs(pvx[t]));
        pvx[t] = -(pvx[t] >> 1);
    } else {
        px[t] = nx;
        psx[t] = s & 255;
    }
    s = psy[t] + pvy[t];
    d = s >> 8;
    ny = py[t] + d;
    if (hit(px[t], ny)) {
        crash(t, iabs(pvy[t]));
        pvy[t] = -(pvy[t] >> 1);
    } else {
        py[t] = ny;
        psy[t] = s & 255;
    }
    if (php[t] == 0) return;                  // Died in the crash

    if (pcool[t] > 0) pcool[t] = pcool[t] - 1;
    if (pinv[t] > 0) pinv[t] = pinv[t] - 1;
    if (pbump[t] > 0) pbump[t] = pbump[t] - 1;
    if ((frame & 15) == 0 && pammo[t] < AMMO_MAX) pammo[t] = pammo[t] + 1;
    if ((keys & KEY_FIRE) && pcool[t] == 0 && pammo[t] > 0) {
        if (add_bullet(px[t] + dxT[a], py[t] + dyT[a],
                       ((dxT[a] * 5) >> 3) + (pvx[t] >> 8), ((dyT[a] * 5) >> 3) + (pvy[t] >> 8), 45, t)) {
            pammo[t] = pammo[t] - 1;
            pcool[t] = 8;
            event(EV_FIRE, t, 0, px[t], py[t]);
        }
    }
    palive[t] = palive[t] + 1;
    if (palive[t] >= 300) {
        palive[t] = 0;
        add_score(t, 5);                      // Survival bonus
    }
}

/* Ships bounce off each other and both take a little damage */
void ship_collisions(void) {
    int a, b, v;
    for (a = 0; a < MAXP; a++) {
        for (b = a + 1; b < MAXP; b++) {
            if (php[a] > 0 && php[b] > 0 && pbump[a] == 0 && pbump[b] == 0
                && iabs(px[a] - px[b]) < 8 && iabs(py[a] - py[b]) < 8) {
                v = pvx[a]; pvx[a] = pvx[b]; pvx[b] = v;
                v = pvy[a]; pvy[a] = pvy[b]; pvy[b] = v;
                pbump[a] = 20; pbump[b] = 20;
                event(EV_CRASH, a, 0, px[a], py[a]);
                hurt(a, 10, b);
                hurt(b, 10, a);
            }
        }
    }
}

/* --- 7. Turret AI (host) --- */
void sim_turrets(void) {
    int k, t, best, bd, d, dx, dy, fx, fy, cr, dt, lim;
    for (k = 0; k < nt; k++) {
        if (thp[k] == 0) {
            if (ttimer[k] > 0) {
                ttimer[k] = ttimer[k] - 1;
                if (ttimer[k] == 0) { thp[k] = TURRET_HP; tboost[k] = 1; }   // Repaired: fires faster
            }
            continue;
        }
        if (tcool[k] > 0) tcool[k] = tcool[k] - 1;
        best = -1;
        bd = 260;
        for (t = 0; t < MAXP; t++) {
            if (php[t] > 0) {
                d = iabs(px[t] - tx[k]) + iabs(py[t] - ty[k]);
                if (d < bd && los(tx[k], ty[k], px[t], py[t])) { bd = d; best = t; }
            }
        }
        if (best < 0) {
            if ((frame & 15) == 0) taim[k] = (taim[k] + 1) & 31;
            continue;
        }
        dx = px[best] - tx[k];
        dy = py[best] - ty[k];
        fx = dxT[taim[k]];
        fy = dyT[taim[k]];
        cr = fx * dy - fy * dx;
        dt = fx * dx + fy * dy;
        lim = (bd * 3) >> 2;
        if (dt <= 0 || iabs(cr) > lim) {
            if ((frame & 3) == 0) {
                if (cr >= 0) taim[k] = (taim[k] + 1) & 31;
                else taim[k] = (taim[k] + 31) & 31;
            }
        } else if (tcool[k] == 0) {
            if (add_bullet(tx[k] + fx, ty[k] + fy, (fx * 3) >> 3, (fy * 3) >> 3, 70, 8)) {
                if (tboost[k]) tcool[k] = 24;
                else tcool[k] = 50;
                event(EV_TFIRE, k, 0, tx[k], ty[k]);
            }
        }
    }
}

/* --- 8. Player management (host) --- */
int find_net(int ns) {
    int t;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && pnet[t] == ns) return t;
    }
    return -1;
}

void send_player(int t) {
    int i;
    obuf[0] = M_PLAYER; obuf[1] = t; obuf[2] = pnet[t]; obuf[3] = psid[t];
    for (i = 0; i < 8; i++) obuf[4 + i] = names[t * 9 + i];
    net_send(obuf, 12);
}

/* A player (network slot ns, random session id sid) asks to join the match */
void register_player(int ns, int sid, char *nm) {
    int t, f, i;
    f = -1;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && pnet[t] == ns) {
            if (psid[t] == sid) return;
            pused[t] = 2; pgone[t] = GONE_FRAMES; php[t] = 0;   // Slot reused by a new session
        }
    }
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 0) { f = t; break; }
    }
    if (f < 0) return;                        // All 8 entries taken (some may be on hold)
    pused[f] = 1; pnet[f] = ns; psid[f] = sid;
    for (i = 0; i < 8; i++) names[f * 9 + i] = nm[i];
    names[f * 9 + 8] = 0;
    pscore[f] = 0; pkills[f] = 0; pdeaths[f] = 0; pkeys[f] = 0; pecho[f] = 0;
    nact[ns & 7] = 1;
    spawn_ship(f);
    send_player(f);
}

void check_disconnects(void) {
    int t;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && !nact[pnet[t] & 7]) {
            pused[t] = 2; pgone[t] = GONE_FRAMES; php[t] = 0; pthr[t] = 0;
        } else if (pused[t] == 2) {
            pgone[t] = pgone[t] - 1;
            if (pgone[t] <= 0) pused[t] = 0;  // Entry (and colour) become free again
        }
    }
}

/* Takes over as host from the mirrored state */
void become_host(void) {
    int t, k;
    for (t = 0; t < MAXP; t++) {
        psx[t] = 0; psy[t] = 0; pcool[t] = 0; pbump[t] = 0;
        if (pused[t] == 2) pgone[t] = GONE_FRAMES;
    }
    for (k = 0; k < MAXB; k++) blife[k] = 0;
    for (k = 0; k < nt; k++) {
        tcool[k] = 40;
        if (thp[k] == 0) ttimer[k] = 600;
    }
}

/* --- 9. Networking --- */
void send_ship(int t) {
    obuf[0] = M_SHIP; obuf[1] = t; obuf[2] = pused[t]; obuf[3] = php[t]; obuf[4] = pang[t];
    obuf[5] = pthr[t];
    if (pinv[t] > 0) obuf[5] = obuf[5] | 2;
    put16(6, px[t]);
    put16(8, py[t]);
    put16(10, pvx[t]);
    put16(12, pvy[t]);
    obuf[14] = pammo[t];
    obuf[15] = (prt[t] + 59) / 60;
    put16(16, pscore[t]);
    obuf[18] = pkills[t]; obuf[19] = pdeaths[t];
    put16(20, pecho[t]);
    net_send(obuf, 22);
}

void send_bullets(int g) {
    int j, i;
    obuf[0] = M_BULLET; obuf[1] = g;
    for (j = 0; j < 10; j++) {
        i = g * 10 + j;
        if (blife[i] > 0) {
            obuf[2 + j * 3] = bx[i] >> 2;
            obuf[3 + j * 3] = by[i] >> 2;
            obuf[4 + j * 3] = bown[i];
        } else {
            obuf[4 + j * 3] = 255;
        }
    }
    net_send(obuf, 32);
}

void send_turrets(void) {
    int k;
    obuf[0] = M_TURRET; obuf[1] = nt;
    for (k = 0; k < nt; k++) {
        obuf[2 + k * 2] = thp[k] | (tboost[k] << 7);
        obuf[3 + k * 2] = taim[k];
    }
    net_send(obuf, 2 + nt * 2);
}

void host_broadcast(void) {
    int t;
    if ((frame & 1) == 0) {
        for (t = 0; t < MAXP; t++) {
            if (pused[t] != 0) send_ship(t);
        }
    } else {
        send_bullets(0);
        send_bullets(1);
        send_turrets();
    }
    if ((frame & 31) == 5) {
        obuf[0] = M_WORLD;
        put16(1, cave_seed);
        net_send(obuf, 3);
    }
    if ((frame & 15) == 3) {
        for (t = 0; t < MAXP; t++) {
            if (pused[t] != 0) send_player(t);
        }
    }
}

void send_join(void) {
    int i;
    obuf[0] = M_NAME; obuf[1] = mysid;
    for (i = 0; i < 8; i++) obuf[2 + i] = myname[i];
    net_send(obuf, 10);
}

void handle_message(int n) {
    int type, s, t, j, k, i, d;
    type = ibuf[0];
    s = net_sender();
    if (i_am_host) {
        if (type == M_NAME && n >= 10) {
            for (i = 0; i < 8; i++) tname[i] = ibuf[2 + i];
            tname[8] = 0;
            register_player(s, ibuf[1], tname);
        } else if (type == M_INPUT && n >= 4) {
            t = find_net(s);
            if (t >= 0) {
                pkeys[t] = ibuf[1];
                pecho[t] = get16(2);
            }
        }
        return;
    }
    if (s != hnet) return;                    // Only the host is trusted
    if (type == M_WORLD) {
        if (!cave_ok || get16(1) != cave_seed) make_world(get16(1));
    } else if (type == M_SHIP && n >= 22) {
        t = ibuf[1] & 7;
        pused[t] = ibuf[2]; php[t] = ibuf[3]; pang[t] = ibuf[4];
        pthr[t] = ibuf[5] & 1;
        if (ibuf[5] & 2) pinv[t] = 1;
        else pinv[t] = 0;
        px[t] = get16(6); py[t] = get16(8); pvx[t] = get16(10); pvy[t] = get16(12);
        pammo[t] = ibuf[14];
        prt[t] = ibuf[15] * 60;
        pscore[t] = get16(16);
        pkills[t] = ibuf[18]; pdeaths[t] = ibuf[19];
        pstale[t] = 0;
        if (t == me) {
            d = (gfx_ticks() - get16(20)) & 0xFFFF;
            if (d < 120) ping = (ping * 3 + d * 17) >> 2;
        }
    } else if (type == M_PLAYER && n >= 12) {
        t = ibuf[1] & 7;
        pnet[t] = ibuf[2]; psid[t] = ibuf[3];
        for (i = 0; i < 8; i++) names[t * 9 + i] = ibuf[4 + i];
        names[t * 9 + 8] = 0;
    } else if (type == M_BULLET && n >= 32) {
        for (j = 0; j < 10; j++) {
            i = (ibuf[1] & 1) * 10 + j;
            if (ibuf[4 + j * 3] == 255) {
                blife[i] = 0;
            } else {
                blife[i] = 1;
                bx[i] = ibuf[2 + j * 3] << 2;
                by[i] = ibuf[3 + j * 3] << 2;
                bown[i] = ibuf[4 + j * 3];
            }
        }
    } else if (type == M_TURRET && n >= 2) {
        for (k = 0; k < ibuf[1] && k < nt; k++) {
            thp[k] = ibuf[2 + k * 2] & 127;
            tboost[k] = ibuf[2 + k * 2] >> 7;
            taim[k] = ibuf[3 + k * 2] & 31;
        }
    } else if (type == M_EVENT && n >= 8) {
        handle_event(ibuf[1], ibuf[2], ibuf[3], get16(4), get16(6));
    }
}

void refresh_net(void) {
    int s;
    myslot = net_slot();
    nready = net_ready();
    hnet = -1;
    for (s = 0; s < MAXP; s++) {
        nact[s] = net_active(s);
        if (nact[s] && hnet < 0) hnet = s;
    }
    i_am_host = nready && myslot >= 0 && hnet == myslot;
}

/* Clients extrapolate ships between the 30 Hz snapshots */
void dead_reckon(void) {
    int t, s;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] != 0 && pstale[t] < 250) pstale[t] = pstale[t] + 1;
        if (pstale[t] > 90) { pused[t] = 0; php[t] = 0; }
        if (pused[t] == 1 && php[t] > 0) {
            s = psx[t] + pvx[t];
            px[t] = px[t] + (s >> 8);
            psx[t] = s & 255;
            s = psy[t] + pvy[t];
            py[t] = py[t] + (s >> 8);
            psy[t] = s & 255;
        }
    }
}

/* --- 10. Rendering --- */
int ox(int a) {
    return (dxT[a] * 3) >> 2;
}

int oy(int a) {
    return (dyT[a] * 3) >> 2;
}

void draw_ship(int t, int sx, int sy) {
    int a, l, r, col, nxp, nyp;
    col = pcol[t];
    a = pang[t];
    l = (a + 13) & 31;
    r = (a + 19) & 31;
    if (pinv[t] > 0 && (frame & 4)) col = DARK_GRAY;
    nxp = sx + ox(a); nyp = sy + oy(a);
    gfx_line(nxp, nyp, sx + ox(l), sy + oy(l), col);
    gfx_line(nxp, nyp, sx + ox(r), sy + oy(r), col);
    gfx_line(sx + ox(l), sy + oy(l), sx + ox(r), sy + oy(r), col);
    gfx_rect(sx - 1, sy - 1, 2, 2, col);
    if (pthr[t] && (frame & 2)) {
        l = (a + 16) & 31;
        gfx_line(sx, sy, sx + ox(l) + ox(l), sy + oy(l) + oy(l), ORANGE);
    }
}

void draw_turret(int k, int sx, int sy) {
    int a;
    if (thp[k] == 0) {
        gfx_rect(sx - 4, sy - 4, 8, 8, DARK_GRAY);
        if (ttimer[k] < 120 && (frame & 8)) gfx_rect(sx - 1, sy - 1, 3, 3, ORANGE);
        return;
    }
    a = taim[k];
    gfx_rect(sx - 6, sy - 6, 12, 12, DARK_RED);
    gfx_rect(sx - 4, sy - 4, 8, 8, RED);
    gfx_line(sx, sy, sx + dxT[a], sy + dyT[a], WHITE);
    if (tboost[k]) gfx_rect(sx - 1, sy - 1, 3, 3, YELLOW);
    if (thp[k] < TURRET_HP) gfx_rect(sx - 5, sy - 10, thp[k] / 6, 2, GREEN);
}

void draw_world(void) {
    int i, k, t, sx, sy;
    gfx_tilemap(cave, MW, MH, camx, camy);
    for (k = 0; k < nt; k++) {
        sx = tx[k] - camx; sy = ty[k] - camy;
        if (sx > -12 && sx < 332 && sy > -12 && sy < 252) draw_turret(k, sx, sy);
    }
    for (i = 0; i < MAXB; i++) {
        if (blife[i] > 0) {
            sx = bx[i] - camx; sy = by[i] - camy;
            if (bown[i] < 8) gfx_rect(sx - 1, sy - 1, 3, 3, pcol[bown[i]]);
            else gfx_rect(sx - 1, sy - 1, 3, 3, RED);
        }
    }
    for (i = 0; i < MAXPART; i++) {
        if (qlife[i] > 0) {
            sx = (qx[i] >> 2) - camx; sy = (qy[i] >> 2) - camy;
            if (sx >= 0 && sx < 320 && sy >= 0 && sy < 240) gfx_pixel(sx, sy, qcol[i]);
        }
    }
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && php[t] > 0) {
            sx = px[t] - camx; sy = py[t] - camy;
            if (sx > -30 && sx < 350 && sy > -30 && sy < 270) {
                draw_ship(t, sx, sy);
                if (t != me) gfx_text(sx - slen(names + t * 9) * 3, sy - 16, names + t * 9, pcol[t]);
            }
        }
    }
}

void draw_radar(void) {
    int k, t;
    gfx_rect(253, 189, 66, 50, GRAY);
    gfx_rect(254, 190, 64, 48, BLACK);
    gfx_sprite(254, 190, 64, 48, cave);
    for (k = 0; k < nt; k++) {
        if (thp[k] > 0) gfx_rect(254 + (tx[k] >> 4), 190 + (ty[k] >> 4), 2, 2, RED);
    }
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && php[t] > 0 && (t != me || (frame & 8))) {
            gfx_rect(253 + (px[t] >> 4), 189 + (py[t] >> 4), 3, 3, pcol[t]);
        }
    }
}

int order[8];
int nord;

void sort_players(void) {
    int t, i, j, v;
    nord = 0;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] != 0) { order[nord] = t; nord++; }
    }
    for (i = 1; i < nord; i++) {
        v = order[i];
        j = i - 1;
        while (j >= 0 && pscore[order[j]] < pscore[v]) { order[j + 1] = order[j]; j--; }
        order[j + 1] = v;
    }
}

void draw_mini_scores(void) {
    char buf[8];
    int i, t, c, y;
    for (i = 0; i < nord; i++) {
        t = order[i];
        y = 4 + i * 9;
        c = pcol[t];
        if (pused[t] == 2) c = DARK_GRAY;
        gfx_rect(236, y, 6, 6, c);
        names[t * 9 + 6] = 0;
        gfx_text(245, y, names + t * 9, c);
        names[t * 9 + 6] = ' ';
        number_text(pscore[t], buf);
        gfx_text(284, y, buf, c);
    }
}

void draw_full_scores(void) {
    char buf[8];
    int i, t, c, y;
    gfx_rect(30, 26, 260, 140, GRAY);
    gfx_rect(31, 27, 258, 138, BLACK);
    gfx_text(40, 32, "PLAYER", GRAY);
    gfx_text(130, 32, "SCORE", GRAY);
    gfx_text(180, 32, "KILLS", GRAY);
    gfx_text(224, 32, "DEATHS", GRAY);
    for (i = 0; i < nord; i++) {
        t = order[i];
        y = 46 + i * 14;
        c = pcol[t];
        if (pused[t] == 2) c = DARK_GRAY;
        gfx_rect(38, y, 8, 8, c);
        gfx_text(52, y, names + t * 9, c);
        number_text(pscore[t], buf);
        gfx_text(130, y, buf, c);
        number_text(pkills[t], buf);
        gfx_text(180, y, buf, c);
        number_text(pdeaths[t], buf);
        gfx_text(224, y, buf, c);
        if (pused[t] == 2) gfx_text(256, y, "OFF", DARK_GRAY);
        else if (php[t] == 0) gfx_text(256, y, "DEAD", RED);
    }
}

void draw_feed_name(int code, int x, int y) {
    if (code < 8) gfx_text(x, y, names + code * 9, pcol[code]);
    else if (code == 8) gfx_text(x, y, "TURRET", RED);
    else gfx_text(x, y, "CRASH", GRAY);
}

int feed_name_width(int code) {
    if (code < 8) return slen(names + code * 9) * 6;
    if (code == 8) return 36;
    return 30;
}

void draw_feed(void) {
    int i, x, y;
    for (i = 0; i < 4; i++) {
        if (ftime[i] > 0) {
            y = 228 - i * 9;
            draw_feed_name(fk[i], 4, y);
            x = 4 + feed_name_width(fk[i]);
            gfx_text(x + 3, y, ">", GRAY);
            draw_feed_name(fv[i], x + 14, y);
        }
    }
}

void draw_hud(void) {
    char buf[8];
    int n, t;
    if (me < 0) return;
    gfx_text(4, 4, names + me * 9, pcol[me]);
    gfx_text(4, 14, "SCORE", GRAY);
    number_text(pscore[me], buf);
    gfx_text(40, 14, buf, WHITE);
    gfx_text(4, 24, "HP", GRAY);
    gfx_rect(20, 24, 42, 6, GRAY);
    gfx_rect(21, 25, 40, 4, BLACK);
    gfx_rect(21, 25, (php[me] * 2) / 5, 4, GREEN);
    gfx_text(4, 33, "AMMO", GRAY);
    gfx_rect(32, 33, 32, 6, GRAY);
    gfx_rect(33, 34, 30, 4, BLACK);
    gfx_rect(33, 34, pammo[me], 4, YELLOW);
    n = 0;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1) n++;
    }
    gfx_text(4, 43, "PLAYERS", GRAY);
    number_text(n, buf);
    gfx_text(48, 43, buf, WHITE);
    if (i_am_host) {
        gfx_text(4, 52, "HOST", GREEN);
    } else {
        gfx_text(4, 52, "PING", GRAY);
        number_text(ping, buf);
        gfx_text(32, 52, buf, WHITE);
        gfx_text(32 + slen(buf) * 6, 52, "MS", GRAY);
    }
    if (php[me] == 0 && pused[me] == 1) {
        gfx_text(100, 100, "SHIP DESTROYED", RED);
        gfx_text(100, 112, "RESPAWN IN", WHITE);
        number_text(prt[me] / 60, buf);
        gfx_text(166, 112, buf, YELLOW);
    }
}

/* --- Menu and main loop --- */
void clear_name_pad(void) {
    int i;
    for (i = 0; i < 8; i++) {
        if (i < name_len) myname[i] = tname[i];
        else myname[i] = ' ';
    }
    myname[8] = 0;
}

void draw_menu(void) {
    int ch, i;
    ch = gfx_getchar();
    if (ch == 8 && name_len > 0) name_len--;
    else if (ch >= 'a' && ch <= 'z' && name_len < 8) { tname[name_len] = ch - 32; name_len++; }
    else if (((ch >= '0' && ch <= '9') || (ch >= 'A' && ch <= 'Z') || ch == ' ') && name_len < 8) {
        tname[name_len] = ch; name_len++;
    }
    gfx_tilemap(cave, MW, MH, 100 + ((frame >> 1) % 500), 200);
    gfx_rect(50, 20, 220, 200, BLACK);
    gfx_rect(50, 20, 220, 1, GRAY);
    gfx_rect(50, 219, 220, 1, GRAY);
    gfx_text(112, 30, "S P A C E C A V E", YELLOW);
    gfx_text(70, 46, "LAN MULTIPLAYER - UP TO 8 PILOTS", GRAY);
    if (show_help) {
        gfx_text(70, 70, "LEFT/RIGHT  ROTATE", WHITE);
        gfx_text(70, 82, "UP          THRUST", WHITE);
        gfx_text(70, 94, "SPACE       FIRE", WHITE);
        gfx_text(70, 106, "TAB         SCOREBOARD", WHITE);
        gfx_text(70, 126, "DESTROY PILOTS AND TURRETS,", GRAY);
        gfx_text(70, 136, "AVOID THE CAVE WALLS.", GRAY);
        gfx_text(70, 146, "RESPAWN TAKES 10 SECONDS.", GRAY);
        gfx_text(70, 190, "TAB: BACK", GREEN);
        return;
    }
    gfx_text(70, 80, "NICKNAME:", WHITE);
    for (i = 0; i < 8; i++) gfx_rect(140 + i * 8, 90, 7, 1, GRAY);
    tname[name_len] = 0;
    gfx_text(140, 80, tname, GREEN);
    if (frame & 16) gfx_rect(140 + name_len * 8, 88, 6, 2, YELLOW);
    gfx_text(70, 130, "ENTER: JOIN GAME", WHITE);
    gfx_text(70, 144, "TAB:   CONTROLS", WHITE);
    if (name_len == 0) gfx_text(70, 170, "TYPE A NAME FIRST", GRAY);
}

void join_game(void) {
    clear_name_pad();
    net_open("spacecave");
    rs = rs ^ gfx_ticks();
    mysid = 1 + rnd(250);
    cave_ok = 0;
    me = -1;
    was_host = 0;
    ping = 0;
    state = 2;
    reset_mirror();
}

void find_me(void) {
    int t;
    me = -1;
    for (t = 0; t < MAXP; t++) {
        if (pused[t] == 1 && psid[t] == mysid && pnet[t] == myslot) me = t;
    }
}

void play_frame(unsigned int keys) {
    int n, t;
    if ((frame % 6) == 0 || hnet < 0) refresh_net();
    if (!nready) {
        gfx_clear(BLACK);
        gfx_text(110, 110, "CONNECTING...", YELLOW);
        return;
    }
    if (i_am_host && !cave_ok) {
        make_world(rs ^ gfx_ticks());
        register_player(myslot, mysid, myname);
    }
    if (i_am_host && !was_host && cave_ok) {
        become_host();
        register_player(myslot, mysid, myname);
    }
    was_host = i_am_host;
    while ((n = net_recv(ibuf)) > 0) handle_message(n);
    if (!cave_ok) {
        gfx_clear(BLACK);
        gfx_text(100, 110, "WAITING FOR HOST...", YELLOW);
        return;
    }
    find_me();
    if (me < 0 && !i_am_host && (frame % 30) == 0) send_join();
    if (i_am_host) {
        check_disconnects();
        if (me >= 0) { pkeys[me] = keys & 31; pecho[me] = gfx_ticks(); }
        for (t = 0; t < MAXP; t++) {
            if (pused[t] == 1) sim_ship(t);
        }
        ship_collisions();
        sim_bullets();
        sim_turrets();
        host_broadcast();
    } else {
        obuf[0] = M_INPUT; obuf[1] = keys & 31;
        put16(2, gfx_ticks());
        net_send(obuf, 4);
        dead_reckon();
    }
    update_particles();
    if (me >= 0 && php[me] > 0) {
        camx = px[me] - 160;
        camy = py[me] - 120;
    }
    if (camx < 0) camx = 0;
    if (camy < 0) camy = 0;
    if (camx > WORLD_W - 320) camx = WORLD_W - 320;
    if (camy > WORLD_H - 240) camy = WORLD_H - 240;
    draw_world();
    sort_players();
    draw_hud();
    draw_mini_scores();
    draw_radar();
    draw_feed();
    if (keys & KEY_TAB) draw_full_scores();
    if (me >= 0 && pthr[me] && php[me] > 0) {
        if (!thr_snd) { gfx_sound(0, 70, 0, WAVE_NOISE, 25); thr_snd = 1; }
    } else if (thr_snd) {
        gfx_sound(0, 0, 0, 0, 0);
        thr_snd = 0;
    }
}

int main(void) {
    unsigned int keys;
    rs = 4242;
    gen_cave(77);
    state = 0;
    name_len = 0;
    while (gfx_getchar() != 0) { }
    while (1) {
        keys = gfx_keys();
        frame++;
        if (state == 2) {
            play_frame(keys);
        } else {
            draw_menu();
            if ((keys & KEY_TAB) && !(prev_keys & KEY_TAB)) show_help = 1 - show_help;
            if ((keys & KEY_START) && !(prev_keys & KEY_START) && name_len > 0 && !show_help) join_game();
        }
        prev_keys = keys;
        gfx_present();
    }
    return 0;
}
