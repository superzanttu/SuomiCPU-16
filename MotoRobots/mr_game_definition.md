# MotoRobots Game Definition

MotoRobots is a Factorio-type automation game focused on resource extraction, processing, and logistics using a fleet of transport robots.

## 1. Core Gameplay Loop
(Define the high-level game loop: extraction -> processing -> storage -> expansion)

## 2. Entities
### 2.1 Buildings
- **Mine**: Produces raw resources (ores, coal, iron, or copper).
- **Factory**: Processes raw ores into refined materials.
- **Storage**: Stores resources and processed materials.

#### Building Construction
(Define how buildings are placed, upgraded, and destroyed here)

### 2.2 Logistics
- **Transport Robot**: Transports items between mines, factories, and storage units. Uses a pathfinding algorithm to determine the optimal route.

#### Robot Specifications
(Define robot movement speed, carrying capacity, and battery/energy limits here)

## 3. World & Map
### 3.1 Map Generation
- **Map Type**: Infinite procedurally generated tile map.

#### Generation Rules
(Define noise functions or algorithms used to generate landmasses, rivers, and mountains here)

#### Biomes
(Define different terrain types or resource distributions across the map here)

### 3.2 Tile Types
- **Ground**: Walkable by robots; suitable for building construction.
- **Rock**: Impassable for robots; no buildings allowed. Forms mountains.
- **Water**: Impassable for robots; no buildings allowed. Forms rivers and lakes.

## 4. Resources & Economy
### 4.1 Materials
(Define all raw and refined materials here)

### 4.2 Production & Recipes

#### Resource Rates
(Define production rates for mines and processing speeds for factories here)

#### Recipe List
(Define detailed list of what ores the factories process and what the resulting outputs are here)

### 4.3 Costs

#### Building Costs
(Define resources required to construct mines, factories, and storage here)

## 5. Technical Systems
### 5.1 Pathfinding

#### Pathfinding Algorithm
(Define specifics of the algorithm used, e.g., A*, Dijkstra and how it handles dynamic obstacles here)

### 5.2 Save/Load System
(Define how the infinite map and entity states are persisted)

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
