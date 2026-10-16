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
#define MAX_TRACK_MARKS 288
#define TRACK_SPACING 4
#define TRACK_COLOR 12
#define EXPLOSION_FRAMES 18
#define SHOT_COOLDOWN 18
#define MUZZLE_FLASH_FRAMES 3
#define HIT_FLASH_FRAMES 8
#define MAX_IMPACTS 12
#define IMPACT_FRAMES 10
#define MAX_PARTICLES 80
#define PARTICLE_FRAMES 36
#define BURN_ROUNDS 10
#define MAX_BURN_SITES ((ROUND_FRAMES / SHOT_COOLDOWN * 2 + 2) * (BURN_ROUNDS + 1))
#define SPEED_SCALE 16
#define MAX_FORWARD_SPEED 32
#define MAX_REVERSE_SPEED 16
#define MOVING_TURN_FRAMES 4
#define STATIONARY_TURN_FRAMES 2
#define HANDBRAKE_DECELERATION 1
#define PARTICLE_SPARK 0
#define PARTICLE_SMOKE 1
#define PARTICLE_FLAME 2
#define PARTICLE_DEBRIS 3

#if MAP_COLUMNS * MAP_TILE_PIXEL_SIZE <= 32767 && MAP_ROWS * MAP_TILE_PIXEL_SIZE <= 32767
#define world_tile(tile, x, y) gfx_world_tile(tile, (int)(x), (int)(y))
#define world_rect(x, y, width, height, color) gfx_world_rect((int)(x), (int)(y), width, height, color)
#define world_particle(x, y, radius, color, opacity) gfx_world_particle((int)(x), (int)(y), radius, color, opacity)
#define world_ammo(x, y) gfx_world_ammo((int)(x), (int)(y))
#else
#define world_tile(tile, x, y) gfx_world_tile_wide(tile, x, y)
#define world_rect(x, y, width, height, color) gfx_world_rect_wide(x, y, width, height, color)
#define world_particle(x, y, radius, color, opacity) gfx_world_particle_wide(x, y, radius, color, opacity)
#define world_ammo(x, y) gfx_world_ammo_wide(x, y)
#endif

#if MAP_COLUMNS * MAP_TILE_PIXEL_SIZE <= 32767 && MAP_ROWS * MAP_TILE_PIXEL_SIZE <= 32767
typedef int WorldCoord;
#define world_emitters(sites, count, tick) gfx_world_emitters(sites, count, tick)
#else
typedef long WorldCoord;
#define world_emitters(sites, count, tick) gfx_world_emitters_wide(sites, count, tick)
#endif

#if MAP_COLUMNS * MAP_TILE_PIXEL_SIZE <= 32767 && MAP_ROWS * MAP_TILE_PIXEL_SIZE <= 32767
#define mask_overlap_world(first, x, y, second, second_x, second_y) \
    gfx_mask_overlap(first, (int)(x), (int)(y), second, \
                     (int)(second_x), (int)(second_y))
#else
#define mask_overlap_world(first, x, y, second, second_x, second_y) \
    gfx_mask_overlap_wide(first, x, y, second, second_x, second_y)
#endif

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int direction;
    int health;
    int cooldown;
    int collision_cooldown;
    int movement_remainder_x;
    int movement_remainder_y;
    int throttle;
    int speed;
    int turn_tick;
    int impulse_x;
    int impulse_y;
    int shake;
    int handbrake;
} Tank;

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int owner;
    int direction;
    int life;
    int movement_remainder_x;
    int movement_remainder_y;
} Shot;

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int velocity_x;
    int velocity_y;
    int life;
    int smoke;
    int age;
    int size;
    int remainder_x;
    int remainder_y;
} Particle;

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int rounds;
    int mask;
} BurnSite;

Tank tanks[2];
Shot shots[MAX_SHOTS];
Particle particles[MAX_PARTICLES];
BurnSite burn_sites[MAX_BURN_SITES];
BurnSite *next_burn_site;
int next_burn_index;
int burn_count;
int burn_capacity_error;
int effect_tick;
unsigned int effect_random = 12345;
int next_particle;
unsigned char sprite_work[TANK_SPRITE_PIXEL_COUNT];
unsigned char object_active[MAP_OBJECT_CAPACITY];
unsigned char object_contacts[MAP_OBJECT_CAPACITY];
unsigned char object_explosion[MAP_OBJECT_CAPACITY];
WorldCoord track_x[MAX_TRACK_MARKS];
WorldCoord track_y[MAX_TRACK_MARKS];
int track_direction[MAX_TRACK_MARKS];
int track_offset_x[MAX_TRACK_MARKS];
int track_offset_y[MAX_TRACK_MARKS];
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
WorldCoord muzzle_x[2];
WorldCoord muzzle_y[2];
WorldCoord impact_x[MAX_IMPACTS];
WorldCoord impact_y[MAX_IMPACTS];
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

int map_obstacle_at(int row, int column)
{
    if (row < 0 || row >= MAP_ROWS ||
        column < 0 || column >= MAP_COLUMNS)
        return 0;
#ifdef MAP_OBSTACLES_EMPTY
    return 0;
#else
#if MAP_CELLS <= 32768
    return map_obstacles[row * MAP_COLUMNS + column];
#else
#if MAP_ROWS > 0
    if (row < 32) return map_obstacles_0[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 32
    if (row < 64) return map_obstacles_1[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 64
    if (row < 96) return map_obstacles_2[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 96
    if (row < 128) return map_obstacles_3[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 128
    if (row < 160) return map_obstacles_4[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 160
    if (row < 192) return map_obstacles_5[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 192
    if (row < 224) return map_obstacles_6[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 224
    if (row < 256) return map_obstacles_7[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 256
    if (row < 288) return map_obstacles_8[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 288
    if (row < 320) return map_obstacles_9[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 320
    if (row < 352) return map_obstacles_10[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 352
    if (row < 384) return map_obstacles_11[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 384
    if (row < 416) return map_obstacles_12[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 416
    if (row < 448) return map_obstacles_13[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 448
    if (row < 480) return map_obstacles_14[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 480
    if (row < 512) return map_obstacles_15[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 512
    if (row < 544) return map_obstacles_16[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 544
    if (row < 576) return map_obstacles_17[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 576
    if (row < 608) return map_obstacles_18[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 608
    if (row < 640) return map_obstacles_19[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 640
    if (row < 672) return map_obstacles_20[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 672
    if (row < 704) return map_obstacles_21[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 704
    if (row < 736) return map_obstacles_22[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 736
    if (row < 768) return map_obstacles_23[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 768
    if (row < 800) return map_obstacles_24[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 800
    if (row < 832) return map_obstacles_25[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 832
    if (row < 864) return map_obstacles_26[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 864
    if (row < 896) return map_obstacles_27[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 896
    if (row < 928) return map_obstacles_28[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 928
    if (row < 960) return map_obstacles_29[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 960
    if (row < 992) return map_obstacles_30[(row % 32) * MAP_COLUMNS + column];
#endif
#if MAP_ROWS > 992
    return map_obstacles_31[(row % 32) * MAP_COLUMNS + column];
#endif
#endif
#endif
    return 0;
}

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

WorldCoord absolute_value(WorldCoord value)
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

int effect_noise(int range)
{
    effect_random = effect_random * 25173 + 13849;
    return (int)((effect_random >> 1) % (unsigned int)range);
}

void add_burn_site(WorldCoord x, WorldCoord y, int mask)
{
    int i;
    int index;
    BurnSite *site;

    index = next_burn_index;
    site = next_burn_site;
    if (site == 0)
        site = burn_sites;
    for (i = 0; i < MAX_BURN_SITES; i++)
    {
        if (site->rounds == 0)
        {
            site->x = x < 0 ? 0 : x;
            site->y = y < 0 ? 0 : y;
            if (site->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE)
                site->x = (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 1;
            if (site->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE)
                site->y = (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE - 1;
            site->rounds = BURN_ROUNDS + 1;
            site->mask = mask;
            if (index == burn_count)
                burn_count++;
            site++;
            index++;
            if (index == MAX_BURN_SITES)
            {
                site = burn_sites;
                index = 0;
            }
            next_burn_site = site;
            next_burn_index = index;
            return;
        }
        site++;
        index++;
        if (index == MAX_BURN_SITES)
        {
            site = burn_sites;
            index = 0;
        }
    }
    burn_capacity_error = 1;
}

void age_burn_sites(void)
{
    int i;
    BurnSite *site;

    site = burn_sites;
    for (i = 0; i < burn_count; i++)
    {
        if (site->rounds > 0)
            site->rounds--;
        site++;
    }
}

void reset_battlefield(void)
{
    int i;
    BurnSite *site;

    site = burn_sites;
    for (i = 0; i < burn_count; i++)
    {
        site->rounds = 0;
        site++;
    }
    burn_count = 0;
    next_burn_site = burn_sites;
    next_burn_index = 0;
    burn_capacity_error = 0;
}

void tank_impulse(int player, WorldCoord x, WorldCoord y, int strength)
{
    tanks[player].impulse_x += (int)x;
    tanks[player].impulse_y += (int)y;
    if (tanks[player].impulse_x > 48)
        tanks[player].impulse_x = 48;
    if (tanks[player].impulse_x < -48)
        tanks[player].impulse_x = -48;
    if (tanks[player].impulse_y > 48)
        tanks[player].impulse_y = 48;
    if (tanks[player].impulse_y < -48)
        tanks[player].impulse_y = -48;
    if (strength > tanks[player].shake)
        tanks[player].shake = strength;
}

void blast_reaction(WorldCoord x, WorldCoord y, int strength)
{
    int player;
    WorldCoord dx;
    WorldCoord dy;
    WorldCoord distance;
    int force;

    for (player = 0; player < 2; player++)
    {
        if (tanks[player].health <= 0)
            continue;
        dx = tanks[player].x - x;
        dy = tanks[player].y - y;
        distance = absolute_value(dx) + absolute_value(dy);
        if (distance < 64)
        {
            force = strength * (64 - distance) / 64;
            if (distance == 0)
            {
                dx = directions_x[tanks[player].direction] / 100;
                dy = directions_y[tanks[player].direction] / 100;
                distance = 10;
            }
            tank_impulse(player, dx * force / distance,
                         dy * force / distance, force / 4 + 2);
            tanks[player].speed /= 2;
        }
    }
}

void emit_particle(WorldCoord x, WorldCoord y, int velocity_x, int velocity_y, int smoke)
{
    Particle *particle;

    particle = &particles[next_particle];
    particle->x = x;
    particle->y = y;
    particle->velocity_x = velocity_x * 4;
    particle->velocity_y = velocity_y * 4;
    particle->life = smoke == PARTICLE_SMOKE ? 36 + effect_noise(25) : smoke == PARTICLE_FLAME ? 12 + effect_noise(13)
                                                                                               : 18 + effect_noise(PARTICLE_FRAMES - 17);
    particle->smoke = smoke;
    particle->age = 0;
    particle->size = smoke == PARTICLE_SMOKE ? 3 + effect_noise(3) : 1 + effect_noise(2);
    particle->remainder_x = 0;
    particle->remainder_y = 0;
    next_particle = (next_particle + 1) % MAX_PARTICLES;
}

void emit_explosion(WorldCoord x, WorldCoord y, int speed)
{
    int direction;

    for (direction = 0; direction < TANK_ROTATION_COUNT; direction += 2)
    {
        emit_particle(x + effect_noise(5) - 2, y + effect_noise(5) - 2,
                      direction_offset(directions_x[direction], speed) + effect_noise(3) - 1,
                      direction_offset(directions_y[direction], speed) - effect_noise(2),
                      direction % 4 == 0 ? PARTICLE_SPARK : PARTICLE_DEBRIS);
    }
    for (direction = 0; direction < 4; direction++)
    {
        emit_particle(x + effect_noise(9) - 4, y + effect_noise(5) - 2,
                      effect_noise(3) - 1, -1, PARTICLE_SMOKE);
        emit_particle(x + effect_noise(7) - 3, y,
                      effect_noise(3) - 1, -1, PARTICLE_FLAME);
    }
}

void explode_object(int index)
{
    object_explosion[index] = EXPLOSION_FRAMES;
    emit_explosion((WorldCoord)map_object_x[index] * MAP_TILE_PIXEL_SIZE +
                       MAP_TILE_PIXEL_SIZE / 2,
                   (WorldCoord)map_object_y[index] * MAP_TILE_PIXEL_SIZE +
                       MAP_TILE_PIXEL_SIZE / 2,
                   2);
    blast_reaction((WorldCoord)map_object_x[index] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2,
                   (WorldCoord)map_object_y[index] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2, 40);
    gfx_sound(0, 90, 8, WAVE_NOISE, 35);
}

void add_impact(WorldCoord x, WorldCoord y, int size)
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
            add_burn_site(tanks[player].x, tanks[player].y,
                          tanks[player].direction & TANK_DIRECTION_MASK);
            tanks[player].speed = 0;
            tanks[player].throttle = 0;
            blast_reaction(tanks[player].x, tanks[player].y, 48);
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

int tank_collision(WorldCoord x, WorldCoord y, int player)
{
    int other;

    other = 1 - player;
    if (tanks[other].health <= 0)
        return 0;
    if (absolute_value(x - tanks[other].x) >= 32 ||
        absolute_value(y - tanks[other].y) >= 32)
        return 0;
    prepare_collision_assets();
    return mask_overlap_world(tank_mask(player), x - 16, y - 16,
                              tank_mask(other), tanks[other].x - 16,
                              tanks[other].y - 16);
}

int obstacle_mask_collision(int mask, WorldCoord x, WorldCoord y, int size)
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
            tile = map_obstacle_at(row, column);
            if (tile != 0 &&
                mask_overlap_world(mask, x, y, GFX_MASK_TILE_BASE + tile,
                                   (WorldCoord)column * MAP_TILE_PIXEL_SIZE,
                                   (WorldCoord)row * MAP_TILE_PIXEL_SIZE))
            {
                return 1;
            }
        }
    }
    return 0;
}

int blocked(WorldCoord x, WorldCoord y, int tank_index)
{
    WorldCoord map_width;
    WorldCoord map_height;

    map_width = (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;
    map_height = (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE;
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

int movement_blocked(WorldCoord old_x, WorldCoord old_y,
                     WorldCoord x, WorldCoord y, int player)
{
    int step;
    int steps;
    int dy;

    steps = absolute_value(x - old_x);
    dy = absolute_value(y - old_y);
    if (dy > steps)
        steps = dy;
    for (step = 1; step <= steps; step++)
    {
        if (blocked(old_x + (x - old_x) * step / steps,
                    old_y + (y - old_y) * step / steps, player))
            return 1;
    }
    return 0;
}

void start_round(void)
{
    int i;
    int player;
    int row;
    int column;
    WorldCoord x;
    WorldCoord y;
    WorldCoord target_x;
    WorldCoord target_y;
    WorldCoord best_x;
    WorldCoord best_y;
    WorldCoord best_score;
    WorldCoord score;
    int found;
    int center_column;
    int center_row;
    int radius;
    int first_row;
    int last_row;
    int first_column;
    int last_column;

    if (round_over)
        age_burn_sites();
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
                       ? (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE / 4
                       : (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE * 3 / 4;
        target_y = (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE / 2;
        center_column = target_x / MAP_TILE_PIXEL_SIZE;
        center_row = target_y / MAP_TILE_PIXEL_SIZE;
        best_x = target_x;
        best_y = target_y;
        best_score = 32767;
        found = 0;
        for (radius = 0;
             radius < MAP_COLUMNS + MAP_ROWS && !found; radius++)
        {
            first_row = center_row - radius;
            if (first_row < 0)
                first_row = 0;
            last_row = center_row + radius;
            if (last_row >= MAP_ROWS)
                last_row = MAP_ROWS - 1;
            first_column = center_column - radius;
            if (first_column < 0)
                first_column = 0;
            last_column = center_column + radius;
            if (last_column >= MAP_COLUMNS)
                last_column = MAP_COLUMNS - 1;
            for (row = first_row; row <= last_row; row++)
            {
                for (column = first_column; column <= last_column; column++)
                {
                    if (absolute_value(column - center_column) +
                            absolute_value(row - center_row) != radius ||
                        (player == 0 && column >= MAP_COLUMNS / 2) ||
                        (player == 1 && column < MAP_COLUMNS / 2))
                        continue;
                    x = (WorldCoord)column * MAP_TILE_PIXEL_SIZE +
                        MAP_TILE_PIXEL_SIZE / 2;
                    y = (WorldCoord)row * MAP_TILE_PIXEL_SIZE +
                        MAP_TILE_PIXEL_SIZE / 2;
                    if (blocked(x, y, player))
                        continue;
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
        tanks[i].throttle = 0;
        tanks[i].speed = 0;
        tanks[i].turn_tick = 0;
        tanks[i].impulse_x = 0;
        tanks[i].impulse_y = 0;
        tanks[i].shake = 0;
        tanks[i].handbrake = 0;
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
            tank_impulse(player, -direction_offset(directions_x[direction], 12),
                         -direction_offset(directions_y[direction], 12), 3);
            emit_particle(muzzle_x[player], muzzle_y[player],
                          direction_offset(directions_x[direction], 2),
                          direction_offset(directions_y[direction], 2), PARTICLE_SMOKE);
            gfx_sound(player, 230, 3, WAVE_NOISE, 22);
            return;
        }
    }
}

void update_tank(int player, unsigned int keys, unsigned int extra)
{
    WorldCoord old_x;
    WorldCoord old_y;
    int direction;
    WorldCoord next_x;
    WorldCoord next_y;
    int movement_numerator_x;
    int movement_numerator_y;
    int can_move_x;
    int can_move_y;
    int forward_key;
    int backward_key;
    int old_direction;
    int turn;
    int turn_frames;
    int left_key;
    int right_key;

    if (tanks[player].health <= 0)
        return;

    old_direction = tanks[player].direction;
    forward_key = player == 0 ? (int)(keys & 4) : (int)(extra & KEYX2_P2_FORWARD);
    backward_key = player == 0 ? (int)(keys & 8) : (int)(extra & KEYX2_P2_BACKWARD);
    left_key = player == 0 ? (int)(keys & 1) : (int)(extra & KEYX2_P2_LEFT);
    right_key = player == 0 ? (int)(keys & 2) : (int)(extra & KEYX2_P2_RIGHT);
    tanks[player].handbrake = left_key && right_key;
    if (tanks[player].handbrake)
    {
        tanks[player].throttle = 0;
        if (tanks[player].speed > HANDBRAKE_DECELERATION)
            tanks[player].speed -= HANDBRAKE_DECELERATION;
        else if (tanks[player].speed < -HANDBRAKE_DECELERATION)
            tanks[player].speed += HANDBRAKE_DECELERATION;
        else
            tanks[player].speed = 0;
        tanks[player].impulse_x = 0;
        tanks[player].impulse_y = 0;
        tanks[player].movement_remainder_x = 0;
        tanks[player].movement_remainder_y = 0;
    }
    else
    {
        if (forward_key && !backward_key && tanks[player].throttle < MAX_FORWARD_SPEED)
            tanks[player].throttle++;
        if (backward_key && !forward_key && tanks[player].throttle > -MAX_REVERSE_SPEED)
            tanks[player].throttle--;
        if (tanks[player].speed < tanks[player].throttle)
            tanks[player].speed++;
        if (tanks[player].speed > tanks[player].throttle)
            tanks[player].speed--;
    }
    turn = (right_key != 0) - (left_key != 0);
    if (turn != 0)
    {
        turn_frames = STATIONARY_TURN_FRAMES +
                      absolute_value(tanks[player].speed) *
                          (MOVING_TURN_FRAMES - STATIONARY_TURN_FRAMES) / MAX_FORWARD_SPEED;
        tanks[player].turn_tick++;
        if (tanks[player].turn_tick >= turn_frames)
        {
            tanks[player].turn_tick = 0;
            tanks[player].direction = (old_direction + turn) & TANK_DIRECTION_MASK;
            if (blocked(tanks[player].x, tanks[player].y, player))
                tanks[player].direction = old_direction;
        }
    }
    else
        tanks[player].turn_tick = 0;
    old_x = tanks[player].x;
    old_y = tanks[player].y;
    direction = tanks[player].direction;
    movement_numerator_x = tanks[player].movement_remainder_x +
                           directions_x[direction] * tanks[player].speed / DIRECTION_SCALE +
                           tanks[player].impulse_x;
    movement_numerator_y = tanks[player].movement_remainder_y +
                           directions_y[direction] * tanks[player].speed / DIRECTION_SCALE +
                           tanks[player].impulse_y;
    next_x = old_x + movement_numerator_x / SPEED_SCALE;
    next_y = old_y + movement_numerator_y / SPEED_SCALE;
    tanks[player].impulse_x = tanks[player].impulse_x * 3 / 4;
    tanks[player].impulse_y = tanks[player].impulse_y * 3 / 4;
    if ((next_x != old_x || next_y != old_y) && tank_collision(next_x, next_y, player))
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
        tank_impulse(player, (old_x - next_x) * 8, (old_y - next_y) * 8, 4);
        tank_impulse(1 - player, (next_x - old_x) * 8, (next_y - old_y) * 8, 4);
        tanks[player].speed = 0;
        tanks[player].throttle = 0;
        tanks[player].movement_remainder_x = 0;
        tanks[player].movement_remainder_y = 0;
    }
    else if (!movement_blocked(old_x, old_y, next_x, next_y, player))
    {
        tanks[player].x = next_x;
        tanks[player].y = next_y;
        tanks[player].movement_remainder_x =
            movement_numerator_x - (next_x - old_x) * SPEED_SCALE;
        tanks[player].movement_remainder_y =
            movement_numerator_y - (next_y - old_y) * SPEED_SCALE;
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
                movement_numerator_x - (next_x - old_x) * SPEED_SCALE;
        }
        else if (can_move_y)
        {
            tanks[player].y = next_y;
            tanks[player].movement_remainder_y =
                movement_numerator_y - (next_y - old_y) * SPEED_SCALE;
        }
        tanks[player].speed /= 2;
        tanks[player].throttle = tanks[player].speed;
        tanks[player].impulse_x = 0;
        tanks[player].impulse_y = 0;
    }
    if ((tanks[player].x != old_x || tanks[player].y != old_y) &&
        absolute_value(tanks[player].x - last_track_x[player]) +
                absolute_value(tanks[player].y - last_track_y[player]) >=
            TRACK_SPACING)
    {
        track_x[next_track] = old_x;
        track_y[next_track] = old_y;
        track_direction[next_track] = direction;
        track_offset_x[next_track] = direction_offset(directions_y[direction], 10);
        track_offset_y[next_track] = -direction_offset(directions_x[direction], 10);
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
                mask_overlap_world(tank_mask(player),
                                   tanks[player].x - 16, tanks[player].y - 16,
                                   GFX_MASK_TILE_BASE + map_object_tile[i],
                                   (WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                                   (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE))
            {
                contacts |= mask;
                if (!(object_contacts[i] & mask))
                {
                    if (map_object_hp[i] < 0 && map_object_removal[i] != OBJECT_EXPLOSION)
                        blast_reaction((WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2,
                                       (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2, 24);
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
        if (tanks[i].shake > 0)
            tanks[i].shake--;
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
    int move;
    Particle *particle;

    for (i = 0; i < MAX_PARTICLES; i++)
    {
        particle = &particles[i];
        if (particle->life <= 0)
            continue;
        particle->life--;
        if (particle->life == 0)
            continue;
        particle->age++;
        if (particle->smoke == PARTICLE_SMOKE || particle->smoke == PARTICLE_FLAME)
        {
            if (particle->age % 4 == 0)
            {
                particle->velocity_x += effect_noise(3) - 1;
                if (particle->velocity_y > -8)
                    particle->velocity_y--;
            }
        }
        else
        {
            if (particle->age % 3 == 0)
            {
                particle->velocity_x = particle->velocity_x * 3 / 4;
                particle->velocity_y++;
            }
        }
        particle->remainder_x += particle->velocity_x;
        move = particle->remainder_x / 4;
        particle->x += move;
        particle->remainder_x -= move * 4;
        particle->remainder_y += particle->velocity_y;
        move = particle->remainder_y / 4;
        particle->y += move;
        particle->remainder_y -= move * 4;
        if (particle->x < 0 || particle->y < 0 ||
            particle->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE ||
            particle->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE)
            particle->life = 0;
    }
    effect_tick = (effect_tick + 1) % 4320;
}

int shot_collision(int index)
{
    int i;
    int player;
    Shot *shot;

    shot = &shots[index];
    if (shot->x < 3 || shot->y < 3 ||
        shot->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 3 ||
        shot->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE - 3 ||
        obstacle_mask_collision(GFX_MASK_AMMO, shot->x - 3, shot->y - 3, 7))
        return 1;
    player = 1 - shot->owner;
    if (tanks[player].health > 0 &&
        gfx_mask_overlap_wide(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                              tank_mask(player), tanks[player].x - 16,
                              tanks[player].y - 16))
    {
        tank_impulse(player, direction_offset(directions_x[shot->direction], 24),
                     direction_offset(directions_y[shot->direction], 24), 8);
        change_health(player, -10);
        return 1;
    }
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_active[i] &&
            gfx_mask_overlap_wide(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                             GFX_MASK_TILE_BASE + map_object_tile[i],
                             (WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                             (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE))
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
            gfx_mask_overlap_wide(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                             GFX_MASK_AMMO, shots[i].x - 3, shots[i].y - 3))
        {
            add_burn_site(shots[i].x, shots[i].y, -1);
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
    WorldCoord old_x;
    WorldCoord old_y;
    int movement_numerator_x;
    int movement_numerator_y;
    WorldCoord next_x;
    WorldCoord next_y;
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
                add_burn_site(shot->x, shot->y, -1);
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
        reset_battlefield();
        round_over = 0;
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
            gfx_sprite_move_wide(tank_sprite_slots[player], 0, 0, 0);
        }
        tank_sprite_slots[player] = slot;
    }
    gfx_sprite_move_wide(slot, tanks[player].x - 16,
                         tanks[player].y - 16,
                         tanks[player].health > 0 ? SPRITE_VISIBLE : 0);
}

WorldCoord camera_origin(WorldCoord position, int viewport_size, WorldCoord map_size)
{
    WorldCoord camera;

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

WorldCoord tank_camera_x(int player)
{
    return camera_origin(tanks[player].x, 320,
                         (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE) +
           (effect_tick % 2 == 0 ? tanks[player].shake / 3 : -tanks[player].shake / 3);
}

WorldCoord tank_camera_y(int player)
{
    return camera_origin(tanks[player].y, 480,
                         (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE) +
           (effect_tick % 3 == 0 ? tanks[player].shake / 4 : -tanks[player].shake / 4);
}

void draw_map_obstacles(WorldCoord camera_x, WorldCoord camera_y)
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
            tile = map_obstacle_at(row, column);
            if (tile != 0)
            {
                world_tile(tile, (WorldCoord)column * MAP_TILE_PIXEL_SIZE,
                           (WorldCoord)row * MAP_TILE_PIXEL_SIZE);
            }
        }
    }
}

void draw_ground(void)
{
    int viewport;
    WorldCoord camera_x;
    WorldCoord camera_y;
    int i;
    int offset_x;
    int offset_y;

    gfx_layer_select(LAYER_3D);
    gfx_clear(0);
    for (i = 0; i < track_count; i++)
    {
        offset_x = track_offset_x[i];
        offset_y = track_offset_y[i];
        world_rect(track_x[i] + offset_x - 1,
                   track_y[i] + offset_y - 1, 3, 3, TRACK_COLOR);
        world_rect(track_x[i] - offset_x - 1,
                   track_y[i] - offset_y - 1, 3, 3, TRACK_COLOR);
    }
    for (viewport = 0; viewport < 2; viewport++)
    {
        camera_x = tank_camera_x(viewport);
        camera_y = tank_camera_y(viewport);
        draw_map_obstacles(camera_x, camera_y);
    }
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_active[i])
        {
            world_tile(
                map_object_tile[i],
                (WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE);
        }
    }
    world_emitters(burn_sites, burn_count, effect_tick);
}

void draw_combat_feedback(void)
{
    int i;
    int radius;
    int color;

    for (i = 0; i < MAX_IMPACTS; i++)
    {
        if (impact_life[i] > 0)
        {
            radius = 2 + impact_life[i] * impact_size[i] / (IMPACT_FRAMES * 2);
            color = impact_life[i] > IMPACT_FRAMES / 2 ? YELLOW : ORANGE;
            world_particle(impact_x[i], impact_y[i], radius, color,
                           impact_life[i] > 7 ? 12 : 4);
        }
    }
    for (i = 0; i < 2; i++)
    {
        if (muzzle_flash[i] > 0)
        {
            world_particle(muzzle_x[i], muzzle_y[i], 5, ORANGE, 8);
            world_particle(muzzle_x[i], muzzle_y[i], 2, WHITE, 16);
        }
        if (hit_flash[i] > 0 && tanks[i].health > 0)
        {
            world_particle(tanks[i].x, tanks[i].y, 16, RED, 2);
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
    WorldCoord explosion_x;
    WorldCoord explosion_y;
    int radius;
    int particle_size;
    int particle_color;
    int opacity;
    Particle *particle;

    gfx_layer_select(LAYER_HUD);
    gfx_clear(15);
    for (i = 0; i < MAX_PARTICLES; i++)
    {
        particle = &particles[i];
        if (particle->life > 0)
        {
            opacity = particle->life < 8 ? particle->life * 2 : 16;
            if (particle->smoke == PARTICLE_SMOKE)
            {
                particle_size = particle->size + particle->age / 8;
                particle_color = particle->age < 18 ? DARK_GRAY : GRAY;
                opacity = particle->life < 16 ? particle->life / 2 : 8;
            }
            else
            {
                particle_size = particle->smoke == PARTICLE_FLAME ? 2 + particle->life / 8 : particle->size;
                particle_color = particle->smoke == PARTICLE_DEBRIS ? TRACK_COLOR : particle->age < 3 ? WHITE
                                                                                : particle->age < 9   ? YELLOW
                                                                                : particle->age < 17  ? ORANGE
                                                                                                      : DARK_RED;
            }
            world_particle(particle->x, particle->y,
                           particle_size, particle_color, opacity);
        }
    }
    draw_combat_feedback();
    for (i = 0; i < MAP_OBJECT_COUNT; i++)
    {
        if (object_explosion[i] > EXPLOSION_FRAMES - 4)
        {
            explosion_x = (WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2;
            explosion_y = (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE + MAP_TILE_PIXEL_SIZE / 2;
            radius = 4 + object_explosion[i] / 2;
            world_particle(explosion_x, explosion_y, radius, ORANGE, 8);
        }
    }
    for (i = 0; i < MAX_SHOTS; i++)
    {
        if (shots[i].life > 0)
            world_ammo(shots[i].x, shots[i].y);
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
    gfx_text(180, 5, tanks[0].handbrake ? "BRAKE" : tanks[0].speed < 0 ? "REV"
                                                                       : "GAS",
             WHITE);
    gfx_rect(180, 19, 64, 4, BLACK);
    gfx_rect(180, 19, absolute_value(tanks[0].throttle) * 2, 4, YELLOW);
    gfx_text(398, 5, tanks[1].handbrake ? "BRAKE" : tanks[1].speed < 0 ? "REV"
                                                                       : "GAS",
             WHITE);
    gfx_rect(398, 19, 64, 4, BLACK);
    gfx_rect(398, 19, absolute_value(tanks[1].throttle) * 2, 4, YELLOW);
    if (burn_capacity_error)
        gfx_text(240, 34, "FIRE CAPACITY REACHED", RED);
}

void draw_game(void)
{
    WorldCoord camera1_x;
    WorldCoord camera1_y;
    WorldCoord camera2_x;
    WorldCoord camera2_y;
    int viewport;
    char countdown[6];

    camera1_x = tank_camera_x(0);
    camera1_y = tank_camera_y(0);
    camera2_x = tank_camera_x(1);
    camera2_y = tank_camera_y(1);
    gfx_split_camera_wide(camera1_x, camera1_y, camera2_x, camera2_y);
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
