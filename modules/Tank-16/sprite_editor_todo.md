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

Q: Where are the water, bridge, and train track tiles?
A: `gfx/tiles_terrain.png` is 10 columns by 9 rows of 64x64 cells. Its original 40 cells (rows 0-3) are unchanged. IDs 40-55 are irregular shoreline variants; add north/east/south/west land-neighbor bits 1/2/4/8 to 40 to choose the shoreline shape. ID 40, coordinate (0, 4), is open water; IDs 56-59 are additional open-water ripple variants. IDs 60-69 are wooden and stone bridges in horizontal/vertical orientations, including end-cap and plank variants. IDs 70-85 are railroad track tiles, where ID = 70 plus the north/east/south/west connection bits 1/2/4/8; adjacent connections curve naturally. IDs 86-89 are wooden and stone rail bridges in horizontal/vertical orientations. Tile coordinates remain zero-based and IDs remain row-major.

Q: What geography is used by `maps/FINLAND.json`?
A: It is a 100x100 north-up Finland overview rasterized from Natural Earth's public-domain 1:10m country boundaries, lakes, roads, railways, and populated places. It includes irregular shorelines, road and railway connection tiles, road/rail bridges over water, city blocks, forest trees, and rocks. The square uses a Lambert azimuthal equal-area projection centered on 26E, 64.5N and covers Finland including Aland with a surrounding margin. At this overview scale, routes show major connections only and are not navigational data. Source URLs, projection bounds, scale, feature counts, cities, and tile counts are stored in the optional `geography` metadata, which the editor preserves.

Q: How can the Finland map and terrain art be regenerated?
A: Run `python modules/Tank-16/generate_finland_map.py` from the repository root with `--countries`, `--lakes`, and `--roads` source paths. Optional `--railways` and `--cities` paths use Natural Earth's `ne_10m_railroads.geojson` and `ne_10m_populated_places.geojson`; without them, the generator uses built-in Finnish rail routes and city locations. Source data is public domain and available from `https://github.com/nvkelso/natural-earth-vector/tree/master/geojson`. The default output is `modules/Tank-16/maps/FINLAND.json` at 100x100 tiles and is also copied to `modules/Tank-16/disks/maps/FINLAND.json` for runtime loading. Runtime map files are limited to 10,000 cells. `--preview <image.png>` writes an overview. `--extend-atlas` builds or refreshes water, bridge, and rail tiles while preserving the original 40 cells.

Q: Which maps are selected during play?
A: At startup and at the beginning of each new round, Tank-16 randomly selects a `.json` map from `disks/maps/` and loads it through the runtime disk API. Map JSON is not embedded in the compiled program. Ten additional natural maps are generated in both `maps/` and `disks/maps/` by running `python modules/Tank-16/generate_natural_maps.py` from the repository root.

Q: How does the game address maps larger than 32767 pixels?
A: World positions use 32-bit C `long` coordinates. Maps whose pixel extents exceed signed 16-bit coordinates use the wide graphics/collision APIs; smaller maps retain the existing narrow calls. At 1024x1024 cells and `MAP_TILE_PIXEL_SIZE` 64, each axis spans 65536 pixels. The loader pages terrain bytes in 32768-byte blocks and obstacles in 32-row pages to avoid truncated C array indices. Empty large obstacle layers do not allocate obstacle pages. GPU rendering still visits only visible tiles. This is runtime support, not just an editor validation change; the general compiler's large-array indexing limitation remains, and generated global data has a 16 MiB limit.
