# 10 — Trade & Economy

## Overview

Great Powers buy raw materials from Minor Nations and sell refined goods back. Trade
generates revenue and improves diplomatic relations. The trade system operates through
structured sessions each turn after all players submit orders.

## Checklist

### Trade Session Mechanics
- [x] Trade resolved after all players end their turns
- [x] Each player sets offers (sell) and bids (buy) on the Trade screen
- [x] Offers: specify goods to sell, quantity, and minimum price
- [x] Bids: specify resources to buy, quantity, and maximum price
- [x] Matching algorithm: pair compatible offers/bids by price
- [x] Priority/preference system: Minor Nations prefer partners with higher relationship scores
- [x] Unit tests: trade matching algorithm correctness
- [x] Unit tests: preference-based tie-breaking

### Trade Infrastructure Requirements
- [x] Trade Consulate required before any trade with a Minor Nation
- [x] Each trade requires sufficient merchant ship cargo capacity
- [x] Cargo holds displayed on trade screen — one hold per item
- [x] Great Powers use their own ships for items they purchase
- [x] Unit tests: consulate prerequisite enforcement
- [x] Unit tests: cargo capacity limiting trade volume

### Tradeable vs. Non-Tradeable
- [x] **Tradeable**: Timber, Coal, Iron, Cotton, Wool, Oil, Fruit, Livestock, Lumber, Steel, Fabric, Furniture, Clothing, Hardware, Arms, Canned Food
- [x] **Non-tradeable**: Grain (cannot be bought or sold), Horses, Paper
- [x] Unit tests: trade system rejects non-tradeable resources

### Trade Subsidies
- [x] Player can offer subsidies to Minor Nations
- [x] Subsidies increase export prices (Minor Nation gets more for selling to you)
- [x] Subsidies decrease import costs (you pay less for buying from them)
- [x] Makes trade more profitable and more likely for the Minor Nation
- [ ] Ctrl+click auto-calculate: determines the subsidy level needed to become the preferred trade partner
- [x] Subsidy cost deducted from treasury each turn it's active
- [x] Unit tests: subsidy calculation algorithm
- [x] Unit tests: subsidy impact on trade preference

### Revenue & Pricing
- [x] Base prices for all tradeable commodities
- [x] Supply and demand affect prices (more sellers → lower price)
- [x] Revenue = quantity × price for each sold item
- [x] Track profit/loss per trade partner per turn
- [x] Historical trade data for player reference
- [x] Unit tests: revenue calculations
- [x] Unit tests: supply/demand price adjustments

### Diplomatic Impact of Trade
- [x] Trading improves relationship with trade partner
- [x] Relationship improvement based on number of distinct commodity types traded (not quantity)
- [x] Consistent trade over multiple turns compounds relationship growth
- [x] Cutting off trade harms relationship
- [x] Unit tests: trade-to-diplomacy relationship score changes

### Merchant Marine
- [x] Merchant ships carry traded goods
- [x] Ship types with cargo capacity: Trader (2), Indiaman (4), Clipper (4), Paddlewheeler (8)
- [x] Ships can be blockaded by enemy warships (some merchant ships may be sunk)
- [x] Merchant ships not directly visible on map but appear in battle reports
- [x] Merchant marine size contributes to game score
- [x] Unit tests: cargo capacity calculations
- [x] Unit tests: blockade effect on trade

### Trade Screen (Domain Logic)
- [x] List all Minor Nations with Trade Consulates
- [x] Show available goods and current prices for each partner
- [x] Allow setting offers and bids with quantity + price
- [x] Show current cargo capacity and utilization
- [x] Preview expected revenue from current offers
- [x] Show diplomatic relationship status per partner
- [x] Unit tests: trade screen data aggregation

### Verification Strategy
- [x] **Unit tests**: Run test suite — all trade tests pass
- [x] **Integration test**: Set up 3 nations, create trade offers/bids → resolve session → verify correct transactions, revenue, and relationship changes
- [x] **Integration test**: Verify blockade reduces trade (some merchant ships sunk)
- [x] **Edge case tests**: No cargo capacity, all bids above all offers, multiple bidders for same resource
- [ ] **Scenario test**: 20-turn trade simulation → verify economic growth trajectory matches expected curves
