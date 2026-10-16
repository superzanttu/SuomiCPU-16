# Map editor questions

Q: What default dimensions should a newly created map use?
A: The editor currently starts maps at 10×8 tiles. Change `MAP_DEFAULT_WIDTH` and `MAP_DEFAULT_HEIGHT` in `map_editor.py` if a different default is preferred.

Q: Should a map have a fixed or user-selected maximum size?
A: Each dimension can be 1-1024 tiles in both the editor and game. Zoom 3 uses a cached overview for large maps so the entire map remains visible; painting any layer invalidates that preview. Large maps save compact JSON rather than expanding each coordinate across several lines.

Q: Should map names allow spaces or non-ASCII characters?
A: Names are currently limited to eight ASCII letters, digits, underscores, or hyphens so each map can be saved as a simple JSON filename.

Q: Should empty map cells be supported?
A: New maps currently fill with tileset coordinate (0, 0); painting always places a valid tileset tile.

Q: Which map file schema should the game loader consume?
A: Finite maps use JSON fields `format_version`, `name`, `width`, `height`, `tile_size`, `tileset`, and `tiles`. Each terrain tile is a `[x, y]` coordinate in the named `gfx/tiles_*.png` atlas. Optional `obstacles` and `objects` grids have the same dimensions; `null` means empty. An obstacle cell is `[tile_x, tile_y]`. An object cell is `{"tile": [x, y], "hp_change": -25, "removal": "explosion"}`. Runtime maps live under `disks/maps/`; Tank-16 reads them through the module-scoped file API. Starting a round first loads `FINLAND.json`, then randomly chooses from the JSON files in that folder before each later round; map dimensions may differ. The editor keeps its working copies in `maps/` and synchronizes saves to `disks/maps/`.

Q: How can a map describe an effectively unbounded world?
A: Set `"infinite": true`, then provide an integer `"seed"`, a nonempty `"terrain_tiles"` list of row-major terrain atlas IDs, and optional `"obstacle_density"` and `"obstacle_tiles"` values. Terrain and obstacle choices are deterministic from the seed and signed world tile coordinates, so the same location is stable across camera views and rounds without storing a tile grid. Density `73`, for example, places an obstacle at roughly one in every 73 cells. `disks/maps/INFINITE.json` is a working example.

Q: How should the editor be started and operated?
A: Run `python map_editor.py` from `modules/Tank-16`. NEW opens the name/width/height form; Enter creates the map. SAVE writes or replaces `maps/<NAME>.json` and synchronizes it to `disks/maps/<NAME>.json`, which the game reads. LOAD lists editor maps from `maps/`. TILESET shows the fixed tileset location. Select a tile in the left column and click map cells to paint. Use the mouse wheel over the tile list to scroll tiles, WASD to pan, and 1/2/3 or +/- to change zoom.

Q: How are obstacle and object layers edited?
A: Press G for ground, O for objects, or B for obstacles, or click the layer button to cycle layers. The layer button shows the active layer and its shortcut. Shortcuts are disabled while a dialog is open; 1/2/3 still control zoom. All layers remain visible. Left-click paints, right-click erases an overlay cell, and middle-click picks an overlay cell's sprite and properties. On OBJECTS, click HP to enter a signed change from -100 to +100, and click the removal button to cycle its behavior. These brush properties are saved per painted cell; pick an existing object, change the brush properties, and paint it again to update it.

Q: What do the object removal modes mean?
A: `fixed` remains and applies its HP change once per tank entry, rearming when that tank leaves. `explosion` is consumed on first contact and produces a short visual/sound explosion. `disappear` is consumed without that effect. HP is clamped to 0-100. Consumed objects and effects reset each round. Obstacle cells block tanks and shots. The old hardcoded blocks and mines are no longer placed.

Q: How are obstacle and object sprites scaled?
A: Source cells are always 64x64 from the top-left of `gfx/tiles_objects.png`. The game smoothly scales them to `MAP_TILE_PIXEL_SIZE` and shares the terrain/object color palette. Only used object cells are uploaded, with a combined terrain/object GPU limit of 256 tile images. The transparent 384x512 atlas uses row 0 for horizontal and vertical walls plus all four wall corners; rows 1-2, columns 0-2, form a 3x3 building set with four corners, four edges, and an interior. Powerups are at row 2, columns 3-5, and row 3, columns 0-2; five increasing mine sizes are at row 3, columns 3-5, and row 4, columns 0-1; rocks fill row 4, columns 2-5; single trees fill row 5. Forest construction tiles fill rows 6-7: the first nine cells are the four outer corners, four edges, and center of a forest layout, followed by three additional forest fills. Run `python tools/generate_object_tileset.py` from the repository root to regenerate it.

Q: How long do tank tracks remain?
A: Moving tanks leave two ground tracks, including when reversing. They are world-space marks visible in either camera, below objects and tanks. The latest 288 pairs remain until replaced or a new round starts; stationary tanks do not create marks.

Q: Where are the water tiles?
A: `gfx/tiles_terrain.png` is now 10 columns by 6 rows of 64x64 cells. Its original 40 cells (rows 0-3) are unchanged. IDs 40-55 are water/shore variants; add the north/east/south/west land-neighbor bits 1/2/4/8 to 40 to choose the shoreline shape. ID 40, coordinate (0, 4), is open water. IDs 56-59 are additional open-water ripple variants. Tile coordinates remain zero-based and IDs remain row-major.

Q: What geography is used by `maps/FINLAND.json`?
A: It is a 100x100 north-up Finland overview rasterized from Natural Earth's public-domain 1:10m country boundaries, lakes, and major roads. Finland is grass, neighboring land is sand, seas and lakes use water cells with matching shore edges, and roads use the terrain atlas road tiles. The square uses a Lambert azimuthal equal-area projection centered on 26E, 64.5N and covers Finland including Aland with a surrounding margin. At this overview scale, roads show major routes only and are not street-level or navigational data. Source URLs, projection bounds, scale, road count, and tile counts are stored in the optional `geography` metadata, which the editor preserves.

Q: How can the Finland map and water art be regenerated?
A: Run `tools/generate_finland_map.py` from the repository root with `--countries <ne_10m_admin_0_countries.geojson> --lakes <ne_10m_lakes.geojson> --roads <ne_10m_roads.geojson>`. All three public-domain files are available from `https://github.com/nvkelso/natural-earth-vector/tree/master/geojson`. The default output is `modules/Tank-16/maps/FINLAND.json` at 100x100 tiles; `--size` supports 1-1024. `--preview <image.png>` writes an overview. Save the map with the editor or copy it to `modules/Tank-16/disks/maps/FINLAND.json` so the runtime file loader can read it. `--extend-atlas` appends the water cells if the atlas still has its original four rows; an already extended atlas is left untouched.

Q: How does the game address maps larger than 32767 pixels?
A: World positions use 32-bit C `long` coordinates. Maps whose pixel extents exceed signed 16-bit coordinates use the wide graphics/collision APIs; smaller maps retain the existing narrow calls. At 1024x1024 cells and `MAP_TILE_PIXEL_SIZE` 64, each axis spans 65536 pixels. The loader pages terrain bytes in 32768-byte blocks and obstacles in 32-row pages to avoid truncated C array indices. Empty large obstacle layers do not allocate obstacle pages. GPU rendering still visits only visible tiles. This is runtime support, not just an editor validation change; the general compiler's large-array indexing limitation remains, and generated global data has a 16 MiB limit.
