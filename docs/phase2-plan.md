# ilang フェーズ2: let / fn / capability アノテーション / 型チェック

## Context

フェーズ1 の四則演算インタプリタの上に、変数・関数・型チェック・capability 構文の土台を載せる。capability の実際の権限チェックはフェーズ3 以降だが、構文は今フェーズで決めて AST に保持する (案C: アノテーション + 内部 capability 値の方針)。

## 実装した機能

### 1. `let` と文 (`;` 区切り)
- `let x = 1 + 2;` で値束縛
- `let x: i64 = 7;` で型注釈付き
- ブロック (`{ ... }`) で Rust 風スコープ
- 値を返す末尾式 (trailing expression、`;` なし)

### 2. 関数定義 / 呼び出し
- `fn add(a: i64, b: i64) -> i64 { a + b }`
- 関数同士の前方参照 OK (型チェック・実行とも)
- 戻り値型省略時は `()` (Unit)
- 再帰呼び出しは深さ 256 で `StackOverflow`

### 3. capability アノテーション (パースのみ)
- `#[requires(net)]` `#[requires(file::read, net)]` の構文を関数宣言に付与
- AST は `FnDecl::attrs: Vec<Attribute>` に保持
- フェーズ2 では**実行時/型検査ともに enforce しない**
- フェーズ3 以降で「呼び出し側にも `#[requires(...)]` が必要」のチェックを追加予定

### 4. 最小の型チェッカー (`ilang-types` crate)
- `Type::I64`, `Type::F64`, `Type::Unit`
- `i64` → `f64` への暗黙昇格 (実行時の挙動と一致)
- `let` の型推論と注釈チェック
- 関数の引数型・戻り値型・arity 検査
- REPL で型環境を永続化

## ディレクトリ構成 (フェーズ2 終了時)

```
crates/
├── ilang-ast/      # Expr, Stmt, FnDecl, Attribute, Type, Program
├── ilang-lexer/    # Ident, Let, Fn, #, [, ], ->, ::, ... 追加
├── ilang-parser/   # Pratt parser + 文/関数/属性/ブロック
├── ilang-eval/     # Interpreter (fns + vars 永続化)
├── ilang-types/    # TypeChecker (新規)
└── ilang-cli/      # 型チェックを実行前に挟む
```

## 構文サンプル

```rust
fn double(x: i64) -> i64 { x * 2 }

#[requires(net)]
fn fetch_count(id: i64) -> i64 {
    id * 100
}

let base: i64 = 5;
let n = fetch_count(base);
double(n) + 1   // tail expression — プログラムの結果
```

## オープン課題 (フェーズ3 で着手)

### capability システム本体
- 呼び出し側にも `#[requires(...)]` を要求するチェック
- main 関数は capability を「束ねた env」を引数に取る (案C 内部表現)
- `use http_client with cap(net = env.net)` のような import 構文
- ライブラリ境界で capability を強制 (サプライチェーン攻撃対策)

### その他
- 制御構造 (`if`, `while`, `loop`, `match`)
- 文字列, ブール, 比較演算子 (`==`, `<`, ...)
- 型注釈の表現を拡張 (ジェネリクス、参照、Result)
- ARC とリソース管理
- `ilang-mir` 中間表現 → Cranelift 経由のネイティブコード化
