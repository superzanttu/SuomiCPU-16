// Asteroids for the SC-8.  Left/Right (or A/D) rotate, Up (or W) thrusts,
// Space fires, Enter restarts after GAME OVER.
// Positions are kept in 1/16 pixel units, so the 320x240 screen is 5120x3840.
#include "suomi_gfx.h"

#define MAXA 12
#define MAXB 4
#define WORLD_W 5120
#define WORLD_H 3840

int dxT[32] = {
    0, 2, 3, 4, 6, 7, 7, 8, 8, 8, 7, 7, 6, 4, 3, 2,
    0, -2, -3, -4, -6, -7, -7, -8, -8, -8, -7, -7, -6, -4, -3, -2
};
int dyT[32] = {
    -8, -8, -7, -7, -6, -4, -3, -2, 0, 2, 3, 4, 6, 7, 7, 8,
    8, 8, 7, 7, 6, 4, 3, 2, 0, -2, -3, -4, -6, -7, -7, -8
};
// Asteroid outlines: 8 (dx, dy) points per size, small to large.
char ast_shape[48] = {
    0, -6, 3, -3, 6, 0, 4, 4, 0, 6, -3, 3, -6, 0, -4, -4,
    0, -11, 6, -6, 11, 0, 7, 7, 0, 11, -6, 6, -11, 0, -7, -7,
    0, -18, 10, -10, 18, 0, 11, 11, 0, 18, -10, 10, -18, 0, -11, -11
};
// Collision half-sizes in 1/16 pixel units for small, medium and large rocks.
int hitbox[4] = {0, 112, 192, 304};
int points[4] = {0, 100, 50, 20};

int ax[MAXA];
int ay[MAXA];
int avx[MAXA];
int avy[MAXA];
int asz[MAXA];
int bx[MAXB];
int by[MAXB];
int bvx[MAXB];
int bvy[MAXB];
int blife[MAXB];

int shipx;
int shipy;
int shipvx;
int shipvy;
int ang;
int invuln;
int cooldown;
int lives;
int level;
int frame;
int over;
int thrusting;
int wait;
unsigned int score;

int iabs(int v)
{
    if (v < 0) {
        return -v;
    }
    return v;
}

int wrapx(int v)
{
    if (v < 0) {
        return v + WORLD_W;
    }
    if (v >= WORLD_W) {
        return v - WORLD_W;
    }
    return v;
}

int wrapy(int v)
{
    if (v < 0) {
        return v + WORLD_H;
    }
    if (v >= WORLD_H) {
        return v - WORLD_H;
    }
    return v;
}

int clamp(int v, int limit)
{
    if (v > limit) {
        return limit;
    }
    if (v < -limit) {
        return -limit;
    }
    return v;
}

int random_speed(void)
{
    int v;
    v = (gfx_random() & 15) - 8;
    if (v >= 0) {
        return v + 3;
    }
    return v - 3;
}

void spawn(int size, int x, int y)
{
    int i;
    for (i = 0; i < MAXA; i++) {
        if (asz[i] == 0) {
            asz[i] = size;
            ax[i] = x;
            ay[i] = y;
            avx[i] = random_speed();
            avy[i] = random_speed();
            return;
        }
    }
}

void start_level(void)
{
    int i;
    int count;
    count = level + 3;
    if (count > 8) {
        count = 8;
    }
    for (i = 0; i < count; i++) {
        if (i & 1) {
            spawn(3, gfx_random() << 4, 3200);
        } else {
            spawn(3, gfx_random() << 4, 100);
        }
    }
}

void respawn_ship(void)
{
    shipx = 2560;
    shipy = 1920;
    shipvx = 0;
    shipvy = 0;
    ang = 0;
    invuln = 120;
}

void new_game(void)
{
    int i;
    for (i = 0; i < MAXA; i++) {
        asz[i] = 0;
    }
    for (i = 0; i < MAXB; i++) {
        blife[i] = 0;
    }
    score = 0;
    lives = 3;
    level = 1;
    over = 0;
    wait = 0;
    respawn_ship();
    start_level();
}

void fire(void)
{
    int i;
    for (i = 0; i < MAXB; i++) {
        if (blife[i] == 0) {
            blife[i] = 40;
            bx[i] = shipx + (dxT[ang] << 4);
            by[i] = shipy + (dyT[ang] << 4);
            bvx[i] = dxT[ang] << 3;
            bvy[i] = dyT[ang] << 3;
            cooldown = 8;
            return;
        }
    }
}

void hit_asteroid(int i)
{
    int size;
    size = asz[i];
    score += points[size];
    if (score > 60000) {
        score = 60000;
    }
    if (size > 1) {
        asz[i] = size - 1;
        avx[i] = random_speed();
        avy[i] = random_speed();
        spawn(size - 1, ax[i], ay[i]);
    } else {
        asz[i] = 0;
    }
}

void update_ship(unsigned int keys)
{
    if ((frame & 1) == 0) {
        if (keys & KEY_LEFT) {
            ang = (ang + 31) & 31;
        }
        if (keys & KEY_RIGHT) {
            ang = (ang + 1) & 31;
        }
    }
    thrusting = 0;
    if (keys & KEY_UP) {
        thrusting = 1;
        if ((frame & 3) == 0) {
            shipvx = clamp(shipvx + dxT[ang], 40);
            shipvy = clamp(shipvy + dyT[ang], 40);
        }
    }
    if ((frame & 7) == 0) {
        if (shipvx > 0) {
            shipvx--;
        }
        if (shipvx < 0) {
            shipvx++;
        }
        if (shipvy > 0) {
            shipvy--;
        }
        if (shipvy < 0) {
            shipvy++;
        }
    }
    shipx = wrapx(shipx + shipvx);
    shipy = wrapy(shipy + shipvy);
    if (cooldown > 0) {
        cooldown--;
    }
    if ((keys & KEY_FIRE) && cooldown == 0) {
        fire();
    }
    if (invuln > 0) {
        invuln--;
    }
}

void update_world(void)
{
    int i;
    int j;
    int alive;
    int dx;
    int dy;
    int reach;
    int px;
    int py;

    alive = 0;
    for (i = 0; i < MAXA; i++) {
        if (asz[i] != 0) {
            alive++;
            ax[i] = wrapx(ax[i] + avx[i]);
            ay[i] = wrapy(ay[i] + avy[i]);
        }
    }
    for (i = 0; i < MAXB; i++) {
        if (blife[i] > 0) {
            blife[i] = blife[i] - 1;
            px = wrapx(bx[i] + bvx[i]);
            py = wrapy(by[i] + bvy[i]);
            bx[i] = px;
            by[i] = py;
            for (j = 0; j < MAXA; j++) {
                if (asz[j] != 0) {
                    reach = hitbox[asz[j]];
                    dx = px - ax[j];
                    if (dx < 0) {
                        dx = -dx;
                    }
                    if (dx < reach) {
                        dy = py - ay[j];
                        if (dy < 0) {
                            dy = -dy;
                        }
                        if (dy < reach) {
                            blife[i] = 0;
                            hit_asteroid(j);
                            j = MAXA;
                        }
                    }
                }
            }
        }
    }
    if (invuln == 0 && over == 0) {
        for (j = 0; j < MAXA; j++) {
            if (asz[j] != 0) {
                reach = hitbox[asz[j]] + 64;
                dx = shipx - ax[j];
                if (dx < 0) {
                    dx = -dx;
                }
                if (dx < reach) {
                    dy = shipy - ay[j];
                    if (dy < 0) {
                        dy = -dy;
                    }
                    if (dy < reach) {
                        lives--;
                        if (lives == 0) {
                            over = 1;
                            wait = 60;
                        } else {
                            respawn_ship();
                        }
                        j = MAXA;
                    }
                }
            }
        }
    }
    if (alive == 0 && over == 0) {
        level++;
        start_level();
    }
}

void number_text(unsigned int value, char *out)
{
    char tmp[6];
    int n;
    int i;
    n = 0;
    if (value == 0) {
        tmp[0] = '0';
        n = 1;
    }
    while (value > 0) {
        tmp[n] = '0' + (value % 10);
        value = value / 10;
        n++;
    }
    for (i = 0; i < n; i++) {
        out[i] = tmp[n - 1 - i];
    }
    out[n] = 0;
}

void draw_asteroid(int i)
{
    gfx_poly(ax[i] >> 4, ay[i] >> 4, 8, ast_shape + ((asz[i] - 1) << 4), WHITE);
}

void draw_ship(void)
{
    int cx;
    int cy;
    int nose;
    int left;
    int right;
    cx = shipx >> 4;
    cy = shipy >> 4;
    nose = ang;
    left = (ang + 13) & 31;
    right = (ang + 19) & 31;
    gfx_line(cx + dxT[nose], cy + dyT[nose], cx + dxT[left], cy + dyT[left], CYAN);
    gfx_line(cx + dxT[nose], cy + dyT[nose], cx + dxT[right], cy + dyT[right], CYAN);
    gfx_line(cx + dxT[left], cy + dyT[left], cx + dxT[right], cy + dyT[right], CYAN);
    if (thrusting && (frame & 2)) {
        left = (ang + 16) & 31;
        gfx_line(cx, cy, cx + dxT[left], cy + dyT[left], ORANGE);
    }
}

void draw_hud(void)
{
    char buffer[8];
    int i;
    number_text(score, buffer);
    gfx_text(4, 4, "SCORE", GRAY);
    gfx_text(40, 4, buffer, WHITE);
    for (i = 0; i < lives; i++) {
        gfx_line(300 - i * 10, 12, 303 - i * 10, 4, CYAN);
        gfx_line(303 - i * 10, 4, 306 - i * 10, 12, CYAN);
    }
}

void draw(void)
{
    int i;
    gfx_clear(BLACK);
    for (i = 0; i < MAXA; i++) {
        if (asz[i] != 0) {
            draw_asteroid(i);
        }
    }
    for (i = 0; i < MAXB; i++) {
        if (blife[i] > 0) {
            gfx_rect(bx[i] >> 4, by[i] >> 4, 2, 2, YELLOW);
        }
    }
    if (over == 0 && (invuln & 4) == 0) {
        draw_ship();
    }
    draw_hud();
    if (over) {
        gfx_text(124, 108, "GAME OVER", RED);
        gfx_text(100, 124, "PRESS ENTER TO RESTART", WHITE);
    }
}

int main(void)
{
    unsigned int keys;
    new_game();
    while (1) {
        keys = gfx_keys();
        if (over) {
            if (wait > 0) {
                wait--;
            } else if (keys & (KEY_START | KEY_FIRE)) {
                new_game();
            }
        } else {
            update_ship(keys);
        }
        update_world();
        draw();
        gfx_present();
        frame++;
    }
    return 0;
}
