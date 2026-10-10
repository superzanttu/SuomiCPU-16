# MotoRobots Game Definition

MotoRobots is a Factorio-type automation game focused on resource extraction, processing, and logistics using a fleet of transport robots.

## 1. Core Gameplay Loop
Mines extracts resources from ground. Robots transport resources from mines to factories, and from facotoris to storage. Player controls robots byt defining source and targer locations for each transport task.


## 2. Entities
### 2.1 Buildings
- **Mine**: Produces raw resources (ores, coal, iron, or copper).
- **Factory**: Processes raw ores into refined materials.
- **Storage**: Stores resources and processed materials.
- **Base**: The central hub for the player, where robots are managed and resources are initially stored.

#### Building Construction
Building are constructed for free, because this is first prototype game.


### 2.2 Logistics
- **Transport Robot**: Transports items between mines, factories, and storage units. Uses a pathfinding algorithm to determine the optimal route.  RObots are programmed by defining source and target locations for transport tasks.

#### Robot Specifications
Robots are autonomous units that follow the instructions given by the player. Robots execute transport tasks by moving between the defined source and target locations. Robots can carry a limited number of items. Robots don't consume energy. Robots have defined movement speed and load/unload speed. Robots can move forward and backward, turn left and right. Robots avoid obstacles and other robots to prevent collisions.

## 3. World & Map
### 3.1 Map Generation
- **Map Type**: Infinite procedurally generated tile map.

#### Generation Rules
Maps have lakes and rivers. Mountains are formed by clusters of rocks.
Building can exist on ground tiles only. Mines can only be placed on ground tiles that contain the corresponding resource.

#### Biomes
Resources are formed on clusted or resource-rich ground tiles. Resources are infinite. Resources cluster are distributed evenly on map.

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

### 4.2 Production & Recipes
- **Mine**: Extracts raw resources from the ground.
- **Factory**: Processes raw resources into refined materials.
- **Storage**: Holds raw and refined materials for later use.
- **Base**: The central hub for the player, where robots are managed and resources are initially stored.

#### Resource Rates
| Resource     | Production Rate |
| ------------ | --------------- |
| Coal         | 1 unit/sec      |
| Iron Ore     | 1 unit/sec      |
| Copper Ore   | 1 unit/sec      |
| Iron Plate   | 1 unit/sec      |
| Copper Plate | 1 unit/sec      |

#### Recipe List


(Define detailed list of what ores the factories process and what the resulting outputs are here)

### 4.3 Costs

#### Building Costs
(Define resources required to construct mines, factories, and storage here)

## 5. Technical Systems
### 5.1 Pathfinding

#### Pathfinding Algorithm
RRobots use A* to find routes on the map. If robot hits a blocked tile, it recalculates the path to avoid the obstacle.

### 5.2 Save/Load System
Game state, including the infinite map and all entity states, is saved to a persistent storage system. When loading, the game reconstructs the map and entities from this saved state. Map is stored as a series of chunks, each containing a portion of the map and its associated entities.
State of robots and other entities is also saved and restored during the save/load process.
    
## 6. User Interface (GUI)
### 6.1 Map View
- **View**: Zoomable map view.

#### Map Navigation
(Define how the user pans and zooms the map here)

### 6.2 Interaction & Controls
- **Input**: Full mouse support for navigation and interaction.

#### Control Scheme
(Define detailed mapping of mouse buttons and keyboard shortcuts here)

### 6.3 HUD & Feedback

#### HUD Elements
(Define the visual elements displayed on screen, e.g., resource counters, active robot count)

#### Notification System
(Define how the player is notified of events, e.g., storage full)

## 7. Progression & Goals
### 7.1 Milestones
(Define specific goals or milestones for the player)

### 7.2 Tech Tree / Upgrades

#### Progression System
(Define how the player unlocks new buildings or upgrades robots here)

### 7.3 Win/Loss Conditions
(Define if there are specific milestones or an endgame goal here)
