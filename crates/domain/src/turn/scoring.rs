use crate::diplomacy::DiplomacyState;
use crate::map::Province;
use crate::nation::Nation;
use crate::types::*;

/// Score breakdown for a nation.
#[derive(Debug, Clone, Default)]
pub struct NationScore {
    pub military_score: u32,
    pub labor_score: u32,
    pub transport_score: u32,
    pub merchant_marine_score: u32,
    pub diplomatic_score: u32,
    pub province_score: u32,
    pub tech_score: u32,
    pub treasury_score: u32,
    pub building_score: u32,
    pub total: u32,
}

/// Detail of a single governor's vote.
#[derive(Debug, Clone)]
pub struct GovernorVoteDetail {
    /// The province this governor represents.
    pub province_name: String,
    /// Owner of the province.
    pub province_owner: NationId,
    /// Whether the owner is a Great Power or Minor Nation.
    pub owner_type: NationType,
    /// The GP this governor voted for.
    pub voted_for: NationId,
    /// Reason the governor voted this way.
    pub reason: String,
}

/// Result of a Council of Governors vote.
#[derive(Debug, Clone)]
pub struct CouncilVoteResult {
    /// Each entry is (nation_id, number of governors supporting that nation).
    pub votes: Vec<(NationId, u32)>,
    /// Total number of governors (= total provinces).
    pub total_governors: u32,
    /// 2/3 of total governors — the threshold to win outright.
    pub majority_threshold: u32,
    /// `Some` if a nation achieved the required majority (or won the final vote).
    pub winner: Option<NationId>,
    /// `true` if this is the 1915 final vote.
    pub is_final: bool,
    /// Detailed per-governor vote information.
    pub governor_details: Vec<GovernorVoteDetail>,
}

/// Calculate the game score for a nation.
///
/// Scoring components:
/// - **Military**: sum firepower of all army units (placeholder — currently 0)
/// - **Labor**: total workers * 10
/// - **Transport**: freight cars + railroad miles (placeholder — currently 0)
/// - **Merchant marine**: cargo capacity (placeholder — currently 0)
/// - **Diplomatic**: placeholder — currently 50
/// - **Province**: number of provinces * 100
pub fn calculate_score(nation: &Nation) -> NationScore {
    if nation.diplomacy.is_in_anarchy {
        return NationScore::default();
    }
    let military_score = nation
        .military.army
        .iter()
        .map(|u| u.effective_firepower() as u32)
        .sum::<u32>();
    let labor_score = nation.economy.labor.total_workers() * 10;
    let transport_score = nation.military.transport.freight_cars * 5;
    let merchant_marine_score = nation.total_cargo_capacity() * 20;
    let diplomatic_score = 50; // placeholder
    let province_score = nation.province_count() as u32 * 75;

    // Economic scoring components to prevent game stagnation
    let tech_score = nation.researched_techs.len() as u32 * 30;
    let treasury_score = (nation.economy.treasury.as_dollars().max(0) / 100).min(500) as u32;
    let building_score = nation.economy.buildings.len() as u32 * 10;

    let total = military_score
        + labor_score
        + transport_score
        + merchant_marine_score
        + diplomatic_score
        + province_score
        + tech_score
        + treasury_score
        + building_score;

    NationScore {
        military_score,
        labor_score,
        transport_score,
        merchant_marine_score,
        diplomatic_score,
        province_score,
        tech_score,
        treasury_score,
        building_score,
        total,
    }
}

/// Determine which GP a governor votes for.
///
/// - **Great Power governors**: always vote for their own GP.
/// - **Minor Nation governors**: vote for the GP with the highest diplomatic
///   relationship score. Ties are broken by lowest NationId.
pub fn governor_vote(
    province_owner: NationId,
    owner_type: NationType,
    diplomacy: &DiplomacyState,
    great_power_ids: &[NationId],
) -> NationId {
    match owner_type {
        NationType::GreatPower => province_owner,
        NationType::MinorNation => {
            // Find GP with best relationship score
            let mut best_gp = great_power_ids[0];
            let mut best_score = i32::MIN;
            for &gp_id in great_power_ids {
                let score = diplomacy
                    .get_relation(province_owner, gp_id)
                    .map(|rel| rel.score)
                    .unwrap_or(0);
                if score > best_score || (score == best_score && gp_id.0 < best_gp.0) {
                    best_score = score;
                    best_gp = gp_id;
                }
            }
            best_gp
        }
    }
}

/// Run the Council of Governors vote.
///
/// Each province has one governor. Governor voting preference:
/// - **Great Power provinces**: the governor always supports their own nation.
/// - **Minor Nation provinces**: the governor supports the Great Power with the
///   highest diplomatic relationship score (trade relationship).
///
/// A nation wins outright if it secures at least 2/3 of total governors.
/// On the final vote (1915) with no 2/3 majority, the nation with the most
/// governors wins by default.
pub fn run_council_vote(
    nations: &[Nation],
    provinces: &[Province],
    is_final: bool,
    diplomacy: &DiplomacyState,
) -> CouncilVoteResult {
    let total_governors = provinces.len() as u32;
    // 2/3 majority — rounding up so the threshold is strict.
    let majority_threshold = (total_governors * 2).div_ceil(3);

    // Collect Great Power IDs (exclude anarchic nations).
    let great_power_ids: Vec<NationId> = nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    if great_power_ids.is_empty() {
        return CouncilVoteResult {
            votes: Vec::new(),
            total_governors,
            majority_threshold,
            winner: None,
            is_final,
            governor_details: Vec::new(),
        };
    }

    // Tally governor votes per Great Power.
    let mut vote_tally: std::collections::HashMap<NationId, u32> = std::collections::HashMap::new();
    let mut governor_details: Vec<GovernorVoteDetail> = Vec::new();

    for province in provinces {
        // Anarchic nation provinces have no functioning governance — governors abstain.
        let owner_nation = nations.iter().find(|n| n.id == province.owner);
        if owner_nation.is_some_and(|n| n.diplomacy.is_in_anarchy) {
            continue;
        }

        // Determine which nation the governor of this province supports.
        let (owner_type, supported_gp, reason) = match owner_nation {
            Some(n) if n.is_great_power() => {
                (NationType::GreatPower, n.id, "own nation".to_string())
            }
            Some(n) => {
                // Minor Nation governor — vote based on diplomatic relationship.
                let voted_for =
                    governor_vote(n.id, NationType::MinorNation, diplomacy, &great_power_ids);
                let score = diplomacy
                    .get_relation(n.id, voted_for)
                    .map(|r| r.score)
                    .unwrap_or(0);
                let reason = format!("trade relationship (score: {})", score);
                (NationType::MinorNation, voted_for, reason)
            }
            None => {
                // Unknown owner — vote for first GP.
                (
                    NationType::MinorNation,
                    great_power_ids[0],
                    "unknown owner".to_string(),
                )
            }
        };

        governor_details.push(GovernorVoteDetail {
            province_name: province.name.clone(),
            province_owner: province.owner,
            owner_type,
            voted_for: supported_gp,
            reason,
        });

        *vote_tally.entry(supported_gp).or_insert(0) += 1;
    }

    // Build sorted votes vector (descending by vote count).
    let mut votes: Vec<(NationId, u32)> = vote_tally.into_iter().collect();
    votes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.0.cmp(&b.0.0)));

    // Determine winner.
    let winner = if let Some(&(nation_id, count)) = votes.first() {
        if count >= majority_threshold {
            // Outright 2/3 majority.
            Some(nation_id)
        } else if is_final {
            // Final vote — most governors wins (already sorted descending).
            Some(nation_id)
        } else {
            None
        }
    } else {
        None
    };

    CouncilVoteResult {
        votes,
        total_governors,
        majority_threshold,
        winner,
        is_final,
        governor_details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::DiplomacyState;
    use crate::hex::HexCoord;
    use crate::map::Province;
    use crate::nation::{Nation, NationColor};

    // ── Helpers ────────────────────────────────────────────────────

    /// Build a Great Power nation with configurable province count and worker count.
    fn make_great_power(id: u32, name: &str, province_count: u32, workers: u32) -> Nation {
        let mut nation = Nation::new(
            NationId(id),
            name.to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(id * 1000),
        );
        // Nation::new already adds the capital province, so add the remaining ones.
        for i in 1..province_count {
            nation.add_province(ProvinceId(id * 1000 + i));
        }
        nation.economy.labor.untrained = workers;
        nation
    }

    /// Build a Minor Nation with a given province count.
    fn make_minor_nation(id: u32, name: &str, province_count: u32) -> Nation {
        let mut nation = Nation::new(
            NationId(id),
            name.to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(id * 1000),
        );
        for i in 1..province_count {
            nation.add_province(ProvinceId(id * 1000 + i));
        }
        nation
    }

    /// Generate `count` provinces owned by the given nation.
    fn make_provinces(owner: NationId, start_id: u32, count: u32) -> Vec<Province> {
        (0..count)
            .map(|i| {
                Province::new(
                    ProvinceId(start_id + i),
                    format!("Province {}", start_id + i),
                    owner,
                    HexCoord::new(i as i32, 0),
                    vec![HexCoord::new(i as i32, 0)],
                    4,
                )
            })
            .collect()
    }

    // ── Score calculation ──────────────────────────────────────────

    #[test]
    fn score_with_known_nation_state() {
        let mut nation = Nation::new(
            NationId(1),
            "TestNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(100),
        );
        // Add 4 more provinces (total 5)
        for i in 1..5 {
            nation.add_province(ProvinceId(100 + i));
        }
        // Set up workers: 3 untrained + 2 trained = 5 total
        nation.economy.labor.untrained = 3;
        nation.economy.labor.trained = 2;

        let score = calculate_score(&nation);

        assert_eq!(score.military_score, 0); // placeholder
        assert_eq!(score.labor_score, 50); // 5 workers * 10
        assert_eq!(score.transport_score, 0); // placeholder
        assert_eq!(score.merchant_marine_score, 0); // placeholder
        assert_eq!(score.diplomatic_score, 50); // placeholder
        assert_eq!(score.province_score, 375); // 5 provinces * 75
        assert_eq!(score.tech_score, 0); // no techs researched
        assert_eq!(score.treasury_score, 0); // treasury is $0
        assert_eq!(score.building_score, 0); // no buildings
        assert_eq!(score.total, 475); // 0 + 50 + 0 + 0 + 50 + 375 + 0 + 0 + 0
    }

    #[test]
    fn score_default_is_zeroed() {
        let score = NationScore::default();
        assert_eq!(score.total, 0);
        assert_eq!(score.military_score, 0);
        assert_eq!(score.province_score, 0);
    }

    // ── governor_vote: GP province always votes for own GP ────────

    #[test]
    fn governor_vote_gp_always_votes_own() {
        let diplomacy = DiplomacyState::new();
        let gp_ids = vec![NationId(1), NationId(2)];

        // A GP province owner always votes for itself
        let vote = governor_vote(NationId(1), NationType::GreatPower, &diplomacy, &gp_ids);
        assert_eq!(vote, NationId(1));

        let vote2 = governor_vote(NationId(2), NationType::GreatPower, &diplomacy, &gp_ids);
        assert_eq!(vote2, NationId(2));
    }

    // ── governor_vote: MN province votes for GP with best relationship ──

    #[test]
    fn governor_vote_mn_votes_best_relationship() {
        let mut diplomacy = DiplomacyState::new();
        let gp_ids = vec![NationId(1), NationId(2), NationId(3)];

        // MN(10) has best relationship with GP(2)
        diplomacy.ensure_relation(NationId(10), NationId(1)).score = 10;
        diplomacy.ensure_relation(NationId(10), NationId(2)).score = 50;
        diplomacy.ensure_relation(NationId(10), NationId(3)).score = 30;

        let vote = governor_vote(NationId(10), NationType::MinorNation, &diplomacy, &gp_ids);
        assert_eq!(
            vote,
            NationId(2),
            "MN should vote for GP with highest relationship score"
        );
    }

    #[test]
    fn governor_vote_mn_tie_broken_by_lowest_id() {
        let mut diplomacy = DiplomacyState::new();
        let gp_ids = vec![NationId(1), NationId(2), NationId(3)];

        // MN(10) has equal relationship with GP(2) and GP(1)
        diplomacy.ensure_relation(NationId(10), NationId(1)).score = 50;
        diplomacy.ensure_relation(NationId(10), NationId(2)).score = 50;
        diplomacy.ensure_relation(NationId(10), NationId(3)).score = 20;

        let vote = governor_vote(NationId(10), NationType::MinorNation, &diplomacy, &gp_ids);
        assert_eq!(vote, NationId(1), "Tie should be broken by lowest NationId");
    }

    // ── Council vote uses diplomatic relationships ────────────────

    #[test]
    fn council_vote_uses_diplomatic_relationships() {
        // GP1: 30 provinces, GP2: 30 provinces, MN: 60 provinces
        // Total = 120, threshold = 80
        // MN(10) has best relationship with GP2 => GP2 gets 30 + 60 = 90 votes
        let gp1 = make_great_power(1, "Alpha", 30, 0);
        let gp2 = make_great_power(2, "Beta", 30, 0);
        let minor = make_minor_nation(10, "MinorNat", 60);

        let nations = vec![gp1, gp2, minor];

        let mut provinces = make_provinces(NationId(1), 1, 30);
        provinces.extend(make_provinces(NationId(2), 100, 30));
        provinces.extend(make_provinces(NationId(10), 200, 60));

        let mut diplomacy = DiplomacyState::new();
        // MN(10) prefers GP(2)
        diplomacy.ensure_relation(NationId(10), NationId(1)).score = 10;
        diplomacy.ensure_relation(NationId(10), NationId(2)).score = 60;

        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 120);
        assert!(result.winner.is_some());
        // GP2 should win because MN votes go to GP2 (higher relationship)
        assert_eq!(result.winner.unwrap(), NationId(2));
    }

    // ── Council vote: clear majority ──────────────────────────────

    #[test]
    fn council_vote_clear_majority() {
        // GP1 controls 85 provinces, GP2 controls 15, minor has 20
        // Total = 120 provinces. 2/3 threshold = 80.
        // With no diplomacy, MN governors default to GP with lowest ID (score 0 tie).
        // GP1 gets 85 + 20 = 105.
        let gp1 = make_great_power(1, "Empire", 85, 10);
        let gp2 = make_great_power(2, "Republic", 15, 2);
        let minor = make_minor_nation(10, "SmallLand", 20);

        let nations = vec![gp1, gp2, minor];

        let mut provinces = make_provinces(NationId(1), 1, 85);
        provinces.extend(make_provinces(NationId(2), 100, 15));
        provinces.extend(make_provinces(NationId(10), 200, 20));

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 120);
        assert_eq!(result.majority_threshold, 80);
        assert!(result.winner.is_some());
        assert_eq!(result.winner.unwrap(), NationId(1));
        assert!(!result.is_final);
    }

    // ── Council vote: no majority ─────────────────────────────────

    #[test]
    fn council_vote_no_majority() {
        // GP1: 35, GP2: 35, GP3: 10, minor: 40. Total=120, threshold=80.
        // With no diplomacy, MN governors default to lowest GP ID (GP1).
        // GP1 = 35 + 40 = 75 < 80. No majority.
        let gp1 = make_great_power(1, "Alpha", 35, 0);
        let gp2 = make_great_power(2, "Beta", 35, 0);
        let gp3 = make_great_power(3, "Gamma", 10, 0);
        let minor = make_minor_nation(10, "MinorNat", 40);

        let nations = vec![gp1, gp2, gp3, minor];

        let mut provinces = make_provinces(NationId(1), 1, 35);
        provinces.extend(make_provinces(NationId(2), 100, 35));
        provinces.extend(make_provinces(NationId(3), 200, 10));
        provinces.extend(make_provinces(NationId(10), 300, 40));

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 120);
        assert_eq!(result.majority_threshold, 80);
        assert!(result.winner.is_none());
        assert!(!result.is_final);
    }

    // ── Final vote: picks most governors when no 2/3 majority ─────

    #[test]
    fn final_vote_picks_most_governors() {
        // Same setup as no-majority test, but is_final = true.
        let gp1 = make_great_power(1, "Alpha", 35, 0);
        let gp2 = make_great_power(2, "Beta", 35, 0);
        let gp3 = make_great_power(3, "Gamma", 10, 0);
        let minor = make_minor_nation(10, "MinorNat", 40);

        let nations = vec![gp1, gp2, gp3, minor];

        let mut provinces = make_provinces(NationId(1), 1, 35);
        provinces.extend(make_provinces(NationId(2), 100, 35));
        provinces.extend(make_provinces(NationId(3), 200, 10));
        provinces.extend(make_provinces(NationId(10), 300, 40));

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, true, &diplomacy);

        assert!(result.is_final);
        // GP1 gets 35 own + 40 minor = 75 (default tie goes to lowest ID).
        // GP2 gets 35. GP3 gets 10.
        // 75 < 80 threshold, but is_final, so GP1 wins by most governors.
        assert!(result.winner.is_some());
        assert_eq!(result.winner.unwrap(), NationId(1));
    }

    // ── Majority threshold is 2/3 of total governors ──────────────

    #[test]
    fn majority_threshold_is_two_thirds() {
        // 120 provinces => threshold = (120*2+2)/3 = 242/3 = 80
        let gp = make_great_power(1, "Solo", 120, 0);
        let nations = vec![gp];
        let provinces = make_provinces(NationId(1), 1, 120);

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 120);
        assert_eq!(result.majority_threshold, 80);
    }

    #[test]
    fn majority_threshold_rounds_up_for_non_divisible() {
        // 10 provinces => threshold = (10*2+2)/3 = 22/3 = 7 (integer division)
        // 2/3 of 10 = 6.67, rounded up = 7.
        let gp = make_great_power(1, "Solo", 10, 0);
        let nations = vec![gp];
        let provinces = make_provinces(NationId(1), 1, 10);

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 10);
        assert_eq!(result.majority_threshold, 7);
    }

    #[test]
    fn empty_provinces_produces_no_winner() {
        let gp = make_great_power(1, "Solo", 1, 0);
        let nations = vec![gp];
        let provinces: Vec<Province> = Vec::new();

        let diplomacy = DiplomacyState::new();
        let result = run_council_vote(&nations, &provinces, false, &diplomacy);

        assert_eq!(result.total_governors, 0);
        assert!(result.winner.is_none());
        assert!(result.votes.is_empty());
    }
}
