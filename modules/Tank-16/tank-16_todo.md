# Tank-16 TODO List

## Completed
### Gameplay
- [x] Implement the game in SC-16 C in `main.c` for a 640×480 display.
- [x] Support two players in a top-down tank battle; start Player 1 on the left and Player 2 on the right.
- [x] Keep tanks within screen bounds and let players drive, avoid obstacles, and fire projectiles.
- [x] Allow tanks to slide along obstacle edges when a diagonal movement is blocked.
- [x] Map controls: Player 1 uses A/D to turn, W/S to adjust throttle, and C to fire; Player 2 uses J/L to turn, I/K to adjust throttle, and period to fire.
- [x] Give each player one life and 100 health; apply 10 damage for an ammunition hit, 25 for a mine, and 5 for a tank collision.
- [x] End a round when a player's health reaches zero.
- [x] Render terrain only for the cells visible in each camera view.
- [x] Load finite JSON maps and seeded, effectively unbounded terrain from the module's `disks/maps/` folder through the generic file API.
- [x] Keep map and terrain/object pixel data out of the executable; load map JSON and the tile-atlas JSON from the module disk at runtime.
- [x] Store finite runtime maps as compact flat terrain indices and sparse object layers so large map JSON files fit the SC-16 heap.
- [x] Randomly select a valid JSON map from `disks/maps/` when the game starts and at the beginning of every new round; keep map data out of the executable.
- [x] Preserve the tileset's source RGB colors through per-program palette entries.
- [x] Convert terrain and object PNG atlases into a JSON manifest and per-tile JSON pixel files, then upload only map/game-used tiles from disk on demand.
- [x] Use 64×64 cells in `gfx/tiles_objects.png` and `gfx/tiles_terrain.png` as the source sprite/tile size.
- [x] Scale 64×64 terrain atlas cells to 16×16 runtime tiles when preparing map assets.
- [x] Provide `sprite_viewer.py` to inspect 64×64 cells, atlas coordinates, and row-major IDs.
- [x] Provide `gfx/tiles_template.png` as a transparent, numbered 64×64-cell atlas template.
- [x] Precalculate 32 tank sprite rotations and projectile directions.
- [x] Show both players' health and the game timer and win count in the HUD.
- [x] Show the active map name beneath the timer in the existing top bar.
- [x] Show a centered minimap below the unchanged 30-pixel HUD bar, across both viewports.
- [x] Keep the visible map area below the HUD bar and make wall/building objects block tanks.
- [x] Use the Tank-16 military-stencil font in the existing SC-16 5x7 font format.
- [x] Apply terrain-dependent tank speed and deep-water damage.
- [x] Limit each round to five minutes; decide the winner by remaining health and declare a draw on equal health.
- [x] Start a new round automatically after each round and retain wins; reset the win count when the game restarts.
- [x] Add short muzzle flashes, shot-impact sparks, damage flashes, destruction bursts, and shooting/hit/healing sounds.
- [x] Show reload progress below health bars, warn on low health, and separate the two camera views with a divider.
- [x] Show round results and the next-round countdown inside each player's viewport.
- [x] Use opaque-pixel collision masks for rotated tanks, obstacles, map objects, and ammunition; sweep moving shots to detect thin obstacles.
- [x] Draw larger 7x7 ammunition with a white core and yellow outline, using the same shape for collision detection.
- [x] Emit moving sparks and rising smoke from explosions and impacts, and flames/smoke from destroyed tanks during the round countdown.
- [x] Keep burning wrecks and ammunition impact fires throughout ten subsequent rounds.
- [x] Add inertial throttle, reverse, slower moving turns, recoil, blast knockback, and per-player camera shake.
- [x] Retain three times as many track marks and show forward/reverse throttle in the HUD.
- [x] Add timed health, speed, and shot-damage power-ups.
- [x] Add periodic tank movement sounds.

## Driving and battle reactions

Hold W to increase signed throttle; hold S to decrease it. Releasing both retains
the setting, so the tank cruises instead of stopping immediately. S brakes a
forward-moving tank through zero into reverse; W brakes reverse travel back into
forward. Player 2 uses I/K identically. Both throttle keys together leave the
setting unchanged. Speed approaches the setting gradually. Forward speed is
capped at 2 pixels/update; reverse at 1 pixel/update. Values use `SPEED_SCALE`
subpixel units. The HUD shows throttle and REV when reversing.

One 32-direction steering step varies with absolute speed: 2 updates at rest,
3 at half forward speed, and `MOVING_TURN_FRAMES` (4) at full forward speed.
`STATIONARY_TURN_FRAMES` (2) makes stationary pivoting twice as fast as full-speed
turning. A+D (J+L for Player 2) is a handbrake: it overrides throttle input,
resets gas to zero, prevents turning, and sheds `HANDBRAKE_DECELERATION` (8)
speed units per update. A full-speed tank stops within four updates and stays
stopped after release until throttle is applied again. The HUD displays BRAKE.
Collision checks use the new orientation
and sweep every pixel of translation, including recoil and blast displacement.
Blocked motion loses speed rather than storing up acceleration against a wall.

Firing produces backwards recoil and muzzle smoke. Shell hits push in the shot's
direction. Mines and destruction blasts push nearby surviving tanks away with
distance falloff and reduce their speed; existing HP amounts remain unchanged.
Impulses decay, stop at solid obstacles/map bounds, and briefly shake only the
affected player's camera. Fresh rounds clear throttle, impulses, and camera shake.

## Collision and particle effects

Collision masks use the actual uploaded sprite/tile pixels: palette index zero is
transparent. Map obstacle cells block tanks and ammunition only where opaque
pixels overlap. Wall atlas tiles 0-5 and building tiles 6-8 and 12-14 in the
object layer also block tanks; ammunition already collides with active map
objects. Tank rotation is rejected if the new orientation overlaps an obstacle
or the other tank. Ground tiles and cosmetic tracks, bursts, and particles are
not colliders.

Ammunition collides with opposing tanks, obstacles, active map objects, and other
ammunition. Shooting an object consumes the shot; fixed objects remain, explosion
objects are removed with an explosion, and disappear objects are removed without
an object explosion. Object HP changes still apply only on tank contact.

Transient particles use a fixed pool of `MAX_PARTICLES` (80) entries. New emissions
replace the oldest transient particles when full. Explosions eject randomized
sparks, gravity-affected debris, hot flame, and expanding smoke. Sparks cool from
white through yellow/orange to dark red. Smoke and flames rise with turbulence;
soft dithered circular particles fade instead of drawing solid squares. Spark
lifetimes vary up to `PARTICLE_FRAMES` (36) updates; smoke lasts up to 60.

Persistent fires are separate from that transient pool. Every ammunition collision
leaves a small fire and scorch at its world impact point. Destroyed tanks leave a
charred silhouette at their final position/orientation with larger fire plumes.
These emitters animate through countdowns and new rounds. `BURN_ROUNDS` (10) means
ten complete *subsequent matches*, not seconds or frames; an emitter is retired at
the start of the eleventh subsequent match. Burn sites are cosmetic, do not spread,
and do not add collision damage or block spawn locations. Map JSON is not modified.
R starts a fresh battle, clearing both accumulated burns and the win count.

The burn-site capacity covers the maximum possible shell impacts and two destroyed
tanks per match over the entire eleven-match retention window. Live sites are never
evicted for new effects; an unexpected overflow is reported in the HUD. GPU rendering
culls off-screen emitters, merges coincident drawing, and shares dense smoke plumes
without discarding individual impact fires. Starting a round clears only transient
particles. Both camera views clip effects to the viewport and map boundaries.
Round-result banners stay near the bottom so centered wreck fires remain visible.

`MAX_TRACK_MARKS` is 288 (previously 96), at the same spacing: visible trail history
is three times longer. Track marks clear between rounds as before.

The HUD bar remains 30 pixels tall. The minimap is centered immediately below it
and rendered on the full-screen HUD layer, spanning both camera views. Finite-map
minimap rendering samples already-loaded rows only, so it does not defeat lazy
map loading. On infinite maps it shows a local region around both tanks. The
camera keeps the play area below the top bar at finite-map edges.

Every tank samples nine terrain locations across its footprint. Water limits
forward/reverse speed to 10% of the vehicle maximum; grass and sand limit it to
70%; paved roads and road bridges allow the full maximum. Mixed terrain blends
these limits by sample count. On dark-water tile 90, five or more of the nine
samples trigger 10 HP damage per second; leaving deep water resets the damage
timer. Map loading and saving automatically mark water more than two tiles from
land as deep water, while preserving shoreline tiles near land and not treating
road or rail bridges as land.

The graphics API exposes `gfx_mask_overlap(first, first_x, first_y, second,
second_x, second_y)` with top-left world coordinates. Sprite mask IDs are 0-63;
tile masks are `GFX_MASK_TILE_BASE + tile_id`; `GFX_MASK_AMMO` selects the 7x7
projectile diamond. The result is 1 only for overlapping nonzero pixels.
`gfx_world_ammo(x, y)` draws the same diamond centered at world coordinates on the
selected non-background layer through both cameras.

`gfx_world_rect(x, y, width, height, color)` draws clipped world rectangles.
`gfx_world_particle(x, y, radius, color, opacity)` draws a soft circular particle
with radius 0-32 and dither opacity 0-16. `gfx_world_emitters(sites, count, tick)`
renders an array of four-word records: signed world x/y, positive remaining round
count (zero means inactive), and sprite mask ID (`-1` for a small fire without a
wreck). These commands require a non-background layer. The game owns persistence;
the GPU uses the animation tick and stable seeds to reconstruct rising flame and
smoke particles without allocating one guest particle pool per old fire.

## 64x64 atlas cells

Run `sprite_viewer.py` to view either atlas. Use Tab to switch atlases, +/- to zoom, and click a cell to print its atlas name, ID, and coordinates. IDs are numbered from zero, left-to-right and then top-to-bottom within the selected atlas. The viewer pads incomplete edge cells for display.

The Python helpers `sprite_id_at(image, x, y)`, `sprite_bounds(image, sprite_id)`, and `extract_sprite(image, sprite_id)` map between coordinates, IDs, and 64×64 images. The ID is the source cell index; C code still has to upload the selected pixel data to the GPU's tile or sprite slot.

In `main.c`, `TILE_*` defines name the tile IDs stored in the map, while `TANK_SPRITE_*_BASE` defines the sprite-slot ranges used by the two players' rotated tanks. Use these names instead of raw IDs when referring to the game's tile and sprite slots.

Both atlas images use aligned 64×64 cells. The object atlas is a 6×8 grid; its final cells are aligned forest tiles, and the health, damage, and speed power-ups use row 2, columns 3-5. The current game API stores runtime terrain tiles at the configured map-tile size and sprite images up to 64×64.

## Timed power-ups and movement audio

One power-up appears after five seconds, then another can appear 12 seconds after
the previous one is collected or expires. A power-up remains for 12 seconds and
spawns on a clear map tile. Health restores up to 25 points immediately; speed
raises forward and reverse limits by 50%; damage doubles shot damage. Speed and
damage boosts last ten seconds and reset each round. Pickup icons use the health,
ammunition, and blue shield/speed cells in the object atlas. While moving, each
tank emits a quiet short engine tone every 12 frames, with pitch tied to speed.

## Map editor and map format Q&A

Q: What default dimensions should a newly created map use?
A: The editor currently starts maps at 10×8 tiles. Change `MAP_DEFAULT_WIDTH` and `MAP_DEFAULT_HEIGHT` in `map_editor.py` if a different default is preferred.

Q: Should a map have a fixed or user-selected maximum size?
A: Each dimension can be 1-1024 tiles in both the editor and game. Zoom 3 uses a cached overview for large maps so the entire map remains visible; painting any layer invalidates that preview. Large maps save compact JSON rather than expanding each coordinate across several lines.

Q: Should map names allow spaces or non-ASCII characters?
A: Names are currently limited to eight ASCII letters, digits, underscores, or hyphens so each map can be saved as a simple JSON filename.

Q: Should empty map cells be supported?
A: New maps currently fill with tileset coordinate (0, 0); painting always places a valid tileset tile.

Q: Which map file schema should the game loader consume?
A: Finite maps use JSON fields `format_version`, `name`, `width`, `height`, `tile_size`, `tileset`, and `tiles`. Each terrain tile is a `[x, y]` coordinate in the named `gfx/tiles_*.png` atlas. Optional `obstacles` and `objects` grids have the same dimensions; `null` means empty. An obstacle cell is `[tile_x, tile_y]`. An object cell is `{"tile": [x, y], "hp_change": -25, "removal": "explosion"}`. Runtime maps live under `disks/maps/`; Tank-16 reads them through the module-scoped file API. Starting a round first loads `FINLAND.json`, then randomly chooses from the JSON files in that folder before each later round; map dimensions may differ. The editor reads and writes the same `disks/maps/` files; there is no separate editor copy.

Q: How can a map describe an effectively unbounded world?
A: Set `"infinite": true`, then provide an integer `"seed"`, a nonempty `"terrain_tiles"` list of row-major terrain atlas IDs, and optional `"obstacle_density"` and `"obstacle_tiles"` values. Terrain and obstacle choices are deterministic from the seed and signed world tile coordinates, so the same location is stable across camera views and rounds without storing a tile grid. Density `73`, for example, places an obstacle at roughly one in every 73 cells. `disks/maps/INFINITE.json` is a working example.

Q: How should the editor be started and operated?
A: Run `python map_editor.py` from `modules/Tank-16`. NEW opens the name/width/height form; Enter creates the map. SAVE writes or replaces `disks/maps/<NAME>.json`, which the game reads. LOAD lists maps from `disks/maps/`. TILESET shows the fixed tileset location. Select a tile in the left column and click map cells to paint. Use the mouse wheel over the tile list to scroll tiles, WASD to pan, and 1/2/3 or +/- to change zoom.

Q: How are obstacle and object layers edited?
A: Press G for ground, O for objects, or B for obstacles, or click the layer button to cycle layers. The layer button shows the active layer and its shortcut. Shortcuts are disabled while a dialog is open; 1/2/3 still control zoom. All layers remain visible. Left-click paints, right-click erases an overlay cell, and middle-click picks an overlay cell's sprite and properties. On OBJECTS, click HP to enter a signed change from -100 to +100, and click the removal button to cycle its behavior. These brush properties are saved per painted cell; pick an existing object, change the brush properties, and paint it again to update it.

Q: What do the object removal modes mean?
A: `fixed` remains and applies its HP change once per tank entry, rearming when that tank leaves. `explosion` is consumed on first contact and produces a short visual/sound explosion. `disappear` is consumed without that effect. HP is clamped to 0-100. Consumed objects and effects reset each round. Obstacle cells block tanks and shots. The old hardcoded blocks and mines are no longer placed.

Q: How are obstacle and object sprites scaled?
A: Source cells are always 64x64 from the top-left of `gfx/tiles_objects.png`. The game smoothly scales them to `MAP_TILE_PIXEL_SIZE` and shares the terrain/object color palette. Only used object cells are uploaded, with a combined terrain/object GPU limit of 256 tile images. The transparent 384x512 atlas uses row 0 for horizontal and vertical walls plus all four wall corners; rows 1-2, columns 0-2, form a 3x3 building set with four corners, four edges, and an interior. Powerups are at row 2, columns 3-5, and row 3, columns 0-2; five increasing mine sizes are at row 3, columns 3-5, and row 4, columns 0-1; rocks fill row 4, columns 2-5; single trees fill row 5. Forest construction tiles fill rows 6-7: the first nine cells are the four outer corners, four edges, and center of a forest layout, followed by three additional forest fills. Run `python modules/Tank-16/generate_object_tileset.py` from the repository root to regenerate it.

Q: How long do tank tracks remain?
A: Moving tanks leave two ground tracks, including when reversing. They are world-space marks visible in either camera, below objects and tanks. The latest 288 pairs remain until replaced or a new round starts; stationary tanks do not create marks.

Q: Where are the water, bridge, and train track tiles?
A: `gfx/tiles_terrain.png` is 10 columns by 10 rows of 64x64 cells. Its original 40 cells (rows 0-3) are unchanged. IDs 40-55 are irregular shoreline variants; add north/east/south/west land-neighbor bits 1/2/4/8 to 40 to choose the shoreline shape. ID 40, coordinate (0, 4), is open water; IDs 56-59 are additional open-water ripple variants. ID 90, coordinate (0, 9), is dark-blue deep water. Loading or saving a map automatically uses it for water more than two tiles from land; road and rail bridges do not count as land for this distance. IDs 60-69 are wooden and stone bridges in horizontal/vertical orientations, including end-cap and plank variants. IDs 70-85 are railroad track tiles, where ID = 70 plus the north/east/south/west connection bits 1/2/4/8; adjacent connections curve naturally. IDs 86-89 are wooden and stone rail bridges in horizontal/vertical orientations. Tile coordinates remain zero-based and IDs remain row-major.

Q: Which font does Tank-16 use?
A: Tank-16 uses `../../fonts/tank16_military5x7.asm`, a military-stencil font
with the same 5×7 glyph bitmap, 6×8 cell, and glyph ordering as the standard
SC-16 font. Its local `font5x7.asm` wrapper selects this font only when compiling
Tank-16.

Q: What geography is used by `disks/maps/FINLAND.json`?
A: It is a 100x100 north-up Finland overview rasterized from Natural Earth's public-domain 1:10m country boundaries, lakes, roads, railways, and populated places. It includes irregular shorelines, road and railway connection tiles, road/rail bridges over water, city blocks, forest trees, and rocks. The square uses a Lambert azimuthal equal-area projection centered on 26E, 64.5N and covers Finland including Aland with a surrounding margin. At this overview scale, routes show major connections only and are not navigational data. Source URLs, projection bounds, scale, feature counts, cities, and tile counts are stored in the optional `geography` metadata, which the editor preserves.

Q: How can the Finland map and terrain art be regenerated?
A: Run `python modules/Tank-16/generate_finland_map.py` from the repository root with `--countries`, `--lakes`, and `--roads` source paths. Optional `--railways` and `--cities` paths use Natural Earth's `ne_10m_railroads.geojson` and `ne_10m_populated_places.geojson`; without them, the generator uses built-in Finnish rail routes and city locations. Source data is public domain and available from `https://github.com/nvkelso/natural-earth-vector/tree/master/geojson`. The default output is `modules/Tank-16/disks/maps/FINLAND.json` at 100x100 tiles, read directly by the game at runtime. Runtime map files are limited to 10,000 cells. `--preview <image.png>` writes an overview. `--extend-atlas` builds or refreshes water, bridge, and rail tiles while preserving the original 40 cells.

Q: Which maps are selected during play?
A: At startup and at the beginning of each new round, Tank-16 randomly selects a `.json` map from `disks/maps/` and loads it through the runtime disk API. Map JSON is not embedded in the compiled program. Ten additional natural maps are generated in `disks/maps/` by running `python modules/Tank-16/generate_natural_maps.py` from the repository root.

Q: How does the game address maps larger than 32767 pixels?
A: World positions use 32-bit C `long` coordinates. Maps whose pixel extents exceed signed 16-bit coordinates use the wide graphics/collision APIs; smaller maps retain the existing narrow calls. At 1024x1024 cells and `MAP_TILE_PIXEL_SIZE` 64, each axis spans 65536 pixels. The loader pages terrain bytes in 32768-byte blocks and obstacles in 32-row pages to avoid truncated C array indices. Empty large obstacle layers do not allocate obstacle pages. GPU rendering still visits only visible tiles. This is runtime support, not just an editor validation change; the general compiler's large-array indexing limitation remains, and generated global data has a 16 MiB limit.
