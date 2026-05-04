# ilang

English | [日本語](docs/README_ja.md)

A compiler/runtime for **ilang**, a new programming language under
active design.

## Vision

- **Capability-based security** — libraries and classes carry
  permissions like `net`, `file`; the host grants them at use sites
  to reduce supply-chain blast radius.
- **ARC** for memory safety — no ownership / `mut` / borrow checker.
- **Rust-flavoured** declarations and type names.
- **C / JavaScript** arithmetic semantics (operator precedence,
  integer-promotion behaviour).

## Syntax cheatsheet

The implemented syntax / types / built-ins are catalogued in
**[docs/syntax.md](docs/syntax.md)**.

## Status

| Category | Status |
| --- | :---: |
| Numeric types (i8–i64 / u8–u64 / f32 / f64) + `as` casts | ✅ |
| `bool` / comparison / short-circuit logic | ✅ |
| `let` / assignment / compound assignment (`+=` and friends) | ✅ |
| Control flow (`if` / `else` / `while` / `loop` / `break` / `continue` / early `return`) | ✅ |
| Strings / arrays (dynamic & fixed-length) | ✅ |
| `class` / `new` / `init` / `this` / `deinit` (JS-style) | ✅ |
| Inheritance (`extends` / `super`) + virtual dispatch | ✅ |
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
| Cranelift JIT (`ilang run --jit`) | ✅ |
| FFI (`@extern(C) {}` blocks calling C libraries) | ✅ |
| Capability annotations | parse-only |

No ownership / `mut` / borrow checker — every variable is
reassignable. Errors are emitted in the uniform
`filename [row:col]: message` format.

## Setup

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

## Usage

```sh
# REPL — let / fn persist across lines, interpreter mode
cargo run -p ilang-cli

# Run a file (`;` is optional, newlines act as statement separators
# JS-ASI style)
cargo run -p ilang-cli -- run path/to/script.il

# Run via the JIT (Cranelift native code — tens to hundreds of times
# faster than the interpreter)
cargo run -p ilang-cli -- run --jit path/to/script.il
```

Code that calls into a C library through `@extern(C) {}` (the SDL2
sample below, for example) is **JIT-only**. Symbols are resolved
through dlsym at JIT-build time, so the interpreter has no path to
those functions.

### Sample: count multiples of 3 or 5 from 1 to 100

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

### Classes

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

### Sample: an SDL2 game window

`examples/sdl_bouncing_ball/` contains an SDL2 demo: a ball bounces
around the window, beeping every time it hits a wall. The arrow
keys (or `A` / `D`) move a paddle along the bottom; `ESC` exits
early.

Install SDL2 first:

```sh
# macOS (Homebrew)
brew install sdl2

# Debian/Ubuntu
sudo apt install libsdl2-dev libsdl2-2.0-0
```

Run:

```sh
cargo run -p ilang-cli -- run --jit examples/sdl_bouncing_ball/main.il
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

## Development

```sh
cargo test --workspace
```

## License

MIT OR Apache-2.0
