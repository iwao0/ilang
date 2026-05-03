# ilang 構文チートシート

実装済みの構文を一覧で示します。各項目は実際にパース・型チェック・実行が通る形のみ。

`.il` ファイルを `cargo run -p ilang-cli -- run path.il` (ツリーウォーク) または `... run --jit path.il` (Cranelift JIT) で実行できます。引数なしで起動すると REPL に入ります。文末セミコロン `;` は省略可で、改行が文の区切りになります (JS 風 ASI)。

---

## 1. リテラル

| 種類 | 例 | 自然な型 |
| --- | --- | --- |
| 整数 | `42`, `-7`, `0xff`, `0o755`, `0b1011`, `1_000_000` | `i64` |
| 整数 (型サフィックス) | `1_i32`, `255_u8`, `0xffff_u16` | サフィックスの型 |
| 浮動小数 | `3.14`, `1.5e10`, `2.5_f32` | サフィックスがあればその型、無ければ `f64` |
| bool | `true`, `false` | `bool` |
| 文字列 | `"hello"`, `"line\nbreak"` (`\n` `\t` `\r` `\\` `\"` `\0`) | `string` |
| Unit | `()` (式で生まれる、自前で書かない) | `()` |
| Optional | `none`, `some(x)` | `T?` |
| 配列 | `[1, 2, 3]`, `[1, 2, 3,]` (末尾コンマ可) | `T[]` |
| タプル | `(1, "hello")`, `(true, 3.14, [1,2])` | `(T1, T2, ...)` (2 要素以上) |
| Map | `{"a": 1, "b": 2}` | `Map<K, V>` |

---

## 2. 型

```text
i8  i16  i32  i64
u8  u16  u32  u64
f32  f64
bool  string
()                  // Unit (戻り値型などで)
ClassName           // クラスインスタンス
T[]                 // 動的配列 (push 可)
T[N]                // 固定長配列
(T1, T2, ...)       // タプル (2 要素以上、`(T)` はグループ化)
T?                  // Optional (none もしくは some(t))
ClassName.weak      // 弱参照 (Object 限定)
ClassName<T1, T2>   // ジェネリッククラスのインスタンス化
Map<K, V>           // 組み込み辞書 (K = string / 整数 / bool)
fn(T1, T2): R       // 関数値 (キャプチャなし)
```

後置修飾子 `[]` `[N]` `?` `.weak` は重ねられる: `Foo[]?`, `User?[]`, `Node.weak?` 等。`.weak` は `ClassName.weak` の形のみ (string や i64 には付けられない)。

### 暗黙の型変換

| from → to | 暗黙? |
| --- | --- |
| 同符号の整数同士 (狭→広 / 広→狭) | yes |
| 整数 → 浮動 | yes |
| `f32` ↔ `f64` | yes |
| 符号またぎ (`i32` ↔ `u32` 等) | **no** (`as` 必須) |
| 浮動 → 整数 | **no** (`as` 必須) |
| `T` → `T?` (Optional 自動 wrap) | yes |
| `Foo` → `Foo.weak` (strong → weak 自動 downgrade) | yes (同一クラスのみ) |

`expr as Type` で明示キャスト。`if`/`else` の枝合流では暗黙の数値拡張を許さない (整数リテラルのみ例外的に他方の型へ coerce)。

---

## 3. 変数

```rust
let x = 1                  // 型推論
let y: f64 = 1             // 注釈付き (整数 → f64 暗黙変換)
let s: string = "hi"
let xs: i32[] = [1, 2, 3]
let maybe: User? = some(u) // Optional は `T → T?` 自動 wrap
let w: User.weak = u       // strong → weak 自動 downgrade
```

- `mut` キーワード **なし**。すべての `let` は再代入可。
- 同名 `let` で内側スコープのシャドウ可 (外側の値はスコープ脱出時に復元)。
- 空配列リテラル `[]` には型注釈が必要 (`let a: i32[] = []`)。
- タプル要素アクセスは配列と同じ `t[N]` 記法だが、`N` は **コンパイル時の非負整数リテラル** に限る (要素ごとに型が異なるため)。要素への代入はサポートしない。

```rust
x = x + 1                  // 単純代入
x += 1                     // 複合代入: += -= *= /= %= &= |= ^= <<= >>=
obj.field = v
arr[i] = v                 // 配列添字代入
map[k] = v                 // Map 添字代入
this.field = v             // メソッド内
```

---

## 4. 演算子 (低 → 高)

| 優先度 | 演算子 | 結合 |
| --- | --- | --- |
| 1 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` | 右 |
| 2 | `\|\|` | 左 (短絡) |
| 3 | `&&` | 左 (短絡) |
| 4 | `\|` | 左 |
| 5 | `^` | 左 |
| 6 | `&` | 左 |
| 7 | `==` `!=` | 左 |
| 8 | `<` `<=` `>` `>=` | 左 |
| 9 | `<<` `>>` | 左 |
| 10 | `+` `-` | 左 |
| 11 | `*` `/` `%` | 左 |
| 12 | `as` (キャスト、後置) | — |
| 13 | 単項 `-` `+` `!` `~` | 前置 |
| 14 | `.` (フィールド/メソッド) / `[]` (添字) / `(...)` (呼び出し) | 後置 |

文字列に対しては `+` (連結) と `==`/`!=` (構造的等値) のみ。オブジェクトの `==`/`!=` は同一クラスでの参照等値。`%` は浮動小数では未対応。

### 文字列の組み込みメソッド

```rust
"hello".length              // i64 — Unicode コードポイント数 ("あいう".length == 3)
"hello".charAt(1)           // string — 1 文字。範囲外は ""
"hello".includes("ell")     // bool
"hello".startsWith("he")    // bool
"hello".endsWith("lo")      // bool
"Hi".toUpper()          // string
"Hi".toLower()          // string
"  hi  ".trim()             // string
"a,b,c".split(",")          // string[]  ─ 空セパレータでは1文字ずつ
"abca".replace("a", "_")    // string    ─ 全箇所置換 (Rust流)
"hello".slice(1, 4)         // string    ─ 添字は Unicode コードポイント、範囲外はクランプ
```

文字列補間は未実装。上記のメソッドは interpreter / JIT とも対応。

---

## 5. 制御フロー

```rust
// if は式
let r = if n > 0 { n } else { -n }
if cond { ... } elif cond2 { ... } else { ... }   // `else if` ではなく `elif`

// while
while cond { ... }

// loop は break のみで抜けられる
let i = 0
loop {
    if i >= 10 { break }
    if i % 2 == 0 { i += 1; continue }
    i += 1
}

// for-in (配列 or 範囲を回す)
let xs: i64[] = [10, 20, 30]
for x in xs { console.log(x) }     // break / continue 可

// 範囲 (Rust 風) — 排他 `..` と包含 `..=`
for i in 1..5 { console.log(i) }   // 1, 2, 3, 4
for i in 1..=5 { console.log(i) }  // 1, 2, 3, 4, 5

// if let — Optional のパターンマッチ (`match` 以外で使える唯一の pattern 形)
let x: i64? = some(42)
if let some(v) = x {
    // v: i64 として使える
} else {
    // none ケース
}
```

`break` / `continue` はループ内のみ (型チェッカーが範囲外を拒否)。

範囲式 `a..b` / `a..=b` は **`for-in` のイテレータ位置でのみ** 使えます。`let r = 1..10` のように値として保持しようとするのは型エラー。両端は同じ整数型である必要があり、ループ変数はその型にバインドされます。

`loop` は `break v` で値を持って抜けられ、その値が `loop` 式自身の型/値になります (Rust と同じ)。`while` / `for` には条件で完走するパスがあるため `break v` は使えません (型チェッカーが拒否)。

```rust
let n = loop {
    if ready() { break compute() }     // loop は break の型 i64 を持つ
}

let i = 0
let first_even = loop {
    if i % 2 == 0 && i > 0 { break i }
    i = i + 1
}

loop { break }                          // 値なし break — loop の型は Unit
```

- 同じ `loop` 内の `break v` はすべて型一致が必要 (型チェッカーが mismatch を拒否)
- `break` (値なし) は常に許容、`break v` は `loop` 内のみ
- interpreter / JIT とも対応

```rust
// return — 関数/メソッドからの早期脱出。値ありと値なし両対応。
fn abs(n: i64): i64 {
    if n < 0 { return -n }
    n
}
fn maybe_bump(c: Counter, n: i64) {
    if n < 0 { return }   // Unit fn の値なし return
    c.bump()
}
```

末尾式は今までどおり戻り値として使える。`return` を書かなくても良い。

---

## 6. 関数

```rust
// 戻り値型は `: T` (TS 風)
fn add(a: i64, b: i64): i64 {
    a + b                  // 末尾式が戻り値
}

fn greet(name: string) {   // 戻り値型省略 = ()
    console.log("hi,", name)
}

fn factorial(n: i64): i64 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}
```

- パラメータは型必須。
- 関数のジェネリクスはサポート (`fn name<T, U>(...)`) — 詳細は [ジェネリック関数](#ジェネリック関数) 参照。
- variadic は組み込み (`console.log` のみ) のみ対応。
- **デフォルト引数**: `fn open(path: string, mode: string = "r")` のように末尾のパラメータに `= 式` を付けると、呼び出し側で省略可能。デフォルト式は呼び出し時に毎回評価される。デフォルトを持つパラメータの後に必須パラメータは置けない。オーバーロードされた関数群に混ぜてもよく、引数数が完全一致する候補が常に優先される (デフォルト埋め込みの候補は +1000 ペナルティ)。

### ジェネリック関数

クラスや enum と同じく `<T, U>` で型パラメータを宣言できます。型引数は呼び出し時の引数の型から推論されます (明示指定の構文は未提供)。

```rust
fn id<T>(x: T): T { x }
fn first<T>(xs: T[]): T { xs[0] }

id(42)            // T = i64
id("hello")       // T = string
first([1, 2, 3])  // T = i64
```

- 推論は引数の型から左から右へ進み、最初に決まったバインディングを採用する (enum コンストラクタと同じ方針)。
- 戻り値の型に出てくる `T` は推論されたバインディングで置換される。
- interpreter / JIT とも対応。JIT 時は型検査で得たバインディングを元に、各 (関数, 型引数) ペアごとにモノモルフ化された具象関数を生成する。

### 関数のオーバーロード

同名で異なるパラメータ型・個数の関数を複数宣言できます。呼び出し側では引数型から最良のオーバーロードを選択します。

```rust
fn show(n: i64): string { "int" }
fn show(s: string): string { "str" }
fn show(b: bool): string { "bool" }

show(42)        // "int"
show("hi")      // "str"
show(true)      // "bool"

// アリティ違いも OK
fn make(): string { "default" }
fn make(s: string): string { s }
fn make(s: string, suffix: string): string { s + suffix }
```

**選別ルール (best-match scoring)**: 各オーバーロードを実行可能 (引数型ごとに暗黙変換が成立する) 候補として、スコア合計が最小のものを選ぶ。
- 完全一致 = 0
- 同符号整数の widening (`i32 → i64` 等) = 1
- `f32 ↔ f64` = 1
- 整数 → float = 2
- リテラル幅変換 = 2
- `T → T?` (auto-wrap) = 3
- `Object → Weak` = 4

最良スコアが複数のオーバーロードで同点の場合は **ambiguous エラー** で拒否されます。完全一致するオーバーロードがあれば常に勝つので、よくある「明示的に書いた版が選ばれる」期待と一致します。

**禁止される組み合わせ**:
- 同名でジェネリック関数と非ジェネリック関数を両方宣言: `fn id<T>(x: T): T` と `fn id(x: i64): i64` を同時に書くとエラー (ジェネリック解決パスとオーバーロード解決を混ぜないため)
- 完全に同じシグネチャの重複宣言

**ファーストクラス参照**: オーバーロードされた名前を `let f = name` で参照するのは ambiguous エラー。直接呼び出すか、`fn name__i64` のようにマングル後の名前を使う必要があります (内部実装が漏れるので非推奨)。

interpreter / JIT とも対応。型検査後にオーバーロードされた名前は `name__<param_types>` にマングルされ、各呼び出しサイトもそれに合わせて書き換えられます。

### ファーストクラス関数

関数は値として変数に代入したり、引数や戻り値として渡せます。匿名関数の本体は外側のローカル変数を **値でキャプチャ** できます (interpreter / JIT とも対応、すべての型をキャプチャ可能)。

```rust
fn add(a: i64, b: i64): i64 { a + b }
let f = add                          // 関数値を代入 (型は fn(i64, i64): i64)
f(2, 3)                              // 5

// 匿名関数 (即値) — 既存 fn 構文から名前を抜いた形
let inc = fn(x: i64): i64 { x + 1 }
inc(41)                              // 42

// クロージャ — 外側のローカルを値でキャプチャ
let factor = 10
let scale = fn(x: i64): i64 { x * factor }
scale(3)                             // 30

// 関数を返すことで closure-of-closure も書ける
fn make_adder(n: i64): fn(i64): i64 {
    fn(x: i64): i64 { x + n }
}
let add5 = make_adder(5)
add5(3)                              // 8

// 関数を引数に取る/返す
fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
fn double(n: i64): i64 { n * 2 }
apply(double, 7)                     // 14
```

- 関数型: `fn(T1, T2): R` (戻り値が `()` なら `: R` 省略可)
- ローカル `let f = some_fn` は同名のトップレベル fn より優先 (シャドーイング)
- **キャプチャは値スナップショット**: クロージャ作成時点の outer 変数の値を retain (ARC型) もしくはコピー (プリミティブ)。後から outer 変数が変わってもクロージャの内部値は変わらない (Rust の `move` クロージャ相当)
- パラメータが capture と同名の場合はパラメータが優先 (シャドーイング)
- 同名のトップレベル fn / クラスはキャプチャ対象外 (グローバルな名前として解決)
- **interpreter / JIT 両対応**。すべての型を capture 可能 (i64 / f64 / bool / string / object / array / optional / map)。JIT は closure を `[fn_ptr | env_field0 | ...]` 構造体としてヒープに確保し、ARC で管理 (capture が ARC 型なら retain、closure 解放時に自動 release)
- ネストしたクロージャ (closure-of-closure) も対応 — 内側の closure は外側の closure の captures を再 capture できる
- top-level fn を fn 値として参照 (`let f = some_fn`) すると、env を無視してターゲットを呼ぶ trampoline closure が自動生成される

### 属性 / アノテーション (パースのみ、enforce は未実装)

```rust
@requires(net)
fn fetch(url: string): string { ... }

@requires(net, file)
@deprecated(use_v2)
fn download(url: string, path: string) { ... }
```

`@name(args)` 形式 (TS / Java / Python のデコレータ風)。複数並べる場合はそれぞれ `@` から始める。引数リストは省略不可 (`@x` 単独はパースエラー)。属性はメソッドにも付けられるが、クラス自体への付与は未対応。

---

## 7. クラス

```rust
class Counter {
    count: i64                          // フィールド宣言
    init(start: i64) { this.count = start }
    bump(): i64 {
        count += 1                      // 暗黙 this (フィールド/メソッド)
        count
    }
    deinit() { ... }                    // スコープ脱出時に走る (省略可)
}

let c = new Counter(10)
c.bump()                                // メソッド呼び出し
c.count                                 // フィールド読み取り
```

- `init` は唯一のコンストラクタ (Swift 風)。`init() {}` を省略するとデフォルトで引数なし `new` 可。
- `deinit` は引数なし・戻り値 () 限定。明示呼び出し不可 (`c.deinit()` はエラー)。
- 暗黙 `this`: メソッド本体内で `this.` を省略可。ただしローカル変数や引数があればそちら優先。
- 継承 (`extends`) / `static` / `get`/`set` プロパティは下記の節で詳述。`private` 修飾子は未実装。
- 同一行に複数のクラスメンバーは書けない (ASI が効かないので `;` か改行必須)。

### ジェネリッククラス

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

let b = new Box<i64>(42)            // 型引数は明示必須
let p = new Pair<string, i64>("k", 1)
let nested = new Box<Box<i64>>(new Box<i64>(99))   // ネストも可 (>> 自動分割)
```

- 型引数は **インスタンス化時に明示必須** (`new Box<i64>(42)` — 推論なし)
- 制約 (bounds) はサポートされない (任意の型を入れられる)
- **JIT 対応済**。コンパイル時にモノモーフ化 (`Box<i64>` と `Box<f64>` は別クラスとしてコード生成)
- 関数のジェネリクスは [§6 ジェネリック関数](#ジェネリック関数) 参照 — interpreter / JIT とも対応
- 型変数同士の演算 (例: `class Pair<A, B> { ... a + b ... }`) は型チェッカが拒否 (制約がないため)

### メソッド / `init` のオーバーロード

同名で異なるパラメータ型・個数のメソッドを複数宣言できます。`init` も同様で、`new C(...)` の引数から最良の `init` が選ばれます。スコアリングと曖昧性ルールは [§6 関数のオーバーロード](#関数のオーバーロード) と完全に同じです。

```rust
class Greeter {
    init() {}
    init(name: string) { this.name = name }     // init オーバーロード OK
    name: string
    greet(): string { "hi" }
    greet(n: i64): string { "hi x" + (n as string) }   // メソッドオーバーロード OK
}

let a = new Greeter()
let b = new Greeter("ada")
b.greet()                                       // → "hi"
b.greet(3)                                      // → "hi x3"
```

- **`deinit` はオーバーロード不可**: 常にランタイムから引数 0 で呼ばれるため、複数宣言はエラー。
- **ジェネリッククラスのメソッドはオーバーロード不可**: `class Box<T> { f(x: i64): ...  f(x: string): ... }` はエラー (mono とオーバーロード解決パスを混ぜないため)。
- interpreter / JIT とも対応。型検査後にオーバーロードされたメソッドは `name__<param_types>` にマングルされ、`new C(...)` の AST には選ばれた `init_method` が記録されます。

### `get` / `set` プロパティ

`get name(): T { ... }` と `set name(v: T) { ... }` で計算プロパティを定義できます。呼び出し側はフィールドと区別なく `obj.name` で読み・`obj.name = v` で書きます。バッキング ストアは別フィールドで自前。

```rust
class Temp {
    celsius: f64
    init(c: f64) { this.celsius = c }
    get fahrenheit(): f64 { this.celsius * 9.0 / 5.0 + 32.0 }
    set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
}

let t = new Temp(0.0)
t.fahrenheit              // 32.0  (getter 呼び出し)
t.fahrenheit = 100.0      // setter 呼び出し
t.celsius                 // 37.77...
```

- getter は引数なし・戻り値型必須、setter は引数 1 個・戻り値なし。型チェッカが強制
- getter のみ (read-only) / setter のみ (write-only) も可。逆操作はそれぞれ「getter なし」「setter なし」で型エラー
- getter の戻り値型と setter の引数型はプロパティ型で一致が必要
- プロパティ名はフィールド名・メソッド名と重複不可
- `get` / `set` はクラス本体内でのみキーワードとして扱われ、それ以外の場所では普通の識別子として使えます (contextual keyword)
- interpreter / JIT とも対応

### `static` メソッド

`static` を付けるとインスタンスなしで `ClassName.method(args)` で呼べる **クラスレベル メソッド** になります。本体内で `this` は使えません (型エラー)。

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

- オーバーロード不可 (`static foo` を 2 つ以上書くとエラー)
- フィールド / インスタンスメソッド / プロパティと同名は不可
- ジェネリッククラスでの静的メソッドは未対応 (型パラメータが静的コンテキストで参照できないため)
- `static` はクラス本体内のみキーワード (contextual keyword)
- ローカル変数 `let Vec2 = ...` がある場合はそちら優先 (シャドウ)
- interpreter / JIT とも対応

#### `static` フィールド

`static name: T = const_expr` でクラスレベルの可変ストレージを宣言できます。全インスタンスで共有され、`ClassName.field` で読み書き。

```rust
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1; Counter.total = Counter.total + 1 }

    static total: i64 = 0
    static threshold: i64 = 1 + 2 * 5      // 11 (const 折りたたみ)
}

let a = new Counter(); let b = new Counter()
a.bump(); a.bump(); b.bump()
Counter.total              // 3
```

- 型は **`i64` / `f64` / `bool`** のみ (Phase 1)。string / オブジェクトなどヒープ型は ARC 設計が確定するまで未対応
- 初期値は **コンパイル時定数式** 限定 (top-level `const` と同じ folder で評価)。関数呼び出し等のランタイム式は不可
- mutable: `Counter.total = 100` で書き換え可
- 同名の他フィールド・メソッド・プロパティ・静的メソッドとは衝突不可
- ジェネリッククラスでの静的フィールドは未対応 (静的メソッドと同じ理由)
- 内部実装: JIT は `Box<[i64]>` を確保してスロット割り当て、アクセスは絶対アドレスの load/store で f64/bool は bitcast / truncate

### 継承 (`extends`)

`class Child extends Parent { ... }` で単一継承できます (Phase B: 仮想ディスパッチ + override + super)。interpreter / JIT とも対応。

```rust
class Animal {
    name: string
    init(n: string) { this.name = n }
    speak(): string { "generic sound" }
    describe(): string { this.name + " says " + this.speak() }
}

class Dog extends Animal {
    init(n: string) { super(n) }              // 親の init を super(...) で呼ぶ
    override speak(): string { "woof" }       // override 必須
}

let d = new Dog("rex")
d.speak()                                      // "woof"
d.describe()                                   // "rex says woof" — Animal.describe が
                                               // 呼ぶ speak() が Dog のものに dispatch (仮想)

fn introduce(a: Animal): string { a.describe() }
introduce(d)                                   // OK — Dog is-a Animal (subtyping)
```

- 単一継承のみ (多重継承なし)
- 親はすでに宣言済み必須 (前方参照不可)
- `override` キーワード必須。親に同名メソッドがないのに `override` を付けるとエラー、親にあるのに `override` を付けないとエラー (hides parent)
- override のシグネチャは親と完全一致が必要 (現状)
- `super.method(args)` で親のバージョンを呼ぶ (静的解決、自分のクラスの親を遡る)
- `super(args)` は子の `init` 内で親の `init` を呼ぶ
- インスタンスフィールド継承: 親のフィールドが先、子の追加フィールドが後
- メソッドのオーバーロードは継承階層では未対応 (ルートクラスのみ)
- `init` / `deinit` は per-class (継承の override 対象外)
- 静的メソッド・静的フィールドの継承は未対応
- ジェネリッククラスでの継承は未対応
- サブタイプ: `Child` を `Parent` 型の binding / 引数 / 戻り値に渡せる
- JIT: object header に vtable ポインタを足し (`[strong | weak | drop_fn | vtable | fields...]`、32 byte ヘッダ)、各クラスに `Box<[i64]>` vtable を確保。仮想呼び出しは `obj.vtable[slot]` の load → call_indirect。`super.method` は親の特定関数への直接呼び出し

---

## 8. 配列

```rust
let xs: i32[] = [10, 20, 30]    // 動的配列リテラル
let ys: i32[3] = [1, 2, 3]      // 固定長 (要素数も型に固定)
let zs: i32[] = []              // 空配列は注釈必須
let trailing = [1, 2, 3,]       // 末尾コンマ可

xs[1]                            // 添字読み取り
xs[0] = 100                      // 添字代入
xs.length                        // i64 を返す (組み込み)
xs.push(40)                      // 動的配列のみ。固定長は型エラー
xs.pop()                         // T? を返す (空なら none)。動的配列のみ
xs.indexOf(20)                   // i64 を返す (見つからなければ -1)
xs.includes(20)                  // bool を返す
```

高階メソッドもサポート: `xs.map(fn)` / `xs.filter(pred)` / `xs.forEach(fn)` / `xs.slice(start, end)`。コールバックは **第一級関数** または **クロージャ** (匿名 `fn` で外側のローカル変数を value-capture できる — §6 参照)。`length` / `push` / `pop` / `indexOf` / `includes` / `for-in` 含めて **interpreter / JIT とも同等** — 要素型の制限はありません。

---

## 9. 辞書 (Map)

```rust
let m: Map<string, i64> = {"a": 1, "b": 2}        // リテラル
let empty: Map<string, i64> = new Map<string, i64>()  // 空マップ

m["c"] = 3                       // 書き込み
m["a"]                           // 読み取り (キー欠如は実行時エラー)
m.get("nope")                    // V? を返す (none あり、安全な読み取り)
m.has("a")                       // bool
m.delete("a")                    // bool (削除できたか)
m.set("d", 4)                    // m["d"] = 4 と同等
m.size()                         // i64
m.keys()                         // K[]
m.values()                       // V[]
```

- キー型は `string` / `i*` / `u*` / `bool` のみ (float / オブジェクト不可 — `Eq`/`Hash` の整合性確保のため)
- リテラル `{ key: value, ... }` の最初のキーから K を、最初の値から V を推論
- 空マップは `new Map<K, V>()` で構築 (`{}` は空ブロック扱い)
- パーサは `{<key-token> :` の 2 トークン先読みで map literal とブロックを区別 (ID/Str/Int/Bool + `:` で map)
- interpreter / JIT とも対応 (基本演算 + `get` / `keys` / `values` / リテラル全部)

---

## 10. Optional

```rust
let a: User? = some(user)        // 通常の構築
let b: i64? = none               // 不在
let c: i64? = 7                  // T → T? 自動 wrap

if let some(v) = a {             // パターン分岐
    use(v)
}

a.isSome()                       // bool
a.isNone()                       // bool
a.unwrap()                       // T (none ならランタイム panic)
```

- `T?` の T は実行系で扱える任意の型 — interpreter / JIT とも対応 (JIT は primitive 内部の場合 `[rc:i64 | payload:T]` のヒープボックスで表現)。
- `T?` を関数の引数・戻り値・フィールド型にも使える。
- `none` は単独では型不定 — 文脈の Optional 型から推論される。

---

## 11. enum / match

```rust
// Phase 1: 単純な C 風列挙型 (バリアント名は組み込み Result に合わせて小文字始まりを推奨)
enum Color { red, green, blue }

let c = Color.green
// match パターンは `Enum.` を省略可 — scrutinee 型から推論
let name = match c {
    red { "red" }
    green { "green" }
    blue { "blue" }
}

// Phase 2: ペイロード付き (タプル / 名前付きフィールド)
enum Shape {
    circle: (f64)              // タプルペイロードは `: (...)` で導入
    rect: (f64, f64)
    square: { side: f64 }      // struct ペイロードは `: { ... }`
}

fn area(s: Shape): f64 {
    match s {
        circle(r) { 3.14 * r * r }
        rect(w, h) { w * h }
        square { side } { side * side }   // struct shorthand: { side: side }
    }
}

// `_` ワイルドカードで残りを捕捉。長い形 `Color.red` も引き続き有効。
let day = Color.red
match day {
    red { "alert" }
    _ { "ok" }
}
```

- **enum 宣言**: ペイロード持ちのバリアントは名前と型を `:` で区切る (`circle: (f64)`)。ユニットバリアントは `:` なし (`red`)。
- **バリアント名のケース**: 大文字でも小文字でも構文的には OK。ただし組み込み `Result.ok` / `Result.err` と統一して **小文字始まり推奨**。
- **match arm**: `=>` を使わず、パターンの直後に `{ body }` を書く (`Color.red { "red" }`)。
- 構築は `Enum.` プレフィクス必須 (`Shape.circle(3.0)`)。
- **match のパターン側は `Enum.` を省略可** — scrutinee の静的型から推論される (`circle(r)` は `Shape.circle(r)` と同義)。長い形 (`Shape.circle(r)`) も引き続き有効。
- 全バリアントを網羅するか `_` が必要 (型チェッカが拒否)
- 各 arm 値は型が揃う必要あり (if/else と同じ)
- パターンの束縛: タプルは位置 (`Shape.circle(r)`)、struct は名前 (`{ side }` または `{ side: s }`)、`_` で無視
- ペイロード内の heap 型 (Object / Str / Array / Optional / Weak / 別 enum) は ARC で正しく解放される

### ジェネリック enum

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

- `enum Name<T, U> { ... }` でクラスと同じ形式の型パラメータが書ける
- バリアント構築時の型引数は **引数から自動推論** (`Either.Right("hi")` → `Either<Any, string>`、注釈と統合される)
- 型推論で埋まらないパラメータは内部的に `Any` のままで、let 注釈や関数戻り値型でピン止めされる
- match 側もスクルチニーの具体型から束縛変数の型が自動で復元される
- interpreter / JIT とも対応 (JIT は `(enum_name, type_args)` ごとに具体 EnumDecl を生成して per-instantiation でレイアウトを取る)

### 組み込み Result<T, E>

Rust 風の組み込みジェネリック enum。バリアント名は **小文字 `ok` / `err`**。

```rust
enum Result<T, E> { ok: (T), err: (E) }   // 概念的な定義 (実装側で登録済み)

fn divide(a: i64, b: i64): Result<i64, string> {
    if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
}

match divide(10, 2) {
    ok(v) { v }            // ← match のパターンは scrutinee 型から推論されるので `Result.` 省略可
    err(_) { -1 }
}
```

- 構築は **`Result.ok(v)` / `Result.err(e)`** (`Enum.variant(...)` の通常形式)
- match パターンでは `ok(v)` / `err(e)` と短縮可 (バリアント短縮形と同じ仕組み)
- `ok` / `err` は **予約語ではない** — 変数名として使える (使うと混乱するのでおすすめしないが)
- 名前 `Result` は予約 (ユーザが `enum Result { ... }` を定義するとエラー)
- 構築時の型引数は推論 — 戻り値型や let 注釈で T/E が確定する
- match の網羅性検査もそのまま (`ok` と `err` を両方カバーするか `_` が必要)
- interpreter / JIT とも対応 (`(T, E)` ごとに具体 enum をモノモルフ化)

---

## 12. Weak (弱参照)

```rust
class Node {
    parent: Node.weak           // 循環回避用フィールド
    init(p: Node) { this.parent = p }
}

let root = new Node(...)
let w: Node.weak = root         // strong → weak 自動 downgrade

if let some(n) = w.get() {      // .get() は T? を返す (生存時 Some)
    n.method()
} else {
    // 既に解放されている
}
```

- `.weak` は **クラス型のみ**。`string.weak` や `i64.weak` は型エラー。
- weak は所有しない: strong rc を増やさない。
- `.get(): T?` で「生きていれば取得」「死んでいれば none」。
- 主用途は **循環参照の解消**: `Parent ↔ Child` のような所有グラフで子から親への back-edge を `.weak` にすると親の deinit がきちんと走る。
- JIT 実装は二重 rc (strong + weak) 方式。

---

## 13. console (組み込み)

```rust
console.log(1, "hello", true)        // variadic、空白区切り、末尾改行
console.log()                        // 改行のみ
console.log(arr, obj, opt)           // 配列/オブジェクト/Optional も整形して出力
```

- `console` は予約識別子で、ユーザが `let console = ...` や同名クラスを定義するとエラー。
- 引数の型は混在可。

---

## 13b. モジュール (`use`)

別ファイルの items (`fn` / `class` / `enum`) を取り込みます。Rust 風の **同一ディレクトリ解決**: `use utils` は隣の `utils.il` を読みます。

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
use utils                       // 名前空間越しに使う
use math { sqrt, pi }           // 選択的に取り込む

let c = new utils.Counter(10)
c.bump()
utils.double(c.get())            // → 22
```

- **2 形式**:
  - `use module` — 名前空間越し参照 (`module.foo()`, `new module.Class()`, `module.Enum.variant`)
  - `use module { name1, name2 }` — 個別取り込み (バレネームで使う)
- すべての top-level item は **public** (可視性キーワードなし)
- 循環インポート (`A → B → A`) は **DAG 検出してエラー**
- 同じモジュールを複数回 `use` しても一度しかロードされない (ファイルパスで dedupe)
- 全モジュールが 1 つの Program にマージされる (ファイル境界は型チェッカ以降は意識されない)
- 名前空間越し import の中身は `module.X` プレフィクスで内部識別されるため、`use module` した時に親プログラムの bare 名と衝突しない
- **同梱モジュール**: 一部のモジュールはコンパイラに埋め込まれており、ディスクの探索より優先される。現状は `math` のみ。

### `@extern fn` (組み込みホスト関数)

ボディの代わりに **runtime が実装を供給する** 関数宣言。`@extern` 属性を付けて本体を省略します。

```rust
// 同梱の math.il より抜粋
@extern fn sin(x: f64): f64
@extern fn sqrt(x: f64): f64
@extern fn pow(base: f64, exp: f64): f64
```

- `@extern` 属性付きの fn は **本体 `{ }` を書かない** (parse error にならない)
- 型チェッカは本体検査をスキップ (シグネチャのみ契約)
- interpreter: ホスト側のレジストリ (`crates/ilang-eval/src/externs.rs`) に名前 → Rust 関数を登録、実行時にディスパッチ
- JIT: `Linkage::Import` で宣言、`JITBuilder::symbol("math.sin", ...)` で関数アドレスを供給 (cranelift が直接 Rust 関数を呼び出す)
- 名前は loader でマングル後の qualified 形 (`math.sin`) でレジストリに登録

#### `@extern("libname")` — ネイティブ動的ライブラリの呼び出し (JIT 専用)

属性に文字列引数を渡すと、`libloading` でその名前のダイナミックライブラリを `dlopen` し、関数名と同じシンボルを引いて呼び出します。

```rust
@extern("libm.dylib") fn sqrt(x: f64): f64    // macOS
// @extern("libm.so.6") fn sqrt(x: f64): f64  // Linux

sqrt(16.0)                                     // 4.0 (libm の C 関数を直接実行)
```

- 対応する型は **整数全幅 (`i8`〜`i64` / `u8`〜`u64`) / `f32` / `f64` / `bool` / `string` / opaque extern class** (+ 戻り値の `()`、+ opaque class の `T?`)。配列 / 通常 Object / Map / Optional<primitive> 等のマーシャリングは未対応
- 整数の **符号拡張は Cranelift の C 呼出規約が自動処理** — `int abs(int)` を `fn abs(x: i32): i32` で宣言した場合、負の値も正しく行き来する
- `string` 引数は呼び出し時に **NUL 終端 UTF-8 のコピーを `malloc`** して C 側に渡し、関数復帰後に解放
- `string` 戻り値はデフォルトで C 側のポインタを **静的 / 永続と仮定** して新しい `StringRc` にコピー (`getenv` などはこれで OK)
- C 側が **ヒープに確保した文字列を返す** 場合は `@extern("lib", owned_return)` を付けると、JIT がコピー後に `libc::free(ptr)` を呼んでメモリを解放する (`strdup` 等)。`owned_return` は `string` 戻り値の fn にしか付けられない (型チェック)
- 文字列内の NUL バイトは最初の出現で切り捨て (C のセマンティクスに合わせる)
- ライブラリ名は **2 形式** 受け付け:
  - **Bare name** (`"m"` / `"sqlite3"` 等、`.` `/` `\` を含まない): OS の規約に従って自動補完。macOS = `lib{name}.dylib` / `{name}.dylib`、Linux = `lib{name}.so` → `lib{name}.so.6` → `…so.0` の順で試行、Windows = `{name}.dll` / `lib{name}.dll`
  - **Literal filename** (`"libc.dylib"` / `"libm.so.6"` / `"./build/foo.so"`): そのまま `dlopen` (補完なし)。バージョン固定や絶対パスを使いたい場合に
- ライブラリ open 失敗 / シンボル未存在 はコンパイル時 (JIT 構築時) エラー
- **JIT のみ** 対応 — interpreter からは呼び出せない (`@extern("libname")` 付きの fn を interpreter で実行すると未定義シンボルとして失敗)
- 同じライブラリは 1 回だけ `dlopen` され、ハンドルは JIT モジュールが生きている間維持される

例:
```rust
// Bare name — クロスプラットフォーム
@extern("m") fn sqrt(x: f64): f64
@extern("c") fn strlen(s: string): i64
@extern("c") fn getenv(name: string): string
@extern("c", owned_return) fn strdup(s: string): string

sqrt(81.0)                // 9.0  (macOS は libm.dylib、Linux は libm.so.6 等)
strlen("hello")           // 5
getenv("HOME")            // "/Users/..."         (静的、free しない)
strdup("copy me")         // "copy me"            (libc::free 自動)

// Literal — 特定のバージョン / パスを固定したい場合
@extern("./build/mylib.dylib") fn my_fn(): i64
```

#### `@extern("libname") class Foo {}` — opaque ハンドル型

C ライブラリが返す **不透明なハンドル** (`FILE*`、`sqlite3*` など) を ilang から型安全に保持・受け渡しするための仕組み。

```rust
@extern("c") class FILE {}
@extern("c") fn tmpfile(): FILE?
@extern("c") fn fclose(stream: FILE)

if let some(f) = tmpfile() {
    fclose(f)
}
```

- 本体は **空 or `deinit { ... }` のみ** 許される。フィールド・init・継承・型パラメータ・property などは不可
- `new Foo(...)` は **禁止** (型エラー) — 値は extern fn の戻り値からしか得られない
- 型は名前で区別される: `FILE` と `Sqlite3` は別の型なので `fclose(sqlite_handle)` は型エラー
- 現状 **JIT のみ** 対応 (native extern と同じ制約)

**deinit 無し** (`class FILE {}`):
- ABI 上は **i64 の C ポインタそのもの** (ilang 側のヘッダ無し)。`Foo?` の `none` は `NULL` に対応
- ARC 対象外 — retain/release は走らない。**解放は呼び出し側の責任** (`fclose(handle)` などを明示的に呼ぶ)
- 低レベル FFI で「ハンドルは別の C コードが管理する」場合に向く

**deinit 付き** (`class FILE { deinit() { fclose(this) } }`):
- ilang が **1 スロットの ARC ボックス** で C ポインタを包む。型 `FILE` の値はそのボックスへのポインタ
- 通常のクラス同様 ARC が動く。最後の参照が消えると `deinit` が自動実行 (RAII)
- ネイティブ extern 境界で **自動ラップ/アンラップ**: `tmpfile()` の戻り値はラップ、`fclose(this)` の引数はアンラップして C に渡す
- `deinit` 本体に書くべきは「C 側ハンドルの解放呼び出し」のみ (例: `fclose(this)`、`sqlite3_close(this)`)

### 組み込み `math` モジュール

```rust
use math
math.sqrt(16.0)              // 4.0
math.sin(math.pi / 2.0)      // 1.0  ← `math.pi` は const、parens 不要
math.pow(2.0, 10.0)          // 1024.0
math.atan2(1.0, 1.0)         // π/4
```

提供関数 (すべて f64): `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sqrt`, `pow`, `exp`, `ln`, `log10`, `log2`, `floor`, `ceil`, `round`, `abs`。定数: `pi`, `e` (`const` 宣言で同梱)。interpreter / JIT 両対応。

### 組み込み `test` モジュール

自己アサーションのスクリプト + 結合テストフィクスチャ用。失敗時は stderr にメッセージを出して **exit code 2** で終了する。

```rust
use test
test.expect(1 + 2 * 3, 7)              // i64 同士
test.expectStr("ab" + "c", "abc")      // string 同士
test.expectBool(false, false)
test.expectF64(2.5 + 0.5, 3.0)
test.expectTrue(1 < 2)                 // 単一条件
test.expectFalse(1 > 2)
test.fail("should not reach here")    // 強制失敗
```

interpreter / JIT 両対応。`crates/ilang-cli/tests/programs/*.il` に置いた `.il` ファイルは、ハーネス (`programs.rs`) が両方で実行 + 終了コードを比較してくれる。

### 組み込み `os` モジュール

OS レベルの状態にアクセスするための薄いヘルパー。errno の読み書きと、POSIX 標準のエラーコード定数を提供。

```rust
use os
@extern("c") class FILE {}
@extern("c") fn fopen(path: string, mode: string): FILE?

if let some(f) = fopen("/missing", "r") {
    // 成功時の処理
} else {
    let code = os.errno()
    if code == os.ENOENT {
        // ファイルが見つからない
    } else if code == os.EACCES {
        // 権限エラー
    }
}
```

**関数:**
- `os.errno(): i32` — 現在のスレッドの `errno` を返す (Windows では `GetLastError()`)。失敗を示す値 (NULL / -1 / 0 など) を返した libc 呼び出しの直後に読むのが慣例
- `os.setErrno(code: i32)` — errno を上書き。`os.setErrno(0)` で「次の呼び出し失敗を確実に検出」したい時の前処理に
- 値はエラーが起きるまで持続する。次の libc 呼び出しが成功してもクリアされない (POSIX 仕様)

**定数 (i32)**: `EPERM`, `ENOENT`, `ESRCH`, `EINTR`, `EIO`, `ENXIO`, `E2BIG`, `ENOEXEC`, `EBADF`, `ECHILD`, `ENOMEM`, `EACCES`, `EFAULT`, `EBUSY`, `EEXIST`, `EXDEV`, `ENODEV`, `ENOTDIR`, `EISDIR`, `EINVAL`, `ENFILE`, `EMFILE`, `ENOTTY`, `ETXTBSY`, `EFBIG`, `ENOSPC`, `ESPIPE`, `EROFS`, `EMLINK`, `EPIPE`, `EDOM`, `ERANGE`。値は macOS / Linux glibc で一致するもののみ収録。プラットフォームで値が異なる `EAGAIN` / `EWOULDBLOCK` / `ENOTSUP` 等は意図的に含めず、必要なら `@extern("c")` から直接呼ぶか、ハードコードを推奨

interpreter / JIT 両対応 (Rust の C runtime の errno を直接読み書きする実装で共通)

### `const` (定数宣言)

トップレベルで不変の定数を宣言できます。RHS には **コンパイル時に値が決まる式** が書けます。loader の inline pass で folding され、参照箇所はリテラルに置換されます。

```rust
const TWO: i64 = 2
const N: i64 = 1 + 2 * 3            // 7 (算術)
const TWO_N: i64 = N * 2            // 14 (前の const を参照)
const HELLO: string = "Hi, " + "World"
const FLAG: bool = !(1 == 2) && (3 < 5)
const MASK: i64 = 0xFF & 0x3C       // 60
const HALF: f64 = 1.0 / 2.0

fn double(n: i64): i64 { n * TWO }
double(21)                          // 42
```

- 使える演算: 算術 (`+ - * / %`)、ビット (`& | ^ << >> ~`)、比較 (`== != < <= > >=`)、論理 (`&& || !`)、文字列連結 (`+`)、`as` キャスト (数値間)
- 使える参照: 同じファイルで宣言済の他の `const` のみ (宣言順で folding するので前方参照は不可)
- **使えない**: 関数呼び出し、フィールド/メソッド、配列、`new`、`if`/`match`、ループなどランタイム必要な式
- 0 除算など folding 中のエラーは **コンパイル時エラー** になる
- 型注釈 (`: T`) は省略可 (推論)
- 同梱 `math` モジュールの `pi` / `e` はこの仕組みで定義されている
- モジュール越しに `math.pi` のように参照可能 (loader が `math.pi` という qualified 名で扱う)

---

## 14. コメント

```rust
// 行コメント
/* ブロックコメント */
/* ネストできる: /* outer /* inner */ outer */ も OK */
```

---

## 15. ASI (自動セミコロン挿入)

- 改行 (LF または CRLF) と `;` と `}` `EOF` がいずれも文の終わりとして受理される。
- 式の途中の改行は無視される: `let x = 1\n + 2` は `let x = 1 + 2`。
- クラスメンバーの宣言間は **必ず改行か `;`** が必要 (同一行に並べると ASI が効かない)。

---

## 16. 実行モデル

| モード | コマンド | 特徴 |
| --- | --- | --- |
| ツリーウォーク | `ilang run path.il` | 全機能サポート、起動が速い |
| Cranelift JIT | `ilang run --jit path.il` | ネイティブコード、interpreter の数十〜数百倍速いが一部機能制限あり |
| REPL | `ilang` (引数なし) | 1 行ずつ評価、`let`/`fn`/`class` が永続化、interpreter のみ |

JIT で `Unsupported` になる主なケース:
- ネイティブ extern (`@extern("libname")`) は **JIT 専用** — interpreter からは "no extern handler" エラー
- 静的フィールドの **`string` / オブジェクト型** (現状は `i64` / `f64` / `bool` のみ — 継承の vtable とは別の Phase)

---

## 17. 未実装 (今後の TODO)

- **`?` 演算子** (Result short-circuit。`let v = parse(s)?` で `Result.err` を即 return)
- **文字列補間** (バッククォート + `${expr}` などのテンプレート構文)
- **Iterator プロトコル** (ユーザ型に `next()` を実装させて `for-in` に乗せる)
- **名前付き引数** (`open(path: "x", mode: "w")`) — デフォルト引数は実装済み、名前付き呼び出しは未実装
- **演算子オーバーロード** (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- **Trait / Interface** (型シェイプによる抽象化)
- **デストラクチャリング** (`let (a, b) = pair` / `let { x, y } = point`)
- **Async / await** (並行性)
- **ジェネリック制約 (bounds)**
- **継承の階層メソッドオーバーロード** (現状はルートクラスのみオーバーロード可)
- **静的フィールド/メソッドの継承** (Phase 2)
- **ジェネリッククラスでの継承 / 静的メンバー / プロパティ** (型パラメータ解決の制約により未対応)

### 採用しない方針

- **例外 (`throw` / `try` / `catch`)**: 採用しない。失敗するかもしれない関数は `Result<T, E>` で表現し、`match` で処理する。回復不能なバグ (ゼロ除算、配列範囲外、`unwrap()` on `none`) は **panic** として実行を停止 (catch 不可)。
  - 理由: 制御フローがシグネチャに現れる、型システムを抜けない、ARC との相性。Rust / Go / Zig などと同じ方針。

---

詳細な内部設計やフェーズごとの経緯は [`HANDOFF.md`](HANDOFF.md) と `docs/phaseN-plan.md` 参照。
