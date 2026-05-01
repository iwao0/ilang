# ilang HANDOFF

新しいセッションに引き継ぐ用。`/clear` 後にこのファイルを読めば現状の文脈を把握できる構成。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS 風のクラス** (`class`/`new`/`this`)、Rust 風の関数宣言、TypeScript 風の戻り値型注釈
- 四則演算規則は **C / JavaScript** とほぼ同一
- 文末は **JS 風 ASI** (改行が `;` 代わり)

実装言語: **Rust 1.95**。実行モデル: ツリーウォーク型インタプリタ。最終的には [docs/phase1-plan.md](docs/phase1-plan.md) のロードマップ通り **Cranelift 経由のネイティブコード化** を目指す (LLVM は後付け候補)。

## 現在地 (フェーズ4 完了)

`26965bf` (`Phase 4: classes`) が最新コミット。**77 テスト** 全通過。

| フェーズ | 内容 | コミット |
| --- | --- | --- |
| 1 | 数値の四則演算インタプリタ | `437fc16` |
| 2 | `let` / 関数定義 / `#[requires(...)]` パース / 最小型チェッカー | `3301d17` |
| 3 | bool / 比較 / 短絡論理 / `if`/`while` / 代入 | `b3be50b` |
| 4 | クラス (`class`/`new`/`this`/`init`) | `26965bf` |

途中で:
- 各 crate の `lib.rs` を役割別モジュールに分割 (`712d2cb`)
- テストを `tests/` ディレクトリに移動して統合テスト化 (`0f99194`)
- 戻り値型を `-> T` から `: T` に変更 (`064b708`)
- 改行を文区切りに (ASI) (`3535983`)

## ワークスペース構成

```
crates/
├── ilang-ast/       # 純粋な AST 定義 (型・式・文・項目)
├── ilang-lexer/     # 字句解析 (Token, Span, leading_newline フラグ)
├── ilang-parser/    # Pratt 構文解析
├── ilang-types/     # 型チェッカー
├── ilang-eval/      # ツリーウォーク評価器 (REPL 状態は Interpreter が保持)
└── ilang-cli/       # `ilang` バイナリ (REPL + `ilang run path.il`)
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

// クラス — JS 風、`init` がコンストラクタ
class Counter {
    count: i64
    init(start: i64) { this.count = start }
    increment(): i64 {
        this.count = this.count + 1
        this.count
    }
}
let c = new Counter(10)
c.increment()
```

### 型
- プリミティブ: `i64`, `f64`, `bool`, `()` (Unit)
- ユーザ定義: `Type::Object(class_name)` (クラス名と同一)
- `i64 → f64` の暗黙昇格あり (混合算術と引数渡しで)

### 演算子優先順位 (低 → 高)
| 演算子 | l_bp / r_bp | 結合性 |
| --- | --- | --- |
| `=` | 2 / 1 | 右 |
| `\|\|` | 3 / 4 | 左 |
| `&&` | 5 / 6 | 左 |
| `==` `!=` | 7 / 8 | 左 |
| `<` `<=` `>` `>=` | 9 / 10 | 左 |
| `+` `-` | 10 / 11 | 左 |
| `*` `/` `%` | 20 / 21 | 左 |
| 単項 `-` `+` `!` | — / 30 | 前置 |
| 後置 `.` (field/method) | (parse_postfix で個別処理) | 左 |

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

## 既知の制限 / TODO

- **未初期化フィールド読み出しはランタイムエラー**: 型チェッカーは definite initialization を追跡しない
- **同一行に複数のクラスメンバー**: ASI は効かない (`;` か改行が必要)
- **クラス自体への属性**: `#[requires(...)]` は今のところメソッドにしか付与不可
- **`return` 文なし**: 関数/メソッドは末尾式が戻り値
- **文字列 / 配列 / 辞書 / `match` / ADT / `loop`/`break`/`continue`**: 未実装
- **`use` / モジュール / インポート**: 未実装

## 次の候補 (次フェーズの選択肢)

ユーザーと相談して選ぶこと:

1. **capability の enforce** ← サプライチェーン対策の核。優先度高い
   - 呼び出し側にも `#[requires(...)]` を要求するチェック
   - `use http_client with cap(net = env.net)` のような import 構文
   - クラスに capability を持たせる構文設計
2. **`loop` / `break` / `continue`**
3. **文字列型** (リテラル、`+` 連結、補間?)
4. **配列 / 辞書**
5. **`match` + `enum` (ADT)**
6. **継承** (`extends` / `super`) — 必要性をまず議論
7. **`ilang-mir` 中間表現** → Cranelift コード生成 (フェーズ5+ で有用)

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
