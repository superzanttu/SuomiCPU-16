// SuomiCPU graphics library: 320x240, 8-bit palette, double buffered.
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
unsigned int gfx_keys(void);
unsigned int gfx_random(void);
unsigned int gfx_ticks(void);
unsigned int gfx_rtc(unsigned char field);
void gfx_irq_init(void);
unsigned int gfx_irq_ticks(void);
unsigned int gfx_irq_keys(void);

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
