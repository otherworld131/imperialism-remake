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
    pub total: u32,
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
    let military_score = 0; // TODO: sum firepower of all army units (placeholder)
    let labor_score = nation.labor.total_workers() * 10;
    let transport_score = 0; // TODO: freight cars + railroad miles
    let merchant_marine_score = 0; // TODO: cargo capacity
    let diplomatic_score = 50; // placeholder
    let province_score = nation.province_count() as u32 * 100;

    let total = military_score
        + labor_score
        + transport_score
        + merchant_marine_score
        + diplomatic_score
        + province_score;

    NationScore {
        military_score,
        labor_score,
        transport_score,
        merchant_marine_score,
        diplomatic_score,
        province_score,
        total,
    }
}

/// Run the Council of Governors vote.
///
/// Each province has one governor. Governor voting preference:
/// - **Great Power provinces**: the governor always supports their own nation.
/// - **Minor Nation provinces**: the governor supports the Great Power with the
///   highest total score (simplified heuristic).
///
/// A nation wins outright if it secures at least 2/3 of total governors.
/// On the final vote (1915) with no 2/3 majority, the nation with the most
/// governors wins by default.
pub fn run_council_vote(
    nations: &[Nation],
    provinces: &[Province],
    is_final: bool,
) -> CouncilVoteResult {
    let total_governors = provinces.len() as u32;
    // 2/3 majority — rounding up so the threshold is strict.
    let majority_threshold = (total_governors * 2).div_ceil(3);

    // Pre-compute scores for all Great Powers so we can determine
    // which GP the minor-nation governors prefer.
    let great_powers: Vec<(NationId, u32)> = nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| (n.id, calculate_score(n).total))
        .collect();

    // Find the GP with the highest score (tie-break: lowest NationId).
    let best_gp = great_powers
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.0.cmp(&a.0.0)));

    // Tally governor votes per Great Power.
    let mut vote_tally: std::collections::HashMap<NationId, u32> = std::collections::HashMap::new();

    for province in provinces {
        // Determine which nation the governor of this province supports.
        let owner_nation = nations.iter().find(|n| n.id == province.owner);

        let supported_gp = match owner_nation {
            Some(n) if n.is_great_power() => {
                // Great Power governor supports own nation.
                n.id
            }
            _ => {
                // Minor Nation governor (or unknown owner) supports the
                // GP with the highest score.
                match best_gp {
                    Some(&(gp_id, _)) => gp_id,
                    None => continue, // no Great Powers — skip
                }
            }
        };

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        nation.labor.untrained = workers;
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
        nation.labor.untrained = 3;
        nation.labor.trained = 2;

        let score = calculate_score(&nation);

        assert_eq!(score.military_score, 0); // placeholder
        assert_eq!(score.labor_score, 50); // 5 workers * 10
        assert_eq!(score.transport_score, 0); // placeholder
        assert_eq!(score.merchant_marine_score, 0); // placeholder
        assert_eq!(score.diplomatic_score, 50); // placeholder
        assert_eq!(score.province_score, 500); // 5 provinces * 100
        assert_eq!(score.total, 600); // 0 + 50 + 0 + 0 + 50 + 500
    }

    #[test]
    fn score_default_is_zeroed() {
        let score = NationScore::default();
        assert_eq!(score.total, 0);
        assert_eq!(score.military_score, 0);
        assert_eq!(score.province_score, 0);
    }

    // ── Council vote: clear majority ──────────────────────────────

    #[test]
    fn council_vote_clear_majority() {
        // GP1 controls 85 provinces, GP2 controls 15, minor has 20
        // Total = 120 provinces. 2/3 threshold = 80.
        // GP1 governors = 85, minor nation governors all back GP1 (highest score),
        // so GP1 gets 85 + 20 = 105.
        let gp1 = make_great_power(1, "Empire", 85, 10);
        let gp2 = make_great_power(2, "Republic", 15, 2);
        let minor = make_minor_nation(10, "SmallLand", 20);

        let nations = vec![gp1, gp2, minor];

        let mut provinces = make_provinces(NationId(1), 1, 85);
        provinces.extend(make_provinces(NationId(2), 100, 15));
        provinces.extend(make_provinces(NationId(10), 200, 20));

        let result = run_council_vote(&nations, &provinces, false);

        assert_eq!(result.total_governors, 120);
        assert_eq!(result.majority_threshold, 80);
        assert!(result.winner.is_some());
        assert_eq!(result.winner.unwrap(), NationId(1));
        assert!(!result.is_final);
    }

    // ── Council vote: no majority ─────────────────────────────────

    #[test]
    fn council_vote_no_majority() {
        // GP1: 40 provinces, GP2: 40 provinces, minor: 40 provinces
        // Total = 120. Threshold = 80.
        // GP1 score = 40*100 + 50 = 4050, GP2 score = 40*100 + 50 = 4050.
        // Tie in score — minor governors go to lower NationId (GP1).
        // GP1 gets 40 + 40 = 80. Threshold is 80. That equals the threshold.
        // Actually let's make it so neither reaches 2/3:
        // GP1: 30, GP2: 50, minor: 40.
        // GP2 score = 50*100+50 = 5050 > GP1 score = 30*100+50 = 3050.
        // Minor governors back GP2. GP2 gets 50+40 = 90. That's a majority.
        //
        // To get no majority: 3 GPs splitting votes + minor backing only one.
        // GP1: 35, GP2: 35, GP3: 10, minor: 40. Total=120, threshold=80.
        // GP2 and GP1 have same score (3550). Minor backs GP1 (lower id).
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

        let result = run_council_vote(&nations, &provinces, false);

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

        let result = run_council_vote(&nations, &provinces, true);

        assert!(result.is_final);
        // GP1 gets 35 own + 40 minor = 75. GP2 gets 35. GP3 gets 10.
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

        let result = run_council_vote(&nations, &provinces, false);

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

        let result = run_council_vote(&nations, &provinces, false);

        assert_eq!(result.total_governors, 10);
        assert_eq!(result.majority_threshold, 7);
    }

    #[test]
    fn empty_provinces_produces_no_winner() {
        let gp = make_great_power(1, "Solo", 1, 0);
        let nations = vec![gp];
        let provinces: Vec<Province> = Vec::new();

        let result = run_council_vote(&nations, &provinces, false);

        assert_eq!(result.total_governors, 0);
        assert!(result.winner.is_none());
        assert!(result.votes.is_empty());
    }
}
