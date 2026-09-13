mod generate;
mod grammar;

use std::env;
use std::fs;
use std::process::ExitCode;

struct Args {
    path: String,
    start: String,
    count: usize,
    seed: Option<u64>,
    format: Format,
}

#[derive(Clone, Copy)]
enum Format {
    None,
    Upper,
    Lower,
    Capitalize,
}

impl Format {
    fn parse(raw: &str) -> Result<Format, String> {
        match raw {
            "none" => Ok(Format::None),
            "upper" => Ok(Format::Upper),
            "lower" => Ok(Format::Lower),
            "capitalize" => Ok(Format::Capitalize),
            other => Err(format!(
                "--format expects one of: none, upper, lower, capitalize (got '{other}')"
            )),
        }
    }

    fn apply(self, name: &str) -> String {
        match self {
            Format::None => name.to_string(),
            Format::Upper => name.to_uppercase(),
            Format::Lower => name.to_lowercase(),
            Format::Capitalize => {
                let mut chars = name.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            }
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut argv = env::args().skip(1);
    let mut path = None;
    let mut start = "name".to_string();
    let mut count = 10usize;
    let mut seed = None;
    let mut format = Format::None;

    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--start" => {
                start = argv.next().ok_or("--start requires a rule name")?;
            }
            "--count" => {
                let raw = argv.next().ok_or("--count requires a number")?;
                count = raw
                    .parse()
                    .map_err(|_| format!("--count expects a positive integer, got '{raw}'"))?;
            }
            "--seed" => {
                let raw = argv.next().ok_or("--seed requires a number")?;
                seed = Some(
                    raw.parse()
                        .map_err(|_| format!("--seed expects an unsigned integer, got '{raw}'"))?,
                );
            }
            "--format" => {
                let raw = argv.next().ok_or("--format requires a value")?;
                format = Format::parse(&raw)?;
            }
            other if path.is_none() && !other.starts_with('-') => {
                path = Some(other.to_string());
            }
            other => {
                return Err(format!("unrecognized argument '{other}'"));
            }
        }
    }

    let path = path.ok_or("missing grammar file argument")?;
    Ok(Args {
        path,
        start,
        count,
        seed,
        format,
    })
}

fn print_usage() {
    eprintln!("namegen - generate random names from a grammar file\n");
    eprintln!(
        "usage: namegen <grammar-file> [--start <rule>] [--count <n>] [--seed <n>] [--format <mode>]\n"
    );
    eprintln!("  <grammar-file>   path to a .namegen grammar file");
    eprintln!("  --start <rule>   rule to expand first (default: name)");
    eprintln!("  --count <n>      how many names to generate (default: 10)");
    eprintln!("  --seed <n>       fix the random seed for reproducible output");
    eprintln!("  --format <mode>  casing to apply: none, upper, lower, capitalize (default: none)");
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    let source = match fs::read_to_string(&args.path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{}': {e}", args.path);
            return ExitCode::FAILURE;
        }
    };

    let grammar = match grammar::parse(&source) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    if !grammar.rules.contains_key(&args.start) {
        let mut names: Vec<&str> = grammar.rules.keys().map(|s| s.as_str()).collect();
        names.sort();
        eprintln!(
            "error: unknown start rule '{}' - grammar defines: {}",
            args.start,
            names.join(", ")
        );
        return ExitCode::FAILURE;
    }

    let mut rng = match args.seed {
        Some(seed) => generate::Rng::new_seeded(seed),
        None => generate::Rng::from_entropy(),
    };

    for _ in 0..args.count {
        match generate::generate(&grammar, &args.start, &mut rng) {
            Ok(name) => println!("{}", args.format.apply(&name)),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
