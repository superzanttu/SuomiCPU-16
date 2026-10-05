// Simple MS Paint style drawing program for the SC-16 (images cannot be saved).
//
// Mouse controls:
// - Left button draws with the foreground color, right button with the background color
// - Click a tool, a brush size or a palette color in the toolbar on the left
//   (left click on a color sets the foreground, right click sets the background)
// - Drag with a button held to draw; shapes show a live preview while you drag
// - Double-click a palette color to fill the whole canvas with it
// - Double-click the CLR button to clear the canvas to the background color
//
// The canvas is simply the back buffer: strokes are drawn into it and stay there.
// Shape previews use gfx_save()/gfx_restore() to roll back to the picture before the drag.
#include "suomi_gfx.h"

/* --- 1. Constants and state --- */
#define TB_W 46          // Toolbar width; the canvas starts at this x
#define CANVAS_W 274
#define TOOLBAR_BG 40

#define T_PEN 0
#define T_ERASER 1
#define T_SPRAY 2
#define T_LINE 3
#define T_RECT 4
#define T_BOX 5
#define T_CLEAR 6        // Action button, not a drawing tool

int pal[16] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 100, 150, 200, 230};
int sizes[4] = {1, 2, 4, 8};

int tool, size_idx, fg, bg;
int drag, dmask, color;          // Drag state: 0 none, mouse button mask, drawing color
int ax, ay, lx, ly;              // Drag anchor and previous point

/* --- 2. Helpers --- */
int iabs(int v) {
    if (v < 0) return -v;
    return v;
}

int sign(int v) {
    if (v < 0) return -1;
    if (v > 0) return 1;
    return 0;
}

int clamp_x(int x) {
    if (x < TB_W) return TB_W;
    if (x > 319) return 319;
    return x;
}

int clamp_y(int y) {
    if (y < 0) return 0;
    if (y > 239) return 239;
    return y;
}

int is_shape(int t) {
    return t >= T_LINE && t <= T_BOX;
}

/* --- 3. Drawing primitives --- */
/* A square dot of the given size centered on (x, y) */
void stamp(int x, int y, int s, int c) {
    if (s == 1) gfx_pixel(x, y, c);
    else gfx_rect(x - (s >> 1), y - (s >> 1), s, s, c);
}

/* Connects two points with dots spaced one dot apart, so thick strokes stay solid */
void stroke(int x0, int y0, int x1, int y1, int s, int c) {
    int dx, dy, ax_, ay_, d, err, x, y;
    if (s == 1) {
        gfx_line(x0, y0, x1, y1, c);
        return;
    }
    dx = x1 - x0;
    dy = y1 - y0;
    ax_ = iabs(dx);
    ay_ = iabs(dy);
    x = x0;
    y = y0;
    err = 0;
    stamp(x, y, s, c);
    if (ax_ >= ay_) {
        for (d = s; d <= ax_; d += s) {
            x += sign(dx) * s;
            err += ay_ * s;
            while (err >= ax_) { y += sign(dy); err -= ax_; }
            stamp(x, y, s, c);
        }
    } else {
        for (d = s; d <= ay_; d += s) {
            y += sign(dy) * s;
            err += ax_ * s;
            while (err >= ay_) { x += sign(dx); err -= ay_; }
            stamp(x, y, s, c);
        }
    }
    stamp(x1, y1, s, c);
}

/* Scatters random dots around the pointer like an airbrush */
void spray(int x, int y, int c) {
    int i, ox, oy;
    for (i = 0; i < 10; i++) {
        ox = (gfx_random() & 15) - 8;
        oy = (gfx_random() & 15) - 8;
        if (ox * ox + oy * oy <= 64) gfx_pixel(clamp_x(x + ox), clamp_y(y + oy), c);
    }
}

/* Line, rectangle outline or filled rectangle between two corner points */
void draw_shape(int x0, int y0, int x1, int y1, int s, int c) {
    int x, y, w, h;
    if (tool == T_LINE) {
        stroke(x0, y0, x1, y1, s, c);
        return;
    }
    x = x0; w = x1 - x0;
    if (w < 0) { x = x1; w = -w; }
    y = y0; h = y1 - y0;
    if (h < 0) { y = y1; h = -h; }
    w++;
    h++;
    if (tool == T_BOX) {
        gfx_rect(x, y, w, h, c);
        return;
    }
    if (s > w) s = w;
    if (s > h) s = h;
    gfx_rect(x, y, w, s, c);
    gfx_rect(x, y + h - s, w, s, c);
    gfx_rect(x, y, s, h, c);
    gfx_rect(x + w - s, y, s, h, c);
}

void fill_canvas(int c) {
    gfx_rect(TB_W, 0, CANVAS_W, 240, c);
}

/* --- 4. Mouse handling --- */
/* Click in the toolbar: choose a tool, brush size or color */
void toolbar_click(int mx, int my, int mask, unsigned int ev) {
    int i, c, r;
    int dbl = (ev & (MOUSE_LEFT_DOUBLE | MOUSE_RIGHT_DOUBLE)) != 0;
    if (mx < 2) return;
    c = (mx - 2) / 22;
    if (c > 1) c = 1;
    if (my >= 2 && my < 70) {
        i = ((my - 2) / 17) * 2 + c;
        if (i == T_CLEAR) {
            if (dbl) fill_canvas(bg);
        } else if (i < T_CLEAR) tool = i;
        gfx_sound(2, 600, 3, WAVE_SQUARE, 25);
    } else if (my >= 72 && my < 86) {
        i = (mx - 2) / 11;
        if (i < 4) size_idx = i;
        gfx_sound(2, 600, 3, WAVE_SQUARE, 25);
    } else if (my >= 130 && my < 234) {
        r = (my - 130) / 13;
        i = r * 2 + c;
        if (mask == 1) fg = pal[i];
        else bg = pal[i];
        if (dbl) {
            fill_canvas(pal[i]);
            bg = pal[i];
        }
        gfx_sound(2, 800, 3, WAVE_SQUARE, 25);
    }
}

int brush_size(void) {
    return sizes[size_idx];
}

int eraser_size(void) {
    return brush_size() * 2 + 2;
}

/* Starts a stroke or shape on the canvas at (x, y) */
void begin_drag(int x, int y) {
    drag = 1;
    color = fg;
    if (dmask == 2) color = bg;
    ax = x; ay = y; lx = x; ly = y;
    if (is_shape(tool)) gfx_save();
    else if (tool == T_PEN) stamp(x, y, brush_size(), color);
    else if (tool == T_ERASER) stamp(x, y, eraser_size(), bg);
    else spray(x, y, color);
}

/* Continues the active stroke, or re-draws the shape preview from its anchor */
void update_drag(int x, int y) {
    if (is_shape(tool)) {
        gfx_restore();
        draw_shape(ax, ay, x, y, brush_size(), color);
    } else {
        if (tool == T_PEN) stroke(lx, ly, x, y, brush_size(), color);
        else if (tool == T_ERASER) stroke(lx, ly, x, y, eraser_size(), bg);
        else spray(x, y, color);
    }
    lx = x;
    ly = y;
}

/* --- 5. Toolbar rendering --- */
void draw_tool_button(int i) {
    int x = 2 + (i & 1) * 22;
    int y = 2 + (i >> 1) * 17;
    char *label;
    int back = 10;
    if (i == tool) back = 4;
    gfx_rect(x, y, 21, 16, back);
    if (i == T_PEN) label = "PEN";
    else if (i == T_ERASER) label = "ERS";
    else if (i == T_SPRAY) label = "SPR";
    else if (i == T_LINE) label = "LIN";
    else if (i == T_RECT) label = "REC";
    else if (i == T_BOX) label = "BOX";
    else label = "CLR";
    gfx_text(x + 2, y + 4, label, WHITE);
}

void draw_toolbar(void) {
    int i, x, y, c;
    gfx_rect(0, 0, TB_W, 240, TOOLBAR_BG);
    for (i = 0; i <= T_CLEAR; i++) draw_tool_button(i);
    /* Brush sizes */
    for (i = 0; i < 4; i++) {
        c = 10;
        if (i == size_idx) c = 4;
        gfx_rect(2 + i * 11, 72, 10, 14, c);
        stamp(7 + i * 11, 79, sizes[i], WHITE);
    }
    /* Foreground over background color swatch */
    gfx_rect(6, 92, 20, 16, WHITE);
    gfx_rect(18, 98, 20, 16, WHITE);
    gfx_rect(19, 99, 18, 14, bg);
    gfx_rect(7, 93, 18, 14, fg);
    gfx_text(2, 114, "DBL-CLK", GRAY);
    gfx_text(2, 122, "CLR/PAL", GRAY);
    for (i = 0; i < 16; i++) {
        x = 2 + (i & 1) * 22;
        y = 130 + (i >> 1) * 13;
        gfx_rect(x, y, 21, 12, pal[i]);
        if (pal[i] == fg) gfx_rect(x, y, 3, 3, WHITE);
        if (pal[i] == bg) gfx_rect(x + 18, y + 9, 3, 3, GRAY);
    }
}

/* --- 6. Main loop --- */
void new_picture(void) {
    tool = T_PEN;
    size_idx = 1;
    fg = 0;
    bg = 1;
    drag = 0;
    fill_canvas(bg);
}

int main(void) {
    int mx, my, cx, cy;
    unsigned int buttons, ev;
    new_picture();
    while (1) {
        mx = gfx_mouse_x();
        my = gfx_mouse_y();
        buttons = gfx_mouse_buttons();
        ev = gfx_mouse_events();
        cx = clamp_x(mx);
        cy = clamp_y(my);
        if (drag == 0 && (ev & (MOUSE_LEFT_DOWN | MOUSE_RIGHT_DOWN))) {
            dmask = 1;
            if (!(ev & MOUSE_LEFT_DOWN)) dmask = 2;
            if (mx < TB_W) toolbar_click(mx, my, dmask, ev);
            else begin_drag(cx, cy);
        }
        if (drag) {
            if (buttons & dmask) update_drag(cx, cy);
            else {
                update_drag(cx, cy);
                drag = 0;
            }
        }
        draw_toolbar();
        gfx_present();
    }
    return 0;
}
