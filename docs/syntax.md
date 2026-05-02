# ilang 構文チートシート

実装済みの構文を一覧で示します。各項目は実際にパース・型チェック・実行が通る形のみ。

`.il` ファイルを `cargo run -p ilang-cli -- run path.il` (ツリーウォーク) または `... run --jit path.il` (Cranelift JIT) で実行できます。引数なしで起動すると REPL に入ります。文末セミコロン `;` は省略可で、改行が文の区切りになります (JS 風 ASI)。

---

## 1. リテラル

| 種類 | 例 | 自然な型 |
| --- | --- | --- |
| 整数 | `42`, `-7`, `0xff`, `0b1011`, `1_000_000` | `i64` |
| 整数 (型サフィックス) | `1_i32`, `255_u8`, `0xffff_u16` | サフィックスの型 |
| 浮動小数 | `3.14`, `1.5e10`, `2.5_f32` | サフィックスがあればその型、無ければ `f64` |
| bool | `true`, `false` | `bool` |
| 文字列 | `"hello"`, `"line\nbreak"` (`\n` `\t` `\r` `\\` `\"` `\0`) | `string` |
| Unit | `()` (式で生まれる、自前で書かない) | `()` |
| Optional | `none`, `some(x)` | `T?` |
| 配列 | `[1, 2, 3]`, `[1, 2, 3,]` (末尾コンマ可) | `T[]` |
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

### 文字列の組み込みメソッド (JS 風)

```rust
"hello".length              // i64 — Unicode コードポイント数 ("あいう".length == 3)
"hello".charAt(1)           // string — 1 文字。範囲外は ""
"hello".includes("ell")     // bool
"hello".startsWith("he")    // bool
"hello".endsWith("lo")      // bool
"Hi".toUpperCase()          // string
"Hi".toLowerCase()          // string
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

// for-in (配列を回す)
let xs: i64[] = [10, 20, 30]
for x in xs { console.log(x) }     // break / continue 可

// if let — Optional のパターンマッチ (`match` 以外で使える唯一の pattern 形)
let x: i64? = some(42)
if let some(v) = x {
    // v: i64 として使える
} else {
    // none ケース
}
```

`break` / `continue` はループ内のみ (型チェッカーが範囲外を拒否)。

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

### ファーストクラス関数

関数は値として変数に代入したり、引数や戻り値として渡せます。**クロージャ (キャプチャ) は未対応** — 匿名関数の本体からは外側のローカル変数を参照できません。

```rust
fn add(a: i64, b: i64): i64 { a + b }
let f = add                          // 関数値を代入 (型は fn(i64, i64): i64)
f(2, 3)                              // 5

// 匿名関数 (即値) — 既存 fn 構文から名前を抜いた形
let inc = fn(x: i64): i64 { x + 1 }
inc(41)                              // 42

// 関数を引数に取る/返す
fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
fn double(n: i64): i64 { n * 2 }
apply(double, 7)                     // 14

fn make_inc(): fn(i64): i64 { fn(x: i64): i64 { x + 1 } }
```

- 関数型: `fn(T1, T2): R` (戻り値が `()` なら `: R` 省略可)
- ローカル `let f = some_fn` は同名のトップレベル fn より優先 (シャドーイング)
- 匿名関数本体は **自分のパラメータ** と **トップレベル fn / クラス / enum** しか参照できない (キャプチャ不可)
- JIT 対応済 (`func_addr` + `call_indirect`)。匿名関数は内部でトップレベルにホイストされる

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
- 継承・`static`・`private` はスコープ外 (未実装)。
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

高階メソッドもサポート: `xs.map(fn)` / `xs.filter(pred)` / `xs.forEach(fn)` / `xs.slice(start, end)`。コールバックは **第一級関数** (名前参照または匿名 `fn`) を渡す形 — クロージャ未対応のため fn 本体は外側のローカル変数を参照不可。`length` / `push` / `pop` / `indexOf` / `includes` / `for-in` 含めて **interpreter / JIT とも同等** — 要素型の制限はありません。

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
- 現状ユーザ定義 extern は無対応 — 内部メカニズムとして `math` モジュールに使われている

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

### `const` (定数宣言)

トップレベルで不変の定数を宣言できます。RHS は **リテラル限定** (数値 / bool / 文字列、単項 `-` と `as` キャスト可)。複雑な式は受け付けません。

```rust
const TWO: i64 = 2
const NAME: string = "alice"
const TWO_PI: f64 = 6.283185307179586     // 計算式は不可。値を書く

fn double(n: i64): i64 { n * TWO }
double(21)                                 // 42
```

- 型注釈 (`: T`) は省略可 (リテラルから推論)
- ローダの inline pass で参照箇所が **コンパイル時に値に置換** される (実行時のルックアップなし)
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
- 継承 / 動的ディスパッチ (interpreter にも未実装)

---

## 17. 未実装 (今後の TODO)

- 継承 (`extends`, `super`)
- 文字列補間 (バッククォート + `${expr}` などのテンプレート構文)
- ジェネリック制約 (bounds)
- クロージャ (関数のキャプチャ。ファーストクラス関数 + 匿名関数のキャプチャなしは実装済 — §6 参照)
- Rust 風 `?` 演算子 (Result の早期 return — エルゴノミクス向上、いつか追加するかも)

### 採用しない方針

- **例外 (`throw` / `try` / `catch`)**: 採用しない。失敗するかもしれない関数は `Result<T, E>` で表現し、`match` で処理する。回復不能なバグ (ゼロ除算、配列範囲外、`unwrap()` on `none`) は **panic** として実行を停止 (catch 不可)。
  - 理由: 制御フローがシグネチャに現れる、型システムを抜けない、ARC との相性。Rust / Go / Zig などと同じ方針。

---

詳細な内部設計やフェーズごとの経緯は [`HANDOFF.md`](../HANDOFF.md) と `docs/phaseN-plan.md` 参照。
