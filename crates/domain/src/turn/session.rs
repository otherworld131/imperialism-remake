//! Mid-turn session state (card #494): `begin_turn` runs the diplomacy/AI
//! half of the pipeline and pauses with a [`TurnSession`]; the frontend
//! drives the interactive diplomatic + trade sessions against it, then
//! `finish_turn` consumes it to resolve trade, combat, and the economy.
//! The atomic `process_turn` path composes the two halves with no
//! interactive decisions, so batch runs, observers, and skip runs behave
//! exactly as before.

use crate::economy::trade::{AcceptedTrade, Commodity, PreparedTradeSession, TradeOffer};
use crate::events::HeadlineCategory;
use crate::game_state::GameState;
use crate::turn::processor::TurnReport;
use crate::types::*;

/// A diplomatic change surfaced in the between-turns diplomatic session.
#[derive(Debug, Clone)]
pub struct DiploSessionEvent {
    pub text: String,
    pub category: HeadlineCategory,
    /// Nations involved (drives the flag row in the session UI).
    pub nation_ids: Vec<NationId>,
}

/// One seller's offer as presented to the player during the trade session.
#[derive(Debug, Clone)]
pub struct SessionOffer {
    pub seller: NationId,
    pub resource: ResourceType,
    /// Quantity still available after the player's earlier acceptances.
    pub remaining: u32,
    pub price_per_unit: Money,
    /// The player's diplomatic score toward the seller (presentation order).
    pub relation_score: i32,
}

/// The paused, half-resolved turn: player pending diplomacy, AI decisions,
/// diplomacy resolution, and the pre-trade economy phases have run; the
/// trade-offer pool is frozen. Trade resolution and everything after happen
/// in `finish_turn`.
pub struct TurnSession {
    pub(crate) report: TurnReport,
    /// Frozen offer pool the trade phase will consume.
    pub offers: Vec<TradeOffer>,
    /// Player-accepted trades, in acceptance order.
    pub accepted: Vec<AcceptedTrade>,
    /// Whether an interactive session drove the trade decisions. `false`
    /// (atomic path) makes the human seat fall back to wishlist auto-bids.
    pub interactive: bool,
    /// Diplomatic changes to show in the diplomatic session.
    pub diplo_events: Vec<DiploSessionEvent>,
    /// The player's blockade-adjusted cargo capacity, capping session buys.
    pub player_cargo_capacity: u32,
}

impl TurnSession {
    /// Remaining quantity a seller still offers of `resource` after the
    /// player's earlier acceptances.
    pub fn remaining_for(&self, seller: NationId, resource: ResourceType) -> u32 {
        let offered: u32 = self
            .offers
            .iter()
            .filter(|o| o.seller == seller && o.commodity == Commodity::Resource(resource))
            .map(|o| o.quantity)
            .sum();
        let taken: u32 = self
            .accepted
            .iter()
            .filter(|a| a.seller == seller && a.resource == resource)
            .map(|a| a.quantity)
            .sum();
        offered.saturating_sub(taken)
    }

    /// Cargo already committed by acceptances.
    pub fn cargo_committed(&self) -> u32 {
        self.accepted.iter().map(|a| a.quantity).sum()
    }

    /// Money already committed by acceptances.
    pub fn money_committed(&self) -> Money {
        self.accepted.iter().fold(Money::ZERO, |acc, a| {
            acc + a.price_per_unit * i64::from(a.quantity)
        })
    }

    /// Validate and record a trade acceptance. The quantity is clamped to
    /// the seller's remaining offer, the player's remaining cargo capacity,
    /// and the player's remaining treasury; returns the quantity actually
    /// accepted.
    pub fn accept_trade(
        &mut self,
        game: &GameState,
        seller: NationId,
        resource: ResourceType,
        quantity: u32,
    ) -> Result<u32, String> {
        if quantity == 0 {
            return Err("quantity must be positive".into());
        }
        let remaining = self.remaining_for(seller, resource);
        if remaining == 0 {
            return Err("that offer is sold out".into());
        }
        let price = self
            .offers
            .iter()
            .find(|o| o.seller == seller && o.commodity == Commodity::Resource(resource))
            .map(|o| o.price_per_unit)
            .ok_or_else(|| "no such offer".to_string())?;

        let mut qty = quantity.min(remaining);
        let cargo_left = self
            .player_cargo_capacity
            .saturating_sub(self.cargo_committed());
        if cargo_left == 0 {
            return Err("no merchant cargo capacity left".into());
        }
        qty = qty.min(cargo_left);

        let human_id = game.human_player_nation;
        let treasury = game
            .get_nation(human_id)
            .map(|n| n.economy.treasury)
            .unwrap_or(Money::ZERO);
        let cash_left = treasury - self.money_committed();
        if price > Money::ZERO {
            let affordable = (cash_left.as_dollars() / price.as_dollars().max(1)).max(0) as u32;
            if affordable == 0 {
                return Err("cannot afford this offer".into());
            }
            qty = qty.min(affordable);
        }
        if qty == 0 {
            return Err("nothing to buy".into());
        }

        self.accepted.push(AcceptedTrade {
            seller,
            resource,
            quantity: qty,
            price_per_unit: price,
        });
        Ok(qty)
    }

    /// The offers to present in the trade session: one entry per
    /// (seller, resource) on the player's buy wishlist, ordered by wishlist
    /// order, then the player's relationship with the seller (best first),
    /// then seller id for determinism. Sold-out entries are dropped.
    pub fn offers_for_player(&self, game: &GameState) -> Vec<SessionOffer> {
        let human_id = game.human_player_nation;
        let Some(human) = game.get_nation(human_id) else {
            return Vec::new();
        };
        let mut seen_resources = std::collections::HashSet::new();
        let mut out = Vec::new();
        for &resource in &human.diplomacy.buy_wishlist {
            if !seen_resources.insert(resource) {
                continue;
            }
            let mut sellers_seen = std::collections::HashSet::new();
            let mut per_resource: Vec<SessionOffer> = Vec::new();
            for offer in &self.offers {
                if offer.commodity != Commodity::Resource(resource) || offer.seller == human_id {
                    continue;
                }
                if !sellers_seen.insert(offer.seller) {
                    continue;
                }
                let remaining = self.remaining_for(offer.seller, resource);
                if remaining == 0 {
                    continue;
                }
                let relation_score = game
                    .world
                    .diplomacy
                    .get_relation(human_id, offer.seller)
                    .map(|r| r.score)
                    .unwrap_or(0);
                per_resource.push(SessionOffer {
                    seller: offer.seller,
                    resource,
                    remaining,
                    price_per_unit: offer.price_per_unit,
                    relation_score,
                });
            }
            per_resource.sort_by(|a, b| {
                b.relation_score
                    .cmp(&a.relation_score)
                    .then(a.seller.0.cmp(&b.seller.0))
            });
            out.extend(per_resource);
        }
        out
    }

    /// Split the session into the parts `finish_turn` needs.
    pub(crate) fn into_finish_parts(self) -> (TurnReport, PreparedTradeSession) {
        (
            self.report,
            PreparedTradeSession {
                offers: self.offers,
                accepted: self.accepted,
                interactive: self.interactive,
            },
        )
    }
}

/// Diplomatic changes worth surfacing in the session: every diplomacy-ish
/// headline pushed during the begin half, plus AI war declarations (which
/// only become headlines at newspaper time).
pub(super) fn collect_diplo_events(report: &TurnReport) -> Vec<DiploSessionEvent> {
    let mut events: Vec<DiploSessionEvent> = Vec::new();
    for action in &report.ai_actions {
        if !action.is_non_action && action.text.contains("declared war on") {
            events.push(DiploSessionEvent {
                text: action.text.clone(),
                category: HeadlineCategory::War,
                nation_ids: vec![action.nation_id],
            });
        }
    }
    events.extend(
        report
            .newspaper_headlines
            .iter()
            .filter(|h| {
                !h.is_non_action
                    && matches!(
                        h.category,
                        HeadlineCategory::War
                            | HeadlineCategory::Diplomacy
                            | HeadlineCategory::Politics
                    )
            })
            .map(|h| DiploSessionEvent {
                text: h.text.clone(),
                category: h.category,
                nation_ids: h.nation_ids.clone(),
            }),
    );
    events
}
