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
        /// Compile to native code via Cranelift instead of using the
        /// tree-walking interpreter. Currently supports a numeric subset
        /// (i64 / bool, control flow, function definitions); falls back
        /// with an error for strings, arrays, classes, etc.
        #[arg(long)]
        jit: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => run_repl(),
        Some(Cmd::Run { path, jit }) => run_file(&path, jit),
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
                match eval_in(&mut interp, &mut tc, trimmed, "<repl>") {
                    Ok(Value::Unit) => {}
                    Ok(v) => println!("{v}"),
                    Err(e) => eprintln!("{e}"),
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

fn run_file(path: &PathBuf, jit: bool) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let display_path = path.display().to_string();
    if jit {
        return jit_file(&display_path, src.trim());
    }
    let mut interp = Interpreter::new();
    let mut tc = TypeChecker::new();
    match eval_in(&mut interp, &mut tc, src.trim(), &display_path) {
        Ok(Value::Unit) => ExitCode::SUCCESS,
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Type-check then JIT-compile and run the program. Errors from any
/// stage are prefixed with the source label (filename) so location
/// information matches the interpreter path.
fn jit_file(source_label: &str, src: &str) -> ExitCode {
    let toks = match tokenize(src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{source_label} {e}");
            return ExitCode::FAILURE;
        }
    };
    let prog = match parse(&toks) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{source_label} {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut tc = TypeChecker::new();
    if let Err(e) = tc.check(&prog) {
        eprintln!("{source_label} {e}");
        return ExitCode::FAILURE;
    }
    match ilang_codegen::jit_run(&prog) {
        Ok(v) => {
            // Don't print Unit (matches the interpreter's behaviour).
            let s = format!("{v}");
            if !s.is_empty() {
                println!("{s}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{source_label}: jit error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Run one chunk of source. `source_label` (filename or `<repl>`) is
/// prepended to each error's leading `[row:col]` so users see exactly where
/// the message came from. Errors already start with `[row:col]: ...`.
fn eval_in(
    interp: &mut Interpreter,
    tc: &mut TypeChecker,
    src: &str,
    source_label: &str,
) -> Result<Value, String> {
    let toks = tokenize(src).map_err(|e| format!("{source_label} {e}"))?;
    let prog = parse(&toks).map_err(|e| format!("{source_label} {e}"))?;
    tc.check(&prog).map_err(|e| format!("{source_label} {e}"))?;
    interp.run(&prog).map_err(|e| format!("{source_label} {e}"))
}
