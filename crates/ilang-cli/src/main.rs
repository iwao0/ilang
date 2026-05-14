mod project;
mod walk;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ilang_ast::{Item, Program as AstProgram, StmtKind, Symbol};
use std::collections::HashMap;

use project::collect_dep_paths;
use walk::{collect_fn_free_var_refs, wrap_trailing_print};
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
    /// Evaluate an .il source file via the MIR → Cranelift JIT.
    Run {
        path: PathBuf,
        /// Compatibility flag — selecting the mir-jit pipeline is the
        /// default and only behaviour now.
        #[arg(long = "mir-jit")]
        mir_jit: bool,
    },
    /// Compile an .il source file to a native executable. M0 scope
    /// only handles programs whose tail expression is an integer
    /// literal (the i64 return becomes the process exit code).
    Build {
        path: PathBuf,
        /// Output path for the executable.
        #[arg(short = 'o', long = "output")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => run_repl(),
        Some(Cmd::Run { path, mir_jit }) => run_file(&path, mir_jit),
        Some(Cmd::Build { path, output }) => build_file(&path, &output),
    }
}

/// Find `libilang_runtime.a` next to the running `ilang` executable.
/// Cargo lays both into the same `target/<profile>/` directory, so we
/// can resolve via `current_exe()`. Returns `None` if the file isn't
/// there (e.g. the user copied just the `ilang` binary somewhere) so
/// the linker step still runs — programs that don't pull in any
/// runtime symbol will link fine without it.
fn locate_runtime_lib() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let lib_name = if cfg!(windows) { "ilang_runtime.lib" } else { "libilang_runtime.a" };
    let candidate = dir.join(lib_name);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn build_file(path: &PathBuf, output: &PathBuf) -> ExitCode {
    let extra_paths = match collect_dep_paths(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let prog = match ilang_parser::loader::load_program_with_paths(path, &extra_paths) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let display_path = path.display().to_string();
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
    // Mirror run_file's top-level-let-to-slot promotion: any module-
    // level mutable `let` referenced from a free fn / method body
    // becomes a host-slot binding so `lower_program_with_slots` can
    // emit `__repl_load_slot` / `__repl_store_slot` instead of an
    // unbound-variable error. The runtime ships these in both the JIT
    // and AOT paths now.
    let mut slot_table: HashMap<Symbol, (u32, ilang_ast::Type)> = HashMap::new();
    {
        let mut top_let_names: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        for stmt in &prog.stmts {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                top_let_names.insert(*name);
            }
        }
        let mut referenced: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_fn_free_var_refs(&prog, &top_let_names, &mut referenced);
        let mut next_slot: u32 = 0;
        for stmt in &prog.stmts {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                if !referenced.contains(name) {
                    continue;
                }
                if let Some(ty) = tc.lookup_global(*name) {
                    slot_table.insert(*name, (next_slot, ty));
                    next_slot += 1;
                }
            }
        }
        ilang_mir_codegen::reset_repl_slots();
    }
    let prog = ilang_mir::monomorphize::monomorphize(&prog);
    let prog = ilang_mir::monomorphize::monomorphize_enums(&prog, &tc.enum_ctor_type_args());
    let prog = ilang_mir::monomorphize::monomorphize_fns(&prog, &tc.fn_call_type_args());
    let mut mir = match ilang_mir::lower_program_with_slots(&prog, &slot_table) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{display_path}: mir lower: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Promote single-def primitive locals into SSA so `let a = 2*3`
    // surfaces as a regular ValueId chain. Inline + const_fold both
    // gate on "no Local references" and "operand is Const" so this
    // pass widens their reach. `ILANG_NO_PROMOTE_LOCALS=1` disables.
    if std::env::var_os("ILANG_NO_PROMOTE_LOCALS").is_none() {
        ilang_mir::passes::promote_locals::run_program(&mut mir);
    }
    // Inline small leaf fns first so the ARC peephole sees the
    // post-inline shape (a call that turns into BinOp + Const lets
    // the peephole cancel matched retain/release pairs the call
    // hid behind a function boundary). `ILANG_NO_INLINE=1` disables
    // the pass for A/B benchmarking and bug isolation.
    if std::env::var_os("ILANG_NO_INLINE").is_none() {
        ilang_mir::passes::inline::run_program(&mut mir);
    }
    // Fold compile-time constants after inlining — params bound to
    // literal args fold all the way through their inlined bodies.
    // `ILANG_NO_CONST_FOLD=1` disables for A/B.
    if std::env::var_os("ILANG_NO_CONST_FOLD").is_none() {
        ilang_mir::passes::const_fold::run_program(&mut mir);
    }
    // Collapse `CondBr` / `Switch` whose condition / scrutinee is a
    // known Const into an unconditional `Br`. Pairs with const_fold:
    // a folded `1 < 2` exposes the taken branch. `ILANG_NO_BRANCH_FOLD=1`
    // disables for A/B.
    if std::env::var_os("ILANG_NO_BRANCH_FOLD").is_none() {
        ilang_mir::passes::branch_fold::run_program(&mut mir);
    }
    // Sweep unreferenced pure instructions (now-dead Consts that
    // fed folded BinOps, abandoned mid-chain values, etc.).
    // `ILANG_NO_DCE=1` disables for A/B.
    if std::env::var_os("ILANG_NO_DCE").is_none() {
        ilang_mir::passes::dce::run_program(&mut mir);
    }
    ilang_mir::passes::arc_peephole::run_program(&mut mir);

    let object_bytes = match ilang_mir_codegen::compile_program_to_object(&mir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{display_path}: aot: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Drop the `.o` next to the eventual executable so users can
    // inspect or rerun the link manually if needed. Naming it
    // `<output>.o` keeps the intermediate artifact under their chosen
    // path rather than littering /tmp.
    let object_path = output.with_extension("o");
    if let Err(e) = std::fs::write(&object_path, &object_bytes) {
        eprintln!("{display_path}: write {}: {e}", object_path.display());
        return ExitCode::FAILURE;
    }

    // Link via the system C compiler. macOS ships `ld`/`cc` with the
    // Xcode Command Line Tools; we don't bundle a linker yet (LLD
    // shipped as a library is a follow-up).
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut cmd = std::process::Command::new(&cc);
    cmd.arg(&object_path).arg("-o").arg(output);
    // Strip unreachable code / data. The runtime archive ships every
    // helper the JIT and AOT paths between them might need; without
    // dead-strip the linker pulls in unused __retain_*, __release_*,
    // __print_* etc. for whichever shapes the user program never
    // touches. macOS `ld` accepts `-dead_strip`; we route it via cc.
    cmd.arg("-Wl,-dead_strip");
    // `console.log` and other AOT runtime symbols live in
    // `libilang_runtime.a`. Locate it next to the running `ilang`
    // binary (cargo lays both into the same `target/<profile>/` dir).
    if let Some(rt) = locate_runtime_lib() {
        cmd.arg(&rt);
    }
    // Every `@lib("X")`-annotated extern fn body resolves through the
    // system loader at runtime in the JIT, but the AOT linker needs
    // an explicit `-lX`. Pick the first lib name per fn that the AOT
    // codegen probed as loadable — primary or fallback. Missing libs
    // are skipped; if the fn was `@optional` it gets a local stub
    // (emitted by aot.rs) so the link still succeeds.
    let available_libs = ilang_mir_codegen::aot::probe_available_libs(&mir);
    let mut seen_libs: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for f in mir.functions.iter() {
        for sym in f.libs.iter() {
            let name = sym.as_str().to_string();
            if available_libs.contains(&name) {
                seen_libs.insert(name);
                break;
            }
        }
    }
    if !seen_libs.is_empty() {
        // Standard macOS install paths. Homebrew on Apple Silicon
        // lives under /opt/homebrew; Intel Macs keep /usr/local. Pass
        // both as search paths and as rpath entries so the linker
        // resolves now and the loader finds the dylib at runtime.
        for p in ["/opt/homebrew/lib", "/usr/local/lib"] {
            if std::path::Path::new(p).is_dir() {
                cmd.arg(format!("-L{p}"));
                cmd.arg(format!("-Wl,-rpath,{p}"));
            }
        }
    }
    for lib in &seen_libs {
        cmd.arg(format!("-l{lib}"));
    }
    let status = cmd.status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!(
                "{display_path}: linker exited with status {:?}",
                s.code()
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!(
                "{display_path}: failed to spawn linker `{}`: {e}",
                std::path::Path::new(&cc).display()
            );
            return ExitCode::FAILURE;
        }
    }
    // Strip the symbol table after linking. Reduces binary size with
    // no effect on behaviour. macOS ships `strip` with the Command
    // Line Tools alongside `cc`. Non-fatal on failure — the executable
    // still runs without it.
    let _ = std::process::Command::new("strip").arg(output).status();
    ExitCode::SUCCESS
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
        let mut mir = ilang_mir::lower_program_with_slots(&prog, &self.slot_table)
            .map_err(|e| format!("<repl> mir: {e}"))?;
        ilang_mir::passes::promote_locals::run_program(&mut mir);
        ilang_mir::passes::inline::run_program(&mut mir);
        ilang_mir::passes::const_fold::run_program(&mut mir);
        ilang_mir::passes::branch_fold::run_program(&mut mir);
        ilang_mir::passes::dce::run_program(&mut mir);
        ilang_mir::passes::arc_peephole::run_program(&mut mir);
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

fn run_file(path: &PathBuf, mir_jit: bool) -> ExitCode {
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
    {
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
        // Promote a top-level `let` to a host-side slot only when
        // some named function (a free fn or class method) actually
        // references the binding — without slot promotion, fn
        // bodies can't see entry-/module-level lets, but promoting
        // every top-level let interacts badly with closure-capture
        // semantics and ARC for binds only `__main` itself uses.
        // Walk every fn / method body, collect their free vars,
        // intersect with the top-level let names.
        let mut slot_table: HashMap<Symbol, (u32, ilang_ast::Type)> = HashMap::new();
        {
            let mut top_let_names: std::collections::HashSet<Symbol> =
                std::collections::HashSet::new();
            for stmt in &prog.stmts {
                if let StmtKind::Let { name, .. } = &stmt.kind {
                    top_let_names.insert(*name);
                }
            }
            let mut referenced: std::collections::HashSet<Symbol> =
                std::collections::HashSet::new();
            collect_fn_free_var_refs(&prog, &top_let_names, &mut referenced);
            let mut next_slot: u32 = 0;
            for stmt in &prog.stmts {
                if let StmtKind::Let { name, .. } = &stmt.kind {
                    if !referenced.contains(name) {
                        continue;
                    }
                    if let Some(ty) = tc.lookup_global(*name) {
                        slot_table.insert(*name, (next_slot, ty));
                        next_slot += 1;
                    }
                }
            }
            ilang_mir_codegen::reset_repl_slots();
        }
        let prog = ilang_mir::monomorphize::monomorphize(&prog);
        let prog = ilang_mir::monomorphize::monomorphize_enums(
            &prog,
            &tc.enum_ctor_type_args(),
        );
        let prog = ilang_mir::monomorphize::monomorphize_fns(
            &prog,
            &tc.fn_call_type_args(),
        );
        let mut mir = match ilang_mir::lower_program_with_slots(&prog, &slot_table) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{display_path}: mir lower: {e}");
                return ExitCode::FAILURE;
            }
        };
        if std::env::var_os("ILANG_MIR_DUMP").is_some() {
            eprintln!("--- MIR (post-lower, pre-pass) ---\n{}\n--- end MIR ---",
                ilang_mir::print_program(&mir));
        }
        let dump_stats = std::env::var_os("ILANG_MIR_PASS_STATS").is_some();
        let (retains_before, releases_before) = if dump_stats {
            count_retain_release(&mir)
        } else {
            (0, 0)
        };
        let promote_stats = if std::env::var_os("ILANG_NO_PROMOTE_LOCALS").is_some() {
            ilang_mir::passes::promote_locals::Stats::default()
        } else {
            ilang_mir::passes::promote_locals::run_program(&mut mir)
        };
        let inline_stats = if std::env::var_os("ILANG_NO_INLINE").is_some() {
            ilang_mir::passes::inline::Stats::default()
        } else {
            ilang_mir::passes::inline::run_program(&mut mir)
        };
        let const_fold_stats = if std::env::var_os("ILANG_NO_CONST_FOLD").is_some() {
            ilang_mir::passes::const_fold::Stats::default()
        } else {
            ilang_mir::passes::const_fold::run_program(&mut mir)
        };
        let branch_fold_stats = if std::env::var_os("ILANG_NO_BRANCH_FOLD").is_some() {
            ilang_mir::passes::branch_fold::Stats::default()
        } else {
            ilang_mir::passes::branch_fold::run_program(&mut mir)
        };
        let dce_stats = if std::env::var_os("ILANG_NO_DCE").is_some() {
            ilang_mir::passes::dce::Stats::default()
        } else {
            ilang_mir::passes::dce::run_program(&mut mir)
        };
        let arc_stats = ilang_mir::passes::arc_peephole::run_program(&mut mir);
        if dump_stats {
            eprintln!(
                "{display_path}: promote_locals locals={} uses={} inline calls_inlined={} const_fold folds_applied={} branch_fold branches={} dce removed={}",
                promote_stats.locals_promoted,
                promote_stats.uses_rewritten,
                inline_stats.calls_inlined,
                const_fold_stats.folds_applied,
                branch_fold_stats.branches_folded,
                dce_stats.insts_removed,
            );
        }
        if dump_stats {
            let (retains_after, releases_after) = count_retain_release(&mir);
            eprintln!(
                "{display_path}: arc_peephole retains={retains_before}->{retains_after} releases={releases_before}->{releases_after} pairs={}",
                arc_stats.pairs_removed
            );
        }
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
        ExitCode::SUCCESS
    }
}


fn count_retain_release(prog: &ilang_mir::Program) -> (usize, usize) {
    let mut retains = 0;
    let mut releases = 0;
    for f in &prog.functions {
        for block in &f.blocks {
            for inst in &block.insts {
                match inst {
                    ilang_mir::Inst::Retain { .. } => retains += 1,
                    ilang_mir::Inst::Release { .. } => releases += 1,
                    _ => {}
                }
            }
        }
    }
    (retains, releases)
}
