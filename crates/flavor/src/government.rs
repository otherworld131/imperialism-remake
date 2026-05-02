//! 19th-century-appropriate government forms used only for display/flavor.
//!
//! Each variant carries a short-label for the UI and a title template used
//! to turn a raw country name into a formal title (e.g. `Devronia` →
//! `Empire of Devronia`).

use crate::rng::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GovernmentForm {
    AbsoluteMonarchy,
    ConstitutionalMonarchy,
    Empire,
    Kingdom,
    Republic,
    FederalRepublic,
    Confederation,
    Duchy,
    Principality,
    GrandDuchy,
    CityState,
    Theocracy,
    Shogunate,
    Khanate,
    Sultanate,
    Emirate,
    TribalConfederacy,
    MilitaryJunta,
    Dominion,
}

impl GovernmentForm {
    pub const ALL: &'static [GovernmentForm] = &[
        GovernmentForm::AbsoluteMonarchy,
        GovernmentForm::ConstitutionalMonarchy,
        GovernmentForm::Empire,
        GovernmentForm::Kingdom,
        GovernmentForm::Republic,
        GovernmentForm::FederalRepublic,
        GovernmentForm::Confederation,
        GovernmentForm::Duchy,
        GovernmentForm::Principality,
        GovernmentForm::GrandDuchy,
        GovernmentForm::CityState,
        GovernmentForm::Theocracy,
        GovernmentForm::Shogunate,
        GovernmentForm::Khanate,
        GovernmentForm::Sultanate,
        GovernmentForm::Emirate,
        GovernmentForm::TribalConfederacy,
        GovernmentForm::MilitaryJunta,
        GovernmentForm::Dominion,
    ];

    /// Parse a form identifier (e.g. "Empire") into the matching variant.
    pub fn parse(name: &str) -> Option<GovernmentForm> {
        use GovernmentForm::*;
        let v = match name {
            "AbsoluteMonarchy" => AbsoluteMonarchy,
            "ConstitutionalMonarchy" => ConstitutionalMonarchy,
            "Empire" => Empire,
            "Kingdom" => Kingdom,
            "Republic" => Republic,
            "FederalRepublic" => FederalRepublic,
            "Confederation" => Confederation,
            "Duchy" => Duchy,
            "Principality" => Principality,
            "GrandDuchy" => GrandDuchy,
            "CityState" => CityState,
            "Theocracy" => Theocracy,
            "Shogunate" => Shogunate,
            "Khanate" => Khanate,
            "Sultanate" => Sultanate,
            "Emirate" => Emirate,
            "TribalConfederacy" => TribalConfederacy,
            "MilitaryJunta" => MilitaryJunta,
            "Dominion" => Dominion,
            _ => return None,
        };
        Some(v)
    }

    /// Short label shown in the UI (no country name).
    pub fn short_label(self) -> &'static str {
        use GovernmentForm::*;
        match self {
            AbsoluteMonarchy => "Absolute Monarchy",
            ConstitutionalMonarchy => "Constitutional Monarchy",
            Empire => "Empire",
            Kingdom => "Kingdom",
            Republic => "Republic",
            FederalRepublic => "Federal Republic",
            Confederation => "Confederation",
            Duchy => "Duchy",
            Principality => "Principality",
            GrandDuchy => "Grand Duchy",
            CityState => "Free City",
            Theocracy => "Theocracy",
            Shogunate => "Shogunate",
            Khanate => "Khanate",
            Sultanate => "Sultanate",
            Emirate => "Emirate",
            TribalConfederacy => "Tribal Confederacy",
            MilitaryJunta => "Military Junta",
            Dominion => "Dominion",
        }
    }

    /// One-line prose describing the form (used in country-info panels).
    pub fn description(self) -> &'static str {
        use GovernmentForm::*;
        match self {
            AbsoluteMonarchy => "A hereditary ruler whose will is law.",
            ConstitutionalMonarchy => "A crowned head of state bound by parliament and charter.",
            Empire => "A multi-national realm presided over by an emperor.",
            Kingdom => "A sovereign realm under a king or queen.",
            Republic => "Citizens elect the head of state and legislature.",
            FederalRepublic => "Autonomous states bound by a shared federal constitution.",
            Confederation => "A league of sovereign polities cooperating on defence and trade.",
            Duchy => "A territory ruled by a duke owing fealty to no one.",
            Principality => "A small sovereign realm ruled by a reigning prince.",
            GrandDuchy => "A senior duchy whose ruler holds the rank of grand duke.",
            CityState => "An autonomous merchant city governing its own hinterland.",
            Theocracy => "Religious authorities exercise both temporal and spiritual power.",
            Shogunate => "A hereditary military dictatorship beneath a ceremonial throne.",
            Khanate => "A steppe realm led by a khan drawn from a ruling clan.",
            Sultanate => "An Islamic realm governed by a sultan.",
            Emirate => "A smaller realm governed by an emir.",
            TribalConfederacy => "Allied clans who elect their paramount chief in council.",
            MilitaryJunta => "A council of officers ruling by decree.",
            Dominion => "A self-governing polity within a larger imperial system.",
        }
    }

    /// Whether this form is typical for a Great Power (GP).
    pub fn is_great_power_form(self) -> bool {
        use GovernmentForm::*;
        matches!(
            self,
            AbsoluteMonarchy
                | ConstitutionalMonarchy
                | Empire
                | Kingdom
                | Republic
                | FederalRepublic
                | Shogunate
                | Sultanate
        )
    }

    /// Whether this form is typical for a Minor Nation.
    pub fn is_minor_nation_form(self) -> bool {
        use GovernmentForm::*;
        matches!(
            self,
            Duchy
                | Principality
                | GrandDuchy
                | CityState
                | Theocracy
                | Khanate
                | Emirate
                | TribalConfederacy
                | MilitaryJunta
                | Dominion
                | Confederation
                | Kingdom
        )
    }
}

/// A weighted distribution over government forms. Callers supply raw
/// weights (percentages are fine — they're normalized internally) and
/// `pick` draws one form deterministically from the RNG.
///
/// Example — "10 countries, 80% monarchies, 20% republics":
/// ```
/// # use flavor::{GovernmentMix, GovernmentForm, Rng};
/// let mix = GovernmentMix::new()
///     .add(GovernmentForm::Kingdom, 80.0)
///     .add(GovernmentForm::Republic, 20.0);
/// let mut rng = Rng::from_seed(1);
/// let forms: Vec<_> = (0..10).map(|_| mix.pick(&mut rng)).collect();
/// assert_eq!(forms.len(), 10);
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GovernmentMix {
    pub weights: Vec<(GovernmentForm, f32)>,
}

impl GovernmentMix {
    pub fn new() -> Self {
        Self {
            weights: Vec::new(),
        }
    }

    /// Fluent builder — adds (or overwrites) the weight for `form`. A weight
    /// of `0.0` effectively disables the form.
    pub fn add(mut self, form: GovernmentForm, weight: f32) -> Self {
        if let Some(slot) = self.weights.iter_mut().find(|(f, _)| *f == form) {
            slot.1 = weight;
        } else {
            self.weights.push((form, weight));
        }
        self
    }

    /// Preset: a plausible mix of Great Power government forms (mid-19th
    /// century flavor — lots of empires and kingdoms, some republics).
    pub fn great_power_default() -> Self {
        use GovernmentForm::*;
        Self::new()
            .add(Empire, 25.0)
            .add(Kingdom, 25.0)
            .add(ConstitutionalMonarchy, 20.0)
            .add(AbsoluteMonarchy, 10.0)
            .add(Republic, 10.0)
            .add(FederalRepublic, 5.0)
            .add(Shogunate, 2.5)
            .add(Sultanate, 2.5)
    }

    /// Preset: a plausible mix of Minor-Nation government forms (small
    /// duchies, principalities, city-states, tribal polities).
    pub fn minor_nation_default() -> Self {
        use GovernmentForm::*;
        Self::new()
            .add(Duchy, 15.0)
            .add(Principality, 15.0)
            .add(GrandDuchy, 10.0)
            .add(CityState, 10.0)
            .add(Kingdom, 10.0)
            .add(Emirate, 8.0)
            .add(Khanate, 8.0)
            .add(TribalConfederacy, 8.0)
            .add(Theocracy, 5.0)
            .add(Dominion, 5.0)
            .add(Confederation, 3.0)
            .add(MilitaryJunta, 3.0)
    }

    fn total(&self) -> f32 {
        self.weights.iter().map(|(_, w)| w.max(0.0)).sum()
    }

    /// Pick one government form from the distribution. If the mix is empty
    /// or all weights are zero, falls back to `ConstitutionalMonarchy` so
    /// callers always get a valid value.
    pub fn pick(&self, rng: &mut Rng) -> GovernmentForm {
        let total = self.total();
        if total <= 0.0 || self.weights.is_empty() {
            return GovernmentForm::ConstitutionalMonarchy;
        }
        let mut roll = rng.unit() * total;
        for (form, w) in &self.weights {
            let w = w.max(0.0);
            if roll < w {
                return *form;
            }
            roll -= w;
        }
        // Float-rounding fallback — return the last entry with nonzero weight.
        self.weights
            .iter()
            .rev()
            .find(|(_, w)| *w > 0.0)
            .map(|(f, _)| *f)
            .unwrap_or(GovernmentForm::ConstitutionalMonarchy)
    }
}

/// Legacy helper — picks a random government form biased toward GP or MN
/// variants. Retained as a thin wrapper over `GovernmentMix` for tests and
/// any caller that doesn't need a custom distribution.
pub fn random(rng: &mut Rng, is_great_power: bool) -> GovernmentForm {
    let mix = if is_great_power {
        GovernmentMix::great_power_default()
    } else {
        GovernmentMix::minor_nation_default()
    };
    mix.pick(rng)
}

/// Compose the full formal title: e.g. `Empire of Devronia`, `Free City of
/// Kessel`, `Republic of Pontar`.
pub fn government_title(country_name: &str, form: GovernmentForm) -> String {
    use GovernmentForm::*;
    let connector = match form {
        // "Empire of X" — genitive.
        AbsoluteMonarchy
        | ConstitutionalMonarchy
        | Empire
        | Kingdom
        | Republic
        | FederalRepublic
        | Duchy
        | Principality
        | GrandDuchy
        | CityState
        | Theocracy
        | Shogunate
        | Khanate
        | Sultanate
        | Emirate
        | Dominion => "of",
        // "Confederation of the X" reads oddly — use "of".
        Confederation | TribalConfederacy => "of",
        MilitaryJunta => "of",
    };
    format!("{} {} {}", form.short_label(), connector, country_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_has_a_nonempty_label_and_description() {
        for form in GovernmentForm::ALL {
            assert!(!form.short_label().is_empty());
            assert!(!form.description().is_empty());
        }
    }

    #[test]
    fn random_gp_returns_gp_form() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..50 {
            let form = random(&mut rng, true);
            assert!(form.is_great_power_form(), "{form:?} is not a GP form");
        }
    }

    #[test]
    fn random_minor_returns_minor_form() {
        let mut rng = Rng::from_seed(11);
        for _ in 0..50 {
            let form = random(&mut rng, false);
            assert!(form.is_minor_nation_form(), "{form:?} is not a minor form");
        }
    }

    #[test]
    fn title_includes_country_name() {
        let t = government_title("Devronia", GovernmentForm::Empire);
        assert!(t.contains("Devronia"));
        assert!(t.starts_with("Empire"));
    }

    #[test]
    fn mix_is_empty_safe() {
        let mix = GovernmentMix::new();
        let mut rng = Rng::from_seed(1);
        // An empty mix must not panic — returns the safe fallback.
        let form = mix.pick(&mut rng);
        assert_eq!(form, GovernmentForm::ConstitutionalMonarchy);
    }

    #[test]
    fn mix_respects_weights_approximately() {
        // 80/20 split between Kingdom and Republic — after 10_000 draws the
        // tally should be close to the target ratio.
        let mix = GovernmentMix::new()
            .add(GovernmentForm::Kingdom, 80.0)
            .add(GovernmentForm::Republic, 20.0);
        let mut rng = Rng::from_seed(42);
        let mut kingdoms = 0;
        let mut republics = 0;
        for _ in 0..10_000 {
            match mix.pick(&mut rng) {
                GovernmentForm::Kingdom => kingdoms += 1,
                GovernmentForm::Republic => republics += 1,
                other => panic!("unexpected form {other:?}"),
            }
        }
        // Allow a 3% slack on either side.
        let ratio = kingdoms as f32 / (kingdoms + republics) as f32;
        assert!(
            (0.77..=0.83).contains(&ratio),
            "expected ~80% kingdoms, got {ratio}"
        );
    }

    #[test]
    fn mix_zero_weight_never_chosen() {
        let mix = GovernmentMix::new()
            .add(GovernmentForm::Kingdom, 1.0)
            .add(GovernmentForm::Republic, 0.0);
        let mut rng = Rng::from_seed(3);
        for _ in 0..500 {
            assert_eq!(mix.pick(&mut rng), GovernmentForm::Kingdom);
        }
    }
}
