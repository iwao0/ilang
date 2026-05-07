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
    // Resolve any `ilang.toml` next to (or above) the entry file
    // and turn its `[deps]` table into extra `use`-resolution paths.
    let extra_paths = match collect_dep_paths(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    // Use the loader so `use module` items get followed and merged
    // into one program before type-checking.
    let _t0 = std::time::Instant::now();
    let prog = match ilang_parser::loader::load_program_with_paths(path, &extra_paths) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    if std::env::var("ILANG_TIMING").is_ok() {
        eprintln!("[timing] parse+load: {:?}", _t0.elapsed());
    }
    let display_path = path.display().to_string();
    if jit {
        let _t1 = std::time::Instant::now();
        let mut tc = TypeChecker::new();
        if let Err(e) = tc.check(&prog) {
            eprintln!("{display_path} {e}");
            return ExitCode::FAILURE;
        }
        if std::env::var("ILANG_TIMING").is_ok() {
            eprintln!("[timing] typecheck: {:?}", _t1.elapsed());
        }
        let _t2 = std::time::Instant::now();
        // Mangle overloaded fn names (no-op when no name is overloaded).
        let prog = ilang_types::mangle::mangle_overloads(prog, &tc.fn_overload_picks(), &tc.method_overload_picks(), &tc.call_default_fills());
        if std::env::var("ILANG_TIMING").is_ok() {
            eprintln!("[timing] mangle: {:?}", _t2.elapsed());
        }
        if std::env::var("ILANG_TIMING_QUIT_BEFORE_JIT").is_ok() {
            return ExitCode::SUCCESS;
        }
        return match ilang_codegen::jit_run_with(
            &prog,
            &tc.fn_call_type_args(),
            &tc.enum_ctor_type_args(),
            &tc.loop_break_types(),
            &tc.class_method_slots(),
            &tc.class_vtable_lens(),
            &tc.fn_expr_captures(),
            &tc.fn_expr_this_class(),
        ) {
            Ok(v) => {
                let s = format!("{v}");
                if !s.is_empty() {
                    println!("{s}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{display_path}: jit error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let mut tc = TypeChecker::new();
    if let Err(e) = tc.check(&prog) {
        eprintln!("{display_path} {e}");
        return ExitCode::FAILURE;
    }
    let enum_ctor_args = tc.enum_ctor_type_args();
    let prog = ilang_types::mangle::mangle_overloads(prog, &tc.fn_overload_picks(), &tc.method_overload_picks(), &tc.call_default_fills());
    let mut interp = Interpreter::new();
    interp.set_enum_ctor_type_args(enum_ctor_args);
    match interp.run(&prog) {
        Ok(Value::Unit) => ExitCode::SUCCESS,
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{display_path} {e}");
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
    interp.set_enum_ctor_type_args(tc.enum_ctor_type_args());
    interp.run(&prog).map_err(|e| format!("{source_label} {e}"))
}

/// `ilang.toml` schema:
///
/// ```toml
/// [package]
/// name = "my_game"
///
/// [deps]
/// sdl2 = "../../bindings/sdl2"   # path → search directory
/// ```
///
/// Each `[deps]` entry maps a name to a directory; every `.il` file
/// under that directory becomes resolvable via `use <name>` from
/// the project (the dep name itself is informational, not the
/// module name — `use sdl` finds `sdl.il` under any registered
/// directory). Paths are interpreted relative to the project file.
#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: std::collections::BTreeMap<String, DepSpec>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DepSpec {
    Path(String),
    Detailed { path: String },
}

impl DepSpec {
    fn path(&self) -> &str {
        match self {
            DepSpec::Path(p) => p,
            DepSpec::Detailed { path } => path,
        }
    }
}

fn collect_dep_paths(entry: &PathBuf) -> Result<Vec<PathBuf>, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    // Walk upward from the entry's directory looking for the
    // closest `ilang.toml`. Stops at the first hit; absent file is
    // not an error (project file is optional).
    let project_file = find_project_file(&entry_dir);
    let Some(project_file) = project_file else {
        return Ok(Vec::new());
    };
    let project_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let src = std::fs::read_to_string(&project_file)
        .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
    let parsed: ProjectFile = toml::from_str(&src)
        .map_err(|e| format!("invalid {}: {e}", project_file.display()))?;
    let mut out = Vec::new();
    for (_name, dep) in parsed.deps {
        let p = project_dir.join(dep.path());
        let canon = p.canonicalize().map_err(|e| {
            format!(
                "{}: dep path {:?} doesn't exist: {e}",
                project_file.display(),
                dep.path()
            )
        })?;
        out.push(canon);
    }
    Ok(out)
}

fn find_project_file(start: &std::path::Path) -> Option<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(dir) = cur {
        let candidate = dir.join("ilang.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    None
}
