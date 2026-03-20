# 05 — Resource & Production System

## Overview

Imperialism features a mercantilist economy: the state controls all production. Raw resources
are extracted from terrain tiles, processed into materials in mills, then refined into finished
goods in factories. Three parallel production chains exist.

## Checklist

### Resource Categories

#### Raw Resources (from terrain)
- [ ] **Timber** — from Hardwood Forest (1-3/turn) and Scrub Forest (1/turn fixed)
- [ ] **Coal** — from Barren Hills / Mountains (2-6/turn depending on mine level)
- [ ] **Iron** — from Barren Hills / Mountains (2-6/turn depending on mine level)
- [ ] **Cotton** — from Plantation (1-3/turn)
- [ ] **Wool** — from Fertile Hills (1-3/turn)
- [ ] **Grain** — from Farm (1-3/turn) + Dry Plains (1/turn fixed); **cannot be traded**
- [ ] **Fruit** — from Orchard (1-3/turn)
- [ ] **Livestock** — from Open Range (1/turn)
- [ ] **Horses** — from Horse Ranch (1/turn)
- [ ] **Oil** — from Desert/Swamp/Tundra (1-3/turn after Oil Drilling tech)
- [ ] **Gold** — from Mountains (1-2/turn); directly converts to money
- [ ] **Gems** — from Mountains (1-2/turn); directly converts to money (very lucrative)

### Production Chains (3 parallel chains)

#### Timber Chain
- [ ] Timber → **Lumber Mill** (2 timber + 2 labor → 1 lumber)
- [ ] Lumber → **Furniture Factory** (2 lumber + 2 labor → 1 furniture)
- [ ] Lumber used for: transport cars, ships, building expansion, railroad construction
- [ ] Furniture used for: recruiting new workers (immigration), trade revenue

#### Metal Chain
- [ ] Coal + Iron → **Steel Mill** (1 coal + 1 iron + 2 labor → 1 steel)
- [ ] Steel → **Hardware Factory** (2 steel + 2 labor → 1 hardware)
- [ ] Steel used for: transport cars, ships, arms production, building expansion, forts
- [ ] Hardware used for: trade revenue

#### Textile Chain
- [ ] Cotton or Wool → **Textile Mill** (2 cotton/wool + 2 labor → 1 fabric)
- [ ] Fabric → **Clothing Factory** (2 fabric + 2 labor → 1 clothing)
- [ ] Fabric used for: ship sails, trade revenue
- [ ] Clothing used for: recruiting new workers (immigration), trade revenue

### Special Products
- [ ] **Paper** — produced in Trade School (trains workers); used for specialist training
- [ ] **Arms** — produced in Armory; used for military unit construction
- [ ] **Canned Food** — produced in Food Processing from grain/fruit/livestock; feeds population

### Mill & Factory Mechanics
- [ ] Mills process raw resources into materials (2:1 ratio)
- [ ] Factories process materials into finished goods (2:1 ratio)
- [ ] Each mill/factory has a capacity (units processed per turn)
- [ ] Capacity starts at 2 (Easy) or must be built (harder difficulties)
- [ ] Expansion costs 1 lumber + 1 steel per capacity unit
- [ ] Capacity progression: 2 → 4 → 8 → 12 → 16 → ...
- [ ] 2-turn delay before new capacity becomes active
- [ ] Production assignment: player allocates resources to each mill/factory each turn
- [ ] Unit tests for production calculations

### Food System
- [ ] Population requires food each turn: grain preferred by ≥ 50% of population
- [ ] Food types: Grain, Fruit, Livestock (meat)
- [ ] Food Processing building converts raw food → Canned Food (half nutrition value)
- [ ] Canned Food can be sold and is needed for immigration
- [ ] Immigration requires: Canned Food + Clothing + Furniture
- [ ] Starvation mechanics if food supply is insufficient
- [ ] Unit tests for food consumption and immigration requirements

### Labor System
- [ ] **Untrained Worker** — base labor unit, recruited via Capitol building
- [ ] **Trained Worker** — produced in Trade School (1 untrained + paper)
- [ ] **Expert Worker** — produced in Trade School (1 trained + paper)
- [ ] Workers assigned to: mills, factories, civilian units, military units
- [ ] Each production facility needs 2 labor units per unit of output
- [ ] Worker pool management — track available vs. assigned workers
- [ ] Worker recruitment limited by province count (1 worker per N provinces)
- [ ] Reward: Capitol expansion at 10 and 30 expert workers
- [ ] Unit tests for labor allocation and training

### Warehouse / Inventory
- [ ] `Warehouse` entity — stores all resources, materials, and goods for a nation
- [ ] Tracks incoming shipments (from transport) vs. available stock
- [ ] Display of current inventory and expected next-turn deliveries
- [ ] Resources not transported to capital are lost (no local storage)
- [ ] Unit tests for warehouse accounting

### Revenue & Treasury
- [ ] Gold and Gems automatically convert to money
- [ ] Selling goods on trade market generates revenue
- [ ] Military maintenance: $25/turn per arm in active army units
- [ ] Building construction and expansion costs
- [ ] Civilian unit creation costs
- [ ] Technology research costs
- [ ] Diplomatic costs (embassies: $5,000, consulates: $500, grants, subsidies)
- [ ] Bankruptcy protection / deficit handling rules
- [ ] Unit tests for treasury calculations

### Verification Strategy
- [ ] **Unit tests**: `cargo test` — all resource, production, food, labor, warehouse, treasury tests pass
- [ ] **Production chain test**: Feed known inputs into each chain → verify exact outputs (e.g., 4 timber + 4 labor → 2 lumber; 4 lumber + 4 labor → 2 furniture)
- [ ] **Food balance test**: Create nation with 10 population, 8 grain, 2 fruit → verify food sufficient; reduce to 5 grain → verify shortage
- [ ] **Immigration test**: Stock warehouse with canned food + clothing + furniture → recruit worker → verify goods consumed, worker appears next turn
- [ ] **Worker pipeline test**: Recruit untrained → train → expert → specialist → verify each step costs correct resources
- [ ] **Mill capacity test**: Mill at capacity 4, feed 8 timber → verify 4 lumber produced; feed 10 timber → verify still only 4 (capped)
- [ ] **Factory expansion test**: Expand factory → verify 2-turn delay before new capacity active
- [ ] **Treasury test**: Start with $10,000, spend $3,000 on tech, $500 on consulate, earn $2,000 from trade → verify balance = $8,500
