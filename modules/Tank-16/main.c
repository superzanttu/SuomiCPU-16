#include "suomi_gfx.h"
#include "terrain_tiles.h"
#include "tank_sprite_rotations.h"

#define MAP_COLUMNS 40
#define MAP_ROWS 30
#define MAP_CELLS (MAP_COLUMNS * MAP_ROWS)
#define MAX_SHOTS 12
#define ROUND_FRAMES 9000
#define ROUND_DELAY 90
#define DIRECTION_SCALE 1000
#define TANK_ROTATION_COUNT 32
#define TANK_DIRECTION_MASK (TANK_ROTATION_COUNT - 1)
#define TANK_SPRITE_WIDTH 32
#define TANK_SPRITE_HEIGHT 32
#define TANK_SPRITE_PIXEL_COUNT (TANK_SPRITE_WIDTH * TANK_SPRITE_HEIGHT)
#define TANK_SPRITE_PLAYER_2_BASE 0
#define TANK_SPRITE_PLAYER_1_BASE TANK_ROTATION_COUNT

typedef struct {
    int x;
    int y;
    int direction;
    int health;
    int cooldown;
    int collision_cooldown;
    int movement_remainder_x;
    int movement_remainder_y;
} Tank;

typedef struct {
    int x;
    int y;
    int owner;
    int direction;
    int life;
    int movement_remainder_x;
    int movement_remainder_y;
} Shot;

typedef struct {
    int x;
    int y;
    int width;
    int height;
} Obstacle;

Tank tanks[2];
Shot shots[MAX_SHOTS];
unsigned char map_data[MAP_CELLS];
unsigned char sprite_work[TANK_SPRITE_PIXEL_COUNT];
unsigned char mine_active[3];
Obstacle obstacles[7];
int wins[2];
int round_frames;
int round_delay;
int round_over;
int round_winner;
int previous_keys;
int tank_sprite_slots[2];
int directions_x[TANK_ROTATION_COUNT] = {
    0, 195, 383, 556, 707, 831, 924, 981, 1000, 981, 924, 831, 707, 556, 383, 195,
    0, -195, -383, -556, -707, -831, -924, -981, -1000, -981, -924, -831, -707, -556, -383, -195};
int directions_y[TANK_ROTATION_COUNT] = {
    -1000, -981, -924, -831, -707, -556, -383, -195, 0, 195, 383, 556, 707, 831, 924, 981,
    1000, 981, 924, 831, 707, 556, 383, 195, 0, -195, -383, -556, -707, -831, -924, -981};

void number_text(unsigned int value, char *out)
{
    char digits[6];
    int count;
    int i;

    count = 0;
    if (value == 0) {
        digits[count++] = '0';
    }
    while (value > 0 && count < 5) {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    }
    for (i = 0; i < count; i++) {
        out[i] = digits[count - i - 1];
    }
    out[count] = 0;
}

int absolute_value(int value)
{
    return value < 0 ? -value : value;
}

int direction_offset(int component, int distance)
{
    int offset;

    offset = component * distance;
    if (offset >= 0) {
        offset += DIRECTION_SCALE / 2;
    } else {
        offset -= DIRECTION_SCALE / 2;
    }
    return offset / DIRECTION_SCALE;
}

int overlaps(int x, int y, int radius, Obstacle *obstacle)
{
    return x + radius > obstacle->x &&
           x - radius < obstacle->x + obstacle->width &&
           y + radius > obstacle->y &&
           y - radius < obstacle->y + obstacle->height;
}

int blocked(int x, int y, int tank_index)
{
    int i;
    int other;

    if (x < 16 || x > 624 || y < 46 || y > 464) {
        return 1;
    }
    for (i = 0; i < 7; i++) {
        if (overlaps(x, y, 16, &obstacles[i])) {
            return 1;
        }
    }
    other = 1 - tank_index;
    if (absolute_value(x - tanks[other].x) < 20 &&
        absolute_value(y - tanks[other].y) < 20) {
        return 1;
    }
    return 0;
}

void start_round(void)
{
    int i;

    tanks[0].x = 76;
    tanks[0].y = 260;
    tanks[0].direction = 8;
    tanks[1].x = 564;
    tanks[1].y = 260;
    tanks[1].direction = 24;
    for (i = 0; i < 2; i++) {
        tanks[i].health = 100;
        tanks[i].cooldown = 0;
        tanks[i].collision_cooldown = 0;
        tanks[i].movement_remainder_x = 0;
        tanks[i].movement_remainder_y = 0;
    }
    for (i = 0; i < MAX_SHOTS; i++) {
        shots[i].life = 0;
    }
    mine_active[0] = 1;
    mine_active[1] = 1;
    mine_active[2] = 1;
    round_frames = ROUND_FRAMES;
    round_delay = 0;
    round_over = 0;
    round_winner = -1;
}

void load_tank_sprites(void)
{
    int angle;
    int pixel;
    unsigned char color;

    for (angle = 0; angle < TANK_ROTATION_COUNT; angle++) {
        gfx_sprite_upload(TANK_SPRITE_PLAYER_2_BASE + angle,
                          TANK_SPRITE_WIDTH, TANK_SPRITE_HEIGHT,
                          tank_sprite_rotations[angle]);
        for (pixel = 0; pixel < TANK_SPRITE_PIXEL_COUNT; pixel++) {
            color = tank_sprite_rotations[angle][pixel];
            if (color == RED) {
                color = GREEN;
            } else if (color == DARK_RED) {
                color = DARK_GRAY;
            }
            sprite_work[pixel] = color;
        }
        gfx_sprite_upload(TANK_SPRITE_PLAYER_1_BASE + angle,
                          TANK_SPRITE_WIDTH, TANK_SPRITE_HEIGHT, sprite_work);
    }
}

void setup_map(void)
{
    int x;
    int y;
    int tile;

    obstacles[0].x = 192;
    obstacles[0].y = 112;
    obstacles[0].width = 56;
    obstacles[0].height = 24;
    obstacles[1].x = 392;
    obstacles[1].y = 112;
    obstacles[1].width = 56;
    obstacles[1].height = 24;
    obstacles[2].x = 192;
    obstacles[2].y = 376;
    obstacles[2].width = 56;
    obstacles[2].height = 24;
    obstacles[3].x = 392;
    obstacles[3].y = 376;
    obstacles[3].width = 56;
    obstacles[3].height = 24;
    obstacles[4].x = 296;
    obstacles[4].y = 168;
    obstacles[4].width = 48;
    obstacles[4].height = 24;
    obstacles[5].x = 296;
    obstacles[5].y = 320;
    obstacles[5].width = 48;
    obstacles[5].height = 24;
    obstacles[6].x = 304;
    obstacles[6].y = 232;
    obstacles[6].width = 32;
    obstacles[6].height = 16;

    for (y = 0; y < MAP_ROWS; y++) {
        for (x = 0; x < MAP_COLUMNS; x++) {
            tile = ((x * 7 + y * 11 + (x ^ y)) & 15) == 0
                       ? TILE_GRASS_DETAIL : TILE_GRASS;
            if (y >= 13 && y <= 16 && x >= 3 && x <= 36) {
                tile = TILE_HORIZONTAL_STRIP;
            }
            if ((x == 19 || x == 20) && y >= 4 && y <= 25) {
                tile = TILE_VERTICAL_STRIP;
            }
            map_data[y * MAP_COLUMNS + x] = (unsigned char)tile;
        }
    }
}

void fire_shot(int player)
{
    int i;
    int direction;
    Shot *shot;

    if (tanks[player].cooldown > 0) {
        return;
    }
    for (i = 0; i < MAX_SHOTS; i++) {
        if (shots[i].life == 0) {
            direction = tanks[player].direction & TANK_DIRECTION_MASK;
            shot = &shots[i];
            shot->x = tanks[player].x +
                      direction_offset(directions_x[direction], 12);
            shot->y = tanks[player].y +
                      direction_offset(directions_y[direction], 12);
            shot->owner = player;
            shot->direction = direction;
            shot->life = 120;
            shot->movement_remainder_x = 0;
            shot->movement_remainder_y = 0;
            tanks[player].cooldown = 18;
            return;
        }
    }
}

void update_tank(int player, unsigned int keys, unsigned int extra)
{
    int old_x;
    int old_y;
    int direction;
    int move;
    int next_x;
    int next_y;
    int movement_numerator_x;
    int movement_numerator_y;
    int can_move_x;
    int can_move_y;
    int forward_key;
    int backward_key;

    forward_key = player == 0 ? (int)(keys & 4) :
                  (int)(extra & KEYX2_P2_FORWARD);
    backward_key = player == 0 ? (int)(keys & 8) :
                   (int)(extra & KEYX2_P2_BACKWARD);
    if ((player == 0 && (keys & 1)) ||
        (player == 1 && (extra & KEYX2_P2_LEFT))) {
        tanks[player].direction =
            (tanks[player].direction - 1) & TANK_DIRECTION_MASK;
    }
    if ((player == 0 && (keys & 2)) ||
        (player == 1 && (extra & KEYX2_P2_RIGHT))) {
        tanks[player].direction =
            (tanks[player].direction + 1) & TANK_DIRECTION_MASK;
    }
    old_x = tanks[player].x;
    old_y = tanks[player].y;
    direction = tanks[player].direction;
    move = forward_key ? 2 : 0;
    if (backward_key) {
        move = -2;
    }
    movement_numerator_x = tanks[player].movement_remainder_x +
                           directions_x[direction] * move;
    movement_numerator_y = tanks[player].movement_remainder_y +
                           directions_y[direction] * move;
    next_x = old_x + movement_numerator_x / DIRECTION_SCALE;
    next_y = old_y + movement_numerator_y / DIRECTION_SCALE;
    if (absolute_value(next_x - tanks[1 - player].x) < 20 &&
        absolute_value(next_y - tanks[1 - player].y) < 20) {
        if (tanks[player].collision_cooldown == 0) {
            tanks[player].health -= 5;
            tanks[player].collision_cooldown = 30;
        }
        if (tanks[1 - player].collision_cooldown == 0) {
            tanks[1 - player].health -= 5;
            tanks[1 - player].collision_cooldown = 30;
        }
        if (tanks[player].health < 0) {
            tanks[player].health = 0;
        }
        if (tanks[1 - player].health < 0) {
            tanks[1 - player].health = 0;
        }
    } else if (!blocked(next_x, next_y, player)) {
        tanks[player].x = next_x;
        tanks[player].y = next_y;
        tanks[player].movement_remainder_x =
            movement_numerator_x - (next_x - old_x) * DIRECTION_SCALE;
        tanks[player].movement_remainder_y =
            movement_numerator_y - (next_y - old_y) * DIRECTION_SCALE;
    } else {
        tanks[player].movement_remainder_x = 0;
        tanks[player].movement_remainder_y = 0;
        can_move_x = !blocked(next_x, old_y, player);
        can_move_y = !blocked(old_x, next_y, player);
        if (can_move_x &&
            (!can_move_y ||
             absolute_value(next_x - old_x) >=
             absolute_value(next_y - old_y))) {
            tanks[player].x = next_x;
            tanks[player].movement_remainder_x =
                movement_numerator_x - (next_x - old_x) * DIRECTION_SCALE;
        } else if (can_move_y) {
            tanks[player].y = next_y;
            tanks[player].movement_remainder_y =
                movement_numerator_y - (next_y - old_y) * DIRECTION_SCALE;
        }
    }
    if ((extra & (player == 0 ? KEYX2_P1_FIRE : KEYX2_P2_FIRE)) != 0) {
        fire_shot(player);
    }
    if (tanks[player].cooldown > 0) {
        tanks[player].cooldown--;
    }
    if (tanks[player].collision_cooldown > 0) {
        tanks[player].collision_cooldown--;
    }
}

void finish_round(void)
{
    round_over = 1;
    round_delay = ROUND_DELAY;
    if (tanks[0].health > tanks[1].health) {
        round_winner = 0;
        wins[0]++;
    } else if (tanks[1].health > tanks[0].health) {
        round_winner = 1;
        wins[1]++;
    } else {
        round_winner = -1;
    }
}

void update_mines(void)
{
    int i;
    int mine_x[3];
    int mine_y[3];
    int player;

    mine_x[0] = 136;
    mine_y[0] = 184;
    mine_x[1] = 504;
    mine_y[1] = 184;
    mine_x[2] = 320;
    mine_y[2] = 280;
    for (i = 0; i < 3; i++) {
        if (!mine_active[i]) {
            continue;
        }
        for (player = 0; player < 2; player++) {
            if (absolute_value(tanks[player].x - mine_x[i]) < 12 &&
                absolute_value(tanks[player].y - mine_y[i]) < 12) {
                tanks[player].health -= 25;
                if (tanks[player].health < 0) {
                    tanks[player].health = 0;
                }
                mine_active[i] = 0;
            }
        }
    }
}

void update_shots(void)
{
    int i;
    int player;
    int blocked_shot;
    int movement_numerator_x;
    int movement_numerator_y;
    int next_x;
    int next_y;
    Shot *shot;

    for (i = 0; i < MAX_SHOTS; i++) {
        shot = &shots[i];
        if (shot->life == 0) {
            continue;
        }
        movement_numerator_x = shot->movement_remainder_x +
                               directions_x[shot->direction] * 5;
        movement_numerator_y = shot->movement_remainder_y +
                               directions_y[shot->direction] * 5;
        next_x = shot->x + movement_numerator_x / DIRECTION_SCALE;
        next_y = shot->y + movement_numerator_y / DIRECTION_SCALE;
        shot->movement_remainder_x =
            movement_numerator_x - (next_x - shot->x) * DIRECTION_SCALE;
        shot->movement_remainder_y =
            movement_numerator_y - (next_y - shot->y) * DIRECTION_SCALE;
        shot->x = next_x;
        shot->y = next_y;
        shot->life--;
        blocked_shot = shot->x < 8 || shot->x > 632 ||
                       shot->y < 32 || shot->y > 472;
        for (player = 0; player < 7 && !blocked_shot; player++) {
            if (overlaps(shot->x, shot->y, 2, &obstacles[player])) {
                blocked_shot = 1;
            }
        }
        if (blocked_shot) {
            shot->life = 0;
            continue;
        }
        player = 1 - shot->owner;
        if (absolute_value(shot->x - tanks[player].x) < 12 &&
            absolute_value(shot->y - tanks[player].y) < 12) {
            tanks[player].health -= 10;
            if (tanks[player].health < 0) {
                tanks[player].health = 0;
            }
            shot->life = 0;
        }
    }
}

void update_game(unsigned int keys, unsigned int extra)
{
    if ((keys & 32) && !(previous_keys & 32)) {
        wins[0] = 0;
        wins[1] = 0;
        start_round();
    }
    previous_keys = (int)keys;
    if (round_over) {
        if (round_delay > 0) {
            round_delay--;
        } else {
            start_round();
        }
        return;
    }
    update_tank(0, keys, extra);
    update_tank(1, keys, extra);
    update_mines();
    update_shots();
    round_frames--;
    if (tanks[0].health == 0 || tanks[1].health == 0 ||
        round_frames <= 0) {
        finish_round();
    }
}

void draw_tank(int player)
{
    int slot;

    slot = tanks[player].direction & TANK_DIRECTION_MASK;
    if (player == 0) {
        slot += TANK_SPRITE_PLAYER_1_BASE;
    } else {
        slot += TANK_SPRITE_PLAYER_2_BASE;
    }
    if (tank_sprite_slots[player] != slot) {
        if (tank_sprite_slots[player] >= 0) {
            gfx_sprite_move(tank_sprite_slots[player], 0, 0, 0);
        }
        tank_sprite_slots[player] = slot;
    }
    gfx_sprite_move(slot, tanks[player].x - 16,
                    tanks[player].y - 16, SPRITE_VISIBLE);
}

void draw_hud(void)
{
    char health[6];
    char score[14];
    char timer[16];
    unsigned int seconds;
    int i;
    int mine_x[3];
    int mine_y[3];

    gfx_layer_select(LAYER_HUD);
    gfx_clear(15);
    for (i = 0; i < 7; i++) {
        gfx_rect(obstacles[i].x, obstacles[i].y,
                 obstacles[i].width, obstacles[i].height, DARK_GRAY);
        gfx_rect(obstacles[i].x + 2, obstacles[i].y + 2,
                 obstacles[i].width - 4, obstacles[i].height - 4, GRAY);
    }
    mine_x[0] = 136;
    mine_y[0] = 184;
    mine_x[1] = 504;
    mine_y[1] = 184;
    mine_x[2] = 320;
    mine_y[2] = 280;
    for (i = 0; i < 3; i++) {
        if (mine_active[i]) {
            gfx_rect(mine_x[i] - 5, mine_y[i] - 5, 10, 10, ORANGE);
            gfx_rect(mine_x[i] - 2, mine_y[i] - 2, 4, 4, DARK_RED);
        }
    }
    for (i = 0; i < MAX_SHOTS; i++) {
        if (shots[i].life > 0) {
            gfx_line(
                shots[i].x -
                    direction_offset(directions_x[shots[i].direction], 4),
                shots[i].y -
                    direction_offset(directions_y[shots[i].direction], 4),
                shots[i].x +
                    direction_offset(directions_x[shots[i].direction], 4),
                shots[i].y +
                    direction_offset(directions_y[shots[i].direction], 4),
                YELLOW);
        }
    }
    gfx_rect(0, 0, 640, 30, DARK_GRAY);
    gfx_text(8, 5, "P1", WHITE);
    number_text((unsigned int)tanks[0].health, health);
    gfx_text(28, 5, health, WHITE);
    gfx_rect(68, 7, 100, 10, DARK_RED);
    gfx_rect(68, 7, tanks[0].health, 10, GREEN);
    gfx_text(472, 5, "P2", WHITE);
    number_text((unsigned int)tanks[1].health, health);
    gfx_text(492, 5, health, WHITE);
    gfx_rect(528, 7, 100, 10, DARK_RED);
    gfx_rect(528, 7, tanks[1].health, 10, RED);
    seconds = (unsigned int)(round_frames > 0 ?
                             (round_frames + 29) / 30 : 0);
    timer[0] = 'T';
    timer[1] = 'I';
    timer[2] = 'M';
    timer[3] = 'E';
    timer[4] = ' ';
    timer[5] = (char)('0' + seconds / 60);
    timer[6] = ':';
    timer[7] = (char)('0' + (seconds % 60) / 10);
    timer[8] = (char)('0' + seconds % 10);
    timer[9] = 0;
    gfx_text(274, 5, timer, YELLOW);
    number_text((unsigned int)wins[0], score);
    i = 0;
    while (score[i] != 0 && i < 5) {
        i++;
    }
    score[i++] = '-';
    number_text((unsigned int)wins[1], score + i);
    gfx_text(338, 5, score, WHITE);
}

void draw_game(void)
{
    gfx_layer_select(LAYER_SPRITES);
    draw_tank(0);
    draw_tank(1);
    draw_hud();
    if (round_over) {
        if (round_winner < 0) {
            gfx_text(285, 220, "DRAW", YELLOW);
        } else if (round_winner == 0) {
            gfx_text(270, 220, "PLAYER 1 WINS", GREEN);
        } else {
            gfx_text(270, 220, "PLAYER 2 WINS", RED);
        }
    }
}

int main(void)
{
    int i;
    unsigned int keys;
    unsigned int extra;

    gfx_mode(GFX_MODE_640_480);
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_layer_reset((1 << LAYER_TILES) | (1 << LAYER_SPRITES) |
                    (1 << LAYER_HUD));
    gfx_layer_transparency(LAYER_HUD, 15);
    load_tank_sprites();
    for (i = 0; i < TILE_COUNT; i++) {
        gfx_tile_upload(i, terrain_tiles[i]);
    }
    setup_map();
    gfx_layer_select(LAYER_TILES);
    gfx_tilemap(map_data, MAP_COLUMNS, MAP_ROWS, 0, 0);
    wins[0] = 0;
    wins[1] = 0;
    previous_keys = 0;
    tank_sprite_slots[0] = -1;
    tank_sprite_slots[1] = -1;
    start_round();
    while (1) {
        keys = gfx_keys();
        extra = gfx_keys_ext2();
        update_game(keys, extra);
        draw_game();
        gfx_present();
    }
    return 0;
}
