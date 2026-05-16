# Benchmarks

Six micro-benchmarks comparing ilang against C, Rust, Node.js, Lua,
and Python on the same machine. All implementations follow the same
algorithm so the numbers reflect language / runtime overhead rather
than algorithmic differences.

## Benchmarks

| Name              | What it stresses                                            |
| ----------------- | ----------------------------------------------------------- |
| `fib`             | Recursive function calls and integer arithmetic (`fib(40)`) |
| `mandelbrot`      | `f64` tight loop, branch-heavy escape test (1024×1024×1000) |
| `sort`            | Manual quicksort over 2 000 000 `i64` values                |
| `linked_list`     | Heap allocation + traversal (10 000 000 ARC / GC nodes)     |
| `string_concat`   | Quadratic `s = s + "x"` (500 000 iterations)                |
| `ffi`             | C ABI boundary: `abs()` × 100 000 000                       |

Each implementation prints a final value (sum, length, etc.) so the
optimiser can't elide the work.

## Running

```sh
./run.sh
```

The runner

1. Builds release native binaries for C (`cc -O3`), Rust
   (`rustc -O`), and ilang AOT (`ilang build`).
2. Runs each implementation three times per benchmark and reports
   the **median wall-clock time** (`/usr/bin/time -p`).
3. Prints a markdown table at the end.

Missing tools are skipped — the corresponding cell renders as `--`.

`ILANG_BIN` overrides the ilang binary path (default
`../target/release/ilang`; the runner falls back to
`cargo build --release -p ilang` when the binary is missing).

## Notes

- `string_concat` is intentionally the textbook O(n²) `s = s + "x"`
  shape — Python's CPython optimises `+=` to mutate in place when
  possible, so its number is faster than the algorithm suggests.
  Use `"".join(...)` in real Python code.
- `linked_list` allocates one ARC / GC node per iteration; languages
  with bump allocators (Rust's `Box`) or relaxed memory ordering can
  do much better than naive `malloc`.
- The `ffi` benchmark calls libc `abs` through each language's FFI
  layer; Lua and Node.js don't ship a stable FFI in their stock
  distributions, so those rows render as `--`. JavaScript runtimes
  also tend to inline trivial math, so a fair cross-language FFI
  comparison there would need a non-trivial native callee.
- `ilang JIT` measures `ilang run`, which **includes compilation**
  in the timed window — short benchmarks are dominated by it. Use
  the `ilang AOT` row for steady-state code-quality comparisons.
- Numbers are not committed here on purpose: relative ordering
  between machines is informative, absolute numbers are not.
