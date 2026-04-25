-- Procedural government mix used by the flavor crate.
--
-- Two top-level globals: `great_power_mix` and `minor_nation_mix`.
-- Each maps a GovernmentForm identifier (matching the Rust enum variant
-- name) to a non-negative weight. Weights are relative; a weight of 0
-- effectively disables a form. Unknown identifiers are ignored with a
-- warning at parse time.
--
-- Edited freely; the parser tolerates blank lines, line comments, and
-- trailing commas. Numeric values may be integer or decimal.

great_power_mix = {
    Empire = 25,
    Kingdom = 25,
    ConstitutionalMonarchy = 20,
    AbsoluteMonarchy = 10,
    Republic = 10,
    FederalRepublic = 5,
    Shogunate = 2.5,
    Sultanate = 2.5,
}

minor_nation_mix = {
    Duchy = 15,
    Principality = 15,
    GrandDuchy = 10,
    CityState = 10,
    Kingdom = 10,
    Emirate = 8,
    Khanate = 8,
    TribalConfederacy = 8,
    Theocracy = 5,
    Dominion = 5,
    Confederation = 3,
    MilitaryJunta = 3,
}
