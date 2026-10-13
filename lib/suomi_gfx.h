// 320x240 indexed or 640x480 packed graphics, double buffered.
// All drawing goes to a back buffer; gfx_present() shows it and waits for the next frame.
void gfx_clear(unsigned char color);
void gfx_pixel(int x, int y, unsigned char color);
void gfx_rect(int x, int y, int w, int h, unsigned char color);
void gfx_line(int x0, int y0, int x1, int y1, unsigned char color);
void gfx_sprite(int x, int y, int w, int h, unsigned char *data);
void gfx_bitmap(int x, int y, int w, int h, unsigned char *data, unsigned char color);
void gfx_poly(int x, int y, int count, char *points, unsigned char color);
void gfx_text(int x, int y, char *text, unsigned char color);
void gfx_present(void);
void gfx_mode(int mode);
// Static layer: gfx_save() keeps the back buffer, gfx_restore() redraws it in one call.
void gfx_save(void);
void gfx_restore(void);
// Sound: channel 0-3, freq in Hz (0 = silence channel), frames (0 = loop), wave, volume 1-100.
void gfx_sound(int channel, int freq, int frames, int wave, int volume);
unsigned int gfx_keys(void);
unsigned int gfx_keys_ext(void);
// Mouse: position in screen pixels, held buttons and one-frame events (see MOUSE_* bits).
int gfx_mouse_x(void);
int gfx_mouse_y(void);
unsigned int gfx_mouse_buttons(void);
unsigned int gfx_mouse_events(void);
int gfx_mouse_wheel(void);

/* Tile map: one byte per tile = palette color (0 = black); draws 16x16 tiles over the
   whole back buffer with the camera at pixel (cam_x, cam_y). */
void gfx_tilemap(unsigned char *map, int map_w, int map_h, int cam_x, int cam_y);
/* GPU scene: 10-word header, 3 words per vertex, then 4 words per triangle. */
int gfx3d_draw(int *scene);
#define GPU3D_ONE 256
#define GPU3D_FILL 1
#define GPU3D_WIREFRAME 2
#define GPU3D_OK 0
#define GPU3D_ERROR_SCENE 1
#define GPU3D_ERROR_FACE 2
#define GPU3D_HEADER_WORDS 10
#define GPU3D_VERTEX_WORDS 3
#define GPU3D_FACE_WORDS 4
#define GPU3D_MAX_VERTICES 128
#define GPU3D_MAX_FACES 128
#define GPU3D_FIXED(value) ((value) * GPU3D_ONE)
/* Next typed character (ASCII, 8 = backspace, 13 = enter) or 0 when none. */
int gfx_getchar(void);

/* LAN networking (UDP broadcast, no server). Instances with the same title find
   each other; at most NET_MAX_PLAYERS share one title. Messages are 1..NET_MAX_MSG bytes. */
int net_open(char *title);                      /* join group; 1 = ok (title up to 16 chars) */
void net_close(void);                           /* leave group */
int net_send(unsigned char *data, int len);     /* broadcast to all others; 1 = sent */
int net_recv(unsigned char *buf);               /* next message: returns length, 0 = none */
int net_sender(void);                           /* slot of the last net_recv() sender */
int net_players(void);                          /* live instances including this one */
int net_slot(void);                             /* own slot 0..7 (-1 = not joined) */
int net_active(int slot);                       /* 1 if a player occupies slot */
int net_ready(void);                            /* 1 once slot assignment has settled */
int net_full(void);                             /* 1 if the group had no free slot */
int net_is_open(void);

#define NET_MAX_PLAYERS 8
#define NET_MAX_MSG 48
unsigned int gfx_random(void);
unsigned int gfx_ticks(void);
unsigned int gfx_rtc(unsigned char field);
void gfx_speed(int mode);
void gfx_vsync(int enabled);
#define SPEED_VERY_SLOW 0
#define SPEED_SLOW 1
#define SPEED_NORMAL 2
#define SPEED_FAST 3
#define SPEED_MAX 4
#define GFX_MODE_320_240 0
#define GFX_MODE_640_480 1
#define SCREEN_W_640 640
#define SCREEN_H_480 480
void gfx_irq_init(void);
unsigned int gfx_irq_ticks(void);
unsigned int gfx_irq_keys(void);

#define WAVE_SQUARE 0
#define WAVE_NOISE 1
#define WAVE_TRIANGLE 2

#define SCREEN_W 320
#define SCREEN_H 240

#define BLACK 0
#define WHITE 1
#define RED 2
#define GREEN 3
#define BLUE 4
#define YELLOW 5
#define CYAN 6
#define MAGENTA 7
#define ORANGE 8
#define GRAY 9
#define DARK_GRAY 10
#define DARK_RED 11

#define KEY_LEFT 1
#define KEY_RIGHT 2
#define KEY_UP 4
#define KEY_DOWN 8
#define KEY_FIRE 16
#define KEY_START 32
#define KEY_TAB 64

/* Bits of gfx_mouse_buttons() (held) and gfx_mouse_events() (latched until read). */
#define MOUSE_LEFT 1
#define MOUSE_RIGHT 2
#define MOUSE_LEFT_DOWN 1
#define MOUSE_RIGHT_DOWN 2
#define MOUSE_LEFT_DOUBLE 4
#define MOUSE_RIGHT_DOUBLE 8
#define MOUSE_LEFT_UP 16
#define MOUSE_RIGHT_UP 32

/* Bits returned by gfx_keys_ext(). */
#define KEYX_ROLL_LEFT 1
#define KEYX_ROLL_RIGHT 2
#define KEYX_THRUST 4
#define KEYX_REVERSE 8
