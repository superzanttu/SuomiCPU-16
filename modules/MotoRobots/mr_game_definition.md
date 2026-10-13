# MotoRobots Game Definition

MotoRobots is a Factorio-type automation game focused on resource extraction, processing, and logistics using a fleet of transport robots.

**Target Platform**: SuomiCPU-16. The game is designed to fit within the architecture's constraints (16-bit words, limited memory, specific I/O capabilities) and is programmed in the SuomiCPU-16 C-language.

## 1. Core Gameplay Loop
Mines extract resources from ground. Robots transport resources from mines to factories, and from factories to storage. Player controls robots by defining source and target locations for each transport task.
The loop follows:
1. **Extraction**: Place Mines on ore deposits to gather raw materials.
2. **Logistics**: Command Robots to transport raw ores to Factories.
3. **Processing**: Factories refine raw ores into usable components.
4. **Storage**: Refined components are stored for future construction.
5. **Expansion**: Use stored components to build more Mines and Factories.


## 2. Entities
### 2.1 Buildings
- **Mine**: Produces raw resources (ores, coal, iron, or copper).
- **Factory**: Processes raw ores into refined materials.
- **Storage**: Stores resources and processed materials.
- **Base**: The central hub for the player, where robots are managed and resources are initially stored.

#### Building Construction
Buildings are constructed for free, because this is first prototype game.

### 2.2 Logistics
- **Transport Robot**: Transports items from mines or stocked buildings to another building. Factories accept iron/copper ore for plate refining and iron/copper plates for Rombonent assembly. Select Route mode, click the source building, then click a non-mine destination.

#### Robot Specifications
- **Movement**: Robots are autonomous units that follow the instructions given by the player.
- **Task Execution**: Robots execute transport tasks by moving between the defined source and target locations.
- **Capacity**: Robots can carry up to 5 items. (Limited to fit within register/memory constraints).
- **Energy**: Robots don't consume energy.
- **Speed**: Robots move one tile per second. Load/unload speed is 0.5 seconds per item.
- **Collision**: Robots plan a complete breadth-first route in small per-frame steps, avoiding rocks, water, buildings, and other active robots. A found route is retained and reversed for the return journey. If its next step is blocked or unusable, the robot discards it, waits one second, then asynchronously searches for a new route.
- **Busy destinations**: Multiple robots can enter the same destination tile, so a robot unloading there does not permanently prevent other robots from reaching it.
- **Fleet**: Routes use an existing idle robot when possible and otherwise create a robot, up to a limit of eight.
- **Buffers**: Mines hold up to 5 items; other buildings hold up to 20 items of one resource type. A robot waits if its destination is full or contains another resource type.

## 3. World & Map
### 3.1 Map Generation
- **Map Type**: A compact 48x48 tile map described by irregular terrain and resource features rather than a full tile array.

#### Generation Rules
- **Terrain**: The map includes irregular, impassable rock formations, a winding river, a pond, and clustered resource deposits. Deposits are passable ground and use a lighter gray for coal so they are visually distinct from rock. Feature edges and positions vary between runs to avoid square, repetitive layouts.
- **Placement**: Buildings can exist on ground tiles only. Mines can only be placed on ground tiles that contain the corresponding resource.

#### Biomes
- **Distribution**: Coal, iron ore, and copper ore appear in distinct clusters.
- **Availability**: Resources are infinite. Resource clusters are distributed evenly on map.

### 3.2 Tile Types
- **Ground**: Walkable by robots; suitable for building construction.
- **Rock**: Impassable for robots; no buildings allowed. Forms mountains.
- **Water**: Impassable for robots; no buildings allowed. Forms rivers and lakes.

## 4. Resources & Economy
### 4.1 Materials
- **Coal**: A basic fuel resource used in various production processes.
- **Iron Ore**: A raw material used to produce iron plates.
- **Copper Ore**: A raw material used to produce copper plates.
- **Iron Plate**: A refined material produced from iron ore.
- **Copper Plate**: A refined material produced from copper ore.    
- **Rombonent**: A basic component used in various production processes.

### 4.2 Production & Recipes

#### Resource Rates
| Resource     | Production Rate |
| ------------ | --------------- |
| Coal         | 1 unit/sec      |
| Iron Ore     | 1 unit/sec      |
| Copper Ore   | 1 unit/sec      |
| Iron Plate   | 1 unit/sec      |
| Copper Plate | 1 unit/sec      |
| Rombonent    | 1 unit/sec      |
| Robot        | 1 unit/sec when fleet capacity allows |

#### Recipe List
- **Iron Plate**: 1 Iron Ore -> 1 Iron Plate.
- **Copper Plate**: 1 Copper Ore -> 1 Copper Plate.
- **Rombonent**: 1 Iron Plate + 2 Copper Plate -> 1 Rombonent.
- **Robot**: 1 Rombonent + 1 Iron Plate -> 1 Robot.

Factories keep separate iron-plate and copper-plate ingredient buffers for
Rombonent assembly. Each factory assembles one Rombonent per second when it
has the required ingredients and room in its output buffer.
Factories use Rombonent and iron-plate inputs to add a new idle robot to the
fleet. Production waits when the eight-robot fleet limit is reached.

### 4.3 Costs

#### Building Costs
- **All Buildings**: Free (Prototype version).

## 5. Technical Systems
### 5.1 Pathfinding

#### Pathfinding Algorithm
- **Algorithm**: Breadth-first search records a complete route before a robot starts moving toward a source or target.
- **Incremental Search**: Path searches expand a bounded number of map tiles per game update so route finding does not stall rendering, input, or simulation.
- **Dynamic Obstacles**: Before each movement step, the robot checks whether its next tile remains clear. If blocked, it pauses and requests a new incremental path from its current position.

### 5.2 Save/Load System
- **Status**: Save/load is not implemented in the current prototype.
    
## 6. User Interface (GUI)
### 6.1 Map View
- **View**: Zoomable map view with three discrete zoom levels. 3 zoom levels are available for different levels of detail.

#### Map Navigation
- **Panning**: Right-click and drag or hold WASD keys.
- **Zooming**: Mouse wheel scroll, anchored to the pointer over the map.

### 6.2 Interaction & Controls
- **Input**: Full mouse support for navigation and interaction.
- **Keyboard**: WASD keys for panning the map.
- **Mouse**: Left click for selection and placement, right click for deletion.

#### Control Scheme
- **Left Click**: Select building / Place building.
- **Right Click**: Delete a building / Pan map by dragging.

### 6.3 HUD & Feedback

#### HUD Elements
- **Resource Bar**: Top of screen showing totals of Coal, Iron Ore, Copper Ore, Iron Plate, Copper Plate, and Rombonent.
- **Robot Status**: Bottom toolbar showing active robots versus the total fleet.
- **Status LED**: One character cell wide and blinking at approximately one hertz, synchronized to rendered frames.
- **Mini-map**: Bottom right showing terrain landmarks, buildings, active robots, and the approximate map-view center.

#### Feedback
- **Console**: Building references include stable IDs and coordinates. Robot messages report
  path searches, completed routes, arrivals, departures, load/unload start and end, and
  factory empty/full transitions. The emulator writes console output to `logs/console.log`,
  replacing the previous log at startup and reset/reboot.

## 7. Progression & Goals
### 7.1 Milestones
- **First Plate**: Produce the first Iron or Copper plate.
- **Logistics Hub**: Establish a fully automated loop from mine to storage.
- **Global Reach**: Expand the base to cover 3 different resource clusters.

### 7.2 Tech Tree / Upgrades

#### Progression System
- **Unlocks**: New robot capacities and building efficiencies are unlocked by collecting a specific total amount of refined materials.

### 7.3 Win/Loss Conditions
- **Goal**: Build a "Mega-Storage" that holds 10,000 units of total materials.
- **Loss**: No permanent loss conditions.

## 8. SuomiCPU-16 Specific Constraints
### 8.1 Memory Layout
- **Global Storage**: Compiler-managed global and static variables start at `0x80000`.
- **Memory Limit**: The emulator provides 1 GiB of byte-addressable memory. The current compact 48x48 feature map fits in RAM; larger worlds may still need chunk streaming or external storage for game-design or performance reasons.
- **Stack**: Local variables and function calls use the machine stack, which starts at `0x3FFFFFFF` and grows toward lower addresses.

### 8.2 Execution & Timing
- **Frame Budget**: The emulator runs at up to 30 FPS with a 30,000-instruction frame budget. Static terrain, buildings, and HUD layers are cached until the camera, selection, or toolbar changes. Robot pathfinding is cooperative: it expands at most 32 map tiles per game update.
- **Game Speed**: MotoRobots selects the emulator maximum-speed register to accelerate pathfinding and enables VSync so each presented game frame is paced at the display rate. This keeps robot movement at one tile per real-time second and loading/unloading at 0.5 seconds per item. While a search is active, discovered tiles are highlighted directly on the map: cyan for expanded tiles and yellow for the frontier. A found route is highlighted in magenta ahead of its moving robot.
- **Precision**: Floating point values use IEEE-754 binary16 (half precision).

### 8.3 Graphics & I/O
- **Resolution**: 320x240 pixels, 8-bit palette. Maximum framerate is 30.
- **Drawing**: All drawing is performed on a back buffer using `gfx_*` functions; `gfx_present()` is called once per frame to update the display.
- **Input**: Keyboard and mouse are accessed via memory-mapped registers or `gfx_*` helpers.

## 9. Current Prototype Status

The current prototype implements irregular rock formations, a winding river,
a pond, and clustered coal, iron, and copper deposits. It has a three-level
zoomable map, WASD and right-drag panning, a resource HUD, a landmark minimap,
and a bottom toolbar with Mine, Factory, Storage, Select, and Route modes.
Buildings can be placed for free. Mines extract one unit per second into a
five-item buffer. Robots can be assigned mine-to-building routes, carry up to
five items, and deliver them while avoiding terrain and active robots. The
resource bar includes items carried by robots and ingredients held by factory
buffers. Factories refine iron ore into iron plates and copper ore into copper
plates at one item per second; they process their current input batch before
accepting more deliveries. They also assemble Rombonents from one iron plate
and two copper plates per second when those ingredients and output space are
available. Factories use one Rombonent and one iron plate to manufacture a
new idle transport robot per second, up to the eight-robot fleet limit.
The renderer caches the static world and HUD layers between changes so normal
frames redraw only moving robots and changing status values.

Controls:
- **WASD**: Pan the map; holding a key repeats every 0.15 seconds.
- **Right mouse drag**: Pan the map.
- **Right click on a building**: Delete it. The base cannot be removed.
- **Mouse wheel**: Zoom in or out at the pointer across three levels.
- **Zoom steps**: Each wheel update moves one level, even for high-delta events.
- **Bottom buttons**: Choose Mine, Factory, Storage, Select, or Route mode.
- **Tab**: Cycle between all five modes.
- **Left click**: Place the selected building, or select an existing building.
- **Route mode**: Click a mine or stocked building, then a different
  non-mine building to assign an available robot. Factories accept iron/copper
  ore for refining, iron/copper plates for Rombonent assembly, and Rombonents
  for robot production.
- **Minimap click**: Center the map view on that location.
- **Console**: Displays build-mode changes, selections, placement results, and
  placement errors; no transient notification log is drawn over the map.

Building costs, save/load, and the remaining progression systems are not
implemented yet.
