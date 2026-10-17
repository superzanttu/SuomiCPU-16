from pathlib import Path
import math

import pygame


GAME_DIR = Path(__file__).resolve().parent
OUTPUT = GAME_DIR / "gfx" / "tiles_objects.png"
SIZE = 64
COLORS = {
    "outline": (28, 34, 35),
    "stone": (104, 111, 108),
    "stone_light": (160, 164, 151),
    "stone_dark": (65, 73, 72),
    "brick": (145, 88, 58),
    "brick_light": (201, 133, 83),
    "wood": (113, 72, 42),
    "wood_light": (171, 120, 68),
    "grass": (45, 104, 57),
    "leaf": (69, 151, 70),
    "leaf_light": (116, 189, 82),
    "leaf_dark": (34, 83, 48),
    "red": (197, 57, 48),
    "red_light": (242, 102, 72),
    "blue": (48, 124, 190),
    "blue_light": (107, 194, 225),
    "gold": (238, 178, 44),
    "cream": (232, 219, 172),
    "metal": (120, 139, 143),
    "metal_light": (190, 201, 190),
}


def polygon(surface, points, color):
    pygame.draw.polygon(surface, color, points)


def outlined_polygon(surface, points, fill, outline=None):
    pygame.draw.polygon(surface, outline or COLORS["outline"], points)
    pygame.draw.polygon(surface, fill, points, 0)


def outlined_rect(surface, rect, fill, outline=None, width=2):
    pygame.draw.rect(surface, outline or COLORS["outline"], rect, border_radius=4)
    inner = pygame.Rect(rect).inflate(-width * 2, -width * 2)
    if inner.width > 0 and inner.height > 0:
        pygame.draw.rect(surface, fill, inner, border_radius=3)


def wall(surface, style):
    pieces = (
        ("horizontal",),
        ("vertical",),
        ("top", "left"),
        ("top", "right"),
        ("bottom", "left"),
        ("bottom", "right"),
    )
    for edge in pieces[style]:
        if edge == "horizontal":
            rect = (0, 24, 64, 16)
        elif edge == "vertical":
            rect = (24, 0, 16, 64)
        elif edge == "top":
            rect = (0, 0, 64, 18)
        elif edge == "bottom":
            rect = (0, 46, 64, 18)
        elif edge == "left":
            rect = (0, 0, 18, 64)
        else:
            rect = (46, 0, 18, 64)
        pygame.draw.rect(surface, COLORS["outline"], rect)
        inner = pygame.Rect(rect).inflate(-4, -4)
        pygame.draw.rect(surface, COLORS["stone"], inner)
        if rect[2] > rect[3]:
            pygame.draw.line(surface, COLORS["stone_light"],
                             (rect[0] + 2, rect[1] + 4),
                             (rect[0] + rect[2] - 2, rect[1] + 4), 2)
            pygame.draw.line(surface, COLORS["stone_dark"],
                             (rect[0] + 2, rect[1] + rect[3] - 4),
                             (rect[0] + rect[2] - 2, rect[1] + rect[3] - 4), 2)
            for x in range(rect[0] + 13, rect[0] + rect[2], 15):
                pygame.draw.line(surface, COLORS["stone_dark"],
                                 (x, rect[1] + 5), (x, rect[1] + rect[3] - 2), 2)
        else:
            pygame.draw.line(surface, COLORS["stone_light"],
                             (rect[0] + 4, rect[1] + 2),
                             (rect[0] + 4, rect[1] + rect[3] - 2), 2)
            pygame.draw.line(surface, COLORS["stone_dark"],
                             (rect[0] + rect[2] - 4, rect[1] + 2),
                             (rect[0] + rect[2] - 4, rect[1] + rect[3] - 2), 2)
            for y in range(rect[1] + 13, rect[1] + rect[3], 15):
                pygame.draw.line(surface, COLORS["stone_dark"],
                                 (rect[0] + 5, y), (rect[0] + rect[2] - 2, y), 2)


def building(surface, style):
    base = (103, 117, 117)
    roof = (133, 150, 146)
    pygame.draw.rect(surface, COLORS["outline"], (0, 0, 64, 64))
    pygame.draw.rect(surface, base, (2, 2, 60, 60))
    pygame.draw.rect(surface, roof, (6, 6, 52, 52))
    pygame.draw.line(surface, (157, 172, 163), (9, 12), (54, 12), 2)
    pygame.draw.line(surface, (91, 108, 108), (9, 53), (54, 53), 2)
    pygame.draw.line(surface, (111, 130, 128), (12, 20), (52, 20), 1)
    pygame.draw.line(surface, (111, 130, 128), (12, 43), (52, 43), 1)
    wall_edges = (
        ("top", "left"),
        ("top",),
        ("top", "right"),
        ("left",),
        (),
        ("right",),
        ("bottom", "left"),
        ("bottom",),
        ("bottom", "right"),
    )
    for edge in wall_edges[style]:
        if edge == "top":
            rect = (0, 0, 64, 9)
        elif edge == "bottom":
            rect = (0, 55, 64, 9)
        elif edge == "left":
            rect = (0, 0, 9, 64)
        else:
            rect = (55, 0, 9, 64)
        pygame.draw.rect(surface, COLORS["outline"], rect)
        inner = pygame.Rect(rect).inflate(-3, -3)
        pygame.draw.rect(surface, COLORS["stone"], inner)
        if rect[2] > rect[3]:
            pygame.draw.line(surface, COLORS["stone_light"],
                             (rect[0] + 3, rect[1] + 2),
                             (rect[0] + rect[2] - 3, rect[1] + 2), 2)
            for x in range(rect[0] + 15, rect[0] + rect[2], 16):
                pygame.draw.line(surface, COLORS["stone_dark"],
                                 (x, rect[1] + 2), (x, rect[1] + rect[3] - 2), 1)
        else:
            pygame.draw.line(surface, COLORS["stone_light"],
                             (rect[0] + 2, rect[1] + 3),
                             (rect[0] + 2, rect[1] + rect[3] - 3), 2)
            for y in range(rect[1] + 15, rect[1] + rect[3], 16):
                pygame.draw.line(surface, COLORS["stone_dark"],
                                 (rect[0] + 2, y), (rect[0] + rect[2] - 2, y), 1)
    if style == 4:
        outlined_rect(surface, (24, 24, 16, 16), COLORS["metal"])
        pygame.draw.line(surface, COLORS["metal_light"], (28, 28), (36, 28), 2)
        pygame.draw.line(surface, COLORS["metal_light"], (28, 34), (36, 34), 2)
    if style == 7:
        outlined_rect(surface, (23, 45, 18, 18), COLORS["wood"])
        pygame.draw.circle(surface, COLORS["gold"], (36, 53), 1)


def powerup(surface, style):
    colors = [COLORS["red"], COLORS["gold"], COLORS["blue"], COLORS["blue"],
              (224, 111, 35), (82, 177, 101)]
    pygame.draw.circle(surface, COLORS["outline"], (32, 33), 23)
    pygame.draw.circle(surface, colors[style], (32, 31), 19)
    pygame.draw.circle(surface, (245, 238, 206), (25, 23), 5)
    pygame.draw.circle(surface, COLORS["cream"], (32, 31), 15, 2)
    if style == 0:
        pygame.draw.rect(surface, (248, 248, 225), (27, 19, 10, 25))
        pygame.draw.rect(surface, (248, 248, 225), (19, 27, 26, 9))
    elif style == 1:
        outlined_rect(surface, (17, 18, 30, 29), COLORS["gold"], width=2)
        pygame.draw.line(surface, COLORS["cream"], (22, 24), (41, 24), 2)
        for x in (24, 31, 38):
            pygame.draw.rect(surface, COLORS["red"], (x, 29, 4, 13))
    elif style == 2:
        outlined_polygon(surface, [(36, 14), (23, 31), (31, 31),
                                   (27, 47), (43, 26), (34, 26)],
                         COLORS["cream"])
    elif style == 3:
        pygame.draw.arc(surface, COLORS["cream"], (18, 15, 28, 33), 0.2, 3.0, 5)
        outlined_polygon(surface, [(32, 13), (41, 18), (39, 34), (32, 42),
                                   (25, 34), (23, 18)], COLORS["blue_light"])
    elif style == 4:
        for x in (23, 32, 41):
            pygame.draw.line(surface, COLORS["cream"], (x, 42), (x, 22), 3)
            polygon(surface, [(x - 4, 25), (x, 17), (x + 4, 25)], COLORS["gold"])
    else:
        pygame.draw.line(surface, COLORS["cream"], (22, 39), (41, 21), 5)
        pygame.draw.line(surface, COLORS["cream"], (22, 21), (41, 39), 5)
        pygame.draw.circle(surface, COLORS["red"], (32, 30), 5)


def mine(surface, style):
    radius = (7, 10, 14, 18, 23, 20)[style]
    center = (32, 34)
    if style == 5:
        for angle in range(0, 360, 45):
            radians = math.radians(angle)
            start = (round(center[0] + 17 * math.cos(radians)),
                     round(center[1] + 17 * math.sin(radians)))
            end = (round(center[0] + 27 * math.cos(radians)),
                   round(center[1] + 27 * math.sin(radians)))
            pygame.draw.line(surface, COLORS["outline"], start, end, 5)
            pygame.draw.line(surface, COLORS["metal_light"], start, end, 2)
    pygame.draw.circle(surface, COLORS["outline"], center, radius + 3)
    pygame.draw.circle(surface, COLORS["stone_dark"], center, radius)
    pygame.draw.circle(surface, COLORS["metal"], center, max(3, radius - 4))
    pygame.draw.circle(surface, COLORS["metal_light"],
                       (center[0] - radius // 3, center[1] - radius // 3),
                       max(2, radius // 3))
    pygame.draw.circle(surface, COLORS["red"], center, max(2, radius // 5))
    for x in range(center[0] - radius + 4, center[0] + radius - 2, 7):
        pygame.draw.line(surface, COLORS["outline"],
                         (x, center[1] - radius + 3),
                         (x, center[1] - radius + 6), 2)


def rock(surface, style):
    shapes = [
        [(24, 39), (27, 30), (35, 28), (42, 35), (39, 43), (28, 44)],
        [(14, 40), (18, 27), (31, 23), (43, 30), (48, 43), (38, 49), (22, 47)],
        [(10, 39), (16, 22), (29, 13), (45, 18), (54, 34), (48, 49), (26, 54), (14, 49)],
        [(7, 42), (12, 24), (25, 11), (42, 13), (56, 29), (54, 45), (37, 54), (17, 52)],
        [(8, 45), (14, 25), (24, 19), (32, 8), (44, 17), (55, 27), (57, 43), (43, 53), (21, 53)],
        [(6, 44), (14, 27), (23, 29), (30, 12), (39, 25), (50, 18), (58, 37), (48, 51), (27, 54), (13, 51)],
    ]
    shape = shapes[style]
    outlined_polygon(surface, shape, COLORS["stone"])
    highlight = [(x - 3, y - 4) for x, y in shape[:max(3, len(shape) // 2)]]
    polygon(surface, highlight, COLORS["stone_light"])
    pygame.draw.line(surface, COLORS["stone_dark"], shape[0], shape[-1], 3)
    for x, y in ((25, 37), (37, 42), (31, 28)):
        if 8 < x < 56 and 8 < y < 56:
            pygame.draw.rect(surface, COLORS["stone_dark"], (x, y, 3, 2))
    if style in (4, 5):
        pygame.draw.polygon(surface, COLORS["stone_dark"],
                            [(x + 2, y + 3) for x, y in shape[2:5]])


def tree(surface, style):
    if style == 0:
        pygame.draw.ellipse(surface, COLORS["outline"], (16, 16, 32, 39))
        for y, half in ((16, 3), (25, 10), (35, 15), (45, 11), (53, 5)):
            polygon(surface, [(32, y - 6), (32 - half, y + 7),
                              (32 + half, y + 7)], COLORS["leaf_dark"])
            polygon(surface, [(32, y - 5), (32 - half + 3, y + 5),
                              (32, y + 2)], COLORS["leaf"])
        pygame.draw.rect(surface, COLORS["wood"], (29, 48, 6, 9))
    else:
        base = (18, 14, 28, 40)
        pygame.draw.ellipse(surface, COLORS["outline"], base)
        palettes = [
            (COLORS["leaf"], COLORS["leaf_light"], COLORS["leaf_dark"]),
            ((53, 128, 61), (129, 184, 91), (35, 84, 46)),
            ((71, 145, 71), (174, 198, 119), (52, 97, 56)),
            ((37, 106, 71), (77, 159, 96), (26, 75, 57)),
            ((57, 121, 56), (123, 173, 72), (37, 80, 43)),
        ]
        dark, light, shadow = palettes[(style - 1) % len(palettes)]
        pygame.draw.ellipse(surface, shadow, (21, 18, 23, 31))
        pygame.draw.ellipse(surface, dark, (17, 13, 26, 35))
        pygame.draw.ellipse(surface, light, (21, 16, 17, 20))
        for dx, dy, r in ((-8, -2, 8), (8, -4, 9), (-7, 10, 8), (7, 12, 7)):
            pygame.draw.circle(surface, COLORS["outline"], (32 + dx, 32 + dy), r + 1)
            pygame.draw.circle(surface, dark, (32 + dx, 31 + dy), r)
            pygame.draw.circle(surface, light, (29 + dx, 28 + dy), max(2, r // 3))
        pygame.draw.rect(surface, COLORS["wood"], (29, 48, 6, 9))
        if style == 2:
            pygame.draw.line(surface, (235, 230, 198), (23, 33), (25, 43), 2)


def forest(surface, style):
    green = (38, 92, 48)
    pygame.draw.ellipse(surface, (28, 58, 36), (3, 5, 58, 56))
    pygame.draw.ellipse(surface, green, (6, 7, 52, 50))
    layouts = (
        ((8, 9), (23, 8), (9, 25), (26, 25), (45, 10)),
        ((7, 9), (24, 8), (43, 9), (15, 27), (37, 26)),
        ((55, 9), (40, 8), (54, 25), (36, 25), (19, 10)),
        ((8, 9), (8, 28), (8, 48), (27, 17), (26, 39)),
        ((10, 10), (31, 8), (51, 11), (15, 31), (37, 29), (26, 49), (50, 48)),
        ((55, 9), (55, 28), (55, 48), (37, 17), (38, 39)),
        ((9, 54), (23, 55), (9, 38), (26, 37), (44, 54)),
        ((8, 55), (24, 55), (43, 55), (15, 38), (37, 38)),
        ((55, 54), (40, 55), (54, 38), (36, 37), (19, 54)),
        ((17, 17), (44, 20), (23, 43), (46, 46)),
        ((9, 30), (29, 10), (50, 32), (30, 52)),
        ((8, 10), (27, 8), (47, 13), (13, 30), (36, 29), (53, 34),
         (9, 50), (29, 49), (48, 51)),
    )
    palettes = (
        ((46, 129, 58), (100, 177, 76), (28, 76, 42)),
        ((47, 142, 64), (111, 186, 79), (30, 80, 44)),
        ((58, 132, 59), (135, 178, 79), (31, 76, 40)),
        ((34, 124, 65), (89, 169, 88), (25, 72, 47)),
    )
    light, highlight, shade = palettes[style % len(palettes)]
    radius = 13 if style != 9 else 11
    for index, (x, y) in enumerate(layouts[style]):
        r = radius if style != 9 else 9
        pygame.draw.circle(surface, (21, 45, 30), (x, y + 2), r + 2)
        pygame.draw.circle(surface, shade, (x, y), r + 1)
        pygame.draw.circle(surface, light, (x, y - 1), r)
        pygame.draw.circle(surface, highlight, (x - 4, y - 4), max(2, r // 3))
        if index % 3 == 1:
            pygame.draw.circle(surface, (143, 190, 91), (x + 4, y + 2), 2)
    if style in (4, 9, 10):
        pygame.draw.line(surface, (112, 79, 46), (18, 46), (45, 20), 5)
        pygame.draw.line(surface, (179, 132, 75), (19, 44), (44, 19), 2)


def make_tile(row, column):
    surface = pygame.Surface((SIZE, SIZE), pygame.SRCALPHA)
    if row == 0:
        wall(surface, column)
    elif row == 1 or (row == 2 and column < 3):
        building(surface, row * 6 + column - 6)
    elif row == 2:
        powerup(surface, column - 3)
    elif row == 3 and column < 3:
        powerup(surface, column + 3)
    elif row == 3:
        mine(surface, column - 3)
    elif row == 4 and column < 2:
        mine(surface, column + 3)
    elif row == 4:
        rock(surface, column - 2)
    elif row == 5:
        tree(surface, column)
    else:
        forest(surface, column if row == 6 else column + 6)
    return surface


def main():
    pygame.init()
    atlas = pygame.Surface((SIZE * 6, SIZE * 8), pygame.SRCALPHA)
    for row in range(8):
        for column in range(6):
            atlas.blit(make_tile(row, column), (column * SIZE, row * SIZE))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    pygame.image.save(atlas, str(OUTPUT))
    pygame.quit()


if __name__ == "__main__":
    main()
