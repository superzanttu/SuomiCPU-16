// Infinite City for SC-16
// ICON
// ################
// #..............#
// #..............#
// #..#...........#
// #..#..########.#
// #..#..#......#.#
// #..#..########.#
// #..#...........#
// #..#############
// #..............#
// #..##..##..##..#
// #..##..##..##..#
// #..............#
// #....########..#
// #..............#
// ################
#include "suomi_gfx.h"

#define BUILDING_MAX 48
#define ENEMY_MAX 12
#define ROCKET_MAX 6
#define PICKUP_MAX 8
#define BOMB_MAX 16
#define PLAYER_W 12
#define PLAYER_H 16
#define SCREEN_CX 160
#define GROUND_Y 200
#define ROOF_BASE 128

#define ENEMY_MRSIREN 0
#define ENEMY_MEGABIRD 1
#define ENEMY_SPRINGU 2

typedef struct
{
    int x;
    int y;
    int w;
    int h;
    int roof_y;
    int ladder;
} Building;

typedef struct
{
    int x;
    int y;
    int vx;
    int vy;
    int alive;
    int kind;
    int hp;
    int anim;
    int phase;
    int on_ground;
    int carried;
    int bomb_cd;
} Enemy;

typedef struct
{
    int x;
    int y;
    int vx;
    int vy;
    int life;
    int active;
    int damage;
    int from_player;
} Rocket;

typedef struct
{
    int x;
    int y;
    int vy;
    int alive;
} Bomb;

typedef struct
{
    int x;
    int y;
    int vx;
    int vy;
    int on_ground;
    int on_ladder;
    int climb_dir;
    int facing;
    int hp;
    int ammo;
    int fire_cd;
    int anim;
    int fall_start;
    int fall_height;
    int invuln;
    int carried_by;
    int last_y;
} Player;

typedef struct
{
    int x;
    int y;
    int alive;
    int amount;
} Pickup;

Building city[BUILDING_MAX];
Enemy enemies[ENEMY_MAX];
Rocket rockets[ROCKET_MAX];
Bomb bombs[BOMB_MAX];
Pickup pickups[PICKUP_MAX];
Player player;
int camera_x;
int frame_count;
int spawn_timer;
int game_over;
int restart_wait;
int kills_mr;
int kills_bird;
int kills_spring;
int rockets_fired;
int rockets_hit;
int total_kills;
int sky_phase;
unsigned int prev_keys;

void number_text(int value, char *out)
{
    char tmp[16];
    int n = 0;
    int i;
    int sign = 0;
    if (value < 0)
    {
        sign = 1;
        value = -value;
    }
    if (value == 0)
    {
        tmp[n++] = '0';
    }
    while (value > 0)
    {
        tmp[n++] = '0' + (value % 10);
        value = value / 10;
    }
    if (sign)
    {
        out[0] = '-';
        i = 1;
    }
    else
    {
        i = 0;
    }
    while (n > 0)
    {
        out[i++] = tmp[n - 1];
        n--;
    }
    out[i] = 0;
}

void init_city(void)
{
    int i;
    int x = 0;
    int roof;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        city[i].w = 26 + (gfx_random() % 42);
        city[i].h = 52 + (gfx_random() % 104);
        city[i].x = x;
        roof = 180 - city[i].h;
        if (roof < 60)
        {
            roof = 60;
        }
        city[i].roof_y = roof;
        city[i].ladder = ((gfx_random() & 3) == 0) ? 1 : 0;
        x += city[i].w + 16 + (gfx_random() % 18);
    }
}

void reset_game(void)
{
    int i;
    camera_x = 0;
    frame_count = 0;
    spawn_timer = 40;
    game_over = 0;
    restart_wait = 0;
    kills_mr = 0;
    kills_bird = 0;
    kills_spring = 0;
    total_kills = 0;
    rockets_fired = 0;
    rockets_hit = 0;
    prev_keys = 0;
    sky_phase = 0;
    player.x = 36;
    player.y = 152;
    player.vx = 0;
    player.vy = 0;
    player.on_ground = 0;
    player.on_ladder = 0;
    player.climb_dir = 0;
    player.facing = 1;
    player.hp = 100;
    player.ammo = 10;
    player.fire_cd = 0;
    player.anim = 0;
    player.fall_start = 0;
    player.fall_height = 0;
    player.invuln = 0;
    player.carried_by = -1;
    player.last_y = player.y;
    for (i = 0; i < ENEMY_MAX; i++)
    {
        enemies[i].alive = 0;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        rockets[i].active = 0;
    }
    for (i = 0; i < BOMB_MAX; i++)
    {
        bombs[i].alive = 0;
    }
    for (i = 0; i < PICKUP_MAX; i++)
    {
        pickups[i].alive = 0;
    }
    for (i = 0; i < 3; i++)
    {
        pickups[i].alive = 1;
        pickups[i].x = 220 + i * 240;
        pickups[i].y = 150 + (i * 10);
        pickups[i].amount = 5;
    }
}

int clamp_to_range(int value, int lo, int hi)
{
    if (value < lo)
    {
        return lo;
    }
    if (value > hi)
    {
        return hi;
    }
    return value;
}

int building_roof_at_world(int wx)
{
    int i;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (wx >= city[i].x && wx < city[i].x + city[i].w)
        {
            return city[i].roof_y;
        }
    }
    return GROUND_Y;
}

int building_ladder_at_world(int wx)
{
    int i;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (city[i].ladder && wx >= city[i].x && wx < city[i].x + city[i].w)
        {
            return city[i].x + city[i].w / 2;
        }
    }
    return -1;
}

void spawn_enemy(void)
{
    int i;
    int kind;
    int x;
    int y;
    kind = gfx_random() % 3;
    x = camera_x + 330 + (gfx_random() % 40);
    if (kind == ENEMY_MRSIREN)
    {
        y = GROUND_Y - 10;
    }
    else if (kind == ENEMY_MEGABIRD)
    {
        y = 36 + (gfx_random() % 80);
    }
    else
    {
        y = 150 + (gfx_random() % 30);
    }
    for (i = 0; i < ENEMY_MAX; i++)
    {
        if (!enemies[i].alive)
        {
            enemies[i].alive = 1;
            enemies[i].kind = kind;
            enemies[i].x = x;
            enemies[i].y = y;
            enemies[i].vx = 0;
            enemies[i].vy = 0;
            enemies[i].hp = (kind == ENEMY_MEGABIRD) ? 2 : 1;
            enemies[i].anim = 0;
            enemies[i].phase = 0;
            enemies[i].on_ground = 0;
            enemies[i].carried = 0;
            enemies[i].bomb_cd = 0;
            return;
        }
    }
}

void fire_rocket(void)
{
    int i;
    int dx;
    int dy;
    int length;
    int rocket_x;
    int rocket_y;
    int active_rockets = 0;
    if (player.fire_cd > 0 || player.ammo <= 0)
    {
        return;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (rockets[i].active)
        {
            active_rockets++;
        }
    }
    if (active_rockets >= 3)
    {
        return;
    }
    dx = gfx_mouse_x() - (player.x - camera_x + 6);
    dy = gfx_mouse_y() - (player.y + 8);
    if (dx == 0 && dy == 0)
    {
        dx = 1;
        dy = 1;
    }
    if (dx < 0)
    {
        dx = -dx;
    }
    if (dy < 0)
    {
        dy = -dy;
    }
    rocket_x = player.x - camera_x + 6;
    rocket_y = player.y + 8;
    length = dx;
    if (dy > length)
    {
        length = dy;
    }
    if (length == 0)
    {
        length = 1;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (!rockets[i].active)
        {
            rockets[i].x = rocket_x;
            rockets[i].y = rocket_y;
            rockets[i].vx = (gfx_mouse_x() - rocket_x) * 6 / length;
            rockets[i].vy = (gfx_mouse_y() - rocket_y) * 6 / length;
            rockets[i].life = 120;
            rockets[i].active = 1;
            rockets[i].damage = 50;
            rockets[i].from_player = 1;
            player.ammo--;
            player.fire_cd = 9;
            rockets_fired++;
            return;
        }
    }
}

void spawn_pickup(int x, int y)
{
    int i;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            pickups[i].alive = 1;
            pickups[i].x = x;
            pickups[i].y = y;
            pickups[i].amount = 5;
            return;
        }
    }
}

void update_player(unsigned int keys)
{
    int ladder_x;
    int roof_y;
    int player_screen_x;
    int push_x;
    int px;
    int py;
    int next_roof;
    int on_ground_before;
    int new_y;
    if (game_over)
    {
        return;
    }
    if (player.carried_by >= 0)
    {
        return;
    }
    player.last_y = player.y;
    player.vx = 0;
    if (keys & KEY_LEFT)
    {
        player.vx -= 2;
        player.facing = -1;
    }
    if (keys & KEY_RIGHT)
    {
        player.vx += 2;
        player.facing = 1;
    }
    ladder_x = building_ladder_at_world(player.x + 6);
    player.on_ladder = 0;
    if (ladder_x >= 0 && abs(player.x + 6 - ladder_x) < 6)
    {
        player.on_ladder = 1;
    }
    if (player.on_ladder && (keys & KEY_UP || keys & KEY_DOWN))
    {
        player.vy = 0;
        if (keys & KEY_UP)
        {
            player.vy -= 2;
        }
        if (keys & KEY_DOWN)
        {
            player.vy += 2;
        }
        player.climb_dir = (keys & KEY_UP) ? -1 : 1;
        player.y += player.vy;
        player.y = clamp_to_range(player.y, 20, 190);
        player.on_ground = 0;
    }
    else
    {
        if (keys & KEY_UP && player.on_ground)
        {
            player.vy = -5;
            player.on_ground = 0;
        }
    }
    if (keys & KEY_DOWN && player.on_ground)
    {
        if (building_roof_at_world(player.x + 6) < GROUND_Y)
        {
            player.on_ground = 0;
            player.vy = 1;
        }
    }
    player.vy += 1;
    if (player.vy > 7)
    {
        player.vy = 7;
    }
    player.x += player.vx;
    player.y += player.vy;
    if (player.x < 12)
    {
        player.x = 12;
    }
    if (player.x > 5000)
    {
        player.x = 5000;
    }
    player_screen_x = player.x - camera_x;
    px = player.x + 6;
    py = player.y + PLAYER_H;
    on_ground_before = player.on_ground;
    roof_y = building_roof_at_world(px);
    if (player.vy >= 0 && py >= roof_y && player.last_y + PLAYER_H <= roof_y + 4)
    {
        player.y = roof_y - PLAYER_H;
        player.vy = 0;
        player.on_ground = 1;
    }
    else if (player.y + PLAYER_H >= GROUND_Y && player.last_y + PLAYER_H <= GROUND_Y + 4)
    {
        player.y = GROUND_Y - PLAYER_H;
        player.vy = 0;
        player.on_ground = 1;
    }
    else
    {
        player.on_ground = 0;
    }
    if (player.y > 220)
    {
        player.y = 220;
    }
    if (player.on_ground && !on_ground_before && player.last_y > player.y + 30)
    {
        player.fall_height = player.last_y - player.y;
        if (player.fall_height > 72)
        {
            player.hp -= 50;
            player.invuln = 20;
        }
    }
    if (player.on_ground)
    {
        player.fall_start = 0;
    }
    if (player.hp <= 0)
    {
        player.hp = 0;
        game_over = 1;
    }
    player.anim++;
    if (player.anim > 3)
    {
        player.anim = 0;
    }
    if (player.fire_cd > 0)
    {
        player.fire_cd--;
    }
    if (player.invuln > 0)
    {
        player.invuln--;
    }
    camera_x = player.x - 100;
    if (camera_x < 0)
    {
        camera_x = 0;
    }
}

void draw_player(int sx, int sy)
{
    int frame = player.anim & 1;
    int body_y = sy + 7;
    int head_y = sy + 2;
    int leg_t = sy + 14;
    int x0 = sx;
    int body_x = sx + 4;
    gui:
    gfx_rect(sx + 2, head_y, 8, 5, WHITE);
    gfx_rect(sx + 1, head_y + 5, 10, 5, YELLOW);
    gfx_rect(sx + 2, body_y, 8, 6, YELLOW);
    gfx_rect(sx + 1, body_y + 6, 2, 5, BLUE);
    gfx_rect(sx + 9, body_y + 6, 2, 5, BLUE);
    gfx_rect(sx + 0, sy + 11, 3, 5, DARK_RED);
    gfx_rect(sx + 9, sy + 11, 3, 5, DARK_RED);
    gfx_rect(sx + 3, sy + 12, 2, 2, BLACK);
    gfx_rect(sx + 7, sy + 12, 2, 2, BLACK);
    if (player.on_ladder)
    {
        gfx_rect(sx + 4, sy + 2, 2, 12, CYAN);
    }
    if (player.climb_dir != 0)
    {
        gfx_rect(sx + 4, sy + 3, 1, 1, ORANGE);
    }
    if (player.vx != 0)
    {
        if (frame == 0)
        {
            gfx_rect(sx + 1, leg_t, 3, 4, MAGENTA);
            gfx_rect(sx + 8, leg_t + 1, 3, 3, MAGENTA);
        }
        else
        {
            gfx_rect(sx + 2, leg_t + 1, 3, 3, MAGENTA);
            gfx_rect(sx + 8, leg_t, 3, 4, MAGENTA);
        }
    }
    else
    {
        gfx_rect(sx + 2, leg_t, 3, 4, MAGENTA);
        gfx_rect(sx + 7, leg_t, 3, 4, MAGENTA);
    }
    if (player.invuln > 0 && (frame_count & 1))
    {
        gfx_rect(sx - 1, sy - 1, 14, 20, YELLOW);
    }
}

void draw_city(void)
{
    int i;
    int sx;
    int x0;
    int x1;
    int y;
    int row;
    int cell;
    int lx;
    int ly;
    int win_x;
    int win_y;
    gfx_rect(0, 0, 320, 240, BLACK);
    gfx_rect(0, 0, 320, 100, BLUE);
    for (i = 0; i < 32; i++)
    {
        x0 = ((i * 47) - (camera_x % 47)) % 320;
        y = 20 + ((i * 13) & 31);
        gfx_pixel(x0, y, WHITE);
        gfx_pixel(x0 + 1, y - 2, WHITE);
        gfx_pixel(x0 + 2, y + 2, WHITE);
    }
    x0 = -(camera_x % 60);
    for (i = 0; i < 14; i++)
    {
        sx = x0 + i * 60;
        gfx_line(sx, 210, sx + 18, 210, DARK_GRAY);
        gfx_line(sx + 18, 210, sx + 18, 225, DARK_GRAY);
    }
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (city[i].x + city[i].w < camera_x - 40)
        {
            continue;
        }
        if (city[i].x > camera_x + 360)
        {
            break;
        }
        sx = city[i].x - camera_x;
        gfx_rect(sx, city[i].roof_y, city[i].w, 200 - city[i].roof_y, DARK_GRAY);
        gfx_rect(sx + 2, city[i].roof_y + 2, city[i].w - 4, 6, CYAN);
        for (row = 0; row < 6; row++)
        {
            for (cell = 0; cell < 3; cell++)
            {
                win_x = sx + 6 + cell * 12;
                win_y = city[i].roof_y + 16 + row * 16;
                if (win_x < 320 && win_y < 240)
                {
                    if (((row + cell + i) & 1) == 0)
                    {
                        gfx_rect(win_x, win_y, 6, 8, YELLOW);
                    }
                    else
                    {
                        gfx_rect(win_x, win_y, 6, 8, ORANGE);
                    }
                }
            }
        }
        if (city[i].ladder)
        {
            lx = sx + city[i].w / 2;
            ly = city[i].roof_y + 2;
            gfx_rect(lx, ly, 2, 18, GRAY);
            for (row = 0; row < 6; row++)
            {
                gfx_line(lx - 4, ly + row * 4, lx + 4, ly + row * 4, CYAN);
            }
        }
    }
    gfx_rect(0, GROUND_Y, 320, 40, DARK_GRAY);
    gfx_line(0, GROUND_Y, 319, GROUND_Y, GRAY);
}

void draw_pickups(void)
{
    int i;
    int sx;
    int sy;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            continue;
        }
        sx = pickups[i].x - camera_x;
        sy = pickups[i].y;
        if (sx < -10 || sx > 330)
        {
            continue;
        }
        gfx_rect(sx, sy, 8, 8, GREEN);
        gfx_rect(sx + 2, sy + 2, 4, 4, YELLOW);
        gfx_line(sx + 4, sy - 2, sx + 4, sy + 10, WHITE);
        gfx_line(sx - 2, sy + 4, sx + 10, sy + 4, WHITE);
    }
}

void draw_enemy(Enemy *e)
{
    int sx;
    int sy;
    int step;
    if (!e->alive)
    {
        return;
    }
    sx = e->x - camera_x;
    sy = e->y;
    if (sx < -20 || sx > 340)
    {
        return;
    }
    switch (e->kind)
    {
        case ENEMY_MRSIREN:
            gfx_rect(sx - 8, sy, 16, 10, MAGENTA);
            gfx_rect(sx - 7, sy - 3, 14, 4, DARK_RED);
            gfx_rect(sx - 10, sy + 2, 3, 4, BLACK);
            gfx_rect(sx + 7, sy + 2, 3, 4, BLACK);
            gfx_rect(sx - 9, sy + 9, 3, 2, YELLOW);
            gfx_rect(sx + 6, sy + 9, 3, 2, YELLOW);
            if ((frame_count & 1) == 0)
            {
                gfx_rect(sx - 6, sy + 10, 3, 3, GREEN);
                gfx_rect(sx + 3, sy + 10, 3, 3, GREEN);
            }
            else
            {
                gfx_rect(sx - 6, sy + 10, 2, 3, GREEN);
                gfx_rect(sx + 4, sy + 10, 2, 3, GREEN);
            }
            break;
        case ENEMY_MEGABIRD:
            step = (frame_count >> 1) & 1;
            gfx_rect(sx - 8, sy, 16, 6, GRAY);
            gfx_rect(sx - 12, sy + 2, 4, 2, GRAY);
            gfx_rect(sx + 8, sy + 2, 4, 2, GRAY);
            if (step == 0)
            {
                gfx_line(sx - 10, sy + 1, sx - 2, sy - 3, WHITE);
                gfx_line(sx + 2, sy - 3, sx + 10, sy + 1, WHITE);
            }
            else
            {
                gfx_line(sx - 10, sy + 1, sx - 1, sy + 4, WHITE);
                gfx_line(sx + 1, sy + 4, sx + 10, sy + 1, WHITE);
            }
            gfx_rect(sx - 2, sy + 6, 4, 3, DARK_RED);
            break;
        case ENEMY_SPRINGU:
            gfx_rect(sx - 7, sy - 2, 14, 12, CYAN);
            gfx_rect(sx - 6, sy + 4, 12, 4, BLUE);
            gfx_pixel(sx, sy + 2, RED);
            gfx_line(sx - 8, sy + 8, sx + 8, sy + 8, WHITE);
            break;
    }
}

void draw_bombs(void)
{
    int i;
    int sx;
    int sy;
    for (i = 0; i < BOMB_MAX; i++)
    {
        if (!bombs[i].alive)
        {
            continue;
        }
        sx = bombs[i].x - camera_x;
        sy = bombs[i].y;
        if (sx < -8 || sx > 340)
        {
            continue;
        }
        gfx_rect(sx - 2, sy - 2, 5, 5, ORANGE);
        gfx_pixel(sx, sy + 3, RED);
    }
}

void draw_rocket(Rocket *r)
{
    int sx;
    int sy;
    if (!r->active)
    {
        return;
    }
    sx = r->x - camera_x;
    sy = r->y;
    if (sx < -5 || sx > 330 || sy < -5 || sy > 245)
    {
        return;
    }
    gfx_rect(sx - 1, sy - 1, 3, 3, YELLOW);
    gfx_rect(sx, sy, 1, 1, RED);
}

void draw_hud(void)
{
    char s[32];
    gfx_rect(0, 0, 320, 24, DARK_GRAY);
    gfx_rect(0, 0, 320, 2, WHITE);
    number_text(player.hp, s);
    gfx_text(8, 5, "HP:", WHITE);
    gfx_text(38, 5, s, WHITE);
    number_text(player.x / 10, s);
    gfx_text(100, 5, "M:", WHITE);
    gfx_text(120, 5, s, YELLOW);
    gfx_text(180, 5, "MR:", WHITE);
    number_text(kills_mr, s);
    gfx_text(208, 5, s, WHITE);
    gfx_text(235, 5, "B:", WHITE);
    number_text(kills_bird, s);
    gfx_text(248, 5, s, WHITE);
    gfx_text(275, 5, "S:", WHITE);
    number_text(kills_spring, s);
    gfx_text(288, 5, s, WHITE);
    gfx_rect(8, 18, 100, 5, DARK_GRAY);
    gfx_rect(8, 18, (player.hp * 100) / 100, 5, GREEN);
    gfx_text(8, 28, "AMMO", WHITE);
    number_text(player.ammo, s);
    gfx_text(55, 28, s, YELLOW);
    gfx_text(116, 28, "FIRED", WHITE);
    number_text(rockets_fired, s);
    gfx_text(170, 28, s, YELLOW);
    gfx_text(216, 28, "HITS", WHITE);
    number_text(rockets_hit, s);
    gfx_text(250, 28, s, YELLOW);
}

void update_enemies(void)
{
    int i;
    int dx;
    int dy;
    int dist;
    int roof;
    for (i = 0; i < ENEMY_MAX; i++)
    {
        Enemy *e = &enemies[i];
        if (!e->alive)
        {
            continue;
        }
        e->anim++;
        switch (e->kind)
        {
            case ENEMY_MRSIREN:
                if (player.x > e->x)
                {
                    e->x += 1;
                }
                else
                {
                    e->x -= 1;
                }
                if (player.vx != 0 && abs(player.x - e->x) < 40)
                {
                    e->x += player.facing;
                }
                e->y = GROUND_Y - 10;
                if (abs(player.x - e->x) < 12 && abs(player.y - e->y) < 12)
                {
                    player.hp -= 10;
                    player.invuln = 18;
                    e->alive = 0;
                    kills_mr++;
                    total_kills++;
                }
                break;
            case ENEMY_MEGABIRD:
                e->x -= 2;
                if (e->carried)
                {
                    e->phase++;
                    player.x = e->x + 12;
                    player.y = e->y - 6;
                    if (e->phase > 36)
                    {
                        e->carried = 0;
                        player.vy = 2;
                        player.on_ground = 0;
                        player.carried_by = -1;
                        e->alive = 0;
                        kills_bird++;
                        total_kills++;
                    }
                }
                else
                {
                    if (abs(player.x - e->x) < 70 && abs(player.y - e->y) < 46)
                    {
                        e->carried = 1;
                        e->phase = 0;
                        player.carried_by = i;
                        player.vx = 0;
                    }
                    e->y += ((frame_count + i) & 1) ? 1 : -1;
                    if (e->bomb_cd <= 0)
                    {
                        int b;
                        for (b = 0; b < BOMB_MAX; b++)
                        {
                            if (!bombs[b].alive)
                            {
                                bombs[b].alive = 1;
                                bombs[b].x = e->x;
                                bombs[b].y = e->y + 5;
                                bombs[b].vy = 2;
                                break;
                            }
                        }
                        e->bomb_cd = 36;
                    }
                    else
                    {
                        e->bomb_cd--;
                    }
                }
                if (abs(player.x - e->x) < 14 && abs(player.y - e->y) < 12 && !e->carried)
                {
                    player.hp -= 30;
                    player.invuln = 20;
                    e->alive = 0;
                    kills_bird++;
                    total_kills++;
                }
                if (e->x < camera_x - 30)
                {
                    e->alive = 0;
                }
                break;
            case ENEMY_SPRINGU:
                if (e->on_ground)
                {
                    e->vy = -4;
                    e->on_ground = 0;
                }
                e->vy += 1;
                e->y += e->vy;
                if (e->y >= GROUND_Y - 10)
                {
                    e->y = GROUND_Y - 10;
                    e->vy = 0;
                    e->on_ground = 1;
                }
                if (e->x > player.x)
                {
                    e->x -= 1;
                }
                else
                {
                    e->x += 1;
                }
                if (abs(player.x - e->x) < 10 && abs(player.y - e->y) < 12)
                {
                    player.hp -= 10;
                    player.invuln = 18;
                }
                break;
        }
        if (e->x < camera_x - 20)
        {
            e->alive = 0;
        }
    }
    for (i = 0; i < BOMB_MAX; i++)
    {
        if (!bombs[i].alive)
        {
            continue;
        }
        bombs[i].y += bombs[i].vy;
        if (bombs[i].y > GROUND_Y + 20 || bombs[i].y < 0)
        {
            bombs[i].alive = 0;
            continue;
        }
        if (abs(player.x - bombs[i].x) < 12 && abs(player.y - bombs[i].y) < 10)
        {
            player.hp -= 20;
            player.invuln = 10;
            bombs[i].alive = 0;
        }
    }
}

void update_pickups(void)
{
    int i;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            continue;
        }
        if (abs(player.x - pickups[i].x) < 12 && abs(player.y - pickups[i].y) < 12)
        {
            player.ammo += pickups[i].amount;
            if (player.ammo > 10)
            {
                player.ammo = 10;
            }
            pickups[i].alive = 0;
        }
    }
}

void update_rockets(void)
{
    int i;
    int j;
    int dx;
    int dy;
    int dist;
    if (game_over)
    {
        return;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (!rockets[i].active)
        {
            continue;
        }
        rockets[i].x += rockets[i].vx;
        rockets[i].y += rockets[i].vy;
        rockets[i].life--;
        if (rockets[i].life <= 0 || rockets[i].x < -10 || rockets[i].x > 340 || rockets[i].y < -10 || rockets[i].y > 260)
        {
            rockets[i].active = 0;
            continue;
        }
        for (j = 0; j < ENEMY_MAX; j++)
        {
            Enemy *e = &enemies[j];
            if (!e->alive)
            {
                continue;
            }
            dx = rockets[i].x - e->x;
            dy = rockets[i].y - e->y;
            dist = dx * dx + dy * dy;
            if (dist < 80)
            {
                e->alive = 0;
                rockets[i].active = 0;
                rockets_hit++;
                if (e->kind == ENEMY_MRSIREN)
                {
                    kills_mr++;
                    total_kills++;
                }
                else if (e->kind == ENEMY_MEGABIRD)
                {
                    kills_bird++;
                    total_kills++;
                }
                else if (e->kind == ENEMY_SPRINGU)
                {
                    kills_spring++;
                    total_kills++;
                }
                break;
            }
        }
    }
}

void draw_terrain_hud(void)
{
    char s[32];
    gfx_text(8, 236, "Infinite City", WHITE);
    number_text(player.x / 10, s);
    gfx_text(200, 236, "M:", WHITE);
    gfx_text(220, 236, s, YELLOW);
}

void end_game_screen(void)
{
    char s[32];
    gfx_rect(30, 80, 260, 70, DARK_GRAY);
    gfx_line(30, 80, 290, 80, WHITE);
    gfx_line(30, 150, 290, 150, WHITE);
    gfx_line(30, 80, 30, 150, WHITE);
    gfx_line(290, 80, 290, 150, WHITE);
    gfx_text(110, 95, "CITY LOST", WHITE);
    gfx_text(78, 112, "PRESS START", WHITE);
    number_text(kills_mr + kills_bird + kills_spring, s);
    gfx_text(104, 128, "KILLS:", WHITE);
    gfx_text(170, 128, s, YELLOW);
}

int main(void)
{
    unsigned int keys;
    unsigned int pressed;
    int i;
    init_city();
    reset_game();
    while (1)
    {
        keys = gfx_keys();
        pressed = keys & (~prev_keys);
        if (!game_over)
        {
            if (keys & KEY_LEFT)
            {
                player.x -= 2;
            }
            if (keys & KEY_RIGHT)
            {
                player.x += 2;
            }
            if ((keys & KEY_FIRE) && !prev_keys)
            {
                fire_rocket();
            }
            update_player(keys);
            if (keys & KEY_FIRE && player.fire_cd == 0 && player.ammo > 0)
            {
                fire_rocket();
            }
            if (keys & KEY_START)
            {
                if (restart_wait <= 0)
                {
                    reset_game();
                }
            }
            if (frame_count % 80 == 0)
            {
                if (camera_x + 320 < 5000)
                {
                    spawn_enemy();
                }
            }
            update_enemies();
            update_pickups();
            update_rockets();
            if (player.hp <= 0)
            {
                game_over = 1;
            }
        }
        else
        {
            if (keys & KEY_START)
            {
                reset_game();
            }
        }
        draw_city();
        for (i = 0; i < PICKUP_MAX; i++)
        {
            if (pickups[i].alive)
            {
                if (pickups[i].x < camera_x - 20 || pickups[i].x > camera_x + 340)
                {
                    pickups[i].alive = 0;
                }
            }
        }
        draw_pickups();
        for (i = 0; i < ENEMY_MAX; i++)
        {
            draw_enemy(&enemies[i]);
        }
        for (i = 0; i < ROCKET_MAX; i++)
        {
            draw_rocket(&rockets[i]);
        }
        draw_bombs();
        draw_player(player.x - camera_x, player.y);
        draw_hud();
        draw_terrain_hud();
        if (game_over)
        {
            end_game_screen();
        }
        gfx_present();
        prev_keys = keys;
        frame_count++;
        if (player.fire_cd > 0)
        {
            player.fire_cd--;
        }
        if (player.hp <= 0)
        {
            player.hp = 0;
        }
    }
    return 0;
}