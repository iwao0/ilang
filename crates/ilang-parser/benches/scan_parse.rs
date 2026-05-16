//! Scanner / parser micro-benchmarks.
//!
//! Inputs:
//! - `stdlib` : the 7 .il files bundled with ilang-parser.
//! - `programs` : every .il file under `crates/ilang-cli/tests/programs`.
//! - `big_concat` : all `programs` sources concatenated into one buffer
//!   to amplify per-call overheads (parser cannot run on this because
//!   independent programs would collide; lexer-only).
//!
//! Each input is benchmarked at three layers:
//! - `lex`   : `ilang_lexer::tokenize`
//! - `parse` : `tokenize` + `ilang_parser::parse`
//!
//! The benches read the .il files from disk once at startup. They do NOT
//! resolve `use` directives — that would drag the file loader into the
//! measurement and inflate per-file overhead. Parse failures on inputs
//! that need stdlib symbols are tolerated (we time the partial parse).

use std::fs;
use std::path::PathBuf;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/ilang-parser.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().parent().unwrap().to_path_buf()
}

fn load_dir(rel: &str) -> Vec<(String, String)> {
    let dir = workspace_root().join(rel);
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("il") {
            if let Ok(src) = fs::read_to_string(p) {
                let name = p.strip_prefix(&dir).unwrap_or(p).display().to_string();
                out.push((name, src));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn total_bytes(files: &[(String, String)]) -> u64 {
    files.iter().map(|(_, s)| s.len() as u64).sum()
}

fn bench_corpus(c: &mut Criterion, group_name: &str, files: &[(String, String)]) {
    let bytes = total_bytes(files);
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(bytes));

    // Some fixtures intentionally contain trojan-source / malformed
    // input to exercise lexer errors. Skip them rather than panic — we
    // still want to measure the lexer on every byte we feed it.
    group.bench_function("lex", |b| {
        b.iter(|| {
            let mut total_tokens = 0usize;
            for (_, src) in files {
                if let Ok(toks) = ilang_lexer::tokenize(src) {
                    total_tokens += toks.len();
                }
            }
            std::hint::black_box(total_tokens);
        });
    });

    group.bench_function("parse", |b| {
        b.iter(|| {
            let mut ok = 0usize;
            for (_, src) in files {
                let Ok(toks) = ilang_lexer::tokenize(src) else { continue };
                // Some test programs depend on stdlib symbols and only
                // parse cleanly through the loader; ignore parse errors
                // and measure the parser work that did happen.
                if ilang_parser::parse(&toks).is_ok() {
                    ok += 1;
                }
            }
            std::hint::black_box(ok);
        });
    });

    group.finish();
}

fn bench_concat_lex(c: &mut Criterion, files: &[(String, String)]) {
    let mut big = String::new();
    for (_, s) in files {
        // Skip fixtures that the lexer rejects on their own — we don't
        // want a single trojan-source test poisoning the concat input.
        if ilang_lexer::tokenize(s).is_err() {
            continue;
        }
        big.push_str(s);
        big.push('\n');
    }
    let bytes = big.len() as u64;
    let mut group = c.benchmark_group("big_concat");
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("lex", |b| {
        b.iter(|| {
            let toks = ilang_lexer::tokenize(&big).expect("lex failed");
            std::hint::black_box(toks.len());
        });
    });
    group.finish();
}

fn bench_all(c: &mut Criterion) {
    let stdlib = load_dir("crates/ilang-parser/src/stdlib");
    let programs = load_dir("crates/ilang-cli/tests/programs");
    eprintln!(
        "[bench] stdlib: {} files / {} bytes",
        stdlib.len(),
        total_bytes(&stdlib)
    );
    eprintln!(
        "[bench] programs: {} files / {} bytes",
        programs.len(),
        total_bytes(&programs)
    );
    bench_corpus(c, "stdlib", &stdlib);
    bench_corpus(c, "programs", &programs);
    bench_concat_lex(c, &programs);
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
