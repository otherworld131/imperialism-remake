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
}
