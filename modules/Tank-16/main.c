#include "suomi_gfx.h"
#include "map_assets.h"
#include "tank_sprite_rotations.h"
#ifdef RUNTIME_MAP_LOADING
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "disk.h"
#endif

#ifndef POWERUP_HEALTH_TILE
#ifdef RUNTIME_MAP_LOADING
#define POWERUP_HEALTH_TILE (TERRAIN_TILE_COUNT + 15)
#define POWERUP_DAMAGE_TILE (TERRAIN_TILE_COUNT + 16)
#define POWERUP_SPEED_TILE (TERRAIN_TILE_COUNT + 17)
#else
#define POWERUP_HEALTH_TILE (TILE_COUNT + 15)
#define POWERUP_DAMAGE_TILE (TILE_COUNT + 16)
#define POWERUP_SPEED_TILE (TILE_COUNT + 17)
#endif
#endif

#ifdef RUNTIME_MAP_LOADING
#define RUNTIME_MAP_MAX_CELLS 10000UL
#define RUNTIME_MAP_MAX_ROWS 1024
#define RUNTIME_MAP_CHUNK 4096UL
unsigned char runtime_map_data[RUNTIME_MAP_MAX_CELLS];
unsigned char runtime_map_obstacles[RUNTIME_MAP_MAX_CELLS];
unsigned char runtime_row_loaded[RUNTIME_MAP_MAX_ROWS];
int runtime_map_lazy;
unsigned long runtime_grid_offset;
unsigned long runtime_row_stride;
int runtime_band_rows;
int runtime_map_rows_loaded;
int runtime_tilemap_dirty;
char runtime_map_path[129];
int load_map_band(int row);
unsigned char runtime_empty_tilemap[1];
int runtime_terrain_usage[256];
int runtime_obstacle_usage[256];
unsigned char runtime_tile_needed[256];
int runtime_tile_progress_total;
int runtime_tile_progress_loaded;
int runtime_loading_active;
int runtime_loading_frame;
char *runtime_loading_stage;
void draw_runtime_loading_screen(char *message, int current, int total);
#endif

#define MAP_TILE_PIXEL_SIZE 64
#define TANK16_HUD_HEIGHT 30
#define MINIMAP_MAX_WIDTH 96
#define MINIMAP_MAX_HEIGHT 24
#define MAP_OBJECT_SOLID_CAPACITY 10000
#define MAX_SHOTS 12
#define ROUND_FRAMES 9000
#define ROUND_DELAY 90
#define POWERUP_DURATION 300
#define POWERUP_SPAWN_DELAY 300
#define POWERUP_RESPAWN_DELAY 360
#define POWERUP_LIFETIME 360
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
#define AMMO_BURN_SECONDS 10
#define TANK_BURN_SECONDS 30
#define MAX_AMMO_BURN_SITES_PER_PLAYER 10
#define MAX_TANK_BURN_SITES_PER_PLAYER 2
#define MAX_BURN_SITES ((MAX_AMMO_BURN_SITES_PER_PLAYER + MAX_TANK_BURN_SITES_PER_PLAYER) * 2)
#define BURN_RENDER_BATCH 8
#define BURN_KIND_AMMO 0
#define BURN_KIND_TANK 1
#define SPEED_SCALE 16
#define MAX_FORWARD_SPEED 32
#define MAX_REVERSE_SPEED 16
#define DEEP_WATER_TILE_ID 90
#define WATER_DAMAGE_INTERVAL 30
#define WATER_DAMAGE_AMOUNT 10
#define ROUND_MAP_PATH_CAPACITY 64
#define MOVING_TURN_FRAMES 4
#define STATIONARY_TURN_FRAMES 2
#define HANDBRAKE_DECELERATION 8
#define PARTICLE_SPARK 0
#define PARTICLE_SMOKE 1
#define PARTICLE_FLAME 2
#define PARTICLE_DEBRIS 3
#define POWERUP_HEALTH 0
#define POWERUP_SPEED 1
#define POWERUP_DAMAGE 2

#ifndef MAP_VARIANT_COUNT
#define MAP_VARIANT_COUNT 1
#endif

#ifdef RUNTIME_MAP_LOADING
#define world_tile(tile, x, y) gfx_world_tile_wide(tile, x, y)
#define world_rect(x, y, width, height, color) gfx_world_rect_wide(x, y, width, height, color)
#define world_particle(x, y, radius, color, opacity) gfx_world_particle_wide(x, y, radius, color, opacity)
#define world_ammo(x, y) gfx_world_ammo_wide(x, y)
typedef long WorldCoord;
#define world_emitters(sites, count, tick) gfx_world_emitters_wide(sites, count, tick)
#define mask_overlap_world(first, x, y, second, second_x, second_y) \
    gfx_mask_overlap_wide(first, x, y, second, second_x, second_y)
#else
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
    int speed_boost_frames;
    int damage_boost_frames;
} Tank;

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int owner;
    int direction;
    int life;
    int damage;
    int movement_remainder_x;
    int movement_remainder_y;
} Shot;

typedef struct
{
    WorldCoord x;
    WorldCoord y;
    int type;
    int life;
} PowerUp;

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
    int remaining_seconds;
    int mask;
} BurnSite;

Tank tanks[2];
Shot shots[MAX_SHOTS];
Particle particles[MAX_PARTICLES];
BurnSite burn_sites[MAX_BURN_SITES];
unsigned char burn_owner[MAX_BURN_SITES];
unsigned char burn_kind[MAX_BURN_SITES];
BurnSite *next_burn_site;
int next_burn_index;
int burn_count;
int burn_active_count;
int burn_render_cursor;
int burn_capacity_error;
unsigned int burn_last_time;
int burn_clock_initialized;
int effect_tick;
unsigned int effect_random = 12345;
int next_particle;
PowerUp powerup;
int powerup_spawn_timer;
int movement_sound_cooldown[2];
int deep_water_frames[2];
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
int map_object_count = MAP_OBJECT_COUNT;
unsigned char solid_map_objects[MAP_OBJECT_SOLID_CAPACITY];
int current_map_variant = -1;
int round_map_selection_started;
void reset_battlefield(void);
#ifdef RUNTIME_MAP_LOADING
int map_infinite;
char current_map_path[129];
char round_map_paths[ROUND_MAP_PATH_CAPACITY][129];
int round_map_path_count;
int round_map_cursor;
int round_map_selection_active;
int round_map_choice;
int round_map_choice_ready;
int round_map_path_limit_reached;
unsigned long map_seed;
unsigned char map_terrain_choices[16];
int map_terrain_choice_count;
unsigned char map_infinite_obstacles[16];
int map_infinite_obstacle_count;
int map_obstacle_density;
int map_load_error;
#endif
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

#ifdef RUNTIME_MAP_LOADING
unsigned long map_hash(int row, int column)
{
    unsigned long value;

    value = map_seed ^ (unsigned long)column * 2654435761UL;
    value ^= (unsigned long)row * 2246822519UL;
    value ^= value >> 13;
    value *= 3266489917UL;
    value ^= value >> 16;
    return value;
}

void json_skip_space(char **cursor)
{
    while (**cursor == ' ' || **cursor == '\n' || **cursor == '\r' ||
           **cursor == '\t' || **cursor == ',')
        (*cursor)++;
}

void json_skip_whitespace(char **cursor)
{
    while (**cursor == ' ' || **cursor == '\n' || **cursor == '\r' ||
           **cursor == '\t')
        (*cursor)++;
}

char *json_value(char *json, char *key)
{
    char needle[32];
    char *found;

    needle[0] = '"';
    strcpy(needle + 1, key);
    strcat(needle, "\"");
    found = strstr(json, needle);
    if (found == 0)
        return 0;
    found = found + strlen(needle);
    while (*found == ' ' || *found == '\t' || *found == '\r' ||
           *found == '\n')
        found++;
    if (*found++ != ':')
        return 0;
    while (*found == ' ' || *found == '\t' || *found == '\r' ||
           *found == '\n')
        found++;
    return found;
}

int json_string_is(char *value, char *expected)
{
    int length;

    if (value == 0 || *value != '"')
        return 0;
    value++;
    length = strlen(expected);
    return strncmp(value, expected, length) == 0 && value[length] == '"';
}

int json_integer(char **cursor, int *value)
{
    int sign;
    int result;
    int digit;

    json_skip_whitespace(cursor);
    sign = 1;
    if (**cursor == '-')
    {
        sign = -1;
        (*cursor)++;
    }
    if (**cursor < '0' || **cursor > '9')
        return 0;
    result = 0;
    while (**cursor >= '0' && **cursor <= '9')
    {
        digit = **cursor - '0';
        if (result > (32767 - digit) / 10)
            return 0;
        result = result * 10 + digit;
        (*cursor)++;
    }
    *value = sign * result;
    return 1;
}

int json_unsigned_long(char **cursor, unsigned long *value)
{
    unsigned long result;
    int digit;

    json_skip_whitespace(cursor);
    if (**cursor < '0' || **cursor > '9')
        return 0;
    result = 0;
    while (**cursor >= '0' && **cursor <= '9')
    {
        digit = **cursor - '0';
        if (result > (4294967295UL - (unsigned long)digit) / 10UL)
            return 0;
        result = result * 10UL + (unsigned long)digit;
        (*cursor)++;
    }
    *value = result;
    return 1;
}

int json_hex_digit(int character)
{
    if (character >= '0' && character <= '9')
        return character - '0';
    if (character >= 'a' && character <= 'f')
        return character - 'a' + 10;
    if (character >= 'A' && character <= 'F')
        return character - 'A' + 10;
    return -1;
}

int json_pair(char **cursor, int *tile_x, int *tile_y)
{
    json_skip_space(cursor);
    if (**cursor != '[')
        return 0;
    (*cursor)++;
    if (!json_integer(cursor, tile_x))
        return 0;
    json_skip_whitespace(cursor);
    if (**cursor != ',')
        return 0;
    (*cursor)++;
    if (!json_integer(cursor, tile_y))
        return 0;
    json_skip_whitespace(cursor);
    if (**cursor != ']')
        return 0;
    (*cursor)++;
    return 1;
}

int load_json_bytes(char *path, char **json, unsigned long *size)
{
    unsigned long offset;
    int length;
    int i;
    unsigned long copied;
    int progress_chunks;

    length = disk_read_file(path, 0, map_json_chunk, 4096, size);
    if (length <= 0 || *size == 0 || length > *size ||
        *size > 67108864UL)
    {
        printf("JSON READ ERROR: %s first-chunk=%d size=%lu\n",
               path, length, *size);
        return 0;
    }
    *json = (char *)malloc(*size + 1);
    if (*json == 0)
    {
        printf("JSON READ ERROR: %s allocation failed (%lu bytes)\n",
               path, *size + 1);
        return 0;
    }
    for (i = 0; i < length; i++)
        (*json)[i] = map_json_chunk[i];
    offset = (unsigned long)length;
    copied = offset;
    progress_chunks = 0;
    while (offset < *size)
    {
        length = disk_read_file(path, offset, map_json_chunk, 4096, size);
        if (length <= 0)
        {
            printf("JSON READ ERROR: %s offset=%lu chunk=%d\n",
                   path, offset, length);
            free(*json);
            *json = 0;
            return 0;
        }
        for (i = 0; i < length; i++)
            (*json)[offset + (unsigned long)i] = map_json_chunk[i];
        offset += (unsigned long)length;
        copied += length;
        progress_chunks++;
        if (runtime_loading_active && progress_chunks % 32 == 0)
            draw_runtime_loading_screen(
                runtime_loading_stage, runtime_tile_progress_loaded,
                runtime_tile_progress_total);
    }
    (*json)[copied] = 0;
    return 1;
}

int load_runtime_tile_data(void)
{
    char path[] = RUNTIME_TILE_DATA_PATH;
    char *json;
    char *cursor;
    char *value;
    unsigned long file_size;
    int version;
    int palette_entries;
    int terrain_count;
    int object_count;
    int tile_count;
    int declared_tile_count;
    int tile;
    int red;
    int green;
    int blue;
    int result;

    printf("TILE DATA: reading %s\n", path);
    if (!load_json_bytes(path, &json, &file_size))
    {
        printf("TILE DATA ERROR: could not read JSON file\n");
        return 0;
    }
    value = json_value(json, "format");
    result = json_string_is(value, "SC16-TANK16-TILES");
    value = json_value(json, "version");
    if (!result || value == 0 || !json_integer(&value, &version) ||
        version != 1)
    {
        free(json);
        return 0;
    }
    value = json_value(json, "tile_size");
    if (value == 0 || !json_integer(&value, &runtime_tile_size) ||
        runtime_tile_size != MAP_TILE_PIXEL_SIZE ||
        runtime_tile_size < 1 || runtime_tile_size > 64)
    {
        free(json);
        return 0;
    }
    value = json_value(json, "terrain_columns");
    if (value == 0 || !json_integer(&value, &runtime_terrain_columns) ||
        runtime_terrain_columns < 1 || runtime_terrain_columns > 16)
    {
        free(json);
        return 0;
    }
    value = json_value(json, "terrain_rows");
    if (value == 0 || !json_integer(&value, &runtime_terrain_rows) ||
        runtime_terrain_rows < 1 || runtime_terrain_rows > 16)
    {
        free(json);
        return 0;
    }
    value = json_value(json, "object_columns");
    if (value == 0 || !json_integer(&value, &runtime_object_columns) ||
        runtime_object_columns < 1 || runtime_object_columns > 16)
    {
        free(json);
        return 0;
    }
    value = json_value(json, "object_rows");
    if (value == 0 || !json_integer(&value, &runtime_object_rows) ||
        runtime_object_rows < 1 || runtime_object_rows > 16)
    {
        free(json);
        return 0;
    }
    terrain_count = TERRAIN_TILE_COUNT;
    object_count = OBJECT_COLUMNS * OBJECT_ROWS;
    tile_count = terrain_count + object_count;
    if (tile_count > 256 || tile_count < 1)
    {
        free(json);
        return 0;
    }
    printf("TILE DATA HEADER: tile-size=%d terrain=%dx%d objects=%dx%d "
           "tiles=%d\n", runtime_tile_size,
           runtime_terrain_columns, runtime_terrain_rows,
           runtime_object_columns, runtime_object_rows, tile_count);
    cursor = json_value(json, "palette");
    if (cursor == 0)
    {
        free(json);
        return 0;
    }
    json_skip_whitespace(&cursor);
    if (*cursor++ != '[')
    {
        free(json);
        return 0;
    }
    palette_entries = 0;
    json_skip_whitespace(&cursor);
    while (*cursor != ']')
    {
        if (palette_entries >= 224)
        {
            free(json);
            return 0;
        }
        json_skip_whitespace(&cursor);
        if (*cursor++ != '[' ||
            !json_integer(&cursor, &red) || red < 0 || red > 255)
        {
            free(json);
            return 0;
        }
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' ||
            !json_integer(&cursor, &green) || green < 0 || green > 255)
        {
            free(json);
            return 0;
        }
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' ||
            !json_integer(&cursor, &blue) || blue < 0 || blue > 255)
        {
            free(json);
            return 0;
        }
        json_skip_whitespace(&cursor);
        if (*cursor++ != ']')
        {
            free(json);
            return 0;
        }
        gfx_palette((unsigned char)(MAP_PALETTE_BASE + palette_entries),
                    (unsigned char)red, (unsigned char)green,
                    (unsigned char)blue);
        palette_entries++;
        json_skip_whitespace(&cursor);
        if (*cursor == ',')
        {
            cursor++;
            json_skip_whitespace(&cursor);
        }
        else if (*cursor != ']')
        {
            free(json);
            return 0;
        }
    }
    cursor++;
    if (palette_entries < 1)
    {
        free(json);
        return 0;
    }
    runtime_palette_count = palette_entries + 1;
    printf("TILE DATA PALETTE: %d RGB colors loaded\n", palette_entries);

    value = json_value(json, "pixel_encoding");
    runtime_tile_binary = 0;
    if (value != 0)
    {
        if (!json_string_is(value, "raw-index-v1"))
        {
            free(json);
            return 0;
        }
        runtime_tile_binary = 1;
    }
    value = json_value(json, "tile_count");
    if (value == 0 || !json_integer(&value, &declared_tile_count) ||
        declared_tile_count != tile_count)
    {
        free(json);
        return 0;
    }
    printf("TILE DATA: %s (%lu bytes)\n", path, file_size);
    runtime_tile_count = tile_count;
    runtime_tile_data_ready = 1;
    for (tile = 0; tile < 256; tile++)
        runtime_tile_loaded[tile] = 0;
    free(json);
    return 1;
}

int ensure_runtime_tile_loaded(int tile)
{
    char tile_path[] = "assets/tiles/tile_000.json";
    char binary_path[] = RUNTIME_TILE_BINARY_PATH;
    char *tile_json;
    char *cursor;
    unsigned long tile_file_size;
    int length;
    int pixel;
    int red;
    int green;
    int index;
    int palette_entries;

    if (!runtime_tile_data_ready || tile < 0 ||
        tile >= runtime_tile_count)
    {
        printf("TILE DATA ERROR: invalid tile request id=%d\n", tile);
        return 0;
    }
    if (runtime_tile_loaded[tile])
        return 1;
    if (runtime_tile_binary)
    {
        length = disk_read_file(binary_path,
                                (unsigned long)tile *
                                    (unsigned long)TILE_PIXEL_COUNT,
                                runtime_tile_pixels, TILE_PIXEL_COUNT,
                                &tile_file_size);
        if (length != TILE_PIXEL_COUNT ||
            tile_file_size != (unsigned long)runtime_tile_count *
                                  (unsigned long)TILE_PIXEL_COUNT)
        {
            printf("TILE DATA ERROR: invalid binary tile %d read=%d "
                   "size=%lu\n", tile, length, tile_file_size);
            return 0;
        }
        palette_entries = runtime_palette_count - 1;
        for (pixel = 0; pixel < TILE_PIXEL_COUNT; pixel++)
        {
            index = runtime_tile_pixels[pixel];
            if (index != 0 &&
                (index < MAP_PALETTE_BASE ||
                 index >= MAP_PALETTE_BASE + palette_entries))
            {
                printf("TILE DATA ERROR: invalid palette index %d in tile %d\n",
                       index, tile);
                return 0;
            }
        }
        gfx_tile_upload(tile, runtime_tile_pixels);
        runtime_tile_loaded[tile] = 1;
        if (runtime_loading_active)
        {
            runtime_tile_progress_loaded++;
            if (runtime_tile_progress_loaded % 4 == 0 ||
                runtime_tile_progress_loaded == runtime_tile_progress_total)
                draw_runtime_loading_screen(
                    "LOADING TILES", runtime_tile_progress_loaded,
                    runtime_tile_progress_total);
        }
        if (tile < TERRAIN_TILE_COUNT)
            printf("TILE %d terrain source=(%d,%d) loaded (%lu bytes)\n",
                   tile, tile % runtime_terrain_columns,
                   tile / runtime_terrain_columns, tile_file_size /
                       (unsigned long)runtime_tile_count);
        else
        {
            int object_tile;

            object_tile = tile - TERRAIN_TILE_COUNT;
            printf("TILE %d object source=(%d,%d) loaded (%lu bytes)\n",
                   tile, object_tile % runtime_object_columns,
                   object_tile / runtime_object_columns,
                   tile_file_size / (unsigned long)runtime_tile_count);
        }
        return 1;
    }
    tile_path[18] = (char)('0' + tile / 100);
    tile_path[19] = (char)('0' + (tile / 10) % 10);
    tile_path[20] = (char)('0' + tile % 10);
    if (!load_json_bytes(tile_path, &tile_json, &tile_file_size))
    {
        printf("TILE DATA ERROR: could not read %s\n", tile_path);
        return 0;
    }
    cursor = json_value(tile_json, "pixels");
    if (cursor == 0)
    {
        free(tile_json);
        return 0;
    }
    json_skip_whitespace(&cursor);
    if (*cursor++ != '"')
    {
        free(tile_json);
        return 0;
    }
    palette_entries = runtime_palette_count - 1;
    for (pixel = 0; pixel < TILE_PIXEL_COUNT; pixel++)
    {
        red = json_hex_digit(*cursor++);
        green = json_hex_digit(*cursor++);
        index = red * 16 + green;
        if (red < 0 || green < 0 ||
            (index != 0 &&
             (index < MAP_PALETTE_BASE ||
              index >= MAP_PALETTE_BASE + palette_entries)))
        {
            free(tile_json);
            return 0;
        }
        runtime_tile_pixels[pixel] = (unsigned char)index;
    }
    if (*cursor++ != '"')
    {
        free(tile_json);
        return 0;
    }
    json_skip_whitespace(&cursor);
    if (*cursor++ != '}')
    {
        free(tile_json);
        return 0;
    }
    json_skip_whitespace(&cursor);
    if (*cursor != 0)
    {
        free(tile_json);
        return 0;
    }
    gfx_tile_upload(tile, runtime_tile_pixels);
    runtime_tile_loaded[tile] = 1;
    if (runtime_loading_active)
    {
        runtime_tile_progress_loaded++;
        if (runtime_tile_progress_loaded % 4 == 0 ||
            runtime_tile_progress_loaded == runtime_tile_progress_total)
            draw_runtime_loading_screen(
                "LOADING TILES", runtime_tile_progress_loaded,
                runtime_tile_progress_total);
    }
    if (tile < TERRAIN_TILE_COUNT)
        printf("TILE %d terrain source=(%d,%d) loaded (%lu bytes)\n", tile,
               tile % runtime_terrain_columns,
               tile / runtime_terrain_columns, tile_file_size);
    else
    {
        int object_tile;

        object_tile = tile - TERRAIN_TILE_COUNT;
        printf("TILE %d object source=(%d,%d) loaded (%lu bytes)\n", tile,
               object_tile % runtime_object_columns,
               object_tile / runtime_object_columns, tile_file_size);
    }
    free(tile_json);
    return 1;
}

int map_dimensions(char *json, int *width, int *height)
{
    char *value;

    value = json_value(json, "width");
    if (value == 0)
        return 0;
    if (!json_integer(&value, width))
        return 0;
    value = json_value(json, "height");
    if (value == 0)
        return 0;
    if (!json_integer(&value, height))
        return 0;
    return *width > 0 && *height > 0 &&
           (unsigned long)*width * (unsigned long)*height <= 16777216UL;
}

int parse_object_list(char *cursor, unsigned long cells, int width)
{
    int cell;
    int tile;
    int hp;
    int removal;

    json_skip_whitespace(&cursor);
    if (*cursor++ != '[')
        return 0;
    json_skip_whitespace(&cursor);
    while (*cursor != ']')
    {
        if (*cursor++ != '[' ||
            !json_integer(&cursor, &cell) || cell < 0 ||
            (unsigned long)cell >= cells)
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' ||
            !json_integer(&cursor, &tile) ||
            tile < 0 || tile >= OBJECT_COLUMNS * OBJECT_ROWS)
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' || !json_integer(&cursor, &hp))
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' || !json_integer(&cursor, &removal) ||
            removal < OBJECT_FIXED || removal > OBJECT_DISAPPEAR)
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ']' || map_object_count >= MAP_OBJECT_CAPACITY)
            return 0;
        map_object_x[map_object_count] = cell % width;
        map_object_y[map_object_count] = cell / width;
        map_object_tile[map_object_count] = OBJECT_TILE_BASE + tile;
        map_object_source_tile[map_object_count] = tile;
        map_object_hp[map_object_count] = hp;
        map_object_removal[map_object_count] = removal;
        map_object_count++;
        json_skip_whitespace(&cursor);
        if (*cursor == ',')
        {
            cursor++;
            json_skip_whitespace(&cursor);
        }
        else if (*cursor != ']')
            return 0;
    }
    return 1;
}

int load_finite_map_flat(char *json, int width, int height)
{
    char *cursor;
    int cell;
    int tile;
    int hp;
    int removal;
    unsigned long cells;
    unsigned long index;
    char *value;

    cells = (unsigned long)width * (unsigned long)height;
    if (cells > RUNTIME_MAP_MAX_CELLS)
        return 0;
    map_data = runtime_map_data;
    map_obstacles = runtime_map_obstacles;
    memset(map_data, 0, cells);
    memset(map_obstacles, 0, cells);
    value = json_value(json, "tiles");
    if (value == 0)
        return 0;
    cursor = value;
    json_skip_space(&cursor);
    if (*cursor++ != '[')
        return 0;
    for (index = 0; index < cells; index++)
    {
        if (!json_integer(&cursor, &tile) ||
            tile < 0 || tile >= TERRAIN_TILE_COUNT)
            return 0;
        map_data[index] = (unsigned char)tile;
        if (index + 1 < cells)
        {
            json_skip_whitespace(&cursor);
            if (*cursor++ != ',')
                return 0;
        }
    }
    json_skip_whitespace(&cursor);
    if (*cursor++ != ']')
        return 0;

    value = json_value(json, "obstacles");
    if (value != 0)
    {
        cursor = value;
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        json_skip_space(&cursor);
        while (*cursor != ']')
        {
            if (*cursor++ != '[' ||
                !json_integer(&cursor, &cell) || cell < 0 ||
                (unsigned long)cell >= cells)
                return 0;
            json_skip_whitespace(&cursor);
            if (*cursor++ != ',' ||
                !json_integer(&cursor, &tile) ||
                tile < 0 || tile >= OBJECT_COLUMNS * OBJECT_ROWS)
                return 0;
            json_skip_whitespace(&cursor);
            if (*cursor++ != ']' || map_obstacles[cell] != 0)
                return 0;
            map_obstacles[cell] = (unsigned char)(OBJECT_TILE_BASE + tile);
            json_skip_whitespace(&cursor);
            if (*cursor == ',')
            {
                cursor++;
                json_skip_whitespace(&cursor);
            }
            else if (*cursor != ']')
                return 0;
        }
    }
    value = json_value(json, "objects");
    if (value != 0 && !parse_object_list(value, cells, width))
        return 0;
    map_columns = width;
    map_rows = height;
    map_infinite = 0;
    return 1;
}

int parse_usage_list(char *cursor, int *usage, int base, int limit)
{
    int tile;
    int count;

    if (cursor == 0)
        return 0;
    json_skip_whitespace(&cursor);
    if (*cursor++ != '[')
        return 0;
    json_skip_whitespace(&cursor);
    while (*cursor != ']')
    {
        if (*cursor++ != '[' || !json_integer(&cursor, &tile) ||
            tile < 0 || tile >= limit)
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ',' || !json_integer(&cursor, &count) || count < 0)
            return 0;
        json_skip_whitespace(&cursor);
        if (*cursor++ != ']')
            return 0;
        usage[base + tile] = count;
        json_skip_whitespace(&cursor);
        if (*cursor == ',')
        {
            cursor++;
            json_skip_whitespace(&cursor);
        }
        else if (*cursor != ']')
            return 0;
    }
    return 1;
}

int read_file_range(char *path, unsigned long offset, unsigned long size,
                    char **out)
{
    unsigned long copied;
    unsigned long file_size;
    int request;
    int length;

    *out = (char *)malloc(size + 1);
    if (*out == 0)
    {
        printf("MAP READ ERROR: %s allocation failed (%lu bytes)\n",
               path, size + 1);
        return 0;
    }
    copied = 0;
    while (copied < size)
    {
        request = size - copied > 4096UL ? 4096 : (int)(size - copied);
        length = disk_read_file(
            path, offset + copied,
            (volatile unsigned char *)(*out + copied), request, &file_size);
        if (length <= 0)
        {
            printf("MAP READ ERROR: %s offset=%lu chunk=%d\n",
                   path, offset + copied, length);
            free(*out);
            *out = 0;
            return 0;
        }
        copied += (unsigned long)length;
    }
    (*out)[size] = 0;
    return 1;
}

int load_finite_map_grid(char *path, char *header, int width, int height)
{
    char *value;
    char *objects;
    unsigned long cells;
    unsigned long objects_offset;
    unsigned long objects_size;
    int i;
    int result;

    cells = (unsigned long)width * (unsigned long)height;
    if (cells > RUNTIME_MAP_MAX_CELLS || height > RUNTIME_MAP_MAX_ROWS)
        return 0;
    value = json_value(header, "grid_offset");
    if (value == 0 || !json_unsigned_long(&value, &runtime_grid_offset))
        return 0;
    runtime_row_stride = (unsigned long)width * 4UL + 4UL;
    runtime_band_rows = (int)(RUNTIME_MAP_CHUNK / runtime_row_stride);
    if (runtime_band_rows < 1)
        runtime_band_rows = 1;
    map_data = runtime_map_data;
    map_obstacles = runtime_map_obstacles;
    memset(map_data, 0, cells);
    memset(map_obstacles, 0, cells);
    for (i = 0; i < height; i++)
        runtime_row_loaded[i] = 0;
    for (i = 0; i < 256; i++)
    {
        runtime_terrain_usage[i] = 0;
        runtime_obstacle_usage[i] = 0;
    }
    if (!parse_usage_list(json_value(header, "used_tiles"),
                          runtime_terrain_usage, 0, TERRAIN_TILE_COUNT) ||
        !parse_usage_list(json_value(header, "used_obstacles"),
                          runtime_obstacle_usage, OBJECT_TILE_BASE,
                          OBJECT_COLUMNS * OBJECT_ROWS))
        return 0;
    value = json_value(header, "objects_offset");
    if (value == 0 || !json_unsigned_long(&value, &objects_offset))
        return 0;
    value = json_value(header, "objects_size");
    if (value == 0 || !json_unsigned_long(&value, &objects_size) ||
        objects_size < 2 || objects_size > 65000UL)
        return 0;
    if (!read_file_range(path, objects_offset, objects_size, &objects))
        return 0;
    result = parse_object_list(objects, cells, width);
    free(objects);
    if (!result)
    {
        printf("MAP FORMAT ERROR: %s objects not at offset %lu "
               "(were line endings converted?)\n", path, objects_offset);
        return 0;
    }
    for (i = 0; i < 128 && path[i] != 0; i++)
        runtime_map_path[i] = path[i];
    runtime_map_path[i] = 0;
    map_columns = width;
    map_rows = height;
    map_infinite = 0;
    runtime_map_lazy = 1;
    runtime_map_rows_loaded = 0;
    printf("MAP INDEX: rows=%d stride=%lu grid-offset=%lu band=%d rows "
           "objects=%d\n", height, runtime_row_stride, runtime_grid_offset,
           runtime_band_rows, map_object_count);
    return 1;
}

int load_map_band(int row)
{
    int first;
    int last;
    int current_row;
    int column_char;
    int row_chars;
    int length;
    int request;
    int i;
    int digit;
    int value;
    unsigned long offset;
    unsigned long end;
    unsigned long cell;
    unsigned long file_size;

    first = row - row % runtime_band_rows;
    last = first + runtime_band_rows;
    if (last > map_rows)
        last = map_rows;
    offset = runtime_grid_offset + (unsigned long)first * runtime_row_stride;
    end = runtime_grid_offset + (unsigned long)last * runtime_row_stride;
    current_row = first;
    column_char = 0;
    value = 0;
    row_chars = map_columns * 4;
    cell = (unsigned long)first * (unsigned long)map_columns;
    for (i = first; i < last; i++)
        runtime_row_loaded[i] = 1;
    while (offset < end)
    {
        request = end - offset > 4096UL ? 4096 : (int)(end - offset);
        length = disk_read_file(runtime_map_path, offset, map_json_chunk,
                                request, &file_size);
        if (length <= 0)
        {
            printf("MAP ROW ERROR: %s y=%d..%d offset=%lu\n",
                   runtime_map_path, first, last - 1, offset);
            map_load_error = 1;
            return 0;
        }
        for (i = 0; i < length; i++)
        {
            if (column_char == 0)
            {
                if (map_json_chunk[i] != '"')
                {
                    printf("MAP ROW ERROR: %s y=%d is not at %lu\n",
                           runtime_map_path, current_row, offset + i);
                    map_load_error = 1;
                    return 0;
                }
            }
            else if (column_char <= row_chars)
            {
                digit = json_hex_digit(map_json_chunk[i]);
                if (digit < 0)
                {
                    map_load_error = 1;
                    return 0;
                }
                value = value * 16 + digit;
                if ((column_char & 3) == 2)
                {
                    if (value >= TERRAIN_TILE_COUNT)
                        value = 0;
                    map_data[cell] = (unsigned char)value;
                    value = 0;
                }
                else if ((column_char & 3) == 0)
                {
                    if (value > 0 && value <= OBJECT_COLUMNS * OBJECT_ROWS)
                        map_obstacles[cell] =
                            (unsigned char)(OBJECT_TILE_BASE + value - 1);
                    value = 0;
                    cell++;
                }
            }
            column_char++;
            if ((unsigned long)column_char == runtime_row_stride)
            {
                column_char = 0;
                current_row++;
            }
        }
        offset += (unsigned long)length;
    }
    runtime_map_rows_loaded += last - first;
    runtime_tilemap_dirty = 1;
    printf("MAP ROWS: y=%d..%d loaded (%lu bytes, %d/%d rows)\n",
           first, last - 1, end - (runtime_grid_offset +
           (unsigned long)first * runtime_row_stride),
           runtime_map_rows_loaded, map_rows);
    return 1;
}

void ensure_map_rows(int first_row, int last_row)
{
    int row;

    if (!runtime_map_lazy)
        return;
    if (first_row < 0)
        first_row = 0;
    if (last_row >= map_rows)
        last_row = map_rows - 1;
    for (row = first_row; row <= last_row; row++)
        if (!runtime_row_loaded[row])
            load_map_band(row);
}

void sync_runtime_tilemap(void)
{
    if (runtime_tilemap_dirty && !map_infinite && map_data != 0)
    {
        gfx_tilemap(map_data, map_columns, map_rows, 0, 0);
        runtime_tilemap_dirty = 0;
    }
}

int load_finite_map(char *json, int width, int height)
{
    char *cursor;
    char *cell;
    char *value;
    int row;
    int column;
    int tile_x;
    int tile_y;
    int hp;
    int removal;
    unsigned long cells;
    unsigned long index;
    char *removal_text;

    cells = (unsigned long)width * (unsigned long)height;
    if (cells > RUNTIME_MAP_MAX_CELLS)
        return 0;
    value = json_value(json, "runtime_encoding");
    if (json_string_is(value, "flat-v1"))
        return load_finite_map_flat(json, width, height);
    map_data = runtime_map_data;
    map_obstacles = runtime_map_obstacles;
    memset(map_data, 0, cells);
    memset(map_obstacles, 0, cells);
    cursor = json_value(json, "tiles");
    if (cursor == 0)
        return 0;
    json_skip_space(&cursor);
    if (*cursor++ != '[')
        return 0;
    for (row = 0; row < height; row++)
    {
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        for (column = 0; column < width; column++)
        {
            if (!json_pair(&cursor, &tile_x, &tile_y) ||
                tile_x < 0 || tile_x >= TERRAIN_COLUMNS ||
                tile_y < 0 || tile_y >= TERRAIN_ROWS)
                return 0;
            map_data[(unsigned long)row * (unsigned long)width + column] =
                (unsigned char)(tile_y * TERRAIN_COLUMNS + tile_x);
        }
        json_skip_space(&cursor);
        if (*cursor++ != ']')
            return 0;
    }
    json_skip_space(&cursor);
    if (*cursor++ != ']')
        return 0;

    cursor = json_value(json, "obstacles");
    if (cursor != 0)
    {
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        for (row = 0; row < height; row++)
        {
            json_skip_space(&cursor);
            if (*cursor++ != '[')
                return 0;
            for (column = 0; column < width; column++)
            {
                json_skip_space(&cursor);
                index = (unsigned long)row * (unsigned long)width +
                        (unsigned long)column;
                if (strncmp(cursor, "null", 4) == 0)
                {
                    cursor = cursor + 4;
                    continue;
                }
                if (!json_pair(&cursor, &tile_x, &tile_y) ||
                    tile_x < 0 || tile_x >= OBJECT_COLUMNS ||
                    tile_y < 0 || tile_y >= OBJECT_ROWS)
                    return 0;
                map_obstacles[index] =
                    (unsigned char)(OBJECT_TILE_BASE +
                                    tile_y * OBJECT_COLUMNS + tile_x);
            }
            json_skip_space(&cursor);
            if (*cursor++ != ']')
                return 0;
        }
        json_skip_space(&cursor);
        if (*cursor++ != ']')
            return 0;
    }

    cursor = json_value(json, "objects");
    if (cursor != 0)
    {
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        for (row = 0; row < height; row++)
        {
            json_skip_space(&cursor);
            if (*cursor++ != '[')
                return 0;
            for (column = 0; column < width; column++)
            {
                json_skip_space(&cursor);
                if (strncmp(cursor, "null", 4) == 0)
                {
                    cursor = cursor + 4;
                    continue;
                }
                if (*cursor != '{' || map_object_count >= MAP_OBJECT_CAPACITY)
                    return 0;
                cell = cursor;
                cursor = strstr(cursor, "\"tile\"");
                if (cursor == 0)
                    return 0;
                cursor = strchr(cursor, ':');
                if (cursor == 0)
                    return 0;
                cursor++;
                if (!json_pair(&cursor, &tile_x, &tile_y) ||
                    tile_x < 0 || tile_x >= OBJECT_COLUMNS ||
                    tile_y < 0 || tile_y >= OBJECT_ROWS)
                    return 0;
                hp = -25;
                removal = OBJECT_EXPLOSION;
                value = json_value(cell, "hp_change");
                if (value != 0 && !json_integer(&value, &hp))
                    return 0;
                value = json_value(cell, "removal");
                if (value != 0 && *value == '"')
                {
                    removal_text = value + 1;
                    if (strncmp(removal_text, "fixed", 5) == 0)
                        removal = OBJECT_FIXED;
                    else if (strncmp(removal_text, "disappear", 9) == 0)
                        removal = OBJECT_DISAPPEAR;
                }
                map_object_x[map_object_count] = column;
                map_object_y[map_object_count] = row;
                map_object_tile[map_object_count] =
                    OBJECT_TILE_BASE + tile_y * OBJECT_COLUMNS + tile_x;
                map_object_source_tile[map_object_count] =
                    tile_y * OBJECT_COLUMNS + tile_x;
                cursor = cell;
                cursor = strchr(cursor, '}');
                if (cursor == 0)
                    return 0;
                map_object_hp[map_object_count] = hp;
                map_object_removal[map_object_count] = removal;
                map_object_count++;
                cursor++;
            }
            json_skip_space(&cursor);
            if (*cursor++ != ']')
                return 0;
        }
        json_skip_space(&cursor);
        if (*cursor++ != ']')
            return 0;
    }
    map_columns = width;
    map_rows = height;
    map_infinite = 0;
    return 1;
}

int load_infinite_map(char *json)
{
    char *value;
    char *cursor;
    int number;
    unsigned long seed;

    map_infinite = 1;
    map_columns = 0;
    map_rows = 0;
    map_seed = 1;
    map_terrain_choice_count = 0;
    map_infinite_obstacle_count = 0;
    map_obstacle_density = 0;
    map_object_count = 0;
    value = json_value(json, "seed");
    if (value != 0)
    {
        if (!json_unsigned_long(&value, &seed))
            return 0;
        map_seed = seed;
    }
    value = json_value(json, "obstacle_density");
    if (value != 0)
    {
        if (!json_integer(&value, &number) || number < 0)
            return 0;
        map_obstacle_density = number;
    }
    value = json_value(json, "terrain_tiles");
    if (value != 0)
    {
        cursor = value;
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        while (map_terrain_choice_count < 16)
        {
            json_skip_space(&cursor);
            if (*cursor == ']')
                break;
            if (!json_integer(&cursor, &number) ||
                number < 0 || number >= TERRAIN_TILE_COUNT)
                return 0;
            map_terrain_choices[map_terrain_choice_count++] =
                (unsigned char)number;
        }
        json_skip_space(&cursor);
        if (*cursor != ']')
            return 0;
    }
    if (map_terrain_choice_count == 0)
    {
        map_terrain_choices[0] = 0;
        map_terrain_choice_count = 1;
    }
    value = json_value(json, "obstacle_tiles");
    if (value != 0)
    {
        cursor = value;
        json_skip_space(&cursor);
        if (*cursor++ != '[')
            return 0;
        while (map_infinite_obstacle_count < 16)
        {
            json_skip_space(&cursor);
            if (*cursor == ']')
                break;
            if (!json_integer(&cursor, &number) ||
                number < 0 || number >= OBJECT_COLUMNS * OBJECT_ROWS)
                return 0;
            map_infinite_obstacles[map_infinite_obstacle_count++] =
                (unsigned char)(OBJECT_TILE_BASE + number);
        }
        json_skip_space(&cursor);
        if (*cursor != ']')
            return 0;
    }
    map_data = 0;
    map_obstacles = 0;
    return 1;
}

int load_runtime_map_tiles(void)
{
    unsigned long index;
    int tile;
    int object;
    int i;

    if (!runtime_tile_data_ready)
        return 1;
    for (i = 0; i < runtime_tile_count; i++)
        runtime_tile_needed[i] = 0;
    runtime_tile_needed[POWERUP_HEALTH_TILE] = 1;
    runtime_tile_needed[POWERUP_DAMAGE_TILE] = 1;
    runtime_tile_needed[POWERUP_SPEED_TILE] = 1;
    if (map_infinite)
    {
        for (tile = 0; tile < map_terrain_choice_count; tile++)
            runtime_tile_needed[map_terrain_choices[tile]] = 1;
        for (tile = 0; tile < map_infinite_obstacle_count; tile++)
            runtime_tile_needed[map_infinite_obstacles[tile]] = 1;
    }
    else
    {
        for (tile = 0; tile < 256; tile++)
            if (runtime_terrain_usage[tile] > 0 ||
                runtime_obstacle_usage[tile] > 0)
                runtime_tile_needed[tile] = 1;
        for (object = 0; object < map_object_count; object++)
            runtime_tile_needed[map_object_tile[object]] = 1;
    }
    runtime_tile_progress_total = 0;
    runtime_tile_progress_loaded = 0;
    for (i = 0; i < runtime_tile_count; i++)
        if (runtime_tile_needed[i] && !runtime_tile_loaded[i])
            runtime_tile_progress_total++;
    runtime_loading_stage = "LOADING TILES";
    draw_runtime_loading_screen("LOADING TILES", 0,
                                runtime_tile_progress_total);
    if (!ensure_runtime_tile_loaded(POWERUP_HEALTH_TILE) ||
        !ensure_runtime_tile_loaded(POWERUP_DAMAGE_TILE) ||
        !ensure_runtime_tile_loaded(POWERUP_SPEED_TILE))
    {
        runtime_loading_active = 0;
        return 0;
    }
    if (map_infinite)
    {
        for (tile = 0; tile < map_terrain_choice_count; tile++)
            if (!ensure_runtime_tile_loaded(map_terrain_choices[tile]))
            {
                runtime_loading_active = 0;
                return 0;
            }
        for (tile = 0; tile < map_infinite_obstacle_count; tile++)
            if (!ensure_runtime_tile_loaded(map_infinite_obstacles[tile]))
            {
                runtime_loading_active = 0;
                return 0;
            }
        draw_runtime_loading_screen("MAP READY", 1, 1);
        return 1;
    }
    for (i = 0; i < runtime_tile_count; i++)
        if (runtime_tile_needed[i] && !ensure_runtime_tile_loaded(i))
        {
            runtime_loading_active = 0;
            return 0;
        }
    draw_runtime_loading_screen("MAP READY", 1, 1);
    return 1;
}

void compute_map_usage(void)
{
    unsigned long index;
    unsigned long cells;
    int i;

    for (i = 0; i < 256; i++)
    {
        runtime_terrain_usage[i] = 0;
        runtime_obstacle_usage[i] = 0;
    }
    cells = (unsigned long)map_columns * (unsigned long)map_rows;
    for (index = 0; index < cells; index++)
    {
        runtime_terrain_usage[map_data[index]]++;
        if (map_obstacles[index] != 0)
            runtime_obstacle_usage[map_obstacles[index]]++;
    }
}

void configure_runtime_tilemap(void)
{
    gfx_layer_select(LAYER_TILES);
    runtime_tilemap_dirty = 0;
    if (map_infinite)
        gfx_tilemap(runtime_empty_tilemap, 1, 1, 0, 0);
    else
        gfx_tilemap(map_data, map_columns, map_rows, 0, 0);
}

int load_map_header(char *path, char **header, unsigned long *file_size)
{
    char *value;
    unsigned long grid_offset;
    int length;

    length = disk_read_file(path, 0, map_json_chunk, 4096, file_size);
    if (length <= 0 || *file_size == 0)
    {
        printf("MAP READ ERROR: %s header=%d\n", path, length);
        return 0;
    }
    *header = (char *)malloc(length + 1);
    if (*header == 0)
        return 0;
    memcpy(*header, (char *)map_json_chunk, length);
    (*header)[length] = 0;
    value = json_value(*header, "runtime_encoding");
    if (!json_string_is(value, "grid-hex-v2"))
    {
        free(*header);
        *header = 0;
        return 1;
    }
    value = json_value(*header, "grid_offset");
    if (value == 0 || !json_unsigned_long(&value, &grid_offset) ||
        grid_offset > (unsigned long)length)
    {
        free(*header);
        *header = 0;
        return 0;
    }
    (*header)[grid_offset] = 0;
    printf("MAP HEADER: %s %lu of %lu bytes read\n",
           path, grid_offset, *file_size);
    return 1;
}

int check_map_tilesets(char *json)
{
    char *value;

    value = json_value(json, "tileset");
    if (!json_string_is(value, "tiles_terrain.png"))
        return 0;
    value = json_value(json, "objects_tileset");
    return value == 0 || json_string_is(value, "tiles_objects.png");
}

int load_map_path(char *path)
{
    char *json;
    char *header;
    char *value;
    unsigned long file_size;
    int width;
    int height;
    int result;

    map_object_count = 0;
    runtime_map_lazy = 0;
    runtime_tilemap_dirty = 0;
    if (!load_map_header(path, &header, &file_size))
        return 0;
    json = 0;
    if (header != 0)
    {
        result = map_dimensions(header, &width, &height) &&
                 check_map_tilesets(header) &&
                 load_finite_map_grid(path, header, width, height);
        free(header);
    }
    else
    {
        if (!load_json_bytes(path, &json, &file_size))
            return 0;
        value = json_value(json, "infinite");
        if (value != 0 && strncmp(value, "true", 4) == 0)
            result = load_infinite_map(json);
        else
            result = map_dimensions(json, &width, &height) &&
                     check_map_tilesets(json) &&
                     load_finite_map(json, width, height);
        if (result && !map_infinite)
            compute_map_usage();
        free(json);
    }
    if (result)
        result = load_runtime_map_tiles();
    if (result)
        configure_runtime_tilemap();
    if (!result)
    {
        runtime_map_lazy = 0;
        printf("MAP LOAD ERROR: invalid or unavailable map %s\n", path);
    }
    return result;
}

void log_loaded_map(char *path)
{
    int i;
    int tile;

    printf("MAP FILE: %s\n", path);
    if (map_infinite)
    {
        printf("MAP COORDINATES: unbounded; seed=%lu\n", map_seed);
        for (i = 0; i < map_terrain_choice_count; i++)
        {
            tile = map_terrain_choices[i];
            printf("MAP TERRAIN TILE: id=%d source=(%d,%d)\n", tile,
                   tile % TERRAIN_COLUMNS, tile / TERRAIN_COLUMNS);
        }
        for (i = 0; i < map_infinite_obstacle_count; i++)
        {
            tile = map_infinite_obstacles[i] - OBJECT_TILE_BASE;
            printf("MAP OBSTACLE TILE: source=(%d,%d)\n",
                   tile % OBJECT_COLUMNS, tile / OBJECT_COLUMNS);
        }
        return;
    }

    printf("MAP COORDINATES: x=0..%d y=0..%d cells=%d\n",
           map_columns - 1, map_rows - 1, map_columns * map_rows);
    for (tile = 0; tile < TERRAIN_TILE_COUNT; tile++)
    {
        if (runtime_terrain_usage[tile] > 0)
            printf("MAP TERRAIN TILE: id=%d source=(%d,%d) cells=%d\n",
                   tile, tile % TERRAIN_COLUMNS,
                   tile / TERRAIN_COLUMNS, runtime_terrain_usage[tile]);
    }
    for (tile = OBJECT_TILE_BASE;
         tile < OBJECT_TILE_BASE + OBJECT_COLUMNS * OBJECT_ROWS; tile++)
    {
        if (runtime_obstacle_usage[tile] > 0)
            printf("MAP OBSTACLE TILE: id=%d source=(%d,%d) cells=%d\n",
                   tile, (tile - OBJECT_TILE_BASE) % OBJECT_COLUMNS,
                   (tile - OBJECT_TILE_BASE) / OBJECT_COLUMNS,
                   runtime_obstacle_usage[tile]);
    }
    for (i = 0; i < map_object_count; i++)
    {
        tile = map_object_tile[i] - OBJECT_TILE_BASE;
        printf("MAP OBJECT: at=(%d,%d) source=(%d,%d) hp=%d removal=%d\n",
               map_object_x[i], map_object_y[i],
               tile % OBJECT_COLUMNS, tile / OBJECT_COLUMNS,
               map_object_hp[i], map_object_removal[i]);
    }
}

int map_tile_at(int row, int column)
{
    unsigned long hash;

    if (!map_infinite)
    {
        if (row < 0 || row >= map_rows || column < 0 || column >= map_columns)
            return 0;
        if (runtime_map_lazy && !runtime_row_loaded[row])
            load_map_band(row);
        return map_data[(unsigned long)row * (unsigned long)map_columns +
                        (unsigned long)column];
    }
    hash = map_hash(row, column);
    return map_terrain_choices[hash % (unsigned int)map_terrain_choice_count];
}

int map_coordinate(WorldCoord position)
{
    int coordinate;

    coordinate = (int)(position / MAP_TILE_PIXEL_SIZE);
    if (position < 0 && position % MAP_TILE_PIXEL_SIZE != 0)
        coordinate--;
    return coordinate;
}
#endif

int terrain_tile_at(int row, int column)
{
#ifdef RUNTIME_MAP_LOADING
    return map_tile_at(row, column);
#else
    if (row < 0 || row >= MAP_ROWS ||
        column < 0 || column >= MAP_COLUMNS)
        return 0;
    return map_data[row * MAP_COLUMNS + column];
#endif
}

int is_road_terrain(int tile)
{
    return (tile >= 0 && tile <= 3) || (tile >= 10 && tile <= 13) ||
           (tile >= 60 && tile <= 69) ||
           (tile >= 86 && tile <= 89);
}

int tank_terrain_speed_percent(WorldCoord x, WorldCoord y,
                               int *deep_water_samples)
{
    int offset_x;
    int offset_y;
    int row;
    int column;
    int tile;
    int speed_percent;
    int speed_percent_total;

    speed_percent_total = 0;
    *deep_water_samples = 0;
    for (offset_y = -12; offset_y <= 12; offset_y += 12)
    {
        for (offset_x = -12; offset_x <= 12; offset_x += 12)
        {
#ifdef RUNTIME_MAP_LOADING
            row = map_coordinate(y + offset_y);
            column = map_coordinate(x + offset_x);
#else
            row = (y + offset_y) / MAP_TILE_PIXEL_SIZE;
            column = (x + offset_x) / MAP_TILE_PIXEL_SIZE;
#endif
            tile = terrain_tile_at(row, column);
            if ((tile >= 40 && tile <= 59) ||
                tile == DEEP_WATER_TILE_ID)
            {
                speed_percent = 10;
                if (tile == DEEP_WATER_TILE_ID)
                    (*deep_water_samples)++;
            }
            else if (is_road_terrain(tile))
                speed_percent = 100;
            else
                speed_percent = 70;
            speed_percent_total += speed_percent;
        }
    }
    return (speed_percent_total + 4) / 9;
}

int map_obstacle_at(int row, int column)
{
#ifdef RUNTIME_MAP_LOADING
    unsigned long hash;

    if (map_infinite)
    {
        if (map_obstacle_density <= 0 ||
            map_infinite_obstacle_count == 0)
            return 0;
        hash = map_hash(row, column) ^ 0x9E3779B9UL;
        if (hash % (unsigned int)map_obstacle_density != 0)
            return 0;
        return map_infinite_obstacles[
            (hash >> 8) % (unsigned int)map_infinite_obstacle_count];
    }
    if (row < 0 || row >= map_rows ||
        column < 0 || column >= map_columns)
        return 0;
    if (runtime_map_lazy && !runtime_row_loaded[row])
        load_map_band(row);
    return map_obstacles[(unsigned long)row * (unsigned long)map_columns +
                         (unsigned long)column];
#else
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

#ifdef RUNTIME_MAP_LOADING
void draw_runtime_loading_screen(char *message, int current, int total)
{
    char percent[6];
    int percentage;
    int fill_width;
    int spinner;
    int dot_x;
    int dot_y;
    int dot;

    runtime_loading_stage = message;
    runtime_tile_progress_loaded = current;
    runtime_tile_progress_total = total;
    percentage = total > 0 ? current * 100 / total : 0;
    if (percentage < 0)
        percentage = 0;
    if (percentage > 100)
        percentage = 100;
    fill_width = percentage * 4;

    gfx_layer_select(LAYER_HUD);
    gfx_clear(BLACK);
    gfx_rect(72, 164, 496, 152, DARK_GRAY);
    gfx_rect(76, 168, 488, 144, BLACK);
    gfx_text(270, 188, "TANK-16", YELLOW);
    gfx_text(220, 226, message, WHITE);
    gfx_rect(120, 270, 400, 18, DARK_GRAY);
    if (fill_width > 0)
        gfx_rect(120, 270, fill_width, 18, GREEN);
    number_text((unsigned int)percentage, percent);
    dot = 0;
    while (percent[dot] != 0 && dot < 4)
        dot++;
    percent[dot++] = '%';
    percent[dot] = 0;
    gfx_text(308, 298, percent, YELLOW);

    spinner = runtime_loading_frame++ % 8;
    for (dot = 0; dot < 8; dot++)
    {
        if (dot == 0)
        {
            dot_x = 497;
            dot_y = 212;
        }
        else if (dot == 1)
        {
            dot_x = 513;
            dot_y = 220;
        }
        else if (dot == 2)
        {
            dot_x = 521;
            dot_y = 236;
        }
        else if (dot == 3)
        {
            dot_x = 513;
            dot_y = 252;
        }
        else if (dot == 4)
        {
            dot_x = 497;
            dot_y = 260;
        }
        else if (dot == 5)
        {
            dot_x = 481;
            dot_y = 252;
        }
        else if (dot == 6)
        {
            dot_x = 473;
            dot_y = 236;
        }
        else
        {
            dot_x = 481;
            dot_y = 220;
        }
        gfx_rect(dot_x, dot_y, 6, 6, dot == spinner ? YELLOW : GRAY);
    }
    gfx_vsync(0);
    gfx_present();
    gfx_vsync(1);
}
#endif

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

#ifdef RUNTIME_MAP_LOADING
int is_runtime_map_path(volatile char *path)
{
    int length;

    if (path[0] != 'm' || path[1] != 'a' || path[2] != 'p' ||
        path[3] != 's' || path[4] != '/')
        return 0;
    length = 0;
    while (length < 129 && path[length] != 0)
        length++;
    return length > 5 && path[length - 5] == '.' &&
           path[length - 4] == 'j' && path[length - 3] == 's' &&
           path[length - 2] == 'o' && path[length - 1] == 'n';
}

void discover_runtime_map_paths(void)
{
    int file_count;
    int file_index;
    int listed;
    int path_index;
    int path_length;

    round_map_path_count = 0;
    round_map_path_limit_reached = 0;
    file_count = disk_list_files(0, map_path_buffer,
                                 sizeof(map_path_buffer));
    if (file_count < 0)
    {
        printf("MAP LIST ERROR: %d\n", file_count);
        return;
    }
    for (file_index = 0; file_index < file_count; file_index++)
    {
        listed = disk_list_files(file_index, map_path_buffer,
                                 sizeof(map_path_buffer));
        if (listed < 0)
        {
            printf("MAP LIST ERROR: index=%d result=%d\n",
                   file_index, listed);
            round_map_path_count = 0;
            return;
        }
        if (!is_runtime_map_path(map_path_buffer))
            continue;
        if (round_map_path_count >= ROUND_MAP_PATH_CAPACITY)
        {
            round_map_path_limit_reached = 1;
            continue;
        }
        path_length = 0;
        while (path_length < 128 && map_path_buffer[path_length] != 0)
        {
            round_map_paths[round_map_path_count][path_length] =
                map_path_buffer[path_length];
            path_length++;
        }
        if (path_length >= 128)
        {
            round_map_paths[round_map_path_count][0] = 0;
            printf("MAP LIST ERROR: path exceeds 128 characters\n");
            continue;
        }
        round_map_paths[round_map_path_count][path_length] = 0;
        round_map_path_count++;
    }
    if (round_map_path_limit_reached)
        printf("MAP LIST: showing first %d maps\n",
               ROUND_MAP_PATH_CAPACITY);
}

void use_default_infinite_map(void)
{
    map_data = 0;
    map_obstacles = 0;
    runtime_map_lazy = 0;
    map_infinite = 1;
    map_columns = 0;
    map_rows = 0;
    map_seed = effect_random;
    map_terrain_choices[0] = 0;
    map_terrain_choice_count = 1;
    map_infinite_obstacle_count = 0;
    map_obstacle_density = 0;
    map_object_count = 0;
}

void select_runtime_round_map(void)
{
    int file_count;
    int map_count;
    int chosen_map;
    int map_index;
    int target_map;
    int attempt;
    int attempt_count;
    int explicit_choice;
    int i;

    map_data = 0;
    map_obstacles = 0;
    runtime_map_lazy = 0;
    map_object_count = 0;
    map_load_error = 0;
    map_selected_path[0] = 0;
    runtime_loading_active = 1;
    runtime_loading_stage = "FINDING MAP";
    draw_runtime_loading_screen("FINDING MAP", 0, 0);
    file_count = disk_list_files(0, map_path_buffer,
                                 sizeof(map_path_buffer));
    map_count = 0;
    if (file_count > 0)
    {
        for (i = 0; i < file_count; i++)
        {
            if (disk_list_files(i, map_path_buffer,
                                sizeof(map_path_buffer)) < 0 ||
                !is_runtime_map_path(map_path_buffer))
                continue;
            map_count++;
        }
    }
    if (map_count > 0)
    {
        explicit_choice = round_map_choice_ready;
        chosen_map = round_map_choice_ready
                         ? round_map_choice
                         : effect_noise(map_count);
        if (chosen_map < 0 || chosen_map >= map_count)
            chosen_map = effect_noise(map_count);
        round_map_choice_ready = 0;
        attempt_count = explicit_choice ? 1 : map_count;
        for (attempt = 0; attempt < attempt_count; attempt++)
        {
            target_map = (chosen_map + attempt) % map_count;
            map_index = 0;
            for (i = 0; i < file_count; i++)
            {
                if (disk_list_files(i, map_path_buffer,
                                    sizeof(map_path_buffer)) < 0 ||
                    !is_runtime_map_path(map_path_buffer))
                    continue;
                if (map_index++ == target_map)
                {
                    map_index = 0;
                    while (map_path_buffer[map_index] != 0 &&
                           map_index < 128)
                    {
                        map_selected_path[map_index] =
                            map_path_buffer[map_index];
                        map_index++;
                    }
                    map_selected_path[map_index] = 0;
                    break;
                }
            }
            if (map_selected_path[0] != 0)
            {
                runtime_loading_active = 1;
                runtime_loading_stage = "READING MAP";
                draw_runtime_loading_screen("READING MAP", 0, 0);
                if (load_map_path(map_selected_path))
                {
                    map_index = 0;
                    log_loaded_map(map_selected_path);
                    while (map_index < 128 &&
                           current_map_path[map_index] != 0 &&
                           current_map_path[map_index] ==
                               map_selected_path[map_index])
                    {
                        map_index++;
                    }
                    if (current_map_path[map_index] !=
                        map_selected_path[map_index])
                    {
                        reset_battlefield();
                        map_index = 0;
                        while (map_index < 128 &&
                               map_selected_path[map_index] != 0)
                        {
                            current_map_path[map_index] =
                                map_selected_path[map_index];
                            map_index++;
                        }
                        current_map_path[map_index] = 0;
                    }
                    round_map_selection_started++;
                    current_map_variant = round_map_selection_started - 1;
                    runtime_loading_active = 0;
                    return;
                }
            }
        }
    }
    reset_battlefield();
    current_map_path[0] = 0;
    use_default_infinite_map();
    configure_runtime_tilemap();
    map_load_error = 1;
    round_map_choice_ready = 0;
    round_map_selection_started++;
    current_map_variant = -1;
    runtime_loading_active = 0;
}
#endif

void select_round_map(void)
{
#ifndef RUNTIME_MAP_LOADING
    int previous_map_variant;

    previous_map_variant = current_map_variant;
#endif
#ifdef RUNTIME_MAP_LOADING
    select_runtime_round_map();
#else
#if MAP_VARIANT_COUNT > 1
    int variant;
    int i;

    if (round_map_selection_started == 0)
    {
        variant = 0;
        round_map_selection_started = 1;
    }
    else
    {
        variant = effect_noise(MAP_VARIANT_COUNT);
    }
    current_map_variant = variant;
    for (i = 0; i < MAP_CELLS; i++)
    {
        map_data[i] = map_variant_data[variant][i];
        map_obstacles[i] = map_variant_obstacles[variant][i];
    }
    map_object_count = map_variant_object_count[variant];
    for (i = 0; i < MAP_OBJECT_CAPACITY; i++)
    {
        map_object_x[i] = map_variant_object_x[variant][i];
        map_object_y[i] = map_variant_object_y[variant][i];
        map_object_tile[i] = map_variant_object_tile[variant][i];
        map_object_source_tile[i] =
            map_variant_object_source_tile[variant][i];
        map_object_hp[i] = map_variant_object_hp[variant][i];
        map_object_removal[i] = map_variant_object_removal[variant][i];
    }
    gfx_layer_select(LAYER_TILES);
    gfx_tilemap(map_data, MAP_COLUMNS, MAP_ROWS, 0, 0);
#else
    current_map_variant = 0;
    map_object_count = MAP_OBJECT_COUNT;
#endif
#endif
#ifndef RUNTIME_MAP_LOADING
    if (previous_map_variant >= 0 &&
        previous_map_variant != current_map_variant)
        reset_battlefield();
#endif
}

void store_burn_site(WorldCoord x, WorldCoord y, int mask, int player,
                     int kind)
{
    int i;
    int index;
    int matching_count;
    int replacement_index;
    int limit;
    BurnSite *site;

    limit = kind == BURN_KIND_TANK ?
        MAX_TANK_BURN_SITES_PER_PLAYER :
        MAX_AMMO_BURN_SITES_PER_PLAYER;
    matching_count = 0;
    replacement_index = -1;
    for (i = 0; i < MAX_BURN_SITES; i++)
    {
        index = next_burn_index + i;
        if (index >= MAX_BURN_SITES)
            index -= MAX_BURN_SITES;
        site = &burn_sites[index];
        if (site->remaining_seconds > 0 && burn_owner[index] == player &&
            burn_kind[index] == kind)
        {
            matching_count++;
            if (replacement_index < 0)
                replacement_index = index;
        }
    }
    if (matching_count >= limit)
    {
        index = replacement_index;
    }
    else
    {
        index = -1;
        for (i = 0; i < MAX_BURN_SITES; i++)
        {
            int candidate;

            candidate = next_burn_index + i;
            if (candidate >= MAX_BURN_SITES)
                candidate -= MAX_BURN_SITES;
            if (burn_sites[candidate].remaining_seconds == 0)
            {
                index = candidate;
                break;
            }
        }
    }
    if (index < 0)
    {
        burn_capacity_error = 1;
        return;
    }
    site = &burn_sites[index];
    if (site->remaining_seconds == 0)
    {
        burn_active_count++;
        if (index >= burn_count)
            burn_count = index + 1;
    }
#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
    {
        site->x = x;
        site->y = y;
    }
    else
#endif
    {
        site->x = x < 0 ? 0 : x;
        site->y = y < 0 ? 0 : y;
    }
#ifdef RUNTIME_MAP_LOADING
    if (!map_infinite &&
        site->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE)
#else
    if (site->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE)
#endif
        site->x = (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 1;
#ifdef RUNTIME_MAP_LOADING
    if (!map_infinite &&
        site->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE)
#else
    if (site->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE)
#endif
        site->y = (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE - 1;
    site->remaining_seconds = kind == BURN_KIND_TANK ?
        TANK_BURN_SECONDS : AMMO_BURN_SECONDS;
    site->mask = mask;
    burn_owner[index] = (unsigned char)player;
    burn_kind[index] = (unsigned char)kind;
    next_burn_index = index + 1;
    if (next_burn_index == MAX_BURN_SITES)
        next_burn_index = 0;
    next_burn_site = &burn_sites[next_burn_index];
}

void add_burn_site(WorldCoord x, WorldCoord y, int mask)
{
    store_burn_site(x, y, mask, 0, BURN_KIND_AMMO);
}

void add_ammo_burn_site(WorldCoord x, WorldCoord y, int player)
{
    store_burn_site(x, y, -1, player, BURN_KIND_AMMO);
}

void add_tank_burn_site(WorldCoord x, WorldCoord y, int mask, int player)
{
    store_burn_site(x, y, mask, player, BURN_KIND_TANK);
}

void age_burn_sites(unsigned int elapsed_seconds)
{
    int i;
    BurnSite *site;

    site = burn_sites;
    for (i = 0; i < burn_count; i++)
    {
        if (site->remaining_seconds > 0)
        {
            if ((unsigned int)site->remaining_seconds <= elapsed_seconds)
            {
                site->remaining_seconds = 0;
                burn_active_count--;
            }
            else
                site->remaining_seconds -= (int)elapsed_seconds;
        }
        site++;
    }
}

void update_burn_site_lifetimes(void)
{
    unsigned int now;
    unsigned int elapsed;

    if (burn_active_count == 0)
    {
        burn_clock_initialized = 0;
        return;
    }
    now = gfx_rtc(1) * 60 + gfx_rtc(0);
    if (!burn_clock_initialized)
    {
        burn_last_time = now;
        burn_clock_initialized = 1;
        return;
    }
    elapsed = (now + 3600 - burn_last_time) % 3600;
    if (elapsed > 0)
    {
        age_burn_sites(elapsed);
        burn_last_time = now;
    }
}

void reset_battlefield(void)
{
    int i;
    BurnSite *site;

    site = burn_sites;
    for (i = 0; i < burn_count; i++)
    {
        site->remaining_seconds = 0;
        site++;
    }
    burn_count = 0;
    burn_active_count = 0;
    burn_render_cursor = 0;
    next_burn_index = 0;
    next_burn_site = burn_sites;
    burn_capacity_error = 0;
    burn_clock_initialized = 0;
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
            add_tank_burn_site(tanks[player].x, tanks[player].y,
                               tanks[player].direction & TANK_DIRECTION_MASK,
                               player);
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
#ifndef RUNTIME_MAP_LOADING
        upload_terrain_tiles();
#endif
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

 #ifdef RUNTIME_MAP_LOADING
    left = map_coordinate(x);
    right = map_coordinate(x + size - 1);
    top = map_coordinate(y);
    bottom = map_coordinate(y + size - 1);
 #else
    left = x / MAP_TILE_PIXEL_SIZE;
    right = (x + size - 1) / MAP_TILE_PIXEL_SIZE;
    top = y / MAP_TILE_PIXEL_SIZE;
    bottom = (y + size - 1) / MAP_TILE_PIXEL_SIZE;
 #endif
#ifdef RUNTIME_MAP_LOADING
    if (!map_infinite)
    {
#endif
    if (left < 0)
        left = 0;
    if (top < 0)
        top = 0;
    if (right >= MAP_COLUMNS)
        right = MAP_COLUMNS - 1;
    if (bottom >= MAP_ROWS)
        bottom = MAP_ROWS - 1;
#ifdef RUNTIME_MAP_LOADING
    }
#endif
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

int is_solid_map_object(int object)
{
    int tile;

    if (object < 0 || object >= map_object_count)
        return 0;
    tile = map_object_source_tile[object];
    return tile >= 0 && (tile <= 8 || (tile >= 12 && tile <= 14));
}

void set_solid_map_object(int object, int active)
{
    int cell;

#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
        return;
    cell = map_object_y[object] * map_columns + map_object_x[object];
#else
    cell = map_object_y[object] * MAP_COLUMNS + map_object_x[object];
#endif
    if (cell < 0 || cell >= MAP_OBJECT_SOLID_CAPACITY)
        return;
    solid_map_objects[cell] = active
                                  ? (unsigned char)map_object_tile[object]
                                  : 0;
}

void rebuild_solid_map_objects(void)
{
    int i;
    int object;

    for (i = 0; i < MAP_OBJECT_SOLID_CAPACITY; i++)
        solid_map_objects[i] = 0;
    for (object = 0; object < map_object_count; object++)
    {
        if (object_active[object] && is_solid_map_object(object))
            set_solid_map_object(object, 1);
    }
}

int solid_map_object_collision(int mask, WorldCoord x, WorldCoord y, int size)
{
    int left;
    int right;
    int top;
    int bottom;
    int row;
    int column;
    int cell;
    int tile;
    int object;

#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
        return 0;
    left = map_coordinate(x);
    right = map_coordinate(x + size - 1);
    top = map_coordinate(y);
    bottom = map_coordinate(y + size - 1);
#else
    left = x / MAP_TILE_PIXEL_SIZE;
    right = (x + size - 1) / MAP_TILE_PIXEL_SIZE;
    top = y / MAP_TILE_PIXEL_SIZE;
    bottom = (y + size - 1) / MAP_TILE_PIXEL_SIZE;
#endif
    for (row = top; row <= bottom; row++)
    {
        for (column = left; column <= right; column++)
        {
#ifdef RUNTIME_MAP_LOADING
            if (row < 0 || row >= map_rows ||
                column < 0 || column >= map_columns)
                continue;
            cell = row * map_columns + column;
#else
            if (row < 0 || row >= MAP_ROWS ||
                column < 0 || column >= MAP_COLUMNS)
                continue;
            cell = row * MAP_COLUMNS + column;
#endif
            if (cell < 0)
                continue;
            tile = cell < MAP_OBJECT_SOLID_CAPACITY
                       ? solid_map_objects[cell] : 0;
            if (cell >= MAP_OBJECT_SOLID_CAPACITY)
            {
                for (object = 0; object < map_object_count; object++)
                {
                    if (object_active[object] &&
                        map_object_x[object] == column &&
                        map_object_y[object] == row &&
                        is_solid_map_object(object))
                    {
                        tile = map_object_tile[object];
                        break;
                    }
                }
            }
            if (tile != 0 &&
                mask_overlap_world(mask, x, y, GFX_MASK_TILE_BASE + tile,
                                   (WorldCoord)column * MAP_TILE_PIXEL_SIZE,
                                   (WorldCoord)row * MAP_TILE_PIXEL_SIZE))
                return 1;
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
#ifdef RUNTIME_MAP_LOADING
    if (!map_infinite &&
        (x < 16 || x > map_width - 16 ||
         y < 16 || y > map_height - 16))
#else
    if (x < 16 || x > map_width - 16 ||
        y < 16 || y > map_height - 16)
#endif
    {
        return 1;
    }
    prepare_collision_assets();
    if (obstacle_mask_collision(tank_mask(tank_index), x - 16, y - 16, 32))
    {
        return 1;
    }
    if (solid_map_object_collision(tank_mask(tank_index),
                                   x - 16, y - 16, 32))
        return 1;
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
    int radius_limit;

    select_round_map();
    for (i = 0; i < map_object_count; i++)
    {
        object_active[i] = 1;
        object_contacts[i] = 0;
        object_explosion[i] = 0;
    }
    rebuild_solid_map_objects();
    prepare_collision_assets();
    tanks[0].direction = 8;
    tanks[1].direction = 24;
    tanks[0].x = -1000;
    tanks[0].y = -1000;
    tanks[1].x = -1000;
    tanks[1].y = -1000;
    for (player = 0; player < 2; player++)
    {
#ifdef RUNTIME_MAP_LOADING
        if (map_infinite)
        {
            target_x = player == 0 ? -128 : 128;
            target_y = 0;
            radius_limit = 12;
        }
        else
        {
#endif
        target_x = player == 0
                       ? (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE / 4
                       : (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE * 3 / 4;
        target_y = (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE / 2;
#ifdef RUNTIME_MAP_LOADING
            radius_limit = MAP_COLUMNS + MAP_ROWS;
        }
#else
        radius_limit = MAP_COLUMNS + MAP_ROWS;
#endif
#ifdef RUNTIME_MAP_LOADING
        center_column = map_infinite
                            ? map_coordinate(target_x)
                            : (int)(target_x / MAP_TILE_PIXEL_SIZE);
        center_row = map_infinite
                         ? map_coordinate(target_y)
                         : (int)(target_y / MAP_TILE_PIXEL_SIZE);
#else
        center_column = target_x / MAP_TILE_PIXEL_SIZE;
        center_row = target_y / MAP_TILE_PIXEL_SIZE;
#endif
        best_x = target_x;
        best_y = target_y;
        best_score = 32767;
        found = 0;
        for (radius = 0; radius < radius_limit && !found; radius++)
        {
            first_row = center_row - radius;
#ifdef RUNTIME_MAP_LOADING
            if (!map_infinite)
            {
#endif
            if (first_row < 0)
                first_row = 0;
#ifdef RUNTIME_MAP_LOADING
            }
#endif
            last_row = center_row + radius;
#ifdef RUNTIME_MAP_LOADING
            if (!map_infinite)
            {
#endif
            if (last_row >= MAP_ROWS)
                last_row = MAP_ROWS - 1;
#ifdef RUNTIME_MAP_LOADING
            }
#endif
            first_column = center_column - radius;
#ifdef RUNTIME_MAP_LOADING
            if (!map_infinite)
            {
#endif
            if (first_column < 0)
                first_column = 0;
#ifdef RUNTIME_MAP_LOADING
            }
#endif
            last_column = center_column + radius;
#ifdef RUNTIME_MAP_LOADING
            if (!map_infinite)
            {
#endif
            if (last_column >= MAP_COLUMNS)
                last_column = MAP_COLUMNS - 1;
#ifdef RUNTIME_MAP_LOADING
            }
#endif
            for (row = first_row; row <= last_row; row++)
            {
                for (column = first_column; column <= last_column; column++)
                {
                    if (absolute_value(column - center_column) +
                            absolute_value(row - center_row) != radius)
                        continue;
#ifdef RUNTIME_MAP_LOADING
                    if (!map_infinite &&
                        ((player == 0 && column >= MAP_COLUMNS / 2) ||
                         (player == 1 && column < MAP_COLUMNS / 2)))
#else
                    if ((player == 0 && column >= MAP_COLUMNS / 2) ||
                        (player == 1 && column < MAP_COLUMNS / 2))
#endif
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
        tanks[i].speed_boost_frames = 0;
        tanks[i].damage_boost_frames = 0;
        deep_water_frames[i] = 0;
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
    powerup.life = 0;
    powerup_spawn_timer = POWERUP_SPAWN_DELAY;
    movement_sound_cooldown[0] = 0;
    movement_sound_cooldown[1] = 0;
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
            shot->damage = tanks[player].damage_boost_frames > 0 ? 20 : 10;
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
    int maximum_forward_speed;
    int maximum_reverse_speed;
    int terrain_max_forward_speed;
    int terrain_max_reverse_speed;
    int target_speed;
    int terrain_speed_percent;
    int deep_water_samples;
    int steering_speed;

    if (tanks[player].health <= 0)
        return;

    old_direction = tanks[player].direction;
    maximum_forward_speed = tanks[player].speed_boost_frames > 0
                                ? MAX_FORWARD_SPEED * 3 / 2
                                : MAX_FORWARD_SPEED;
    maximum_reverse_speed = tanks[player].speed_boost_frames > 0
                                ? MAX_REVERSE_SPEED * 3 / 2
                                : MAX_REVERSE_SPEED;
    terrain_speed_percent = tank_terrain_speed_percent(
        tanks[player].x, tanks[player].y, &deep_water_samples);
    terrain_max_forward_speed =
        maximum_forward_speed * terrain_speed_percent / 100;
    terrain_max_reverse_speed =
        maximum_reverse_speed * terrain_speed_percent / 100;
    if (terrain_max_forward_speed < 1)
        terrain_max_forward_speed = 1;
    if (terrain_max_reverse_speed < 1)
        terrain_max_reverse_speed = 1;
    if (deep_water_samples >= 5)
    {
        deep_water_frames[player]++;
        if (deep_water_frames[player] >= WATER_DAMAGE_INTERVAL)
        {
            deep_water_frames[player] = 0;
            change_health(player, -WATER_DAMAGE_AMOUNT);
            if (tanks[player].health <= 0)
                return;
        }
    }
    else
        deep_water_frames[player] = 0;
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
        if (forward_key && !backward_key &&
            tanks[player].throttle < maximum_forward_speed)
            tanks[player].throttle++;
        if (backward_key && !forward_key &&
            tanks[player].throttle > -maximum_reverse_speed)
            tanks[player].throttle--;
        if (tanks[player].throttle > maximum_forward_speed)
            tanks[player].throttle = maximum_forward_speed;
        if (tanks[player].throttle < -maximum_reverse_speed)
            tanks[player].throttle = -maximum_reverse_speed;
        if (tanks[player].speed > terrain_max_forward_speed)
            tanks[player].speed = terrain_max_forward_speed;
        if (tanks[player].speed < -terrain_max_reverse_speed)
            tanks[player].speed = -terrain_max_reverse_speed;
        target_speed = tanks[player].throttle;
        if (target_speed > terrain_max_forward_speed)
            target_speed = terrain_max_forward_speed;
        if (target_speed < -terrain_max_reverse_speed)
            target_speed = -terrain_max_reverse_speed;
        if (tanks[player].speed < target_speed)
            tanks[player].speed++;
        if (tanks[player].speed > target_speed)
            tanks[player].speed--;
    }
    turn = (right_key != 0) - (left_key != 0);
    if (turn != 0)
    {
        steering_speed = absolute_value(tanks[player].speed);
        if (steering_speed > MAX_FORWARD_SPEED)
            steering_speed = MAX_FORWARD_SPEED;
        turn_frames = STATIONARY_TURN_FRAMES +
                      steering_speed *
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
    if (tanks[player].speed > terrain_max_forward_speed)
        tanks[player].speed = terrain_max_forward_speed;
    if (tanks[player].speed < -terrain_max_reverse_speed)
        tanks[player].speed = -terrain_max_reverse_speed;
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
    if (movement_sound_cooldown[player] > 0)
        movement_sound_cooldown[player]--;
    if (absolute_value(tanks[player].speed) >= 4 &&
        movement_sound_cooldown[player] == 0)
    {
        gfx_sound(player, 72 + absolute_value(tanks[player].speed) * 3,
                  5, WAVE_TRIANGLE, 12);
        movement_sound_cooldown[player] = 12;
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

    for (i = 0; i < map_object_count; i++)
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
                        set_solid_map_object(i, 0);
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

    for (i = 0; i < map_object_count; i++)
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
#ifdef RUNTIME_MAP_LOADING
        if (!map_infinite &&
            (particle->x < 0 || particle->y < 0 ||
             particle->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE ||
             particle->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE))
#else
        if (particle->x < 0 || particle->y < 0 ||
            particle->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE ||
            particle->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE)
#endif
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
#ifdef RUNTIME_MAP_LOADING
    if ((!map_infinite &&
         (shot->x < 3 || shot->y < 3 ||
          shot->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 3 ||
          shot->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE - 3)) ||
        obstacle_mask_collision(GFX_MASK_AMMO, shot->x - 3, shot->y - 3, 7))
#else
    if (shot->x < 3 || shot->y < 3 ||
        shot->x >= (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE - 3 ||
        shot->y >= (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE - 3 ||
        obstacle_mask_collision(GFX_MASK_AMMO, shot->x - 3, shot->y - 3, 7))
#endif
        return 1;
    player = 1 - shot->owner;
    if (tanks[player].health > 0 &&
        gfx_mask_overlap_wide(GFX_MASK_AMMO, shot->x - 3, shot->y - 3,
                              tank_mask(player), tanks[player].x - 16,
                              tanks[player].y - 16))
    {
        tank_impulse(player, direction_offset(directions_x[shot->direction], 24),
                     direction_offset(directions_y[shot->direction], 24), 8);
        change_health(player, -(shot->damage > 0 ? shot->damage : 10));
        return 1;
    }
    for (i = 0; i < map_object_count; i++)
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
                set_solid_map_object(i, 0);
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
            add_ammo_burn_site(shots[i].x, shots[i].y, shot->owner);
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
                add_ammo_burn_site(shot->x, shot->y, shot->owner);
                shot->life = 0;
                break;
            }
        }
        if (shot->life > 0)
            shot->life--;
    }
}

int spawn_powerup(void)
{
    int attempt;
    int row;
    int column;
#ifdef RUNTIME_MAP_LOADING
    int center_row;
    int center_column;
#endif
    WorldCoord x;
    WorldCoord y;

#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
    {
        center_column = map_coordinate((tanks[0].x + tanks[1].x) / 2);
        center_row = map_coordinate((tanks[0].y + tanks[1].y) / 2);
    }
#endif
    for (attempt = 0; attempt < 48; attempt++)
    {
#ifdef RUNTIME_MAP_LOADING
        if (map_infinite)
        {
            column = center_column + effect_noise(41) - 20;
            row = center_row + effect_noise(41) - 20;
        }
        else
#endif
        {
#ifdef RUNTIME_MAP_LOADING
            column = effect_noise(map_columns);
            row = effect_noise(map_rows);
#else
            column = effect_noise(MAP_COLUMNS);
            row = effect_noise(MAP_ROWS);
#endif
        }
        x = (WorldCoord)column * MAP_TILE_PIXEL_SIZE +
            MAP_TILE_PIXEL_SIZE / 2;
        y = (WorldCoord)row * MAP_TILE_PIXEL_SIZE +
            MAP_TILE_PIXEL_SIZE / 2;
        if (blocked(x, y, 0) || blocked(x, y, 1))
            continue;
        powerup.x = x;
        powerup.y = y;
        powerup.type = effect_noise(3);
        powerup.life = POWERUP_LIFETIME;
        return 1;
    }
    return 0;
}

void update_powerups(void)
{
    int player;
    int maximum_forward_speed;
    int maximum_reverse_speed;

    for (player = 0; player < 2; player++)
    {
        if (tanks[player].speed_boost_frames > 0)
        {
            tanks[player].speed_boost_frames--;
            if (tanks[player].speed_boost_frames == 0)
            {
                maximum_forward_speed = MAX_FORWARD_SPEED;
                maximum_reverse_speed = MAX_REVERSE_SPEED;
                if (tanks[player].throttle > maximum_forward_speed)
                    tanks[player].throttle = maximum_forward_speed;
                if (tanks[player].throttle < -maximum_reverse_speed)
                    tanks[player].throttle = -maximum_reverse_speed;
                if (tanks[player].speed > maximum_forward_speed)
                    tanks[player].speed = maximum_forward_speed;
                if (tanks[player].speed < -maximum_reverse_speed)
                    tanks[player].speed = -maximum_reverse_speed;
            }
        }
        if (tanks[player].damage_boost_frames > 0)
            tanks[player].damage_boost_frames--;
    }
    if (powerup.life > 0)
    {
        for (player = 0; player < 2; player++)
        {
            if (tanks[player].health > 0 &&
                absolute_value(tanks[player].x - powerup.x) < 24 &&
                absolute_value(tanks[player].y - powerup.y) < 24)
            {
                if (powerup.type == POWERUP_HEALTH)
                    change_health(player, 25);
                else if (powerup.type == POWERUP_SPEED)
                    tanks[player].speed_boost_frames = POWERUP_DURATION;
                else
                    tanks[player].damage_boost_frames = POWERUP_DURATION;
                powerup.life = 0;
                powerup_spawn_timer = POWERUP_RESPAWN_DELAY;
                gfx_sound(3, 880, 4, WAVE_TRIANGLE, 30);
                return;
            }
        }
        powerup.life--;
        if (powerup.life == 0)
            powerup_spawn_timer = POWERUP_RESPAWN_DELAY;
        return;
    }
    if (powerup_spawn_timer > 0)
    {
        powerup_spawn_timer--;
        return;
    }
    if (!spawn_powerup())
        powerup_spawn_timer = 30;
}

#ifdef RUNTIME_MAP_LOADING
void begin_round_map_selection(void)
{
    discover_runtime_map_paths();
    round_map_cursor = 0;
    round_map_selection_active = 1;
    round_map_choice_ready = 0;
}

void confirm_round_map_selection(int choose_random)
{
    if (choose_random || round_map_path_count == 0)
    {
        round_map_choice_ready = 0;
    }
    else
    {
        round_map_choice = round_map_cursor;
        round_map_choice_ready = 1;
    }
    round_map_selection_active = 0;
    start_round();
}

void update_round_map_selection(unsigned int keys)
{
    unsigned int mouse_events;
    int mouse_x;
    int mouse_y;

    if ((keys & KEY_LEFT) && !(previous_keys & KEY_LEFT) &&
        round_map_path_count > 0)
    {
        round_map_cursor =
            (round_map_cursor + round_map_path_count - 1) %
            round_map_path_count;
    }
    if ((keys & KEY_RIGHT) && !(previous_keys & KEY_RIGHT) &&
        round_map_path_count > 0)
    {
        round_map_cursor = (round_map_cursor + 1) % round_map_path_count;
    }
    if ((keys & KEY_FIRE) && !(previous_keys & KEY_FIRE))
    {
        confirm_round_map_selection(0);
        return;
    }
    if ((keys & KEY_START) && !(previous_keys & KEY_START))
    {
        confirm_round_map_selection(1);
        return;
    }
    mouse_events = gfx_mouse_events();
    if (!(mouse_events & MOUSE_LEFT_DOWN))
        return;
    mouse_x = gfx_mouse_x();
    mouse_y = gfx_mouse_y();
    if (mouse_y >= 218 && mouse_y < 278)
    {
        if (mouse_x >= 96 && mouse_x < 176 && round_map_path_count > 0)
            round_map_cursor =
                (round_map_cursor + round_map_path_count - 1) %
                round_map_path_count;
        else if (mouse_x >= 464 && mouse_x < 544 &&
                 round_map_path_count > 0)
            round_map_cursor = (round_map_cursor + 1) % round_map_path_count;
    }
    else if (mouse_y >= 312 && mouse_y < 376)
    {
        if (mouse_x >= 128 && mouse_x < 304)
            confirm_round_map_selection(0);
        else if (mouse_x >= 336 && mouse_x < 512)
            confirm_round_map_selection(1);
    }
}
#endif

void update_game(unsigned int keys, unsigned int extra)
{
#ifdef RUNTIME_MAP_LOADING
    if (round_map_selection_active)
    {
        update_round_map_selection(keys);
        previous_keys = (int)keys;
        return;
    }
#endif
    update_burn_site_lifetimes();
    if ((keys & 32) && !(previous_keys & 32))
    {
        wins[0] = 0;
        wins[1] = 0;
        reset_battlefield();
        round_over = 0;
#ifdef RUNTIME_MAP_LOADING
        begin_round_map_selection();
#else
        start_round();
#endif
        previous_keys = (int)keys;
        return;
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
#ifdef RUNTIME_MAP_LOADING
            begin_round_map_selection();
#else
            start_round();
#endif
        }
        return;
    }
    update_powerups();
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
#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
        return tanks[player].x - 160 +
               (effect_tick % 2 == 0 ? tanks[player].shake / 3
                                     : -tanks[player].shake / 3);
#endif
    return camera_origin(tanks[player].x, 320,
                         (WorldCoord)MAP_COLUMNS * MAP_TILE_PIXEL_SIZE) +
           (effect_tick % 2 == 0 ? tanks[player].shake / 3 : -tanks[player].shake / 3);
}

WorldCoord tank_camera_y(int player)
{
#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
        return tanks[player].y - (TANK16_HUD_HEIGHT +
                                  (480 - TANK16_HUD_HEIGHT) / 2) +
               (effect_tick % 3 == 0 ? tanks[player].shake / 4
                                     : -tanks[player].shake / 4);
#endif
    {
        WorldCoord camera;

        camera = camera_origin(
                     tanks[player].y, 480 - TANK16_HUD_HEIGHT,
                     (WorldCoord)MAP_ROWS * MAP_TILE_PIXEL_SIZE) -
                 TANK16_HUD_HEIGHT;
        camera += effect_tick % 3 == 0 ? tanks[player].shake / 4
                                       : -tanks[player].shake / 4;
        return camera;
    }
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

 #ifdef RUNTIME_MAP_LOADING
    first_column = map_coordinate(camera_x);
    first_row = map_coordinate(camera_y);
    last_column = map_coordinate(camera_x + 319);
    last_row = map_coordinate(camera_y + 479);
 #else
    first_column = camera_x / MAP_TILE_PIXEL_SIZE;
    first_row = camera_y / MAP_TILE_PIXEL_SIZE;
    last_column = (camera_x + 319) / MAP_TILE_PIXEL_SIZE;
    last_row = (camera_y + 479) / MAP_TILE_PIXEL_SIZE;
 #endif
#ifdef RUNTIME_MAP_LOADING
    if (!map_infinite)
    {
#endif
    if (first_column < 0)
        first_column = 0;
    if (first_row < 0)
        first_row = 0;
    if (last_column >= MAP_COLUMNS)
        last_column = MAP_COLUMNS - 1;
    if (last_row >= MAP_ROWS)
        last_row = MAP_ROWS - 1;
#ifdef RUNTIME_MAP_LOADING
    }
#endif
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

#ifdef RUNTIME_MAP_LOADING
void draw_runtime_terrain(WorldCoord camera_x, WorldCoord camera_y)
{
    int first_column;
    int last_column;
    int first_row;
    int last_row;
    int row;
    int column;

    first_column = map_coordinate(camera_x);
    first_row = map_coordinate(camera_y);
    last_column = map_coordinate(camera_x + 319);
    last_row = map_coordinate(camera_y + 479);
    if (!map_infinite)
    {
        if (first_column < 0)
            first_column = 0;
        if (first_row < 0)
            first_row = 0;
        if (last_column >= MAP_COLUMNS)
            last_column = MAP_COLUMNS - 1;
        if (last_row >= MAP_ROWS)
            last_row = MAP_ROWS - 1;
    }
    for (row = first_row; row <= last_row; row++)
    {
        for (column = first_column; column <= last_column; column++)
        {
            world_tile(map_tile_at(row, column),
                       (WorldCoord)column * MAP_TILE_PIXEL_SIZE,
                       (WorldCoord)row * MAP_TILE_PIXEL_SIZE);
        }
    }
}
#endif

void draw_burn_sites(void)
{
    int batch_count;

    if (burn_active_count <= 0 || burn_count <= 0)
        return;
    if (burn_render_cursor >= burn_count)
        burn_render_cursor = 0;
    batch_count = burn_count - burn_render_cursor;
    if (batch_count > BURN_RENDER_BATCH)
        batch_count = BURN_RENDER_BATCH;
    world_emitters(&burn_sites[burn_render_cursor], batch_count, effect_tick);
    burn_render_cursor += batch_count;
    if (burn_render_cursor >= burn_count)
        burn_render_cursor = 0;
}

void draw_ground(void)
{
    int viewport;
    WorldCoord camera_x;
    WorldCoord camera_y;
    int i;
    int batch_count;
    int offset_x;
    int offset_y;

    gfx_layer_select(LAYER_TILES);
#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
    {
        gfx_clear(0);
        for (viewport = 0; viewport < 2; viewport++)
            draw_runtime_terrain(tank_camera_x(viewport),
                                 tank_camera_y(viewport));
    }
#endif
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
    for (i = 0; i < map_object_count; i++)
    {
        if (object_active[i])
        {
            world_tile(
                map_object_tile[i],
                (WorldCoord)map_object_x[i] * MAP_TILE_PIXEL_SIZE,
                (WorldCoord)map_object_y[i] * MAP_TILE_PIXEL_SIZE);
        }
    }
    if (powerup.life > 0)
    {
        if (powerup.type == POWERUP_HEALTH)
            i = POWERUP_HEALTH_TILE;
        else if (powerup.type == POWERUP_SPEED)
            i = POWERUP_SPEED_TILE;
        else
            i = POWERUP_DAMAGE_TILE;
        world_tile(i, powerup.x - MAP_TILE_PIXEL_SIZE / 2,
                   powerup.y - MAP_TILE_PIXEL_SIZE / 2);
    }
    draw_burn_sites();
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

int minimap_tile_color(int tile)
{
    if (tile >= 40 && tile <= 59)
        return BLUE;
    if (tile >= 60 && tile <= 69)
        return TRACK_COLOR;
    if (tile >= 70 && tile <= 89)
        return GRAY;
    return GREEN;
}

void draw_map_name(void)
{
#ifdef RUNTIME_MAP_LOADING
    char name[13];
    int path_index;
    int name_length;

    path_index = 0;
    name_length = 0;
    while (current_map_path[path_index] != 0)
    {
        if (current_map_path[path_index] == '/' ||
            current_map_path[path_index] == '\\')
            name_length = path_index + 1;
        path_index++;
    }
    if (path_index == 0)
        return;
    path_index = name_length;
    name_length = 0;
    while (current_map_path[path_index] != 0 &&
           current_map_path[path_index] != '.' && name_length < 12)
    {
        name[name_length++] = current_map_path[path_index++];
    }
    if (name_length == 0)
        return;
    name[name_length] = 0;
    gfx_text(301 - name_length * 3, 17, name, WHITE);
#endif
}

int minimap_coordinate(WorldCoord position)
{
#ifdef RUNTIME_MAP_LOADING
    return map_coordinate(position);
#else
    return position / MAP_TILE_PIXEL_SIZE;
#endif
}

void draw_minimap(void)
{
    int columns;
    int rows;
    int left;
    int top;
    int width;
    int height;
    int x;
    int y;
    int column;
    int row;
    int tile;
    int pixel_column;
    int pixel_row;
    int player;
    int player_column;
    int player_row;
    WorldCoord first_column;
    WorldCoord first_row;
    WorldCoord column_span;
    WorldCoord row_span;

#ifdef RUNTIME_MAP_LOADING
    if (map_infinite)
    {
        first_column = minimap_coordinate(tanks[0].x);
        first_row = minimap_coordinate(tanks[0].y);
        if (minimap_coordinate(tanks[1].x) < first_column)
            first_column = minimap_coordinate(tanks[1].x);
        if (minimap_coordinate(tanks[1].y) < first_row)
            first_row = minimap_coordinate(tanks[1].y);
        column_span = absolute_value(
            minimap_coordinate(tanks[0].x) -
            minimap_coordinate(tanks[1].x)) + 16;
        row_span = absolute_value(
            minimap_coordinate(tanks[0].y) -
            minimap_coordinate(tanks[1].y)) + 16;
        if (column_span < 32)
        {
            first_column -= (32 - column_span) / 2;
            column_span = 32;
        }
        if (row_span < 32)
        {
            first_row -= (32 - row_span) / 2;
            row_span = 32;
        }
        columns = (int)column_span;
        rows = (int)row_span;
        left = (int)first_column;
        top = (int)first_row;
    }
    else
    {
        columns = map_columns;
        rows = map_rows;
        left = 0;
        top = 0;
    }
#else
    columns = MAP_COLUMNS;
    rows = MAP_ROWS;
    left = 0;
    top = 0;
#endif

    gfx_rect(269, 31, 102, 28, DARK_GRAY);
    gfx_rect(271, 33, 98, 24, BLACK);
    if (columns <= 0 || rows <= 0)
        return;

    width = MINIMAP_MAX_WIDTH;
    height = MINIMAP_MAX_HEIGHT;
    if ((long)columns * MINIMAP_MAX_HEIGHT >
        (long)rows * MINIMAP_MAX_WIDTH)
        height = (int)((long)rows * MINIMAP_MAX_WIDTH / columns);
    else
        width = (int)((long)columns * MINIMAP_MAX_HEIGHT / rows);
    if (width < 1)
        width = 1;
    if (height < 1)
        height = 1;
    x = 320 - width / 2;
    y = 33 + (MINIMAP_MAX_HEIGHT - height) / 2;

    for (pixel_row = 0; pixel_row < height; pixel_row++)
    {
        row = top + (int)((long)pixel_row * rows / height);
#ifdef RUNTIME_MAP_LOADING
        if (!map_infinite && runtime_map_lazy &&
            (row < 0 || row >= map_rows || !runtime_row_loaded[row]))
            continue;
#endif
        for (pixel_column = 0; pixel_column < width; pixel_column++)
        {
            column = left + (int)((long)pixel_column * columns / width);
#ifdef RUNTIME_MAP_LOADING
            tile = map_tile_at(row, column);
#else
            tile = map_data[row * MAP_COLUMNS + column];
#endif
            gfx_rect(x + pixel_column, y + pixel_row, 1, 1,
                     minimap_tile_color(tile));
        }
    }

    for (player = 0; player < 2; player++)
    {
        player_column = (int)(
            ((long)minimap_coordinate(tanks[player].x) - left) *
            width / columns);
        player_row = (int)(
            ((long)minimap_coordinate(tanks[player].y) - top) *
            height / rows);
        if (player_column < 0)
            player_column = 0;
        if (player_column >= width)
            player_column = width - 1;
        if (player_row < 0)
            player_row = 0;
        if (player_row >= height)
            player_row = height - 1;
        gfx_rect(x + player_column - 1, y + player_row - 1, 3, 3,
                 player == 0 ? CYAN : RED);
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
    for (i = 0; i < map_object_count; i++)
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
    draw_map_name();
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
    draw_minimap();
    if (burn_capacity_error)
        gfx_text(240, 60, "FIRE CAPACITY REACHED", RED);
#ifdef RUNTIME_MAP_LOADING
    if (map_load_error)
        gfx_text(268, 60, "MAP LOAD FAILED", RED);
#endif
}

#ifdef RUNTIME_MAP_LOADING
void draw_round_map_selection(void)
{
    char map_name[25];
    int source_index;
    int name_length;
    int destination_index;
    int i;

    gfx_layer_select(LAYER_HUD);
    gfx_clear(15);
    gfx_rect(0, 0, 640, 480, BLACK);
    gfx_rect(80, 92, 480, 320, DARK_GRAY);
    gfx_rect(84, 96, 472, 312, BLACK);
    gfx_text(266, 128, "CHOOSE MAP", YELLOW);
    gfx_text(206, 164, "SELECT A MAP FOR THE NEXT ROUND", WHITE);

    gfx_rect(96, 218, 80, 60, DARK_GRAY);
    gfx_text(132, 243, "<", YELLOW);
    gfx_rect(184, 218, 272, 60, DARK_GRAY);
    gfx_rect(188, 222, 264, 52, BLACK);
    gfx_rect(464, 218, 80, 60, DARK_GRAY);
    gfx_text(500, 243, ">", YELLOW);
    if (round_map_path_count > 0)
    {
        source_index = 5;
        while (round_map_paths[round_map_cursor][source_index] != 0 &&
               round_map_paths[round_map_cursor][source_index] != '.' &&
               source_index < 127)
            source_index++;
        name_length = source_index - 5;
        if (name_length > 24)
            name_length = 24;
        for (i = 0; i < name_length; i++)
            map_name[i] = round_map_paths[round_map_cursor][i + 5];
        map_name[name_length] = 0;
        gfx_text(320 - name_length * 3, 244, map_name, WHITE);
    }
    else
        gfx_text(260, 244, "NO MAPS FOUND", RED);

    gfx_rect(128, 312, 176, 64, GREEN);
    gfx_rect(132, 316, 168, 56, DARK_GRAY);
    gfx_text(167, 337, "PLAY SELECTED", WHITE);
    gfx_rect(336, 312, 176, 64, ORANGE);
    gfx_rect(340, 316, 168, 56, DARK_GRAY);
    gfx_text(379, 337, "RANDOM MAP", YELLOW);
    gfx_text(155, 390, "LEFT/RIGHT CHOOSE   SPACE PLAY   ENTER RANDOM",
             WHITE);
    if (round_map_path_limit_reached)
        gfx_text(221, 401, "LIST LIMITED TO 64 MAPS", ORANGE);
}
#endif

void draw_game(void)
{
    WorldCoord camera1_x;
    WorldCoord camera1_y;
    WorldCoord camera2_x;
    WorldCoord camera2_y;
    int viewport;
    char countdown[6];

#ifdef RUNTIME_MAP_LOADING
    if (round_map_selection_active)
    {
        draw_round_map_selection();
        return;
    }
#endif
    camera1_x = tank_camera_x(0);
    camera1_y = tank_camera_y(0);
    camera2_x = tank_camera_x(1);
    camera2_y = tank_camera_y(1);
#ifdef RUNTIME_MAP_LOADING
    if (runtime_map_lazy)
    {
        ensure_map_rows(map_coordinate(camera1_y),
                        map_coordinate(camera1_y + 479));
        ensure_map_rows(map_coordinate(camera2_y),
                        map_coordinate(camera2_y + 479));
        sync_runtime_tilemap();
    }
#endif
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
#ifndef RUNTIME_MAP_LOADING
    for (i = 1; i < MAP_PALETTE_COUNT; i++)
    {
        gfx_palette((unsigned char)(MAP_PALETTE_BASE + i - 1),
                    map_palette[i][0],
                    map_palette[i][1], map_palette[i][2]);
    }
#endif
    load_tank_sprites();
    gfx_tile_size(MAP_TILE_PIXEL_SIZE);
#ifdef RUNTIME_MAP_LOADING
    runtime_loading_active = 1;
    runtime_loading_stage = "LOADING TILE DATA";
    runtime_tile_progress_loaded = 0;
    runtime_tile_progress_total = 0;
    draw_runtime_loading_screen("LOADING TILE DATA", 0, 0);
    if (!load_runtime_tile_data())
    {
        runtime_loading_active = 0;
        gfx_layer_select(LAYER_HUD);
        gfx_text(218, 230, "TILE DATA LOAD FAILED", RED);
        while (1)
            gfx_present();
    }
    runtime_loading_active = 0;
#else
    upload_terrain_tiles();
#endif
    collision_assets_ready = 1;
#ifndef RUNTIME_MAP_LOADING
    gfx_layer_select(LAYER_TILES);
    gfx_tilemap(map_data, MAP_COLUMNS, MAP_ROWS, 0, 0);
#endif
    wins[0] = 0;
    wins[1] = 0;
    previous_keys = 0;
#ifdef RUNTIME_MAP_LOADING
    effect_random = ((unsigned int)gfx_random() << 8) ^ gfx_random();
#endif
    tank_sprite_slots[0] = -1;
    tank_sprite_slots[1] = -1;
#ifdef RUNTIME_MAP_LOADING
    begin_round_map_selection();
#else
    start_round();
#endif
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
