use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ilang_ast::{Expr, ExprKind, Item, Program as AstProgram, StmtKind, Symbol};
use std::collections::HashMap;
// `ilang-eval` removed in M1 Step 6 part 5 — the interpreter is no
// longer reachable from the CLI (mir-jit is the sole execution
// backend besides the legacy `--jit` codegen).
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
        /// Compile via the legacy `ilang-codegen` pipeline (the
        /// pre-MIR Cranelift JIT). Retained for parity testing
        /// against the new mir-jit; deprecated for new use.
        #[arg(long)]
        jit: bool,
        /// Compile via the new MIR → Cranelift pipeline. This is
        /// now the default; the flag stays for explicit selection
        /// and back-compat with existing test commands.
        #[arg(long = "mir-jit")]
        mir_jit: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => run_repl(),
        Some(Cmd::Run { path, jit, mir_jit }) => run_file(&path, jit, mir_jit),
    }
}

/// Persistent state for the JIT REPL. Each turn appends new
/// definitions (fn / class / enum) to `accumulated_items`; top-level
/// `let` bindings whose declared type can be lowered to a `MirTy`
/// are promoted to a stable host-side slot index so future chunks
/// can read / mutate them via `__repl_load_slot` / `__repl_store_slot`.
///
/// Each turn compiles a *fresh* program: all accumulated items + a
/// chunk-only `__main` body containing only the new chunk's stmts /
/// tail. Side effects fire exactly once per chunk because the prior
/// chunks' bodies are not re-run.
///
/// Limitation: top-level lets whose type the AST→MIR conversion
/// can't handle (currently bare `Object` / `Generic` instantiations
/// — class types lack a stable id outside the lower context) fall
/// through as ordinary chunk-local lets and don't persist. The
/// TypeChecker still tracks them, so subsequent chunks that
/// reference them produce a clean undefined-variable runtime error.
struct ReplSession {
    /// TypeChecker carried across chunks — it accumulates fn / class
    /// / enum signatures and top-level `let` types in `self.vars`.
    tc: TypeChecker,
    /// Accumulated definitions (Item::Fn / Class / Enum / ExternC /
    /// Const / Use) from every chunk so far. Replayed verbatim into
    /// the per-chunk MIR program so chunk bodies can call them.
    accumulated_items: Vec<Item>,
    /// Top-level `let` name → (slot index, AST Type). The AST type
    /// is resolved to MirTy inside `lower_program_with_slots` once
    /// per-chunk class / enum tables are populated. Drives slot
    /// emission downstream.
    slot_table: HashMap<Symbol, (u32, ilang_ast::Type)>,
    /// Next slot index handed out for a newly-promoted top-level let.
    next_slot: u32,
}

impl ReplSession {
    fn new() -> Self {
        Self {
            tc: TypeChecker::new(),
            accumulated_items: Vec::new(),
            slot_table: HashMap::new(),
            next_slot: 0,
        }
    }

    fn run_chunk(&mut self, src: &str) -> Result<String, String> {
        let toks = tokenize(src).map_err(|e| format!("<repl> {e}"))?;
        let chunk_prog = parse(&toks).map_err(|e| format!("<repl> {e}"))?;

        // Type-check the chunk in isolation — the persistent
        // TypeChecker already remembers fn / class / enum / let
        // signatures from prior chunks via `self.vars` / `self.fns`
        // / `self.classes`.
        self.tc
            .check(&chunk_prog)
            .map_err(|e| format!("<repl> {e}"))?;

        // Promote any new top-level `let` to a slot. The AST type
        // gets resolved to MirTy inside the lowerer once it has
        // class / enum ids; we just store the AST type here.
        for stmt in &chunk_prog.stmts {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                if self.slot_table.contains_key(name) {
                    continue;
                }
                let Some(ty) = self.tc.lookup_global(*name) else {
                    continue;
                };
                let idx = self.next_slot;
                self.next_slot += 1;
                self.slot_table.insert(*name, (idx, ty));
            }
        }

        // Build the per-chunk Program: accumulated definitions stay
        // available for calls / class instantiations; only the new
        // chunk's stmts / tail run inside the synthesised __main.
        let mut per_chunk = AstProgram::default();
        per_chunk.items = self.accumulated_items.clone();
        per_chunk.items.extend(chunk_prog.items.iter().cloned());
        per_chunk.stmts = chunk_prog.stmts.clone();
        per_chunk.tail = chunk_prog.tail.clone();

        // The downstream MIR pipeline reads picks / type-arg dicts
        // from the persistent TypeChecker — it has accumulated
        // entries for every chunk seen so far, including this one.
        let prog = ilang_types::mangle::mangle_overloads(
            per_chunk,
            &self.tc.fn_overload_picks(),
            &self.tc.method_overload_picks(),
            &self.tc.call_default_fills(),
        );
        let prog = ilang_mir::monomorphize::monomorphize(&prog);
        let prog = ilang_mir::monomorphize::monomorphize_enums(
            &prog,
            &self.tc.enum_ctor_type_args(),
        );
        let prog =
            ilang_mir::monomorphize::monomorphize_fns(&prog, &self.tc.fn_call_type_args());
        let mir = ilang_mir::lower_program_with_slots(&prog, &self.slot_table)
            .map_err(|e| format!("<repl> mir: {e}"))?;
        let compiled = ilang_mir_codegen::compile_program(&mir)
            .map_err(|e| format!("<repl> mir-codegen: {e}"))?;
        let r = ilang_mir_codegen::run_main(&compiled);

        // Commit the chunk's definitions to the accumulated state
        // only after a successful run — partial failures don't
        // pollute future chunks.
        self.accumulated_items.extend(chunk_prog.items.into_iter());

        if r != 0 {
            Ok(r.to_string())
        } else {
            Ok(String::new())
        }
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
    ilang_mir_codegen::reset_repl_slots();
    let mut session = ReplSession::new();
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                match session.run_chunk(trimmed) {
                    Ok(out) if out.is_empty() => {}
                    Ok(out) => println!("{out}"),
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

/// If the program has a trailing expression, wrap it in
/// `console.log(<tail>)` so the JIT path's `__main` prints the
/// value the user expected to see. Mirrors the tree-walking
/// interpreter's behaviour of returning + auto-printing the tail.
/// Programs without a tail (everything in fixture form) are
/// untouched.
fn wrap_trailing_print(mut prog: AstProgram) -> AstProgram {
    if let Some(tail) = prog.tail.take() {
        let span = tail.span;
        let console = Expr::new(ExprKind::Var(Symbol::intern("console")), span);
        let log_call = Expr::new(
            ExprKind::MethodCall {
                obj: Box::new(console),
                method: Symbol::intern("log"),
                args: Box::new([tail]),
            },
            span,
        );
        prog.tail = Some(log_call);
    }
    prog
}

fn run_file(path: &PathBuf, jit: bool, mir_jit: bool) -> ExitCode {
    // Backend selection: explicit `--jit` selects the legacy
    // ilang-codegen pipeline; everything else (no flag, or
    // `--mir-jit`) routes through the new mir-jit backend.
    let use_mir_jit = !jit;
    let _ = mir_jit;
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
    if use_mir_jit {
        // Auto-print the trailing expression — matches what the
        // tree-walking interpreter does with the value of
        // `Interpreter::run`. Wrapping at the AST level routes
        // through the existing `console.log` builtin (variadic +
        // type-aware), so heap values pretty-print without extra
        // codegen plumbing.
        let prog = wrap_trailing_print(prog);
        let mut tc = TypeChecker::new();
        if let Err(e) = tc.check(&prog) {
            eprintln!("{display_path} {e}");
            return ExitCode::FAILURE;
        }
        let prog = ilang_types::mangle::mangle_overloads(
            prog,
            &tc.fn_overload_picks(),
            &tc.method_overload_picks(),
            &tc.call_default_fills(),
        );
        // Monomorphize generics (classes / enums / fns) before
        // AST→MIR lowering. We deliberately skip `hoist_anon_fns`
        // — the MIR lowerer handles anon fn / closure capture
        // analysis itself, and the legacy hoister rewrites them in
        // a way that conflicts with our cell-based capture model.
        let prog = ilang_mir::monomorphize::monomorphize(&prog);
        let prog = ilang_mir::monomorphize::monomorphize_enums(
            &prog,
            &tc.enum_ctor_type_args(),
        );
        let prog = ilang_mir::monomorphize::monomorphize_fns(
            &prog,
            &tc.fn_call_type_args(),
        );
        let mir = match ilang_mir::lower_program(&prog) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{display_path}: mir lower: {e}");
                return ExitCode::FAILURE;
            }
        };
        let compiled = match ilang_mir_codegen::compile_program(&mir) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{display_path}: mir-codegen: {e}");
                return ExitCode::FAILURE;
            }
        };
        let r = ilang_mir_codegen::run_main(&compiled);
        // The MIR pipeline returns __main's i64; print it only if
        // it's non-zero so stdout-capture-based tests stay clean.
        if r != 0 {
            println!("{r}");
        }
        return ExitCode::SUCCESS;
    }
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
    // Unreachable: every path above returns. The legacy interpreter
    // fallback was removed in M1 Step 6 part 5.
    unreachable!("backend selection above always returns")
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
