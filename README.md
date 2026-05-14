# 🚀 ilang

English | [日本語](docs/README_ja.md)

> 🦀 **ARC** memory safety · ⚡ **MIR → Cranelift JIT** · 🔬 **leak checks built in** · 🔗 **C FFI** · 🎮 ships with an **SDL2 game demo**

A compiler/runtime for **ilang**, a young programming language
under active design. MIR-based Cranelift JIT for fast cold-start
*and* fast steady-state, ARC for deterministic destruction (no
GC pauses), and a Rust-flavoured surface syntax that talks to C
libraries when you need to.

<p align="center">
  <a><img src="https://github.com/iwao0/ilang/releases/download/demo-assets/breakout.gif" alt="Breakout written in ilang, running on the Cranelift JIT with SDL2" width="600"></a>
</p>

> Breakout in ilang — classes, ARC, closures and SDL2 bindings, JIT-compiled.
> Source under [`examples/sdl_breakout/`](examples/sdl_breakout/).

## ✨ A taste of the syntax

```rust
fn fib(n: i64): i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
console.log(fib(20))    // → 6765
```

## 🔬 Leak checks are first-class

Most languages need a separate tool for memory hygiene —
Valgrind, ASan, Xcode Instruments, a heap-snapshot diff. ilang
exposes the allocator counter through the standard `test`
module, so a leak assertion is *just another `expect`*:

```rust
use test

let baseline = test.liveAllocBytes()

let i = 0
while i < 1000 {
    let _ = "x" + i.toString()   // intermediate strings, fresh heap
    i = i + 1
}

// All that intermediate heap should be reclaimed by now.
test.expectTrue(test.liveAllocBytes() - baseline < 1024)
```

`test.liveAllocBytes()` / `liveAllocCount()` / `liveStringCount()`
report what the runtime is currently holding. The fixture suite
under [`tests/programs/05_edge_cases/leak_*.il`](crates/ilang-cli/tests/programs/05_edge_cases/)
uses this pattern to lock down per-construct memory contracts —
30+ fixtures across string concat, array push, closure cells,
enum payloads, map deletions, weak upgrades, and more.

This is the part of ilang we think is genuinely uncommon: every
PR runs its leak tests automatically, no external tooling, no
sampling profiler, no environment setup.

---

## ✨ Vision

- 🛡️ **Capability-based security** — libraries and classes carry
  permissions like `net`, `file`; the host grants them at use sites
  to reduce supply-chain blast radius. *(annotations parse today;
  enforcement is the next big milestone)*
- 🦀 **ARC** for memory safety — no ownership / `mut` / borrow
  checker.
- 🦀 **Rust-flavoured** declarations and type names.
- 🌐 **C / JavaScript** arithmetic semantics (operator precedence,
  integer-promotion behaviour).

## 📚 Syntax cheatsheet

The implemented syntax / types / built-ins are catalogued in
**[docs/syntax.md](docs/syntax.md)**.

## ✅ Status

| Category | Status |
| --- | :---: |
| Numeric types (i8–i64 / u8–u64 / f32 / f64) + `as` casts | ✅ |
| `bool` / comparison / short-circuit logic | ✅ |
| `let` / assignment / compound assignment (`+=` and friends) | ✅ |
| Control flow (`if` / `else` / `while` / `loop` / `break` / `continue` / early `return`) | ✅ |
| Strings / arrays (dynamic & fixed-length) | ✅ |
| `class` / `new` / `init` / `this` / `deinit` (JS-style) | ✅ |
| Inheritance (`class Sub: Parent` / `super`) + virtual dispatch | ✅ |
| Interfaces (`interface I { ... }`) + dynamic dispatch | ✅ |
| `console.log` | ✅ |
| Optional (`T?` / `some` / `none` / `if let`) | ✅ |
| Weak references (`T.weak` / `.get()`) | ✅ |
| `enum` + `match` (with built-in `Result<T, E>`) | ✅ |
| `Map<K, V>` / Tuple | ✅ |
| Closures (with capture) | ✅ |
| Generics (functions / classes / enums) | ✅ |
| Function overloading | ✅ |
| ARC-based memory management | ✅ |
| Type checking | ✅ |
| MIR → Cranelift JIT (`ilang run`, default) | ✅ |
| MIR → Cranelift AOT (`ilang build foo.il -o foo`, macOS) | ✅ |
| Built-in leak helpers (`test.liveAllocBytes` / `liveAllocCount` / `liveStringCount`) | ✅ |
| FFI (`@extern(C) {}` blocks calling C libraries) | ✅ |
| Capability annotations | 🚧 parse-only |

No ownership / `mut` / borrow checker — every variable is
reassignable. Errors are emitted in the uniform
`filename [row:col]: message` format.

## ⏱️ Benchmarks

Six micro-benchmarks comparing ilang against C, Rust, Node.js, Lua,
and Python. Same algorithm in every implementation; numbers are the
median of three runs. The `ilang JIT` column times `ilang run` end
to end — parse + compile + execute — so short benchmarks are
dominated by the compile step. See [benchmarks/](benchmarks/) for the
sources and `benchmarks/run.sh` for the runner.

Author's machine (M-series Mac, macOS 15, `cc` from Xcode CLT 16,
`rustc 1.x -O`, Node.js 22, Lua 5.5, Python 3.13):

| Benchmark       | C     | Rust  | ilang AOT | ilang JIT | Node.js | Lua   | Python |
|-----------------|-------|-------|-----------|-----------|---------|-------|--------|
| `fib(40)`       | 0.15s | 0.16s | 0.34s     | 0.34s     | 0.65s   | 3.43s | 13.55s |
| `mandelbrot`    | 0.46s | 0.52s | 0.67s     | 0.59s     | 0.59s   | 8.56s | 57.96s |
| `sort` (200 k)  | 0.01s | 0.01s | 0.02s     | 0.01s     | 0.05s   | 0.06s | 0.22s  |
| `linked_list`   | 0.03s | 0.02s | 0.04s     | 0.03s     | 0.05s   | 0.11s | 0.42s  |
| `string_concat` | 0.02s | 0.00s | 0.00s¹    | 0.00s¹    | 0.02s   | 0.04s | 0.02s  |
| `ffi`           | 0.01s | 0.01s | 0.01s     | 0.01s     | --      | --    | 2.14s  |

¹ Below the 10 ms resolution of `time -p`. The `s = s + expr`
shape is detected at MIR time and lowered to an in-place
doubling-realloc helper so the total cost is O(n) instead of the
naive O(n²) every other column except Python / Node.js / Lua here
actually pays. Measured at ~3 ms wall clock for 50 k appends.

Reproduce on your own machine with `bash benchmarks/run.sh`. Tools
that aren't installed render as `--` (Node.js and stock Lua ship no
built-in FFI, so the `ffi` row leaves their cells empty).

The big takeaways:

- **Hot integer / floating-point loops** (`fib`, `mandelbrot`) land
  in the 1.3–2.3× C range — Cranelift isn't LLVM, but it's in the
  same neighbourhood.
- **Heap-allocation heavy** workloads (`linked_list`) match C
  because ilang's ARC overhead per node is small (no GC pauses
  either).
- `string_concat`'s `s = s + expr` shape is rewritten at MIR time
  to an in-place doubling-realloc, so the whole loop runs in
  amortised O(n). Other ARC paths (e.g. building a string through
  a closure-captured accumulator, or `t = s + u` with `s` aliased)
  still allocate a fresh buffer and stay on the slow path.
- `ilang JIT` ≈ `ilang AOT` on these benchmarks because the
  programs run long enough that compile cost is a small slice.
  Sub-second scripts will see JIT > AOT in wall-clock time.

## 🔧 Setup

ilang is implemented in Rust, so you'll need a **Rust toolchain**
(rustup / cargo). If it isn't installed, grab it from
<https://rustup.rs>:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Clone the repo:

```sh
git clone https://github.com/iwao0/ilang
cd ilang
cargo build           # first run resolves deps + compiles (~1 minute)
```

## 🚀 Usage

```sh
# 💬 REPL — let / fn persist across lines, JIT-backed
cargo run -p ilang-cli

# 📄 Run a file (`;` is optional, newlines act as statement separators
#    JS-ASI style). Routes through the MIR → Cranelift JIT pipeline.
cargo run -p ilang-cli -- run path/to/script.il

# 📦 Build a native executable (macOS, Cranelift AOT). The system
#    `cc` links the emitted object against `libilang_runtime.a` plus
#    any `@lib("X")` the program references; the output runs without
#    a Rust toolchain installed.
cargo run -p ilang-cli -- build path/to/script.il -o path/to/script
./path/to/script
```

The JIT path resolves `@extern(C) {}` C symbols through dlsym at
JIT-build time; the AOT path resolves them at link time and skips
missing `@optional` libs.

### 🧮 Sample: count multiples of 3 or 5 from 1 to 100

```sh
cat > sample.il <<'EOF'
fn count_div(n: i64): i64 {
    let i = 1
    let count = 0
    while i <= n {
        if i % 3 == 0 || i % 5 == 0 {
            count = count + 1
        }
        i = i + 1
    }
    count
}
count_div(100)
EOF
cargo run -p ilang-cli -- run sample.il   # => 47
```

### 🧱 Classes

JS-flavoured objects with `class` / `new` / `init` (constructor) /
`this`. Inside method bodies you can omit `this.` for fields and
methods (a local or parameter of the same name still wins).

```sh
cat > counter.il <<'EOF'
class Counter {
    count: i64
    init(start: i64) { this.count = start }
    bump(): i64 {
        count += 1     // same as `this.count += 1`
        count
    }
}

let c = new Counter(10)
let i = 0
loop {
    if i >= 5 { break }
    c.bump()
    i += 1
}
c.bump()
EOF
cargo run -p ilang-cli -- run counter.il   # => 16
```

### 🎮 Sample: an SDL2 game window

`examples/sdl_breakout/` contains an SDL2 demo: a neon-style
breakout game with paddle, bricks, particles, and gamepad support.
Arrow keys / `A` `D` (or D-pad / left stick) move the paddle,
`Space` launches the ball, `F` toggles fullscreen, `R` restarts
after game over, `ESC` quits.

Install SDL2 first:

```sh
# 🍎 macOS (Homebrew)
brew install sdl2

# 🐧 Debian/Ubuntu
sudo apt install libsdl2-dev libsdl2-2.0-0

# 🪟 Windows (vcpkg)
vcpkg install sdl2:x64-windows
# Then copy SDL2.dll next to the binary so the JIT can load it at runtime:
copy "%VCPKG_ROOT%\installed\x64-windows\bin\SDL2.dll" target\release\
```

Run:

```sh
# JIT
cargo run -p ilang-cli -- run examples/sdl_breakout/main.il

# AOT — produces a standalone ~735 KB Mach-O binary that links
# against SDL2 at load time.
cargo run -p ilang-cli -- build examples/sdl_breakout/main.il -o breakout
./breakout
```

The sample pulls in the SDL2 bindings under `bindings/sdl2/` via a
plain `use sdl`. The mechanism (an `ilang.toml` with a `[deps]`
table) is documented in
[bindings/sdl2/README.md](bindings/sdl2/README.md).

To use the SDL2 bindings from your own project, drop an
`ilang.toml` next to (or above) your entry file:

```toml
[package]
name = "my_game"

[deps]
sdl2 = "/path/to/ilang/bindings/sdl2"
```

The CLI walks upward from the entry file looking for `ilang.toml`
at startup; each `[deps]` value becomes an additional search
directory for `use module` resolution.

## 🧩 VSCode

`vscode-extension/` ships syntax highlighting plus a language
server (`ilang-lsp`) for diagnostics, hover, and go-to-definition.
Local install:

```sh
# 1. Build the language server
cargo build -p ilang-lsp

# 2. Build the extension client
cd vscode-extension
npm install
npm run compile

# 3. Symlink into VSCode's extensions directory
ln -s "$(pwd)" ~/.vscode/extensions/ilang
```

Restart VSCode. See
[vscode-extension/README.md](vscode-extension/README.md) for
configuration (`ilang.serverPath`) and current limitations.

## 🛠️ Development

Run the whole test suite — Rust unit tests across every crate plus
the language-level fixtures under
`crates/ilang-cli/tests/programs/` (each `.il` fixture runs through
the MIR → Cranelift JIT, with `expect:` / `expect-error:` magic
comments asserting the outcome):

```sh
cargo test --workspace

# also exercise the AOT path on every fixture (build + run + diff
# stdout against the JIT output). Adds ~80 s to the run.
ILANG_TEST_AOT=1 cargo test --workspace
```

## 📄 License

MIT OR Apache-2.0
