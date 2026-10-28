#include "suomi_gfx.h"

/* Super Mario-style platformer for SC-16 */
/* Controls: Arrow keys to move */

/* Simple sprite data for Mario (16x16 pixels, palette indices) */
unsigned char mario_sprite[] = {
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 1, 2, 2, 1, 1, 1, 1, 1, 1, 2, 2, 0, 0, 0,
    0, 0, 1, 2, 2, 2, 1, 1, 1, 1, 2, 2, 2, 0, 0, 0,
    0, 0, 1, 1, 2, 2, 2, 1, 1, 2, 2, 2, 1, 0, 0, 0,
    0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 7, 1, 1, 1, 7, 1, 1, 0, 0, 0, 0,
    0, 0, 0, 1, 7, 7, 7, 1, 7, 7, 7, 1, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 7, 7, 7, 7, 7, 7, 7, 7, 0, 0, 0, 0,
    0, 0, 0, 1, 7, 7, 7, 7, 7, 7, 7, 7, 1, 0, 0, 0,
    0, 0, 0, 1, 7, 7, 7, 7, 7, 7, 7, 7, 1, 0, 0, 0,
    0, 0, 1, 1, 1, 7, 7, 7, 7, 7, 7, 1, 1, 0, 0, 0,
    0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0};

/* Simple sprite data for Goomba enemy (16x16 pixels) */
unsigned char goomba_sprite[] = {
    0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0,
    0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0,
    0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0,
    0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0,
    0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0,
    0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};

/* Simple sprite data for coin (8x8 pixels) */
unsigned char coin_sprite[] = {
    0, 0, 2, 2, 2, 2, 0, 0,
    0, 2, 2, 2, 2, 2, 2, 0,
    2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2,
    0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 2, 2, 2, 2, 0, 0};

/* Game variables */
int mario_x, mario_y;
int mario_vx, mario_vy;
int on_ground;
int score;
int level;
int lives;
int coin_x, coin_y;
int coin_collected;

/* Goomba positions */
int goomba_x, goomba_y;
int goomba_active;
int goomba_direction;

/* Level map - simplified representation */
/* Each cell is 16x16 pixels */
/* 0 = empty, 1 = ground, 2 = brick, 3 = question block, 4 = pipe */
unsigned char level_map[15][20] = {
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0},
    {0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0},
    {1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1},
    {1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1},
    {1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1},
    {1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}};

void init_game()
{
    mario_x = 20;
    mario_y = 200;
    mario_vx = 0;
    mario_vy = 0;
    on_ground = 0;
    score = 0;
    level = 1;
    lives = 3;
    coin_x = 160;
    coin_y = 140;
    coin_collected = 0;
    goomba_x = 120;
    goomba_y = 200;
    goomba_active = 1;
    goomba_direction = -1;
}

void handle_input()
{
    unsigned int keys = gfx_keys();
    mario_vx = 0;

    if (keys & KEY_LEFT)
    {
        mario_vx = -2;
    }
    if (keys & KEY_RIGHT)
    {
        mario_vx = 2;
    }
    if ((keys & KEY_UP) && on_ground)
    {
        mario_vy = -12;
        on_ground = 0;
    }
}

void update_mario()
{
    /* FIXED: All variable declarations moved to top */
    int i, j;
    int px, py, pw, ph;
    int tile_x1, tile_y1, tile_x2, tile_y2;

    /* Apply gravity */
    mario_vy += 1;

    /* Update position */
    mario_x += mario_vx;
    mario_y += mario_vy;

    /* Screen boundaries */
    if (mario_x < 0)
        mario_x = 0;
    if (mario_x > 300)
        mario_x = 300;

    on_ground = 0;

    /* Convert pixel position to tile coordinates */
    tile_x1 = mario_x / 16;
    tile_y1 = mario_y / 16;
    tile_x2 = (mario_x + 15) / 16;
    tile_y2 = (mario_y + 15) / 16;

    /* Check collision with solid tiles (1, 2, 4) */
    for (j = tile_y1; j <= tile_y2; j++)
    {
        for (i = tile_x1; i <= tile_x2; i++)
        {
            if (i >= 0 && i < 20 && j >= 0 && j < 15)
            {
                if (level_map[j][i] == 1 || level_map[j][i] == 2 || level_map[j][i] == 4)
                {
                    /* Check if landing on top of tile */
                    if (mario_vy > 0 && mario_y + 16 > j * 16 - 2 &&
                        mario_y + 16 < j * 16 + 18 &&
                        mario_x + 16 > i * 16 && mario_x < i * 16 + 16)
                    {
                        mario_y = j * 16 - 16;
                        mario_vy = 0;
                        on_ground = 1;
                    }
                }
            }
        }
    }

    /* Check ground at bottom */
    if (mario_y > 224)
    {
        mario_y = 224;
        mario_vy = 0;
        on_ground = 1;
    }

    /* Check coin collection */
    if (!coin_collected)
    {
        if (mario_x + 16 > coin_x && mario_x < coin_x + 8 &&
            mario_y + 16 > coin_y && mario_y < coin_y + 8)
        {
            coin_collected = 1;
            score += 100;
        }
    }

    /* Check goomba collision */
    if (goomba_active)
    {
        /* Jump on goomba */
        if (mario_y + 16 > goomba_y && mario_y < goomba_y + 16 &&
            mario_x + 16 > goomba_x && mario_x < goomba_x + 16)
        {
            if (mario_vy > 0 && mario_y < goomba_y)
            {
                /* Stomp goomba */
                goomba_active = 0;
                mario_vy = -8;
                score += 200;
            }
            else
            {
                /* Lose a life */
                lives--;
                mario_x = 20;
                mario_y = 200;
                mario_vx = 0;
                mario_vy = 0;
            }
        }
    }
}

void update_enemy()
{
    if (goomba_active)
    {
        goomba_x += goomba_direction * 1;

        /* Simple AI: turn around at edges */
        if (goomba_x <= 100)
            goomba_direction = 1;
        if (goomba_x >= 140)
            goomba_direction = -1;
    }
}

void draw_scene()
{
    /* FIXED: All variable declarations moved to top */
    int i, j;
    int tile_x, tile_y;
    unsigned char tile_type;
    char score_text[16];
    char life_text[8];

    /* Clear screen with blue background */
    gfx_clear(1); /* Blue background */

    /* Draw level tiles */
    for (j = 0; j < 15; j++)
    {
        for (i = 0; i < 20; i++)
        {
            tile_x = i * 16;
            tile_y = j * 16;
            tile_type = level_map[j][i];

            if (tile_type == 1)
            {
                /* Ground - brown */
                gfx_rect(tile_x, tile_y, 16, 16, 6);
            }
            else if (tile_type == 2)
            {
                /* Brick - red */
                gfx_rect(tile_x, tile_y, 16, 16, 2);
            }
            else if (tile_type == 3)
            {
                /* Question block - yellow */
                gfx_rect(tile_x, tile_y, 16, 16, 4);
            }
            else if (tile_type == 4)
            {
                /* Pipe - green */
                gfx_rect(tile_x, tile_y, 16, 16, 3);
            }
        }
    }

    /* Draw coin if not collected */
    if (!coin_collected)
    {
        gfx_sprite(coin_x, coin_y, 8, 8, coin_sprite);
    }

    /* Draw Goomba if active */
    if (goomba_active)
    {
        gfx_sprite(goomba_x, goomba_y, 16, 16, goomba_sprite);
    }

    /* Draw Mario sprite */
    gfx_sprite(mario_x, mario_y, 16, 16, mario_sprite);

    /* Draw HUD text */
    score_text[0] = 'S';
    score_text[1] = 'C';
    score_text[2] = 'O';
    score_text[3] = 'R';
    score_text[4] = 'E';
    score_text[5] = ':';
    score_text[6] = '0' + (score / 100000) % 10;
    score_text[7] = '0' + (score / 10000) % 10;
    score_text[8] = '0' + (score / 1000) % 10;
    score_text[9] = '0' + (score / 100) % 10;
    score_text[10] = '0' + (score / 10) % 10;
    score_text[11] = '0' + score % 10;
    score_text[12] = ' ';
    score_text[13] = 'L';
    score_text[14] = 'I';
    score_text[15] = 0;

    gfx_text(10, 10, score_text, 1);

    /* Show remaining lives */
    if (lives > 0)
    {
        life_text[0] = 'L';
        life_text[1] = 'I';
        life_text[2] = 'V';
        life_text[3] = 'E';
        life_text[4] = 'S';
        life_text[5] = ':';
        life_text[6] = '0' + lives;
        life_text[7] = 0;
        gfx_text(250, 10, life_text, 1);
    }

    gfx_present();
}

int main()
{
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_layer_reset(1 << LAYER_BACKGROUND);
    init_game();

    while (1)
    {
        handle_input();
        update_mario();
        update_enemy();
        draw_scene();
    }

    return 0;
}
