use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ilang_eval::{Interpreter, Value};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[derive(Parser, Debug)]
#[command(name = "ilang", version, about = "ilang interpreter")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Evaluate an .il source file.
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
    println!("ilang 0.2.0 — Ctrl-D to exit");
    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("failed to start REPL: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut interp = Interpreter::new();
    let mut tc = TypeChecker::new();
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                match eval_in(&mut interp, &mut tc, trimmed) {
                    Ok(Value::Unit) => {}
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
    let mut interp = Interpreter::new();
    let mut tc = TypeChecker::new();
    match eval_in(&mut interp, &mut tc, src.trim()) {
        Ok(Value::Unit) => ExitCode::SUCCESS,
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

fn eval_in(
    interp: &mut Interpreter,
    tc: &mut TypeChecker,
    src: &str,
) -> Result<Value, String> {
    let toks = tokenize(src).map_err(|e| e.to_string())?;
    let prog = parse(&toks).map_err(|e| e.to_string())?;
    tc.check(&prog).map_err(|e| format!("type error: {e}"))?;
    interp.run(&prog).map_err(|e| e.to_string())
}
