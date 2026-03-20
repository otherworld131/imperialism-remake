# 07 — Transport System

## Overview

The transport system moves resources from terrain tiles to the capital city for processing.
Freight cars carry goods along railroad networks. Transport capacity is a key strategic
bottleneck — resources not transported are wasted.

## Checklist

### Freight Cars
- [ ] Built in the Railyard: 2 labor + 1 lumber + 1 steel per car
- [ ] No maintenance cost once built — permanent assets
- [ ] Each car carries one unit of one resource type per turn
- [ ] Cars assigned to routes via the Transport screen
- [ ] Assignment uses slider UI (proportion of capacity per resource)
- [ ] Unit tests: freight car construction cost validation
- [ ] Unit tests: capacity correctly increases with each car built

### Transport Allocation
- [ ] Player assigns transport priority per resource type each turn
- [ ] "Give Transport Orders" — sliders determine what percentage of capacity goes to each resource
- [ ] Resources from connected tiles are collected automatically
- [ ] Only resources from tiles connected via railroad/depot/port reach the capital
- [ ] Unconnected tiles' resources are wasted
- [ ] Excess resources beyond transport capacity are left behind
- [ ] Unit tests: allocation algorithm distributes capacity correctly
- [ ] Unit tests: disconnected tiles produce no deliveries

### Military Transport
- [ ] Army unit transport size = number of arms used to build it
- [ ] Rail transport capacity: 1 army unit per 5 freight cars
- [ ] Troops can be moved via rail to any connected province in one turn
- [ ] Amphibious transport: landing force size = total arms of all ships in beachhead fleet
- [ ] Example: 4 frigates (2 arms each) = landing force of size 8
- [ ] Generals count as 1 transport unit
- [ ] Unit tests: military transport capacity calculations
- [ ] Unit tests: amphibious landing force size calculations

### Transport Screen (Domain Logic)
- [ ] Calculate total freight car capacity
- [ ] Calculate total resource production from all connected tiles
- [ ] Show surplus/deficit per resource type
- [ ] Allow reallocation without leaving the screen
- [ ] Preview next turn's deliveries based on current allocation
- [ ] Unit tests: transport screen data calculations

### Verification Strategy
- [ ] **Unit tests**: Run `cargo test` — all transport-related tests pass
- [ ] **Integration test**: Create a game state with 3 connected tiles + 5 freight cars → verify correct resource delivery after turn processing
- [ ] **Edge case tests**: 0 freight cars, more resources than capacity, disconnected tiles, province captured mid-turn
- [ ] **Regression test**: Verify barges have no effect (matching original game behavior)
