// LAN networking demo for the SC-16 (UDP broadcast, no server program needed).
// ICON
// .......##.......
// .....######.....
// ...##......##...
// ..#..........#..
// .#....####....#.
// #...##....##...#
// #..#........#..#
// #.#...####...#.#
// #.#..#....#..#.#
// #.#.#......#.#.#
// #.##...##...##.#
// #..#...##...#..#
// .#....####....#.
// ..#..........#..
// ...##......##...
// .....######.....
//
// Start the emulator several times on the same network (up to 8 instances):
//     python src/SuomiCPU.py examples/net_game_demo.c
// Every instance announces itself with the game title "netdemo" and only talks to
// other instances with the same title. Each player gets a slot 0..7 (a colour).
//
// Controls: arrow keys move your square, Space broadcasts a "ping" ring to everybody.
//
// Networking pattern used here: every frame the game broadcasts its own state
// (a 4 byte message) and drains the receive queue, drawing whoever is still active.
#include "suomi_gfx.h"

#define MSG_POS 1
#define MSG_PING 2

int colors[8] = {2, 3, 4, 5, 6, 7, 8, 1};   // One colour per slot
int px[8], py[8];                            // Last known position of each slot
int ring[8], ringx[8], ringy[8];             // Ping animation (radius 0 = off)
unsigned char buf[NET_MAX_MSG];
int me, x, y, prev_keys;

/* --- Messages: type, x high, x low, y --- */
void send_state(int type) {
    buf[0] = type;
    buf[1] = x >> 8;
    buf[2] = x & 255;
    buf[3] = y;
    net_send(buf, 4);
}

void receive_all(void) {
    int n, s, rx, ry;
    n = net_recv(buf);
    while (n > 0) {
        s = net_sender();
        if (n == 4 && s >= 0 && s < NET_MAX_PLAYERS) {
            rx = (buf[1] << 8) | buf[2];
            ry = buf[3];
            px[s] = rx;
            py[s] = ry;
            if (buf[0] == MSG_PING) {
                ring[s] = 1;
                ringx[s] = rx;
                ringy[s] = ry;
            }
        }
        n = net_recv(buf);
    }
}

/* --- Drawing --- */
void draw_ring(int s) {
    int r, cx, cy;
    r = ring[s];
    cx = ringx[s];
    cy = ringy[s];
    gfx_rect(cx - r, cy - r, r * 2 + 8, 1, colors[s]);
    gfx_rect(cx - r, cy + r + 8, r * 2 + 8, 1, colors[s]);
    gfx_rect(cx - r, cy - r, 1, r * 2 + 8, colors[s]);
    gfx_rect(cx + r + 8, cy - r, 1, r * 2 + 8, colors[s]);
    ring[s] = r + 2;
    if (ring[s] > 40) ring[s] = 0;
}

void draw(void) {
    int s;
    gfx_clear(BLACK);
    gfx_text(8, 6, "NET DEMO - LAN", WHITE);
    // Slot lights: lit when a player occupies the slot
    for (s = 0; s < NET_MAX_PLAYERS; s++) {
        if (net_active(s)) gfx_rect(200 + s * 14, 6, 10, 8, colors[s]);
        else gfx_rect(200 + s * 14, 6, 10, 8, DARK_GRAY);
    }
    if (net_full()) gfx_text(8, 20, "GROUP FULL - 8 PLAYERS ALREADY", RED);
    else if (!net_ready()) gfx_text(8, 20, "JOINING...", YELLOW);
    else if (net_players() < 2) gfx_text(8, 20, "ALONE - START ANOTHER INSTANCE", GRAY);
    else gfx_text(8, 20, "SPACE = PING EVERYONE", GREEN);
    for (s = 0; s < NET_MAX_PLAYERS; s++) {
        if (s != me && net_active(s)) gfx_rect(px[s], py[s], 8, 8, colors[s]);
        if (ring[s] > 0) draw_ring(s);
    }
    if (me >= 0) gfx_rect(x, y, 8, 8, colors[me]);
    gfx_present();
}

int main(void) {
    int keys;
    x = 150;
    y = 120;
    net_open("netdemo");
    while (1) {
        keys = gfx_keys();
        me = net_slot();
        if (keys & KEY_LEFT) x = x - 2;
        if (keys & KEY_RIGHT) x = x + 2;
        if (keys & KEY_UP) y = y - 2;
        if (keys & KEY_DOWN) y = y + 2;
        if (x < 0) x = 0;
        if (x > SCREEN_W - 8) x = SCREEN_W - 8;
        if (y < 30) y = 30;
        if (y > SCREEN_H - 8) y = SCREEN_H - 8;
        if ((keys & KEY_FIRE) && !(prev_keys & KEY_FIRE) && me >= 0) {
            send_state(MSG_PING);
            ring[me] = 1;
            ringx[me] = x;
            ringy[me] = y;
        } else {
            send_state(MSG_POS);
        }
        prev_keys = keys;
        receive_all();
        draw();
    }
    return 0;
}
