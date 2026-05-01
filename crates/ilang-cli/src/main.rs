use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ilang_eval::{eval, Value};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[derive(Parser, Debug)]
#[command(name = "ilang", version, about = "ilang interpreter (phase 1: arithmetic)")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Evaluate an .il source file as a single expression.
    Run {
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => run_repl(),
        Some(Cmd::Run { path }) => run_file(&path),
    }
}

fn run_repl() -> ExitCode {
    println!("ilang 0.1.0 (phase 1) — Ctrl-D to exit");
    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("failed to start REPL: {e}");
            return ExitCode::FAILURE;
        }
    };
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                match eval_str(trimmed) {
                    Ok(v) => println!("{v}"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn run_file(path: &PathBuf) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match eval_str(src.trim()) {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn eval_str(src: &str) -> Result<Value, String> {
    let toks = tokenize(src).map_err(|e| e.to_string())?;
    let ast = parse(&toks).map_err(|e| e.to_string())?;
    eval(&ast).map_err(|e| e.to_string())
}
