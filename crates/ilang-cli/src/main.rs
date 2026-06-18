mod manifest;
mod project;
mod walk;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ilang_ast::{Item, Program as AstProgram, StmtKind, Symbol};
use std::collections::HashMap;

use project::collect_dep_tree;
use walk::{collect_fn_free_var_refs, wrap_trailing_print};
// `ilang-eval` removed in M1 Step 6 part 5 — the tree-walking
// interpreter is no longer reachable from the CLI. `mir-jit` is
// the sole execution backend; AOT goes through `build`.
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
    // Opt-in crash reporter (`ILANG_TRACE_CRASH=1`): Windows vectored
    // exception handler / Unix sigaction that print signal/exception
    // details to stderr before the OS terminates. Safe to call cold.
    ilang_runtime::crash_handler::install_if_enabled();

    // Always install a panic hook so Rust panics from the lower /
    // codegen / runtime print location + a forced backtrace before
    // exit, even when `RUST_BACKTRACE` is unset. Cheap (just registers
    // a callback) and helps the parallel-spawn harnesses surface the
    // panic site instead of "command failed: <empty>".
    std::panic::set_hook(Box::new(|info| {
        eprintln!("ilang: panic: {info}");
        if let Some(loc) = info.location() {
            eprintln!("  at {}:{}:{}", loc.file(), loc.line(), loc.column());
        }
        eprintln!("backtrace:");
        eprintln!("{}", std::backtrace::Backtrace::force_capture());
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }));

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
/// If `lib` is a macOS framework path (e.g.
/// `/System/Library/Frameworks/AppKit.framework/AppKit`), pull
/// out the framework's name (`AppKit`). Returns `None` for plain
/// library names so they fall through to the regular `-l<lib>`
/// path.
#[cfg(target_os = "macos")]
fn extract_framework_name(lib: &str) -> Option<String> {
    let path = std::path::Path::new(lib);
    let mut comps = path.components().rev();
    let last = comps.next()?.as_os_str().to_string_lossy().to_string();
    let parent = comps.next()?.as_os_str().to_string_lossy().to_string();
    // Path looks like `.../Name.framework/Name` when the last
    // two components match modulo the `.framework` suffix.
    let stripped = parent.strip_suffix(".framework")?;
    if stripped == last {
        Some(last)
    } else {
        None
    }
}

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

/// Search common locations for the vcpkg x64-windows lib directory so
/// the MSVC linker can find SDL2.lib and other vcpkg-installed import libs.
/// Checks (in order): `VCPKG_ROOT` env var, `VCPKG_INSTALLED_DIR` env var,
/// and a handful of conventional install paths.
#[cfg(windows)]
fn find_vcpkg_lib_path() -> Option<PathBuf> {
    let triplet = "x64-windows";
    let sub = ["installed", triplet, "lib"];

    // VCPKG_ROOT is set by `vcpkg integrate install` / shell integration.
    if let Ok(root) = std::env::var("VCPKG_ROOT") {
        let p = sub.iter().fold(PathBuf::from(root), |acc, s| acc.join(s));
        if p.is_dir() { return Some(p); }
    }
    // VCPKG_INSTALLED_DIR may be set independently.
    if let Ok(dir) = std::env::var("VCPKG_INSTALLED_DIR") {
        let p = [triplet, "lib"].iter().fold(PathBuf::from(dir), |acc, s| acc.join(s));
        if p.is_dir() { return Some(p); }
    }
    // Conventional manual-install paths.
    for base in [r"C:\vcpkg", r"C:\tools\vcpkg", r"C:\dev\vcpkg"] {
        let p = sub.iter().fold(PathBuf::from(base), |acc, s| acc.join(s));
        if p.is_dir() { return Some(p); }
    }
    None
}

// ============================================================
// Shared pipeline (used by both `build_file` and `run_file`)
// ============================================================

/// Run `f`, or return `S::default()` when `env` is set. The MIR
/// opt passes all follow the "set ILANG_NO_<X>=1 to disable"
/// convention, so callers express the gate as a single line.
fn run_if_enabled<S: Default>(env: &str, f: impl FnOnce() -> S) -> S {
    if std::env::var_os(env).is_some() {
        S::default()
    } else {
        f()
    }
}

/// Rewrite every `Array { fixed: Some(_) }` inside a type to
/// `fixed: None`. The slot's runtime value is always built by the
/// MIR lowerer as a dynamic array (header + buffer), so the slot
/// type must agree — otherwise the inline-fixed read path in
/// `lower_array_inst` interprets the header bytes as element data
/// and `count[0]` returns the array's length instead of element 0.
fn normalize_slot_ty(ty: ilang_ast::Type) -> ilang_ast::Type {
    use ilang_ast::Type;
    match ty {
        Type::Array { elem, fixed: _ } => Type::Array {
            elem: Box::new(normalize_slot_ty(*elem)),
            fixed: None,
        },
        Type::Tuple(elems) => Type::Tuple(
            elems
                .into_vec()
                .into_iter()
                .map(normalize_slot_ty)
                .collect(),
        ),
        Type::Optional(inner) => Type::Optional(Box::new(normalize_slot_ty(*inner))),
        Type::Weak(inner) => Type::Weak(Box::new(normalize_slot_ty(*inner))),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const,
            inner: Box::new(normalize_slot_ty(*inner)),
        },
        Type::Generic(g) => {
            let ilang_ast::GenericTy { base, args } = *g;
            Type::Generic(Box::new(ilang_ast::GenericTy {
                base,
                args: args.into_vec().into_iter().map(normalize_slot_ty).collect(),
            }))
        }
        Type::Fn(f) => {
            let ilang_ast::FnTy { params, ret } = *f;
            Type::Fn(Box::new(ilang_ast::FnTy {
                params: params.into_vec().into_iter().map(normalize_slot_ty).collect(),
                ret: normalize_slot_ty(ret),
            }))
        }
        other => other,
    }
}

/// Build the host-slot table: a `let` declared at module scope and
/// referenced from any free fn / method body becomes a host-side
/// slot so `lower_program_with_slots` emits `__repl_load_slot` /
/// `__repl_store_slot` instead of an unbound-variable error.
/// Promoting every top-level let interacts badly with closure
/// capture and ARC for binds only `__main` itself uses, so we
/// scope the promotion to actually-referenced names.
fn build_slot_table(
    prog: &AstProgram,
    tc: &TypeChecker,
) -> HashMap<Symbol, (u32, ilang_ast::Type)> {
    let mut top_let_names: std::collections::HashSet<Symbol> =
        std::collections::HashSet::new();
    for stmt in &prog.stmts {
        if let StmtKind::Let { name, .. } = &stmt.kind {
            top_let_names.insert(*name);
        }
    }
    let mut referenced: std::collections::HashSet<Symbol> =
        std::collections::HashSet::new();
    collect_fn_free_var_refs(prog, &top_let_names, &mut referenced);
    let mut slot_table: HashMap<Symbol, (u32, ilang_ast::Type)> = HashMap::new();
    let mut next_slot: u32 = 0;
    for stmt in &prog.stmts {
        if let StmtKind::Let { name, .. } = &stmt.kind {
            if !referenced.contains(name) {
                continue;
            }
            if let Some(ty) = tc.lookup_global(*name) {
                slot_table.insert(*name, (next_slot, normalize_slot_ty(ty)));
                next_slot += 1;
            }
        }
    }
    ilang_mir_codegen::reset_repl_slots();
    slot_table
}

/// Preserve generic class / enum templates across the first
/// monomorphize round so the second pass can synthesise concrete
/// versions surfaced by specialized fn bodies (e.g. `new Box<i64>`
/// produced by `make<i64>` after fn-spec).
fn monomorphize_with_template_reattach(
    prog: AstProgram,
    tc: &TypeChecker,
) -> AstProgram {
    let generic_class_templates: Vec<ilang_ast::ClassDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            ilang_ast::Item::Class(c) if !c.type_params.is_empty() => Some(c.clone()),
            _ => None,
        })
        .collect();
    let generic_enum_templates: Vec<ilang_ast::EnumDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            ilang_ast::Item::Enum(e) if !e.type_params.is_empty() => Some(e.clone()),
            _ => None,
        })
        .collect();
    let prog = ilang_mir::monomorphize::monomorphize(&prog, &tc.enum_ctor_type_args());
    let prog = ilang_mir::monomorphize::monomorphize_enums(&prog, &tc.enum_ctor_type_args());
    // Re-attach templates BEFORE fn-spec so `monomorphize_fns`
    // sees `generic_enums` populated and can mangle EnumCtor refs
    // in specialized bodies (e.g. `MyOpt.some(v)` inside
    // `wrap_i64` becomes `MyOpt<i64>.some(v)`).
    let reattach_templates = |p: &mut AstProgram| {
        // Strip any template copy first so re-attaching never duplicates.
        p.items.retain(|it| match it {
            Item::Class(c) => c.type_params.is_empty(),
            Item::Enum(e) => e.type_params.is_empty(),
            _ => true,
        });
        for c in &generic_class_templates {
            p.items.push(Item::Class(c.clone()));
        }
        for e in &generic_enum_templates {
            p.items.push(Item::Enum(e.clone()));
        }
    };
    let mut prog = prog;
    reattach_templates(&mut prog);
    let prog = ilang_mir::monomorphize::monomorphize_fns(
        &prog,
        &tc.fn_call_type_args(),
        &tc.enum_ctor_type_args(),
    );
    // Specialize generic class methods, then iterate class + method
    // monomorphization to a FIXED POINT. Each round can mint new class
    // instantiations whose specialized bodies mint further ones — a
    // chained `a.remap(f).remap(g)` walks `Box<i64>` -> `Box<string>`
    // -> `Box<i64>`, and only the round that synthesizes `Box<string>`
    // can specialize its `remap<i64>`. Re-attach the generic templates
    // each round so class-mono can synthesize the newly-discovered
    // instantiations, and stop once the program stops growing. Bounded
    // for safety; genuine infinite instantiation is caught earlier by
    // class-mono's recursion limit.
    // Structural fingerprint: concrete class names plus their method
    // names. A bare count would miss a same-count specialization (a
    // `remap<U>` template replaced by `remap<string>`) that hasn't yet
    // minted a new class — stopping before the round that would.
    let fingerprint = |p: &AstProgram| -> String {
        let mut rows: Vec<String> = Vec::new();
        for it in &p.items {
            if let Item::Class(c) = it {
                if c.type_params.is_empty() {
                    let mut names: Vec<&str> = c
                        .methods
                        .iter()
                        .chain(c.static_methods.iter())
                        .map(|m| m.name.as_str())
                        .collect();
                    names.sort_unstable();
                    rows.push(format!("{}:{}", c.name, names.join(",")));
                }
            }
        }
        rows.sort_unstable();
        rows.join(";")
    };
    let mut prog = prog;
    let mut prev = fingerprint(&prog);
    for _ in 0..64 {
        reattach_templates(&mut prog);
        let next = ilang_mir::monomorphize::monomorphize(&prog, &tc.enum_ctor_type_args());
        let next =
            ilang_mir::monomorphize::monomorphize_methods(&next, &tc.method_call_type_args());
        let fp = fingerprint(&next);
        prog = next;
        if fp == prev {
            break;
        }
        prev = fp;
    }
    ilang_mir::monomorphize::monomorphize_enums(&prog, &tc.enum_ctor_type_args())
}

/// Load + type-check + monomorphize + lower the program at `path`.
/// `wrap_print` is true for `run` (auto-prints the trailing expr
/// like an interpreter) and false for `build` (the executable
/// returns the i64 directly). On error, the relevant message is
/// already on stderr and the returned `ExitCode` is forwarded.
fn lower_to_mir(
    path: &PathBuf,
    wrap_print: bool,
) -> Result<(ilang_mir::Program, String), ExitCode> {
    let dep_tree = match collect_dep_tree(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return Err(ExitCode::FAILURE);
        }
    };
    let extra_paths = dep_tree.dirs;
    let t0 = std::time::Instant::now();
    let prog = match ilang_parser::loader::load_program_full(
        path,
        &extra_paths,
        &dep_tree.parents,
        &dep_tree.names_to_dirs,
        &std::collections::HashMap::new(),
    ) {
        Ok(p) => p,
        Err(e) => {
            let display_path = path.display().to_string();
            let prefix = e.source_file().unwrap_or(&display_path);
            eprintln!("{prefix}: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
    let timing = std::env::var("ILANG_TIMING").is_ok();
    if timing {
        let parse_load = t0.elapsed();
        let lt = ilang_parser::loader::take_loader_timing();
        let dur = std::time::Duration::from_nanos;
        eprintln!(
            "[timing] parse+load: {:?} (files={})",
            parse_load, lt.files,
        );
        eprintln!(
            "[timing]   load_recursive: {:?} = read {:?} + tok {:?} + prescan_use {:?} + dir_objc {:?} + parse {:?} + target {:?} + embed {:?} + objc_collect {:?} + tag_spans {:?}",
            dur(lt.load_recursive_ns),
            dur(lt.read_ns),
            dur(lt.tokenize_ns),
            dur(lt.prescan_use_ns),
            dur(lt.dir_objc_scan_ns),
            dur(lt.parse_ns),
            dur(lt.target_filter_ns),
            dur(lt.expand_embeds_ns),
            dur(lt.collect_objc_ns),
            dur(lt.tag_spans_ns),
        );
        eprintln!(
            "[timing]   post-load: visibility {:?} + apply_use {:?} + renorm {:?} + dup_pub {:?} + auto_lift {:?} + derive {:?} + inline_const {:?} + async {:?}",
            dur(lt.visibility_ns),
            dur(lt.apply_use_ns),
            dur(lt.renormalize_ns),
            dur(lt.dup_pub_ns),
            dur(lt.auto_lift_ns),
            dur(lt.derive_ns),
            dur(lt.inline_const_ns),
            dur(lt.async_lower_ns),
        );
        eprintln!(
            "[timing]     apply_use: prologue {:?} + clone {:?} + toposort {:?} + named_globals {:?} + qualify {:?} + rename {:?} + prefix {:?} + stmts {:?} + wildcard {:?} + selective {:?} | (nested-loop wall {:?})",
            dur(lt.au_prologue_ns),
            dur(lt.au_clone_ns),
            dur(lt.au_toposort_ns),
            dur(lt.au_named_globals_ns),
            dur(lt.au_qualify_ns),
            dur(lt.au_rename_ns),
            dur(lt.au_prefix_ns),
            dur(lt.au_stmts_ns),
            dur(lt.au_wildcard_ns),
            dur(lt.au_selective_ns),
            dur(lt.au_nested_ns),
        );
    }
    let display_path = path.display().to_string();
    let prog = if wrap_print { wrap_trailing_print(prog) } else { prog };
    let t_tc = std::time::Instant::now();
    let mut tc = TypeChecker::new();
    let (_, errs) = tc.check(&prog);
    if timing { eprintln!("[timing] typecheck: {:?}", t_tc.elapsed()); }
    if !errs.is_empty() {
        for e in &errs {
            let err_file = e.span().source_file.as_str();
            let path_for_err = if err_file.is_empty() {
                display_path.as_str()
            } else {
                err_file
            };
            eprintln!("{path_for_err} {e}");
        }
        return Err(ExitCode::FAILURE);
    }
    for w in tc.warnings() {
        let warn_file = w.span.source_file.as_str();
        let path_for_warn = if warn_file.is_empty() {
            display_path.as_str()
        } else {
            warn_file
        };
        eprintln!(
            "{path_for_warn} [{}:{}]: warning: {}",
            w.span.line, w.span.col, w.message
        );
    }
    let t_mangle = std::time::Instant::now();
    let prog = ilang_types::mangle::mangle_overloads(
        prog,
        &tc.fn_overload_picks(),
        &tc.method_overload_picks(),
        &tc.call_default_fills(),
        &tc.objc_invoke_obj_to_obj_spans(),
    );
    if timing { eprintln!("[timing] mangle_overloads: {:?}", t_mangle.elapsed()); }
    let t_slots = std::time::Instant::now();
    let slot_table = build_slot_table(&prog, &tc);
    if timing { eprintln!("[timing] build_slot_table: {:?}", t_slots.elapsed()); }
    let t_mono = std::time::Instant::now();
    let prog = monomorphize_with_template_reattach(prog, &tc);
    if timing { eprintln!("[timing] monomorphize: {:?}", t_mono.elapsed()); }
    let t_ast_dce = std::time::Instant::now();
    let mut prog = prog;
    let ast_dce_stats = if std::env::var_os("ILANG_NO_AST_DCE").is_some() {
        ilang_mir::ast_dce::Stats::default()
    } else {
        ilang_mir::ast_dce::run(&mut prog)
    };
    let prog = prog;
    if timing {
        let n_items = prog.items.len();
        eprintln!(
            "[timing] ast_dce: {:?} (items_removed={}, kept={})",
            t_ast_dce.elapsed(),
            ast_dce_stats.items_removed,
            n_items,
        );
    }
    if std::env::var("ILANG_PIPE_DEBUG").is_ok() {
        for item in &prog.items {
            match item {
                ilang_ast::Item::Fn(f) => eprintln!(
                    "[pipe] Fn {} type_params={:?}",
                    f.name.as_str(),
                    f.type_params.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                ),
                ilang_ast::Item::Class(c) => eprintln!(
                    "[pipe] Class {} type_params={:?}",
                    c.name.as_str(),
                    c.type_params.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                ),
                ilang_ast::Item::Enum(e) => eprintln!(
                    "[pipe] Enum {} type_params={:?}",
                    e.name.as_str(),
                    e.type_params.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                ),
                _ => {}
            }
        }
    }
    let t_lower = std::time::Instant::now();
    let mir = match ilang_mir::lower_program_with_slots(&prog, &slot_table) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{display_path}: mir lower: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
    if timing {
        let n_fn = mir.functions.len();
        let n_extern: usize = mir.functions.iter().filter(|f| matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })).count();
        let n_insts: usize = mir.functions.iter().flat_map(|f| f.blocks.iter()).map(|b| b.insts.len()).sum();
        eprintln!("[timing] mir lower: {:?} (fns={}, extern={}, insts={})", t_lower.elapsed(), n_fn, n_extern, n_insts);
    }
    if std::env::var_os("ILANG_MIR_DUMP").is_some() {
        eprintln!(
            "--- MIR (post-lower, pre-pass) ---\n{}\n--- end MIR ---",
            ilang_mir::print_program(&mir)
        );
    }
    Ok((mir, display_path))
}

/// Run every MIR optimization pass in order, honouring the
/// `ILANG_NO_<X>` env-gates. Emits a one-line stats summary when
/// `ILANG_MIR_PASS_STATS` is set; otherwise the stats are
/// discarded.
fn run_mir_opt_passes(mir: &mut ilang_mir::Program, display_path: &str) {
    let dump_stats = std::env::var_os("ILANG_MIR_PASS_STATS").is_some();
    let (retains_before, releases_before) = if dump_stats {
        count_retain_release(mir)
    } else {
        (0, 0)
    };
    let dce_fn_stats = run_if_enabled("ILANG_NO_DCE_FN", || {
        ilang_mir::passes::dce_fn::run_program(mir)
    });
    let promote_stats = run_if_enabled("ILANG_NO_PROMOTE_LOCALS", || {
        ilang_mir::passes::promote_locals::run_program(mir)
    });
    let inline_stats = run_if_enabled("ILANG_NO_INLINE", || {
        ilang_mir::passes::inline::run_program(mir)
    });
    let const_fold_stats = run_if_enabled("ILANG_NO_CONST_FOLD", || {
        ilang_mir::passes::const_fold::run_program(mir)
    });
    let branch_fold_stats = run_if_enabled("ILANG_NO_BRANCH_FOLD", || {
        ilang_mir::passes::branch_fold::run_program(mir)
    });
    let dce_stats = run_if_enabled("ILANG_NO_DCE", || {
        ilang_mir::passes::dce::run_program(mir)
    });
    let arc_stats = ilang_mir::passes::arc_peephole::run_program(mir);
    if dump_stats {
        eprintln!(
            "{display_path}: dce_fn fns_removed={} classes_removed={} promote_locals locals={} uses={} inline calls_inlined={} const_fold folds_applied={} branch_fold branches={} dce removed={}",
            dce_fn_stats.fns_removed,
            dce_fn_stats.classes_removed,
            promote_stats.locals_promoted,
            promote_stats.uses_rewritten,
            inline_stats.calls_inlined,
            const_fold_stats.folds_applied,
            branch_fold_stats.branches_folded,
            dce_stats.insts_removed,
        );
        let (retains_after, releases_after) = count_retain_release(mir);
        eprintln!(
            "{display_path}: arc_peephole retains={retains_before}->{retains_after} releases={releases_before}->{releases_after} pairs={}",
            arc_stats.pairs_removed
        );
    }
}

/// Pick one loadable lib per fn (primary or fallback) to feed the
/// AOT linker. Missing libs are skipped; `@optional` fns get
/// local abort-stubs (emitted by aot.rs) so the link still
/// succeeds.
fn collect_seen_libs(mir: &ilang_mir::Program) -> std::collections::BTreeSet<String> {
    let available_libs = ilang_mir_codegen::aot::probe_available_libs(mir);
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
    seen_libs
}

// ============================================================
// Linker drivers
// ============================================================

/// Drive the platform linker against the freshly-written object
/// file. Dispatches to `link_unix` or `link_windows`; the
/// returned `ExitCode` is what the surrounding `build_file`
/// should propagate.
fn link_executable(
    object_path: &std::path::Path,
    output: &std::path::Path,
    mir: &ilang_mir::Program,
    seen_libs: &std::collections::BTreeSet<String>,
    display_path: &str,
) -> ExitCode {
    #[cfg(not(windows))]
    {
        link_unix(object_path, output, mir, seen_libs, display_path)
    }
    #[cfg(windows)]
    {
        link_windows(object_path, output, mir, seen_libs, display_path)
    }
}

#[cfg(not(windows))]
fn link_unix(
    object_path: &std::path::Path,
    output: &std::path::Path,
    mir: &ilang_mir::Program,
    seen_libs: &std::collections::BTreeSet<String>,
    display_path: &str,
) -> ExitCode {
    // Use `cc` (or $CC) as the driver. macOS ships `cc` with Xcode
    // CLT; Linux uses whatever GCC / Clang the distro provides.
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut cmd = std::process::Command::new(&cc);
    cmd.arg(object_path).arg("-o").arg(output);
    // Dead-strip unused runtime helpers from the archive. The flag
    // name differs per linker: ld64 (macOS) takes `-dead_strip`,
    // GNU/LLD use `--gc-sections`. Skip the strip when any
    // `ilang_objc_imp__*` IMPs are present — those are referenced
    // only via `dlsym` at runtime, so the linker can't tell they're
    // live and would otherwise prune them.
    let has_objc_imp = mir
        .functions
        .iter()
        .any(|f| f.name.as_str().starts_with("$objc.imp."));
    if !has_objc_imp {
        #[cfg(target_os = "macos")]
        cmd.arg("-Wl,-dead_strip");
        #[cfg(all(not(windows), not(target_os = "macos")))]
        cmd.arg("-Wl,--gc-sections");
    }
    if let Some(rt) = locate_runtime_lib() {
        cmd.arg(&rt);
    }
    if !seen_libs.is_empty() {
        // Homebrew on Apple Silicon: /opt/homebrew; Intel: /usr/local.
        for p in ["/opt/homebrew/lib", "/usr/local/lib"] {
            if std::path::Path::new(p).is_dir() {
                cmd.arg(format!("-L{p}"));
                cmd.arg(format!("-Wl,-rpath,{p}"));
            }
        }
    }
    for lib in seen_libs {
        // macOS framework path detection: `@lib("/System/Library/
        // Frameworks/AppKit.framework/AppKit")`-style entries are
        // routed through the linker's `-framework <name>` flag,
        // not `-l<path>`. dyld resolves them by walking
        // `DYLD_FRAMEWORK_PATH` / standard framework search paths.
        #[cfg(target_os = "macos")]
        if let Some(fw_name) = extract_framework_name(lib) {
            cmd.arg("-framework").arg(fw_name);
            continue;
        }
        cmd.arg(format!("-l{lib}"));
    }
    // Linux: the runtime's `math.*` wrappers pull in glibc's libm
    // (exp/sin/...) which must be linked explicitly. pthread/dl
    // are commonly required by transitive deps; harmless if unused.
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        cmd.arg("-lm");
        cmd.arg("-lpthread");
        cmd.arg("-ldl");
    }
    match cmd.status() {
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
    // Strip symbol table (non-fatal). link.exe does this by default.
    //
    // Plain `strip` removes both local and global symbols, which
    // breaks ObjC IMPs (the runtime resolves them by name via
    // `dlsym`); use `-x` to drop just the local symbols and leave
    // every exported `ilang_objc_imp__*` global intact. Shaves
    // roughly 20–25% off the binary on AppKit-using programs.
    let _ = std::process::Command::new("strip")
        .arg("-x")
        .arg(output)
        .status();
    ExitCode::SUCCESS
}

#[cfg(windows)]
fn link_windows(
    object_path: &std::path::Path,
    output: &std::path::Path,
    mir: &ilang_mir::Program,
    seen_libs: &std::collections::BTreeSet<String>,
    display_path: &str,
) -> ExitCode {
    // `link.exe` collides with Git for Windows' POSIX `link`
    // (hard-link utility). Instead we use the `cc` crate to locate
    // `cl.exe` from the Visual Studio installation, then invoke it
    // as a linker driver with `/link` to pass flags through to the
    // real `link.exe`. The `cc` crate also supplies the required
    // VC++ env vars (LIB, PATH) so `link.exe` can find `ucrt.lib`,
    // `vcruntime.lib`, etc. without needing an open VS Developer
    // Command Prompt. Override with $CC to use any MSVC-compatible
    // driver (e.g. clang-cl, a specific cl.exe path).
    let msvc_target = if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let (cl_path, cl_env) = if let Some(ov) = std::env::var_os("CC") {
        (std::path::PathBuf::from(ov), vec![])
    } else {
        // `cc::windows_registry::find_tool` performs VS discovery
        // (registry, vswhere, env vars) without requiring Cargo's
        // OPT_LEVEL / TARGET / HOST env vars — safe to call from a
        // binary rather than build.rs.
        match cc::windows_registry::find_tool(msvc_target, "cl.exe") {
            Some(tool) => (tool.path().to_path_buf(), tool.env().to_vec()),
            None => {
                eprintln!(
                    "{display_path}: could not locate MSVC cl.exe\n\
                     hint: install Visual Studio with the \
                     \"Desktop development with C++\" workload, \
                     or set the CC environment variable to cl.exe"
                );
                return ExitCode::FAILURE;
            }
        }
    };

    let mut cmd = std::process::Command::new(&cl_path);
    // Apply the VC++ environment (LIB, PATH, etc.) so cl.exe can
    // find link.exe and the CRT import libs.
    for (k, v) in &cl_env {
        cmd.env(k, v);
    }
    cmd.arg("/nologo");
    // cl.exe treats .o and .obj files as pre-compiled objects and
    // passes them directly to link.exe without recompilation.
    cmd.arg(object_path);
    // /Fe: specifies the output executable path.
    cmd.arg(format!("/Fe:{}", output.display()));
    // /link: everything after this is forwarded verbatim to link.exe.
    cmd.arg("/link");
    cmd.arg("/SUBSYSTEM:CONSOLE");
    // Dead-strip equivalent: drop unreferenced functions and COMDAT.
    cmd.arg("/OPT:REF");
    cmd.arg("/OPT:ICF");
    if let Some(rt) = locate_runtime_lib() {
        cmd.arg(&rt);
    }
    // vcpkg lib dir so link.exe resolves SDL2.lib etc.
    if let Some(vcpkg_lib) = find_vcpkg_lib_path() {
        cmd.arg(format!("/LIBPATH:{}", vcpkg_lib.display()));
    }
    // MSVC import libs are named `<lib>.lib`. The dlopen-based
    // probe used above tells us which DLL the JIT loader would
    // pick, but the SDK occasionally ships an import library under
    // a *different* name than the DLL — e.g. the runtime DLL is
    // `d3dcompiler_47.dll` but the Windows 10 SDK only installs
    // `d3dcompiler.lib` (a forwarder). Rebuild the lib set against
    // the LIB search paths the linker will actually use, walking
    // each extern fn's `@lib(name1, name2, ...)` list left-to-right
    // and picking the first `<name>.lib` that exists on disk.
    // Falls back to the dlopen-probe winner when nothing on LIB
    // matches (e.g. a system DLL that has no SDK-side import
    // library at all — those resolve via /DEFAULTLIB records in
    // the object).
    let lib_dirs: Vec<std::path::PathBuf> = cl_env
        .iter()
        .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case("LIB"))
        .map(|(_, v)| std::env::split_paths(v).collect())
        .unwrap_or_default();
    let has_lib = |name: &str| -> bool {
        lib_dirs
            .iter()
            .any(|d| d.join(format!("{name}.lib")).is_file())
    };
    let mut linker_libs: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for f in mir.functions.iter() {
        if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
            continue;
        }
        // Prefer an alternate whose .lib is locatable; fall back to
        // the original dlopen-probe winner if every name fails the
        // on-disk check.
        let mut picked: Option<String> = None;
        for sym in f.libs.iter() {
            let n = sym.as_str();
            if has_lib(n) {
                picked = Some(n.to_string());
                break;
            }
        }
        if picked.is_none() {
            for sym in f.libs.iter() {
                let n = sym.as_str().to_string();
                if seen_libs.contains(&n) {
                    picked = Some(n);
                    break;
                }
            }
        }
        if let Some(n) = picked {
            linker_libs.insert(n);
        }
    }
    for lib in &linker_libs {
        cmd.arg(format!("{lib}.lib"));
    }
    // MSVC CRT and Win32 platform libs. Rust's staticlib objects
    // carry /DEFAULTLIB records pointing to these, but link.exe
    // only resolves them when the libs are locatable via LIB —
    // adding them explicitly avoids the dependency on implicit
    // DEFAULTLIB handling.
    //   msvcrt.lib    — dynamic multithreaded CRT
    //   ucrt.lib      — Universal CRT
    //   vcruntime.lib — VC++ runtime helpers
    //   kernel32.lib  — Win32 kernel
    //   advapi32.lib  — Registry / security (used by Rust std)
    //   bcrypt.lib    — Crypto RNG (used by Rust std for random)
    //   ntdll.lib     — NT API layer (used by Rust std)
    //   userenv.lib   — User profile (used by Rust std)
    //   ws2_32.lib    — Winsock (used by Rust std networking)
    for lib in &[
        "msvcrt.lib", "ucrt.lib", "vcruntime.lib",
        "kernel32.lib", "advapi32.lib", "bcrypt.lib",
        "ntdll.lib", "userenv.lib", "ws2_32.lib",
    ] {
        cmd.arg(lib);
    }
    match cmd.status() {
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
                cl_path.display()
            );
            return ExitCode::FAILURE;
        }
    }
    // link.exe omits debug info by default (no /DEBUG), so no
    // separate strip step is needed.
    ExitCode::SUCCESS
}

// ============================================================
// Subcommand entry points
// ============================================================

/// AOT static capability check: every capability the program's extern
/// calls require must be granted. Returns the first missing one's name.
fn check_caps(mir: &ilang_mir::Program, granted: u32) -> Result<(), &'static str> {
    use ilang_mir::passes::cap_gate::CapKind;
    use ilang_runtime::caps::{CAP_FFI, CAP_FILE, CAP_NET, CAP_OS};
    for cap in ilang_mir::passes::cap_gate::required_caps(mir) {
        let bit = match cap {
            CapKind::File => CAP_FILE,
            CapKind::Os => CAP_OS,
            CapKind::Ffi => CAP_FFI,
            CapKind::Net => CAP_NET,
        };
        if granted & bit == 0 {
            return Err(cap.manifest_name());
        }
    }
    Ok(())
}

fn build_file(path: &PathBuf, output: &PathBuf) -> ExitCode {
    let (mut mir, display_path) = match lower_to_mir(path, /*wrap_print=*/ false) {
        Ok(a) => a,
        Err(code) => return code,
    };
    run_mir_opt_passes(&mut mir, &display_path);
    // Capability enforcement (AOT): every extern call's capability must be
    // granted by `ilang.toml`, verified statically so the produced binary
    // is known-safe at build time (a denied capability fails the build).
    let granted = match manifest::granted_caps(path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{display_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(missing) = check_caps(&mir, granted) {
        eprintln!(
            "{display_path}: capability '{missing}' is required but not \
             granted — add it to ilang.toml (e.g. `capabilities = \
             [\"{missing}\"]`)"
        );
        return ExitCode::FAILURE;
    }
    let object_bytes = match ilang_mir_codegen::compile_program_to_object(&mir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{display_path}: aot: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Drop the intermediate object file next to the eventual
    // executable so users can inspect or rerun the link manually.
    // `.obj` on Windows (MSVC convention; avoids cl.exe warning
    // D9024) and `.o` everywhere else.
    let object_path = output.with_extension(if cfg!(windows) { "obj" } else { "o" });
    if let Err(e) = std::fs::write(&object_path, &object_bytes) {
        eprintln!("{display_path}: write {}: {e}", object_path.display());
        return ExitCode::FAILURE;
    }
    let seen_libs = collect_seen_libs(&mir);
    link_executable(&object_path, output, &mir, &seen_libs, &display_path)
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
    /// Accumulated RAW definitions (Item::Fn / Class / Enum /
    /// ExternC / Const) from every successful chunk. Replayed into
    /// each new chunk's merged program, which then goes through the
    /// loader-equivalent normalize chain (enum-ref renormalize,
    /// @derive, const inlining, async desugar) and a FRESH
    /// type-check — the raw form keeps both idempotent.
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
            accumulated_items: Vec::new(),
            slot_table: HashMap::new(),
            next_slot: 0,
        }
    }

    fn run_chunk(&mut self, src: &str) -> Result<String, String> {
        let toks = tokenize(src).map_err(|e| format!("<repl> {e}"))?;
        let chunk_prog = parse(&toks).map_err(|e| format!("<repl> {e}"))?;

        // `use` needs the file loader's module resolution (std lib
        // injection, renames, prefixing) — not wired up for chunks
        // yet. Reject up front with an actionable message instead of
        // the old "unexpected Item::Use post-loader" MIR error.
        if chunk_prog.items.iter().any(|i| matches!(i, Item::Use(_))) {
            return Err(
                "<repl> `use` isn't supported in the REPL yet — run a file with \
                 `ilang run` for module imports"
                    .into(),
            );
        }

        // Build the merged program first: accumulated definitions +
        // the new chunk's items, with only the new chunk's stmts /
        // tail in the synthesised __main. The normalize chain below
        // needs the accumulated decls in the SAME program (the
        // enum-ref rewrite resolves `E.a` against the program's own
        // enum items).
        let mut per_chunk = AstProgram::default();
        per_chunk.items = self.accumulated_items.clone();
        per_chunk.items.extend(chunk_prog.items.iter().cloned());
        per_chunk.stmts = chunk_prog.stmts.clone();
        per_chunk.tail = chunk_prog.tail.clone();

        // Loader-equivalent normalize: enum-ref renormalize, @derive
        // expansion, const inlining, async desugar. The REPL used to
        // skip all of these — enums were unusable across chunks
        // (`E.a` stayed a Field access → "undefined variable E"),
        // and `async fn` hit the legacy pre-state-machine error.
        let per_chunk = ilang_parser::loader::normalize_repl_chunk(per_chunk)
            .map_err(|e| format!("<repl> {e}"))?;

        // Echo a bare trailing expression's value, whatever its type —
        // `console.log(tail)` prints i64 / string / bool / float / array
        // / tuple / optional / object uniformly (the old path printed
        // only `__main`'s i64 return, so `"hello"` / `true` / `3.14`
        // silently produced nothing). Unconditional like `run`'s
        // wrap_trailing_print: a `console.log(x)` tail becomes
        // `console.log(console.log(x))`, but the inner returns Unit and
        // `console.log(())` prints nothing, so it doesn't double up.
        // A statement-only chunk (`let x = 5`) has no tail and is left
        // alone.
        let per_chunk = wrap_trailing_print(per_chunk);

        // Fresh TypeChecker per chunk over the whole merged program.
        // (Re-checking accumulated items with a persistent checker
        // trips the duplicate-overload guard.) Prior chunks' lets
        // exist only as host slots — seed their types so the new
        // chunk's references resolve.
        let mut tc = TypeChecker::new();
        for (name, (_idx, ty)) in &self.slot_table {
            tc.define_global(*name, ty.clone());
        }
        let (_, chunk_errs) = tc.check(&per_chunk);
        if let Some(e) = chunk_errs.first() {
            return Err(format!("<repl> {e}"));
        }

        // Promote any new top-level `let` to a slot. The AST type
        // gets resolved to MirTy inside the lowerer once it has
        // class / enum ids; we just store the AST type here. A let
        // the checker has no type for can't persist — say so instead
        // of silently dropping it (the old behaviour left the next
        // chunk with a bare "unbound variable" from MIR).
        for stmt in &chunk_prog.stmts {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                if let Some((_idx, slot_ty)) = self.slot_table.get(name) {
                    // Same-type re-`let` overwrites the slot (normal
                    // REPL workflow). A DIFFERENT type would store
                    // the new value's bits into a slot the table
                    // still types as the old one — the next read
                    // reinterpreted them (a string re-let over an
                    // i64 slot printed its raw pointer). Refuse it.
                    let new_ty = tc.lookup_global(*name).map(normalize_slot_ty);
                    if let Some(new_ty) = new_ty {
                        if new_ty != *slot_ty {
                            return Err(format!(
                                "<repl> `{name}` is already bound with a different \
                                 type — re-`let` keeps the slot's original type, so \
                                 changing it isn't supported; use a new name"
                            ));
                        }
                    }
                    continue;
                }
                let Some(ty) = tc.lookup_global(*name) else {
                    eprintln!(
                        "<repl> note: `{name}` has no resolvable top-level type — \
                         it won't persist past this chunk"
                    );
                    continue;
                };
                let idx = self.next_slot;
                self.next_slot += 1;
                self.slot_table.insert(*name, (idx, normalize_slot_ty(ty)));
            }
        }

        // The downstream MIR pipeline reads picks / type-arg dicts
        // from this chunk's checker — it saw the whole merged
        // program, so entries cover accumulated items too.
        let prog = ilang_types::mangle::mangle_overloads(
            per_chunk,
            &tc.fn_overload_picks(),
            &tc.method_overload_picks(),
            &tc.call_default_fills(),
            &tc.objc_invoke_obj_to_obj_spans(),
        );
        // Seed both monomorphize passes with the persistent slot
        // types: a chunk that only READS a generic-typed slot
        // (`Result<i64, string>`, `Box<i64>`) has no instantiation
        // site of its own, and without the seed the specialized
        // class / enum never materializes and the slot silently
        // fails to resolve ("unbound variable" on the next read).
        let slot_request_types: Vec<ilang_ast::Type> =
            self.slot_table.values().map(|(_i, t)| t.clone()).collect();
        let prog = ilang_mir::monomorphize::monomorphize_with_requests(
            &prog,
            &slot_request_types,
            &tc.enum_ctor_type_args(),
        );
        let prog = ilang_mir::monomorphize::monomorphize_enums_with_requests(
            &prog,
            &tc.enum_ctor_type_args(),
            &slot_request_types,
        );
        let prog =
            ilang_mir::monomorphize::monomorphize_fns(
                &prog,
                &tc.fn_call_type_args(),
                &tc.enum_ctor_type_args(),
            );
        let prog = ilang_mir::monomorphize::monomorphize_methods(
            &prog,
            &tc.method_call_type_args(),
        );
        let mut mir = ilang_mir::lower_program_with_slots_opts(
            &prog,
            &self.slot_table,
            /*release_slots_at_exit=*/ false,
        )
        .map_err(|e| format!("<repl> mir: {e}"))?;
        ilang_mir::passes::promote_locals::run_program(&mut mir);
        ilang_mir::passes::inline::run_program(&mut mir);
        ilang_mir::passes::const_fold::run_program(&mut mir);
        ilang_mir::passes::branch_fold::run_program(&mut mir);
        ilang_mir::passes::dce::run_program(&mut mir);
        ilang_mir::passes::arc_peephole::run_program(&mut mir);
        let compiled = ilang_mir_codegen::compile_program(&mir)
            .map_err(|e| format!("<repl> mir-codegen: {e}"))?;
        let _ = ilang_mir_codegen::run_main(&compiled);

        // Commit the chunk's definitions to the accumulated state
        // only after a successful run — partial failures don't
        // pollute future chunks.
        self.accumulated_items.extend(chunk_prog.items.into_iter());

        // Any value to show was already printed by the wrapped trailing
        // `console.log` above; nothing for `run_repl` to echo.
        Ok(String::new())
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

fn run_file(path: &PathBuf, mir_jit: bool) -> ExitCode {
    let _ = mir_jit;
    let timing = std::env::var("ILANG_TIMING").is_ok();
    let (mut mir, display_path) = match lower_to_mir(path, /*wrap_print=*/ true) {
        Ok(a) => a,
        Err(code) => return code,
    };
    let t_opt = std::time::Instant::now();
    run_mir_opt_passes(&mut mir, &display_path);
    if timing {
        let n_fn = mir.functions.len();
        let n_extern: usize = mir.functions.iter().filter(|f| matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })).count();
        let n_insts: usize = mir.functions.iter().flat_map(|f| f.blocks.iter()).map(|b| b.insts.len()).sum();
        eprintln!("[timing] mir opt passes: {:?} (post-opt fns={}, extern={}, insts={})", t_opt.elapsed(), n_fn, n_extern, n_insts);
    }
    // Capability enforcement (JIT): grant from `ilang.toml`, then insert
    // a runtime gate before every extern call. A denied capability aborts
    // at runtime when the gated sink is actually reached.
    let granted = match manifest::granted_caps(path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{display_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    ilang_runtime::caps::set_granted(granted);
    ilang_mir::passes::cap_gate::insert_gates(&mut mir);
    let t_cg = std::time::Instant::now();
    let compiled = match ilang_mir_codegen::compile_program(&mir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{display_path}: mir-codegen: {e}");
            return ExitCode::FAILURE;
        }
    };
    if timing {
        eprintln!("[timing] cranelift compile: {:?}", t_cg.elapsed());
        eprintln!("[timing] ---- entering run_main ----");
    }
    let r = ilang_mir_codegen::run_main(&compiled);
    // The MIR pipeline returns __main's i64; print it only if
    // it's non-zero so stdout-capture-based tests stay clean.
    if r != 0 {
        println!("{r}");
    }
    ExitCode::SUCCESS
}
