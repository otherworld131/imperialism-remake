use clap::Parser;

#[derive(Parser)]
#[command(
    name = "imperialism",
    about = "Imperialism Remake — a turn-based grand strategy game"
)]
pub(crate) struct CliArgs {
    /// Map key for world generation
    pub map_key: Option<String>,

    /// Nation index (0-based)
    pub nation_index: Option<usize>,

    /// Run N headless AI-only games and output JSON report
    #[arg(long)]
    pub batch: Option<u32>,

    /// Load a scenario by ID
    #[arg(long)]
    pub scenario: Option<String>,

    /// Enable AI debug output
    #[arg(long)]
    pub ai_debug: bool,

    /// In batch mode, include per-turn cash-flow breakdowns for every GP in
    /// the JSON report. Default is cumulative year-snapshot totals only;
    /// this flag switches to a firehose that records income/expense by
    /// source/sink for every turn. Expect output size to grow ~15x per game.
    #[arg(long)]
    pub batch_verbose_cashflow: bool,

    /// In batch mode, stop each game after this many turns instead of
    /// running to 1915 game-over. Useful for fast smoke tests.
    #[arg(long)]
    pub batch_max_turns: Option<u32>,

    /// Load a save file from saves/ at startup instead of starting a new game.
    #[arg(long)]
    pub load: Option<String>,

    /// Diagnostic: after loading (or starting), run AI once and dump per-GP
    /// transport state as JSON to stdout, then exit. Skips the REPL.
    #[arg(long)]
    pub dump_transport: bool,

    /// With --dump-transport, force observer_mode=true so AI runs on every
    /// Great Power (including the saved human player's nation).
    #[arg(long)]
    pub force_observer: bool,

    /// With --dump-transport, advance this many turns before dumping (lets us
    /// inspect AI behavior at late game without needing a saved file).
    #[arg(long, default_value_t = 1)]
    pub auto_turns: u32,
}
