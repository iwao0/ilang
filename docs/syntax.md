# ilang syntax cheatsheet

English | [日本語](syntax_ja.md)

A list of every syntactic construct that's actually implemented and
compilable. Anything not on this page is either unimplemented or
buggy.

`.il` files run with `cargo run -p ilang-cli -- run path.il`
(tree-walking interpreter) or `... run --jit path.il` (Cranelift
JIT). Launching `ilang` with no arguments drops into the REPL.
Trailing semicolons are optional — newlines act as statement
separators (JS-style ASI).

---

## Reserved words

```
as       break    class    const    continue elif     else     enum
extends  false    fn       for      if       in       is       let
loop     match    new      none     override return   some     super
this     true     use      while
```

These tokens are reserved and cannot be used as variable / parameter
/ field / function / class names.

**Carve-outs** — the following reserved words *can* be used as enum
variant names (declaration, `Enum.<name>` access, and short / qualified
match patterns):

```
as       class    enum     extends  false    fn       in       none
override return   some     super    this     true
```

This is a practical concession for binding to C enums whose members
happen to collide (e.g. `SDL_HINT_OVERRIDE`, `SDL_FLIP_NONE`,
`SDL_FALSE` / `SDL_TRUE`, `SDL_SCANCODE_RETURN`).

**Contextual keywords** — only special inside specific positions,
otherwise plain identifiers:

| Word | Where it's a keyword |
| --- | --- |
| `static` | class-body member modifier |
| `get` / `set` | property accessor declarations inside a class |
| `weak` | type-position suffix `ClassName.weak` |

**Reserved identifiers** — usable as variant / field names but
shadowing them at top-level is rejected:

- `console` — built-in singleton.
- `Result` — built-in two-variant enum.

## 1. Literals

| Kind | Examples | Natural type |
| --- | --- | --- |
| Integer | `42`, `-7`, `0xff`, `0o755`, `0b1011`, `1_000_000` | `i64` |
| Integer (typed suffix) | `1_i32`, `255_u8`, `0xffff_u16` | the suffix's type |
| Float | `3.14`, `1.5e10`, `2.5_f32` | the suffix's type if present, otherwise `f64` |
| bool | `true`, `false` | `bool` |
| String | `"hello"`, `"line\nbreak"` (`\n` `\t` `\r` `\\` `\"` `\0`) | `string` |
| Unit | `()` (produced by expressions, not written by hand) | `()` |
| Optional | `none`, `some(x)` | `T?` |
| Array | `[1, 2, 3]`, `[1, 2, 3,]` (trailing comma allowed) | `T[]` |
| Tuple | `(1, "hello")`, `(true, 3.14, [1,2])` | `(T1, T2, ...)` (≥ 2 elements) |
| Map | `{"a": 1, "b": 2}` | `Map<K, V>` |

---

## 2. Types

```text
i8  i16  i32  i64
u8  u16  u32  u64
f32  f64
bool  string
()                  // Unit (return types etc.)
ClassName           // class instance
T[]                 // dynamic array (push-able)
T[N]                // fixed-length array
(T1, T2, ...)       // tuple (≥ 2 elements; `(T)` is grouping)
T?                  // Optional (none or some(t))
ClassName.weak      // weak reference (Object only)
ClassName<T1, T2>   // generic-class instantiation
Map<K, V>           // built-in dictionary (K = string / integer / bool)
fn(T1, T2): R       // function value (no captures)
```

The postfix modifiers `[]` `[N]` `?` `.weak` stack: `Foo[]?`,
`User?[]`, `Node.weak?`, etc. `.weak` only attaches to
`ClassName.weak` (you can't write `string.weak` or `i64.weak`).

### Implicit conversions

| from → to | Implicit? |
| --- | --- |
| Same-signed integer ↔ same-signed integer (widening or narrowing) | yes |
| Integer → float | yes |
| `f32` ↔ `f64` | yes |
| Sign-crossing (`i32` ↔ `u32` etc.) | **no** (`as` required) |
| Float → integer | **no** (`as` required) |
| `T` → `T?` (Optional auto-wrap) | yes |
| `Foo` → `Foo.weak` (strong → weak auto-downgrade) | yes (same class only) |

`expr as Type` performs an explicit cast. `if`/`else` arm joins
don't allow implicit numeric widening (integer literals are the
only thing coerced to the other arm's type).

---

## 3. Variables

```rust
let x = 1                  // type inferred
let y: f64 = 1             // annotated (integer → f64 implicitly)
let s: string = "hi"
let xs: i32[] = [1, 2, 3]
let maybe: User? = some(u) // Optional auto-wraps `T → T?`
let w: User.weak = u       // strong → weak auto-downgrade
```

- No `mut` keyword. Every `let` is reassignable.
- Same-name `let` in an inner scope shadows the outer (the outer
  value reappears when the scope ends).
- Empty array literal `[]` needs a type annotation
  (`let a: i32[] = []`).
- Tuple element access uses array-style `t[N]`, but `N` must be
  a **compile-time non-negative integer literal** (each slot has
  its own type). Tuple element assignment isn't supported.

```rust
x = x + 1                  // plain assignment
x += 1                     // compound: += -= *= /= %= &= |= ^= <<= >>=
obj.field = v
arr[i] = v                 // array index assignment
map[k] = v                 // Map index assignment
this.field = v             // inside a method
```

### Destructuring `let`

```rust
// Tuple — flat, `_` ignores a slot.
let pair: (i64, string) = (42, "hi")
let (n, s) = pair                       // n: i64, s: string
let (_, only_b, _) = (1, 2, 3)          // ignore the others

// Object (struct) — Rust-style with the class name. Field names
// must match; rename and rest are not supported in v1.
class Point { x: f64; y: f64; init(a: f64, b: f64) { this.x = a; this.y = b } }
let p = new Point(1.0, 2.0)
let Point { x, y } = p                  // x: f64, y: f64
```

- Only `let` statements support destructuring (function params and
  `for-in` patterns are not yet covered).
- Tuple destructure needs at least two slots (use `let pair = ...`
  for a single binding).
- Nested destructuring (`let ((a, b), c) = ...`) is rejected for now.

---

## 4. Operators (low → high precedence)

| Prec | Operators | Assoc |
| --- | --- | --- |
| 1 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` | right |
| 2 | `\|\|` | left (short-circuit) |
| 3 | `&&` | left (short-circuit) |
| 4 | `\|` | left |
| 5 | `^` | left |
| 6 | `&` | left |
| 7 | `==` `!=` | left |
| 8 | `<` `<=` `>` `>=` | left |
| 9 | `<<` `>>` | left |
| 10 | `+` `-` | left |
| 11 | `*` `/` `%` | left |
| 12 | `as` (cast, postfix) | — |
| 13 | unary `-` `+` `!` `~` | prefix |
| 14 | `.` (field/method) / `[]` (index) / `(...)` (call) | postfix |

For strings, only `+` (concatenation) and `==`/`!=` (structural
equality) are defined. Object `==`/`!=` is reference equality on
the same class. `%` is unsupported on floats.

### Built-in string methods

```rust
"hello".length              // i64 — Unicode code-point count ("あいう".length == 3)
"hello".charAt(1)           // string — single character. Out of range → ""
"hello".includes("ell")     // bool
"hello".startsWith("he")    // bool
"hello".endsWith("lo")      // bool
"Hi".toUpper()              // string
"Hi".toLower()              // string
"  hi  ".trim()             // string
"a,b,c".split(",")          // string[]  ─ empty separator splits per character
"abca".replace("a", "_")    // string    ─ replaces every match (Rust-style)
"hello".slice(1, 4)         // string    ─ indices are Unicode code points; out-of-range clamps
```

String interpolation isn't implemented yet. Every method above
works in both interpreter and JIT.

### Built-in `.toString()` on numerics and `bool`

```rust
(42).toString()             // "42"
(-7).toString()             // "-7"
(true).toString()           // "true"
(3.14).toString()           // "3.14"
(1.0).toString()            // "1.0"  — JS-style trailing `.0` for integral floats
let n: u8 = 255
n.toString()                // "255"
```

Available on every numeric primitive (`i8`..`u64`, `f32`, `f64`)
plus `bool`. Float formatting matches `console.log` (integral
values print as `N.0`).

---

## 5. Control flow

```rust
// if is an expression
let r = if n > 0 { n } else { -n }
if cond { ... } elif cond2 { ... } else { ... }   // `elif`, not `else if`

// while
while cond { ... }

// loop is exited only via `break`
let i = 0
loop {
    if i >= 10 { break }
    if i % 2 == 0 { i += 1; continue }
    i += 1
}

// for-in (over arrays or ranges)
let xs: i64[] = [10, 20, 30]
for x in xs { console.log(x) }     // break / continue allowed

// ranges (Rust-style) — exclusive `..` and inclusive `..=`
for i in 1..5 { console.log(i) }   // 1, 2, 3, 4
for i in 1..=5 { console.log(i) }  // 1, 2, 3, 4, 5
// open-ended `1..` (RangeFrom) — body must `break` to exit
for i in 1.. { if i > 100 { break }; sum += i }

// if let — pattern match on Optional (the only pattern form
// outside `match`)
let x: i64? = some(42)
if let some(v) = x {
    // v: i64 here
} else {
    // none branch
}
```

`break` / `continue` only work inside loops (the type checker
rejects them outside).

Range expressions `a..b` / `a..=b` / `a..` are valid **only as the
iterator of a `for-in`**. Trying to bind a range to a value
(`let r = 1..10`) is a type error. The bounded forms require both
endpoints to be the same integer type; the loop variable is bound
to that type. The half-open `a..` (RangeFrom) form has no upper
bound — the body must `break` to exit (running to integer
overflow wraps without panic). Mirroring Rust, the start-less
forms (`..N`, `..`) are **not** iterable and are rejected.

`loop` can break with a value (`break v`); the value becomes the
type and value of the `loop` expression itself (Rust-style).
`while` and `for` can complete normally without hitting a `break`,
so `break v` isn't allowed in them (the type checker rejects it).

```rust
let n = loop {
    if ready() { break compute() }     // loop has the break's i64 type
}

let i = 0
let first_even = loop {
    if i % 2 == 0 && i > 0 { break i }
    i = i + 1
}

loop { break }                          // value-less break — loop type is Unit
```

- All `break v` sites in the same `loop` must agree on the type
  (the type checker rejects mismatches).
- `break` (no value) is always allowed; `break v` is `loop`-only.
- Both interpreter and JIT support the above.

```rust
// return — early exit. Allowed at top level too (no value at top
// level — the program's value comes from its tail expression).
fn abs(n: i64): i64 {
    if n < 0 { return -n }
    n
}
fn maybe_bump(c: Counter, n: i64) {
    if n < 0 { return }   // value-less return from a Unit fn
    c.bump()
}

// At program top level:
let rc = init()
if rc < 0 {
    console.log("init failed")
    return                // exits the program
}
```

The trailing expression still serves as the return value; an
explicit `return` is optional.

---

## 6. Functions

```rust
// Return types use `: T` (TS-style)
fn add(a: i64, b: i64): i64 {
    a + b                  // tail expression is the return value
}

fn greet(name: string) {   // omitted return type = ()
    console.log("hi,", name)
}

fn factorial(n: i64): i64 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}
```

- Parameter types are mandatory.
- Generics are supported (`fn name<T, U>(...)`) — see
  [Generic functions](#generic-functions).
- Variadics are only supported on built-ins (`console.log`).
- **Default arguments**: `fn open(path: string, mode: string = "r")`
  attaches a default expression to a trailing parameter; callers
  can omit it. The default expression is re-evaluated at every
  call. A required parameter cannot follow one with a default.
  Defaults coexist with overloading — a candidate whose arity
  matches exactly always wins (default-filled candidates carry a
  +1000 score penalty).

### Generic functions

Like classes and enums, you can declare type parameters with
`<T, U>`. Arguments are inferred from the call site (no explicit
type-argument syntax).

```rust
fn id<T>(x: T): T { x }
fn first<T>(xs: T[]): T { xs[0] }

id(42)            // T = i64
id("hello")       // T = string
first([1, 2, 3])  // T = i64
```

- Inference walks arguments left-to-right and adopts the first
  binding that pins each type variable (same approach as enum
  constructors).
- Type variables in the return type are substituted with the
  inferred bindings.
- Both interpreter and JIT support generics. The JIT
  monomorphises each `(function, type-args)` pair into a
  separate concrete function.

### Function overloading

You can declare multiple functions with the same name and
different parameter types/arities. Each call site picks the best
overload from the argument types.

```rust
fn show(n: i64): string { "int" }
fn show(s: string): string { "str" }
fn show(b: bool): string { "bool" }

show(42)        // "int"
show("hi")      // "str"
show(true)      // "bool"

// Different arities are also fine
fn make(): string { "default" }
fn make(s: string): string { s }
fn make(s: string, suffix: string): string { s + suffix }
```

**Picking rule (best-match scoring)**: among the candidates that
each accept the call's arguments under implicit conversion, pick
the lowest-scoring one.
- Exact match = 0
- Same-signed integer widening (`i32 → i64` etc.) = 1
- `f32 ↔ f64` = 1
- Integer → float = 2
- Literal-fitting narrowing = 2
- `T → T?` (auto-wrap) = 3
- `Object → Weak` = 4

If multiple overloads tie for best score, the call is rejected
as **ambiguous**. Exact matches always win, matching the usual
"the explicit version is selected" intuition.

**Disallowed combinations**:
- A generic function and a non-generic function with the same name
  (`fn id<T>(x: T): T` together with `fn id(x: i64): i64`) — keeps
  generic resolution and overload resolution distinct.
- Two declarations with identical signatures.

**First-class references**: referring to an overloaded name with
`let f = name` is an ambiguous error. Either call directly with
arguments, or use the post-mangled name like `fn name__i64`
(internal-implementation, not recommended).

Both interpreter and JIT support overloading. After type
checking, overloaded names are mangled to `name__<param_types>`
and call sites are rewritten to match.

### First-class functions

Functions are values: assignable to variables, passable as
arguments, returnable. Anonymous-function bodies can **capture
outer locals by value** (interpreter / JIT both support every
capture type).

```rust
fn add(a: i64, b: i64): i64 { a + b }
let f = add                          // assign function value (type fn(i64, i64): i64)
f(2, 3)                              // 5

// Anonymous (immediate) function — drop the name from `fn`
let inc = fn(x: i64): i64 { x + 1 }
inc(41)                              // 42

// Closure — captures outer locals by value
let factor = 10
let scale = fn(x: i64): i64 { x * factor }
scale(3)                             // 30

// Returning a function gives you closure-of-closure
fn make_adder(n: i64): fn(i64): i64 {
    fn(x: i64): i64 { x + n }
}
let add5 = make_adder(5)
add5(3)                              // 8

// Take and return functions
fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
fn double(n: i64): i64 { n * 2 }
apply(double, 7)                     // 14
```

- Function type: `fn(T1, T2): R` (`: R` may be omitted when the
  return is `()`).
- A local `let f = some_fn` shadows a top-level `fn` of the same
  name.
- **Captures are value snapshots**: the closure retains (ARC) or
  copies (primitive) the outer-variable values at the moment it's
  built. Later mutation of the outer variable doesn't bleed into
  the closure (Rust's `move` closure equivalent).
- Parameters with the same name as a capture shadow the capture.
- Top-level functions/classes with the same name are *not*
  captured — they resolve as globals.
- **Both interpreter and JIT** support every capture type
  (i64 / f64 / bool / string / object / array / optional / map).
  The JIT lays a closure out as a heap struct
  `[fn_ptr | env_field0 | ...]` managed by ARC (heap captures get
  retained, the env releases its captures when the closure is
  dropped).
- Nested closures (closure-of-closure) are supported — the inner
  closure may re-capture from the outer's captured environment.
- Referring to a top-level function as a function value
  (`let f = some_fn`) auto-wraps it in a trampoline closure that
  ignores the env slot and forwards to the target.

### Attributes / annotations (parse-only unless noted)

```rust
@requires(net)
fn fetch(url: string): string { ... }

@requires(net, file)
@deprecated(use_v2)
fn download(url: string, path: string) { ... }
```

`@name(args)` form (TS / Java / Python decorator-style). Multiple
attributes each start with their own `@`. The argument list is not
optional (`@x` without parens is a parse error). Attributes attach
to methods too, but not to the class itself.

The attributes that actually carry meaning today are `@override`
(inheritance) and the FFI ones inside `@extern(C) { ... }`:
`@lib`, `@optional`, `@symbol`, `@packed`, `@bits(N)`. Everything
else is parsed and ignored.

---

## 7. Classes

```rust
class Counter {
    count: i64                          // field
    init(start: i64) { this.count = start }
    bump(): i64 {
        count += 1                      // implicit `this.` (field/method)
        count
    }
    deinit() { ... }                    // runs on scope exit (optional)
}

let c = new Counter(10)
c.bump()                                // method call
c.count                                 // field read
```

- `init` is the (only) constructor (Swift-style). Omit it and the
  class can be `new`-ed without arguments.
- `deinit` is parameter-less and returns `()`. Calling it
  explicitly (`c.deinit()`) is an error.
- Implicit `this`: in method bodies you can drop `this.` for
  fields / methods. A local variable or parameter with the same
  name still wins.
- Inheritance (`extends`) / `static` / `get`-`set` properties are
  detailed below. There's no `private` modifier.
- Multiple class members on the same line aren't allowed (ASI
  doesn't fire — you need a newline or `;`).

#### Field defaults / required init assignment

Fields whose runtime representation has a usable blank value are
auto-zero-initialised at `new` and don't need to appear in
`init`:

| Field type | Default |
| --- | --- |
| `i8`..`u64`, `f32`, `f64` | `0` |
| `bool` | `false` |
| `string` | `""` |
| `T?` | `none` |
| `T[]` (dynamic) | `[]` |
| `T[N]` (fixed) | element-wise default |
| `T.weak` | dead weak |

Other heap fields (`Object` references, `Map<K, V>`, function
values, tuples) have no safe blank — every `init` overload **must
assign** them, otherwise the class declaration fails to type-check
with a clear error. Wrapping such a field in `T?` opts in to a
`none` default.

### Generic classes

```rust
class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}

class Pair<A, B> {
    a: A
    b: B
    init(x: A, y: B) { this.a = x; this.b = y }
}

let b = new Box<i64>(42)            // type arguments are mandatory at construction
let p = new Pair<string, i64>("k", 1)
let nested = new Box<Box<i64>>(new Box<i64>(99))   // nested OK (>> is auto-split)
```

- Type arguments **must be specified at instantiation** — there
  is no inference for `new Box<i64>(42)`.
- Bounds aren't supported (any type goes).
- **JIT-supported**. Each `(class, type-args)` pair monomorphises
  into a separate generated class.
- Function generics are documented in
  [§6 Generic functions](#generic-functions).
- Operations between type variables (e.g. `class Pair<A, B> { ... a + b ... }`)
  are rejected by the type checker since there are no constraints.

### Method / `init` overloading

You can declare multiple methods with the same name and different
parameter types/arities. The same applies to `init` — the best
match is selected at `new C(...)` based on the arguments. The
scoring and ambiguity rules match
[§6 Function overloading](#function-overloading).

```rust
class Greeter {
    init() {}
    init(name: string) { this.name = name }     // init overload OK
    name: string
    greet(): string { "hi" }
    greet(n: i64): string { "hi x" + (n as string) }   // method overload OK
}

let a = new Greeter()
let b = new Greeter("ada")
b.greet()                                       // → "hi"
b.greet(3)                                      // → "hi x3"
```

- **`deinit` cannot be overloaded** — the runtime always calls it
  with zero arguments, so multiple declarations are rejected.
- **Methods on generic classes cannot be overloaded** — mixing
  monomorphisation with overload resolution is disallowed
  (`class Box<T> { f(x: i64): ...  f(x: string): ... }` errors).
- Both interpreter and JIT support method/init overloading.
  Overloaded names are mangled to `name__<param_types>` after
  type checking, and `new C(...)` AST nodes record the chosen
  `init_method`.

### `get` / `set` properties

`get name(): T { ... }` and `set name(v: T) { ... }` define
computed properties. Callers use them like fields: `obj.name` to
read, `obj.name = v` to write. Backing storage lives in a separate
field if needed.

```rust
class Temp {
    celsius: f64
    init(c: f64) { this.celsius = c }
    get fahrenheit(): f64 { this.celsius * 9.0 / 5.0 + 32.0 }
    set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
}

let t = new Temp(0.0)
t.fahrenheit              // 32.0  (calls the getter)
t.fahrenheit = 100.0      // calls the setter
t.celsius                 // 37.77...
```

- The getter takes no arguments and must declare a return type;
  the setter takes one argument and returns nothing. Enforced by
  the type checker.
- A getter alone (read-only) or setter alone (write-only) is
  fine; the missing direction errors at use sites ("no setter" /
  "no getter").
- The getter's return type must equal the setter's parameter type.
- Property names can't collide with field or method names.
- `get` / `set` are contextual keywords — only special inside a
  class body, regular identifiers everywhere else.
- Both interpreter and JIT support properties.

### `static` methods

`static` makes a method **class-level**, callable without an
instance via `ClassName.method(args)`. `this` is unavailable in
the body (the type checker rejects it).

```rust
class Vec2 {
    x: f64; y: f64
    init(x: f64, y: f64) { this.x = x; this.y = y }

    static zero(): Vec2 { new Vec2(0.0, 0.0) }
    static of(x: f64, y: f64): Vec2 { new Vec2(x, y) }
    static dot(a: Vec2, b: Vec2): f64 { a.x * b.x + a.y * b.y }
}

let z = Vec2.zero()
let p = Vec2.of(3.0, 4.0)
let d = Vec2.dot(z, p)
```

- Not overloadable (multiple `static foo` errors).
- Cannot share a name with a field, instance method, or property.
- Static methods on generic classes are unsupported (type
  parameters aren't visible in static context).
- `static` is a contextual keyword (only inside a class body).
- A local `let Vec2 = ...` shadows the class name (so `static`
  dispatch resolves to the binding, not the class).
- Both interpreter and JIT support static methods.

#### `static` fields and `const` constants

`static name: T = const_expr` declares class-level mutable
storage shared by all instances. `const name: T = const_expr`
declares the same storage as immutable — reassignment is a
compile-time error. Read either through `ClassName.field`.

```rust
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1; Counter.total = Counter.total + 1 }

    static total: i64 = 0
    static threshold: i64 = 1 + 2 * 5      // 11 (const folding)
    const max: i64 = 1000                  // immutable; Counter.max = ... is rejected
}

let a = new Counter(); let b = new Counter()
a.bump(); a.bump(); b.bump()
Counter.total              // 3
Counter.max                // 1000
```

- Type is restricted to **`i64` / `f64` / `bool`** for now. String
  / object / other heap types await a settled ARC design.
- The initialiser must be a **compile-time constant expression**
  (the same folder used for top-level `const`); runtime expressions
  (calls etc.) are rejected.
- `static` is mutable (`Counter.total = 100` is allowed).
- `const` is immutable — `Counter.max = 100` is a type error.
- Names must not collide with fields, methods, properties, or
  other static members.
- Static fields on generic classes are unsupported (same reason
  as static methods).
- Implementation: the JIT allocates a `Box<[i64]>` and assigns
  slots; access is a load/store at an absolute address with
  bitcast/truncate for f64/bool.

### Inheritance (`extends`)

`class Child extends Parent { ... }` for single inheritance with
virtual dispatch + `override` + `super`. Both interpreter and JIT
support it.

```rust
class Animal {
    name: string
    init(n: string) { this.name = n }
    speak(): string { "generic sound" }
    describe(): string { this.name + " says " + this.speak() }
}

class Dog extends Animal {
    init(n: string) { super(n) }              // call parent init
    override speak(): string { "woof" }       // override required
}

let d = new Dog("rex")
d.speak()                                      // "woof"
d.describe()                                   // "rex says woof" — the speak()
                                               // call inside Animal.describe
                                               // dispatches to Dog.speak (virtual)

fn introduce(a: Animal): string { a.describe() }
introduce(d)                                   // OK — Dog is-a Animal (subtyping)
```

- Single inheritance only.
- The parent must already be declared (no forward references).
- `override` is mandatory: it's an error if there's no parent
  method with the same name, and an error if there is one and you
  forget the keyword (which would silently hide it).
- The override's signature must match the parent's exactly.
- `super.method(args)` calls the parent's version (statically
  resolved up the chain).
- `super(args)` inside a child's `init` calls the parent's init.
- Field inheritance: parent fields come first, child additions
  follow.
- Method overloading isn't supported across the inheritance
  hierarchy (only on the root class).
- `init` and `deinit` are per-class (not subject to override
  rules).
- Inheriting static members isn't supported.
- Inheritance on generic classes isn't supported.
- Subtyping: a `Child` value can flow into any binding /
  argument / return slot typed `Parent`.
- JIT: object headers gain a vtable pointer
  (`[strong | weak | drop_fn | vtable | fields...]`, 32 byte
  header) and each class allocates a `Box<[i64]>` vtable. Virtual
  calls become `obj.vtable[slot]` load → `call_indirect`;
  `super.method` is a direct call to the parent's specific
  function.

---

## 8. Arrays

```rust
let xs: i32[] = [10, 20, 30]    // dynamic-array literal
let ys: i32[3] = [1, 2, 3]      // fixed-length (length is part of the type)
let zs: i32[] = []              // empty needs annotation
let trailing = [1, 2, 3,]       // trailing comma allowed

xs[1]                            // index read
xs[0] = 100                      // index write
xs.length                        // returns i64 (built-in)
xs.push(40)                      // dynamic only; fixed-length errors
xs.pop()                         // returns T? (none if empty); dynamic only
xs.indexOf(20)                   // i64 (-1 if missing)
xs.includes(20)                  // bool
```

Higher-order methods: `xs.map(fn)` / `xs.filter(pred)` /
`xs.forEach(fn)` / `xs.slice(start, end)`. Callbacks may be
**first-class functions** or **closures** (anonymous `fn` capturing
outer locals by value — see §6). `length` / `push` / `pop` /
`indexOf` / `includes` / `for-in` and the higher-order methods all
work in **both interpreter and JIT** with no element-type
restrictions.

---

## 9. Maps

```rust
let m: Map<string, i64> = {"a": 1, "b": 2}        // literal
let empty: Map<string, i64> = new Map<string, i64>()  // empty map

m["c"] = 3                       // write
m["a"]                           // read (missing key panics at runtime)
m.get("nope")                    // V? (none for missing — safe read)
m.has("a")                       // bool
m.delete("a")                    // bool (whether the key existed)
m.set("d", 4)                    // same as m["d"] = 4
m.size()                         // i64
m.keys()                         // K[]
m.values()                       // V[]
```

- Key types: `string` / `i*` / `u*` / `bool`. Floats and objects
  are rejected (Eq / Hash consistency).
- `K` is inferred from the first key, `V` from the first value in
  the literal.
- Empty maps need `new Map<K, V>()` — `{}` is parsed as an empty
  block.
- The parser distinguishes map literals from blocks by looking
  two tokens ahead: `{<key-token> :` (Ident/Str/Int/Bool followed
  by `:`) is a map.
- Both interpreter and JIT support maps (literals, basic ops,
  `get` / `keys` / `values`).

---

## 10. Optional

```rust
let a: User? = some(user)        // build via `some`
let b: i64? = none               // absent
let c: i64? = 7                  // T → T? auto-wrap

if let some(v) = a {             // pattern match
    use(v)
}

a.isSome                       // bool
a.isNone                       // bool
a.unwrap()                       // T (panics at runtime if none)
```

- Any type works as `T`. Both interpreter and JIT handle
  Optionals (the JIT represents `T?` of a primitive as a heap box
  `[rc:i64 | payload:T]`).
- `T?` is valid for parameters / return types / fields.
- `none` on its own has no type — it's inferred from the
  surrounding Optional context.

---

## 11. enum / match

```rust
// Phase 1 — plain C-style enum (lowercase variant names recommended
// to match the built-in Result)
enum Color { red, green, blue }

let c = Color.green
// match patterns can omit the `Enum.` prefix — inferred from the
// scrutinee type
let name = match c {
    red { "red" }
    green { "green" }
    blue { "blue" }
}

// Phase 2 — payloaded variants (tuple / named-field)
enum Shape {
    circle: (f64)              // tuple payload via `: (...)`
    rect: (f64, f64)
    square: { side: f64 }      // struct payload via `: { ... }`
}

fn area(s: Shape): f64 {
    match s {
        circle(r) { 3.14 * r * r }
        rect(w, h) { w * h }
        square { side } { side * side }   // struct shorthand: { side: side }
    }
}

// `_` wildcard for the rest. The fully qualified `Color.red` form
// is still accepted.
let day = Color.red
match day {
    red { "alert" }
    _ { "ok" }
}
```

- **Enum declaration**: payloaded variants name then payload type
  separated by `:` (`circle: (f64)`). Unit variants drop the
  colon (`red`).
- **Variant casing**: any case is syntactically OK, but lowercase
  is recommended to match the built-in `Result.ok` / `Result.err`.
- **Keyword-named variants**: `override`, `class`, and `none` can
  be used as variant names (and accessed as `Enum.override` etc.)
  even though they're reserved elsewhere. Useful when binding to C
  enums whose members happen to collide with ilang keywords (e.g.
  `SDL_HINT_OVERRIDE`, `SDL_FLIP_NONE`). `static` was never
  reserved, so it works without any special handling.
- **Match arms**: no `=>`, just write `{ body }` after the
  pattern (`Color.red { "red" }`).
- Construction needs the `Enum.` prefix (`Shape.circle(3.0)`).
- **Match patterns may omit `Enum.`** — inferred from the
  scrutinee's static type. The fully qualified form
  (`Shape.circle(r)`) still works.
- Coverage must be exhaustive or include `_` (the type checker
  enforces this).
- Each arm's value must be type-compatible (same rule as
  `if`/`else`).
- Pattern bindings: tuple uses positions (`Shape.circle(r)`),
  struct uses names (`{ side }` or `{ side: s }`), `_` discards.
- Heap types inside a payload (Object / Str / Array / Optional /
  Weak / nested enum) are released correctly by ARC.

### Matching on primitives

`match` also works on **integer / bool / string** scrutinees with
literal patterns:

```rust
let label = match n {
    1 { "one" }
    2 { "two" }
    -1 { "neg" }
    _  { "other" }
}

// Integer ranges — exclusive `..`, inclusive `..=`, and the
// half-open forms `..N`, `..=N`, `N..` (Rust-style).
let bucket = match n {
    ..0     { "neg" }
    0..10   { "small" }
    10..=99 { "tens" }
    100..   { "big" }
    _       { "?" }
}

let s = match flag {
    true  { "on" }
    false { "off" }
}

let kind = match name {
    "ok"   { 0 }
    "err"  { 1 }
    _      { -1 }
}
```

- Integer patterns (`1`, `-7`) match same-signed integer scrutinees
  via structural equality.
- Integer range patterns (`a..b`, `a..=b`, `a..`, `..b`, `..=b`)
  match when `x` falls in the range. Bounds are integer literals
  (optionally with a `-` sign); empty bounded ranges (`5..5`,
  `5..3`) are rejected at compile time. Half-open forms have no
  bound on the missing side. The `a..=` form (no upper bound,
  inclusive) is rejected — it makes no sense.
- `bool` patterns require both `true` and `false` arms (exhaustive)
  *or* a `_` wildcard.
- All other primitive matches need a `_` wildcard — the value space
  isn't enumerable.
- Float and tuple scrutinees are not supported (use `if`/`elif`).

### Value-tagged fieldless enums

Fieldless (unit-only) variants may be given an explicit integer
discriminant with `= <int>`. Variants without one continue with
`previous + 1` (default 0). An optional `: <numeric>` after the enum
name pins the underlying integer type — useful when the enum value
needs to be cast to a specific width.

```rust
enum Priority: u32 {
    low    = 1
    medium = 5
    high   = 10
}

let p: u32 = Priority.high as u32   // 10
```

- Discriminants are only allowed on unit variants.
- Without `: <type>`, the cast target chooses the result width
  (`Priority.high as i64`).
- Casting an enum value to any numeric primitive resolves to the
  variant's discriminant.
- Casting a numeric primitive to a fieldless enum (`x as MyEnum`)
  reinterprets the integer as a discriminant. Only allowed when the
  enum has no payloaded variants — payloaded enums have no integer
  representation. Lets C-side return values flow back into the typed
  enum (`SDL_GetKeyFromScancode(...) as Keycode`).

### `@flags` enums

Bitflag enums that support `|`, `&`, `^`, `~` between values. The
attribute is placed above the `enum` keyword. Without an explicit
`: <type>`, the underlying repr defaults to `u64` (matching the
default integer literal type).

```rust
@flags
enum InitFlag {
    timer = 0x01
    audio = 0x10
    video = 0x20
}

let combined = InitFlag.audio | InitFlag.video
combined.has(InitFlag.audio)        // true
combined.has(InitFlag.timer)        // false
let cleared = combined & ~InitFlag.audio
```

- Variants must be fieldless. Discriminants follow the same rules
  as the base form (explicit `= N`, otherwise `previous + 1`).
- Bitwise operands must be the same flags enum on both sides;
  mixing two different flag types still requires an explicit `as`.
- `value.has(other)` is a synthetic method equivalent to
  `(value & other) == other` — it handles multi-bit `other`.
- `match` is not supported on `@flags` enum values. Combined values
  don't correspond to a single named variant; use `has` (or
  bitwise compares) for control flow.
- The runtime representation is the underlying integer, so
  `combined as u32` / `combined as i64` give the raw bits.

### Generic enums

```rust
enum Either<L, R> {
    left: (L)
    right: (R)
}

let e: Either<i64, string> = Either.right("hi")
match e {
    left(_) { "left" }
    right(s) { s }
}
```

- `enum Name<T, U> { ... }` syntax matches generic classes.
- Variant constructors **infer** type arguments from arguments
  (`Either.right("hi")` → `Either<Any, string>`, then merged with
  any annotation).
- Parameters left as `Any` get pinned by `let` annotations or
  function return types.
- Match-side bindings recover their concrete type from the
  scrutinee.
- Both interpreter and JIT support generic enums (the JIT
  generates per-instantiation EnumDecls and per-instantiation
  layouts).

### Built-in `Result<T, E>`

A Rust-style built-in generic enum. Variant names are **lowercase
`ok` / `err`**.

```rust
enum Result<T, E> { ok: (T), err: (E) }   // conceptual — registered internally

fn divide(a: i64, b: i64): Result<i64, string> {
    if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
}

match divide(10, 2) {
    ok(v) { v }            // patterns can omit `Result.` (inferred from scrutinee)
    err(_) { -1 }
}

let r = divide(10, 2)
r.isOk                     // bool — true when the variant is `ok`
r.isErr                    // bool — true when the variant is `err`
```

- Build with `Result.ok(v)` / `Result.err(e)` (the usual
  `Enum.variant(...)` form).
- `r.isOk` / `r.isErr` are **properties** (no parentheses) returning
  `bool`. Mirror Optional's `isSome` / `isNone`.
- Match patterns can shorten to `ok(v)` / `err(e)` (the
  variant-shorthand mechanism).
- `ok` / `err` are **not reserved words** — usable as variable
  names (though confusing).
- The name `Result` is reserved (defining `enum Result { ... }`
  is an error).
- Type arguments are inferred at construction; T/E are pinned by
  return types or annotations.
- Match exhaustiveness still applies (cover `ok` and `err`, or
  use `_`).
- Both interpreter and JIT support `Result` (monomorphised per
  `(T, E)`).

---

## 12. Weak references

```rust
class Node {
    parent: Node.weak           // breaks cycles
    init(p: Node) { this.parent = p }
}

let root = new Node(...)
let w: Node.weak = root         // strong → weak auto-downgrade

if let some(n) = w.get() {      // .get() returns T? (Some when alive)
    n.method()
} else {
    // already freed
}
```

- `.weak` only attaches to **class types**. `string.weak` /
  `i64.weak` are type errors.
- A weak reference doesn't own its target — it doesn't bump the
  strong rc.
- `.get(): T?` returns Some when the target is still alive, None
  otherwise.
- Main use case is **cycle breaking**: in a `Parent ↔ Child`
  ownership graph, making the child→parent back-edge `.weak` lets
  the parent's `deinit` actually run.
- The JIT uses a dual-rc (strong + weak) layout.

---

## 13. console (built-in)

```rust
console.log(1, "hello", true)        // variadic, space-separated, trailing newline
console.log()                        // newline only
console.log(arr, obj, opt)           // arrays / objects / optionals are formatted
```

- `console` is a reserved identifier; user `let console = ...` or
  classes named `Console` error.
- Argument types may mix freely.

---

## 13a. RTTI: `typeof` and `Type`

Every value can be inspected at runtime via the `typeof(x): Type`
built-in. Returns a `Type` handle exposing the value's *dynamic*
type (a `Parent`-typed slot holding a `Child` reports `Child`).

```rust
class Animal { sound(): string { "?" } }
class Dog extends Animal { override sound(): string { "woof" } }

let a: Animal = new Dog()
typeof(a).name         // "Dog" (dynamic — not "Animal")
typeof(a).kind         // TypeKind.class

typeof(42).name        // "i64"
typeof("hi").name      // "string"
typeof(some(1)).name   // "optional"

let r: Result<i64, string> = Result.ok(1)
typeof(r).name         // "Result"  (type args surfaced separately
                       //  by `typeArgs()` in a later phase)
```

`Type` exposes:

| Property | Type | Description |
| --- | --- | --- |
| `.name` | `string` | User-facing type name (e.g. `"Dog"`, `"i64"`, `"Result"`) |
| `.kind` | `TypeKind` | One of `primitive`, `class`, `enum`, `optional`, `array`, `fn`, `tuple`, `string`, `unit` |
| `.parent` | `Type?` | Direct parent class for `extends`; `none` for non-class types or root classes |
| `.fields` | `string[]` | Names of declared fields (classes only; empty for other kinds). Inherited fields are NOT included — chase `.parent` for those |
| `.methods` | `string[]` | Names of declared methods (classes only; empty for other kinds). `init` is included |
| `.typeArgs` | `Type[]` | Generic-instance arguments (e.g. `[Type("i64"), Type("string")]` for `Result<i64, string>`). Empty for non-generic types. Interpreter and JIT both report the inferred args |

`Type` also exposes per-member type lookups (methods, not getters):

```rust
class Foo {
    name: string
    init(n: string) { this.name = n }
    greet(): string { "hi " + this.name }
}

let t = typeof(new Foo("x"))
t.fieldType("name")            // some(Type("string"))
t.fieldType("nope")             // none
t.methodReturn("greet")         // some(Type("string"))
t.methodParams("greet")         // some([])  — zero-arg method
t.methodParams("init")          // some([Type("string")])
t.methodReturn("nope")          // none
```

| Method | Return | Description |
| --- | --- | --- |
| `.fieldType(name: string)` | `Type?` | Declared type of the named field, or `none` if not a class / not declared |
| `.methodReturn(name: string)` | `Type?` | Declared return type of the named method, or `none` |
| `.methodParams(name: string)` | `Type[]?` | Parameter types of the named method (in order), or `none` |

### Type tests and downcasts

```rust
class Animal {}
class Dog extends Animal {}
let a: Animal = new Dog()    // (assuming subclass auto-coerces)

a is Dog        // bool — true (parent chain walked)
a is Animal     // bool — true
a is Cat        // bool — false (when Cat is unrelated)

let d: Dog? = a as? Dog      // some(d) on success
let c: Cat? = a as? Cat      // none on failure
```

`is T` and `as? T` walk the dynamic class's parent chain at
runtime. Currently `T` must be a **class** type.

`TypeKind` is a built-in unit enum and can be `match`ed normally:

```rust
let label = match typeof(x).kind {
    primitive { "num" }
    string { "text" }
    class { "obj" }
    _ { "other" }
}
```

- `Type` and `TypeKind` are reserved names — user code can't
  redefine them.
- Dynamic class dispatch goes through the vtable header, so RTTI
  works under inheritance for both interpreter and JIT.
- `.fields` / `.methods` currently expose only **declared**
  members (no inherited names). For per-member type info use
  `fieldType(name)` / `methodReturn(name)` / `methodParams(name)`.

---

## 13b. Modules (`use`)

Imports `fn` / `class` / `enum` items from another file. Rust-style
**same-directory resolution**: `use utils` reads the sibling
`utils.il`.

```rust
// utils.il
fn double(n: i64): i64 { n * 2 }
class Counter {
    n: i64
    init(start: i64) { this.n = start }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

// main.il
use utils                       // namespaced
use math { sqrt, pi }           // selective

let c = new utils.Counter(10)
c.bump()
utils.double(c.get())            // → 22
```

- **Two forms**:
  - `use module` — namespaced reference (`module.foo()`,
    `new module.Class()`, `module.Enum.variant`).
  - `use module { name1, name2 }` — selective import (used by
    bare name). Selective import follows `@export use` chains, so
    `use sdl { InitFlag }` resolves `InitFlag` even when it's
    declared in `sdl_core` and re-exported by the umbrella `sdl`
    module.
- All top-level items are **public** (no visibility keywords).
- Circular imports (`A → B → A`) are rejected as a DAG cycle.
- Loading the same module multiple times is a no-op (deduped by
  file path).
- All modules merge into one `Program`; the type checker doesn't
  see file boundaries.
- Items imported namespaced are internally tagged with
  `module.X`, so they don't collide with parent-program bare
  names.
- **Built-in modules**: a few modules ship inside the compiler
  and are preferred over disk lookup. Today these are `math`,
  `os`, `test`.

### `@export use` (re-export, umbrella modules)

`@export use other_module` inside a module re-exposes
`other_module`'s items under the *current* module's namespace.
Useful for umbrella files that bundle several small modules:

```ilang
// sdl.il (umbrella)
@export use sdl_core
@export use sdl_window
@export use sdl_renderer
@export use sdl_audio

// main.il
use sdl
sdl.init(sdl.INIT_VIDEO)        // from sdl_core
new sdl.Window(...)             // from sdl_window
ren.fillRect(...)               // method from sdl_renderer
```

Without `@export`, a nested `use sdl_window` inside `sdl.il` would
expose those items as `sdl_window.*` even when callers say
`use sdl`. The `@export` form re-prefixes them under `sdl.*`. At
the entry point (no parent module) `@export use` is a regular
nested `use`.

### `ilang.toml` project file

A Cargo-style project file lets bindings live outside any single
project's source tree.

```toml
[package]
name = "my_game"

[deps]
sdl2 = "/path/to/ilang/bindings/sdl2"
```

The CLI walks upward from the entry file looking for
`ilang.toml`. Each `[deps]` value (relative to the project file)
becomes an extra directory the loader checks during `use module`
resolution. Lookup order: importer's own directory, then each
declared dep directory.

### `@extern(C) { ... }` — FFI block

Every C-ABI declaration — calling C functions, declaring
C-compatible structs, accessing C globals — lives inside an
**`@extern(C) { ... }` block**. Raw pointers (`*T` / `*const T`)
and C-only types (`char` / `void` / `size_t` / `ssize_t`) are
nameable only inside the block, and the type system prevents
their values from leaking outside.

```rust
@extern(C) {
    @lib("c") fn strlen(s: *const char): size_t
    @lib("m") fn sqrt(x: f64): f64

    // Opaque handle: an empty struct used as a pointer type
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE
    @lib("c") fn fclose(stream: *FILE): i32

    // C-compat struct
    struct timespec {
        tv_sec: i64
        tv_nsec: i64
    }
    @lib("c") fn clock_gettime(clk: i32, tp: *timespec): i32
}
```

Top-level only. The only attribute the block accepts is
`@extern(C)` itself. **JIT-only** — the interpreter can't call
@lib functions inside the block (host-form bare functions are an
exception).

#### Items allowed inside the block

- **`fn declaration`** — external function call via dlsym /
  host registration
- **`fn definition { body }`** — an ilang body exposed under the C
  ABI (callback / wrapper)
- **`struct Name { fields }`** — C-compat struct
- **`union Name { fields }`** — C union (every field at offset 0)
- **`@packed struct Name { ... }`** — `__attribute__((packed))`
  equivalent (no padding, align=1)
- **`class Name { ... }`** — ARC-managed wrapper class with
  method bodies type-checked in the @extern(C) context

#### `fn` declarations: `@lib` / `@optional` / `@symbol` / variadics

```rust
@extern(C) {
    @lib("c") fn abs(x: i32): i32                         // libc::abs
    @lib("c", "m") fn fallback(x: f64): f64               // try libc, fall back to libm
    @lib("libssl.so.3") @optional fn SSL_new(): *void     // JIT keeps going if missing
    @lib("c") @symbol("snprintf")
        fn formatI64(buf: *u8, n: size_t, fmt: *const char, ...): i32
}
```

- **`@lib("name", "fallback", ...)`** — names of dynamic libraries
  to dlopen. Multiple names are tried in order; the first to open
  wins (covers soname differences). `@lib` is the canonical
  marker for a native call: anything in user-written FFI code
  declaring a function whose body lives in a shared library
  must carry it. Bare extern declarations without `@lib` are
  reserved for the **host-form** path (registered via
  `JITBuilder::symbol(...)`) — that's how the built-in `math` /
  `os` / `test` modules are wired and isn't a path user code
  needs.

  > A `@extern(C, "libname")` shorthand was once on the table but
  > was withdrawn — `@lib(...)` stays as the single way to bind
  > a native function.
- **`@optional`** — a missing library or symbol no longer fails
  JIT build; the function instead binds to a stub that aborts on
  call. Programs guard with `os.libLoaded(name): bool` before
  calling.
- **`@symbol("c_name")`** — separate the ilang-side name from the
  C symbol. Equivalent to C#'s `[DllImport(EntryPoint=...)]`.
  Useful when you want two different ilang signatures over the
  same C function, or to dodge keyword collisions.
- **Trailing `...`** — printf-style variadic. The fixed prefix is
  type-checked normally; trailing arguments flow through with
  their actual JIT types (matching format specifiers is the
  caller's responsibility). On Apple AArch64 the JIT pads the
  signature so variadic args spill to the stack (per ABI).

#### Library-name resolution

- **Bare name** (`"m"`, `"sqlite3"` — no `.` `/` `\`): completed
  per OS conventions. macOS = `lib{name}.dylib` / `{name}.dylib`;
  Linux tries `lib{name}.so`, then `…so.6`, …, `…so.0`;
  Windows = `{name}.dll` / `lib{name}.dll`.
- **Literal filename** (`"libc.dylib"` / `"libm.so.6"` /
  `"./build/foo.so"`): passed to `dlopen` as-is.
- Each library is dlopened once. `os.libLoaded(name)` always
  takes the **canonical (first-listed) name**.
- **`os.libLoadError(name): string`** retrieves the dlopen error
  message for diagnostics — guards still go through `libLoaded`.

#### C-compat struct (`struct Name { ... }`)

```rust
@extern(C) {
    struct timespec {
        tv_sec: i64
        tv_nsec: i64
    }
    @lib("c") fn clock_gettime(clk: i32, tp: *timespec): i32
}

let ts = new timespec()              // zero-initialised
clock_gettime(0 as i32, ts)          // Object → *T auto-coercion (like u8[] → *u8)
console.log(ts.tv_sec)
```

- Methods / `init` / inheritance / type parameters / properties
  are forbidden — fields only.
- Each field gets **natural alignment** (i64=8B, i32=4B, bool=1B)
  — matches C `struct` layout.
- `new ClassName()` **zero-initialises**.
- Field types may be **numeric primitives, bool, `string`, other
  `@extern(C)` structs, raw pointers, fixed-length numeric
  arrays**.
- An **empty struct** (`struct FILE {}`) acts as an **opaque
  handle** — the `*FILE` pointer is type-safe to pass around.
- **`string` field**: an 8-byte heap pointer (`StringRc *`) is
  stored. The physical layout is **not** `char *`; if the C ABI
  needs a `char *` member, either (a) pass `*const char`
  separately as a function argument, or (b) use a fixed-length
  `u8[N]` buffer.
- **Fixed-length numeric array fields** (`u8[8]` / `i64[3]` /
  `f64[2]` …): bytes are inlined. Element access `s.arr[i]` is
  bounds-checked.
- **Nested struct**: another `@extern(C)` struct as a field type
  inlines its bytes. Chain access (`outer.inner.x`) reads /
  writes through.
- **Aggregate literal**: `point { x: 1 as i32, y: 2 as i32 }`
  builds an instance (sugar for `new` plus consecutive
  assignments).
- Declaration order is free — the JIT topologically sorts
  dependency edges before finalising layouts (cycles error out).
- **C99 flexible array member**: a final field with `T[]` (no
  fixed length) is a FAM. `new ClassName(n)` extends the
  allocation by `n` elements; `obj.data[i]` accesses without
  bounds checking.

#### `@packed`, `@bits(N)`

```rust
@extern(C) {
    @packed struct PacketHeader {
        magic: u8
        length: u32        // packed: offset 1 (no padding)
        flags: u8
        code: u16
    }
    struct ModeFlags {
        @bits(3) read_perm: u32
        @bits(3) write_perm: u32
        @bits(3) exec_perm: u32
        // Consecutive bit-fields with the same underlying type pack
        // into one u32 storage unit
    }
}
```

- `@packed` — every field at `offset = sum of prior sizes` and
  the struct align is 1. Aimed at network / file-format headers.
- `@bits(N)` — declares a bit-field of width `N`. Consecutive
  bit-fields with the same underlying type pack into a shared
  storage unit (GCC-style). Constraints: **unsigned integers
  only** (u8/u16/u32/u64); `1 ≤ N ≤ underlying width`.

#### `union Name { ... }`

Every field shares offset 0. Size = `max(field_sizes)`,
align = `max(field_aligns)`.

- Use cases: `union sigval` / `siginfo_t`, integer ↔ float bit
  pattern conversion (type punning).
- Fields are restricted to **numeric primitives / bool /
  fixed-length numeric arrays** (heap types would break ARC).
- Bit-fields and FAM are forbidden; `@packed` cannot be combined.

#### Raw pointers + C-only types (block-only)

Inside the block you can write C types directly:

| ilang | C |
| --- | --- |
| `*T` | `T *` |
| `*const T` | `const T *` |
| `char` | `char` (i8) |
| `void` | `void` (return only; `*void` for `void *`) |
| `size_t` | `size_t` |
| `ssize_t` | `ssize_t` |

These types **cannot appear in expressions or annotations outside
the block**. Bridge to ordinary ilang types via the helper
functions described below.

- **`*T` ↔ `*const T`**: `*T → *const T` is implicit (drops write
  capability); the reverse is forbidden.
- **`*T` ↔ `i64`**: `as` cast both directions (raw address value).
- **`T[]` → `*T` / `*const T`**: implicit (passes the array's
  data-area pointer). ARC keeps the array alive across the call
  even if the C side writes to it.
- **Object (`@extern(C)` struct) → `*StructName`**: implicit
  (passes the user-data pointer).
- **`*T` ↔ `*U`** (block-only): explicit `as` cast for C-style
  type punning (e.g. `*const u8 → *const void`).

#### Marshalling helpers (block-only)

| Helper | Signature | Purpose |
| --- | --- | --- |
| `cstrFromString` | `(s: string): *const char` | Returns a temporary malloc'd NUL-terminated UTF-8 buffer. The C side is expected to copy it during the call. |
| `stringFromCstr` | `(p: *const char): string` | Copies a C pointer into a fresh `StringRc` (length detected via NUL). |
| `freeCstr` | `(p: *const char): unit` | Frees a buffer obtained from `cstrFromString`. |
| `bytesFromBuffer` | `(p: *const void, n: size_t): u8[]` | Copies `n` bytes into a fresh `u8[]`. |
| `readI8`/`readI16`/`readI32`/`readI64` | `(p: *const void, offset: i64): iN` | Alloc-free signed primitive load at `p + offset` (offset in **bytes**). Caller is responsible for alignment. |
| `readU8`/`readU16`/`readU32`/`readU64` | `(p: *const void, offset: i64): uN` | Same shape, unsigned. |
| `readF32`/`readF64` | `(p: *const void, offset: i64): fN` | Float variant. |
| `writeI8`/`writeI16`/`writeI32`/`writeI64` | `(p: *void, offset: i64, value: iN)` | Companion store at `p + offset`. |
| `writeU8`/`writeU16`/`writeU32`/`writeU64` | `(p: *void, offset: i64, value: uN)` | Same shape, unsigned. |
| `writeF32`/`writeF64` | `(p: *void, offset: i64, value: fN)` | Float variant. |
| `arrayFromCArray<T>` | `(p: *const T, n: size_t): T[]` | Copies a primitive array (T = numeric / bool). |
| `cstrArrayToStrings` | `(p: *const *const char): string[]` | Walks a NULL-terminated `char**` and copies each element (`environ` / argv style). |
| `errnoCheck` | `(rc: i32): i32?` | POSIX "negative return = failure". `rc < 0` → `none`, else `some(rc)`. |
| `errnoCheckI64` | `(rc: i64): i64?` | Same shape for `ssize_t`-style return values. |

Look up the failure cause via `os.errno()` separately.

#### Pass-by-value (struct by-value)

When an `@extern(C) { ... }` function takes a `struct` argument or
return, **by-value passing is automatic** (replacing the old
`byValue` flag). The JIT follows the AArch64 AAPCS64 / x86_64
SysV "integer-only ≤ 16 B composite" rule, splitting structs into
1–2 i64 chunks; HFAs ride in FP registers.

- Field constraints: **integers / bool / raw pointer / size_t /
  ssize_t / char** — or an **HFA** (1..=4 same-type floats).
  Mixing int + float fails registration.
- ≤ 8 B → 1 GPR; 9..16 B → 2 GPRs; > 16 B → indirect (caller
  allocates a copy + passes pointer).
- HFA → each element flows in its own FP register
  (V0..V3 / XMM0..XMM3).
- > 16 B return → sret (hidden first parameter pointing at the
  caller-allocated buffer).

#### Encapsulation (no escapes outside the block)

ilang prevents raw pointers and C-only types from appearing
outside the block at the type level:

1. **C-only type names** (`*T` / `char` / `size_t` …) cannot be
   used in annotations or declarations outside the block.
2. **Expressions whose value type contains a C-only type** are
   rejected outside the block:
   ```rust
   let raw = strdup(cstrFromString("x"))   // ERROR: *const char outside @extern(C)
   ```
3. **Marshalling helpers** (`cstrFromString` …) cannot be called
   outside the block.
4. **ilang-side fns inside the block (no `@lib`) cannot expose raw
   pointers in their parameter or return types**, directly *or*
   through a `@extern(C) struct` field that contains one. The check
   walks struct fields recursively. So `fn driverInfo(): SDL_RendererInfo`
   is rejected because `SDL_RendererInfo.name: *const char` —
   build a plain ilang class (e.g. `RendererInfo` with `name:
   string`) and convert at the boundary instead.

This physically prevents accidents like "ilang code keeps holding
strdup's return value". FFI wrapping always stays inside an
`@extern(C)` function that yields ilang-native types
(`string`, `i32`, `T[]`, …).

```rust
@extern(C) {
    @lib("c") fn strdup(s: *const char): *const char

    // strdup → ilang string copy → caller-owned free
    fn dupCounted(s: string): string {
        let raw = strdup(cstrFromString(s))     // OK inside the block
        let copy = stringFromCstr(raw)
        test.countedFree(raw as i64)
        copy
    }
}

// Outside the block, callers only see ilang-native types
let copy = dupCounted("hello")
```

#### Wrapping POSIX errno conventions (`errnoCheck`)

```rust
@extern(C) {
    @lib("c") fn read_raw(fd: i32, buf: *u8, n: size_t): ssize_t

    fn safeRead(fd: i32, buf: u8[]): i64? {
        errnoCheckI64(read_raw(fd, buf, buf.length as u64))
    }
}

if let some(n) = safeRead(fd, buf) {
    // success
} else {
    let code = os.errno()
    // failure
}
```

#### Opaque handles

The successor to `@extern("lib") class Foo {}`. An **empty struct**
acts as the opaque handle:

```rust
@extern(C) {
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE
    @lib("c") fn fclose(stream: *FILE): i32
}
```

- `*FILE` is a raw C pointer (i64) at the ABI level.
- `new FILE()` is **forbidden** (raw pointer types can't be
  constructed from ilang).
- Cleanup is the caller's responsibility — call `fclose(...)`
  explicitly. There's no automatic RAII / ARC (the old
  `deinit`-on-opaque-class behaviour is gone).

#### Out-pointer (sqlite3_open style)

```rust
@extern(C) {
    struct Buf {}
    @lib("c") fn posix_memalign(memptr: *i64, align: size_t, size: size_t): i32
    @lib("c") fn free(ptr: *Buf)

    fn freeRaw(p: i64) { free(p as *Buf) }
}

let slot: i64[] = [0]
if posix_memalign(slot, 64 as u64, 1024 as u64) == 0 {
    let raw = slot[0]                // a regular i64
    // ... use raw ...
    freeRaw(raw)
}
```

- Pass a 1-element `i64[]` as the slot; the written pointer comes
  back as a plain `i64`.
- A thin in-block wrapper (like `freeRaw`) handles the
  `i64 → *Buf` cast when needed.

#### C callbacks (function pointers)

```rust
@extern(C) {
    @lib("c") fn qsort(
        base: *void, nmemb: size_t, size: size_t, compar: fn(*const void, *const void): i32
    )
}

fn cmp(a: *const void, b: *const void): i32 { ... }   // top-level fn
qsort(...)                                            // pass cmp directly
```

- Parameter types are restricted to numeric primitives + raw
  pointers.
- Only **bare top-level function names** can be passed.
  `let f = my_fn; ext(f)` is rejected (closure boxes carry an
  env-pointer slot that the C ABI has no place for).

#### Other quirks

- The automatic `string → NUL-terminated UTF-8` argument
  marshalling is **not used**. Always go through `cstrFromString`
  explicitly.
- The automatic `string` return-value copy is **not used either**
  — use `stringFromCstr`.
- NUL bytes inside a string are truncated at the first occurrence
  during `cstrFromString` (matches C semantics).
- A library-open or symbol-resolve failure (without `@optional`)
  is a compile-time (JIT-build-time) error.
- Items inside an `@extern(C) { ... }` block can be declared in
  any order.

### Built-in `math` module

```rust
use math
math.sqrt(16.0)              // 4.0
math.sin(math.pi / 2.0)      // 1.0  ← `math.pi` is a const, no parens
math.pow(2.0, 10.0)          // 1024.0
math.atan2(1.0, 1.0)         // π/4
```

Functions (all f64): `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`atan2`, `sqrt`, `pow`, `exp`, `ln`, `log10`, `log2`, `floor`,
`ceil`, `round`, `abs`. Constants: `pi`, `e` (declared as `const`
in the bundled module). Both interpreter and JIT.

### Built-in `test` module

For self-asserting scripts and integration-test fixtures. On
failure, prints to stderr and exits with **exit code 2**.

```rust
use test
test.expect(1 + 2 * 3, 7)              // i64 vs i64
test.expectStr("ab" + "c", "abc")      // string vs string
test.expectBool(false, false)
test.expectF64(2.5 + 0.5, 3.0)
test.expectTrue(1 < 2)                 // single condition
test.expectFalse(1 > 2)
test.fail("should not reach here")    // forced failure
```

Both interpreter and JIT. `.il` files in
`crates/ilang-cli/tests/programs/*.il` are picked up by the
harness (`programs.rs`), which runs them in both modes and
compares exit codes.

### Built-in `os` module

A thin wrapper over OS-level state — errno read/write plus
POSIX-standard error-code constants.

```rust
use os

@extern(C) {
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE

    fn tryOpen(path: string, mode: string): i32 {
        let f = fopen(cstrFromString(path), cstrFromString(mode))
        if (f as i64) == 0 { 0 as i32 } else { 1 as i32 }
    }
}

if tryOpen("/missing", "r") == 0 as i32 {
    let code = os.errno()
    if code == os.ENOENT {
        // file not found
    } else if code == os.EACCES {
        // permission error
    }
}
```

**Functions:**
- `os.errno(): i32` — current thread's `errno` (Windows:
  `GetLastError()`). Read it right after a libc call that
  signalled failure (NULL / -1 / 0 etc.).
- `os.setErrno(code: i32)` — overwrite errno. Use
  `os.setErrno(0)` before a call to detect failure
  unambiguously.
- `os.libLoaded(name: string): bool` — whether a `@lib(...)`
  library loaded successfully. Use to guard `@optional`
  functions.
- `os.libLoadError(name: string): string` — dlopen error message
  for a failed load (empty string on success or untried). For
  diagnostics; gating logic should use `libLoaded`.
- The value persists until something else changes it (POSIX
  semantics — successful libc calls don't clear it).

**Constants (all i32):**
- **errno**: `EPERM`(1), `ENOENT`(2), `ESRCH`(3), `EINTR`(4),
  `EIO`(5), `ENXIO`(6), `E2BIG`(7), `ENOEXEC`(8), `EBADF`(9),
  `ECHILD`(10), `ENOMEM`(12), `EACCES`(13), `EFAULT`(14),
  `EBUSY`(16), `EEXIST`(17), `EXDEV`(18), `ENODEV`(19),
  `ENOTDIR`(20), `EISDIR`(21), `EINVAL`(22), `ENFILE`(23),
  `EMFILE`(24), `ENOTTY`(25), `ETXTBSY`(26), `EFBIG`(27),
  `ENOSPC`(28), `ESPIPE`(29), `EROFS`(30), `EMLINK`(31),
  `EPIPE`(32), `EDOM`(33), `ERANGE`(34)
- **Standard fds**: `STDIN_FILENO`(0), `STDOUT_FILENO`(1),
  `STDERR_FILENO`(2)
- **exit**: `EXIT_SUCCESS`(0), `EXIT_FAILURE`(1)
- **lseek whence**: `SEEK_SET`(0), `SEEK_CUR`(1), `SEEK_END`(2)
- **open() access**: `O_RDONLY`(0), `O_WRONLY`(1), `O_RDWR`(2),
  `O_NONBLOCK`(4), `O_APPEND`(8)
- **File-mode bits**: 9 bits of `S_I[RWX][USR/GRP/OTH]` (POSIX
  standard values)
- **Sockets**: `AF_UNIX`(1), `AF_INET`(2), `AF_INET6`(30 — macOS
  value; Linux=10), `SOCK_STREAM`(1), `SOCK_DGRAM`(2),
  `SOCK_RAW`(3)
- **Signals**: `SIGINT`(2), `SIGQUIT`(3), `SIGILL`(4),
  `SIGABRT`(6), `SIGFPE`(8), `SIGKILL`(9), `SIGSEGV`(11),
  `SIGPIPE`(13), `SIGALRM`(14), `SIGTERM`(15)

Only constants whose values match between macOS and Linux glibc
are included. Platform-divergent ones (`EAGAIN`, `O_CREAT`,
`O_TRUNC`, …) are intentionally omitted — hard-code them after
checking your platform's headers, or call libc directly through
`@extern(C) { @lib("c") fn ... }`.

Both interpreter and JIT (the implementation reads / writes
Rust's C-runtime errno directly).

### `const` (constant declaration)

Top-level immutable constants. The RHS is restricted to
**compile-time-evaluable expressions**; the loader's inline pass
folds them and replaces references with the literal value.

```rust
const TWO: i64 = 2
const N: i64 = 1 + 2 * 3            // 7 (arithmetic)
const TWO_N: i64 = N * 2            // 14 (references prior const)
const HELLO: string = "Hi, " + "World"
const FLAG: bool = !(1 == 2) && (3 < 5)
const MASK: i64 = 0xFF & 0x3C       // 60
const HALF: f64 = 1.0 / 2.0

fn double(n: i64): i64 { n * TWO }
double(21)                          // 42
```

- Allowed operations: arithmetic (`+ - * / %`), bitwise
  (`& | ^ << >> ~`), comparison (`== != < <= > >=`), logical
  (`&& || !`), string concat (`+`), `as` cast (between numerics).
- Allowed references: other `const`s declared earlier in the same
  file (folding is order-dependent — no forward references).
- **Forbidden**: function calls, field/method access, arrays,
  `new`, `if`/`match`, loops — anything needing runtime.
- Folding errors (e.g. divide-by-zero) are **compile-time
  errors**.
- Type annotation (`: T`) is optional (inferred). When present
  on a numeric `const`, the annotation propagates to every
  reference: a `const N: u32 = 0x10` substitutes as `(0x10 as u32)`
  at every use, so callers don't need their own `as u32`.
- The bundled `math` module's `pi` / `e` are defined this way.
- References across modules work fine (the loader stores the
  qualified name `math.pi`).

---

## 14. Comments

```rust
// line comment
/* block comment */
/* nestable: /* outer /* inner */ outer */ */
```

---

## 15. ASI (automatic semicolon insertion)

- Newline (LF or CRLF), `;`, `}`, and EOF all end a statement.
- A newline inside an expression is ignored:
  `let x = 1\n + 2` is `let x = 1 + 2`.
- Class members on the same line **must be separated by `;`** —
  ASI doesn't fire between class declarations on a single line.

---

## 16. Execution model

| Mode | Command | Notes |
| --- | --- | --- |
| Tree-walking | `ilang run path.il` | Every feature except FFI; fast startup |
| Cranelift JIT | `ilang run --jit path.il` | Native code; tens to hundreds of times faster than the interpreter |
| REPL | `ilang` (no args) | Line-by-line evaluation, `let`/`fn`/`class` persist; interpreter only |

The CLI walks upward from the entry file looking for an
`ilang.toml`. If present, each `[deps]` value adds an extra
search directory the loader uses for `use module` resolution.

JIT-only:
- Static fields are limited to `i64` / `f64` / `bool` (no
  string / object yet).
- Inheritance / static members / properties on generic classes
  aren't supported (type-parameter resolution constraints).

Interpreter-only:
- `@extern(C) { @lib(...) fn ... }` requires dlsym, which the
  interpreter doesn't drive — host-form (`@lib` omitted) bare
  functions still work in both modes.

---

## 17. Not implemented yet (TODO)

- **`?` operator** (Result short-circuit. `let v = parse(s)?` to
  early-return on `Result.err`).
- **String interpolation** (backtick + `${expr}` style).
- **Iterator protocol** — let user types implement `next()` and
  participate in `for-in`. Foundation for generators.
- **Named arguments** (`open(path: "x", mode: "w")`) — default
  arguments are implemented; named-call sites are not.
- **Operator overloading** (`class Vec2 { + (other: Vec2): Vec2 { ... } }`).
- **Trait / Interface** (shape-based abstraction, orthogonal to
  inheritance).
- **Destructuring** (`let (a, b) = pair` / `let { x, y } = point`).
- **Async / await** (concurrency).
- **Generic constraints (bounds)**.
- **Method overloading across the inheritance hierarchy** (only
  the root class supports overloading today).
- **Inheritance of static fields / methods**.
- **Inheritance / static members / properties on generic
  classes** — blocked by type-parameter resolution.

### Deliberately not adopted

- **Exceptions (`throw` / `try` / `catch`)**: not adopted. Express
  fallible operations with `Result<T, E>` and consume them via
  `match`. Unrecoverable bugs (divide-by-zero, out-of-bounds,
  `unwrap()` on `none`) **panic** — execution stops, no `catch`.
  - Reasons: control flow stays in the type system, signatures
    show fallibility, easier to reason about with ARC.
    Same posture as Rust / Go / Zig.

---

For internal design details and handoff notes see
[`HANDOFF.md`](HANDOFF.md).
