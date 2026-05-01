# ilang 構文チートシート

実装済みの構文を一覧で示します。各項目は実際にパース・型チェック・実行が通る形のみ。

`.il` ファイルを `cargo run -p ilang-cli -- run path.il` (ツリーウォーク) または `... run --jit path.il` (Cranelift JIT) で実行できます。文末セミコロン `;` は省略可で、改行が文の区切りになります (JS 風 ASI)。

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
```

後置修飾子 `[]` `[N]` `?` `.weak` は重ねられる: `Foo[]?`, `User?[]`, `Node.weak?` 等。`.weak` は `ClassName.weak` の形のみ (string や i64 には付けられない)。

### 暗黙の型変換

| from | to | 暗黙? |
| --- | --- | --- |
| 同符号の整数同士 (狭→広 / 広→狭) | | yes |
| 整数 → 浮動 | | yes |
| `f32` → `f64` / `f64` → `f32` | | yes |
| 符号またぎ (`i32` ↔ `u32` 等) | | **no** (`as` 必須) |
| 浮動 → 整数 | | **no** (`as` 必須) |
| `T` → `T?` (Optional 自動 wrap) | | yes |
| `Foo` → `Foo.weak` (strong → weak 自動 downgrade) | | yes (同一クラスのみ) |

`expr as Type` で明示キャスト。

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
arr[i] = v
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
| 14 | `.` (フィールド/メソッド) / `[]` (添字) | 後置 |

文字列に対しては `+` (連結) と `==`/`!=` (構造的等値) のみ。オブジェクトの `==`/`!=` は同一クラスでの参照等値。`%` は浮動小数では未対応。

---

## 5. 制御フロー

```rust
// if は式
let r = if n > 0 { n } else { -n }
if cond { ... } else if cond2 { ... } else { ... }

// while
while cond { ... }

// loop は break のみで抜けられる
let i = 0
loop {
    if i >= 10 { break }
    if i % 2 == 0 { i += 1; continue }
    i += 1
}

// if let — Optional のパターンマッチ (現状サポートされる唯一の pattern 形)
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
- ジェネリクスは未実装。
- variadic は組み込み (`console.log` のみ) のみ対応。

### Capability アノテーション (パースのみ、enforce は未実装)

```rust
#[requires(net)]
fn fetch(url: string): string { ... }

#[requires(net, file)]
fn download(url: string, path: string) { ... }
```

属性はメソッドにも付けられるが、クラス自体への付与は未対応。

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

- `init` は唯一のコンストラクタ (Swift 風)。引数なし `init() {}` を省略するとデフォルトで引数なし `new` 可。
- `deinit` は引数なし・戻り値 () 限定。明示呼び出し不可 (`c.deinit()` はエラー)。
- 暗黙 `this`: メソッド本体内で `this.` を省略可。ただしローカル変数や引数があればそちら優先。
- 継承・`static`・`private` はスコープ外 (未実装)。
- 同一行に複数のクラスメンバーは書けない (ASI が効かないので `;` か改行必須)。

---

## 8. 配列

```rust
let xs: i32[] = [10, 20, 30]    // 動的配列リテラル
let ys: i32[3] = [1, 2, 3]      // 固定長 (要素数も型に固定)
let zs: i32[] = []              // 空配列は注釈必須

xs[1]                            // 添字読み取り
xs[0] = 100                      // 添字代入
xs.length                        // i64 を返す (組み込み)
xs.push(40)                      // 動的配列のみ。固定長は型エラー
```

`pop` / `slice` / `for-of` などの配列メソッド・`map`/`filter` は未実装。

---

## 9. Optional

```rust
let a: User? = some(user)        // 通常の構築
let b: i64? = none               // 不在
let c: i64? = 7                  // T → T? 自動 wrap

if let some(v) = a {             // パターン分岐
    use(v)
}

a.is_some()                      // bool
a.is_none()                      // bool
a.unwrap()                       // T (none ならランタイム panic)
```

- `T?` の T は実行系で扱える任意の型 (interpreter)。**JIT は heap 内部 (Object/Str/Array/Weak) に限定** — `i64?` などの primitive Optional は JIT で `Unsupported` になる (interpreter は問題なく動く)。
- `T?` を関数の引数・戻り値・フィールド型にも使える。
- `none` は単独では型不定 — 文脈の Optional 型から推論される。

---

## 9b. enum / match

```rust
// Phase 1: 単純な C 風列挙型
enum Color { Red, Green, Blue }

let c = Color::Green
let name = match c {
    Color::Red => "red"
    Color::Green => "green"
    Color::Blue => "blue"
}

// Phase 2: ペイロード付き (タプル / 名前付きフィールド)
enum Shape {
    Circle(f64)
    Rect(f64, f64)
    Square { side: f64 }
}

fn area(s: Shape): f64 {
    match s {
        Shape::Circle(r) => 3.14 * r * r
        Shape::Rect(w, h) => w * h
        Shape::Square { side } => side * side    // shorthand: { side: side }
    }
}

// `_` ワイルドカードで残りを捕捉
let day = Color::Red
match day {
    Color::Red => "alert"
    _ => "ok"
}
```

- 全バリアントを網羅するか `_` が必要 (型チェッカが拒否)
- 各 arm 値は型が揃う必要あり (if/else と同じ)
- パターンの束縛: タプルは位置 (`Shape::Circle(r)`)、struct は名前 (`{ side }` または `{ side: s }`)、`_` で無視
- ペイロード内の heap 型 (Object / Str / Array / Optional / Weak / 別 enum) は ARC で正しく解放される

## 10. Weak (弱参照)

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

## 11. console (組み込み)

```rust
console.log(1, "hello", true)        // variadic、空白区切り、末尾改行
console.log()                        // 改行のみ
```

- `console` は予約識別子で、ユーザが `let console = ...` や同名クラスを定義するとエラー。
- 引数の型は混在可。Object 型は JIT では未対応 (interpreter は文字列化して出力)。

---

## 12. コメント

```rust
// 行コメント
/* ブロックコメント */
/* ネストできる: /* outer /* inner */ outer */ も OK */
```

---

## 13. ASI (自動セミコロン挿入)

- 改行 (LF または CRLF) と `;` と `}` `EOF` がいずれも文の終わりとして受理される。
- 式の途中の改行は無視される: `let x = 1\n + 2` は `let x = 1 + 2`。
- クラスメンバーの宣言間は **必ず改行か `;`** が必要 (同一行に並べると ASI が効かない)。

---

## 14. 実行モデル

| モード | コマンド | 特徴 |
| --- | --- | --- |
| ツリーウォーク | `ilang run path.il` | 全機能サポート、起動が速い |
| Cranelift JIT | `ilang run --jit path.il` | ネイティブコード、interpreter の数十〜数百倍速いが一部機能制限あり |
| REPL | `ilang` | 1 行ずつ評価、`let`/`fn`/`class` が永続化、interpreter のみ |

JIT で `Unsupported` になる主なケース:
- `i64?` / `bool?` などの **primitive Optional** (Object/Str/Array/Weak の Optional は OK)
- 継承 / 動的ディスパッチ (interpreter にも未実装)

---

## 15. 未実装

- 継承 (`extends`, `super`)
- 辞書/Map 型
- `for-of` ループ
- 文字列メソッド (`.length`, `.charAt`, 補間)
- 配列メソッド (`pop`, `slice`, `map` 等)
- モジュール / `use` / インポート
- 例外 / `throw` / `try`
- ジェネリクス

---

詳細な内部設計やフェーズごとの経緯は [`HANDOFF.md`](../HANDOFF.md) と `docs/phaseN-plan.md` 参照。
