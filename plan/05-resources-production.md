# 05 — Resource & Production System

## Overview

Imperialism features a mercantilist economy: the state controls all production. Raw resources
are extracted from terrain tiles, processed into materials in mills, then refined into finished
goods in factories. Three parallel production chains exist.

## Checklist

### Resource Categories

#### Raw Resources (from terrain)
- [x] **Timber** — from Hardwood Forest (1-3/turn) and Scrub Forest (1/turn fixed)
- [x] **Coal** — from Barren Hills / Mountains (2-6/turn depending on mine level)
- [x] **Iron** — from Barren Hills / Mountains (2-6/turn depending on mine level)
- [x] **Cotton** — from Plantation (1-3/turn)
- [x] **Wool** — from Fertile Hills (1-3/turn)
- [x] **Grain** — from Farm (1-3/turn) + Dry Plains (1/turn fixed); **cannot be traded**
- [x] **Fruit** — from Orchard (1-3/turn)
- [x] **Livestock** — from Open Range (1/turn)
- [x] **Horses** — from Horse Ranch (1/turn)
- [x] **Oil** — from Desert/Swamp/Tundra (1-3/turn after Oil Drilling tech)
- [x] **Gold** — from Mountains (1-2/turn); directly converts to money
- [x] **Gems** — from Mountains (1-2/turn); directly converts to money (very lucrative)

### Production Chains (3 parallel chains)

#### Timber Chain
- [x] Timber → **Lumber Mill** (2 timber + 2 labor → 1 lumber)
- [x] Lumber → **Furniture Factory** (2 lumber + 2 labor → 1 furniture)
- [x] Lumber used for: transport cars, ships, building expansion, railroad construction
- [x] Furniture used for: recruiting new workers (immigration), trade revenue

#### Metal Chain
- [x] Coal + Iron → **Steel Mill** (1 coal + 1 iron + 2 labor → 1 steel)
- [x] Steel → **Hardware Factory** (2 steel + 2 labor → 1 hardware)
- [x] Steel used for: transport cars, ships, arms production, building expansion, forts
- [x] Hardware used for: trade revenue

#### Textile Chain
- [x] Cotton or Wool → **Textile Mill** (2 cotton/wool + 2 labor → 1 fabric)
- [x] Fabric → **Clothing Factory** (2 fabric + 2 labor → 1 clothing)
- [x] Fabric used for: ship sails, trade revenue
- [x] Clothing used for: recruiting new workers (immigration), trade revenue

### Special Products
- [x] **Paper** — produced in Trade School (trains workers); used for specialist training
- [x] **Arms** — produced in Armory; used for military unit construction
- [x] **Canned Food** — produced in Food Processing from grain/fruit/livestock; feeds population

### Mill & Factory Mechanics
- [x] Mills process raw resources into materials (2:1 ratio)
- [x] Factories process materials into finished goods (2:1 ratio)
- [x] Each mill/factory has a capacity (units processed per turn)
- [x] Capacity starts at 2 (Easy) or must be built (harder difficulties)
- [x] Expansion costs 1 lumber + 1 steel per capacity unit
- [x] Capacity progression: 2 → 4 → 8 → 12 → 16 → ...
- [x] 2-turn delay before new capacity becomes active
- [x] Production assignment: player allocates resources to each mill/factory each turn
- [x] Unit tests for production calculations

### Food System
- [x] Population requires food each turn: grain preferred by ≥ 50% of population
- [x] Food types: Grain, Fruit, Livestock (meat)
- [x] Food Processing building converts raw food → Canned Food (half nutrition value)
- [x] Canned Food can be sold and is needed for immigration
- [x] Immigration requires: Canned Food + Clothing + Furniture
- [x] Starvation mechanics if food supply is insufficient
- [x] Unit tests for food consumption and immigration requirements

### Labor System
- [x] **Untrained Worker** — base labor unit, recruited via Capitol building
- [x] **Trained Worker** — produced in Trade School (1 untrained + paper)
- [x] **Expert Worker** — produced in Trade School (1 trained + paper)
- [x] Workers assigned to: mills, factories, civilian units, military units
- [x] Each production facility needs 2 labor units per unit of output
- [x] Worker pool management — track available vs. assigned workers
- [x] Worker recruitment limited by province count (1 worker per N provinces)
- [x] Reward: Capitol expansion at 10 and 30 expert workers
- [x] Unit tests for labor allocation and training

### Warehouse / Inventory
- [x] `Warehouse` entity — stores all resources, materials, and goods for a nation
- [x] Tracks incoming shipments (from transport) vs. available stock
- [x] Display of current inventory and expected next-turn deliveries
- [x] Resources not transported to capital are lost (no local storage)
- [x] Unit tests for warehouse accounting

### Revenue & Treasury
- [x] Gold and Gems automatically convert to money
- [x] Selling goods on trade market generates revenue
- [x] Military maintenance: $25/turn per arm in active army units
- [x] Building construction and expansion costs
- [x] Civilian unit creation costs
- [x] Technology research costs
- [x] Diplomatic costs (embassies: $5,000, consulates: $500, grants, subsidies)
- [x] Bankruptcy protection / deficit handling rules
- [x] Unit tests for treasury calculations

### Verification Strategy
- [x] **Unit tests**: `cargo test` — all resource, production, food, labor, warehouse, treasury tests pass
- [x] **Production chain test**: Feed known inputs into each chain → verify exact outputs (e.g., 4 timber + 4 labor → 2 lumber; 4 lumber + 4 labor → 2 furniture)
- [x] **Food balance test**: Create nation with 10 population, 8 grain, 2 fruit → verify food sufficient; reduce to 5 grain → verify shortage
- [x] **Immigration test**: Stock warehouse with canned food + clothing + furniture → recruit worker → verify goods consumed, worker appears next turn
- [x] **Worker pipeline test**: Recruit untrained → train → expert → specialist → verify each step costs correct resources
- [x] **Mill capacity test**: Mill at capacity 4, feed 8 timber → verify 4 lumber produced; feed 10 timber → verify still only 4 (capped)
- [x] **Factory expansion test**: Expand factory → verify 2-turn delay before new capacity active
- [x] **Treasury test**: Start with $10,000, spend $3,000 on tech, $500 on consulate, earn $2,000 from trade → verify balance = $8,500
