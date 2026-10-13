// Hertta-3D: find Hertta in a maze and win her over with dog treats.
// Controls: W/S move, A/D turn, Space throws a treat, Enter starts/continues.
#include "suomi_gfx.h"

#define MW 10
#define MH 10
#define RAYS 24
#define TREATS 2
#define MAX_VERTICES 128
#define MAX_FACES 128
#define SCENE_WORDS (GPU3D_HEADER_WORDS + MAX_VERTICES * GPU3D_VERTEX_WORDS + MAX_FACES * GPU3D_FACE_WORDS)
#define ST_MENU 0
#define ST_PLAY 1
#define ST_HAPPY 2
#define ST_DONE 3
#define FIX 256
#define MAP_X 252
#define MAP_Y 25
#define MAP_CELL 6

int sintab[64] = {
    0, 25, 50, 74, 98, 121, 142, 162, 181, 198, 213, 226, 237, 245, 251, 255,
    256, 255, 251, 245, 237, 226, 213, 198, 181, 162, 142, 121, 98, 74, 50, 25,
    0, -25, -50, -74, -98, -121, -142, -162, -181, -198, -213, -226, -237,
    -245, -251, -255, -256, -255, -251, -245, -237, -226, -213, -198, -181,
    -162, -142, -121, -98, -74, -50, -25};

unsigned char maze[MW * MH];
int scene[SCENE_WORDS];
int ray_depths[RAYS];
int ray_cells[RAYS];
int move_dx[4] = {0, 1, 0, -1};
int move_dy[4] = {-1, 0, 1, 0};
int path_queue[MW * MH];
int path_toward[MW * MH];
int vertex_count;
int face_count;
int state;
int level;
int treats_eaten;
int treats_needed;
int player_x;
int player_y;
int player_angle;
int dog_x;
int dog_y;
int dog_angle;
int dog_turn;
int throw_cooldown;
int tick;
int treat_x[TREATS];
int treat_y[TREATS];
int treat_age[TREATS];
unsigned int keys;

int sine(int angle)
{
    return sintab[(angle & 255) >> 2];
}

int cosine(int angle)
{
    return sine(angle + 64);
}

int fixed_mul(int value, int factor)
{
    int whole;
    int fraction;
    int half_fraction;
    int odd_fraction;
    whole = value >> 8;
    fraction = value - whole * FIX;
    half_fraction = fraction >> 1;
    odd_fraction = fraction & 1;
    return whole * factor + ((half_fraction * factor) >> 7) +
           ((odd_fraction * factor) >> 8);
}

int screen_offset(int pixels, int depth)
{
    int whole;
    int remainder;
    whole = depth / 120;
    remainder = depth % 120;
    return pixels * whole + (pixels * remainder) / 120;
}

int absval(int value)
{
    return value < 0 ? -value : value;
}

void number_text(int value, char *out)
{
    char digits[6];
    int count;
    int i;
    count = 0;
    if (value == 0)
        digits[count++] = '0';
    while (value > 0 && count < 5)
    {
        digits[count++] = '0' + value % 10;
        value /= 10;
    }
    for (i = 0; i < count; i++)
        out[i] = digits[count - i - 1];
    out[count] = 0;
}

int wall_at(int x, int y)
{
    int tx;
    int ty;
    if (x < 0 || y < 0 || x >= MW * FIX || y >= MH * FIX)
        return 1;
    tx = x >> 8;
    ty = y >> 8;
    return maze[ty * MW + tx];
}

void make_maze(void)
{
    int stack_x[16];
    int stack_y[16];
    int choices_x[4];
    int choices_y[4];
    int stack_count;
    int choice_count;
    int x;
    int y;
    int next_x;
    int next_y;
    int i;
    int choice;
    for (y = 0; y < MH; y++)
    {
        for (x = 0; x < MW; x++)
        {
            maze[y * MW + x] = 1;
        }
    }
    stack_count = 1;
    stack_x[0] = 1;
    stack_y[0] = 1;
    maze[MW + 1] = 0;
    while (stack_count > 0)
    {
        x = stack_x[stack_count - 1];
        y = stack_y[stack_count - 1];
        choice_count = 0;
        for (i = 0; i < 4; i++)
        {
            if (i == 0) { next_x = x; next_y = y - 2; }
            else if (i == 1) { next_x = x + 2; next_y = y; }
            else if (i == 2) { next_x = x; next_y = y + 2; }
            else { next_x = x - 2; next_y = y; }
            if (next_x > 0 && next_x < MW - 1 &&
                next_y > 0 && next_y < MH - 1 &&
                maze[next_y * MW + next_x])
            {
                choices_x[choice_count] = next_x;
                choices_y[choice_count] = next_y;
                choice_count++;
            }
        }
        if (choice_count == 0)
        {
            stack_count--;
        }
        else
        {
            choice = gfx_random() % choice_count;
            next_x = choices_x[choice];
            next_y = choices_y[choice];
            maze[((y + next_y) / 2) * MW + ((x + next_x) / 2)] = 0;
            maze[next_y * MW + next_x] = 0;
            stack_x[stack_count] = next_x;
            stack_y[stack_count] = next_y;
            stack_count++;
        }
    }
}

void reset_level(void)
{
    int i;
    make_maze();
    player_x = FIX + FIX / 2;
    player_y = FIX + FIX / 2;
    player_angle = 0;
    if (wall_at(player_x, player_y - FIX))
    {
        player_angle = 64;
        if (wall_at(player_x + FIX, player_y))
        {
            player_angle = 128;
            if (wall_at(player_x, player_y + FIX))
                player_angle = 192;
        }
    }
    dog_x = 7 * FIX + FIX / 2;
    dog_y = 7 * FIX + FIX / 2;
    dog_angle = 0;
    dog_turn = 0;
    treats_eaten = 0;
    treats_needed = level + 2;
    throw_cooldown = 0;
    tick = 0;
    for (i = 0; i < TREATS; i++)
        treat_age[i] = 0;
    state = ST_PLAY;
}

int scene_vertex(int x, int y, int z)
{
    int offset;
    int index;
    if (vertex_count >= MAX_VERTICES)
        return 0;
    index = vertex_count++;
    offset = GPU3D_HEADER_WORDS + index * GPU3D_VERTEX_WORDS;
    scene[offset] = x;
    scene[offset + 1] = y;
    scene[offset + 2] = z;
    return index;
}

void scene_face(int a, int b, int c, int color)
{
    int offset;
    if (face_count >= MAX_FACES)
        return;
    offset = GPU3D_HEADER_WORDS + MAX_VERTICES * GPU3D_VERTEX_WORDS +
             face_count * GPU3D_FACE_WORDS;
    scene[offset] = a;
    scene[offset + 1] = b;
    scene[offset + 2] = c;
    scene[offset + 3] = color;
    face_count++;
}

void compact_faces(void)
{
    int source;
    int target;
    int i;
    source = GPU3D_HEADER_WORDS + MAX_VERTICES * GPU3D_VERTEX_WORDS;
    target = GPU3D_HEADER_WORDS + vertex_count * GPU3D_VERTEX_WORDS;
    for (i = 0; i < face_count * GPU3D_FACE_WORDS; i++)
        scene[target + i] = scene[source + i];
}

void scene_quad(int x0, int x1, int half_height, int depth, int color)
{
    int left;
    int right;
    int top;
    int bottom;
    int a;
    int b;
    int c;
    int d;
    if (depth < 32 || vertex_count + 4 > MAX_VERTICES || face_count + 2 > MAX_FACES)
        return;
    left = screen_offset(x0, depth);
    right = screen_offset(x1, depth);
    top = half_height;
    bottom = -top;
    a = scene_vertex(left, top, depth);
    b = scene_vertex(right, top, depth);
    c = scene_vertex(right, bottom, depth);
    d = scene_vertex(left, bottom, depth);
    scene_face(a, b, c, color);
    scene_face(a, c, d, color);
}

void add_billboard(int x, int y, int depth, int half_width, int half_height, int color)
{
    int a;
    int b;
    int c;
    int d;
    if (depth < 32 || vertex_count + 4 > MAX_VERTICES || face_count + 2 > MAX_FACES)
        return;
    a = scene_vertex(x - half_width, y + half_height, depth);
    b = scene_vertex(x + half_width, y + half_height, depth);
    c = scene_vertex(x + half_width, y - half_height, depth);
    d = scene_vertex(x - half_width, y - half_height, depth);
    scene_face(a, b, c, color);
    scene_face(a, c, d, color);
}

void build_view(void)
{
    int ray;
    int angle;
    int relative;
    int dist;
    int wx;
    int wy;
    int depth;
    int dx;
    int dy;
    int dog_local_x;
    int dog_depth;
    int treat_local_x;
    int treat_depth;
    int color;
    int left;
    int right;
    int hit_x;
    int hit_y;
    int group_start;
    int depth_sum;
    int group_count;
    int i;
    vertex_count = 0;
    face_count = 0;
    for (ray = 0; ray < RAYS; ray++)
    {
        relative = -32 + (ray * 64) / (RAYS - 1);
        angle = player_angle + relative;
        dist = 24;
        while (dist <= 1536)
        {
            wx = player_x + fixed_mul(dist, sine(angle));
            wy = player_y - fixed_mul(dist, cosine(angle));
            if (wall_at(wx, wy))
                break;
            dist += 24;
        }
        if (dist > 1536)
            dist = 1536;
        hit_x = wx >> 8;
        hit_y = wy >> 8;
        if (hit_x < 0) hit_x = 0;
        if (hit_x >= MW) hit_x = MW - 1;
        if (hit_y < 0) hit_y = 0;
        if (hit_y >= MH) hit_y = MH - 1;
        depth = fixed_mul(dist, cosine(relative));
        if (depth < 32)
            depth = 32;
        ray_depths[ray] = depth;
        ray_cells[ray] = hit_y * MW + hit_x;
    }

    ray = 0;
    while (ray < RAYS)
    {
        group_start = ray;
        depth_sum = ray_depths[ray];
        group_count = 1;
        while (ray + 1 < RAYS && ray_cells[ray + 1] == ray_cells[group_start])
        {
            ray++;
            depth_sum += ray_depths[ray];
            group_count++;
        }
        depth = depth_sum / group_count;
        left = -160 + (group_start * 320) / RAYS;
        right = -160 + ((ray + 1) * 320) / RAYS;
        color = depth < 192 ? 14 : depth < 320 ? 8 : 9;
        scene_quad(left, right, FIX, depth, color);
        ray++;
    }

    dx = dog_x - player_x;
    dy = dog_y - player_y;
    dog_local_x = fixed_mul(dx, cosine(player_angle)) +
                  fixed_mul(dy, sine(player_angle));
    dog_depth = fixed_mul(dx, sine(player_angle)) -
                fixed_mul(dy, cosine(player_angle));
    if (dog_depth > 0)
    {
        add_billboard(dog_local_x, FIX / 8, dog_depth, FIX / 4, FIX / 3, ORANGE);
        add_billboard(dog_local_x, FIX / 2, dog_depth, FIX / 5, FIX / 5, YELLOW);
        add_billboard(dog_local_x - FIX / 5, FIX * 3 / 4, dog_depth, FIX / 12, FIX / 8, DARK_RED);
        add_billboard(dog_local_x + FIX / 5, FIX * 3 / 4, dog_depth, FIX / 12, FIX / 8, DARK_RED);
    }

    for (i = 0; i < TREATS; i++)
    {
        if (treat_age[i] > 0)
        {
            dx = treat_x[i] - player_x;
            dy = treat_y[i] - player_y;
            treat_local_x = fixed_mul(dx, cosine(player_angle)) +
                            fixed_mul(dy, sine(player_angle));
            treat_depth = fixed_mul(dx, sine(player_angle)) -
                          fixed_mul(dy, cosine(player_angle));
            if (treat_depth > 0)
                add_billboard(treat_local_x, -FIX / 3, treat_depth, FIX / 12, FIX / 12, WHITE);
        }
    }

    scene[0] = vertex_count;
    scene[1] = face_count;
    scene[2] = 0;
    scene[3] = 0;
    scene[4] = 0;
    scene[5] = 0;
    scene[6] = 0;
    scene[7] = 0;
    scene[8] = 120 * FIX;
    scene[9] = GPU3D_FILL;
    compact_faces();
}

void draw_wall_edges(void)
{
    int ray;
    int left;
    int right;
    int group_start;
    int depth_sum;
    int group_count;
    int depth;
    int half_height;
    int top;
    int bottom;
    ray = 0;
    while (ray < RAYS)
    {
        group_start = ray;
        depth_sum = ray_depths[ray];
        group_count = 1;
        while (ray + 1 < RAYS && ray_cells[ray + 1] == ray_cells[group_start])
        {
            ray++;
            depth_sum += ray_depths[ray];
            group_count++;
        }
        left = (group_start * 320) / RAYS;
        right = ((ray + 1) * 320) / RAYS;
        depth = depth_sum / group_count;
        half_height = (120 * FIX) / depth;
        top = 120 - half_height;
        bottom = 120 + half_height;
        gfx_line(left, top, right, top, DARK_GRAY);
        gfx_line(left, bottom, right, bottom, DARK_GRAY);
        gfx_line(left, top, left, bottom, DARK_GRAY);
        ray++;
    }
    gfx_line(right, top, right, bottom, DARK_GRAY);
}

void draw_minimap(void)
{
    int x;
    int y;
    int px;
    int py;
    int player_cell_x;
    int player_cell_y;
    int dog_cell_x;
    int dog_cell_y;
    gfx_rect(MAP_X - 4, MAP_Y - 4, MW * MAP_CELL + 8,
             MH * MAP_CELL + 8, GRAY);
    gfx_rect(MAP_X - 2, MAP_Y - 2, MW * MAP_CELL + 4,
             MH * MAP_CELL + 4, BLACK);
    for (y = 0; y < MH; y++)
    {
        for (x = 0; x < MW; x++)
        {
            px = MAP_X + x * MAP_CELL;
            py = MAP_Y + y * MAP_CELL;
            gfx_rect(px, py, MAP_CELL, MAP_CELL,
                     maze[y * MW + x] ? GRAY : BLACK);
            if (maze[y * MW + x])
            {
                if (y == 0 || !maze[(y - 1) * MW + x])
                    gfx_line(px, py, px + MAP_CELL - 1, py, DARK_GRAY);
                if (x == 0 || !maze[y * MW + x - 1])
                    gfx_line(px, py, px, py + MAP_CELL - 1, DARK_GRAY);
                if (y == MH - 1 || !maze[(y + 1) * MW + x])
                    gfx_line(px, py + MAP_CELL - 1,
                             px + MAP_CELL - 1, py + MAP_CELL - 1, DARK_GRAY);
                if (x == MW - 1 || !maze[y * MW + x + 1])
                    gfx_line(px + MAP_CELL - 1, py,
                             px + MAP_CELL - 1, py + MAP_CELL - 1, DARK_GRAY);
            }
        }
    }
    player_cell_x = (player_x >> 8) * MAP_CELL + MAP_X + 1;
    player_cell_y = (player_y >> 8) * MAP_CELL + MAP_Y + 1;
    dog_cell_x = (dog_x >> 8) * MAP_CELL + MAP_X + 1;
    dog_cell_y = (dog_y >> 8) * MAP_CELL + MAP_Y + 1;
    gfx_rect(player_cell_x, player_cell_y, MAP_CELL - 2, MAP_CELL - 2, GREEN);
    gfx_rect(dog_cell_x, dog_cell_y, MAP_CELL - 2, MAP_CELL - 2, ORANGE);
}

void throw_treat(void)
{
    int i;
    for (i = 0; i < TREATS; i++)
    {
        if (treat_age[i] == 0)
        {
            treat_x[i] = player_x + (sine(player_angle) * 64 >> 8);
            treat_y[i] = player_y - (cosine(player_angle) * 64 >> 8);
            treat_age[i] = 1;
            throw_cooldown = 14;
            gfx_sound(0, 760, 2, WAVE_TRIANGLE, 40);
            return;
        }
    }
}

void update_treats(void)
{
    int i;
    int nx;
    int ny;
    int dx;
    int dy;
    for (i = 0; i < TREATS; i++)
    {
        if (treat_age[i] > 0)
        {
            nx = treat_x[i] + (sine(player_angle) * 38 >> 8);
            ny = treat_y[i] - (cosine(player_angle) * 38 >> 8);
            treat_age[i]++;
            if (wall_at(nx, ny) || treat_age[i] > 34)
            {
                treat_age[i] = 0;
            }
            else
            {
                treat_x[i] = nx;
                treat_y[i] = ny;
                dx = absval(treat_x[i] - dog_x);
                dy = absval(treat_y[i] - dog_y);
                if (dx < 80 && dy < 80)
                {
                    treat_age[i] = 0;
                    treats_eaten++;
                    gfx_sound(1, 520, 5, WAVE_TRIANGLE, 45);
                    if (treats_eaten >= treats_needed)
                        state = ST_HAPPY;
                }
            }
        }
    }
}

int path_direction(int from_x, int from_y, int to_x, int to_y)
{
    int first;
    int last;
    int current;
    int next;
    int x;
    int y;
    int direction;
    int i;
    first = 0;
    last = 0;
    for (i = 0; i < MW * MH; i++)
        path_toward[i] = -1;
    current = (to_y >> 8) * MW + (to_x >> 8);
    next = (from_y >> 8) * MW + (from_x >> 8);
    if (current == next)
        return -1;
    path_queue[last++] = current;
    path_toward[current] = -2;
    while (first < last)
    {
        current = path_queue[first++];
        if (current == next)
            return path_toward[current];
        for (direction = 0; direction < 4; direction++)
        {
            x = current % MW + move_dx[direction];
            y = current / MW + move_dy[direction];
            if (x >= 0 && x < MW && y >= 0 && y < MH)
            {
                i = y * MW + x;
                if (!maze[i] && path_toward[i] == -1)
                {
                    path_toward[i] = (direction + 2) & 3;
                    path_queue[last++] = i;
                }
            }
        }
    }
    return -1;
}

void update_dog(void)
{
    int speed;
    int nx;
    int ny;
    int dx;
    int dy;
    int direction;
    int candidate_count;
    int candidates[4];
    int reverse;
    int player_cell;
    int dog_cell;
    int i;
    if (state == ST_HAPPY)
    {
        dx = player_x - dog_x;
        dy = player_y - dog_y;
        if (absval(dx) < 96 && absval(dy) < 96)
        {
            state = ST_DONE;
            return;
        }
        player_cell = (player_y >> 8) * MW + (player_x >> 8);
        dog_cell = (dog_y >> 8) * MW + (dog_x >> 8);
        if (player_cell == dog_cell)
        {
            if (absval(dx) > absval(dy))
                direction = dx > 0 ? 1 : 3;
            else
                direction = dy > 0 ? 2 : 0;
        }
        else
            direction = path_direction(dog_x, dog_y, player_x, player_y);
        speed = 16 + level;
        if (speed > 28)
            speed = 28;
        if (direction >= 0)
        {
            dog_angle = direction * 64;
            nx = dog_x + (sine(dog_angle) * speed >> 8);
            ny = dog_y - (cosine(dog_angle) * speed >> 8);
            if (!wall_at(nx, ny))
            {
                dog_x = nx;
                dog_y = ny;
            }
        }
        return;
    }

    if (dog_turn > 0)
        dog_turn--;
    if (dog_turn == 0)
    {
        candidate_count = 0;
        reverse = ((dog_angle + 128) & 255) >> 6;
        for (i = 0; i < 4; i++)
        {
            nx = dog_x + move_dx[i] * FIX;
            ny = dog_y + move_dy[i] * FIX;
            if (!wall_at(nx, ny))
                candidates[candidate_count++] = i;
        }
        if (candidate_count > 1)
        {
            for (i = 0; i < candidate_count; i++)
            {
                if (candidates[i] == reverse)
                {
                    candidates[i] = candidates[candidate_count - 1];
                    candidate_count--;
                    break;
                }
            }
        }
        if (candidate_count == 0)
            return;
        dog_angle = candidates[gfx_random() % candidate_count] * 64;
        dog_turn = 8 + (gfx_random() & 15);
    }
    speed = 12 + level;
    if (speed > 24)
        speed = 24;
    nx = dog_x + (sine(dog_angle) * speed >> 8);
    ny = dog_y - (cosine(dog_angle) * speed >> 8);
    if (wall_at(nx, ny))
    {
        dog_angle = (gfx_random() & 3) * 64;
        dog_turn = 0;
    }
    else
    {
        dog_x = nx;
        dog_y = ny;
    }
}

void update_game(void)
{
    int speed;
    int nx;
    int ny;
    tick++;
    keys = gfx_keys();
    if (throw_cooldown > 0)
        throw_cooldown--;
    if (keys & KEY_LEFT)
        player_angle = (player_angle - 3) & 255;
    if (keys & KEY_RIGHT)
        player_angle = (player_angle + 3) & 255;
    speed = (keys & KEY_FIRE) ? -12 : 12;
    if (keys & (KEY_UP | KEY_DOWN))
    {
        if (keys & KEY_DOWN)
            speed = -speed;
        nx = player_x + (sine(player_angle) * speed >> 8);
        ny = player_y - (cosine(player_angle) * speed >> 8);
        if (!wall_at(nx, player_y))
            player_x = nx;
        if (!wall_at(player_x, ny))
            player_y = ny;
    }
    if ((keys & KEY_FIRE) && throw_cooldown == 0)
        throw_treat();
    update_treats();
    update_dog();
}

void draw_hud(void)
{
    char number[6];
    char count[13];
    int i;
    gfx_rect(0, 0, 320, 18, BLACK);
    gfx_text(5, 5, "HERTTA-3D", YELLOW);
    gfx_text(104, 5, "LEVEL", WHITE);
    number_text(level, number);
    gfx_text(142, 5, number, CYAN);
    gfx_text(182, 5, "TREATS", WHITE);
    number_text(treats_eaten, number);
    for (i = 0; number[i]; i++)
        count[i] = number[i];
    count[i++] = '/';
    number_text(treats_needed, number);
    {
        int j;
        for (j = 0; number[j]; j++)
            count[i++] = number[j];
    }
    count[i] = 0;
    gfx_text(230, 5, count, ORANGE);
}

void draw_status(void)
{
    if (state == ST_MENU)
    {
        gfx_rect(30, 54, 260, 116, BLACK);
        gfx_text(82, 66, "HERTTA-3D", YELLOW);
        gfx_text(45, 88, "FIND HERTTA IN THE MAZE", WHITE);
        gfx_text(43, 104, "W/S MOVE   A/D TURN", CYAN);
        gfx_text(43, 120, "SPACE THROWS DOG TREATS", ORANGE);
        gfx_text(65, 146, "PRESS ENTER TO START", GREEN);
    }
    else if (state == ST_HAPPY)
    {
        gfx_rect(58, 86, 204, 54, BLACK);
        gfx_text(90, 96, "HERTTA IS COMING!", YELLOW);
        gfx_text(66, 114, "WAIT FOR HER TO REACH YOU", WHITE);
    }
    else if (state == ST_DONE)
    {
        gfx_rect(58, 86, 204, 54, BLACK);
        gfx_text(82, 96, "LEVEL COMPLETE!", GREEN);
        gfx_text(66, 114, "PRESS ENTER FOR NEXT MAZE", WHITE);
    }
}

void render_game(void)
{
    int status;
    gfx_clear(BLACK);
    gfx_rect(0, 18, SCREEN_W, 90, BLUE);
    gfx_rect(0, 108, SCREEN_W, 132, 10);
    build_view();
    status = gfx3d_draw(scene);
    draw_wall_edges();
    if (status != GPU3D_OK)
    {
        gfx_text(8, 210, "SC-GPU SCENE ERROR", RED);
    }
    gfx_rect(158, 119, 5, 1, WHITE);
    gfx_rect(160, 117, 1, 5, WHITE);
    draw_hud();
    draw_minimap();
    draw_status();
    gfx_present();
}

int main(void)
{
    gfx_speed(SPEED_MAX);
    level = 1;
    state = ST_MENU;
    for (;;)
    {
        if (state == ST_MENU)
        {
            if (gfx_keys() & KEY_START)
                reset_level();
        }
        else if (state == ST_PLAY || state == ST_HAPPY)
        {
            update_game();
        }
        else if (state == ST_DONE && (gfx_keys() & KEY_START))
        {
            level++;
            reset_level();
        }
        render_game();
    }
}
