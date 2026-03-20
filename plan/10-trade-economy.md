# 10 — Trade & Economy

## Overview

Great Powers buy raw materials from Minor Nations and sell refined goods back. Trade
generates revenue and improves diplomatic relations. The trade system operates through
structured sessions each turn after all players submit orders.

## Checklist

### Trade Session Mechanics
- [ ] Trade resolved after all players end their turns
- [ ] Each player sets offers (sell) and bids (buy) on the Trade screen
- [ ] Offers: specify goods to sell, quantity, and minimum price
- [ ] Bids: specify resources to buy, quantity, and maximum price
- [ ] Matching algorithm: pair compatible offers/bids by price
- [ ] Priority/preference system: Minor Nations prefer partners with higher relationship scores
- [ ] Unit tests: trade matching algorithm correctness
- [ ] Unit tests: preference-based tie-breaking

### Trade Infrastructure Requirements
- [ ] Trade Consulate required before any trade with a Minor Nation
- [ ] Each trade requires sufficient merchant ship cargo capacity
- [ ] Cargo holds displayed on trade screen — one hold per item
- [ ] Great Powers use their own ships for items they purchase
- [ ] Unit tests: consulate prerequisite enforcement
- [ ] Unit tests: cargo capacity limiting trade volume

### Tradeable vs. Non-Tradeable
- [ ] **Tradeable**: Timber, Coal, Iron, Cotton, Wool, Oil, Fruit, Livestock, Lumber, Steel, Fabric, Furniture, Clothing, Hardware, Arms, Canned Food
- [ ] **Non-tradeable**: Grain (cannot be bought or sold), Horses, Paper
- [ ] Unit tests: trade system rejects non-tradeable resources

### Trade Subsidies
- [ ] Player can offer subsidies to Minor Nations
- [ ] Subsidies increase export prices (Minor Nation gets more for selling to you)
- [ ] Subsidies decrease import costs (you pay less for buying from them)
- [ ] Makes trade more profitable and more likely for the Minor Nation
- [ ] Ctrl+click auto-calculate: determines the subsidy level needed to become the preferred trade partner
- [ ] Subsidy cost deducted from treasury each turn it's active
- [ ] Unit tests: subsidy calculation algorithm
- [ ] Unit tests: subsidy impact on trade preference

### Revenue & Pricing
- [ ] Base prices for all tradeable commodities
- [ ] Supply and demand affect prices (more sellers → lower price)
- [ ] Revenue = quantity × price for each sold item
- [ ] Track profit/loss per trade partner per turn
- [ ] Historical trade data for player reference
- [ ] Unit tests: revenue calculations
- [ ] Unit tests: supply/demand price adjustments

### Diplomatic Impact of Trade
- [ ] Trading improves relationship with trade partner
- [ ] Relationship improvement based on number of distinct commodity types traded (not quantity)
- [ ] Consistent trade over multiple turns compounds relationship growth
- [ ] Cutting off trade harms relationship
- [ ] Unit tests: trade-to-diplomacy relationship score changes

### Merchant Marine
- [ ] Merchant ships carry traded goods
- [ ] Ship types with cargo capacity: Trader (2), Indiaman (4), Clipper (4), Paddlewheeler (8)
- [ ] Ships can be blockaded by enemy warships (some merchant ships may be sunk)
- [ ] Merchant ships not directly visible on map but appear in battle reports
- [ ] Merchant marine size contributes to game score
- [ ] Unit tests: cargo capacity calculations
- [ ] Unit tests: blockade effect on trade

### Trade Screen (Domain Logic)
- [ ] List all Minor Nations with Trade Consulates
- [ ] Show available goods and current prices for each partner
- [ ] Allow setting offers and bids with quantity + price
- [ ] Show current cargo capacity and utilization
- [ ] Preview expected revenue from current offers
- [ ] Show diplomatic relationship status per partner
- [ ] Unit tests: trade screen data aggregation

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all trade tests pass
- [ ] **Integration test**: Set up 3 nations, create trade offers/bids → resolve session → verify correct transactions, revenue, and relationship changes
- [ ] **Integration test**: Verify blockade reduces trade (some merchant ships sunk)
- [ ] **Edge case tests**: No cargo capacity, all bids above all offers, multiple bidders for same resource
- [ ] **Scenario test**: 20-turn trade simulation → verify economic growth trajectory matches expected curves
