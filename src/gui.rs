#![deny(warnings)]

use clap::Parser;
use domain::types::Difficulty;

#[derive(Parser)]
#[command(
    name = "imperialism-gui",
    about = "Imperialism Remake — graphical interface"
)]
struct GuiArgs {
    /// Map key for world generation
    map_key: Option<String>,

    /// Nation index (0-based)
    nation_index: Option<usize>,

    /// Load a scenario by ID (not yet supported in GUI — use CLI instead)
    #[arg(long)]
    scenario: Option<String>,
}

fn main() {
    let args = GuiArgs::parse();

    if args.scenario.is_some() {
        eprintln!("Error: --scenario is not yet supported in the GUI binary.");
        eprintln!("Use the CLI binary instead: imperialism --scenario <id>");
        std::process::exit(1);
    }

    let map_key = args.map_key.as_deref().unwrap_or("imperialism");
    let nation_index = args.nation_index.unwrap_or(0);

    presentation::run_game(map_key, Difficulty::Normal, nation_index);
}
