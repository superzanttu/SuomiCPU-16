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
- **Transport Robot**: Transports items from mines or stocked buildings to another building. Select Route mode, click the source building, then click a non-mine destination.

#### Robot Specifications
- **Movement**: Robots are autonomous units that follow the instructions given by the player.
- **Task Execution**: Robots execute transport tasks by moving between the defined source and target locations.
- **Capacity**: Robots can carry up to 5 items. (Limited to fit within register/memory constraints).
- **Energy**: Robots don't consume energy.
- **Speed**: Movement speed is 2 tiles per second. Load/unload speed is 0.5 seconds per item.
- **Collision**: Robots use breadth-first search to avoid rocks, water, other buildings, and other active robots. They retry their path on each movement step.
- **Fleet**: Routes use an existing idle robot when possible and otherwise create a robot, up to a limit of eight.
- **Buffers**: Mines hold up to 5 items; other buildings hold up to 20 items of one resource type. A robot waits if its destination is full or contains another resource type.

## 3. World & Map
### 3.1 Map Generation
- **Map Type**: A compact 48x48 tile map described by irregular terrain and resource features rather than a full tile array.

#### Generation Rules
- **Terrain**: The map includes irregular rock formations, a winding river, a pond, and clustered resource deposits. Feature edges and positions vary between runs to avoid square, repetitive layouts.
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

#### Recipe List
- **Iron Plate**: 1 Iron Ore -> 1 Iron Plate.
- **Copper Plate**: 1 Copper Ore -> 1 Copper Plate.
- **Rombonent**: 1 Iron Plate + 2 Copper Plate -> 1 Rombonent.
- **Robot**: 1 Rombonent + 1 Iron Plate -> 1 Robot.

### 4.3 Costs

#### Building Costs
- **All Buildings**: Free (Prototype version).

## 5. Technical Systems
### 5.1 Pathfinding

#### Pathfinding Algorithm
- **Algorithm**: Robots run breadth-first search when selecting each next movement tile.
- **Dynamic Obstacles**: The route is recalculated before each move so newly blocked paths are handled.

### 5.2 Save/Load System
- **Status**: Save/load is not implemented in the current prototype.
    
## 6. User Interface (GUI)
### 6.1 Map View
- **View**: Zoomable map view with three discrete zoom levels.

#### Map Navigation
- **Panning**: Right-click and drag or hold WASD keys.
- **Zooming**: Mouse wheel scroll, anchored to the pointer over the map.

### 6.2 Interaction & Controls
- **Input**: Full mouse support for navigation and interaction.

#### Control Scheme
- **Left Click**: Select building / Place building.
- **Right Click**: Cancel action / Pan map.

### 6.3 HUD & Feedback

#### HUD Elements
- **Resource Bar**: Top of screen showing totals of Coal, Iron Ore, Copper Ore, Iron Plate, and Copper Plate.
- **Robot Status**: Bottom toolbar showing active robots versus the total fleet.
- **Mini-map**: Bottom right showing terrain landmarks, buildings, active robots, and the approximate map-view center.

#### Feedback
- **Console**: Reports build-mode changes, selections, route assignments, and placement errors.

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
- **Global Storage**: Global variables start at `0xE800`.
- **Memory Limit**: Total available memory is 512 KiB. The current compact 48x48 feature map fits in RAM; larger worlds may need chunk streaming or external storage.
- **Stack**: Local variables and function calls use the machine stack (starts at `0x2FFFF`).

### 8.2 Execution & Timing
- **Frame Budget**: The emulator executes up to 30,000 instructions per frame (capped at 60 FPS). Robot pathfinding runs synchronously before each movement step; longer routes may take multiple instruction windows to render.
- **Precision**: Floating point values use IEEE-754 binary16 (half precision).

### 8.3 Graphics & I/O
- **Resolution**: 320x240 pixels, 8-bit palette.
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
resource bar includes items carried by robots.

Controls:
- **WASD**: Pan the map; holding a key repeats every 0.15 seconds.
- **Right mouse drag**: Pan the map; right-click without dragging cancels the
  current build tool.
- **Mouse wheel**: Zoom in or out at the pointer across three levels.
- **Bottom buttons**: Choose Mine, Factory, Storage, Select, or Route mode.
- **Tab**: Cycle between all five modes.
- **Left click**: Place the selected building, or select an existing building.
- **Route mode**: Click a mine or stocked building, then a different
  non-mine building to assign an available robot.
- **Minimap click**: Center the map view on that location.
- **Console**: Displays build-mode changes, selections, placement results, and
  placement errors; no transient notification log is drawn over the map.

Factory recipes, save/load, and the remaining progression systems are not
implemented yet.
