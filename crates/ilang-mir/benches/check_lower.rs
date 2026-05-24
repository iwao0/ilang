//! Type-check and MIR-lower pipeline benchmarks.
//!
//! Measures the post-parse stages: `ilang_types::check` and
//! `ilang_mir::lower_program`. Inputs are loaded through
//! `ilang_parser::loader::load_program` so `use stdlib_module`
//! resolves naturally — without that, most fixtures fail to check.
//!
//! Test fixtures that intentionally trigger type / load errors are
//! skipped (we only bench programs that reach lower successfully on
//! main); the count is reported once at startup so a sudden drop
//! flags fixture-set drift.
//!
//! Three layers:
//! - `check`  : type check only (parse + load is hoisted out of the
//!              measurement loop).
//! - `lower`  : type check + MIR lower (lower needs a checked program
//!              to be meaningful; we time both together because they
//!              run in lockstep in the real pipeline).
//! - `pipeline_no_lex`: parse-of-loaded-program + check + lower.
//!              Approximates "everything after IO/lexing".

use std::path::PathBuf;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Load every `.il` fixture under tests/programs that successfully
/// makes it through load + check + lower on main. Returns the loaded
/// `ast::Program` together with its byte size (sum of all included
/// source files) for throughput reporting.
fn load_corpus() -> Vec<(String, ilang_ast::Program, u64)> {
    let dir = workspace_root().join("crates/ilang-cli/tests/programs");
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("il") {
            continue;
        }
        // Loader pulls in stdlib + sibling modules. Anything that
        // load/check/lower already rejects is by definition not a
        // candidate for steady-state pipeline measurement.
        let Ok(prog) = ilang_parser::loader::load_program(p) else { continue };
        if !ilang_types::check(&prog).1.is_empty() { continue }
        if ilang_mir::lower_program(&prog).is_err() { continue }
        let bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let name = p.strip_prefix(&dir).unwrap_or(p).display().to_string();
        out.push((name, prog, bytes));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn bench_all(c: &mut Criterion) {
    let corpus = load_corpus();
    let bytes: u64 = corpus.iter().map(|(_, _, b)| *b).sum();
    eprintln!(
        "[bench] corpus: {} programs / {} entry-file bytes",
        corpus.len(),
        bytes
    );

    let mut group = c.benchmark_group("check_lower");
    group.throughput(Throughput::Bytes(bytes));

    group.bench_function("check", |b| {
        b.iter(|| {
            let mut ok = 0usize;
            for (_, prog, _) in &corpus {
                if ilang_types::check(prog).1.is_empty() {
                    ok += 1;
                }
            }
            std::hint::black_box(ok);
        });
    });

    group.bench_function("lower", |b| {
        b.iter(|| {
            let mut ok = 0usize;
            for (_, prog, _) in &corpus {
                // The pipeline always runs check before lower; mirror
                // that here so the lower bench reflects realistic
                // upstream state (and because monomorph collection in
                // lower can rely on check-side resolution).
                if !ilang_types::check(prog).1.is_empty() { continue }
                if ilang_mir::lower_program(prog).is_ok() {
                    ok += 1;
                }
            }
            std::hint::black_box(ok);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
