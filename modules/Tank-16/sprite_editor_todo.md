# Map editor questions

Q: What default dimensions should a newly created map use?
A: The editor currently starts maps at 10×8 tiles. Change `MAP_DEFAULT_WIDTH` and `MAP_DEFAULT_HEIGHT` in `map_editor.py` if a different default is preferred.

Q: Should a map have a fixed or user-selected maximum size?
A: The editor currently limits each dimension to 256 tiles to keep maps and JSON files practical.

Q: Should map names allow spaces or non-ASCII characters?
A: Names are currently limited to eight ASCII letters, digits, underscores, or hyphens so each map can be saved as a simple JSON filename.

Q: Should empty map cells be supported?
A: New maps currently fill with tileset coordinate (0, 0); painting always places a valid tileset tile.

Q: Which map file schema should the game loader consume?
A: Maps are saved as JSON with `format_version`, `name`, `width`, `height`, `tile_size`, and `tiles`; each tile is a `[x, y]` coordinate in `gfx/tiles_terrain.png`. The game does not load this schema yet.

Q: How should the editor be started and operated?
A: Run `python map_editor.py` from `modules/Tank-16`. NEW opens the name/width/height form; Enter creates the map. SAVE writes or replaces `maps/<NAME>.json`. LOAD lists saved maps from that folder. TILESET shows the fixed tileset location. Select a tile in the left column and click map cells to paint. Use the mouse wheel over the tile list to scroll tiles, WASD to pan, and 1/2/3 or +/- to change zoom.
