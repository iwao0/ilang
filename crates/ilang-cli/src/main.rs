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
        Some(Cmd::Run { path, jit, mir_jit }) => run_file(&path, jit, mir_jit),
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
    let candidate = dir.join("libilang_runtime.a");
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
    // Inline small leaf fns first so the ARC peephole sees the
    // post-inline shape (a call that turns into BinOp + Const lets
    // the peephole cancel matched retain/release pairs the call
    // hid behind a function boundary). `ILANG_NO_INLINE=1` disables
    // the pass for A/B benchmarking and bug isolation.
    if std::env::var_os("ILANG_NO_INLINE").is_none() {
        ilang_mir::passes::inline::run_program(&mut mir);
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
    // an explicit `-lX`. Walk the MIR for unique lib names, picking
    // the first entry of each fn's `libs` list as the canonical link
    // target (the rest are JIT-side fallbacks for `os.libLoaded`).
    let mut seen_libs: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for f in mir.functions.iter() {
        if let Some(primary) = f.libs.first() {
            seen_libs.insert(primary.as_str().to_string());
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
        ilang_mir::passes::inline::run_program(&mut mir);
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

/// Walk every named fn / method body in the program and collect
/// the top-level let names actually referenced as free variables —
/// i.e. uses where no local binding (param, let in scope) of the
/// same name shadows. This is what tells the lowerer which lets
/// have to be promoted to a host slot so cross-fn reads/writes
/// see a single shared cell.
///
/// The walker tracks a stack of locally-bound names: parameters at
/// fn entry; `let` / `let-tuple` / `let-struct` bindings within a
/// block; FnExpr params when descending into closure bodies. A
/// `Var(name)` / `Assign { target: name }` only counts as
/// referencing the top-level let when the name is in `top_lets`
/// AND not in the local stack.
fn collect_fn_free_var_refs(
    prog: &AstProgram,
    top_lets: &std::collections::HashSet<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                let mut locals: Vec<Symbol> =
                    f.params.iter().map(|p| p.name).collect();
                walk_block(&f.body, top_lets, &mut locals, out);
            }
            Item::Class(c) => {
                for m in c.methods.iter() {
                    let mut locals: Vec<Symbol> = std::iter::once(Symbol::intern("this"))
                        .chain(m.params.iter().map(|p| p.name))
                        .collect();
                    walk_block(&m.body, top_lets, &mut locals, out);
                }
                for sm in c.static_methods.iter() {
                    let mut locals: Vec<Symbol> =
                        sm.params.iter().map(|p| p.name).collect();
                    walk_block(&sm.body, top_lets, &mut locals, out);
                }
            }
            _ => {}
        }
    }
    // Also descend into FnExpr bodies that appear inside top-level
    // stmts — `let f = fn(...) { ... f(...) ... }` is a
    // self-recursive closure where `f` shows up as a free variable
    // inside the FnExpr body. Walking every top-level expression
    // (not just FnExpr bodies) would over-mark refs from regular
    // __main code as "needs a slot", breaking shadowing /
    // ARC semantics for entry-file lets. Only the bodies of
    // FnExprs need promotion.
    for stmt in &prog.stmts {
        match &stmt.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => {
                walk_fnexpr_bodies(value, top_lets, out);
            }
            StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
        }
    }
    if let Some(t) = &prog.tail {
        walk_fnexpr_bodies(t, top_lets, out);
    }
}

/// Recurse through `e`, collecting FnExpr bodies' free-var refs
/// against `top_lets` but NOT counting refs from non-FnExpr
/// surroundings. Distinct from `walk_expr` (which assumes we're
/// already inside a fn body, so every Var ref counts).
fn walk_fnexpr_bodies(
    e: &Expr,
    top_lets: &std::collections::HashSet<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind as E;
    match &e.kind {
        E::FnExpr { params, body, .. } => {
            let mut locals: Vec<Symbol> = params.iter().map(|p| p.name).collect();
            walk_block(body, top_lets, &mut locals, out);
        }
        E::Unary { expr, .. }
        | E::Cast { expr, .. }
        | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. }
        | E::Some(expr)
        | E::Field { obj: expr, .. } => walk_fnexpr_bodies(expr, top_lets, out),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            walk_fnexpr_bodies(lhs, top_lets, out);
            walk_fnexpr_bodies(rhs, top_lets, out);
        }
        E::Call { args, .. } | E::SuperCall { args, .. } | E::New { args, .. } => {
            for a in args.iter() { walk_fnexpr_bodies(a, top_lets, out); }
        }
        E::MethodCall { obj, args, .. } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            for a in args.iter() { walk_fnexpr_bodies(a, top_lets, out); }
        }
        E::Block(b) => {
            for s in &b.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => {
                        walk_fnexpr_bodies(value, top_lets, out);
                    }
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &b.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::If { cond, then_branch, else_branch } => {
            walk_fnexpr_bodies(cond, top_lets, out);
            for s in &then_branch.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &then_branch.tail { walk_fnexpr_bodies(t, top_lets, out); }
            if let Some(e) = else_branch { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::While { cond, body } => {
            walk_fnexpr_bodies(cond, top_lets, out);
            for s in &body.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &body.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::Loop { body } | E::ForIn { body, .. } => {
            for s in &body.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &body.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::IfLet { expr, then_branch, else_branch, .. } => {
            walk_fnexpr_bodies(expr, top_lets, out);
            for s in &then_branch.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &then_branch.tail { walk_fnexpr_bodies(t, top_lets, out); }
            if let Some(e) = else_branch { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Match { scrutinee, arms } => {
            walk_fnexpr_bodies(scrutinee, top_lets, out);
            for arm in arms.iter() { walk_fnexpr_bodies(&arm.body, top_lets, out); }
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start { walk_fnexpr_bodies(s, top_lets, out); }
            if let Some(e) = end { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() { walk_fnexpr_bodies(i, top_lets, out); }
        }
        E::Index { obj, index } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(index, top_lets, out);
        }
        E::Assign { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
        E::AssignField { obj, value, .. } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(value, top_lets, out);
        }
        E::AssignIndex { obj, index, value } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(index, top_lets, out);
            walk_fnexpr_bodies(value, top_lets, out);
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() { walk_fnexpr_bodies(v, top_lets, out); }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                walk_fnexpr_bodies(k, top_lets, out);
                walk_fnexpr_bodies(v, top_lets, out);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter() { walk_fnexpr_bodies(e, top_lets, out); }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter() { walk_fnexpr_bodies(e, top_lets, out); }
            }
        },
        E::Var(_) | E::Closure { .. } | E::This | E::None | E::Continue
        | E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) => {}
    }
}

fn walk_block(
    blk: &ilang_ast::Block,
    top_lets: &std::collections::HashSet<Symbol>,
    locals: &mut Vec<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    let saved = locals.len();
    for s in &blk.stmts {
        match &s.kind {
            StmtKind::Let { name, value, .. } => {
                walk_expr(value, top_lets, locals, out);
                locals.push(*name);
            }
            StmtKind::LetTuple { elems, value } => {
                walk_expr(value, top_lets, locals, out);
                for e in elems.iter().flatten() {
                    locals.push(*e);
                }
            }
            StmtKind::LetStruct { fields, value, .. } => {
                walk_expr(value, top_lets, locals, out);
                for f in fields.iter() {
                    locals.push(*f);
                }
            }
            StmtKind::Expr(e) => walk_expr(e, top_lets, locals, out),
        }
    }
    if let Some(t) = &blk.tail {
        walk_expr(t, top_lets, locals, out);
    }
    locals.truncate(saved);
}

fn walk_expr(
    e: &Expr,
    top_lets: &std::collections::HashSet<Symbol>,
    locals: &mut Vec<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind as E;
    match &e.kind {
        E::Var(name) => {
            if top_lets.contains(name) && !locals.contains(name) {
                out.insert(*name);
            }
        }
        E::Call { callee, args } => {
            if top_lets.contains(callee) && !locals.contains(callee) {
                out.insert(*callee);
            }
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::Assign { target, value } => {
            if top_lets.contains(target) && !locals.contains(target) {
                out.insert(*target);
            }
            walk_expr(value, top_lets, locals, out);
        }
        E::Unary { expr, .. }
        | E::Cast { expr, .. }
        | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. }
        | E::Some(expr)
        | E::Field { obj: expr, .. } => walk_expr(expr, top_lets, locals, out),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            walk_expr(lhs, top_lets, locals, out);
            walk_expr(rhs, top_lets, locals, out);
        }
        E::MethodCall { obj, args, .. } => {
            walk_expr(obj, top_lets, locals, out);
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::SuperCall { args, .. } | E::New { args, .. } => {
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::Block(b) => walk_block(b, top_lets, locals, out),
        E::If { cond, then_branch, else_branch } => {
            walk_expr(cond, top_lets, locals, out);
            walk_block(then_branch, top_lets, locals, out);
            if let Some(e) = else_branch {
                walk_expr(e, top_lets, locals, out);
            }
        }
        E::While { cond, body } => {
            walk_expr(cond, top_lets, locals, out);
            walk_block(body, top_lets, locals, out);
        }
        E::Loop { body } => walk_block(body, top_lets, locals, out),
        E::ForIn { var, iter, body, .. } => {
            walk_expr(iter, top_lets, locals, out);
            let saved = locals.len();
            locals.push(*var);
            walk_block(body, top_lets, locals, out);
            locals.truncate(saved);
        }
        E::IfLet { name, expr, then_branch, else_branch, .. } => {
            walk_expr(expr, top_lets, locals, out);
            let saved = locals.len();
            locals.push(*name);
            walk_block(then_branch, top_lets, locals, out);
            locals.truncate(saved);
            if let Some(e) = else_branch {
                walk_expr(e, top_lets, locals, out);
            }
        }
        E::Match { scrutinee, arms } => {
            walk_expr(scrutinee, top_lets, locals, out);
            for arm in arms.iter() {
                let saved = locals.len();
                if let ilang_ast::PatternKind::Variant { bindings, .. } = &arm.pattern.kind {
                    match bindings {
                        ilang_ast::PatternBindings::Unit => {}
                        ilang_ast::PatternBindings::Tuple(names) => {
                            for n in names.iter() {
                                locals.push(*n);
                            }
                        }
                        ilang_ast::PatternBindings::Struct(pairs) => {
                            for (_, bind) in pairs.iter() {
                                locals.push(*bind);
                            }
                        }
                    }
                }
                walk_expr(&arm.body, top_lets, locals, out);
                locals.truncate(saved);
            }
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start { walk_expr(s, top_lets, locals, out); }
            if let Some(e) = end { walk_expr(e, top_lets, locals, out); }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v { walk_expr(e, top_lets, locals, out); }
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() { walk_expr(i, top_lets, locals, out); }
        }
        E::Index { obj, index } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(index, top_lets, locals, out);
        }
        E::AssignField { obj, value, .. } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(value, top_lets, locals, out);
        }
        E::AssignIndex { obj, index, value } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(index, top_lets, locals, out);
            walk_expr(value, top_lets, locals, out);
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() { walk_expr(v, top_lets, locals, out); }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                walk_expr(k, top_lets, locals, out);
                walk_expr(v, top_lets, locals, out);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter() { walk_expr(e, top_lets, locals, out); }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter() { walk_expr(e, top_lets, locals, out); }
            }
        },
        E::FnExpr { params, body, .. } => {
            let saved = locals.len();
            for p in params.iter() {
                locals.push(p.name);
            }
            walk_block(body, top_lets, locals, out);
            locals.truncate(saved);
        }
        E::Closure { .. } | E::This | E::None | E::Continue
        | E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) => {}
    }
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
        let inline_stats = if std::env::var_os("ILANG_NO_INLINE").is_some() {
            ilang_mir::passes::inline::Stats::default()
        } else {
            ilang_mir::passes::inline::run_program(&mut mir)
        };
        let arc_stats = ilang_mir::passes::arc_peephole::run_program(&mut mir);
        if dump_stats {
            eprintln!(
                "{display_path}: inline calls_inlined={}",
                inline_stats.calls_inlined,
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
