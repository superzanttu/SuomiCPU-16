# Map editor questions

Q: What default dimensions should a newly created map use?
A: The editor currently starts maps at 10×8 tiles. Change `MAP_DEFAULT_WIDTH` and `MAP_DEFAULT_HEIGHT` in `map_editor.py` if a different default is preferred.

Q: Should a map have a fixed or user-selected maximum size?
A: The editor limits each dimension to 128 tiles, matching the SC-16 tilemap hardware limit.

Q: Should map names allow spaces or non-ASCII characters?
A: Names are currently limited to eight ASCII letters, digits, underscores, or hyphens so each map can be saved as a simple JSON filename.

Q: Should empty map cells be supported?
A: New maps currently fill with tileset coordinate (0, 0); painting always places a valid tileset tile.

Q: Which map file schema should the game loader consume?
A: Maps use JSON fields `format_version`, `name`, `width`, `height`, `tile_size`, `tileset`, and `tiles`. Each terrain tile is a `[x, y]` coordinate in the named `gfx/tiles_*.png` atlas. Optional `obstacles` and `objects` grids have the same dimensions; `null` means empty. An obstacle cell is `[tile_x, tile_y]`. An object cell is `{"tile": [tile_x, tile_y], "hp_change": -25, "removal": "explosion"}`. Both use `objects_tileset: "tiles_objects.png"`. Legacy maps load with empty overlay layers. Start the emulator with a map JSON file to compile that map and its tilesets into Tank-16.

Q: How should the editor be started and operated?
A: Run `python map_editor.py` from `modules/Tank-16`. NEW opens the name/width/height form; Enter creates the map. SAVE writes or replaces `maps/<NAME>.json`. LOAD lists saved maps from that folder. TILESET shows the fixed tileset location. Select a tile in the left column and click map cells to paint. Use the mouse wheel over the tile list to scroll tiles, WASD to pan, and 1/2/3 or +/- to change zoom.

Q: How are obstacle and object layers edited?
A: Press G for ground, O for objects, or B for obstacles, or click the layer button to cycle layers. The layer button shows the active layer and its shortcut. Shortcuts are disabled while a dialog is open; 1/2/3 still control zoom. All layers remain visible. Left-click paints, right-click erases an overlay cell, and middle-click picks an overlay cell's sprite and properties. On OBJECTS, click HP to enter a signed change from -100 to +100, and click the removal button to cycle its behavior. These brush properties are saved per painted cell; pick an existing object, change the brush properties, and paint it again to update it.

Q: What do the object removal modes mean?
A: `fixed` remains and applies its HP change once per tank entry, rearming when that tank leaves. `explosion` is consumed on first contact and produces a short visual/sound explosion. `disappear` is consumed without that effect. HP is clamped to 0-100. Consumed objects and effects reset each round. Obstacle cells block tanks and shots. The old hardcoded blocks and mines are no longer placed.

Q: How are obstacle and object sprites scaled?
A: Source cells are always 64x64 from the top-left of `gfx/tiles_objects.png`. The game smoothly scales them to `MAP_TILE_PIXEL_SIZE` and shares the terrain/object color palette. Only used object cells are uploaded, with a combined terrain/object GPU limit of 256 tile images. The existing object atlas is 387x395 and not grid-aligned: partial edge cells are transparently padded, but artwork crossing cell borders is cut. Rearrange the source artwork into 64x64 cells for clean individual sprites; the editor does not guess sprite boundaries.

Q: How long do tank tracks remain?
A: Moving tanks leave two ground tracks, including when reversing. They are world-space marks visible in either camera, below objects and tanks. The latest 96 pairs remain until replaced or a new round starts; stationary tanks do not create marks.
