//! Procedural country and city name generator.
//!
//! Names are built from syllables (onset + vowel + optional coda) and then
//! reshaped by a grammatical *pattern* that guarantees the adjective and
//! demonym forms read consistently (e.g. `Devronia` → `Devronian`,
//! `Devronians`; `Kessland` → `Kessish`, `Kesslanders`).
//!
//! The output is deterministic given a seed.

use crate::rng::Rng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CountryName {
    /// Bare country name, e.g. "Devronia".
    pub name: String,
    /// Adjectival form, e.g. "Devronian" in "the Devronian army".
    pub adjective: String,
    /// Singular demonym, e.g. "Devronian" in "a Devronian merchant".
    pub demonym_singular: String,
    /// Plural demonym, e.g. "Devronians" in "the Devronians revolted".
    pub demonym_plural: String,
}

const ONSETS: &[&str] = &[
    "b", "c", "d", "f", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "z", "br", "cr",
    "dr", "fr", "gl", "gr", "kh", "pr", "sh", "sl", "st", "th", "tr",
];

// Keep the vowel pool mostly single-letter so names stay clean and short.
const VOWELS: &[&str] = &[
    "a", "a", "e", "e", "i", "i", "o", "o", "u", "ae", "ai", "ia", "io",
];

// Codas are biased heavily toward "none" so most syllables end in a vowel —
// this prevents long consonant clusters like "rkst" from stacking up.
const CODAS: &[&str] = &[
    "", "", "", "", "", "", "", "", "n", "r", "s", "l", "m", "t", "rd", "st",
];

/// Substrings that — if they appear anywhere in a generated stem — cause
/// regeneration. English profanity or unfortunate coincidences. The list is
/// deliberately small; add to it as offenders show up.
const BLOCKED_SUBSTRINGS: &[&str] = &[
    "piss", "shit", "fuck", "cunt", "cock", "dick", "tit", "slut", "nazi", "jap", "nigg",
    "ass",
];

/// Capitalize the first ASCII character of `s`.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(c).collect(),
    }
}

/// Build a short stem — 2 syllables with restrained codas to keep the final
/// country name readable. Regenerates up to a few times if the stem hits the
/// blocklist.
fn build_stem(rng: &mut Rng, end_on_vowel: bool) -> String {
    for _ in 0..8 {
        let n_syll = if rng.chance(0.25) { 3 } else { 2 };
        let mut out = String::new();
        for i in 0..n_syll {
            let onset = rng.pick(ONSETS);
            let vowel = rng.pick(VOWELS);
            out.push_str(onset);
            out.push_str(vowel);
            let last = i + 1 == n_syll;
            if last && end_on_vowel {
                // leave the vowel as the last character
            } else {
                let coda = rng.pick(CODAS);
                out.push_str(coda);
            }
        }
        if !BLOCKED_SUBSTRINGS.iter().any(|bad| out.contains(bad)) {
            return out;
        }
    }
    // Escape-hatch — return a bland but safe stem if every retry was blocked.
    "aldor".to_string()
}

/// Convenience: strip a trailing vowel from a stem (used by some patterns
/// that expect a consonantal base, e.g. `Devro` + `nian`).
fn trim_trailing_vowel(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return s.to_string();
    }
    let last = bytes[bytes.len() - 1];
    if matches!(last, b'a' | b'e' | b'i' | b'o' | b'u' | b'y') {
        s[..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// All grammatical patterns we support. Each pattern knows how to derive
/// `(country, adjective, demonym_singular, demonym_plural)` from a raw stem.
#[derive(Debug, Clone, Copy)]
enum Pattern {
    LatinateIa,   // Devron -> Devronia, Devronian, Devronians
    GermanicLand, // Kess    -> Kessland, Kessish, Kesslander(s)
    GermanicMark, // Ven     -> Venmark, Venish, Venmarker(s)
    TurkicStan,   // Kaza    -> Kazastan, Kazai, Kazais
    EshteSuffix,  // Bang    -> Bangesh, Bangeshi, Bangeshis
    RomanicAr,    // Sard    -> Sardaria, Sardarian, Sardarians
    CitystateBurg, // Ham    -> Hamburg, Hamburgian, Hamburger(s)
    AsianTese,    // Hun     -> Hunang, Hunese, Hunese
}

const PATTERNS: &[Pattern] = &[
    Pattern::LatinateIa,
    Pattern::LatinateIa, // double-weight the most grammatical pattern
    Pattern::GermanicLand,
    Pattern::GermanicMark,
    Pattern::TurkicStan,
    Pattern::EshteSuffix,
    Pattern::RomanicAr,
    Pattern::CitystateBurg,
    Pattern::AsianTese,
];

fn apply_pattern(stem: &str, pattern: Pattern) -> CountryName {
    let stem = capitalize(stem);
    match pattern {
        Pattern::LatinateIa => {
            let root = trim_trailing_vowel(&stem);
            let name = format!("{root}ia");
            let adj = format!("{root}ian");
            CountryName {
                adjective: adj.clone(),
                demonym_singular: adj.clone(),
                demonym_plural: format!("{adj}s"),
                name,
            }
        }
        Pattern::GermanicLand => {
            let name = format!("{stem}land");
            let adj = format!("{stem}ish");
            let dem = format!("{stem}lander");
            CountryName {
                name,
                adjective: adj,
                demonym_singular: dem.clone(),
                demonym_plural: format!("{dem}s"),
            }
        }
        Pattern::GermanicMark => {
            let name = format!("{stem}mark");
            let adj = format!("{stem}ish");
            let dem = format!("{stem}marker");
            CountryName {
                name,
                adjective: adj,
                demonym_singular: dem.clone(),
                demonym_plural: format!("{dem}s"),
            }
        }
        Pattern::TurkicStan => {
            let root = trim_trailing_vowel(&stem);
            let name = format!("{root}astan");
            let adj = format!("{root}i");
            CountryName {
                name,
                adjective: adj.clone(),
                demonym_singular: adj.clone(),
                demonym_plural: format!("{adj}s"),
            }
        }
        Pattern::EshteSuffix => {
            let root = trim_trailing_vowel(&stem);
            let name = format!("{root}esh");
            let adj = format!("{root}eshi");
            CountryName {
                name,
                adjective: adj.clone(),
                demonym_singular: adj.clone(),
                demonym_plural: format!("{adj}s"),
            }
        }
        Pattern::RomanicAr => {
            let root = trim_trailing_vowel(&stem);
            let name = format!("{root}aria");
            let adj = format!("{root}arian");
            CountryName {
                name,
                adjective: adj.clone(),
                demonym_singular: adj.clone(),
                demonym_plural: format!("{adj}s"),
            }
        }
        Pattern::CitystateBurg => {
            let name = format!("{stem}burg");
            let adj = format!("{stem}burgian");
            CountryName {
                name,
                adjective: adj.clone(),
                demonym_singular: adj.clone(),
                demonym_plural: format!("{adj}s"),
            }
        }
        Pattern::AsianTese => {
            let root = trim_trailing_vowel(&stem);
            let name = format!("{root}ang");
            let d = format!("{root}ese");
            CountryName {
                name,
                adjective: d.clone(),
                demonym_singular: d.clone(),
                demonym_plural: d,
            }
        }
    }
}

fn pattern_suffix(p: Pattern) -> &'static str {
    match p {
        Pattern::LatinateIa => "ia",
        Pattern::GermanicLand => "land",
        Pattern::GermanicMark => "mark",
        Pattern::TurkicStan => "stan",
        Pattern::EshteSuffix => "esh",
        Pattern::RomanicAr => "aria",
        Pattern::CitystateBurg => "burg",
        Pattern::AsianTese => "ang",
    }
}

/// Generate one country name. If the randomly chosen pattern would duplicate
/// a suffix already present in the stem (e.g. `Mark` + `-mark` → `Markmark`),
/// fall back to the reliable `LatinateIa` pattern instead.
pub fn generate_country_name(rng: &mut Rng) -> CountryName {
    let pattern = *rng.pick(PATTERNS);
    let end_on_vowel = matches!(
        pattern,
        Pattern::LatinateIa
            | Pattern::TurkicStan
            | Pattern::EshteSuffix
            | Pattern::RomanicAr
            | Pattern::AsianTese
    );
    let stem = build_stem(rng, end_on_vowel);
    let lower = stem.to_ascii_lowercase();
    let effective = if lower.ends_with(pattern_suffix(pattern)) {
        Pattern::LatinateIa
    } else {
        pattern
    };
    apply_pattern(&stem, effective)
}

/// Generate `count` distinct country names (retries on collisions).
pub fn generate_country_names(rng: &mut Rng, count: usize) -> Vec<CountryName> {
    let mut out: Vec<CountryName> = Vec::with_capacity(count);
    let mut tries = 0;
    while out.len() < count && tries < count * 20 {
        let cand = generate_country_name(rng);
        if !out.iter().any(|c| c.name == cand.name) {
            out.push(cand);
        }
        tries += 1;
    }
    out
}

// ── City names ────────────────────────────────────────────────

const CITY_SUFFIXES: &[&str] = &[
    "burg", "ton", "ford", "port", "stadt", "grad", "gorod", "chester", "ville", "holm", "mouth",
    "haven", "field", "wich", "cester", "kent", "bury",
];

/// Prefixes (with trailing whitespace or hyphen) that prepend to some city
/// names. Left mostly empty so most cities are a single compound word.
const CITY_PREFIXES: &[&str] = &["", "", "", "", "", "New ", "Old ", "Saint ", "Port "];

/// Generate one city name.
pub fn generate_city_name(rng: &mut Rng) -> String {
    let stem = build_stem(rng, false);
    let stem = capitalize(&stem);
    let prefix = rng.pick(CITY_PREFIXES);
    let suffix = rng.pick(CITY_SUFFIXES);
    format!("{prefix}{stem}{suffix}")
}

/// Generate `count` distinct city names (retries on collisions).
pub fn generate_city_names(rng: &mut Rng, count: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(count);
    let mut tries = 0;
    while out.len() < count && tries < count * 20 {
        let cand = generate_city_name(rng);
        if !out.contains(&cand) {
            out.push(cand);
        }
        tries += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_non_empty_and_capitalized() {
        let mut rng = Rng::from_seed(1);
        for _ in 0..50 {
            let n = generate_country_name(&mut rng);
            assert!(!n.name.is_empty());
            assert!(!n.archives.adjective.is_empty());
            assert!(!n.archives.demonym_singular.is_empty());
            assert!(!n.archives.demonym_plural.is_empty());
            assert!(
                n.name.chars().next().unwrap().is_uppercase(),
                "{} not capitalized",
                n.name
            );
        }
    }

    #[test]
    fn same_seed_same_name() {
        let mut a = Rng::from_seed(999);
        let mut b = Rng::from_seed(999);
        for _ in 0..20 {
            assert_eq!(generate_country_name(&mut a).name, generate_country_name(&mut b).name);
        }
    }

    #[test]
    fn generate_n_distinct() {
        let mut rng = Rng::from_seed(5);
        let names = generate_country_names(&mut rng, 20);
        assert_eq!(names.len(), 20);
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(names[i].name, names[j].name);
            }
        }
    }

    #[test]
    fn cities_are_non_empty() {
        let mut rng = Rng::from_seed(17);
        for _ in 0..50 {
            let c = generate_city_name(&mut rng);
            assert!(!c.is_empty());
        }
    }

    #[test]
    fn demonym_plural_differs_when_pattern_adds_s() {
        // Not all patterns add `s` (AsianTese is invariant). Just ensure
        // that at least some generated names satisfy `plural = singular + "s"`.
        let mut rng = Rng::from_seed(3);
        let names = generate_country_names(&mut rng, 30);
        let any_s = names
            .iter()
            .any(|n| n.archives.demonym_plural == format!("{}s", n.archives.demonym_singular));
        assert!(any_s);
    }
}
