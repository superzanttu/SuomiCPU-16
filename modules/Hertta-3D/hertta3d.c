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

int sintab[64] = {
    0, 25, 50, 74, 98, 121, 142, 162, 181, 198, 213, 226, 237, 245, 251, 255,
    256, 255, 251, 245, 237, 226, 213, 198, 181, 162, 142, 121, 98, 74, 50, 25,
    0, -25, -50, -74, -98, -121, -142, -162, -181, -198, -213, -226, -237,
    -245, -251, -255, -256, -255, -251, -245, -237, -226, -213, -198, -181,
    -162, -142, -121, -98, -74, -50, -25};

unsigned char maze[MW * MH];
int scene[SCENE_WORDS];
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
    int x;
    int y;
    int gate_a;
    int gate_b;
    int gate_c;
    int gate_d;
    for (y = 0; y < MH; y++)
    {
        for (x = 0; x < MW; x++)
        {
            maze[y * MW + x] = (x == 0 || y == 0 || x == MW - 1 || y == MH - 1) ? 1 : 0;
        }
    }
    gate_a = 2 + (level % 5);
    gate_b = 2 + ((level + 2) % 5);
    gate_c = 1 + ((level + 1) % 7);
    gate_d = 1 + ((level + 4) % 7);
    for (y = 1; y < MH - 1; y++)
    {
        if (y != gate_a)
            maze[y * MW + 3] = 1;
        if (y != gate_b)
            maze[y * MW + 6] = 1;
    }
    for (x = 1; x < MW - 1; x++)
    {
        if (x != gate_c)
            maze[4 * MW + x] = 1;
        if (x != gate_d)
            maze[7 * MW + x] = 1;
    }
    if (level > 1)
    {
        x = 1 + (level % 2);
        y = 2 + (level % 4);
        if (x != 1 || y != 1)
            maze[y * MW + x] = 1;
        x = 7 + (level % 2);
        y = 5 + (level % 2);
        maze[y * MW + x] = 1;
    }
    if (level > 3)
    {
        maze[2 * MW + 8] = 1;
        maze[6 * MW + 2] = 1;
    }
    maze[1 * MW + 1] = 0;
    maze[8 * MW + 8] = 0;
}

void reset_level(void)
{
    int i;
    make_maze();
    player_x = FIX + FIX / 2;
    player_y = FIX + FIX / 2;
    player_angle = 32;
    dog_x = 8 * FIX + FIX / 2;
    dog_y = 8 * FIX + FIX / 2;
    dog_angle = 160;
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
    left = (x0 * depth) / 120;
    right = (x1 * depth) / 120;
    top = (half_height * depth) / 120;
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
    int local_x;
    int local_z;
    int dx;
    int dy;
    int dog_local_x;
    int dog_depth;
    int treat_local_x;
    int treat_depth;
    int color;
    int left;
    int right;
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
            wx = player_x + (sine(angle) * dist >> 8);
            wy = player_y - (cosine(angle) * dist >> 8);
            if (wall_at(wx, wy))
                break;
            dist += 24;
        }
        if (dist > 1536)
            dist = 1536;
        depth = (dist * cosine(relative)) >> 8;
        if (depth < 32)
            depth = 32;
        left = -160 + (ray * 320) / RAYS;
        right = -160 + ((ray + 1) * 320) / RAYS;
        color = depth < 400 ? (ray & 1 ? 14 : 8) :
                depth < 800 ? (ray & 1 ? 10 : 9) : (ray & 1 ? 60 : 9);
        scene_quad(left, right, 70, depth, color);
    }

    dx = dog_x - player_x;
    dy = dog_y - player_y;
    dog_local_x = (dx * cosine(player_angle) + dy * sine(player_angle)) >> 8;
    dog_depth = (dx * sine(player_angle) - dy * cosine(player_angle)) >> 8;
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
            treat_local_x = (dx * cosine(player_angle) + dy * sine(player_angle)) >> 8;
            treat_depth = (dx * sine(player_angle) - dy * cosine(player_angle)) >> 8;
            if (treat_depth > 0)
                add_billboard(treat_local_x, -FIX / 3, treat_depth, FIX / 12, FIX / 12, WHITE);
        }
    }

    scene[0] = vertex_count;
    scene[1] = face_count;
    scene[2] = scene[3] = scene[4] = 0;
    scene[5] = scene[6] = scene[7] = 0;
    scene[8] = 120 * FIX;
    scene[9] = GPU3D_FILL;
    compact_faces();
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

void update_dog(void)
{
    int speed;
    int nx;
    int ny;
    int dx;
    int dy;
    int direction;
    int candidate_distance;
    int best_distance;
    int best_direction;
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
        best_distance = absval(dx) + absval(dy);
        best_direction = -1;
        for (i = 0; i < 4; i++)
        {
            direction = (dog_turn + i) & 3;
            nx = dog_x + sine(direction * 64) * 5 / 256;
            ny = dog_y - cosine(direction * 64) * 5 / 256;
            if (!wall_at(nx, ny))
            {
                candidate_distance = absval(player_x - nx) + absval(player_y - ny);
                if (candidate_distance < best_distance)
                {
                    best_distance = candidate_distance;
                    best_direction = direction;
                }
            }
        }
        if (best_direction < 0)
        {
            for (i = 0; i < 4; i++)
            {
                direction = (dog_turn + i) & 3;
                nx = dog_x + sine(direction * 64) * 5 / 256;
                ny = dog_y - cosine(direction * 64) * 5 / 256;
                if (!wall_at(nx, ny))
                {
                    best_direction = direction;
                    break;
                }
            }
        }
        if (best_direction >= 0)
        {
            dog_angle = best_direction * 64;
            dog_x += sine(dog_angle) * 5 / 256;
            dog_y -= cosine(dog_angle) * 5 / 256;
        }
        dog_turn++;
        return;
    }

    dog_turn++;
    if (dog_turn > 30 + (gfx_random() & 15))
    {
        dog_angle = (gfx_random() & 3) * 64;
        dog_turn = 0;
    }
    speed = 3 + level;
    if (speed > 10)
        speed = 10;
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
    if (status != GPU3D_OK)
    {
        gfx_text(8, 210, "SC-GPU SCENE ERROR", RED);
    }
    gfx_rect(158, 119, 5, 1, WHITE);
    gfx_rect(160, 117, 1, 5, WHITE);
    draw_hud();
    draw_status();
    gfx_present();
}

int main(void)
{
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
