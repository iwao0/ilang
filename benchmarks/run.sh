#!/usr/bin/env bash
# Benchmark runner — compiles native binaries, runs each implementation
# three times per benchmark, prints a markdown table of the medians.
#
# Requires (per language used): cc, rustc, python3, node, lua,
# and a `cargo --release` build of the `ilang` CLI. Missing tools are
# skipped with a `--` cell.
set -euo pipefail

cd "$(dirname "$0")"
ROOT="$(pwd)"
OUT="$ROOT/out"
mkdir -p "$OUT"
RES="$OUT/results.tsv"
: >"$RES"

BENCHES=(fib mandelbrot sort linked_list string_concat ffi)
LANGS=(c rs il_aot il_jit js lua py)

ILANG_BIN="${ILANG_BIN:-$ROOT/../target/release/ilang}"
if [[ ! -x "$ILANG_BIN" ]]; then
    echo "Building release ilang…" >&2
    (cd "$ROOT/.." && cargo build --release -p ilang-cli >&2)
fi

have() { command -v "$1" >/dev/null 2>&1; }

run_one() {
    local cmd="$*"
    local t1 t2 t3
    t1=$(/usr/bin/time -p sh -c "$cmd >/dev/null" 2>&1 | awk '/real/{print $2}')
    t2=$(/usr/bin/time -p sh -c "$cmd >/dev/null" 2>&1 | awk '/real/{print $2}')
    t3=$(/usr/bin/time -p sh -c "$cmd >/dev/null" 2>&1 | awk '/real/{print $2}')
    printf "%s\n%s\n%s\n" "$t1" "$t2" "$t3" | sort -n | sed -n '2p'
}

record() {
    # bench lang seconds
    printf '%s\t%s\t%s\n' "$1" "$2" "$3" >>"$RES"
}

for b in "${BENCHES[@]}"; do
    echo "=== $b ===" >&2
    src_dir="$ROOT/$b"

    if have cc && [[ -f "$src_dir/main.c" ]]; then
        bin="$OUT/${b}_c"
        cc -O3 -o "$bin" "$src_dir/main.c"
        record "$b" c "$(run_one "$bin")"
    fi

    if have rustc && [[ -f "$src_dir/main.rs" ]]; then
        bin="$OUT/${b}_rs"
        rustc -O -o "$bin" "$src_dir/main.rs" 2>/dev/null
        record "$b" rs "$(run_one "$bin")"
    fi

    if have python3 && [[ -f "$src_dir/main.py" ]]; then
        record "$b" py "$(run_one "python3 $src_dir/main.py")"
    fi

    if have node && [[ -f "$src_dir/main.js" ]]; then
        record "$b" js "$(run_one "node $src_dir/main.js")"
    fi

    if have lua && [[ -f "$src_dir/main.lua" ]]; then
        record "$b" lua "$(run_one "lua $src_dir/main.lua")"
    fi

    if [[ -x "$ILANG_BIN" && -f "$src_dir/main.il" ]]; then
        bin="$OUT/${b}_il"
        "$ILANG_BIN" build "$src_dir/main.il" -o "$bin" 2>/dev/null
        record "$b" il_aot "$(run_one "$bin")"
        record "$b" il_jit "$(run_one "$ILANG_BIN run $src_dir/main.il")"
    fi
done

lookup() {
    # bench lang -> seconds or empty
    awk -F'\t' -v b="$1" -v l="$2" '$1==b && $2==l { print $3; exit }' "$RES"
}

fmt() {
    local v="$1"
    if [[ -z "$v" ]]; then printf '%s' '--'
    else printf '%.2fs' "$v"
    fi
}

printf '\n| Benchmark       | C       | Rust    | ilang AOT | ilang JIT | Node.js | Lua    | Python  |\n'
printf   '|-----------------|---------|---------|-----------|-----------|---------|--------|---------|\n'
for b in "${BENCHES[@]}"; do
    printf '| %-15s | %-7s | %-7s | %-9s | %-9s | %-7s | %-6s | %-7s |\n' \
        "$b" \
        "$(fmt "$(lookup "$b" c)")" \
        "$(fmt "$(lookup "$b" rs)")" \
        "$(fmt "$(lookup "$b" il_aot)")" \
        "$(fmt "$(lookup "$b" il_jit)")" \
        "$(fmt "$(lookup "$b" js)")" \
        "$(fmt "$(lookup "$b" lua)")" \
        "$(fmt "$(lookup "$b" py)")"
done
