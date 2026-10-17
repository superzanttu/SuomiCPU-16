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
- [x] Keep map data out of the executable; compile only the terrain and object graphics assets.
- [x] Randomly select a valid JSON map from `disks/maps/` when the game starts and at the beginning of every new round; keep map data out of the executable.
- [x] Preserve the tileset's source RGB colors through per-program palette entries.
- [x] Use 64×64 cells in `gfx/tiles_objects.png` and `gfx/tiles_terrain.png` as the source sprite/tile size.
- [x] Scale 64×64 terrain atlas cells to 16×16 runtime tiles when preparing map assets.
- [x] Provide `sprite_viewer.py` to inspect 64×64 cells, atlas coordinates, and row-major IDs.
- [x] Provide `gfx/tiles_template.png` as a transparent, numbered 64×64-cell atlas template.
- [x] Precalculate 32 tank sprite rotations and projectile directions.
- [x] Show both players' health and the game timer and win count in the HUD.
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
transparent. Map obstacles block tanks and ammunition only where opaque pixels
overlap. Tank rotation is rejected if the new orientation overlaps an obstacle or
the other tank. Ground tiles and cosmetic tracks, bursts, and particles are not
colliders.

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
