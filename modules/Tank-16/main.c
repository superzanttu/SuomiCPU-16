#include "suomi_gfx.h"
#include "map_assets.h"
#include "tank_sprite_rotations.h"

#define MAP_TILE_PIXEL_SIZE 64
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
#define MAX_TRACK_MARKS 96
#define TRACK_SPACING 4
#define TRACK_COLOR 12
#define EXPLOSION_FRAMES 18
#define SHOT_COOLDOWN 18
#define MUZZLE_FLASH_FRAMES 3
#define HIT_FLASH_FRAMES 8
#define MAX_IMPACTS 12
#define IMPACT_FRAMES 10
#define MAX_PARTICLES 48
#define PARTICLE_FRAMES 24
#define FIRE_EMISSION_INTERVAL 3

typedef struct
{
    int x;
    int y;
    int direction;
    int health;
    int cooldown;
    int collision_cooldown;
    int movement_remainder_x;
    int movement_remainder_y;
} Tank;

typedef struct
{
    int x;
    int y;
    int owner;
    int direction;
    int life;
    int movement_remainder_x;
    int movement_remainder_y;
} Shot;

typedef struct
{
    int x;
    int y;
    int velocity_x;
    int velocity_y;
    int life;
    int smoke;
} Particle;

Tank tanks[2];
Shot shots[MAX_SHOTS];
Particle particles[MAX_PARTICLES];
int next_particle;
int particle_sequence;
int fire_tick;
unsigned char sprite_work[TANK_SPRITE_PIXEL_COUNT];
unsigned char object_active[MAP_OBJECT_CAPACITY];
unsigned char object_contacts[MAP_OBJECT_CAPACITY];
unsigned char object_explosion[MAP_OBJECT_CAPACITY];
int track_x[MAX_TRACK_MARKS];
int track_y[MAX_TRACK_MARKS];
int track_direction[MAX_TRACK_MARKS];
int track_count;
int next_track;
int last_track_x[2];
int last_track_y[2];
int wins[2];
int round_frames;
int round_delay;
int round_over;
int round_winner;
int previous_keys;
int tank_sprite_slots[2];
int hit_flash[2];
int muzzle_flash[2];
int muzzle_x[2];
int muzzle_y[2];
int impact_x[MAX_IMPACTS];
int impact_y[MAX_IMPACTS];
int impact_life[MAX_IMPACTS];
int impact_size[MAX_IMPACTS];
int next_impact;
int collision_assets_ready;
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
    if (value == 0)
    {
        digits[count++] = '0';
    }
    while (value > 0 && count < 5)
    {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    }
    for (i = 0; i < count; i++)
    {
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
    if (offset >= 0)
    {
        offset += DIRECTION_SCALE / 2;
    }
    else
    {
        offset -= DIRECTION_SCALE / 2;
    }
    return offset / DIRECTION_SCALE;
}

void emit_particle(int x, int y, int velocity_x, int velocity_y, int smoke)
{
    Particle *particle;

    particle = &particles[next_particle];
    particle->x = x;
    particle->y = y;
    particle->velocity_x = velocity_x;
    particle->velocity_y = velocity_y;
    particle->life = PARTICLE_FRAMES;
    particle->smoke = smoke;
    next_particle = (next_particle + 1) % MAX_PARTICLES;
}

void emit_explosion(int x, int y, int speed)
{
    int direction;

    for (direction = 0; direction < TANK_ROTATION_COUNT; direction += 4)
    {
        emit_particle(x, y,
                      direction_offset(directions_x[direction], speed),
                      direction_offset(directions_y[direction], speed), 0);
    }
    emit_particle(x - 2, y, -1, -1, 1);
    emit_particle(x + 2, y, 1, -1, 1);
}

void explode_object(int index)
{
    object_explosion[index] = EXPLOSION_FRAMES;
    emit_explosion(map_object_x[index] * MAP_TILE_PIXEL_SIZE +
                       MAP_TILE_PIXEL_SIZE / 2,
                   map_object_y[index] * MAP_TILE_PIXEL_SIZE +
                       MAP_TILE_PIXEL_SIZE / 2,
                   2);
    gfx_sound(0, 90, 8, WAVE_NOISE, 35);
}

void add_impact(int x, int y, int size)
{
    impact_x[next_impact] = x;
    impact_y[next_impact] = y;
    impact_size[next_impact] = size;
    impact_life[next_impact] = IMPACT_FRAMES;
    next_impact = (next_impact + 1) % MAX_IMPACTS;
    emit_explosion(x, y, size >= 20 ? 3 : 2);
}

void change_health(int player, int amount)
{
    int previous_health;

    previous_health = tanks[player].health;
    tanks[player].health += amount;
    if (tanks[player].health < 0)
    {
        tanks[player].health = 0;
    }
    if (tanks[player].health > 100)
    {
        tanks[player].health = 100;
    }
    if (tanks[player].health < previous_health)
    {
        hit_flash[player] = HIT_FLASH_FRAMES;
        if (tanks[player].health == 0)
        {
            add_impact(tanks[player].x, tanks[player].y, 28);
            gfx_sound(2, 65, 16, WAVE_NOISE, 45);
        }
        else
        {
            gfx_sound(2, 160, 4, WAVE_NOISE, 25);
        }
    }
    else if (tanks[player].health > previous_health)
    {
        gfx_sound(3, 660, 6, WAVE_TRIANGLE, 25);
    }
}

void prepare_collision_assets(void)
{
    int angle;

    if (!collision_assets_ready)
    {
        for (angle = 0; angle < TANK_ROTATION_COUNT; angle++)
        {
            gfx_sprite_upload(angle, TANK_SPRITE_WIDTH, TANK_SPRITE_HEIGHT,
                              tank_sprite_rotations[angle]);
        }
        gfx_tile_size(MAP_TILE_PIXEL_SIZE);
        upload_terrain_tiles();
        collision_assets_ready = 1;
    }
}

int tank_mask(int player)
{
    return tanks[player].direction & TANK_DIRECTION_MASK;
}

int tank_collision(int x, int y, int player)
{
    int other;

    other = 1 - player;
    if (absolute_value(x - tanks[other].x) >= 32 ||
        absolute_value(y - tanks[other].y) >= 32)
        return 0;
    prepare_collision_assets();
    return gfx_mask_overlap(tank_mask(player), x - 16, y - 16,
                            tank_mask(other), tanks[other].x - 16,
                            tanks[other].y - 16);
}

int obstacle_mask_collision(int mask, int x, int y, int size)
{
    int left;
    int right;
    int top;
    int bottom;
    int row;
    int column;
    int tile;

    left = x / MAP_TILE_PIXEL_SIZE;
    right = (x + size - 1) / MAP_TILE_PIXEL_SIZE;
    top = y / MAP_TILE_PIXEL_SIZE;
    bottom = (y + size - 1) / MAP_TILE_PIXEL_SIZE;
    if (left < 0)
        left = 0;
    if (top < 0)
        top = 0;
    if (right >= MAP_COLUMNS)
        right = MAP_COLUMNS - 1;
    if (bottom >= MAP_ROWS)
        bottom = MAP_ROWS - 1;
    for (row = top; row <= bottom; row++)
    {
        for (column = left; column <= right; column++)
        {
            tile = map_obstacles[row * MAP_COLUMNS + column];
            if (tile != 0 &&
                gfx_mask_overlap(mask, x, y, GFX_MASK_TILE_BASE + tile,
                                 column * MAP_TILE_PIXEL_SIZE,
                                 row * MAP_TILE_PIXEL_SIZE))
            {
                return 1;
            }
        }
    }
    return 0;
}

int blocked(int x, int y, int tank_index)
{
    int map_width;
    int map_height;

    map_width = MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;
    map_height = MAP_ROWS * MAP_TILE_PIXEL_SIZE;
    if (x < 16 || x > map_width - 16 ||
        y < 16 || y > map_height - 16)
    {
        return 1;
    }
    prepare_collision_assets();
    if (obstacle_mask_collision(tank_mask(tank_index), x - 16, y - 16, 32))
    {
        return 1;
    }
    if (tank_collision(x, y, tank_index))
    {
        return 1;
    }
    return 0;
}

int movement_blocked(int old_x, int old_y, int x, int y, int player)
{
    return blocked(old_x + (x - old_x) / 2,
                   old_y + (y - old_y) / 2, player) ||
           blocked(x, y, player);
}

void start_round(void)
{
    int i;
    int player;
    int row;
    int column;
    int x;
    int y;
    int target_x;
    int target_y;
    int best_x;
    int best_y;
    int best_score;
    int score;
    int found;

    prepare_collision_assets();
    tanks[0].direction = 8;
    tanks[1].direction = 24;
    tanks[0].x = -1000;
    tanks[0].y = -1000;
    tanks[1].x = -1000;
    tanks[1].y = -1000;
    for (player = 0; player < 2; player++)
    {
        target_x = player == 0
                       ? MAP_COLUMNS * MAP_TILE_PIXEL_SIZE / 4
                       : MAP_COLUMNS * MAP_TILE_PIXEL_SIZE * 3 / 4;
        target_y = MAP_ROWS * MAP_TILE_PIXEL_SIZE / 2;
        best_x = target_x;
        best_y = target_y;
        best_score = 32767;
        found = 0;
        for (row = 0; row < MAP_ROWS; row++)
        {
            y = row * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2;
            for (column = 0; column < MAP_COLUMNS; column++)
            {
                if ((player == 0 && column >= MAP_COLUMNS / 2) ||
                    (player == 1 && column < MAP_COLUMNS / 2))
                {
                    continue;
                }
                x = column * MAP_TILE_PIXEL_SIZE +
                    MAP_TILE_PIXEL_SIZE / 2;
                if (blocked(x, y, player))
                {
                    continue;
                }
                score = absolute_value(x - target_x) +
                        absolute_value(y - target_y);
                if (!found || score < best_score)
                {
                    best_x = x;
                    best_y = y;
                    best_score = score;
                    found = 1;
                }
            }
        }
        tanks[player].x = best_x;
        tanks[player].y = best_y;
    }
    tanks[0].direction = 8;
    tanks[1].direction = 24;
    for (i = 0; i < 2; i++)
    {
        tanks[i].health = 100;
        tanks[i].cooldown = 0;
        tanks[i].collision_cooldown = 0;
        tanks[i].movement_remainder_x = 0;
        tanks[i].movement_remainder_y = 0;
        hit_flash[i] = 0;
        muzzle_flash[i] = 0;
    }
    for (i = 0; i < MAX_SHOTS; i++)
    {
        shots[i].life = 0;
    }
    for (i = 0; i < MAX_IMPACTS; i++)
    {
        impact_life[i] = 0;
    }
    next_impact = 0;
    for (i = 0; i < MAX_PARTICLES; i++)
    {
        particles[i].life = 0;
    }
    next_particle = 0;
    particle_sequence = 0;
    fire_tick = 0;
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        object_active[i] = 1;
        object_contacts[i] = 0;
        object_explosion[i] = 0;
    }
    track_count = 0;
    next_track = 0;
    for (i = 0; i < 2; i++)
    {
        last_track_x[i] = tanks[i].x;
        last_track_y[i] = tanks[i].y;
    }
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

    for (angle = 0; angle < TANK_ROTATION_COUNT; angle++)
    {
        gfx_sprite_upload(TANK_SPRITE_PLAYER_2_BASE + angle,
                          TANK_SPRITE_WIDTH, TANK_SPRITE_HEIGHT,
                          tank_sprite_rotations[angle]);
        for (pixel = 0; pixel < TANK_SPRITE_PIXEL_COUNT; pixel++)
        {
            color = tank_sprite_rotations[angle][pixel];
            if (color == RED)
            {
                color = GREEN;
            }
            else if (color == DARK_RED)
            {
                color = DARK_GRAY;
            }
            sprite_work[pixel] = color;
        }
        gfx_sprite_upload(TANK_SPRITE_PLAYER_1_BASE + angle,
                          TANK_SPRITE_WIDTH, TANK_SPRITE_HEIGHT, sprite_work);
    }
}

void fire_shot(int player)
{
    int i;
    int direction;
    Shot *shot;

    if (tanks[player].cooldown > 0 || tanks[player].health == 0)
    {
        return;
    }
    for (i = 0; i < MAX_SHOTS; i++)
    {
        if (shots[i].life == 0)
        {
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
            tanks[player].cooldown = SHOT_COOLDOWN;
            muzzle_x[player] = tanks[player].x +
                               direction_offset(directions_x[direction], 18);
            muzzle_y[player] = tanks[player].y +
                               direction_offset(directions_y[direction], 18);
            muzzle_flash[player] = MUZZLE_FLASH_FRAMES;
            gfx_sound(player, 230, 3, WAVE_NOISE, 22);
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
    int old_direction;

    old_direction = tanks[player].direction;
    forward_key = player == 0 ? (int)(keys & 4) : (int)(extra & KEYX2_P2_FORWARD);
    backward_key = player == 0 ? (int)(keys & 8) : (int)(extra & KEYX2_P2_BACKWARD);
    if ((player == 0 && (keys & 1)) ||
        (player == 1 && (extra & KEYX2_P2_LEFT)))
    {
        tanks[player].direction =
            (tanks[player].direction - 1) & TANK_DIRECTION_MASK;
    }
    if ((player == 0 && (keys & 2)) ||
        (player == 1 && (extra & KEYX2_P2_RIGHT)))
    {
        tanks[player].direction =
            (tanks[player].direction + 1) & TANK_DIRECTION_MASK;
    }
    if (blocked(tanks[player].x, tanks[player].y, player))
        tanks[player].direction = old_direction;
    old_x = tanks[player].x;
    old_y = tanks[player].y;
    direction = tanks[player].direction;
    move = forward_key ? 2 : 0;
    if (backward_key)
    {
        move = -2;
    }
    movement_numerator_x = tanks[player].movement_remainder_x +
                           directions_x[direction] * move;
    movement_numerator_y = tanks[player].movement_remainder_y +
                           directions_y[direction] * move;
    next_x = old_x + movement_numerator_x / DIRECTION_SCALE;
    next_y = old_y + movement_numerator_y / DIRECTION_SCALE;
    if (tank_collision(next_x, next_y, player))
    {
        if (tanks[player].collision_cooldown == 0)
        {
            change_health(player, -5);
            tanks[player].collision_cooldown = 30;
        }
        if (tanks[1 - player].collision_cooldown == 0)
        {
            change_health(1 - player, -5);
            tanks[1 - player].collision_cooldown = 30;
        }
    }
    else if (!movement_blocked(old_x, old_y, next_x, next_y, player))
    {
        tanks[player].x = next_x;
        tanks[player].y = next_y;
        tanks[player].movement_remainder_x =
            movement_numerator_x - (next_x - old_x) * DIRECTION_SCALE;
        tanks[player].movement_remainder_y =
            movement_numerator_y - (next_y - old_y) * DIRECTION_SCALE;
    }
    else
    {
        tanks[player].movement_remainder_x = 0;
        tanks[player].movement_remainder_y = 0;
        can_move_x = !movement_blocked(old_x, old_y, next_x, old_y, player);
        can_move_y = !movement_blocked(old_x, old_y, old_x, next_y, player);
        if (can_move_x &&
            (!can_move_y ||
             absolute_value(next_x - old_x) >=
                 absolute_value(next_y - old_y)))
        {
            tanks[player].x = next_x;
            tanks[player].movement_remainder_x =
                movement_numerator_x - (next_x - old_x) * DIRECTION_SCALE;
        }
        else if (can_move_y)
        {
            tanks[player].y = next_y;
            tanks[player].movement_remainder_y =
                movement_numerator_y - (next_y - old_y) * DIRECTION_SCALE;
        }
    }
    if ((tanks[player].x != old_x || tanks[player].y != old_y) &&
        absolute_value(tanks[player].x - last_track_x[player]) +
                absolute_value(tanks[player].y - last_track_y[player]) >=
            TRACK_SPACING)
    {
        track_x[next_track] = old_x;
        track_y[next_track] = old_y;
        track_direction[next_track] = direction;
        next_track = (next_track + 1) % MAX_TRACK_MARKS;
        if (track_count < MAX_TRACK_MARKS)
        {
            track_count++;
        }
        last_track_x[player] = tanks[player].x;
        last_track_y[player] = tanks[player].y;
    }
    if ((extra & (player == 0 ? KEYX2_P1_FIRE : KEYX2_P2_FIRE)) != 0)
    {
        fire_shot(player);
    }
    if (tanks[player].cooldown > 0)
    {
        tanks[player].cooldown--;
    }
    if (tanks[player].collision_cooldown > 0)
    {
        tanks[player].collision_cooldown--;
    }
}

void finish_round(void)
{
    round_over = 1;
    round_delay = ROUND_DELAY;
    if (tanks[0].health > tanks[1].health)
    {
        round_winner = 0;
        wins[0]++;
    }
    else if (tanks[1].health > tanks[0].health)
    {
        round_winner = 1;
        wins[1]++;
    }
    else
    {
        round_winner = -1;
    }
}

void update_objects(void)
{
    int i;
    int player;
    int contacts;
    int mask;

    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (!object_active[i])
        {
            continue;
        }
        contacts = 0;
        for (player = 0; player < 2; player++)
        {
            mask = 1 << player;
            if (tanks[player].health > 0 &&
                gfx_mask_overlap(tank_mask(player),
                                 tanks[player].x - 16, tanks[player].y - 16,
                                 GFX_MASK_TILE_BASE + map_object_tile[i],
                                 map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                                 map_object_y[i] * MAP_TILE_PIXEL_SIZE))
            {
                contacts |= mask;
                if (!(object_contacts[i] & mask))
                {
                    change_health(player, map_object_hp[i]);
                    if (map_object_removal[i] != OBJECT_FIXED)
                    {
                        object_active[i] = 0;
                        if (map_object_removal[i] == OBJECT_EXPLOSION)
                        {
                            explode_object(i);
                        }
                        break;
                    }
                }
            }
        }
        object_contacts[i] = (unsigned char)contacts;
    }
}

void update_object_effects(void)
{
    int i;

    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_explosion[i] > 0)
        {
            object_explosion[i]--;
        }
    }
}

void update_combat_effects(void)
{
    int i;

    for (i = 0; i < 2; i++)
    {
        if (hit_flash[i] > 0)
            hit_flash[i]--;
        if (muzzle_flash[i] > 0)
            muzzle_flash[i]--;
    }
    for (i = 0; i < MAX_IMPACTS; i++)
    {
        if (impact_life[i] > 0)
            impact_life[i]--;
    }
}

void update_particles(void)
{
    int i;
    int drift;
    Particle *particle;

    for (i = 0; i < MAX_PARTICLES; i++)
    {
        particle = &particles[i];
        if (particle->life <= 0)
            continue;
        particle->life--;
        if (particle->life == 0)
            continue;
        if (particle->smoke)
        {
            particle->y--;
            if (particle->life % 4 == 0)
                particle->x += particle->velocity_x;
        }
        else
        {
            particle->x += particle->velocity_x;
            particle->y += particle->velocity_y;
            if (particle->life % 4 == 0)
            {
                particle->velocity_x /= 2;
                particle->velocity_y /= 2;
            }
            if (particle->life < PARTICLE_FRAMES / 2)
                particle->y--;
        }
        if (particle->x < 0 || particle->y < 0 ||
            particle->x >= MAP_COLUMNS * MAP_TILE_PIXEL_SIZE ||
            particle->y >= MAP_ROWS * MAP_TILE_PIXEL_SIZE)
            particle->life = 0;
    }
    fire_tick = (fire_tick + 1) % FIRE_EMISSION_INTERVAL;
    if (fire_tick == 0)
    {
        for (i = 0; i < 2; i++)
        {
            if (tanks[i].health == 0)
            {
                drift = particle_sequence % 3 - 1;
                particle_sequence++;
                emit_particle(tanks[i].x + drift * 3, tanks[i].y,
                              drift, -1, 0);
                emit_particle(tanks[i].x - drift * 2, tanks[i].y - 4,
                              -drift, -1, 1);
            }
        }
    }
}

int shot_collision(int index)
{
    int i;
    int player;
    Shot *shot;

    shot = &shots[index];
    if (shot->x < 3 || shot->y < 3 ||
        shot->x >= MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 3 ||
        shot->y >= MAP_ROWS * MAP_TILE_PIXEL_SIZE - 3 ||
        obstacle_mask_collision(GFX_MASK_AMMO, shot->x - 3, shot->y - 3, 7))
        return 1;
    player = 1 - shot->owner;
    if (tanks[player].health > 0 &&
        gfx_mask_overlap(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                         tank_mask(player), tanks[player].x - 16,
                         tanks[player].y - 16))
    {
        change_health(player, -10);
        return 1;
    }
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_active[i] &&
            gfx_mask_overlap(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                             GFX_MASK_TILE_BASE + map_object_tile[i],
                             map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                             map_object_y[i] * MAP_TILE_PIXEL_SIZE))
        {
            if (map_object_removal[i] != OBJECT_FIXED)
            {
                object_active[i] = 0;
                if (map_object_removal[i] == OBJECT_EXPLOSION)
                {
                    explode_object(i);
                }
            }
            return 1;
        }
    }
    for (i = 0; i < MAX_SHOTS; i++)
    {
        if (i != index && shots[i].life > 0 &&
            absolute_value(shot->x - shots[i].x) <= 6 &&
            absolute_value(shot->y - shots[i].y) <= 6 &&
            gfx_mask_overlap(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                             GFX_MASK_AMMO, shots[i].x - 3, shots[i].y - 3))
        {
            shots[i].life = 0;
            return 1;
        }
    }
    return 0;
}

void update_shots(void)
{
    int i;
    int step;
    int old_x;
    int old_y;
    int movement_numerator_x;
    int movement_numerator_y;
    int next_x;
    int next_y;
    Shot *shot;

    prepare_collision_assets();
    for (i = 0; i < MAX_SHOTS; i++)
    {
        shot = &shots[i];
        if (shot->life == 0)
        {
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
        old_x = shot->x;
        old_y = shot->y;
        for (step = 0; step <= 5; step++)
        {
            shot->x = old_x + (next_x - old_x) * step / 5;
            shot->y = old_y + (next_y - old_y) * step / 5;
            if (shot_collision(i))
            {
                add_impact(shot->x, shot->y, 12);
                shot->life = 0;
                break;
            }
        }
        if (shot->life > 0)
            shot->life--;
    }
}

void update_game(unsigned int keys, unsigned int extra)
{
    if ((keys & 32) && !(previous_keys & 32))
    {
        wins[0] = 0;
        wins[1] = 0;
        start_round();
    }
    previous_keys = (int)keys;
    update_object_effects();
    update_combat_effects();
    update_particles();
    if (round_over)
    {
        if (round_delay > 0)
        {
            round_delay--;
        }
        else
        {
            start_round();
        }
        return;
    }
    update_tank(0, keys, extra);
    update_tank(1, keys, extra);
    update_objects();
    update_shots();
    round_frames--;
    if (tanks[0].health == 0 || tanks[1].health == 0 ||
        round_frames <= 0)
    {
        finish_round();
    }
}

void draw_tank(int player)
{
    int slot;

    slot = tanks[player].direction & TANK_DIRECTION_MASK;
    if (player == 0)
    {
        slot += TANK_SPRITE_PLAYER_1_BASE;
    }
    else
    {
        slot += TANK_SPRITE_PLAYER_2_BASE;
    }
    if (tank_sprite_slots[player] != slot)
    {
        if (tank_sprite_slots[player] >= 0)
        {
            gfx_sprite_move(tank_sprite_slots[player], 0, 0, 0);
        }
        tank_sprite_slots[player] = slot;
    }
    gfx_sprite_move(slot, tanks[player].x - 16,
                    tanks[player].y - 16,
                    tanks[player].health > 0 ? SPRITE_VISIBLE : 0);
}

int camera_origin(int position, int viewport_size, int map_size)
{
    int camera;

    if (map_size <= viewport_size)
    {
        return (map_size - viewport_size) / 2;
    }
    camera = position - viewport_size / 2;
    if (camera < 0)
    {
        return 0;
    }
    if (camera > map_size - viewport_size)
    {
        return map_size - viewport_size;
    }
    return camera;
}

void draw_world_rect_view(
    int x, int y, int rect_width, int rect_height, int color,
    int camera_x, int camera_y, int viewport_x)
{
    int map_width;
    int map_height;
    int left;
    int right;
    int top;
    int bottom;

    map_width = MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;
    map_height = MAP_ROWS * MAP_TILE_PIXEL_SIZE;
    left = x > 0 ? x : 0;
    right = x + rect_width < map_width ? x + rect_width : map_width;
    top = y > 0 ? y : 0;
    bottom = y + rect_height < map_height ? y + rect_height : map_height;
    left = left - camera_x + viewport_x;
    right = right - camera_x + viewport_x;
    top -= camera_y;
    bottom -= camera_y;
    if (left < viewport_x)
    {
        left = viewport_x;
    }
    if (right > viewport_x + 320)
    {
        right = viewport_x + 320;
    }
    if (top < 0)
    {
        top = 0;
    }
    if (bottom > 480)
    {
        bottom = 480;
    }
    if (left < right && top < bottom)
    {
        gfx_rect(left, top, right - left, bottom - top, color);
    }
}

void draw_map_obstacles(int camera_x, int camera_y)
{
    int first_column;
    int last_column;
    int first_row;
    int last_row;
    int column;
    int row;
    int tile;

    first_column = camera_x / MAP_TILE_PIXEL_SIZE;
    first_row = camera_y / MAP_TILE_PIXEL_SIZE;
    last_column = (camera_x + 319) / MAP_TILE_PIXEL_SIZE;
    last_row = (camera_y + 479) / MAP_TILE_PIXEL_SIZE;
    if (first_column < 0)
        first_column = 0;
    if (first_row < 0)
        first_row = 0;
    if (last_column >= MAP_COLUMNS)
        last_column = MAP_COLUMNS - 1;
    if (last_row >= MAP_ROWS)
        last_row = MAP_ROWS - 1;
    for (row = first_row; row <= last_row; row++)
    {
        for (column = first_column; column <= last_column; column++)
        {
            tile = map_obstacles[row * MAP_COLUMNS + column];
            if (tile != 0)
            {
                gfx_world_tile(tile, column * MAP_TILE_PIXEL_SIZE,
                               row * MAP_TILE_PIXEL_SIZE);
            }
        }
    }
}

void draw_ground(void)
{
    int viewport;
    int camera_x;
    int camera_y;
    int i;
    int offset_x;
    int offset_y;

    gfx_layer_select(LAYER_3D);
    gfx_clear(0);
    for (viewport = 0; viewport < 2; viewport++)
    {
        camera_x = camera_origin(tanks[viewport].x, 320,
                                 MAP_COLUMNS * MAP_TILE_PIXEL_SIZE);
        camera_y = camera_origin(tanks[viewport].y, 480,
                                 MAP_ROWS * MAP_TILE_PIXEL_SIZE);
        for (i = 0; i < track_count; i++)
        {
            offset_x = direction_offset(directions_y[track_direction[i]], 10);
            offset_y = -direction_offset(directions_x[track_direction[i]], 10);
            draw_world_rect_view(
                track_x[i] + offset_x - 1, track_y[i] + offset_y - 1, 3, 3,
                TRACK_COLOR, camera_x, camera_y, viewport * 320);
            draw_world_rect_view(
                track_x[i] - offset_x - 1, track_y[i] - offset_y - 1, 3, 3,
                TRACK_COLOR, camera_x, camera_y, viewport * 320);
        }
    }
    for (viewport = 0; viewport < 2; viewport++)
    {
        camera_x = camera_origin(tanks[viewport].x, 320,
                                 MAP_COLUMNS * MAP_TILE_PIXEL_SIZE);
        camera_y = camera_origin(tanks[viewport].y, 480,
                                 MAP_ROWS * MAP_TILE_PIXEL_SIZE);
        draw_map_obstacles(camera_x, camera_y);
    }
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_active[i])
        {
            gfx_world_tile(map_object_tile[i],
                           map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                           map_object_y[i] * MAP_TILE_PIXEL_SIZE);
        }
    }
}

void draw_burst(int x, int y, int radius, int color,
                int camera_x, int camera_y, int viewport_x)
{
    int direction;
    int outer_x;
    int outer_y;

    for (direction = 0; direction < TANK_ROTATION_COUNT; direction += 4)
    {
        outer_x = x + direction_offset(directions_x[direction], radius);
        outer_y = y + direction_offset(directions_y[direction], radius);
        draw_world_rect_view(outer_x - 1, outer_y - 1, 3, 3,
                             color, camera_x, camera_y, viewport_x);
    }
}

void draw_combat_feedback(int camera_x, int camera_y, int viewport_x)
{
    int i;
    int radius;
    int color;

    for (i = 0; i < MAX_IMPACTS; i++)
    {
        if (impact_life[i] > 0)
        {
            radius = 2 + (IMPACT_FRAMES - impact_life[i]) * impact_size[i] /
                             IMPACT_FRAMES;
            color = impact_life[i] > IMPACT_FRAMES / 2 ? YELLOW : ORANGE;
            draw_burst(impact_x[i], impact_y[i], radius, color,
                       camera_x, camera_y, viewport_x);
        }
    }
    for (i = 0; i < 2; i++)
    {
        if (muzzle_flash[i] > 0)
        {
            draw_burst(muzzle_x[i], muzzle_y[i], 5, WHITE,
                       camera_x, camera_y, viewport_x);
        }
        if (hit_flash[i] > 0 && tanks[i].health > 0)
        {
            draw_burst(tanks[i].x, tanks[i].y, 20, RED,
                       camera_x, camera_y, viewport_x);
        }
    }
}

void draw_hud(void)
{
    char health[6];
    char score[14];
    char timer[16];
    unsigned int seconds;
    int i;
    int viewport;
    int camera_x;
    int camera_y;
    int map_width;
    int map_height;
    int explosion_x;
    int explosion_y;
    int radius;
    int particle_size;
    int particle_color;
    Particle *particle;

    gfx_layer_select(LAYER_HUD);
    gfx_clear(15);
    map_width = MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;
    map_height = MAP_ROWS * MAP_TILE_PIXEL_SIZE;
    for (viewport = 0; viewport < 2; viewport++)
    {
        camera_x = camera_origin(tanks[viewport].x, 320, map_width);
        camera_y = camera_origin(tanks[viewport].y, 480, map_height);
        for (i = 0; i < MAX_PARTICLES; i++)
        {
            particle = &particles[i];
            if (particle->life > 0)
            {
                if (particle->smoke)
                {
                    particle_size = 3 + (PARTICLE_FRAMES - particle->life) / 6;
                    particle_color = particle->life > PARTICLE_FRAMES / 2 ? DARK_GRAY : GRAY;
                }
                else
                {
                    particle_size = particle->life > PARTICLE_FRAMES / 2 ? 3 : 2;
                    particle_color = particle->life > 18 ? YELLOW : particle->life > 8 ? ORANGE
                                                                                       : DARK_RED;
                }
                draw_world_rect_view(
                    particle->x - particle_size / 2,
                    particle->y - particle_size / 2,
                    particle_size, particle_size, particle_color,
                    camera_x, camera_y, viewport * 320);
            }
        }
        draw_combat_feedback(camera_x, camera_y, viewport * 320);
        for (i = 0; i < MAP_OBJECT_COUNT; i++)
        {
            if (object_explosion[i] > 0)
            {
                explosion_x = map_object_x[i] * MAP_TILE_PIXEL_SIZE +
                              MAP_TILE_PIXEL_SIZE / 2;
                explosion_y = map_object_y[i] * MAP_TILE_PIXEL_SIZE +
                              MAP_TILE_PIXEL_SIZE / 2;
                radius = (EXPLOSION_FRAMES - object_explosion[i] + 4) *
                             MAP_TILE_PIXEL_SIZE / 32 +
                         2;
                draw_burst(explosion_x, explosion_y, radius,
                           object_explosion[i] > EXPLOSION_FRAMES / 2 ? YELLOW : ORANGE,
                           camera_x, camera_y, viewport * 320);
            }
        }
    }
    for (i = 0; i < MAX_SHOTS; i++)
    {
        if (shots[i].life > 0)
            gfx_world_ammo(shots[i].x, shots[i].y);
    }
    gfx_rect(319, 30, 2, 450, DARK_GRAY);
    gfx_rect(0, 0, 640, 30, DARK_GRAY);
    gfx_text(8, 5, "P1", WHITE);
    number_text((unsigned int)tanks[0].health, health);
    gfx_text(28, 5, health, WHITE);
    gfx_rect(68, 7, 100, 10, DARK_RED);
    gfx_rect(68, 7, tanks[0].health, 10,
             hit_flash[0] > 0 ? WHITE : tanks[0].health <= 25 ? ORANGE
                                                              : GREEN);
    gfx_rect(68, 21, 100, 3, BLACK);
    gfx_rect(68, 21, (SHOT_COOLDOWN - tanks[0].cooldown) * 100 / SHOT_COOLDOWN,
             3, tanks[0].cooldown == 0 ? YELLOW : GRAY);
    gfx_text(472, 5, "P2", WHITE);
    number_text((unsigned int)tanks[1].health, health);
    gfx_text(492, 5, health, WHITE);
    gfx_rect(528, 7, 100, 10, DARK_RED);
    gfx_rect(528, 7, tanks[1].health, 10,
             hit_flash[1] > 0 ? WHITE : tanks[1].health <= 25 ? ORANGE
                                                              : RED);
    gfx_rect(528, 21, 100, 3, BLACK);
    gfx_rect(528, 21, (SHOT_COOLDOWN - tanks[1].cooldown) * 100 / SHOT_COOLDOWN,
             3, tanks[1].cooldown == 0 ? YELLOW : GRAY);
    seconds = (unsigned int)(round_frames > 0 ? (round_frames + 29) / 30 : 0);
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
    while (score[i] != 0 && i < 5)
    {
        i++;
    }
    score[i++] = '-';
    number_text((unsigned int)wins[1], score + i);
    gfx_text(338, 5, score, WHITE);
}

void draw_game(void)
{
    int map_width;
    int map_height;
    int camera1_x;
    int camera1_y;
    int camera2_x;
    int camera2_y;
    int viewport;
    char countdown[6];

    map_width = MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;
    map_height = MAP_ROWS * MAP_TILE_PIXEL_SIZE;
    camera1_x = camera_origin(tanks[0].x, 320, map_width);
    camera1_y = camera_origin(tanks[0].y, 480, map_height);
    camera2_x = camera_origin(tanks[1].x, 320, map_width);
    camera2_y = camera_origin(tanks[1].y, 480, map_height);
    gfx_split_camera(camera1_x, camera1_y, camera2_x, camera2_y);
    draw_ground();
    gfx_layer_select(LAYER_SPRITES);
    draw_tank(0);
    draw_tank(1);
    draw_hud();
    if (round_over)
    {
        number_text((unsigned int)((round_delay + 29) / 30), countdown);
        for (viewport = 0; viewport < 2; viewport++)
        {
            gfx_rect(viewport * 320 + 32, 394, 256, 70, BLACK);
            gfx_rect(viewport * 320 + 34, 396, 252, 2,
                     viewport == 0 ? GREEN : RED);
            if (round_winner < 0)
            {
                gfx_text(viewport * 320 + 145, 412, "DRAW", YELLOW);
            }
            else if (round_winner == viewport)
            {
                gfx_text(viewport * 320 + 139, 412, "YOU WIN", GREEN);
            }
            else
            {
                gfx_text(viewport * 320 + 136, 412, "YOU LOSE", RED);
            }
            gfx_text(viewport * 320 + 107, 437, "NEXT ROUND IN", WHITE);
            gfx_text(viewport * 320 + 191, 437, countdown, YELLOW);
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
                    (1 << LAYER_3D) | (1 << LAYER_HUD));
    gfx_layer_order(LAYER_3D, 2);
    gfx_layer_order(LAYER_SPRITES, 3);
    gfx_layer_transparency(LAYER_HUD, 15);
    gfx_palette(TRACK_COLOR, 84, 71, 48);
    for (i = 1; i < MAP_PALETTE_COUNT; i++)
    {
        gfx_palette((unsigned char)(MAP_PALETTE_BASE + i - 1),
                    map_palette[i][0],
                    map_palette[i][1], map_palette[i][2]);
    }
    load_tank_sprites();
    gfx_tile_size(MAP_TILE_PIXEL_SIZE);
    upload_terrain_tiles();
    collision_assets_ready = 1;
    gfx_layer_select(LAYER_TILES);
    gfx_tilemap(map_data, MAP_COLUMNS, MAP_ROWS, 0, 0);
    wins[0] = 0;
    wins[1] = 0;
    previous_keys = 0;
    tank_sprite_slots[0] = -1;
    tank_sprite_slots[1] = -1;
    start_round();
    while (1)
    {
        keys = gfx_keys();
        extra = gfx_keys_ext2();
        update_game(keys, extra);
        draw_game();
        gfx_present();
    }
    return 0;
}
