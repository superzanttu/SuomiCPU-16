#include "game.h"
#include "../../lib/suomi_gfx.h"
#include <stdio.h>

#define BASE_X 24
#define BASE_Y 24
#define MINE_BUFFER_CAP 5
#define BUILDING_BUFFER_CAP 20
#define ROBOT_CAPACITY 5
#define ROBOT_INTERVAL 0.5f
#define TOOLBAR_Y MAP_VIEW_BOTTOM
#define TOOL_BUTTON_COUNT 5
#define TOOL_BUTTON_WIDTH 59
#define TOOL_BUTTON_GAP 3
#define MINIMAP_X 284
#define MINIMAP_Y 166
#define MINIMAP_SIZE 32
#define PAN_REPEAT_DELAY 0.15f

static int tile_size(GameState* state) {
    if (state->zoom == 0) return 8;
    if (state->zoom == 2) return 24;
    return TILE_PIXELS;
}

static void clamp_camera(GameState* state) {
    int max_camera_x;
    int max_camera_y;
    int size;

    size = tile_size(state);
    max_camera_x = state->map.width * size - SCREEN_W;
    max_camera_y = state->map.height * size -
        (MAP_VIEW_BOTTOM - MAP_VIEW_TOP);
    if (max_camera_x < 0) max_camera_x = 0;
    if (max_camera_y < 0) max_camera_y = 0;
    if (state->camera_x < 0) state->camera_x = 0;
    if (state->camera_y < 0) state->camera_y = 0;
    if (state->camera_x > max_camera_x) state->camera_x = max_camera_x;
    if (state->camera_y > max_camera_y) state->camera_y = max_camera_y;
}

static void zoom_at_mouse(GameState* state, int direction, int mouse_x, int mouse_y) {
    int old_size;
    int new_size;
    int focus_y;

    if (direction > 0 && state->zoom >= 2) return;
    if (direction < 0 && state->zoom <= 0) return;
    old_size = tile_size(state);
    state->zoom += direction;
    new_size = tile_size(state);
    focus_y = mouse_y - MAP_VIEW_TOP;
    state->camera_x =
        (state->camera_x + mouse_x) * new_size / old_size - mouse_x;
    state->camera_y =
        (state->camera_y + focus_y) * new_size / old_size - focus_y;
    clamp_camera(state);
    printf("Zoom level %d of 3\n", state->zoom + 1);
}

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
    if (type == BUILDING_ROUTE) return "Route";
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
    for (i = 0; i < state->robot_count; i++) {
        Robot* robot;
        robot = &state->robots[i];
        if (robot->carrying_type > RES_NONE &&
            robot->carrying_type <= RES_ROMBONENT)
            state->total_resources[robot->carrying_type] +=
                robot->carrying_amount;
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

static void assign_robot_route(GameState* state, int source_index,
                               int target_index) {
    int i;
    Robot* robot;
    Building* source;
    Building* target;

    if (source_index < 0 || source_index >= state->building_count ||
        target_index < 0 || target_index >= state->building_count) {
        printf("Route failed: select two buildings\n");
        return;
    }
    source = &state->buildings[source_index];
    target = &state->buildings[target_index];
    if (source_index == target_index || source->type == BUILDING_BASE ||
        target->type == BUILDING_MINE) {
        printf("Route failed: choose a resource source and a non-mine target\n");
        return;
    }
    if (source->type != BUILDING_MINE && source->inventory_count == 0) {
        printf("Route failed: source has no transportable resource\n");
        return;
    }
    robot = 0;
    for (i = 0; i < state->robot_count; i++) {
        if (state->robots[i].state == ROBOT_IDLE) {
            robot = &state->robots[i];
            break;
        }
    }
    if (robot == 0) {
        if (state->robot_count >= MAX_ROBOTS) {
            printf("Route failed: robot limit reached\n");
            return;
        }
        robot = &state->robots[state->robot_count++];
        robot->position = state->buildings[0].position;
    }
    robot->carrying_amount = 0;
    robot->carrying_type = RES_NONE;
    robot->source_building = source_index;
    robot->target_building = target_index;
    robot->state = ROBOT_TO_SOURCE;
    robot->timer = 0.0f;
    printf("Robot route: %s (%d,%d) -> %s (%d,%d)\n",
           building_name(source->type), source->position.x, source->position.y,
           building_name(target->type), target->position.x, target->position.y);
}

static void choose_route_building(GameState* state, int x, int y) {
    int selected;
    Building* source;

    selected = building_at(state, x, y);
    if (selected < 0) {
        printf("Route failed: click an existing building\n");
        return;
    }
    if (state->route_source < 0) {
        source = &state->buildings[selected];
        if (source->type == BUILDING_BASE ||
            (source->type != BUILDING_MINE && source->inventory_count == 0)) {
            printf("Route failed: click a mine or stocked building first\n");
            return;
        }
        state->route_source = selected;
        printf("Route source: %s at (%d,%d); now choose a target\n",
               building_name(source->type),
               source->position.x, source->position.y);
        return;
    }
    assign_robot_route(state, state->route_source, selected);
    state->route_source = -1;
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

static int robot_tile_blocked(GameState* state, Robot* robot, int x, int y,
                              int destination) {
    int building;
    int i;

    if (map_get_tile(&state->map, x, y) != TILE_GROUND) return 1;
    building = building_at(state, x, y);
    if (building >= 0 && building != destination) return 1;
    for (i = 0; i < state->robot_count; i++) {
        Robot* other;
        other = &state->robots[i];
        if (other == robot || other->state == ROBOT_IDLE) continue;
        if (other->position.x == x && other->position.y == y) return 1;
    }
    return 0;
}

static int robot_next_direction(GameState* state, Robot* robot,
                                int destination) {
    int* queue;
    unsigned char* visited;
    int start;
    int goal;
    int head;
    int tail;
    int entry;
    int index;
    int first;
    int x;
    int y;
    int next_x;
    int next_y;
    int next_index;
    int direction;
    int first_direction;
    int byte_index;
    int bit;
    int i;

    queue = state->path_queue;
    visited = state->path_visited;
    if (destination < 0 || destination >= state->building_count) return -1;
    start = robot->position.y * state->map.width + robot->position.x;
    goal = state->buildings[destination].position.y * state->map.width +
        state->buildings[destination].position.x;
    if (start == goal) return -1;
    for (i = 0; i < (MAP_SIZE * MAP_SIZE + 7) / 8; i++) visited[i] = 0;
    head = 0;
    tail = 0;
    queue[tail++] = start | (4 << 12);
    visited[start >> 3] = (unsigned char)(1 << (start & 7));
    while (head < tail) {
        entry = queue[head++];
        index = entry & 4095;
        first = entry >> 12;
        x = index % state->map.width;
        y = index / state->map.width;
        for (direction = 0; direction < 4; direction++) {
            next_x = x;
            next_y = y;
            if (direction == 0) next_x--;
            else if (direction == 1) next_x++;
            else if (direction == 2) next_y--;
            else next_y++;
            if (next_x < 0 || next_x >= state->map.width ||
                next_y < 0 || next_y >= state->map.height) continue;
            if (robot_tile_blocked(state, robot, next_x, next_y, destination))
                continue;
            next_index = next_y * state->map.width + next_x;
            byte_index = next_index >> 3;
            bit = 1 << (next_index & 7);
            if (visited[byte_index] & bit) continue;
            visited[byte_index] =
                (unsigned char)(visited[byte_index] | bit);
            first_direction = first == 4 ? direction : first;
            if (next_index == goal) return first_direction;
            queue[tail++] = next_index | (first_direction << 12);
        }
    }
    return -1;
}

static void update_robots(GameState* state, float delta_time) {
    int i;
    int direction;
    int destination;
    Robot* robot;
    Building* source;
    Building* target;

    for (i = 0; i < state->robot_count; i++) {
        robot = &state->robots[i];
        if (robot->state == ROBOT_IDLE) continue;
        if (robot->state == ROBOT_TO_SOURCE ||
            robot->state == ROBOT_TO_TARGET) {
            robot->timer += delta_time;
            if (robot->timer < ROBOT_INTERVAL) continue;
            robot->timer -= ROBOT_INTERVAL;
            if (robot->state == ROBOT_TO_SOURCE)
                destination = robot->source_building;
            else destination = robot->target_building;
            if (robot->position.x ==
                    state->buildings[destination].position.x &&
                robot->position.y ==
                    state->buildings[destination].position.y) {
                robot->state = robot->state == ROBOT_TO_SOURCE ?
                    ROBOT_LOADING : ROBOT_UNLOADING;
                robot->timer = 0.0f;
                continue;
            }
            direction = robot_next_direction(state, robot, destination);
            if (direction == 0) robot->position.x--;
            else if (direction == 1) robot->position.x++;
            else if (direction == 2) robot->position.y--;
            else if (direction == 3) robot->position.y++;
            if (direction < 0) robot->timer = 0.0f;
            continue;
        }

        source = &state->buildings[robot->source_building];
        target = &state->buildings[robot->target_building];
        if (robot->state == ROBOT_LOADING) {
            if (source->inventory_count <= 0) {
                if (robot->carrying_amount > 0) {
                    robot->state = ROBOT_TO_TARGET;
                    robot->timer = 0.0f;
                }
                continue;
            }
            robot->timer += delta_time;
            if (robot->timer < ROBOT_INTERVAL) continue;
            robot->timer -= ROBOT_INTERVAL;
            if (robot->carrying_amount == 0)
                robot->carrying_type = source->inventory_type;
            if (source->inventory_type != robot->carrying_type) {
                robot->state = ROBOT_TO_TARGET;
                robot->timer = 0.0f;
                continue;
            }
            source->inventory_count--;
            robot->carrying_amount++;
            if (source->inventory_count == 0)
                source->inventory_type = RES_NONE;
            if (robot->carrying_amount >= ROBOT_CAPACITY ||
                source->inventory_count == 0) {
                robot->state = ROBOT_TO_TARGET;
                robot->timer = 0.0f;
            }
        } else if (robot->state == ROBOT_UNLOADING) {
            if ((target->inventory_type != RES_NONE &&
                 target->inventory_type != robot->carrying_type) ||
                target->inventory_count >= BUILDING_BUFFER_CAP) continue;
            robot->timer += delta_time;
            if (robot->timer < ROBOT_INTERVAL) continue;
            robot->timer -= ROBOT_INTERVAL;
            target->inventory_type = robot->carrying_type;
            target->inventory_count++;
            robot->carrying_amount--;
            if (robot->carrying_amount <= 0) {
                robot->carrying_amount = 0;
                robot->carrying_type = RES_NONE;
                robot->state = ROBOT_TO_SOURCE;
                robot->timer = 0.0f;
            }
        }
    }
}

static void draw_resource_total(int x, char* label, int total) {
    char count[6];
    format_count(count, total);
    gfx_text(x, 3, label, WHITE);
    gfx_text(x, 19, count, YELLOW);
}

static void draw_minimap(GameState* state) {
    int x;
    int y;
    int size;
    int center_x;
    int center_y;
    int i;
    unsigned char color;

    gfx_rect(MINIMAP_X - 2, MINIMAP_Y - 2, MINIMAP_SIZE + 4,
             MINIMAP_SIZE + 4, GREEN);
    map_draw_minimap(&state->map, MINIMAP_X, MINIMAP_Y, MINIMAP_SIZE);
    for (i = 0; i < state->building_count; i++) {
        x = state->buildings[i].position.x * MINIMAP_SIZE / MAP_SIZE;
        y = state->buildings[i].position.y * MINIMAP_SIZE / MAP_SIZE;
        if (state->buildings[i].type == BUILDING_BASE) color = YELLOW;
        else color = WHITE;
        gfx_rect(MINIMAP_X + x, MINIMAP_Y + y, 2, 2, color);
    }
    for (i = 0; i < state->robot_count; i++) {
        if (state->robots[i].state == ROBOT_IDLE) continue;
        x = state->robots[i].position.x * MINIMAP_SIZE / MAP_SIZE;
        y = state->robots[i].position.y * MINIMAP_SIZE / MAP_SIZE;
        gfx_rect(MINIMAP_X + x, MINIMAP_Y + y, 2, 2, YELLOW);
    }
    size = tile_size(state);
    center_x = (state->camera_x + SCREEN_W / 2) / size *
        MINIMAP_SIZE / MAP_SIZE;
    center_y = (state->camera_y +
        (MAP_VIEW_BOTTOM - MAP_VIEW_TOP) / 2) / size *
        MINIMAP_SIZE / MAP_SIZE;
    gfx_rect(MINIMAP_X + center_x, MINIMAP_Y + center_y, 2, 2, WHITE);
}

void game_init(GameState* state) {
    Building* base;
    Robot* robot;
    int size;

    map_init(&state->map);
    state->zoom = 1;
    size = tile_size(state);
    state->camera_x = BASE_X * size - SCREEN_W / 2;
    state->camera_y = BASE_Y * size -
        (MAP_VIEW_BOTTOM - MAP_VIEW_TOP) / 2;
    state->build_tool = BUILDING_MINE;
    state->selected_building = 0;
    state->pan_keys_was_down = 0;
    state->pan_repeat_timer = 0.0f;
    state->tab_was_down = 0;
    state->mouse_last_x = 0;
    state->mouse_last_y = 0;
    state->right_dragging = 0;
    state->right_drag_moved = 0;
    state->route_source = -1;

    base = &state->buildings[0];
    base->type = BUILDING_BASE;
    base->position.x = BASE_X;
    base->position.y = BASE_Y;
    base->resource_type = RES_NONE;
    base->inventory_type = RES_NONE;
    base->inventory_count = 0;
    base->production_timer = 0.0f;
    state->building_count = 1;
    robot = &state->robots[0];
    robot->position = base->position;
    robot->carrying_amount = 0;
    robot->carrying_type = RES_NONE;
    robot->source_building = -1;
    robot->target_building = -1;
    robot->state = ROBOT_IDLE;
    robot->timer = 0.0f;
    state->robot_count = 1;
    update_resource_totals(state);
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
    int wheel;
    int size;
    unsigned int pan_keys;
    unsigned int newly_pressed_pan_keys;
    unsigned int mouse_buttons;
    int dx;
    int dy;
    int button;

    if (delta_time < 0.0f) delta_time = 0.0f;
    if (delta_time > 0.25f) delta_time = 0.25f;
    keys = gfx_keys();
    size = tile_size(state);
    pan_keys = keys & (KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN);
    newly_pressed_pan_keys = pan_keys & ~state->pan_keys_was_down;
    if (newly_pressed_pan_keys != 0) {
        if (newly_pressed_pan_keys & KEY_LEFT) state->camera_x -= size;
        if (newly_pressed_pan_keys & KEY_RIGHT) state->camera_x += size;
        if (newly_pressed_pan_keys & KEY_UP) state->camera_y -= size;
        if (newly_pressed_pan_keys & KEY_DOWN) state->camera_y += size;
        state->pan_repeat_timer = 0.0f;
    } else if (pan_keys != 0) {
        state->pan_repeat_timer += delta_time;
        while (state->pan_repeat_timer >= PAN_REPEAT_DELAY) {
            if (pan_keys & KEY_LEFT) state->camera_x -= size;
            if (pan_keys & KEY_RIGHT) state->camera_x += size;
            if (pan_keys & KEY_UP) state->camera_y -= size;
            if (pan_keys & KEY_DOWN) state->camera_y += size;
            state->pan_repeat_timer -= PAN_REPEAT_DELAY;
        }
    } else {
        state->pan_repeat_timer = 0.0f;
    }
    state->pan_keys_was_down = pan_keys;
    clamp_camera(state);

    if ((keys & KEY_TAB) && !state->tab_was_down) {
        if (state->build_tool == BUILDING_ROUTE)
            state->build_tool = BUILDING_MINE;
        else if (state->build_tool == BUILDING_MINE)
            state->build_tool = BUILDING_FACTORY;
        else if (state->build_tool == BUILDING_FACTORY)
            state->build_tool = BUILDING_STORAGE;
        else if (state->build_tool == BUILDING_STORAGE)
            state->build_tool = BUILDING_NONE;
        else state->build_tool = BUILDING_ROUTE;
        state->route_source = -1;
        printf("Build tool: %s\n", building_name(state->build_tool));
    }
    state->tab_was_down = (keys & KEY_TAB) != 0;

    mouse_events = gfx_mouse_events();
    mouse_x = gfx_mouse_x();
    mouse_y = gfx_mouse_y();
    wheel = gfx_mouse_wheel();
    if (wheel != 0 && mouse_y >= MAP_VIEW_TOP && mouse_y < MAP_VIEW_BOTTOM) {
        while (wheel > 0) {
            zoom_at_mouse(state, 1, mouse_x, mouse_y);
            wheel--;
        }
        while (wheel < 0) {
            zoom_at_mouse(state, -1, mouse_x, mouse_y);
            wheel++;
        }
    }
    mouse_buttons = gfx_mouse_buttons();
    if ((mouse_events & MOUSE_RIGHT_DOWN) &&
        mouse_y >= MAP_VIEW_TOP && mouse_y < MAP_VIEW_BOTTOM) {
        state->right_dragging = 1;
        state->right_drag_moved = 0;
        state->mouse_last_x = mouse_x;
        state->mouse_last_y = mouse_y;
    } else if (mouse_events & MOUSE_RIGHT_DOWN) {
        state->build_tool = BUILDING_NONE;
        state->selected_building = -1;
        state->route_source = -1;
        printf("Build cancelled\n");
    }
    if (mouse_buttons & MOUSE_RIGHT) {
        dx = mouse_x - state->mouse_last_x;
        dy = mouse_y - state->mouse_last_y;
        if (state->right_dragging && (dx != 0 || dy != 0)) {
            state->camera_x -= dx;
            state->camera_y -= dy;
            state->right_drag_moved = 1;
            clamp_camera(state);
        }
    }
    if (mouse_events & MOUSE_RIGHT_UP) {
        if (state->right_dragging && !state->right_drag_moved) {
            state->build_tool = BUILDING_NONE;
            state->selected_building = -1;
            state->route_source = -1;
            printf("Build cancelled\n");
        }
        state->right_dragging = 0;
    }
    if ((mouse_events & MOUSE_LEFT_DOWN) &&
        mouse_x >= MINIMAP_X && mouse_x < MINIMAP_X + MINIMAP_SIZE &&
        mouse_y >= MINIMAP_Y && mouse_y < MINIMAP_Y + MINIMAP_SIZE) {
        world_x = (mouse_x - MINIMAP_X) * state->map.width / MINIMAP_SIZE;
        world_y = (mouse_y - MINIMAP_Y) * state->map.height / MINIMAP_SIZE;
        size = tile_size(state);
        state->camera_x = world_x * size - SCREEN_W / 2;
        state->camera_y = world_y * size -
            (MAP_VIEW_BOTTOM - MAP_VIEW_TOP) / 2;
        clamp_camera(state);
    }
    if ((mouse_events & MOUSE_LEFT_DOWN) &&
        mouse_y >= TOOLBAR_Y && mouse_y < SCREEN_H) {
        button = (mouse_x - 4) / (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP);
        if (mouse_x >= 4 && button < TOOL_BUTTON_COUNT &&
            (mouse_x - 4) % (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP) <
                TOOL_BUTTON_WIDTH) {
            if (button == 0) state->build_tool = BUILDING_MINE;
            else if (button == 1) state->build_tool = BUILDING_FACTORY;
            else if (button == 2) state->build_tool = BUILDING_STORAGE;
            else if (button == 3) state->build_tool = BUILDING_NONE;
            else state->build_tool = BUILDING_ROUTE;
            state->selected_building = -1;
            state->route_source = -1;
            printf("Build tool: %s\n", building_name(state->build_tool));
        }
    }
    if ((mouse_events & MOUSE_LEFT_DOWN) &&
        mouse_y >= MAP_VIEW_TOP && mouse_y < TOOLBAR_Y &&
        !(mouse_x >= MINIMAP_X && mouse_x < MINIMAP_X + MINIMAP_SIZE &&
          mouse_y >= MINIMAP_Y && mouse_y < MINIMAP_Y + MINIMAP_SIZE)) {
        size = tile_size(state);
        world_x = mouse_x + state->camera_x;
        world_y = mouse_y - MAP_VIEW_TOP + state->camera_y;
        tile_x = world_x / size;
        tile_y = world_y / size;
        if (state->build_tool == BUILDING_ROUTE)
            choose_route_building(state, tile_x, tile_y);
        else place_building(state, tile_x, tile_y);
    }
    state->mouse_last_x = mouse_x;
    state->mouse_last_y = mouse_y;

    update_mines(state, delta_time);
    update_robots(state, delta_time);
    update_resource_totals(state);
}

void game_draw(GameState* state) {
    int screen_x;
    int screen_y;
    int size;
    int mouse_tile_x;
    int mouse_tile_y;
    int mouse_screen_x;
    int mouse_screen_y;
    int valid_placement;
    int i;
    unsigned char color;
    ResourceType resource;
    char zoom_text[2];

    gfx_clear(BLACK);

    size = tile_size(state);
    map_draw(&state->map, state->camera_x, state->camera_y, size,
             MAP_VIEW_TOP, MAP_VIEW_BOTTOM - MAP_VIEW_TOP);

    for (i = 0; i < state->building_count; i++) {
        Building* building;
        building = &state->buildings[i];
        screen_x = building->position.x * size - state->camera_x;
        screen_y = MAP_VIEW_TOP + building->position.y * size - state->camera_y;
        if (building->type == BUILDING_BASE) color = YELLOW;
        else if (building->type == BUILDING_MINE) color = RED;
        else if (building->type == BUILDING_FACTORY) color = BLUE;
        else color = CYAN;
        gfx_rect(screen_x + 1, screen_y + 1, size - 2, size - 2, color);
        if (building->type == BUILDING_MINE && size >= 8)
            gfx_rect(screen_x + size / 4, screen_y + size / 4, size / 2, size / 2,
                     resource_color(building->resource_type));
        if (i == state->selected_building) {
            gfx_rect(screen_x, screen_y, size, 1, WHITE);
            gfx_rect(screen_x, screen_y + size - 1, size, 1, WHITE);
            gfx_rect(screen_x, screen_y, 1, size, WHITE);
            gfx_rect(screen_x + size - 1, screen_y, 1, size, WHITE);
        }
    }
    for (i = 0; i < state->robot_count; i++) {
        Robot* robot;
        robot = &state->robots[i];
        if (robot->state == ROBOT_IDLE) continue;
        screen_x = robot->position.x * size - state->camera_x;
        screen_y = MAP_VIEW_TOP + robot->position.y * size - state->camera_y;
        color = robot->carrying_type == RES_NONE ?
            WHITE : resource_color(robot->carrying_type);
        gfx_rect(screen_x + size / 3, screen_y + size / 3,
                 size / 3, size / 3, color);
        gfx_rect(screen_x + size / 2, screen_y + size / 2, 2, 2, YELLOW);
    }

    draw_minimap(state);

    gfx_rect(0, 0, SCREEN_W, MAP_VIEW_TOP, BLACK);
    gfx_rect(0, 0, SCREEN_W, 1, CYAN);
    draw_resource_total(5, "Coal", state->total_resources[RES_COAL]);
    draw_resource_total(68, "Iron", state->total_resources[RES_IRON_ORE]);
    draw_resource_total(131, "Copper", state->total_resources[RES_COPPER_ORE]);
    draw_resource_total(194, "Fe plate", state->total_resources[RES_IRON_PLATE]);
    draw_resource_total(257, "Cu plate", state->total_resources[RES_COPPER_PLATE]);
    gfx_text(288, 29, "Z", CYAN);
    zoom_text[0] = (char)('1' + state->zoom);
    zoom_text[1] = '\0';
    gfx_text(298, 29, zoom_text, WHITE);
    if (state->selected_building >= 0 &&
        state->selected_building < state->building_count) {
        Building* selected_building;
        char count[6];
        selected_building = &state->buildings[state->selected_building];
        format_count(count, selected_building->inventory_count);
        if (selected_building->type == BUILDING_MINE) {
            gfx_text(8, TOOLBAR_Y + 5,
                     resource_name(selected_building->resource_type), WHITE);
            gfx_text(80, TOOLBAR_Y + 5, count, YELLOW);
            gfx_text(98, TOOLBAR_Y + 5, "/5", WHITE);
        } else {
            gfx_text(8, TOOLBAR_Y + 5,
                     building_name(selected_building->type), WHITE);
        }
    }
    {
        int active_robots;
        char robot_count[6];
        active_robots = 0;
        for (i = 0; i < state->robot_count; i++)
            if (state->robots[i].state != ROBOT_IDLE) active_robots++;
        gfx_text(145, TOOLBAR_Y + 5, "Bots", CYAN);
        format_count(robot_count, active_robots);
        gfx_text(178, TOOLBAR_Y + 5, robot_count, YELLOW);
        gfx_text(193, TOOLBAR_Y + 5, "/", WHITE);
        format_count(robot_count, state->robot_count);
        gfx_text(201, TOOLBAR_Y + 5, robot_count, WHITE);
        if (state->build_tool == BUILDING_ROUTE) {
            if (state->route_source < 0)
                gfx_text(222, TOOLBAR_Y + 5, "Route: source", GRAY);
            else
                gfx_text(222, TOOLBAR_Y + 5, "Route: target", GRAY);
        }
    }

    mouse_screen_x = gfx_mouse_x();
    mouse_screen_y = gfx_mouse_y();
    if (state->build_tool != BUILDING_NONE &&
        state->build_tool != BUILDING_ROUTE &&
        mouse_screen_y >= MAP_VIEW_TOP && mouse_screen_y < MAP_VIEW_BOTTOM) {
        mouse_tile_x = (mouse_screen_x + state->camera_x) / size;
        mouse_tile_y = (mouse_screen_y - MAP_VIEW_TOP + state->camera_y) / size;
        screen_x = mouse_tile_x * size - state->camera_x;
        screen_y = MAP_VIEW_TOP + mouse_tile_y * size - state->camera_y;
        resource = map_get_resource(&state->map, mouse_tile_x, mouse_tile_y);
        valid_placement =
            map_get_tile(&state->map, mouse_tile_x, mouse_tile_y) == TILE_GROUND &&
            building_at(state, mouse_tile_x, mouse_tile_y) < 0 &&
            (state->build_tool != BUILDING_MINE || resource != RES_NONE);
        color = valid_placement ? WHITE : RED;
        gfx_rect(screen_x, screen_y, size, 1, color);
        gfx_rect(screen_x, screen_y + size - 1, size, 1, color);
        gfx_rect(screen_x, screen_y, 1, size, color);
        gfx_rect(screen_x + size - 1, screen_y, 1, size, color);
    }

    gfx_rect(0, TOOLBAR_Y, SCREEN_W, SCREEN_H - TOOLBAR_Y, BLACK);
    gfx_rect(0, TOOLBAR_Y, SCREEN_W, 1, CYAN);
    gfx_text(140, TOOLBAR_Y + 5, "BUILD MODE", GRAY);
    for (i = 0; i < TOOL_BUTTON_COUNT; i++) {
        int button_x;
        BuildingType button_tool;
        char* button_label;
        button_x = i * (TOOL_BUTTON_WIDTH + TOOL_BUTTON_GAP) + 4;
        if (i == 0) button_tool = BUILDING_MINE;
        else if (i == 1) button_tool = BUILDING_FACTORY;
        else if (i == 2) button_tool = BUILDING_STORAGE;
        else if (i == 3) button_tool = BUILDING_NONE;
        else button_tool = BUILDING_ROUTE;
        button_label = building_name(button_tool);
        color = state->build_tool == button_tool ? CYAN : GRAY;
        gfx_rect(button_x, TOOLBAR_Y + 18, TOOL_BUTTON_WIDTH, 19, DARK_GRAY);
        gfx_rect(button_x, TOOLBAR_Y + 18, TOOL_BUTTON_WIDTH, 2, color);
        gfx_text(button_x + 5, TOOLBAR_Y + 25, button_label, WHITE);
    }
}
