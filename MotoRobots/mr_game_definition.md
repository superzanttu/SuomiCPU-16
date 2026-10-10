# MotoRobots Game Definition

MotoRobots is a Factorio-type automation game focused on resource extraction, processing, and logistics using a fleet of transport robots.

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
- **Transport Robot**: Transports items between mines, factories, and storage units. Uses a pathfinding algorithm to determine the optimal route. Robots are programmed by defining source and target locations for transport tasks.

#### Robot Specifications
- **Movement**: Robots are autonomous units that follow the instructions given by the player.
- **Task Execution**: Robots execute transport tasks by moving between the defined source and target locations.
- **Capacity**: Robots can carry up to 5 items.
- **Energy**: Robots don't consume energy.
- **Speed**: Movement speed is 2 tiles per second. Load/unload speed is 0.5 seconds per item.
- **Movement**: Robots can move forward and backward, turn left and right.
- **Collision**: Robots avoid obstacles and other robots to prevent collisions.

## 3. World & Map
### 3.1 Map Generation
- **Map Type**: Infinite procedurally generated tile map.

#### Generation Rules
- **Terrain**: Maps have lakes and rivers. Mountains are formed by clusters of rocks.
- **Placement**: Buildings can exist on ground tiles only. Mines can only be placed on ground tiles that contain the corresponding resource.

#### Biomes
- **Distribution**: Resources are formed on clustered or resource-rich ground tiles.
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
- **Algorithm**: Robots use A* to find routes on the map.
- **Dynamic Obstacles**: If a robot hits a blocked tile, it recalculates the path to avoid the obstacle.

### 5.2 Save/Load System
- **Persistence**: Game state, including the infinite map and all entity states, is saved to a persistent storage system.
- **Map Storage**: Map is stored as a series of chunks, each containing a portion of the map and its associated entities.
- **State Restoration**: State of robots and other entities is also saved and restored during the save/load process.
    
## 6. User Interface (GUI)
### 6.1 Map View
- **View**: Zoomable map view.

#### Map Navigation
- **Panning**: Right-click and drag or WASD keys.
- **Zooming**: Mouse wheel scroll.

### 6.2 Interaction & Controls
- **Input**: Full mouse support for navigation and interaction.

#### Control Scheme
- **Left Click**: Select building / Place building.
- **Right Click**: Cancel action / Pan map.
- **Shift + Click**: Mass select similar buildings.

### 6.3 HUD & Feedback

#### HUD Elements
- **Resource Bar**: Top of screen showing totals of Coal, Iron Ore, Copper Ore, Iron Plate, and Copper Plate.
- **Robot Status**: Bottom left showing number of active vs. idle robots.
- **Mini-map**: Bottom right showing explored area and buildings.

#### Notification System
- **Alerts**: Simple toast notifications for events such as "Storage Full" or "Resource Depleted".

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
