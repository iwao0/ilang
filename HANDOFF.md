# ilang HANDOFF

新しいセッションに引き継ぐ用。`/clear` 後にこのファイルを読めば現状の文脈を把握できる構成。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS 風のクラス** (`class`/`new`/`this`)、Rust 風の関数宣言、TypeScript 風の戻り値型注釈
- 四則演算規則は **C / JavaScript** とほぼ同一
- 文末は **JS 風 ASI** (改行が `;` 代わり)

実装言語: **Rust 1.95**。実行モデル: ツリーウォーク型インタプリタ + **Cranelift JIT** (`ilang run --jit`)。LLVM は後付け候補。

## 現在地 (フェーズ6/7: Cranelift JIT + ARC Phase A-E + Optional/Weak 完了)

最新コミット `3c7c2ee` (`docs: add syntax cheatsheet`)。**245 テスト** 全通過、警告ゼロ。
構文一覧は [`docs/syntax.md`](docs/syntax.md) に集約済み。

### 完了フェーズ
| フェーズ | 内容 | 代表コミット |
| --- | --- | --- |
| 1 | 数値の四則演算インタプリタ | `437fc16` |
| 2 | `let` / 関数定義 / `#[requires(...)]` パース / 最小型チェッカー | `3301d17` |
| 3 | bool / 比較 / 短絡論理 / `if`/`while` / 代入 | `b3be50b` |
| 4 | クラス (`class`/`new`/`this`/`init`) | `26965bf` |
| 5a | `loop` / `break` / `continue` | `6e05c7f` |
| 5b | エラー span 統一 (`filename [row:col]: msg`) | `4c37a30` |
| 5c | `deinit` (スコープ脱出時、ARC ではなく refcount==1 検出) | `c78c0ab` |
| 5d | `console.log` (variadic、組み込み Console クラス) | `7d4d1f2` `3c8d375` |
| 5e | ビット演算 (`&` `\|` `^` `~` `<<` `>>` + 複合代入) | `2a7e2a0` |
| 5f | 16 進/2 進リテラル + `_` 桁区切り | `b1ac6ee` |
| 5g | 全 10 数値型 (`i8/i16/i32/i64/u8/u16/u32/u64/f32/f64`) + `as` キャスト | `5bb6c82` `c1cacc6` |
| 5h | 文字列型 (`+` 連結、`==` `!=`) | `a8e40e5` |
| 5i | 配列 (`T[]` / `T[N]` + `.length` / `.push`) | `9d61e51` |
| 5j | 数値型サフィックス (`1_i32` `1.5f32`) | `7be28d3` |
| 6a | **Cranelift JIT 導入** (i64/bool/制御フロー/`fn`) | `9cafa6f` `9702258` |
| 6b | JIT: 全数値型 + `as` キャスト | `2f68f53` |
| 6c | JIT: クラス (alloc + メソッド/フィールド + `this`、ARC なし) | `4d54a18` |
| 6d | JIT: `console.log` | `803e49d` |
| 6e | JIT: 文字列 | `a070d7a` |
| 6f | JIT: 配列 | `379b4a7` |
| 6g | **JIT ARC Phase A** (object のみ、refcount + deinit + free) | `38360f6` |
| 6h | **JIT ARC Phase B** (string / array コンテナの refcount + retain/release) | `a116712` |
| 6i | `ilang-codegen` を 11 ファイルに分割 (runtime/ty/value/env/arc/lower_*/compiler/drops) | `726784b` `405a500` `e85c246` |
| 6j | **JIT ARC Phase C** (代入上書き release、関数引数/戻り値の厳密化、捨てられる中間値の release) | `ddf13cc` |
| 6k | **JIT ARC Phase D** (フィールド/配列要素の再帰 release、`__drop_<C>` / `__drop_arr_<id>` JIT 生成) | `d6e0d96` |
| 7a | **Optional `T?`** (interpreter): `none` / `some(x)` / `if let some(v) = x { ... }` / `is_some` / `is_none` / `unwrap` / `T → T?` 自動 wrap | `35a65a5` |
| 7b | **Optional in JIT** (heap inner 限定): nullable pointer 表現、JitTy::Optional + 内部型 interning | `373af5f` |
| 7c | **Weak `T.weak`** (interpreter): `Value::Weak`、`.get(): T?`、strong → weak 自動 downgrade、循環参照解消 | `7b98d15` |
| 7d | **Weak in JIT** (二重 rc): object header 24 byte 化 (strong/weak/drop_fn)、release_object の sentinel weak +/- across drop_fn | `d2861e5` |

### 設計的な大幅整理
- AST に `Span` を持たせて全エラーが `[row:col]` を出力 (`4c37a30`)
- 暗黙 `this` (メソッド本体内でフィールド/メソッドを `this.` なしで参照可) (`0389b8d`)
- 複合代入 `+=` `-=` 等 (`1487d36`)
- メモリリーク対策: object のフィールドや配列要素に保持された Foo の deinit を再帰発火 (`9992cd6`、ただし interpreter のみ)
- 設計レビューの結果に基づく fix: `numeric_result` 厳密化 + `Console`/`console` 予約名 + 空配列メッセージ + recursive release (#1-#3) + `MixedSignedness` ヒント

## ワークスペース構成

```
crates/
├── ilang-ast/       # 純粋な AST 定義 (型・式・文・項目、Span は ast 側)
├── ilang-lexer/     # 字句解析 (Token, leading_newline フラグ、numeric_suffix)
├── ilang-parser/    # Pratt 構文解析
├── ilang-types/     # 型チェッカー (built-in Console + 予約名チェック含む)
├── ilang-eval/      # ツリーウォーク評価器 (REPL 状態は Interpreter が保持)
├── ilang-codegen/   # **Cranelift JIT**: 役割別 11 ファイル (runtime / ty / value / env / arc / lower_op / lower_ctrl / lower_stmt / lower_expr / drops / compiler)
└── ilang-cli/       # `ilang` バイナリ (REPL + `ilang run [--jit] path.il`)
docs/
├── phase1-plan.md   # 四則演算 + Cranelift 採用の経緯
├── phase2-plan.md   # let / 関数 / capability 構文
├── phase3-plan.md   # 制御構造 (mut なし、全変数可変方針)
└── phase4-plan.md   # class (継承等は未実装の決定理由含む)
```

各 crate は `lib.rs` がほぼ re-export だけで、実体は役割別ファイル (例 `ilang-eval/src/{value,error,ops,interpreter}.rs`)。**新コードを書くときも同様に役割別モジュールに分けること**。テストは `crates/<crate>/tests/<name>.rs` の統合テスト形式 (Rust 慣例に合わせた)。

## 言語仕様の現状

```rust
// 関数 — 戻り値型は `: T` (TS 風)
fn add(a: i64, b: i64): i64 { a + b }

// 属性 — パースは通るが enforce はまだ
#[requires(net)]
fn fetch(id: i64): i64 { id * 100 }

// 変数 — `mut` なし、全部再代入可 (JS/Python 風)
let n = 0
let i = 1                 // ; は省略可、改行で区切られる
while i <= 10 {
    n = n + i             // 代入は外側スコープに伝播 (let の shadow は復元)
    i = i + 1
}

// クラス — JS 風、`init` がコンストラクタ、`deinit` がデストラクタ
class Counter {
    count: i64
    init(start: i64) { this.count = start }
    increment(): i64 {
        count += 1            // 暗黙 this (`this.` 省略可)
        count
    }
    deinit() { /* スコープ脱出時に発火 */ }
}
let c = new Counter(10)
c.increment()
console.log("count is:", c.increment())   // count is: 12

// 配列
let xs: i32[] = [1, 2, 3]
xs.push(4)
console.log(xs.length, xs[0])

// 文字列
let s = "hello, " + "world"
console.log(s)
```

`ilang run --jit foo.il` で Cranelift 経由の JIT 実行 (interpreter の数十〜数百倍速い)。

### 型
- 整数: `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64`
- 浮動: `f32` `f64`
- その他: `bool`, `string`, `()` (Unit)
- ユーザ定義: `Type::Object(class_name)` (クラス名と同一)
- 配列: `T[]` (動的) と `T[N]` (固定長)
- **Optional**: `T?` (= `Type::Optional(Box<T>)`) — `none` / `some(x)` / `if let some(v) = x` / `is_some` / `is_none` / `unwrap`。`T → T?` は暗黙 wrap
- **Weak**: `T.weak` (= `Type::Weak(Box<Type::Object(_)>)`) — クラス専用。`.get(): T?` で生存時 upgrade。`Foo → Foo.weak` は暗黙 downgrade
- 暗黙変換: 同一符号同士の整数間 / 整数 → 浮動 / float → float (両方向)。**符号またぎ** と **float → int** は `as` 必須
- 数値リテラルサフィックス: `1_i32` `1.5f32` `0xff_u8` 等

### 演算子優先順位 (低 → 高、C/JS 互換)
| 演算子 | l_bp / r_bp | 結合性 |
| --- | --- | --- |
| `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` | 2 / 1 | 右 |
| `\|\|` | 3 / 4 | 左 |
| `&&` | 5 / 6 | 左 |
| `\|` (BitOr) | 7 / 8 | 左 |
| `^` (BitXor) | 9 / 10 | 左 |
| `&` (BitAnd) | 11 / 12 | 左 |
| `==` `!=` | 13 / 14 | 左 |
| `<` `<=` `>` `>=` | 15 / 16 | 左 |
| `<<` `>>` | 17 / 18 | 左 |
| `+` `-` | 19 / 20 | 左 |
| `*` `/` `%` | 21 / 22 | 左 |
| `as` (cast) | 23 / — | 後置 |
| 単項 `-` `+` `!` `~` | — / 30 | 前置 |
| 後置 `.` (field/method) / `[]` (index) | (parse_postfix で個別処理) | 左 |

### ASI (自動セミコロン挿入)
- `Token` に `leading_newline: bool` がある (lexer が設定)
- `parser/parser.rs` の `consume_stmt_terminator` と `classify_expr_end` で「`;` / 改行 / `}`/`Eof`」を文区切りとして受理
- **式の途中の改行は無視** (`let x = 1\n + 2` は `let x = 1 + 2`)
- 改行は LF / CRLF どちらも対応 (CR 単独は非対応)

## 重要な設計決定 (引き継ぎ要)

| 領域 | 決定 | 理由 / メモ |
| --- | --- | --- |
| 所有権 / `mut` | **採用しない** (Rust 風には寄せない) | ARC 前提。全変数再代入可 |
| 戻り値型構文 | `): T { ... }` | TS 風統一 (`->` トークンは削除済み) |
| ファイル拡張子 | `.il` | 確定 |
| `i64` オーバーフロー | `RuntimeError::Overflow` (Rust 流) | フェーズ完了後の再検討候補 |
| capability | アノテーション方式 (案C: `#[requires(net)]` + 内部 capability 値) | enforce はフェーズ5以降 |
| ネイティブコード | **Cranelift 第一候補**、LLVM は後付け | 軽量さとビルドコスト優先 |
| クラス継承 / static / private | スコープ外 | "案A: 最小" を採用済み |
| コンストラクタ名 | `init` (Swift 風) | キーワードではなく特殊メソッド名 |
| オブジェクト等価性 | 参照等価 (`Rc::ptr_eq`) | structural equality は将来トレイト経由 |

## JIT (`ilang-codegen`) のサブセットと ARC 状況

`ilang run --jit foo.il` で Cranelift 経由のネイティブコード実行。デフォルトはツリーウォーク (互換性維持)。

### JIT 対応済み
- 全 10 数値型 + bool + `as` キャスト
- `if`/`else`/`while`/`loop`/`break`/`continue`、`fn` 定義 + 再帰
- クラス (`new`/`obj.field`/`obj.method()`/`this`/暗黙 this/`init`/`deinit`)
- 文字列 (リテラル、`+`、`==` `!=`)
- 配列 (`T[]`/`T[N]`/`a[i]`/`a.length`/`a.push(x)`)
- `console.log(...)` (variadic、型別 FFI 関数で出力)
- **Optional `T?`** (heap inner 限定 — Object/Str/Array/Weak): nullable pointer 表現、`none`/`some(x)`/`if let`/`is_some`/`is_none`/`unwrap`
- **Weak `T.weak`**: 二重 rc、`.get(): T?` で安全 upgrade、循環参照解消
- **ARC Phase A-E 全て完了**: refcount + retain/release の厳密化 + フィールド/要素の再帰 release + 循環参照解消

### JIT 未対応 (interpreter にフォールバック必要)
- **継承** / 動的ディスパッチ (interpreter にも未実装)
- **Optional の primitive 内部** (`i64?` 等) — タグ付き 16 byte レイアウトが必要、未対応 (interpreter は OK)

### JIT メモリレイアウト
- Object: `[strong_rc | weak_rc | drop_fn_ptr | field0 | ...]` (3 × i64 = 24 byte ヘッダ。ユーザポインタは field0 を指す)
  - strong_rc=0 で drop_fn 発火 (user deinit + heap field 再帰 release)、storage は weak_rc=0 で解放
  - weak_rc=0 かつ strong_rc=0 で storage 解放
- String: `Box<StringRc { rc, s }>` (リテラルは saturated rc で保護、release では解放されない)
- Array: `[rc | drop_fn | len | cap | data_ptr]` (5 × i64 = 40 byte) ヘッダ + 別領域データバッファ。drop_fn は要素ループで release を発火する JIT 生成ラッパ (heap 要素のときのみ非 0)
- Optional<T>: T と同じ i64 ポインタ (0 = none)。inner 型情報は compiler の `optional_inners` 側テーブルで管理 (`JitTy::Optional(u32)`)
- Weak<T>: T と同じ i64 ポインタ。retain/release は weak 側 helper にディスパッチ

### JIT 設計上の核心ルール
- **caller 側で aliased な heap 引数を retain**、callee 側で param を関数出口で release (deinit の `this` は除く — `release_object` が lifecycle を持つので二重に release すると無限再帰)
- ブロック終了時に新規 heap binding を **LIFO 順** で release (依存する binding が先に解放される問題を回避)
- `let y = x` / 関数引数 / `obj.field = x` / `a[i] = x` で borrowed 元 (Var/Field/Index/This) なら retain (fresh 値はそのまま rc=1 を譲渡)
- **`emit_bind_retain` ヘルパが bind 時 rc 調整を集約**: heap 一般則 + 「fresh strong → weak 変換時は retain_weak + release_strong」の特殊ケース
- 代入上書き (`x = newval` / `obj.field = ...` / `a[i] = ...`) は **新値 retain → store → 旧値 release** の順でバランス維持
- ブロック / `__main` の **tail retain は aliased heap 値のときだけ** (fresh tail は rc=1 のまま渡す。無条件に retain すると `fn f(): Foo { new Foo() }` が leak する)
- str_concat / str_eq の fresh オペランドは呼び出し後に release
- StmtKind::Expr で捨てられる fresh heap 値も release
- **release_object は drop_fn 呼び出し中 weak_rc を sentinel +1**: drop_fn 中の back-edge weak 解放で自己 storage が早期解放されるのを防ぐ
- **クラス宣言は 2 段階**: `declare_class_name` で名前→id だけ登録し、`finalize_class_layout` でフィールド型を解決。`Child { p: Parent.weak }` のような前方参照に対応

## 既知の制限 / TODO

### 言語機能
- **未初期化フィールド読み出しはランタイムエラー**: 型チェッカーは definite initialization を追跡しない
- **同一行に複数のクラスメンバー**: ASI は効かない (`;` か改行が必要)
- **クラス自体への属性**: `#[requires(...)]` は今のところメソッドにしか付与不可
- **`return` 文なし**: 関数/メソッドは末尾式が戻り値
- **辞書 / `match` / ADT**: 未実装
- **`use` / モジュール / インポート**: 未実装
- **文字列メソッド** (`length`/`charAt`/補間): なし
- **配列メソッド** (`pop`/`slice`/`for-of`): なし
- **`pop()` / 例外**: なし

### ARC / メモリ
- ARC は **Phase A-E 完了** (Optional + Weak + 二重 rc)。`T.weak` で循環参照は解消可能
- interpreter は scope-based deinit、JIT は refcount-based。両者で `deinit` の発火順序が完全には一致しないケースがある (interpreter は scope 内の到達順、JIT は rc=0 到達順)
- JIT で `i64?` などの **primitive Optional** は未対応 (タグ付き 16 byte レイアウトが必要、interpreter は OK)

## 次の候補 (次フェーズの選択肢)

ユーザーと相談して選ぶこと。現時点で未着手 / 未完のもの:

### A. capability の enforce ← **サプライチェーン対策の核 (元々の言語ビジョン)**
- 呼び出し側にも `#[requires(...)]` を要求するチェック
- `use http_client with cap(net = env.net)` のような import 構文
- クラスに capability を持たせる構文設計
- MIR を挟むタイミングで一緒に入れるのが自然 (関数呼び出しグラフが扱いやすい)

### B. 言語機能拡張
- `for-of` ループ / 配列メソッド (`pop`/`slice`)
- 文字列メソッド (`length`/`charAt`/補間)
- `return` 文 (早期 return)
- `match` + `enum` (ADT) — 大きい
- 継承 (`extends` / `super`) — 必要性を議論してから
- 辞書型
- `use` / モジュール / インポート (capability の前提でもある)

### C. JIT の primitive Optional 対応
- `i64?` / `bool?` 等を JIT で扱えるようにする
- タグ付き 16 byte レイアウト or null sentinel (整数値のうち 1 つを none に予約)
- 現状 interpreter にフォールバックで動作はする — 必要性は低め

### D. `ilang-mir` 中間表現 (未着手)
現状は AST → Cranelift IR を直接 lowering している (`crates/ilang-codegen/src/lower_*.rs` 群)。MIR を挟むと:
- 複数バックエンド (LLVM / wasm / バイトコード VM) を後付けしやすい
- `lower.rs` の「signed?/float?/widen?」分岐を 1 度で解決できる
- constant folding / dead code elim 等の最適化を載せる足場になる
- capability enforce (`#[requires(...)]`) の検査箇所として最適
- tree-walk より速い軽量バイトコード VM が書ける (JIT 起動コストを払いたくない短いスクリプト用)
2 段階で導入想定: ① MIR 定義 + AST → MIR、② MIR → CLIF (現 codegen の書き換え)。LLVM/wasm/VM が欲しくなったタイミングが導入の合図。
   2 段階で導入する想定: ① MIR 定義 + AST → MIR、② MIR → CLIF (現 codegen の書き換え)。LLVM/wasm/VM が欲しくなったタイミングが導入の合図

## 開発フロー

```sh
# テスト全実行
~/.cargo/bin/cargo test --workspace

# REPL (let / fn / class が永続化)
~/.cargo/bin/cargo run -p ilang-cli

# ファイル実行
~/.cargo/bin/cargo run -p ilang-cli -- run path.il
```

`source "$HOME/.cargo/env"` を毎回実行すると権限プロンプトが出る (settings.local.json の Bash allow が `Bash` 単独だと効かない実例を確認)。**`~/.cargo/bin/cargo` 直接呼びを推奨**。

### コミット方針
- 機能単位で 1 コミット (フェーズ単位で大きくまとめる)
- メッセージは英語、`Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` を付与
- ユーザーが「コミットして」と言うまでコミットしない

### コードスタイル
- コメントは「なぜ」だけ書く (「なに」は書かない)
- テストは統合テスト (`tests/` ディレクトリ) に置く
- 公開 API のみ `pub use` で再エクスポート
- 警告ゼロを維持 (`cargo build --workspace` で検証)
- 役割別モジュール分割を維持 (大きな lib.rs にしない)

## 会話のトーンと言語

- ユーザーへの返答は **日本語** (このファイルも日本語)
- コード/識別子/コミットメッセージは英語
- 提案する場合は 2-3 案のトレードオフを示し、推奨を 1 つ明示
- ユーザーが選んだ後は実装まで進める (確認は重要なところだけ)
