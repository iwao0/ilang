# ilang 構文チートシート

[English](syntax.md) | 日本語

実装済みの構文を一覧で示します。各項目は実際にパース・型チェック・実行が通る形のみ。

`.il` ファイルを `cargo run -p ilang -- run path.il` (AST→MIR→Cranelift JIT) で実行できます。引数なしで起動すると REPL に入ります (incremental MIR JIT で fn / class / enum / 多くの top-level let が chunk 間で永続)。文末セミコロン `;` は省略可で、改行が文の区切りになります (JS 風 ASI)。

---

## 予約語

```
as        break     class     const     continue  elif      else      enum
false     fn        for       if        in        interface is        let
loop      match     new       none      override  pub       return    some
super     this      true      use       while
```

これらは予約語で、変数 / 引数 / フィールド / 関数 / クラス名には使えません。

**例外**: 上記の予約語は **すべて** enum の variant 名および `obj.<name>` のメンバアクセス位置で使えます (宣言、`Enum.<name>` アクセス、match の短縮形 / 修飾形パターン、`instance.method()` など)。C / Cocoa ヘッダ由来の enum (`SDL_HINT_OVERRIDE`, `SKRepeatMode.loop`, `SDL_SCANCODE_RETURN`, `SDL_FALSE` / `SDL_TRUE` など) と衝突しないようにするための実用上の配慮です。

**文脈依存キーワード** — 特定位置でのみキーワード扱い、それ以外では通常の識別子:

| 単語 | キーワード扱いになる場所 |
| --- | --- |
| `static` | クラス本体内のメンバ修飾子 |
| `get` / `set` | クラス内のプロパティ getter / setter 宣言 |
| `weak` | 型位置のサフィックス `ClassName.weak` |

**予約識別子** — variant 名・フィールド名としては使えるが、トップレベルで上書きするとエラー:

- `console` — 組み込みシングルトン
- `Result` — 組み込み 2-variant enum

## 1. リテラル

| 種類 | 例 | 自然な型 |
| --- | --- | --- |
| 整数 | `42`, `-7`, `0xff`, `0o755`, `0b1011`, `1_000_000` | `i64` |
| 整数 (型サフィックス) | `1_i32`, `255_u8`, `0xffff_u16` | サフィックスの型 |
| 浮動小数 | `3.14`, `1.5e10`, `2.5_f32` | サフィックスがあればその型、無ければ `f64` |
| bool | `true`, `false` | `bool` |
| 文字列 | `"hello"`, `"line\nbreak"` (`\n` `\t` `\r` `\\` `\"` `\0`) | `string` |
| Unit | *値位置のリテラル形なし* — 戻り値型のない関数、空ブロック `{}`、値なし `return` からだけ生まれる | `()` |
| Optional | `none`, `some(x)` | `T?` |
| 配列 | `[1, 2, 3]`, `[1, 2, 3,]` (末尾コンマ可) | `T[]` (動的) — 固定長にしたい場合は `T[N]` を注釈 |
| タプル | `(1, "hello")`, `(true, 3.14, [1,2])` | `(T1, T2, ...)` (2 要素以上) |
| Map | `{"a": 1, "b": 2}` | `Map<K, V>` |

---

## 2. 型

```text
i8  i16  i32  i64
u8  u16  u32  u64
f32  f64
bool  string
()                  // Unit — 型注釈専用 (値位置に `()` リテラルは書けない)
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
| `Foo` → `Bar.weak` (strong → weak 自動 downgrade) | `Foo` が `Bar` か `Bar` のサブクラスなら yes |

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

### `let` のデストラクチャリング

```rust
// タプル — フラット、`_` でスロット無視。
let pair: (i64, string) = (42, "hi")
let (n, s) = pair                       // n: i64, s: string
let (_, only_b, _) = (1, 2, 3)          // 他を無視

// オブジェクト (構造体) — Rust 風にクラス名を書く。フィールド名は一致必須、
// リネームや rest は v1 では未対応。
class Point { x: f64; y: f64; init(a: f64, b: f64) { this.x = a; this.y = b } }
let p = new Point(1.0, 2.0)
let Point { x, y } = p                  // x: f64, y: f64
```

- 対応箇所は **`let` 文のみ** (関数引数・`for-in` の分解は今後)
- タプル分解は **2 スロット以上** が必須 (1 個なら通常の `let`)
- ネスト (`let ((a, b), c) = ...`) は未対応

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

文字列に対しては `+` (連結) と `==`/`!=` (構造的等値) のみ。オブジェクトの `==`/`!=` は同一クラスでの参照等値。関数 (`fn(...)`) も同じシグネチャ同士で `==`/`!=` を比較できる (クロージャポインタの参照等値 — 別々の `let f = fn(...)` は常に不一致)。`%` は浮動小数では未対応。

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
"abc".concat("de")          // string    ─ "abcde"
"hello".indexOf("l")        // i64       ─ 最初の出現位置 (コードポイント単位)、見つからなければ -1
"hello".indexOf("l", 3)     // i64       ─ 第2引数 fromIndex は省略可 (既定 0)
"abcabc".lastIndexOf("b")   // i64       ─ 最後の出現位置、見つからなければ -1
"abcabc".lastIndexOf("b", 2)// i64       ─ 省略時 fromIndex は文字列末尾
"hi".encodeUtf16()          // u16[]     ─ UTF-16 コードユニット、末尾に 0x0000 を含む (既定)
"hi".encodeUtf16(false)     // u16[]     ─ 終端 0x0000 を含まない、コードユニットのみ
"hello".hashCode()          // i64       ─ FNV-1a 64-bit (バイト列ベース、runs をまたいで安定)
string.fromUtf16([104, 105])// string    ─ "hi" (UTF-16 コードユニット → UTF-8 文字列)
```

`encodeUtf16(nulTerminated?)` は Win32 W系 API への橋渡し用。
既定の `true` で末尾に `0x0000` が付くので、§22 で説明する
`u16[] → *const u16` 暗黙コアースと合わせて `SetWindowTextW` /
`CreateWindowExW` 等にそのまま渡せる。JS の `encodeUtf16` 相当
(終端なし) が要るときは `false` を渡す。

`string.fromUtf16(units)` は逆方向の静的ファクトリ。`u16[]` を
そのまま全部デコードするため、末尾の `0x0000` も U+0000 として
文字列に残る。`encodeUtf16()` (既定で終端あり) と厳密に round-trip
させたいときは `encodeUtf16(false)` を渡しておく。サロゲートの
片割れなど無効な UTF-16 は U+FFFD に置換される (Rust の
`String::from_utf16_lossy` 相当)。


### テンプレートリテラル (文字列補間)

バッククォートで囲んだリテラルは `${expr}` で値を埋め込めます。
式の型は任意 — プリミティブ・文字列・配列・タプル・Optional・enum・
クラスインスタンス・Map・弱参照・クロージャまで `console.log` と
同じ書式で文字列化されます。`"..."` リテラルは従来通り補間しません。

```rust
let name = "world"
let n = 42
`hello ${name}!`              // "hello world!"
`n=${n} half=${n / 2}`        // "n=42 half=21"
`tuple=${(1, "two", true)}`   // "tuple=(1, two, true)"
`arr=${[1, 2, 3]}`            // "arr=[1, 2, 3]"
`opt=${some(7)}`              // "opt=some(7)"
`multi
line ok`                      // 改行はそのまま
```

バッククォート内のエスケープは `` \` ``, `\${`, `\\`, `\n`, `\t`,
`\r`, `\0`, `\xNN`, `\u{NNNN}` — `"..."` と同じ集合。タグ付き
テンプレート (`` tag`...` ``) は未サポート。

### 数値・bool の組み込み `.toString()`

```rust
(42).toString()             // "42"
(-7).toString()             // "-7"
(true).toString()           // "true"
(3.14).toString()           // "3.14"
(1.0).toString()            // "1.0"  — 整数値の浮動小数は `.0` 付き (JS 風)
let n: u8 = 255
n.toString()                // "255"
```

すべての数値プリミティブ (`i8`〜`u64` / `f32` / `f64`) と `bool` で利用可。浮動小数のフォーマットは `console.log` と同じ。

### 全プリミティブの組み込み `.hashCode(): i64`

```rust
(42).hashCode()             // 42 (符号拡張)
(255u8).hashCode()          // 255 (ゼロ拡張)
(-1i8).hashCode()           // -1 (符号拡張)
true.hashCode()             // 1
1.5f32.hashCode()           // 1.5f32 のビットパターンを i64 に符号拡張
1.5.hashCode()              // 1.5f64 のビットパターンを i64 に再解釈
"foo".hashCode()            // FNV-1a 64-bit (string メソッド節を参照)
```

全数値プリミティブ・`bool`・`string` が `hashCode(): i64` を持つ。整数 / bool は宣言された符号性に従って拡張、浮動小数はビットパターンを再解釈するので、別ビットパターンの NaN は別ハッシュになる (`Set<f64>` の dedup と整合)。`@derive(Hash)` のフィールド経路もすべてこのメソッド経由なので、プリミティブと `string` は同じ入り口を通る。

### 整数の associated 定数

```rust
i8.Min                     // i8  — -128
i8.Max                     // i8  — 127
i32.Min                    // i32 — -2_147_483_648
i32.Max                    // i32 — 2_147_483_647
u8.Max                     // u8  — 255
u64.Max                    // u64 — 18_446_744_073_709_551_615
```

`i8`〜`u64` のすべてに `Min` / `Max` を提供。符号なし整数の `Min` は `0`。`i64.Min` / `i64.Max` は Rust の `i64::MIN` / `i64::MAX` と同値。

### 浮動小数の述語メソッド

```rust
let f: f64 = 1.0
f.isFinite()              // bool — 有限の実数のとき true
f.isNaN()                 // bool — NaN のときのみ true (NaN == NaN は常に false)
(f32.NaN).isNaN()         // true
(f64.Infinity).isFinite() // false
```

`.isFinite()` と `.isNaN()` は `f32` / `f64` のみ提供。整数は構造上 NaN や無限大にならないため、メソッドを公開していません。

### 浮動小数の associated 定数

```rust
f32.NaN                    // f32 — NaN リテラル (NaN != NaN)
f32.Infinity               // f32 —  ∞
f32.NegInfinity            // f32 — -∞
f32.Min                    // f32 — 最も負の有限値 (≈ -3.4e38)
f32.Max                    // f32 — 最大の有限値 (≈ 3.4e38)
f32.MinPositive            // f32 — 最小の正の normal 値 (≈ 1.17e-38)
f32.Epsilon                // f32 — 1.0 と次に表現可能な値との差 (≈ 1.19e-7)
```

`f64` も同名 7 個を持ち、型がより広い。値は Rust の `f32::NAN` / `f64::NAN` などをそのまま採用するため IEEE-754 準拠。`Min` は Rust 流で「最も負の有限値」(JS の "最小の正値" ではなく `MinPositive` がその意味になる)。

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
// 上限なし `1..` (RangeFrom) — 本体内で `break` 必須
for i in 1.. { if i > 100 { break }; sum += i }

// if let — Optional のパターンマッチ (`match` 以外で使える唯一の pattern 形)
let x: i64? = some(42)
if let some(v) = x {
    // v: i64 として使える
} else {
    // none ケース
}
```

`break` / `continue` はループ内のみ (型チェッカーが範囲外を拒否)。

範囲式 `a..b` / `a..=b` / `a..` は **`for-in` のイテレータ位置でのみ** 使えます。`let r = 1..10` のように値として保持しようとするのは型エラー。両端ある形では同じ整数型である必要があり、ループ変数はその型にバインドされます。半開 `a..` (RangeFrom) は上限なし — 本体で `break` を入れて抜ける必要があり、整数オーバーフローはラップ (panic なし)。Rust に倣い start のない `..N` / `..` は反復不可として **拒否** されます。

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
- 型検査で得たバインディングを元に、各 (関数, 型引数) ペアごとにモノモルフ化された具象関数を生成する。

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
- `Object → Weak` (同一クラス) = 4。サブクラスを親の `Bar.weak` スロットへ降ろす場合は継承距離も加算 (`4 + steps`)

最良スコアが複数のオーバーロードで同点の場合は **ambiguous エラー** で拒否されます。完全一致するオーバーロードがあれば常に勝つので、よくある「明示的に書いた版が選ばれる」期待と一致します。

**禁止される組み合わせ**:
- 同名でジェネリック関数と非ジェネリック関数を両方宣言: `fn id<T>(x: T): T` と `fn id(x: i64): i64` を同時に書くとエラー (ジェネリック解決パスとオーバーロード解決を混ぜないため)
- 完全に同じシグネチャの重複宣言

**ファーストクラス参照**: オーバーロードされた名前を `let f = name` で参照するのは ambiguous エラー。直接呼び出すか、`fn name__i64` のようにマングル後の名前を使う必要があります (内部実装が漏れるので非推奨)。

型検査後にオーバーロードされた名前は `name__<param_types>` にマングルされ、各呼び出しサイトもそれに合わせて書き換えられます。

### ファーストクラス関数

関数は値として変数に代入したり、引数や戻り値として渡せます。匿名関数の本体は外側のローカル変数を **キャプチャ** できます (すべての型をキャプチャ可能)。クロージャが *読むだけ* のキャプチャは値スナップショット、*代入する* キャプチャは共有されます (下記参照)。

```rust
fn add(a: i64, b: i64): i64 { a + b }
let f = add                          // 関数値を代入 (型は fn(i64, i64): i64)
f(2, 3)                              // 5

// 匿名関数 (即値) — 既存 fn 構文から名前を抜いた形
let inc = fn(x: i64): i64 { x + 1 }
inc(41)                              // 42

// クロージャ — `factor` は読むだけなので値スナップショット
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
- **読むだけのキャプチャは値スナップショット**: クロージャ作成時点の outer 変数の値を retain (ARC型) もしくはコピー (プリミティブ) する。後から outer 変数を再代入してもクロージャの読む値は変わらない (上の `factor` は後で `factor = 999` しても 10 のまま)
- **代入するキャプチャは共有** (JS / Swift 既定): クロージャが capture した変数に *代入* する場合、その変数と同じ名前を capture する全クロージャが 1 つのセルを共有する。よってクロージャ経由の書き込みは外側スコープや兄弟クロージャにも見える。別々の `let` (例: `makeCounter()` を 2 回呼ぶ) はそれぞれ独立したセルを持つ
- パラメータが capture と同名の場合はパラメータが優先 (シャドーイング)
- 同名のトップレベル fn / クラスはキャプチャ対象外 (グローバルな名前として解決)
- すべての型を capture 可能 (i64 / f64 / bool / string / object / array / optional / map)。closure は `[fn_ptr | env_field0 | ...]` 構造体としてヒープに確保し、ARC で管理 (capture が ARC 型なら retain、closure 解放時に自動 release)
- ネストしたクロージャ (closure-of-closure) も対応 — 内側の closure は外側の closure の captures を再 capture できる
- top-level fn を fn 値として参照 (`let f = some_fn`) すると、env を無視してターゲットを呼ぶ trampoline closure が自動生成される
- **自己再帰クロージャ** は明示的な関数型注釈が必須 (`let fib: fn(i64): i64 = fn(n: i64): i64 { ... fib(n - 1) ... }`) — 再帰参照は型推論できないため。注釈があればトップレベル・fn 本体内・入れ子クロージャからの外側クロージャ名参照のいずれでも動く。自己参照は capture ではなく「実行中のクロージャ自身」として解決されるので、参照循環は発生せず、スコープ外へ escape したクロージャも有効なまま

### 属性 / アノテーション (パースのみ、enforce は未実装)

```rust
@requires(net)
fn fetch(url: string): string { ... }

@requires(net, file)
@deprecated(use_v2)
fn download(url: string, path: string) { ... }
```

`@name(args)` 形式 (TS / Java / Python のデコレータ風)。複数並べる場合はそれぞれ `@` から始める。引数リストは省略不可 (`@x` 単独はパースエラー)。属性はメソッドにも付けられる。クラス宣言自身に付与できるのは `@derive(...)` のみで、他の属性はパーサが弾く。

属性名のセットは閉じていて、**未知の属性名はパースエラー**になります
(既知の属性一覧つき)。`@derieve` のような typo が無言で無効になる
ことはありません。既知の属性:

- `@extern(C) { ... }` / `@extern(ObjC) { ... }` のブロックマーカー
- `@extern(C)` 内の FFI 系: `@lib` / `@optional` / `@symbol` / `@packed` / `@bits(N)`
- `@extern(ObjC)` 内の ObjC ブリッジ系: `@objc` / `@objc("selector:")` (optional プロトコルメソッドはメソッド名末尾の `?` で表す。属性ではない)、`@since`、`@com`
- `enum` 宣言に付ける `@flags` (ビットセット意味付け)
- `const` 宣言に付ける `@embed("path/to/file")` (コンパイル時ファイル取り込み)
- `fn` / `class` / メソッドに付ける `@target("os")` — ホスト OS フィルタ (詳細は次節)
- `class` 宣言に付ける `@derive(Eq, Hash)` — `Set<MyClass>` / `Map<MyClass, V>` の値等価プロトコル用に `equals` / `hashCode` を自動合成 (詳細は Map / Set 節)
- `@deprecated(note)`、`@intrinsic`、`@handle`、`@requires(...)` (capability — パースのみ、未 enforce)

(`override` はキーワードであり属性ではありません — Classes 節を参照。)

#### `@target` — OS 別の同名宣言

ビルド時にホストの OS で宣言を絞り込む。Rust の `#[cfg(target_os = "…")]` に対応する。

```rust
@target("macos")
fn fileSeparator(): string { "/" }

@target("windows")
fn fileSeparator(): string { "\\" }

@target("macos", "linux")          // OR — どちらかにマッチで残る
fn isPosix(): bool { true }

@target(not "windows")             // 否定 — 単独の `not "X"` のみ
fn hasFork(): bool { true }
```

- マッチすれば item が残り、`@target` 自体は剥がされる (ホバー / フォーマッタには現れない)
- マッチしないとビルド前に loader が item を捨てる — 型チェッカ・JIT は見ない
- 同じ item に `@target` を複数並べた場合は AND
- `not` 形と他の引数の混在は禁止 (AND が必要なら `@target` を分けて書く)
- OS 名は `os.platform` と同じ集合: `"macos"` / `"linux"` / `"windows"` / `"other"`
- 適用先は `fn` (トップレベル / メソッド / 静的メソッド / `@extern(C)` 内 `FnDef`) と `class` (トップレベル / `@extern(C)` 内)
- 同名の宣言を OS で振り分けた場合、フィルタ後に1つしか残らなければ重複扱いされない。複数残ると既存の重複検出でエラー

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
- `deinit` は引数なし・戻り値 () 限定。明示呼び出し不可 (`c.deinit()` はエラー)。継承チェーンでは Swift と同様に自動で連鎖する: 最派生クラスの `deinit` が先に走り、続いて各祖先の `deinit` が順に走る (`super.deinit()` のような構文は無く、書く必要もない)。自前の `deinit` を持たないクラスは連鎖を切らずにスキップされる。
- 暗黙 `this`: メソッド本体内で `this.` を省略可。ただしローカル変数や引数があればそちら優先。
- 継承 (`class Child: Parent`) / インタフェース (`class Foo: Iface1, Iface2`) / `static` / `get`/`set` プロパティは下記の節で詳述。`private` 修飾子は未実装。
- 同一行に複数のクラスメンバーは書けない (ASI が効かないので `;` か改行必須)。

#### フィールドのデフォルトと init 必須化

ランタイムで安全な「空の値」を持てるフィールドは `new` 時に自動的に zero-init され、`init` での代入を省略できます:

| 型 | デフォルト |
| --- | --- |
| `i8`..`u64`, `f32`, `f64` | `0` |
| `bool` | `false` |
| `string` | `""` |
| `T?` | `none` |
| `T[]` (動的配列) | `[]` |
| `T[N]` (固定長) | 各要素のデフォルト |
| `T.weak` | dead weak |

それ以外のヒープ型 (`Object` 参照、`Map<K, V>`、関数値、タプル) は安全なデフォルトを持たないため、**すべての `init` オーバーロードで代入する必要があります**。漏れていると型チェック時に明確なエラーが出ます。代入できない場合は `T?` でラップして `none` をデフォルトにできます。

#### struct リテラルによる構築 (`Name { field: value, ... }`)

**値型専用** の field-named 構築 — トップレベル `struct` / `union` (および `@extern(C)` 版) のみで使える。通常の ARC クラスでは利用不可: クラスの `init` で保つべき不変条件をリテラルがスキップしてしまうため、型チェッカが `MyClass { ... }` を拒否する。クラスは `new MyClass(...)` で構築する。

```rust
struct Point {
    x: i32
    y: i32
}
let p = Point { x: 3, y: 4 }             // OK — 値型 struct

union Value {
    i: i64
    f: f64
}
let v = Value { i: 42 }                  // OK — ちょうど 1 フィールド
```

- **トップレベル / `@extern(C)` の `struct`**: **全フィールド**を初期化する必要がある — `init` で補える仕組みが無いため、省略すると黙って zero-init されてしまうのを防ぐ
- **トップレベル / `@extern(C)` の `union`**: **ちょうど 1 つ**のフィールドだけ初期化 (variants が同じストレージスロットを共有するため)
- リテラル中のフィールド順序は問わない (チェックされるのは名前集合のみ)
- 同じリテラル内でフィールド名が重複した場合はエラー
- クラスの構築はあくまで `new ClassName(...)` で行う — クラス名をリテラル形式に渡すと型エラー

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
- 関数のジェネリクスは [§6 ジェネリック関数](#ジェネリック関数) 参照
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
- 型検査後にオーバーロードされたメソッドは `name__<param_types>` にマングルされ、`new C(...)` の AST には選ばれた `init_method` が記録されます。

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

- アリティ・パラメータ型でオーバーロード可能。解決とマングリングのルールは [§6 関数のオーバーロード](#関数のオーバーロード) およびインスタンスメソッドのオーバーロードと同じで、型検査後に `name__<param_types>` にマングルされる
- フィールド / インスタンスメソッド / プロパティと同名は不可
- ジェネリッククラスでの静的メソッドは未対応 (型パラメータが静的コンテキストで参照できないため)
- `static` はクラス本体内のみキーワード (contextual keyword)
- ローカル変数 `let Vec2 = ...` がある場合はそちら優先 (シャドウ)

#### `static` フィールドと `const` 定数

`static name: T = const_expr` でクラスレベルの可変ストレージを宣言できます。`const name: T = const_expr` は同じストレージを **immutable** として宣言し、再代入は型エラーになります。どちらも `ClassName.field` で読みます。

```rust
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1; Counter.total = Counter.total + 1 }

    static total: i64 = 0
    static threshold: i64 = 1 + 2 * 5      // 11 (const 折りたたみ)
    const max: i64 = 1000                  // immutable; Counter.max = ... は型エラー
}

let a = new Counter(); let b = new Counter()
a.bump(); a.bump(); b.bump()
Counter.total              // 3
Counter.max                // 1000
```

- 型は **`i64` / `f64` / `bool`** のみ (Phase 1)。string / オブジェクトなどヒープ型は ARC 設計が確定するまで未対応
- 初期値は **コンパイル時定数式** 限定 (top-level `const` と同じ folder で評価)。関数呼び出し等のランタイム式は不可
- `static` は mutable (`Counter.total = 100` で書き換え可)
- `const` は immutable (`Counter.max = 100` は型エラー)
- 同名の他フィールド・メソッド・プロパティ・静的メソッドとは衝突不可
- ジェネリッククラスでの静的フィールドは未対応 (静的メソッドと同じ理由)
- 内部実装: JIT は `Box<[i64]>` を確保してスロット割り当て、アクセスは絶対アドレスの load/store で f64/bool は bitcast / truncate

#### `static get` / `static set` プロパティ

`static get name(): T { ... }` / `static set name(v: T) { ... }` でクラスレベルの計算プロパティを宣言できます。読み出しは `ClassName.name`、書き込みは `ClassName.name = v` と、`static` フィールドと同じ構文。本体内で `this` は使えません。

```rust
class Palette {
    static raw: i64 = 0xFF8800

    static get accent(): i64 { Palette.raw }
    static set accent(v: i64) { Palette.raw = v }

    static get label(): string { "palette" }   // read-only
}

Palette.accent              // 0xFF8800        (static getter 呼び出し)
Palette.accent = 0x00FFAA   // static setter 呼び出し
Palette.label               // "palette"
```

- インスタンス用 `get` / `set` と同じ形状制約: getter は引数なし・戻り値型必須、setter は引数 1 個・戻り値なし。getter の戻り値型と setter の引数型は一致が必要
- getter のみ (read-only) / setter のみ (write-only) も可。逆操作は利用側で「getter なし」「setter なし」の型エラー
- 同名のフィールド・メソッド・プロパティ・他の静的メンバとは衝突不可
- 他の静的メンバと同じくジェネリッククラスでは未対応
- Cocoa バインディングで多用されており、たとえば `NSColor.black` は `@objc("blackColor") pub static get black(): NSColor` として宣言され、読み出し時に ObjC クラスメソッドへディスパッチされる

### 継承 (`: Parent`)

`class Child: Parent { ... }` で単一継承できます (Phase B: 仮想ディスパッチ + override + super)。

```rust
class Animal {
    name: string
    init(n: string) { this.name = n }
    speak(): string { "generic sound" }
    describe(): string { this.name + " says " + this.speak() }
}

class Dog: Animal {
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
- メソッドのオーバーロードはサブクラスでも可。ただしオーバーロードのうち 1 つでも親メソッドと同名を取ろうとするとエラー (`override` は単一シグネチャ専用なので、同名のサブクラス変種は "hides a parent method" 扱い)
- `init` / `deinit` は per-class (継承の override 対象外)。`deinit` は破棄時に自動で連鎖する — 最派生クラスのものが先、続いて各祖先のものが順に走る (上のクラス基本の節を参照)
- 静的メソッド・静的フィールドの継承は未対応
- ジェネリッククラスでの継承は未対応
- サブタイプ: `Child` を `Parent` 型の binding / 引数 / 戻り値に渡せる
- JIT: object header に vtable ポインタを足し (`[strong | weak | drop_fn | vtable | fields...]`、32 byte ヘッダ)、各クラスに `Box<[i64]>` vtable を確保。仮想呼び出しは `obj.vtable[slot]` の load → call_indirect。`super.method` は親の特定関数への直接呼び出し

### インタフェース (`interface I { ... }`)

`interface Name { method(p: T): R … }` でメソッドの契約を宣言します。メソッド宣言は `class { }` 本体と同じ形 (`name(params): ret`、先頭の `fn` 不要) です。クラスは継承と同じ `:` のリストで参加します。リストの最初は親クラス・インタフェース・省略のいずれでも構いません。2つ目以降のカンマ区切りエントリはインタフェースとして扱われます。

```rust
interface Drawable {
    draw(): string
}

interface Speaks {
    speak(): string
}

class Animal {
    init() {}
    kind(): string { "animal" }
}

class Cat: Animal, Drawable, Speaks {     // 親クラス + 2つのインタフェース
    init() { super() }
    draw(): string { "cat-shape" }
    speak(): string { "meow" }
}

class Square: Drawable {                  // 親なし、インタフェースのみ
    init() {}
    draw(): string { "square" }
}

fn render(d: Drawable) {                  // インタフェースを引数型に
    console.log(d.draw())
}

let c: Drawable = new Cat()
render(c)                                  // "cat-shape"
let s: Drawable = new Square()
render(s)                                  // "square"
```

- インタフェースのメソッドはシグネチャのみ。v1 では本体 (デフォルト実装) は書けません。フィールド・プロパティ・`static` メソッド・ジェネリックパラメータも未対応です。
- クラスがインタフェース型のスロットに代入できるのは、そのクラス (または祖先) が `:` のリストにインタフェースを記載し、全メソッドをシグネチャ一致で実装しているときだけ。欠落 / 不一致はコンパイル時にエラー。
- インタフェース型レシーバへのメソッド呼び出しは実行時にレシーバの実クラスへ動的ディスパッチ (継承で使う `__virt_dispatch` パスを共有。インタフェースメソッドは衝突回避のため通常クラスのスロット範囲とは別の高位スロットを使用)。

---

## 8. 配列

```rust
let xs: i32[] = [10, 20, 30]    // 動的配列リテラル (注釈で動的を明示)
let ys: i32[3] = [1, 2, 3]      // 固定長 (要素数も型に固定)
let zs: i32[] = []              // 空配列は注釈必須
let inferred = [1, 2, 3]        // 注釈なしは動的 `i64[]`
let trailing = [1, 2, 3,]       // 末尾コンマ可

xs[1]                            // 添字読み取り
xs[0] = 100                      // 添字代入
xs.length                        // i64 を返す (組み込み)
```

注釈のない配列リテラルは **動的配列** です。`let a = [1, 2, 3]`
は `i64[]` で、そのまま `push` できます。固定長になるのは宣言型が
要求した場合だけです (`let a: i64[3] = [1, 2, 3]`、`T[N]` の field
や引数)。後述の破壊的操作 (`push` / `pop` / `shift` / `unshift` /
`remove` / `removeAt` / `fill`) は固定長レシーバを受け付けません。

固定長配列の要素には heap 型 (class インスタンス・string 等) も
使えます。値セマンティクスで管理され、ARC は要素単位で追跡されます:

```rust
class Box { n: i64
    init(x: i64) { this.n = x } }

let arr: Box[2] = [new Box(1), new Box(2)]   // 束縛がバッファを所有
fn sum(a: Box[2]): i64 { a[0].n + a[1].n }   // 引数渡しは借用
class Pack { items: Box[2] }                  // field は自分のバッファを所有
// p.items = q.items は値コピー (要素を retain した新バッファ)
```

あらゆるコンテナのセルに入れられます — Optional・enum payload・
tuple・動的配列の要素・Map の値・closure のキャプチャ・Promise。
field 代入と同じく、格納時に値コピーが作られます:

```rust
fn hold<T>(v: T): T? { some(v) }   // T = Box[2] で動く
let o = hold(arr)    // o は arr のコピーを所有
arr[0] = new Box(9)  // o の中身には見えない

let xs: Box[2][] = [arr]   // 要素もコピー
let t = (arr, 1)           // tuple のスロットも
let f = fn(): i64 { arr[0].n }   // キャプチャもコピー
```

戻り値と束縛の再代入は読み出し側の意味 (let・引数と同じ共有)、
`Array.fill` はスロットごとに独立したコピーを格納します:

```rust
fn members(t: Team): Box[2] { t.members }   // 共有を返す
arr = other      // arr は other と同じ配列を指すようになる
xs.fill(arr)     // 各スロットに独立コピー
```

`Set` の要素は従来の等値性規則で除外されたままです。

```rust

// 破壊的操作 (動的配列のみ。固定長は型エラー)
xs.push(40)                                  // 末尾に追加、() を返す
xs.pop()                                     // T? — 末尾を取り出す (空なら none)
xs.shift()                                   // T? — 先頭を取り出す
xs.unshift(0)                                // 先頭 (index 0) に挿入、()
xs.remove(20)                                // bool — 値が一致する最初の要素を削除
xs.removeAt(1)                               // T? — 指定 index を削除 (範囲外は none)
xs.fill(7)                                   // 全セルを上書き、()

// 検索・述語
xs.indexOf(20)                               // i64 (見つからなければ -1)
xs.includes(20)                              // bool
xs.find(fn(x: i32): bool { x > 15 })         // T? — 最初に true を返した要素
xs.findIndex(fn(x: i32): bool { x > 15 })    // i64 — そのインデックス、無ければ -1
xs.every(fn(x: i32): bool { x > 0 })         // bool — 全要素が true (空配列なら true)
xs.some(fn(x: i32): bool { x > 0 })          // bool — 少なくとも 1 つが true

// 新しい配列を返す (元の配列は変えない)
xs.slice(0, 2)                               // T[] — [start, end)
xs.concat([100, 200])                        // T[] — this ++ other
xs.reverse()                                 // T[] — 反転コピー
xs.map(fn(x: i32): i64 { x * 10 })           // U[] — 要素ごとに写像
xs.filter(fn(x: i32): bool { x > 10 })       // T[] — マッチした要素のみ
xs.sort(fn(a: i32, b: i32): i64 { a - b })   // T[] — comparator は < / = / > 0
xs.forEach(fn(x: i32) { console.log(x) })    // () — 各要素に対し f を実行

// `string[]` 限定
ss.join(", ")                                // string — 区切り文字列で連結
```

コールバックは **第一級関数** または **クロージャ** (匿名 `fn` で外側のローカル変数を value-capture できる — §6 参照) を渡せます。要素型の制限はありません。破壊的操作 (`push` / `pop` / `shift` / `unshift` / `remove` / `removeAt` / `fill`) は dynamic 配列のみで、固定長レシーバには型エラー。新配列を返す系 (`slice` / `concat` / `reverse` / `map` / `filter` / `sort`) は元の配列を変更せず、ヒープ要素は適切に retain した新バッファを返します。

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
m.entries()                      // (K, V)[] — キー/値のタプル
m.forEach(fn(k, v) { … })        // 全エントリに cb を適用、戻り値は ()
m.clear()                        // 全エントリ削除
```

### Set

```rust
let s: Set<i64> = new Set<i64>()    // 構築 (リテラル構文は未提供)
s.add(1)                            // 追加 (重複は無視)
s.add(1)
s.has(1)                            // bool
s.delete(1)                         // bool (要素が存在したか)
s.size()                            // i64
s.clear()                           // 全要素削除
s.values()                          // T[]
s.forEach(fn(v: T) { … })           // 全要素に cb を適用、戻り値は ()
s.union(other)                      // Set<T> — 和集合
s.intersection(other)               // Set<T> — 積集合
s.difference(other)                 // Set<T> — `self` − `other`
s.isSubsetOf(other)                 // bool
s.isSupersetOf(other)               // bool
s.isDisjointFrom(other)             // bool
```

- 要素型は `string` / `i*` / `u*` / `bool` / `f32` / `f64`、値等価プロトコルを実装したクラス (下記参照)、および全 variant が unit (またはクラス全体が `@flags`) の `enum`。enum は判別子タグでの等価/ハッシュなので既存のプリミティブ store に追加機構なしで載る。浮動小数はビットパターン比較なので、別ビットパターンの NaN は別エントリとして残る
- リテラル構文は未提供 (`{1, 2, 3}` は将来の拡張用として予約)。空 / 初期値付きの Set は `new Set<T>()` + `add` で作る

- キー型は `string` / `i*` / `u*` / `bool`、値等価プロトコルを実装したクラス (下記参照)、unit-variant (または `@flags`) `enum` — Set 要素と同じゲート。float は不可 (NaN ≠ NaN が `Eq` 契約を壊す)
- リテラル `{ key: value, ... }` の最初のキーから K を、最初の値から V を推論
- 空マップは `new Map<K, V>()` で構築 (`{}` は空ブロック扱い)
- パーサは `{<key-token> :` の 2 トークン先読みで map literal とブロックを区別 (ID/Str/Int/Bool + `:` で map)
- 基本演算 + `get` / `keys` / `values` / リテラルすべて対応

### クラスを Set/Map に入れるための値等価プロトコル

`Set<MyClass>` / `Map<MyClass, V>` は、クラスが以下の 2 メソッドを宣言しているとき要素・キーとして受け入れる:

```rust
pub fn equals(other: MyClass): bool   // 構造比較
pub fn hashCode(): i64                // 安定したハッシュ
```

両方必須。`equals` だけ / `hashCode` だけだと `new Set<MyClass>()` / `new Map<MyClass, V>()` の型チェックで弾かれる。コンテナ内では `equals` が `true` を返す 2 インスタンスを「同じ」と見なし、片方だけを保持する。

> 注: クラス参照に対する `==` 演算子は依然として参照等価のまま。値等価は Set / Map の重複判定にのみ影響する。

#### `@derive(Eq, Hash)` で自動合成

`class` 宣言の頭に `@derive(Eq, Hash)` を付けると、loader が両メソッドをフィールドベースで合成する:

```rust
@derive(Eq, Hash)
class Point {
    pub x: i64
    pub y: i64
    pub init(x: i64, y: i64) { this.x = x; this.y = y }
}

let s = new Set<Point>()
s.add(new Point(1, 2))
s.add(new Point(1, 2))    // 構造的に重複 → Set は 1 つだけ保持
test.expect(s.size(), 1)
```

- `@derive(Eq)` / `@derive(Hash)` を単独で書いてもよい。`@derive(Eq, Hash)` は両方の略記
- 手書きの `equals` / `hashCode` が優先される。合成は「ユーザが書いていない方」だけを埋めるので、手書きと derive の混在も可能
- 合成 `equals` はフィールド比較を `&&` で連ねる。クラス型のフィールドには `this.field.equals(other.field)` を出し、入れ子の `@derive` 値も構造比較で判定される
- 合成 `hashCode` は多項式畳み込み: `h_{i+1} = h_i * 31 + field_i.hashCode()`。サポート対象のフィールド型はすべて自身の `hashCode(): i64` を経由する:
  - `i8` / `i16` / `i32` / `i64` / `u8` / `u16` / `u32` / `u64` / `bool` / `f32` / `f64` — 各プリミティブ組み込みの `hashCode`
  - `string` — 組み込み FNV-1a (string メソッド節を参照)
  - クラスフィールド — 各クラスの `hashCode` (手書きまたは `@derive(Hash)`)
- サポート外のフィールド型 (配列・タプル・Optional・ジェネリクス・クロージャ等) は展開時に手書き実装への誘導エラーが出る。該当フィールドを持つクラスは当面 `hashCode` を手書きする

---

## 10. Optional

```rust
let a: User? = some(user)        // 通常の構築
let b: i64? = none               // 不在
let c: i64? = 7                  // T → T? 自動 wrap

if let some(v) = a {             // パターン分岐
    use(v)
}

a.isSome                       // bool
a.isNone                       // bool
a.unwrap()                       // T (none ならランタイム panic)
```

- `T?` の T は実行系で扱える任意の型。primitive 内部の場合は `[rc:i64 | payload:T]` のヒープボックスで表現される。
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
- **予約語バリアント名**: あらゆる予約語 (`override`, `class`, `none`, `loop`, `if`, `return`, …) を variant 名として使え、`Enum.<keyword>` でアクセスできます。C / Cocoa 由来の enum で予約語と衝突しがちなメンバ (`SDL_HINT_OVERRIDE`, `SKRepeatMode.loop`, `SDL_FLIP_NONE` 等) をそのまま命名するための配慮です。`static` は元々予約されていないので特別扱い不要。
- **match arm**: `=>` を使わず、パターンの直後に `{ body }` を書く (`Color.red { "red" }`)。
- 構築は `Enum.` プレフィクス必須 (`Shape.circle(3.0)`)。
- **match のパターン側は `Enum.` を省略可** — scrutinee の静的型から推論される (`circle(r)` は `Shape.circle(r)` と同義)。長い形 (`Shape.circle(r)`) も引き続き有効。
- 全バリアントを網羅するか `_` が必要 (型チェッカが拒否)
- 各 arm 値は型が揃う必要あり (if/else と同じ)
- パターンの束縛: タプルは位置 (`Shape.circle(r)`)、struct は名前 (`{ side }` または `{ side: s }`)、`_` で無視
- ペイロード内の heap 型 (Object / Str / Array / Optional / Weak / 別 enum) は ARC で正しく解放される

### プリミティブの match

`match` は **整数 / bool / 文字列** に対してもリテラルパターンで使えます:

```rust
let label = match n {
    1 { "one" }
    2 { "two" }
    -1 { "neg" }
    _  { "other" }
}

// 整数範囲 — 排他 `..`、包含 `..=`、半開 `..N`/`..=N`/`N..` (Rust 風)
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

- 整数パターン (`1`, `-7`) は **同符号の整数スクルチネ** に対し構造的等値で一致
- 整数範囲パターン (`a..b`, `a..=b`, `a..`, `..b`, `..=b`) は値が範囲に収まれば一致。両端は整数リテラル (または `-Lit`)。空レンジ (`5..5`, `5..3`) はコンパイルエラー。半開形は欠けた側に制限なし (`a..=` のような上限なし包含形は無意味なので拒否)
- `bool` は `true` / `false` 両 arm を書けば網羅 (それ以外は `_` 必須)
- 他のプリミティブ match は **`_` ワイルドカード必須** (値空間が網羅不可能なので)
- 浮動小数 / タプルのスクルチネは未対応 (`if`/`elif` を使う)

### 値付きフィールドレス enum

ユニットバリアントには `= <整数>` で明示的な discriminant を指定できます。指定しないバリアントは `直前 + 1` (先頭は 0) が割り当てられます。enum 名のあとに `: <数値型>` を書くと、内部表現の整数型を明示できます。

```rust
enum Priority: u32 {
    low    = 1
    medium = 5
    high   = 10
}

let p: u32 = Priority.high as u32   // 10
```

- discriminant はユニットバリアントにのみ指定可
- `: <type>` を省略した場合、`as` で指定したキャスト先の幅が使われる (`Priority.high as i64`)
- enum 値を任意の数値プリミティブにキャストすると、対応するバリアントの discriminant が得られる
- 数値プリミティブを **フィールドレス** な enum にキャストする逆方向 (`x as MyEnum`) も可。整数値を discriminant としてそのまま再解釈する。ペイロード付き variant をもつ enum は整数表現を持たないので不可。C 側の返り値を型付き enum に戻すのに使える (`SDL_GetKeyFromScancode(...) as Keycode`)

### `@flags` enum

ビットフラグ用の enum。値どうしの `|` `&` `^` `~` をサポートする。`@flags` 属性は `enum` キーワードの上に書く。`: <type>` を省略した場合、内部表現は **`u64`** (数値リテラルのデフォルト型に合わせる)。

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

- バリアントはフィールドレスのみ。discriminant のルールは値付き enum と同じ。
- ビット演算は両辺が同一の flags enum である必要がある。違う flags enum を混ぜる場合は明示的な `as` が必要。
- `value.has(other)` は `(value & other) == other` 相当の合成メソッド (複数ビットの `other` にも対応)。
- `@flags` enum 値に対して `match` は使えない。合成値は単一バリアントに対応しないため、`has` (またはビット比較) で分岐する。
- 実行時表現は内部整数なので、`combined as u32` / `combined as i64` で生のビット列を取り出せる。

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
- `(enum_name, type_args)` ごとに具体 EnumDecl を生成して per-instantiation でレイアウトを取る

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

let r = divide(10, 2)
r.isOk                     // bool — variant が `ok` なら true
r.isErr                    // bool — variant が `err` なら true
```

- 構築は **`Result.ok(v)` / `Result.err(e)`** (`Enum.variant(...)` の通常形式)
- `r.isOk` / `r.isErr` は **プロパティ** (括弧なし) で `bool` を返す。Optional の `isSome` / `isNone` と同じ形式
- match パターンでは `ok(v)` / `err(e)` と短縮可 (バリアント短縮形と同じ仕組み)
- `ok` / `err` は **予約語ではない** — 変数名として使える (使うと混乱するのでおすすめしないが)
- 名前 `Result` は予約 (ユーザが `enum Result { ... }` を定義するとエラー)
- 構築時の型引数は推論 — 戻り値型や let 注釈で T/E が確定する
- match の網羅性検査もそのまま (`ok` と `err` を両方カバーするか `_` が必要)
- `(T, E)` ごとに具体 enum をモノモルフ化

#### `?` 演算子 (`err` で短絡)

後置 `?` は `Result` を `ok` のペイロードに展開する。`err` のときは囲っている関数からその場で早期 return する。`?` を使うには、囲っている関数の戻り値型が `Result<_, E>` で、オペランドの `E` と一致している必要がある。

```rust
fn parse(s: string): Result<i32, string> {
    if s == "42" { Result.ok(42) } else { Result.err("not 42") }
}

fn doubled(input: string): Result<i32, string> {
    let v = parse(input)?            // ok → 値に展開、err → 早期 return
    Result.ok(v * 2)
}

doubled("42")     // Result.ok(84)
doubled("foo")    // Result.err("not 42") — `v * 2` には到達しない
```

- `match e { ok(v) { v } err(e) { return Result.err(e) } }` に展開される。早期 return するアームは match の結果型に寄与しないので、`e?` 全体は `T` 型に評価される
- 式が書ける場所ならどこでも使える (`let v = e?`、`f(e?)`、tail 位置など)
- 戻り値型が `Result<_, E>` でない関数の中で `?` を使うと型エラーになる (早期 return の形が関数の実際の戻り値型と合わないため)

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

## 13a. RTTI: `typeof` と `Type`

`typeof(x): Type` 組み込みで任意の値の **動的型** を取得できます (`Parent` 型のスロットに入った `Child` インスタンスでも `Child` と報告)。

```rust
class Animal { sound(): string { "?" } }
class Dog: Animal { override sound(): string { "woof" } }

let a: Animal = new Dog()
typeof(a).name         // "Dog" (動的 — "Animal" ではない)
typeof(a).kind         // TypeKind.class

typeof(42).name        // "i64"
typeof("hi").name      // "string"
typeof(some(1)).name   // "optional"

let r: Result<i64, string> = Result.ok(1)
typeof(r).name         // "Result"  (型引数は今後の `typeArgs()` で別途公開)
```

`Type` のプロパティ:

| プロパティ | 型 | 説明 |
| --- | --- | --- |
| `.name` | `string` | 表示用の型名 (`"Dog"` / `"i64"` / `"Result"` 等) |
| `.kind` | `TypeKind` | `primitive` / `class` / `enum` / `optional` / `array` / `fn` / `tuple` / `string` / `unit` のいずれか |
| `.parent` | `Type?` | `: Parent` で指定された直近の親クラス。非クラスやルートクラスは `none` |
| `.fields` | `string[]` | 宣言されたフィールド名一覧 (class のみ。それ以外は空配列)。継承元のフィールドは含まれない — `.parent` を辿って取得 |
| `.methods` | `string[]` | 宣言されたメソッド名一覧 (class のみ。それ以外は空配列)。`init` も含む |
| `.typeArgs` | `Type[]` | ジェネリックインスタンスの型引数 (`Result<i64, string>` なら `[Type("i64"), Type("string")]`)。非ジェネリックは空 |

各メンバの型情報は **ルックアップメソッド** (getter ではなく `()` 付き) で取得します:

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
t.methodParams("greet")         // some([])  — 引数なし
t.methodParams("init")          // some([Type("string")])
t.methodReturn("nope")          // none
```

| メソッド | 戻り値 | 説明 |
| --- | --- | --- |
| `.fieldType(name: string)` | `Type?` | 指定名のフィールドの宣言型。class でない / 該当無しなら `none` |
| `.methodReturn(name: string)` | `Type?` | 指定名のメソッドの戻り値型。該当無しは `none` |
| `.methodParams(name: string)` | `Type[]?` | 指定名のメソッドの引数型一覧。該当無しは `none` |

### 型テストとダウンキャスト

```rust
class Animal {}
class Dog: Animal {}
let a: Animal = new Dog()

a is Dog        // bool — 親チェーンを辿って true
a is Animal     // bool — true
a is Cat        // bool — false (Cat が無関係なら)

let d: Dog? = a as? Dog    // 成功時 some(d)
let c: Cat? = a as? Cat    // 失敗時 none
```

`is T` / `as? T` は実行時に親チェーンを辿ります。現状 `T` は **クラス型** に限られます。

`TypeKind` は組み込みの unit enum で、通常通り `match` で分岐可:

```rust
let label = match typeof(x).kind {
    primitive { "num" }
    string { "text" }
    class { "obj" }
    _ { "other" }
}
```

- `Type` / `TypeKind` は予約名 (ユーザ側で再定義不可)
- クラスの動的型は vtable ヘッダ経由でディスパッチ、継承下でも正しく動作
- `.fields` / `.methods` は **宣言された** メンバ名のみを返し、継承は含まない。型情報は `fieldType(name)` / `methodReturn(name)` / `methodParams(name)` で個別取得

---

## 13b. モジュール (`use`)

別ファイルの items (`fn` / `class` / `enum`) を取り込みます。Rust 風の **同一ディレクトリ解決**: `use utils` は隣の `utils.il` を読みます。

```rust
// utils.il
pub fn double(n: i64): i64 { n * 2 }
pub class Counter {
    n: i64
    pub init(start: i64) { this.n = start }
    pub bump() { this.n = this.n + 1 }
    pub get(): i64 { this.n }
}

// main.il
use utils                            // 名前空間越しに使う
use std.math as math { sqrt, pi }    // 選択取り込み + 別名で名前空間
use std.math as m { e }              // 別名 + 選択取り込み
use std.math as _ { ln }             // 選択取り込みのみ（名前空間抑止）

let c = new utils.Counter(10)
c.bump()
utils.double(c.get())            // → 22
sqrt(2.0)                        // bare（選択取り込み由来）
math.sqrt(2.0)                   // 名前空間越しも引き続き使える
m.cos(0.0)                       // 別名 `m` 経由
ln(2.0)                          // bare のみ。`as _` を付けたので `math.ln` は不可
```

- **2 形式**:
  - `use module` — 名前空間越し参照 (`module.foo()`, `new module.Class()`, `module.Enum.variant`)
  - `use module { name1, name2 }` — 個別取り込み (ベアネームで使う)。**名前空間も同時に登録される**ので、`name1` と `module.name1` の両方が同じファイル内で使える。名前空間を抑止したい場合は `as _`（後述）を使う。`pub use` チェインを辿るので、`use sdl { InitFlag }` は umbrella `sdl` が再エクスポートしている `sdl_core` の `InitFlag` も解決できる
  - どちらの形式も `as <別名>` / `as _` を後置できる（後述 `use ... as`）
- すべての top-level item は デフォルトで **module-private**（宣言したファイルからしか参照できない）。`fn` / `class` / `enum` / `const` / トップレベル `let` / `@extern(C){}` 内のアイテムに `pub` を付けると、他モジュールから `module.X` で参照可能になる。クロスモジュールから非 `pub` なアイテムを参照するとロード時にエラーになる。クラスメンバ（`init` / メソッド / フィールド / プロパティ / `static`）も同様で、デフォルト private、`pub` で外部モジュールから利用可能。（メンバの可視性は現状パース時にフラグとして保持されるのみで、型付きレシーバ越しの private メソッド呼び出しチェックはフォローアップで対応）
- 循環インポート (`A → B → A`) は **DAG 検出してエラー**
- 同じモジュールを複数回 `use` しても一度しかロードされない (ファイルパスで dedupe)
- 全モジュールが 1 つの Program にマージされる (ファイル境界は型チェッカ以降は意識されない)
- 名前空間越し import の中身は `module.X` プレフィクスで内部識別されるため、`use module` した時に親プログラムの bare 名と衝突しない
- **同梱 `std` パッケージ**: 標準ライブラリは `libs/std/*.il` に置かれており、コンパイラバイナリに `include_str!` で同梱される。`use std.<module>` で参照する。現時点で提供しているのは `std.math` / `std.test` / `std.os` / `std.fs` / `std.path` / `std.regex` / `std.time` / `std.events` / `std.ffi`。`std.` プレフィクス無しの bare `use math` などは受け付けられず、`use std.math と書いてください` という診断が出る

#### `use ... as` (別名 / 名前空間抑止)

`use module as <別名>` は、import 元の名前空間名をリネームする。ファイル内では `<別名>.X` で参照する。loader はモジュール本来の名前で item をマージするが、ファイル単位の normalize 処理が `<別名>.X` を canonical な `module.X` に書き戻すため、merge 後の view と齟齬は出ない。

```rust
use sdl_renderer as r
let win: r.Window = ...           // 内部的には sdl_renderer.Window
new r.Texture(win, ...)
```

`use module as _ { ... }` は **名前空間を完全に抑止する**。`module.` も別名も登録されず、selective list で取り込んだ bare name のみが見える:

```rust
use sdl { Renderer, Window }      // `Renderer` も `sdl.Renderer` も両方使える
use sdl as _ { Texture }          // bare `Texture` のみ。`sdl.Texture` は使えない
```

制限:

- `use module as _` は `{ ... }` 必須（名前空間を抑止しつつ bare 名も取り込まないと観測可能な効果が無いため）
- `pub use module as <別名>` / `pub use module as _` はエラー — umbrella の責務は内部モジュールを **元の名前** で公開することなので、別名で混乱させない
- 別名は通常の identifier ルールに従う。`_` は予約された discard 形式

#### `pub use` (再エクスポート / umbrella モジュール)

`pub use other_module` をモジュール内に書くと、`other_module` の item を **現在のモジュールの名前空間** で再公開する。複数の小さなモジュールを束ねる umbrella ファイル用:

```rust
// sdl.il (umbrella)
pub use sdl_core
pub use sdl_window
pub use sdl_renderer

// main.il
use sdl
sdl.init(sdl.INIT_VIDEO)        // sdl_core 由来
new sdl.Window(...)             // sdl_window 由来
```

`pub` を付けずに `use sdl_window` を `sdl.il` 内に書くと、呼び出し側が `use sdl` していても `sdl_window.*` のままになる。`pub use` は `sdl.*` 配下に再プレフィクスする。エントリポイント (親モジュールがない) では `pub use` は普通の入れ子 `use` と同じ。

`pub` は `fn` / `class` / `enum` / `const` / トップレベル `let` / `@extern(C){}` 内の宣言、およびクラスメンバ（`init` / メソッド / フィールド / プロパティ / `static`）に付けられる。`pub` がなければ module-private。属性の後ろにも書ける: `@flags pub enum Color { ... }`。

### トップレベル `struct` / `union` (値型)

`struct` / `union` 宣言は `@extern(C) { ... }` ブロックの **外** (モジュールのトップレベル) でも書ける。セマンティクスはブロック内版と同じ — **C レイアウト・値型・引数は値渡し**、フィールドのみでメソッド・継承なし、代入や引数渡しでコピーされる — ただしフィールドの型は **ilang 側の型** に限定される。バリデータは型を再帰的に追跡し、以下の C-only 型がどこかに登場すれば拒否する:

- `char`
- `void`
- `size_t` / `ssize_t`
- raw ポインタ (`*T` / `*const T`)

```rust
// OK — 全フィールドが ilang 側の型
struct Point {
    x: i32
    y: i32
}

pub struct Rect {
    width: i32
    height: i32
}

union Value {
    i: i64
    f: f64
}

let p = new Point()
p.x = 3
p.y = 4
```

```rust
// 型検査でエラー — `char` は C-only
pub struct Bad {
    c: char
}
```

再帰的なチェックは名前付き struct / union の参照も追う。`struct Outer { inner: SomeCStruct }` は `SomeCStruct` の中のどこかに禁止型があれば拒否される。

`char` / `void` / `size_t` / ポインタを含むフィールドが必要なら (例えば実在する C 型をミラーしたい場合)、その宣言は `@extern(C) { ... }` ブロックの中に置くこと。ブロック内版は型に関する制限を受けない。

属性 (`@packed`, `@bits(N)`) はトップレベル形式では受け付けない。これらが必要なときは `@extern(C)` 形式を使う。

### `@extern(C) { ... }` — FFI ブロック

C ABI で外部関数を呼び出す / C 互換の構造体を扱う / C グローバル変数にアクセスする全ての宣言は **`@extern(C) { ... }` ブロック** に閉じ込めます。raw ポインタ (`*T` / `*const T`) や C-only 型 (`char` / `void` / `size_t` / `ssize_t`) はブロック内でのみ書け、ブロックの外には漏れません。

```rust
@extern(C) {
    @lib("c") fn strlen(s: *const char): size_t
    @lib("m") fn sqrt(x: f64): f64

    // 不透明ハンドル: 空 struct = ポインタ型として使う
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE
    @lib("c") fn fclose(stream: *FILE): i32

    // C 互換 struct
    struct timespec {
        tv_sec: i64
        tv_nsec: i64
    }
    @lib("c") fn clock_gettime(clk: i32, tp: *timespec): i32
}
```

トップレベルでのみ書ける。属性は `@extern(C)` のみ (他属性との併用不可)。

##### `@extern(C, "libname", ...)` — ブロック既定ライブラリ

ブロック内の fn が同じ共有ライブラリを使うときは、`C` の後に文字列を続けてブロック既定ライブラリを宣言できる。各 fn は `@lib`（引数なし）を書くと既定を継承し、`@lib("other")` で個別 override:

```rust
@extern(C, "SDL2") {
    @lib pub fn SDL_Init(flags: u32): i32     // → "SDL2"
    @lib pub fn SDL_Quit()                    // → "SDL2"
    @lib("c") pub fn time(t: i64): i64        // per-fn override
}
```

`@extern(ObjC, "path", ...)` で framework パスを並べるのと同じ書式。既定なしの `@extern(C) { ... }` で bare `@lib` を書くとエラー。

#### ブロック内に書けるもの

- **`fn 関数宣言`** — dlsym / host 登録による外部関数呼び出し
- **`fn 関数定義 { body }`** — ilang で書いた本体を C ABI で公開する関数 (callback など)
- **`struct Name { fields }`** — C 互換構造体 (旧 `@extern(C) struct` の置き換え)
- **`union Name { fields }`** — C union (全フィールド offset 0)
- **`@packed struct Name { ... }`** — `__attribute__((packed))` 相当 (padding なし、align=1)
- **`@handle struct Name {}`** — Win32 `HWND` / `HINSTANCE` 風の、ポインタサイズの不透明ハンドル
- **`class Name { ... }`** — メソッド本体を `@extern(C)` コンテキストで型検査する ARC 管理ラッパクラス

#### fn 宣言: `@lib` / `@optional` / `@symbol` / 可変引数

```rust
@extern(C) {
    @lib("c") fn abs(x: i32): i32                         // libc::abs
    @lib("c", "m") fn fallback(x: f64): f64               // libc 失敗 → libm
    @lib("libssl.so.3") @optional fn SSL_new(): *void     // 失敗しても JIT 続行
    @lib("c") @symbol("snprintf")
        fn formatI64(buf: *u8, n: size_t, fmt: *const char, ...): i32
}
```

- **`@lib("name", "fallback", ...)`** — dlopen するライブラリ名。複数指定すると先頭から順に試し、最初に開けたものを使う (soname 違いの吸収)。**ユーザコードの native 呼び出しは必ず `@lib` を付ける** (canonical なマーカー)。`@lib` を省略した bare 宣言は **host 登録形** 専用で、`JITBuilder::symbol(...)` でホストが事前登録した関数アドレスを使う `math` / `os` / `test` 標準ライブラリ向けの実装方式 — 通常のユーザコードでは出番なし

  > 上記の `@extern(C, "libname", ...)` ブロック既定で `@lib("name")` の繰り返しを bare `@lib` に畳める。fn 単位の `@lib("other")` は引き続き override 可
- **`@optional`** — ライブラリやシンボルが見つからなくてもエラーにせず、その fn は呼ばれたら abort するスタブにバインドされる。プログラム側で `os.libLoaded(name): bool` でガードしてから呼ぶ慣例
- **`@symbol("c_name")`** — ilang 側の fn 名と C 側のシンボル名を分離する。C# の `[DllImport(EntryPoint=...)]` 相当。同じ C 関数に異なる ilang 型で 2 度宣言したい場合や、ilang のキーワードと衝突する C 名を避けたい場合に
- **可変引数 `...`** — シグネチャ末尾に `...` を書くと printf 系 variadic を呼べる。固定 prefix の型は通常通りチェック、追加引数は型不問でそのまま流す (フォーマット指定子との一致は呼び出し側の責任)。Apple AArch64 では variadic args をスタックに spill するための signature padding を JIT が自動挿入

#### ライブラリ名の解決

- **bare name** (`"m"` / `"sqlite3"` 等、`.` `/` `\` を含まない): OS 規約で自動補完。macOS = `lib{name}.dylib` / `{name}.dylib`、Linux = `lib{name}.so` → `…so.6` → `…so.0` の順、Windows = `{name}.dll` / `lib{name}.dll`
- **literal filename** (`"libc.dylib"` / `"libm.so.6"` / `"./build/foo.so"`): 補完なしでそのまま `dlopen`
- 同じライブラリは 1 回だけ `dlopen` される。`os.libLoaded(name)` は **常に最初に書いた canonical 名前** で問い合わせる
- **`os.libLoadError(name): string`** で失敗時の dlopen メッセージを取得できる (診断用、ガードは `libLoaded` を使う)

#### C 互換 struct (`struct Name { ... }`)

```rust
@extern(C) {
    struct timespec {
        tv_sec: i64
        tv_nsec: i64
    }
    @lib("c") fn clock_gettime(clk: i32, tp: *timespec): i32
}

let ts = new timespec()              // 0 初期化
clock_gettime(0 as i32, &ts)         // `&` で構造体のアドレスを渡す
console.log(ts.tv_sec)
```

- メソッド / `init` / 継承 / 型パラメータ / プロパティは禁止 (フィールドのみ)
- 各フィールドは **natural alignment** (i64=8B、i32=4B、bool=1B) — C struct と一致
- `new ClassName()` で **0 初期化**
- フィールドの型は **数値プリミティブ・bool・`string`・他の `@extern(C)` struct・raw ポインタ・固定長数値配列** が許される
- **空 struct** (`struct FILE {}`) は **不透明ハンドル** として使う。`*FILE` 型のポインタが ilang 側で型安全に持ち回せる (旧 `@extern("lib") class Foo {}` の置き換え)
- **`string` フィールド**: 8 バイトの heap ポインタ (`StringRc *`) をスロット保持。物理レイアウトは `char *` ではないので、C ABI で `char *` メンバを必要とする場合は (a) 別の関数引数として `*const char` で渡す、または (b) `u8[N]` 固定長 buffer を使う
- **固定長数値配列フィールド** (`u8[8]`, `i64[3]`, `f64[2]` 等): バイト列がインライン埋め込み。要素アクセス `s.arr[i]` は bounds check あり
- **nested struct**: 別の struct をフィールド型に書くとバイト列が **インライン埋め込み**。chain access (`outer.inner.x`) で読み書き可
- **aggregate literal**: `point { x: 1 as i32, y: 2 as i32 }` で初期化 (`new` + 連続代入の糖衣)
- 内部宣言順序は自由 — JIT が依存をトポロジカルソートしてから layout を確定 (循環埋め込みはエラー)
- **C99 flexible array member**: 最終フィールドに `T[]` (固定長なし) を書くと FAM 扱い。`new ClassName(n)` で末尾領域を `n` 要素ぶん確保、`obj.data[i]` で要素アクセス (bounds check は省略)

#### `@packed`、`@bits(N)`

```rust
@extern(C) {
    @packed struct PacketHeader {
        magic: u8
        length: u32        // packed なので offset 1 (padding なし)
        flags: u8
        code: u16
    }
    struct ModeFlags {
        @bits(3) read_perm: u32
        @bits(3) write_perm: u32
        @bits(3) exec_perm: u32
        // 同 underlying 型の連続ビットフィールドは 1 ユニット (u32) にパッキング
    }
}
```

- `@packed` — 全フィールドが offset = sum of prior sizes に並び、struct 全体の align も 1。ネットワーク/ファイルフォーマットのヘッダ向け
- `@bits(N)` — フィールドを N ビット幅のビットフィールドに。連続する同じ underlying 型のビットフィールドは共有ストレージユニットにパッキング (GCC-style)。制約: **unsigned 整数のみ** (u8/u16/u32/u64)、N は 1..=underlying 幅

#### `@handle struct Name {}` — ポインタサイズの不透明ハンドル

```rust
@extern(C, "user32") {
    @handle pub struct HWND {}
    @handle pub struct HBRUSH {}

    @lib pub fn GetDC(hWnd: HWND): HANDLE
    @lib pub fn FillRect(hDC: HANDLE, lprc: *RECT, hbr: HBRUSH): i32
}
```

Win32 系の `HWND` / `HINSTANCE` / `HMODULE` のように、SDK 上は `void *` 形だけれど **型として別物** として扱いたい不透明ハンドル用。引数の取り違えをコンパイル時に弾きたい場合に使う。

- struct 本体は **必ず空** — `@handle` は handle の **値** を宣言するもので、レイアウトを持たない。フィールドを書くと parse エラー
- 使用箇所では型を **bare で書く** (`hwnd: HWND`)。`*HWND` ではない — handle 自体が既にポインタサイズで、FFI 境界に「handle へのポインタ」型は存在しない
- 各 `@handle` 宣言は **nominally 別型** — `HWND` と `HBRUSH` は両方とも 8 バイトポインタだが、暗黙には相互変換しない
- ただし FFI 境界では `as` キャストで **`*void`**, **`i64`**, **他の `@handle`** 間を自由に行き来できる (SDK 側の `(HWND)hWndOrNull` / `reinterpret_cast<HMODULE>(hInstance)` のような流儀をそのまま写せる)。null handle は `0 as HWND`
- どこから見ても `(size, align) = (8, 8)` を返す — `WNDCLASSEXA.hInstance: HINSTANCE` のように handle を struct のフィールドに置くと、生ポインタと同じ 8 バイトを占める
- handle には **ARC が一切かからない** — 単なるポインタサイズのスカラとして扱われ、retain / release されない。寿命管理は C 側 (`CloseHandle` / `DestroyWindow` 等) の責任

逆に「不透明な **指される側**」をモデル化したいだけのときは普通の空 struct (`struct FILE {}`) を使い、`*FILE` 経由で取り回す。**SDK が既に typedef 済みのポインタ形** を渡してきていて `*HWND` だと一段余分な間接化になるときが **`@handle`** の出番。

#### `union Name { ... }`

全フィールドが offset 0 に重ねて配置される。サイズ = `max(field_sizes)`、align = `max(field_aligns)`。

- 用途: `union sigval` / `siginfo_t` 等、整数 ↔ float の bit pattern 変換 (type punning)
- フィールドは **数値プリミティブ / bool / 固定長数値配列** のみ (heap 型は ARC が壊れるので不可)
- bitfield と FAM は禁止、`@packed` との併用も禁止

#### raw ポインタ + C-only 型 (ブロック内のみ)

ブロック内では C の型を直接書ける:

| ilang | C |
| --- | --- |
| `*T` | `T *` |
| `*const T` | `const T *` |
| `char` | `char` (i8 相当) |
| `void` | `void` (戻り値専用、`*void` で `void *`) |
| `size_t` | `size_t` |
| `ssize_t` | `ssize_t` |

これらは **ブロック外の式・型注釈に書けない**。外側に出すには ilang の通常型に変換するヘルパー fn (後述) を介する。

- **`*T` ↔ `*const T`**: `*T → *const T` は暗黙変換可 (write 権限を捨てる)。逆は不可
- **`*T` ↔ `i64`**: 双方向の `as` キャスト可 (生のアドレス値として扱える)
- **`T[]` → `*T` / `*const T`**: 暗黙変換 (配列の data 領域へのポインタを渡す)。ARC が呼び出し中の解放を防ぐので C 側が write してきても安全
- **`@extern(C)` struct → `*StructName`**: **暗黙変換しない** — `&value` でアドレスを明示的に渡す (例: out パラメータ `let ts = new timespec(); clock_gettime(0, &ts)`)。`&local` / `&field` と同じくアドレス取得を明示させる方針。配列だけは data 領域のレイアウト上ポインタが自然な表現なので `T[] → *T` を暗黙にしている

#### マーシャリングヘルパー (`use std.ffi { ... }`)

下記のヘルパーは `libs/std/ffi.il` 内で `@intrinsic("ffi.X")` 付きの `@extern(C) { ... }` ブロックに宣言されている。呼び出し側は `use std.ffi { ... }` で取り込む:

```rust
use std.ffi { cstrFromString, stringFromCstr, readU32, writeU32 }
```

シグネチャに raw ポインタ / `char` / `void` / `size_t` を含むヘルパー (下記のうち `errnoCheck` / `errnoCheckI64` 以外すべて) は C-only 型のルールで **`@extern(C) { ... }` ブロック内からしか呼べない**。`errnoCheck` / `errnoCheckI64` は `i32` / `i64` だけで完結するのでブロック外でも呼べる。

| ヘルパー | シグネチャ | 用途 |
| --- | --- | --- |
| `cstrFromString` | `(s: string): *const char` | ilang `string` の内部 NUL 終端 UTF-8 バッファを C ポインタとして取り出す |
| `stringFromCstr` | `(p: *const char): string` | C ポインタから新しい ilang `string` にコピー (NUL 終端で長さ検出) |
| `freeCstr` | `(p: *const char): unit` | `cstrFromString` の対称用 no-op (ilang 側がバッファを所有しているので実際の解放はしない) |
| `bytesFromBuffer` | `(p: *const void, n: size_t): u8[]` | 指定長のバイト列をコピーして `u8[]` を返す |
| `readI8`/`readI16`/`readI32`/`readI64` | `(p: *const void, off: i64): iN` | アロケーション無しでポインタ + オフセット (**バイト** 単位) から符号付き整数を読み込み。アラインメントは呼び出し側責任 |
| `readU8`/`readU16`/`readU32`/`readU64` | `(p: *const void, off: i64): uN` | 同上、符号無し |
| `readF32`/`readF64` | `(p: *const void, off: i64): fN` | 浮動小数版 |
| `writeI8`/`writeI16`/`writeI32`/`writeI64` | `(p: *void, off: i64, v: iN)` | `p + off` に書き込む符号付き版 |
| `writeU8`/`writeU16`/`writeU32`/`writeU64` | `(p: *void, off: i64, v: uN)` | 同上、符号無し |
| `writeF32`/`writeF64` | `(p: *void, off: i64, v: fN)` | 浮動小数版 |
| `arrayFromCArray<T>` | `(p: *const T, n: size_t): T[]` | プリミティブ配列をコピー (T は数値 / bool)。stride は呼び出し時に `T` から推論される |
| `cstrArrayToStrings` | `(p: *const *const char): string[]` | NULL 終端 `char**` を ilang `string[]` に変換 (`environ` / argv 系) |
| `errnoCheck` | `(rc: i32): i32?` | POSIX 流 "戻り値 < 0 は失敗" を `i32?` に。失敗 → `none`、成功 → `some(rc)` |
| `errnoCheckI64` | `(rc: i64): i64?` | 同上の i64 版 (`ssize_t` 系) |

呼び出し側は `os.errno()` で失敗の原因を別途参照する。

#### 値渡し (struct by-value)

`@extern(C) {}` 内の fn が `struct` 型の引数 / 戻り値を受け取る場合、**自動で値渡し** になる (旧 `byValue` フラグの代替)。AArch64 AAPCS64 / x86_64 SysV の "integer-only ≤ 16 B composite" ルールに従って 1〜2 個の i64 chunk に分解、HFA は FP register に乗る。

- フィールド制約: **整数 / bool / raw ポインタ / size_t / ssize_t / char** または **HFA** (1..=4 個の同一 float 型)。混在 (int+float) は登録時エラー
- ≤ 8 B → 1 GPR、9..=16 B → 2 GPR、> 16 B → indirect (caller-allocated copy + pointer)
- HFA → 各要素が独立した FP register (V0..V3 / XMM0..XMM3) に
- > 16 B 戻り値 → sret (hidden 第 1 引数で caller-allocated バッファのポインタ)

#### ブロック外への漏れ防止

ilang はブロック外側で raw ポインタや C-only 型に触れることを **型レベルで禁止**:

1. **C-only 型の名前 (`*T` / `char` / `size_t` 等)** はブロック外の型注釈に書けない
2. **値の型に C-only 型が含まれる式** はブロック外で評価不可:
   ```rust
   let raw = strdup(cstrFromString("x"))   // ERROR: *const char outside @extern(C)
   ```
3. **マーシャリングヘルパー** (`cstrFromString` 等) は `use std.ffi { ... }` で取り込んだあとでも C-only 型ルールが残るので、シグネチャに raw ポインタ / `char` / `void` / `size_t` を含むものはブロック外で呼べない (`errnoCheck` / `errnoCheckI64` だけは ilang 型で完結するので例外)
4. **ブロック内でも `@lib(...)` を持たない ilang 側 fn (ラッパー) は、引数 / 戻り値の型に raw pointer を直接または `@extern(C) struct` のフィールド経由で含めてはならない**。チェッカが struct のフィールドを再帰的に walk する。例: `fn driverInfo(): SDL_RendererInfo` は `SDL_RendererInfo.name: *const char` を含むので拒否される。境界で ilang クラス (例: `name: string` を持つ `RendererInfo`) に詰め替える

これにより「strdup の戻り値を ilang コードが触り続ける」ような事故を物理的に防ぐ。FFI のラップは必ず `@extern(C) {}` 内の fn に閉じ込めて、ilang ネイティブ型 (`string`, `i32`, `T[]`, …) を返す形に整える。

```rust
use std.ffi { cstrFromString, stringFromCstr }

@extern(C) {
    @lib("c") fn strdup(s: *const char): *const char

    // strdup → ilang string コピー → libc::free 相当を呼ぶ wrapper
    fn dupCounted(s: string): string {
        let raw = strdup(cstrFromString(s))     // ブロック内なので OK
        let copy = stringFromCstr(raw)
        test.countedFree(raw as i64)
        copy
    }
}

// ブロック外からは ilang ネイティブ型のみが見える
let copy = dupCounted("hello")
```

#### POSIX errno 規約のラップ (`errnoCheck`)

```rust
use std.ffi { errnoCheckI64 }

@extern(C) {
    @lib("c") fn read_raw(fd: i32, buf: *u8, n: size_t): ssize_t

    fn safeRead(fd: i32, buf: u8[]): i64? {
        errnoCheckI64(read_raw(fd, buf, buf.length as u64))
    }
}

if let some(n) = safeRead(fd, buf) {
    // 成功
} else {
    let code = os.errno()
    // 失敗
}
```

#### out-pointer (sqlite3_open 形式)

```rust
@extern(C) {
    struct Buf {}
    @lib("c") fn posix_memalign(memptr: *i64, align: size_t, size: size_t): i32
    @lib("c") fn free(ptr: *Buf)

    fn freeRaw(p: i64) { free(p as *Buf) }
}

let slot: i64[] = [0]
if posix_memalign(slot, 64 as u64, 1024 as u64) == 0 {
    let raw = slot[0]                // 普通の i64
    // ... raw を使う ...
    freeRaw(raw)
}
```

- 1 要素の `i64[]` をスロットとして渡し、書き込まれたポインタを `i64` として取り出す
- `*Buf` への変換が必要なら `freeRaw` のような薄い wrapper をブロック内に書く

#### C コールバック (関数ポインタ)

```rust
@extern(C) {
    @lib("c") fn qsort(
        base: *void, nmemb: size_t, size: size_t, compar: fn(*const void, *const void): i32
    )
}

fn cmp(a: *const void, b: *const void): i32 { ... }   // top-level fn
qsort(...)                                            // cmp を直接渡す
```

- パラメータの型は数値プリミティブ + raw ポインタのみ
- 渡せるのは **top-level fn の名前** だけ。`let f = my_fn; ext(f)` のような let 経由は拒否 (closure box の env_ptr は C ABI に乗らない)

#### その他の細かい挙動

- `string` 引数の自動 NUL 終端 UTF-8 マーシャリングは **使わない** — 必ず `cstrFromString` 経由で明示的に変換する
- `string` 戻り値の自動コピーも **使わない** — `stringFromCstr` で明示的に変換する
- 文字列内の NUL バイトは `cstrFromString` 時点で最初の出現で切り捨て (C のセマンティクスに合わせる)
- ライブラリ open 失敗 / シンボル未存在 (`@optional` なし) はコンパイル時 (JIT 構築時) エラー
- 同じ `@extern(C) { ... }` ブロック内で複数の fn / struct を宣言する場合、宣言順序は自由

### `@extern(ObjC) { ... }` — Objective-C ブリッジブロック

macOS 専用の Objective-C ランタイム側を扱うブロック。`@extern(C)` の ObjC 版で、ObjC ランタイムとつなぐクラス、ObjC **プロトコル** に相当する interface、セレクタメタデータを置く。

```rust
@extern(ObjC) {
    @objc class NSObject {
        @objc("alloc")   static alloc(): NSObject
        @objc("release") release()
    }

    @objc pub interface NSMenuDelegate {
        menuWillOpen?(menu: NSMenu)
        menuDidClose?(menu: NSMenu)
    }
}
```

#### `@objc class Name { ... }` — ブリッジクラス

- `@objc("selector:")` 属性で、メソッドが結び付く ObjC セレクタを指定する。メソッド本体は通常のクラスと同様に型チェックされ、loader が各クラスに IMP トランポリンを生成して Cranelift 呼び出し規約と `objc_msgSend` の間をマーシャリングする
- `alloc` / `init` は `@objc("alloc") static alloc(): Name` / `@objc("init") init(): Name` の形で宣言するのが定石。これでランタイムのアロケート / 初期化経路に到達できる
- クラスは (直接または別の `@objc class` を介して) `NSObject` を継承していること。ランタイム側への登録は `Name.register()` を最初に呼んだ時に行われ、二度目以降は no-op

#### `@objc interface I { ... }` — Objective-C プロトコル

ObjC の **protocol** は ilang の `@objc interface` に対応する。宣言形式は通常の `interface I { ... }` と同じ (シグネチャのみ、本体不可)。`@objc` が加える違いは:

- 各メソッドに `@objc("selector:")` を付けて、既定以外のセレクタ名を指定できる。属性が無ければセレクタ名はメソッド名 + パラメータごとの末尾 `:` (Cocoa 流儀) に決まる
- メソッド名末尾に `?` を付けた宣言 (例: `menuWillOpen?(menu: NSMenu)`) は ObjC の `@optional` プロトコルメソッドに対応する。実装は必須ではなく、ランタイムでは `respondsToSelector:` で振り分けられる。optional を表す唯一の構文がこの末尾 `?` で、旧来の `@optional` 属性は構文エラー。末尾 `?` を書けるのは `@objc` interface の中だけで、通常の interface は strict な実装契約を保つ
- `@objc interface` 型はパラメータ / プロパティ位置でそのまま使える。`pub setDelegate(d: NSMenuDelegate)` のような形が型チェックされ、ブリッジ側の ARC retain / release も自動で配線される

#### ObjC サブクラスを通常の `class` で書く

ObjC ランタイム側のサブクラスを ilang から定義するのに、`@extern(ObjC) { ... }` ブロックや `@objc class` 属性を**自分で書く必要はない**。loader の auto-lift パスは base list に Objective-C 側の型が並んでいれば自動でブロックに包む:

- **プロトコルの実装** — base list に `@objc interface` がある (例: `class AppDelegate : NSApplicationDelegate`)
- **ObjC クラスの継承** — base list に `@objc class` がある (例: `class FormHandler : NSObject`、`class MyView : NSView`)

```rust
use cocoa { NSObject, NSApplicationDelegate, NSNotification,
            sharedApplication, ActivationPolicy, selectorPtr,
            makeWindow, makeButton, StyleMask, nil }

// base list の `@objc interface` でトリガー。
class AppDelegate : NSApplicationDelegate {
    pub applicationDidFinishLaunching(notification: NSNotification) {
        console.log("launched")
    }
}

// `@objc class` 親でトリガー。`submit(sender: …)` のデフォルト
// セレクタは `submit:` で、`selectorPtr("submit:")` が返すものと
// 一致するため target/action 配線がそのまま通る。
class FormHandler : NSObject {
    pub submit(sender: NSObject) {
        console.log("clicked")
    }
}

let app = sharedApplication()
app.setActivationPolicy(ActivationPolicy.regular)
AppDelegate.register()
FormHandler.register()
app.setDelegate(AppDelegate.alloc().init())

let mask = StyleMask.titled | StyleMask.closable
let win = makeWindow("Demo", 0.0, 0.0, 480.0, 320.0, mask)
let handler = FormHandler.alloc().init()
let btn = makeButton("Click", 180.0, 140.0, 120.0, 32.0)
btn.setTarget(handler)
btn.setAction(selectorPtr("submit:"))
win.contentView.addSubview(btn)
win.makeKeyAndOrderFront(nil)
app.run()
```

lift された各クラスには NSObject (または指定された親) ObjC 親、セレクタ配線 (デフォルト = メソッド名 + パラメータ数ぶんの `:`)、`alloc` / `init` / `register` のスタブが自動挿入される。書くのは実装したいメソッドだけでよい。

- プロトコルで実装しなかったメソッドは ObjC 側で optional 扱い (プロトコル側に末尾 `?` が付いているもの) の no-op として残り、ランタイムは `respondsToSelector: NO` を返すだけ
- これらのクラスは ilang の通常クラスとして `new` で生成するものでは**ない** — lifted された `alloc().init()` ペア (`AppDelegate.alloc().init()`) 経由でインスタンス化する
- base list に複数の `@objc interface` を並べてもよい (`class X : NSWindowDelegate, NSMenuDelegate { ... }`)。`@objc class` 親と組み合わせれば「サブクラス + プロトコル」も書ける
- 以下の用途では明示的な `@extern(ObjC) { @objc class … { … } }` 形式に切り替える: raw ポインタ (`*u8`、`*const char` 等) を引数 / 戻り値で扱いたい、もしくはデフォルトのセレクタ形式に乗らない `@objc("selector:")` を指定したい場合

### SIMD ベクタ型 (`simd.fNxM` / `simd.iNxM`)

ハードウェアレジスタ1本に収まる固定幅 SIMD ベクタ。主な用途は Apple の `vector_floatN` / `vector_intN` 型を取る ObjC バインディングを通すこと。

```rust
let v: simd.f32x4 = [1.0, 2.0, 3.0, 4.0]   // 配列リテラルから構築
let p: simd.f32x2 = [0.5, 0.5]
let row: simd.f32x2[] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
```

#### 利用できる要素型 / レーン数の組み合わせ

| 型 | 要素 | レーン数 | 備考 |
| --- | --- | --- | --- |
| `simd.f32x2` | f32 | 2 | Apple `vector_float2` |
| `simd.f32x4` | f32 | 4 | Apple `vector_float4` |
| `simd.f64x2` | f64 | 2 | Apple `vector_double2` |
| `simd.i8x16` | i8  | 16 | Apple `vector_char16` |
| `simd.i16x8` | i16 | 8  | |
| `simd.i32x4` | i32 | 4  | Apple `vector_int4` |
| `simd.i64x2` | i64 | 2  | |

#### 構築方法

現状で SIMD 値を作る唯一の方法は、**型注釈付きスロットに対する配列リテラルの coerce** (`let v: simd.f32x4 = [...]`、関数の引数 / 戻り値、struct フィールドなど)。リテラル長はレーン数とちょうど一致する必要があり、各要素はレーンのスカラー型に収まること (整数リテラルは narrow、float リテラルは f64→f32 にスカラー代入と同じ規則で coerce される)。

```rust
fn make(): simd.f32x4 {
    [0.0, 0.0, 1.0, 1.0]                    // 戻り型に対して coerce
}
```

#### 対応している操作 / していない操作

- **first-class な演算はまだ公開していない** — 要素ごとの加算 / 乗算、レーンの extract / insert、shuffle はいずれも書けない。値を保持して引き渡すことはできるが、ilang 側で計算はできない
- **`simd.fNxM[]` 配列**は利用可能で、レーン幅に応じて連続レイアウト (16バイト / 8バイト アライン) される。`simd.f32x2[]` を `const vector_float2 *` を取る ObjC メソッドに渡すと、生成されたブリッジ内で暗黙の `arr as *const simd.f32x2` キャストが入る
- **`.x` / `.y` / `[i]` といったレーンアクセスは無い** — ObjC API を経由するか、計算が必要なら通常のスカラー配列で行い、境界部分でのみ SIMD に coerce する

### 組み込み `std.math` モジュール

```rust
use std.math as math
math.sqrt(16.0)              // 4.0
math.sin(math.pi / 2.0)      // 1.0  ← `math.pi` は const、parens 不要
math.pow(2.0, 10.0)          // 1024.0
math.atan2(1.0, 1.0)         // π/4
```

提供関数 (すべて f64):

- **三角関数**: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`
- **双曲線関数**: `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`
- **冪乗・対数**: `sqrt`, `cbrt`, `pow`, `hypot`, `exp`, `ln`, `log10`, `log2`
- **丸め**: `floor`, `ceil`, `round`, `trunc`
- **符号・絶対値**: `abs`, `sign`
- **最小・最大**: `min(a, b)`, `max(a, b)` — 固定 2 引数
- **ゲーム / 補間**: `clamp(v, lo, hi)`, `lerp(a, b, t)`, `smoothstep(edge0, edge1, x)`, `remap(v, fromLo, fromHi, toLo, toHi)`
- **乱数**: `random()` — `[0.0, 1.0)` の一様 `f64`

定数 (すべて `f64` `const`): `pi`, `e`, `ln2`, `ln10`, `log2e`, `log10e`, `sqrt2`, `sqrt1_2`。

### 組み込み `std.test` モジュール

自己アサーションのスクリプト + 結合テストフィクスチャ用。失敗時は stderr にメッセージを出して **exit code 2** で終了する。

```rust
use std.test as test
test.expect(1 + 2 * 3, 7)              // i64 同士
test.expectStr("ab" + "c", "abc")      // string 同士
test.expectBool(false, false)
test.expectF64(2.5 + 0.5, 3.0)
test.expectTrue(1 < 2)                 // 単一条件
test.expectFalse(1 > 2)
test.fail("should not reach here")    // 強制失敗
```

`crates/ilang-cli/tests/programs/*.il` に置いた `.il` ファイルは、ハーネス (`programs.rs`) が実行 + 終了コードを検証する。

### 組み込み `std.os` モジュール

OS レベルの状態にアクセスするための薄いヘルパー。errno の読み書きと、POSIX 標準のエラーコード定数を提供。

```rust
use std.os as os
use std.ffi { cstrFromString }

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
        // ファイルが見つからない
    } else if code == os.EACCES {
        // 権限エラー
    }
}
```

**関数:**
- `os.errno(): i32` — 現在のスレッドの `errno` を返す (Windows では `GetLastError()`)。失敗を示す値 (NULL / -1 / 0 など) を返した libc 呼び出しの直後に読むのが慣例
- `os.setErrno(code: i32)` — errno を上書き。`os.setErrno(0)` で「次の呼び出し失敗を確実に検出」したい時の前処理に
- `os.libLoaded(name: string): bool` — 指定の `@lib(...)` ライブラリがロードに成功したかを返す。`@optional` 付き fn を呼ぶ前のガードに
- `os.libLoadError(name: string): string` — ライブラリのロードに失敗した場合の dlopen エラーメッセージ。成功 (または未試行) なら空文字列。診断用、ガードロジックには `libLoaded` を使う
- `os.platform: string` — ホスト OS 名。`"macos"` / `"linux"` / `"windows"` のいずれか、それ以外の Rust 認識ターゲットでは `"other"` になる。ビルド時の `cfg(target_os)` で解決され、トップレベル `pub let` にキャッシュされているので、`()` なしのプロパティ風アクセスで読める:
  ```rust
  if os.platform == "windows" { ... } else { ... }
  ```
- 値はエラーが起きるまで持続する。次の libc 呼び出しが成功してもクリアされない (POSIX 仕様)

**定数 (i32):**
- **errno**: `EPERM`(1), `ENOENT`(2), `ESRCH`(3), `EINTR`(4), `EIO`(5), `ENXIO`(6), `E2BIG`(7), `ENOEXEC`(8), `EBADF`(9), `ECHILD`(10), `ENOMEM`(12), `EACCES`(13), `EFAULT`(14), `EBUSY`(16), `EEXIST`(17), `EXDEV`(18), `ENODEV`(19), `ENOTDIR`(20), `EISDIR`(21), `EINVAL`(22), `ENFILE`(23), `EMFILE`(24), `ENOTTY`(25), `ETXTBSY`(26), `EFBIG`(27), `ENOSPC`(28), `ESPIPE`(29), `EROFS`(30), `EMLINK`(31), `EPIPE`(32), `EDOM`(33), `ERANGE`(34)
- **標準 fd**: `STDIN_FILENO`(0), `STDOUT_FILENO`(1), `STDERR_FILENO`(2)
- **exit**: `EXIT_SUCCESS`(0), `EXIT_FAILURE`(1)
- **lseek whence**: `SEEK_SET`(0), `SEEK_CUR`(1), `SEEK_END`(2)
- **open() アクセス**: `O_RDONLY`(0), `O_WRONLY`(1), `O_RDWR`(2), `O_NONBLOCK`(4), `O_APPEND`(8)
- **ファイルモード bits**: `S_I[RWX][USR/GRP/OTH]` の 9 ビット (POSIX 標準値)
- **ソケット**: `AF_UNIX`(1), `AF_INET`(2), `AF_INET6`(30 — macOS 値; Linux=10), `SOCK_STREAM`(1), `SOCK_DGRAM`(2), `SOCK_RAW`(3)
- **シグナル**: `SIGINT`(2), `SIGQUIT`(3), `SIGILL`(4), `SIGABRT`(6), `SIGFPE`(8), `SIGKILL`(9), `SIGSEGV`(11), `SIGPIPE`(13), `SIGALRM`(14), `SIGTERM`(15)

値は macOS / Linux glibc で一致するもののみ収録。プラットフォームで異なる定数 (`EAGAIN`, `O_CREAT`, `O_TRUNC` 等) は意図的に含めず、必要ならドキュメント参照のうえハードコードするか、`@extern(C) { @lib("c") fn ... }` で直接呼び出すこと

Rust の C runtime の errno を直接読み書きする実装。

### 組み込み `std.regex` モジュール

正規表現エンジン。クラスとして提供される。ランタイムは Rust の [`regex`](https://docs.rs/regex) crate を薄くラップしているため、パターンは真の正規言語に限られる — 線形時間で高速にマッチするかわりに、**後方参照 (backreference) と先読み / 後読み (lookaround) は使えない**。

```rust
use std.regex as regex

let r = new regex.Regex("foo+", "i")

r.test("Hello FOOO")            // true
r.find("yes FOO no")            // some("FOO")
r.findAll("foo Foo FOO")        // ["foo", "Foo", "FOO"]
r.replace("foo and FOO", "X")   // "X and X"
r.split("a foo b Foo c")        // ["a ", " b ", " c"]
```

**構築:**
- `new regex.Regex(pattern: string, flags: string)` — パターンをコンパイル。不正なパターンを与えるとプロセスを abort する (他の「実行時に失敗しない構築」系の組み込みと同じ挙動)

**メソッド:**
- `test(s: string): bool` — `s` のどこかにマッチするか
- `find(s: string): string?` — 最初にマッチした部分文字列。なければ `none`
- `findAll(s: string): string[]` — 重ならない全マッチを左から順に
- `replace(s: string, replacement: string): string` — **すべての** マッチを `replacement` に置換。`$1`, `$2`, … でキャプチャグループを参照できる (regex crate の置換構文)
- `split(s: string): string[]` — マッチ位置で分割

**フラグ** (文字列で渡す。なしの場合は `""`):
- `i` — 大文字小文字を区別しない
- `m` — 複数行 (`^` / `$` が行境界にマッチ)
- `s` — `.` が改行にもマッチ
- `x` — 拡張モード / パターン中の空白を無視

未知のフラグ文字を渡すと診断メッセージを出して abort する

コンパイル済みパターンは不透明なハンドル経由で Rust ヒープ上に保持され、ラッパーの `deinit` で `Regex` オブジェクトの refcount が 0 になったタイミングで解放される。

### 組み込み `std.path` モジュール

Node.js 風のパス操作。**セパレータは常に `/` 固定** (ホスト OS によらない) — Windows 形式の `\\` パスを扱いたい場合は事前に `replace` で `/` に変換しておく。Pure ilang 実装、FFI なし、どこからでも安全に呼べる。

```rust
use std.path as path

path.basename("/foo/bar/baz.txt")        // "baz.txt"
path.basename("/foo/bar/baz.txt", ".txt") // "baz"
path.dirname("/foo/bar/baz.txt")          // "/foo/bar"
path.extname("a.tar.gz")                  // ".gz"
path.isAbsolute("/x")                     // true
path.join(["a", "..", "b"])               // "b"
path.normalize("/a//b/c/../d")            // "/a/b/d"
path.relative("/a/b/c", "/a/b/d")         // "../d"

let p = path.parse("/foo/bar/baz.txt")
// p.dir = "/foo/bar", p.root = "/", p.base = "baz.txt",
// p.name = "baz",     p.ext  = ".txt"
path.format(p)                            // "/foo/bar/baz.txt"
```

**定数:**
- `path.sep: string` — `"/"`
- `path.delimiter: string` — `":"` (PATH 環境変数の区切り文字)

**関数:**
- `basename(p)` / `basename(p, ext)` — 末尾セグメント。`ext` を渡すと末尾の拡張子を剥がす
- `dirname(p)` — 末尾セグメントを除いた残り
- `extname(p)` — 拡張子 (先頭ドット込み)。なければ `""`。先頭ドット + 名前 (`.bashrc`) はファイル名扱いで拡張子と見なさない (Node 互換)
- `isAbsolute(p): bool` — `/` 始まりかどうか
- `join(parts: string[]): string` — `/` で連結し normalize
- `normalize(p): string` — `//` / `.` / `..` を畳み込む。先頭 `/` と末尾 `/` の有無は保つ
- `relative(from, to): string` — 両側を normalize した上で相対パスを返す
- `parse(p): PathParts` — `{ dir, root, base, name, ext }` に分解
- `format(parts: PathParts): string` — `parse` の逆

`PathParts` は public class。`format` に渡す独自の値を作りたい場合は `new PathParts(...)` で組み立てればよい。

### 組み込み `std.events` モジュール

Node.js 風の最小 EventEmitter。ペイロード型ひとつで generic、リスナーは登録順に同期実行。Pure ilang 実装、FFI なし。

```rust
use std.events as events

let bus = new events.EventEmitter<i32>()

let listener = fn(n: i32) { console.log("tick", n) }
bus.on("tick", listener)
bus.emit("tick", 1)                       // → "tick 1"

bus.off("tick", listener)                 // この listener だけ削除
bus.removeAllListeners("tick")            // 全部削除
```

**API (`EventEmitter<T>`):**
- `on(event: string, listener: fn(T))` — 登録
- `off(event: string, listener: fn(T)): bool` — `fn` 値が等しい (参照等価 — `on` に渡したのと同じ値を渡す) listener を削除。見つかれば `true`
- `emit(event: string, value: T)` — 登録順に同期で発火
- `removeAllListeners(event: string)` — 該当イベントの listener を全削除
- `listenerCount(event: string): i32` — 登録数

**Node.js 版との違い:**
- 1 emitter につきペイロード型ひとつ。複数値を渡したいときは struct / class でまとめる

### 組み込み `Promise<T>` と event loop

`Promise<T>` は非同期に到着する値を表す組み込みクラス。実行モデルは JavaScript と同じ run-to-completion: ユーザーコード (`main`・executor・`.then` callback・async fn の再開・タイマー callback) は**すべて単一スレッド**で実行される。executor は `new Promise(...)` のその場で同期実行。継続は FIFO queue に積まれ、**drain ポイントでのみ**実行される — 自分のコードの実行中に callback が割り込むことは決してないので、データ競合は構造上起きない。drain ポイントは (1) プログラム終了時 (runtime が pending な継続とタイマーを drain してから終了するので、トップレベルの `.then` は必ず発火する)、(2) 自前のメインループを持つプログラム (GUI / ゲームのフレームループ) が明示的に呼ぶ `time.tick()`。タイマー (`time.setTimeout` / `setInterval`) は期限順の heap に載り、終了時 drain は期限まで待って発火させる — 未発火のタイマーはプロセスを生かし続ける (Node.js と同じ)。

```rust
// 即解決。
Promise.resolve("hello").then(fn(s: string) {
    console.log(s)              // → hello
})

// JS 同等の値変換チェーン。各 .then は新しい Promise<U> を返す。
Promise.resolve(21)
    .then(fn(n: i64): i64 { n * 2 })
    .then(fn(n: i64) { console.log(n.toString()) })  // → 42

// executor。resolve/reject の最初に呼ばれた方が確定する。
let p = new Promise<string>(fn(resolve: fn(string), reject: fn(string)) {
    if some_cond { resolve("ok") } else { reject("oops") }
})
p.catch(fn(msg: string): string {
    "recovered: " + msg
})

// 集約コンビネータ。
let all = Promise.all<string>([
    Promise.resolve("a"),
    Promise.resolve("b"),
])
all.then(fn(vs: string[]) { ... })   // 全員揃ってから 1 度

let first = Promise.race<string>([p1, p2])
first.then(fn(v: string) { ... })    // 最初に settle した方
```

**API:**
- `Promise.resolve<T>(v: T): Promise<T>` — 既に解決済み
- `Promise.reject(msg: string): Promise<()>` — 既に reject 済み (rejection は値を持たないので `T = ()`。型付き reject は executor で)
- `new Promise<T>(executor: fn(fn(T), fn(string)))` — `executor(resolve, reject)` をその場で同期実行。最初の呼び出しが採用される。`.then` で登録した callback は promise が settle 済みでも drain ポイントまで実行されない
- `p.then<U>(cb: fn(T): U): Promise<U>` — 解決値を受け取る callback を登録、新しいチェーン promise を返す。rejection は `.then` を素通りして次の `.catch` に届く
- `p.catch(cb: fn(string): T): Promise<T>` — rejection を捕まえて upstream 同型の値に復帰
- `Promise.all<T>(ps: Promise<T>[]): Promise<T[]>` — 全部 settle 後にまとめて解決、最初の rejection で reject
- `Promise.race<T>(ps: Promise<T>[]): Promise<T>` — 最初に settle した方 (resolve/reject どちらでも) で確定

**JS との違い:**
- 1 promise につき値型ひとつ — `T` は構築時に固定。union が欲しいときは enum / class でラップ
- `Promise.reject(msg)` は `Promise<()>` を返す (call site の expected type を遡る型推論を持たないため)。型付き rejection が必要なら executor を使う
- `.catch` の handler は upstream と同じ `T` を返す必要がある (`Promise<T | U>` のような union 化はしない)

### `async` / `await`

`async fn foo(args): T { ... }` は呼び出し側に `Promise<T>` を返す。本体内で `await expr` を書くと、対象の `Promise<U>` が settle するまで関数を suspend し、`U` を取り出す。

await 対象が **reject** で settle した場合は JS と同じ意味論: 本体はその `await` から先へ再開されず、async fn 自身の結果 promise が同じメッセージで reject される。呼び出し側は `.catch` で受け取る。

```rust
async fn doubleAsync(p: Promise<i64>): i64 {
    let x: i64 = await p
    x * 2
}

doubleAsync(Promise.resolve(21)).then(fn(n: i64) {
    console.log(n.toString())        // → 42
})

async fn sumThree(a: Promise<i64>, b: Promise<i64>, c: Promise<i64>): i64 {
    let x: i64 = await a
    let y: i64 = await b
    let z: i64 = await c
    x + y + z
}
```

内部的には `async fn foo` は小さな状態機械に展開されます。実行時のエラーやスタックには `__foo_State` / `__foo_StateRef` / `__foo_poll` といった生成名が現れることがあります。

**`await` の書ける位置**

```rust
// 順次に await
async fn pair(a: Promise<i64>, b: Promise<i64>): i64 {
    let x = await a
    let y = await b
    x + y
}

// 式の途中で await(左→右の順に評価される)
async fn sum_args(a: Promise<i64>, b: Promise<i64>): i64 {
    (await a) + (await b)
}

// if の条件、match の対象に直接書ける
async fn pick(p: Promise<i64>): i64 {
    if await p > 0 { 1 } else { -1 }
}
async fn handle(p: Promise<i64>): i64 {
    match await p {
        0 { -1 }
        n { n * 2 }
    }
}

// class のメソッドにも書ける
class Worker {
    pub init() {}
    pub async fn run(p: Promise<i64>): i64 {
        let v = await p
        v * 2
    }
}
```

**制約**

- `while` の条件式に `await` は書けません(毎反復評価する位置のため)。ループの本体内で `let` バインディングを経由します:

  ```rust
  // ❌ 条件位置の await
  async fn poll(p: Promise<bool>): i64 {
      while await p { /* ... */ }
      0
  }
  // ⭕ 本体で受けてから break
  async fn poll(p: Promise<bool>): i64 {
      let count = 0
      while true {
          let ready = await p
          if !ready { break }
          count = count + 1
      }
      count
  }
  ```

- ラムダ(`fn(...) { ... }`)の本体には `await` を書けません。ラムダ自体を async にする仕組みが無いためで、ラムダの外で受け取って渡します。

  ```rust
  // ❌
  let f = fn(p: Promise<i64>): i64 { await p }
  // ⭕ async fn として書く
  async fn f(p: Promise<i64>): i64 { await p }
  ```

- `break v`(値付き break)は async の `while` の中では未対応。`break` 単独と `continue` は使えます。

- `let x = ...` の RHS の形が desugar 側のミニ型推論器の範囲を超えている場合、型注釈が必要です。リテラル / `param` 参照 / `await Var` / `await fn(...)` / `await Promise.resolve(...)` / 配列リテラル / 単純な算術 / フィールド・メソッド呼び出しは推論できます。それ以外で `let x = ...` の型が決まらないと desugar 時にエラーになるので `let x: T = ...` で型を書きます。

- top-level に `await` は書けません(top-level let が他の top-level fn から見える ilang のスコープ規則と、`await` 直前/直後をひとつのスコープに収める実装が両立しないため)。`.then(...)` で繋ぐか、`async fn` でくるんで kick します:

  ```rust
  // ❌
  let v = await Promise.resolve(42)
  console.log(v.toString())

  // ⭕ .then で繋ぐ
  let _ = Promise.resolve(42).then(fn(v: i64) {
      console.log(v.toString())
  })

  // ⭕ async fn にまとめて kick
  async fn run(): i64 {
      let v = await Promise.resolve(42)
      console.log(v.toString())
      0
  }
  let _ = run()
  ```

- `throw` 構文が無いので、`async fn` の中から reject するには `Promise.reject(...)` か executor を使います:

  ```rust
  // executor 経由で型付き reject
  async fn parse(s: string): Promise<i64> {
      new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
          if s == "" { reject("empty") } else { resolve(0) }
      })
  }
  ```

### `const` (定数宣言)

トップレベルの不変束縛。loader はまず RHS をコンパイル時に literal へ fold して各参照に inline しようとし、fold できない式 (関数呼び出し、`new`、フィールド/メソッドアクセスなど) の場合は **モジュール初期化時に一度だけ評価される runtime 初期化** に降格します。どちらの経路でも再代入は型チェッカが拒否します。

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

- コンパイル時 fold で扱える演算: 算術 (`+ - * / %`)、ビット (`& | ^ << >> ~`)、比較 (`== != < <= > >=`)、論理 (`&& || !`)、文字列連結 (`+`)、`as` キャスト (数値間)、同じファイルで宣言済の他の `const` の参照 (宣言順で folding するため前方参照は不可)
- 上記以外 (関数呼び出し、`new`、フィールド / メソッドアクセス、配列、`if` / `match`、ループ等) は runtime 経路に落ちます。RHS はモジュール初期化時に宣言順で **一度だけ** 評価され、その値が以降の参照に束縛されます。モジュール越しの参照でも同じ値が見えます
- runtime 経路で救えないエラー (fold 対象式の中の 0 除算、型注釈に収まらない数値リテラル等) は **コンパイル時エラー**
- 型注釈 (`: T`) は省略可 (推論)
- 同梱 `math` モジュールの `pi` / `e` は fold 経路、`os.platform` は runtime 経路 (ホストの intrinsic 呼び出しで一度初期化)

#### `@embed("path") const X: T` — ファイル埋め込み

`const` の値を **コンパイル時にファイルから読み込む** 形式。Zig の `@embedFile` 相当。パスは **宣言が書かれているソースファイル** からの相対で解決される。

```rust
@embed("assets/banner.txt") const BANNER: string
@embed("assets/icon.png")   const ICON_BYTES: u8[]

console.log(BANNER)
console.log(ICON_BYTES.length)
```

- `@embed` 付き const に `= ...` は書けない (値はファイル由来)。型注釈は **必須**
- `: string` の場合、ファイルは UTF-8 として読まれる。不正な UTF-8 はコンパイルエラー (バイナリファイルは `u8[]` を使う)
- `: u8[]` の場合、ファイルは生バイト列として読まれる。各バイトが `u8` 要素になる。大きい埋め込みは runtime の配列初期化 (プログラム起動時に materialise) として残る
- それ以外の型注釈は拒否される
- ファイルが見つからなければ通常の loader エラーになる

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
| JIT | `ilang run path.il` | AST → MIR → Cranelift IR → プロセス内のネイティブコードを生成して実行 |
| AOT | `ilang build path.il -o out` | 同じパイプラインで Mach-O / ELF / COFF オブジェクトを書き出し、リンクして単独実行ファイルを作成 |
| REPL | `ilang` (引数なし) | 同じ JIT で 1 チャンクずつ評価。`let` / `fn` / `class` 等は次のチャンクにも引き継がれる |

`run` と `build` は同じ lowering パス (`ilang_parser::loader` → `ilang_types::TypeChecker` → `ilang_mir::lower_program` → `ilang_mir_codegen::compile_program`) を通り、Cranelift がメモリ上にコードを置くか (JIT) オブジェクトファイルに書き出すか (AOT) の違いだけ。

CLI はエントリファイルから親方向に `ilang.toml` を探し、見つかれば `[deps]` の各エントリを loader の `use module` 解決の追加検索ディレクトリに加える。

`[deps]` のエントリは次の3つの形を受け付ける:

```toml
[package]
name = "my_game"

[deps]
common = "./libs/common"                       # 文字列
math   = { path = "./libs/math" }              # 単一テーブル

[[deps.gui_impl]]                              # 配列形式 (OS 振り分け)
path   = "./libs/gui-cocoa"
target = "macos"

[[deps.gui_impl]]
path   = "./libs/gui-win32"
target = "windows"
```

`target` は単一の OS 文字列 (`"macos"` / `"linux"` / `"windows"`) か、複数の OS 文字列の配列 (`["macos", "linux"]`、OR マッチ) を受け付ける。ホスト OS と一致しないエントリは静かに破棄される。同じ dep 名で複数のエントリがホスト OS にマッチした場合はエラー — ホストごとにちょうど 1 つだけ残るように排他的な `target` を書くこと。

ルックアップ順: import 元自身のディレクトリ → 宣言された各 dep ディレクトリ。

---

## 17. 未実装 (今後の TODO)

- **Iterator プロトコル** (ユーザ型に `next()` を実装させて `for-in` に乗せる)
- **名前付き引数** (`open(path: "x", mode: "w")`) — デフォルト引数は実装済み、名前付き呼び出しは未実装
- **演算子オーバーロード** (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- **ジェネリック制約 (bounds)**
- **親メソッド名のサブクラス側オーバーロード** — 親が宣言していない名前のオーバーロードは §7 に従い可能だが、親と同じ名前を再利用する場合は単一シグネチャの `override` 形式が必須
- **静的フィールド/メソッドの継承** (Phase 2)
- **ジェネリッククラスでの継承 / 静的メンバー / プロパティ** (型パラメータ解決の制約により未対応)

### 採用しない方針

- **例外 (`throw` / `try` / `catch`)**: 採用しない。失敗するかもしれない関数は `Result<T, E>` で表現し、`match` で処理する。回復不能なバグ (ゼロ除算、配列範囲外、`unwrap()` on `none`) は **panic** として実行を停止 (catch 不可)。
  - 理由: 制御フローがシグネチャに現れる、型システムを抜けない、ARC との相性。Rust / Go / Zig などと同じ方針。

---

詳細な内部設計や引き継ぎノートは [`HANDOFF.md`](HANDOFF.md) を参照。
