#include "game.h"
#include "../../lib/suomi_gfx.h"
#include <stdio.h>
#include <string.h>

#define BASE_X 24
#define BASE_Y 24
#define MINE_BUFFER_CAP 5
#define TOOLBAR_Y MAP_VIEW_BOTTOM
#define TOOL_BUTTON_COUNT 4
#define TOOL_BUTTON_WIDTH 76
#define TOOL_BUTTON_GAP 3
#define PAN_REPEAT_DELAY 0.15f

static int building_at(GameState* state, int x, int y) {
    int i;
    for (i = 0; i < state->building_count; i++) {
        if (state->buildings[i].position.x == x &&
            state->buildings[i].position.y == y) return i;
    }
    return -1;
}

static char* building_name(BuildingType type) {
    if (type == BUILDING_MINE) return "Mine";
    if (type == BUILDING_FACTORY) return "Factory";
    if (type == BUILDING_STORAGE) return "Storage";
    if (type == BUILDING_BASE) return "Base";
    return "Select";
}

static char* resource_name(ResourceType type) {
    if (type == RES_COAL) return "Coal";
    if (type == RES_IRON_ORE) return "Iron ore";
    if (type == RES_COPPER_ORE) return "Copper ore";
    if (type == RES_IRON_PLATE) return "Iron plate";
    if (type == RES_COPPER_PLATE) return "Copper plate";
    if (type == RES_ROMBONENT) return "Rombonent";
    return "None";
}

static unsigned char resource_color(ResourceType type) {
    if (type == RES_COAL) return DARK_GRAY;
    if (type == RES_IRON_ORE) return ORANGE;
    if (type == RES_COPPER_ORE) return CYAN;
    return WHITE;
}

static void format_count(char* output, int value) {
    char digits[6];
    int count;
    int i;

    if (value < 0) value = 0;
    if (value > 99999) value = 99999;
    count = 0;
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value > 0 && count < 5);
    for (i = 0; i < count; i++) output[i] = digits[count - i - 1];
    output[count] = '\0';
}

static void update_resource_totals(GameState* state) {
    int i;
    int type;

    for (type = 0; type < 7; type++) state->total_resources[type] = 0;
    for (i = 0; i < state->building_count; i++) {
        Building* building;
        building = &state->buildings[i];
        if (building->inventory_type > RES_NONE &&
            building->inventory_type <= RES_ROMBONENT) {
            state->total_resources[building->inventory_type] +=
                building->inventory_count;
        }
    }
}

static void place_building(GameState* state, int x, int y) {
    Building* building;
    ResourceType resource;

    if (state->build_tool == BUILDING_NONE) {
        state->selected_building = building_at(state, x, y);
        if (state->selected_building >= 0)
            printf("Selected %s at (%d,%d)\n",
                   building_name(state->buildings[state->selected_building].type),
                   x, y);
        return;
    }
    if (x < 0 || x >= state->map.width || y < 0 || y >= state->map.height) {
        printf("Build failed: outside the map\n");
        return;
    }
    if (state->building_count >= MAX_BUILDINGS) {
        printf("Build failed: building limit reached\n");
        return;
    }
    if (building_at(state, x, y) >= 0) {
        state->selected_building = building_at(state, x, y);
        printf("Selected %s at (%d,%d)\n",
               building_name(state->buildings[state->selected_building].type),
               x, y);
        return;
    }
    if (map_get_tile(&state->map, x, y) != TILE_GROUND) {
        printf("Build failed: (%d,%d) is not ground\n", x, y);
        return;
    }
    resource = map_get_resource(&state->map, x, y);
    if (state->build_tool == BUILDING_MINE && resource == RES_NONE) {
        printf("Build failed: mine needs an ore deposit at (%d,%d)\n", x, y);
        return;
    }

    building = &state->buildings[state->building_count];
    building->type = state->build_tool;
    building->position.x = x;
    building->position.y = y;
    building->resource_type = state->build_tool == BUILDING_MINE ?
        resource : RES_NONE;
    building->inventory_count = 0;
    building->inventory_type = RES_NONE;
    building->production_timer = 0.0f;
    state->selected_building = state->building_count;
    state->building_count++;
    printf("Placed %s at (%d,%d)\n", building_name(building->type), x, y);
}

static void update_mines(GameState* state, float delta_time) {
    int i;
    for (i = 0; i < state->building_count; i++) {
        Building* building;
        building = &state->buildings[i];
        if (building->type != BUILDING_MINE) continue;

        if (building->inventory_count >= MINE_BUFFER_CAP) {
            building->production_timer = 0.0f;
            continue;
        }
        building->production_timer += delta_time;
        while (building->production_timer >= 1.0f &&
               building->inventory_count < MINE_BUFFER_CAP) {
            building->production_timer -= 1.0f;
            building->inventory_type = building->resource_type;
            building->inventory_count++;
        }
    }
}

static void draw_resource_total(int x, char* label, int total) {
    char count[6];
    format_count(count, total);
    gfx_text(x, 4, label, WHITE);
    gfx_text(x, 17, count, YELLOW);
}

void game_init(GameState* state) {
    Building* base;

    memset(state, 0, sizeof(GameState));
    map_init(&state->map);
    state->camera_x = BASE_X * TILE_PIXELS - SCREEN_W / 2;
    state->camera_y = BASE_Y * TILE_PIXELS - SCREEN_H / 2;
    state->build_tool = BUILDING_MINE;
    state->selected_building = 0;
    printf("MotoRobots ready. WASD pans; use the bottom buttons to choose a building.\n");

    base = &state->buildings[0];
    base->type = BUILDING_BASE;
    base->position.x = BASE_X;
    base->position.y = BASE_Y;
    base->resource_type = RES_NONE;
    base->inventory_type = RES_NONE;
    base->inventory_count = 0;
    base->production_timer = 0.0f;
    state->building_count = 1;
}

void game_update(GameState* state, float delta_time) {
    unsigned int keys;
    unsigned int mouse_events;
    int mouse_x;
    int mouse_y;
    int world_x;
    int world_y;
    int tile_x;
    int tile_y;
    unsigned int pan_keys;
    unsigned int newly_pressed_pan_keys;
    int max_camera_x;
    int max_camera_y;

    if (delta_time < 0.0f) delta_time = 0.0f;
    if (delta_time > 0.25f) delta_time = 0.25f;
    keys = gfx_keys();
    pan_keys = keys & (KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN);
    newly_pressed_pan_keys = pan_keys & ~state->pan_keys_was_down;
    if (newly_pressed_pan_keys != 0) {
        if (newly_pressed_pan_keys & KEY_LEFT) state->camera_x -= TILE_PIXELS;
        if (newly_pressed_pan_keys & KEY_RIGHT) state->camera_x += TILE_PIXELS;
        if (newly_pressed_pan_keys & KEY_UP) state->camera_y -= TILE_PIXELS;
        if (newly_pressed_pan_keys & KEY_DOWN) state->camera_y += TILE_PIXELS;
        state->pan_repeat_timer = 0.0f;
    } else if (pan_keys != 0) {
        state->pan_repeat_timer += delta_time;
        while (state->pan_repeat_timer >= PAN_REPEAT_DELAY) {
            if (pan_keys & KEY_LEFT) state->camera_x -= TILE_PIXELS;
            if (pan_keys & KEY_RIGHT) state->camera_x += TILE_PIXELS;
            if (pan_keys & KEY_UP) state->camera_y -= TILE_PIXELS;
            if (pan_keys & KEY_DOWN) state->camera_y += TILE_PIXELS;
            state->pan_repeat_timer -= PAN_REPEAT_DELAY;
        }
    } else {
        state->pan_repeat_timer = 0.0f;
    }
    state->pan_keys_was_down = pan_keys;

    max_camera_x = state->map.width * TILE_PIXELS - SCREEN_W;
    max_camera_y = state->map.height * TILE_PIXELS - SCREEN_H;
    if (state->camera_x < 0) state->camera_x = 0;
    if (state->camera_y < 0) state->camera_y = 0;
    if (state->camera_x > max_camera_x) state->camera_x = max_camera_x;
    if (state->camera_y > max_camera_y) state->camera_y = max_camera_y;

    if ((keys & KEY_TAB) && !state->tab_was_down) {
        if (state->build_tool == BUILDING_NONE) state->build_tool = BUILDING_MINE;
        else if (state->build_tool == BUILDING_MINE)
            state->build_tool = BUILDING_FACTORY;
        else if (state->build_tool == BUILDING_FACTORY)
            state->build_tool = BUILDING_STORAGE;
        else state->build_tool = BUILDING_NONE;
        printf("Build tool: %s\n", building_name(state->build_tool));
    }
    state->tab_was_down = (keys & KEY_TAB) != 0;

    mouse_events = gfx_mouse_events();
    mouse_x = gfx_mouse_x();
    mouse_y = gfx_mouse_y();
    if (mouse_events & MOUSE_RIGHT_DOWN) {
        state->build_tool = BUILDING_NONE;
        state->selected_building = -1;
        printf("Build cancelled\n");
    }
    if ((mouse_events & MOUSE_LEFT_DOWN) &&
        mouse_y >= TOOLBAR_Y && mouse_y < SCREEN_H) {
        int button;
        button = (mouse_x - 4) / (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP);
        if (mouse_x >= 4 && button < TOOL_BUTTON_COUNT &&
            (mouse_x - 4) % (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP) <
                TOOL_BUTTON_WIDTH) {
            if (button == 0) state->build_tool = BUILDING_MINE;
            else if (button == 1) state->build_tool = BUILDING_FACTORY;
            else if (button == 2) state->build_tool = BUILDING_STORAGE;
            else state->build_tool = BUILDING_NONE;
            state->selected_building = -1;
            printf("Build tool: %s\n", building_name(state->build_tool));
        }
    }
    if ((mouse_events & MOUSE_LEFT_DOWN) &&
        mouse_y >= MAP_VIEW_TOP && mouse_y < TOOLBAR_Y) {
        world_x = mouse_x + state->camera_x;
        world_y = mouse_y + state->camera_y;
        tile_x = world_x / TILE_PIXELS;
        tile_y = world_y / TILE_PIXELS;
        place_building(state, tile_x, tile_y);
    }

    update_mines(state, delta_time);
    update_resource_totals(state);
}

void game_draw(GameState* state) {
    int first_x;
    int first_y;
    int offset_x;
    int offset_y;
    int columns;
    int rows;
    int x;
    int y;
    int map_x;
    int map_y;
    int screen_x;
    int screen_y;
    int mouse_tile_x;
    int mouse_tile_y;
    int mouse_screen_x;
    int mouse_screen_y;
    int valid_placement;
    int i;
    unsigned char color;
    TileType tile;
    ResourceType resource;

    gfx_clear(BLACK);

    first_x = state->camera_x / TILE_PIXELS;
    first_y = state->camera_y / TILE_PIXELS;
    offset_x = state->camera_x % TILE_PIXELS;
    offset_y = state->camera_y % TILE_PIXELS;
    columns = SCREEN_W / TILE_PIXELS + 2;
    rows = SCREEN_H / TILE_PIXELS + 2;
    for (x = 0; x < columns; x++) {
        for (y = 0; y < rows; y++) {
            map_x = first_x + x;
            map_y = first_y + y;
            screen_x = x * TILE_PIXELS - offset_x;
            screen_y = y * TILE_PIXELS - offset_y;
            tile = map_get_tile(&state->map, map_x, map_y);
            if (tile == TILE_WATER) color = BLUE;
            else if (tile == TILE_ROCK) color = DARK_GRAY;
            else color = GREEN;
            gfx_rect(screen_x, screen_y, TILE_PIXELS - 1, TILE_PIXELS - 1, color);
            resource = map_get_resource(&state->map, map_x, map_y);
            if (resource != RES_NONE)
                gfx_rect(screen_x + 5, screen_y + 5, 6, 6,
                         resource_color(resource));
        }
    }

    for (i = 0; i < state->building_count; i++) {
        Building* building;
        building = &state->buildings[i];
        screen_x = building->position.x * TILE_PIXELS - state->camera_x;
        screen_y = building->position.y * TILE_PIXELS - state->camera_y;
        if (building->type == BUILDING_BASE) color = YELLOW;
        else if (building->type == BUILDING_MINE) color = RED;
        else if (building->type == BUILDING_FACTORY) color = BLUE;
        else color = CYAN;
        gfx_rect(screen_x + 2, screen_y + 2, 12, 12, color);
        if (building->type == BUILDING_MINE)
            gfx_rect(screen_x + 5, screen_y + 5, 6, 6,
                     resource_color(building->resource_type));
        if (i == state->selected_building) {
            gfx_rect(screen_x + 1, screen_y + 1, 14, 1, WHITE);
            gfx_rect(screen_x + 1, screen_y + 14, 14, 1, WHITE);
            gfx_rect(screen_x + 1, screen_y + 1, 1, 14, WHITE);
            gfx_rect(screen_x + 14, screen_y + 1, 1, 14, WHITE);
        }
    }

    gfx_rect(0, 0, SCREEN_W, MAP_VIEW_TOP, DARK_GRAY);
    gfx_rect(0, MAP_VIEW_BOTTOM, SCREEN_W, SCREEN_H - MAP_VIEW_BOTTOM, DARK_GRAY);
    draw_resource_total(5, "Coal", state->total_resources[RES_COAL]);
    draw_resource_total(54, "Iron", state->total_resources[RES_IRON_ORE]);
    draw_resource_total(103, "Copper", state->total_resources[RES_COPPER_ORE]);
    draw_resource_total(166, "Iron plate", state->total_resources[RES_IRON_PLATE]);
    draw_resource_total(244, "Cu plate", state->total_resources[RES_COPPER_PLATE]);
    if (state->selected_building >= 0 &&
        state->selected_building < state->building_count) {
        Building* selected_building;
        char count[6];
        selected_building = &state->buildings[state->selected_building];
        format_count(count, selected_building->inventory_count);
        gfx_rect(75, MAP_VIEW_TOP + 4, 170, 14, BLACK);
        if (selected_building->type == BUILDING_MINE) {
            gfx_text(80, MAP_VIEW_TOP + 7,
                     resource_name(selected_building->resource_type), WHITE);
            gfx_text(142, MAP_VIEW_TOP + 7, count, YELLOW);
            gfx_text(160, MAP_VIEW_TOP + 7, "/5", WHITE);
        } else {
            gfx_text(80, MAP_VIEW_TOP + 7,
                     building_name(selected_building->type), WHITE);
        }
    }

    mouse_screen_x = gfx_mouse_x();
    mouse_screen_y = gfx_mouse_y();
    if (state->build_tool != BUILDING_NONE &&
        mouse_screen_y >= MAP_VIEW_TOP && mouse_screen_y < MAP_VIEW_BOTTOM) {
        mouse_tile_x = (mouse_screen_x + state->camera_x) / TILE_PIXELS;
        mouse_tile_y = (mouse_screen_y + state->camera_y) / TILE_PIXELS;
        screen_x = mouse_tile_x * TILE_PIXELS - state->camera_x;
        screen_y = mouse_tile_y * TILE_PIXELS - state->camera_y;
        resource = map_get_resource(&state->map, mouse_tile_x, mouse_tile_y);
        valid_placement =
            map_get_tile(&state->map, mouse_tile_x, mouse_tile_y) == TILE_GROUND &&
            building_at(state, mouse_tile_x, mouse_tile_y) < 0 &&
            (state->build_tool != BUILDING_MINE || resource != RES_NONE);
        color = valid_placement ? WHITE : RED;
        gfx_rect(screen_x, screen_y, TILE_PIXELS, 1, color);
        gfx_rect(screen_x, screen_y + TILE_PIXELS - 1, TILE_PIXELS, 1, color);
        gfx_rect(screen_x, screen_y, 1, TILE_PIXELS, color);
        gfx_rect(screen_x + TILE_PIXELS - 1, screen_y, 1, TILE_PIXELS, color);
    }

    gfx_rect(0, TOOLBAR_Y, SCREEN_W, SCREEN_H - TOOLBAR_Y, DARK_GRAY);
    for (i = 0; i < TOOL_BUTTON_COUNT; i++) {
        int button_x;
        BuildingType button_tool;
        char* button_label;
        button_x = i * (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP) + 4;
        if (i == 0) button_tool = BUILDING_MINE;
        else if (i == 1) button_tool = BUILDING_FACTORY;
        else if (i == 2) button_tool = BUILDING_STORAGE;
        else button_tool = BUILDING_NONE;
        button_label = building_name(button_tool);
        color = state->build_tool == button_tool ? YELLOW : GRAY;
        gfx_rect(button_x, TOOLBAR_Y + 2, TOOL_BUTTON_WIDTH, 20, color);
        gfx_text(button_x + 5, TOOLBAR_Y + 8, button_label, BLACK);
    }
}
