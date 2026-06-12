//! Standalone exerciser for the flavor crate.
//!
//! Usage:
//!     flavor-demo names [N] [--seed S]
//!     flavor-demo cities [N] [--seed S]
//!     flavor-demo governments
//!     flavor-demo flags [N] [--seed S] [--out DIR] [--form FORM]
//!     flavor-demo nations [N] [--seed S] [--mix kingdom=80,republic=20]
//!
//! The `--mix` flag takes a comma-separated `FORM=WEIGHT` list (case
//! insensitive, spaces stripped). Unknown form names are ignored with a
//! warning. Defaults to a balanced great-power mix.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use flavor::{
    FlagDesign, FlagRules, GovernmentForm, GovernmentMix, Rng, flags, generate_city_names,
    generate_country_names, generate_nations, government_title, names, svg_for,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }
    let cmd = args[0].as_str();
    let parsed = parse_common(&args[1..]);

    match cmd {
        "names" => run_names(parsed.count.unwrap_or(10), parsed.seed),
        "cities" => run_cities(parsed.count.unwrap_or(10), parsed.seed),
        "governments" => run_governments(),
        "flags" => run_flags(
            parsed.count.unwrap_or(5),
            parsed.seed,
            parsed.out,
            parsed.form,
        ),
        "nations" => run_nations(parsed.count.unwrap_or(10), parsed.seed, parsed.mix),
        _ => {
            print_usage();
            return ExitCode::from(1);
        }
    }
    ExitCode::from(0)
}

fn print_usage() {
    eprintln!(
        "usage: flavor-demo <names|cities|governments|flags|nations> [N] \
         [--seed S] [--out DIR] [--mix form=weight,...] [--form FORM]"
    );
}

struct ParsedArgs {
    count: Option<usize>,
    seed: u64,
    out: Option<PathBuf>,
    mix: Option<GovernmentMix>,
    form: Option<GovernmentForm>,
}

fn parse_common(args: &[String]) -> ParsedArgs {
    let mut out = ParsedArgs {
        count: None,
        seed: 42,
        out: None,
        mix: None,
        form: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--seed" => {
                if let Some(v) = args.get(i + 1)
                    && let Ok(s) = v.parse::<u64>()
                {
                    out.seed = s;
                }
                i += 2;
                continue;
            }
            "--out" => {
                if let Some(v) = args.get(i + 1) {
                    out.out = Some(PathBuf::from(v));
                }
                i += 2;
                continue;
            }
            "--mix" => {
                if let Some(v) = args.get(i + 1) {
                    out.mix = Some(parse_mix(v));
                }
                i += 2;
                continue;
            }
            "--form" => {
                if let Some(v) = args.get(i + 1) {
                    out.form = parse_form(v);
                }
                i += 2;
                continue;
            }
            other => {
                if let Ok(n) = other.parse::<usize>() {
                    out.count = Some(n);
                }
            }
        }
        i += 1;
    }
    out
}

fn parse_form(s: &str) -> Option<GovernmentForm> {
    let needle = s.trim().to_ascii_lowercase().replace(['_', '-', ' '], "");
    for form in GovernmentForm::ALL {
        let label = form.short_label().to_ascii_lowercase().replace(' ', "");
        let debug = format!("{form:?}").to_ascii_lowercase();
        if needle == label || needle == debug {
            return Some(*form);
        }
    }
    eprintln!("warning: unknown government form '{s}'");
    None
}

fn parse_mix(s: &str) -> GovernmentMix {
    let mut mix = GovernmentMix::new();
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (form_str, weight_str) = match entry.split_once('=') {
            Some(x) => x,
            None => {
                eprintln!("warning: mix entry '{entry}' missing '=weight' — ignored");
                continue;
            }
        };
        let form = match parse_form(form_str) {
            Some(f) => f,
            None => continue,
        };
        let weight: f32 = match weight_str.trim().parse() {
            Ok(w) => w,
            Err(_) => {
                eprintln!("warning: bad weight '{weight_str}' — ignored");
                continue;
            }
        };
        mix = mix.add(form, weight);
    }
    mix
}

fn run_names(n: usize, seed: u64) {
    let mut rng = Rng::from_seed(seed);
    let names = generate_country_names(&mut rng, n);
    println!(
        "{:<16} {:<16} {:<16} {:<16}",
        "NAME", "ADJECTIVE", "DEMONYM (S)", "DEMONYM (PL)"
    );
    for name in &names {
        println!(
            "{:<16} {:<16} {:<16} {:<16}",
            name.name, name.adjective, name.demonym_singular, name.demonym_plural
        );
    }
}

fn run_cities(n: usize, seed: u64) {
    let mut rng = Rng::from_seed(seed);
    let cities = generate_city_names(&mut rng, n);
    for c in cities {
        println!("{c}");
    }
}

fn run_governments() {
    println!("{:<26} DESCRIPTION", "FORM");
    for form in GovernmentForm::ALL {
        println!("{:<26} {}", form.short_label(), form.description());
    }
}

fn run_flags(n: usize, seed: u64, out: Option<PathBuf>, form_override: Option<GovernmentForm>) {
    let mut rng = Rng::from_seed(seed);
    let rules = FlagRules::default();
    let mut govt_seeder = Rng::from_seed(seed ^ 0xA11C);
    let gp_mix = GovernmentMix::great_power_default();
    let designs: Vec<(GovernmentForm, FlagDesign)> = (0..n)
        .map(|_| {
            let form = form_override.unwrap_or_else(|| gp_mix.pick(&mut govt_seeder));
            (form, flags::random_for(&mut rng, form, &rules))
        })
        .collect();
    if let Some(dir) = out {
        fs::create_dir_all(&dir).expect("create out dir");
        for (i, (form, d)) in designs.iter().enumerate() {
            let svg = svg_for(d);
            let path = dir.join(format!("flag-{i:02}.svg"));
            fs::write(&path, svg).expect("write svg");
            println!(
                "{}  ({form:?}, {:?}, {:?}, emblem={:?})",
                path.display(),
                d.pattern,
                d.colors,
                d.emblem
            );
        }
    } else {
        for (i, (form, d)) in designs.iter().enumerate() {
            println!(
                "=== flag {i} : {form:?} / pattern {:?} / emblem {:?} ===",
                d.pattern, d.emblem
            );
            println!("{}", svg_for(d));
        }
    }
}

fn run_nations(n: usize, seed: u64, mix: Option<GovernmentMix>) {
    let mut rng = Rng::from_seed(seed);
    let rules = FlagRules::default();
    let mix = mix.unwrap_or_else(GovernmentMix::great_power_default);
    let nations = generate_nations(&mut rng, n, &mix, &rules);
    println!(
        "{:<14} {:<24} {:<14} {:<40} {:<12}",
        "NAME", "GOVT", "ADJECTIVE", "TITLE", "FLAG PATTERN"
    );
    for f in &nations {
        println!(
            "{:<14} {:<24} {:<14} {:<40} {:?}",
            f.name,
            f.government.short_label(),
            f.adjective,
            f.government_title,
            f.flag.pattern
        );
    }
    // Exercise the re-exports so the compiler keeps them public.
    let _ = names::generate_city_name(&mut Rng::from_seed(seed));
    let _ = government_title("Demo", GovernmentForm::Republic);
}
