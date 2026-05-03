# ilang HANDOFF

新しいセッションへの引き継ぎ用。`/clear` 後にこのファイルを読めば現状の文脈が把握できる構成。

言語仕様の詳細は [`docs/syntax.md`](docs/syntax.md) を参照。このファイルは **「実装の現状」と「次に何をやるか」の引き継ぎ** に絞る。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS / TypeScript / Rust 風** のハイブリッド構文。文末は **JS 風 ASI** (改行が `;` 代わり)
- 例外なし。失敗は `Result<T, E>`、回復不能エラーは panic

実装言語: **Rust 1.95**。実行モデル: ツリーウォーク インタプリタ + **Cranelift JIT** (`ilang run --jit`)。

## 現在地

最新コミット `a6c9df9` (`JIT closures: Stages B + C`)。**395 テスト全通過**、警告ゼロ。
基本〜上級言語機能はおおむね実装完了。次のフェーズは **capability の enforce** か **未実装の言語機能 (タプル / `?` 演算子 / Iterator など)**。

## 実装済み機能 (一覧)

### コア
- 全 10 数値型 (`i8/i16/i32/i64/u8/u16/u32/u64/f32/f64`) + bool + string + Unit
- 整数リテラル: 10進 / 16進 (`0xff`) / **8進 (`0o755`)** / 2進 (`0b1011`) + `_` 桁区切り
- 数値型サフィックス (`1_i32`, `1.5_f32`)
- 暗黙の型変換規則 (同符号整数間 / 整数→浮動 / 浮動↔浮動)、符号またぎと浮動→整数は `as` 必須

### 制御フロー
- `if` / `elif` / `else` (式)
- `while` / `loop` / `for in`
- **range 式** `a..b` (排他) / `a..=b` (包含) — for-in イテレータ位置のみ
- `break` / `continue` / **`break v`** (loop からの値付き脱出)
- `return` (値あり / なし)
- `match` (enum 上のパターンマッチ)
- `if let some(v) = x` (Optional 専用パターン)

### 関数
- `fn` 宣言 (引数型必須、戻り値型 `: T` (TS 風))
- **ジェネリック関数** `fn id<T>(x: T): T { x }` — 推論ベース、interpreter / JIT (mono) 両対応
- **関数オーバーロード** — best-match scoring、ambiguous エラー
- ファーストクラス関数 (`let f = add; f(2, 3)`)
- 匿名関数 `let inc = fn(x: i64): i64 { x + 1 }`
- **クロージャ** (value-capture、interp / JIT 両対応、全 capture 型対応)
- `@extern fn` (組み込みホスト関数: `math.*`, `test.*`)
- **`@extern("libname")`** (動的ライブラリ呼び出し、JIT 専用): bare name 自動補完 / `owned_return` フラグ / string マーシャリング
- `@requires(...)` 等の属性 (パースのみ、enforce は未実装)

### クラス (OOP フル)
- `class C { fields; init(); methods; deinit() }` — `init` 可、`deinit` 可
- 暗黙 `this` (フィールド/メソッドを `this.` なしで参照可)
- `==` / `!=` は参照等値 (`Rc::ptr_eq`)
- **ジェネリッククラス** `class Box<T> { ... }` — JIT mono 化
- **メソッド/init オーバーロード** (best-match scoring)
- **`get` / `set` プロパティ** (`obj.x` がアクセサ呼び出し)
- **`static` メソッド** (`ClassName.method(args)`)
- **`static` フィールド** (`i64`/`f64`/`bool` のみ、定数式初期化、mutable)
- **継承 (`extends`)**: 単一継承、`override` キーワード必須、`super.method(...)` / `super(...)` (init 連鎖)、仮想ディスパッチ (vtable)、サブタイプ (`Child` を `Parent` 型に渡せる) — interp / JIT 両対応

### コレクション
- 配列 `T[]` / `T[N]`: literal / index / push / pop / length / slice / indexOf / includes / map / filter / forEach
- Map `Map<K, V>` (K = string / int / bool): `m[k]` / `m[k] = v` / has / delete / size / keys / values / get
- Optional `T?`: `none` / `some(x)` / `if let` / `is_some` / `is_none` / `unwrap` / `T → T?` 自動 wrap
- Weak `T.weak`: `.get(): T?` / `Foo → Foo.weak` 自動 downgrade / 二重 rc
- enum + 構造体的 payload (`tuple` / `struct` / `unit` バリアント)
- **Result<T, E>**: 組み込みジェネリック enum、`Result.ok(v)` / `Result.err(e)` で構築

### 文字列
- リテラル + エスケープ (`\n` `\t` `\r` `\\` `\"` `\0`)
- `+` (連結)、`==` `!=` (構造的等値)
- メソッド: `length` (Unicode コードポイント) / `charAt` / `includes` / `startsWith` / `endsWith` / **`toUpper`** / **`toLower`** / `trim` / `split` / `replace` / `slice`
- 文字列補間は **未実装**

### モジュール / 定数
- `use module` (whole) / `use module { foo, bar }` (selective) — 隣接 `<module>.il` を読み込み
- `const NAME: T = const_expr` — 算術 / ビット / 比較 / 論理 / `as` キャスト / 他の const 参照を **コンパイル時に折りたたみ**
- 同梱モジュール: `math` (sqrt/sin/cos/pi/e ほか) / `test` (expect/expectStr/...)

### メモリ管理 (ARC)
- 全ヒープ値は ref-counted: Object / String / Array / Optional / Weak / Map / **closure** / EnumHeap
- `deinit` がスコープ脱出時 / rc=0 時に発火
- 二重 rc (strong/weak) で循環参照を `T.weak` で解消可能
- フィールド / 配列要素 / capture の再帰 release

## 実行モデル

| モード | コマンド | 用途 |
| --- | --- | --- |
| **interpreter** | `ilang run path.il` | 全機能サポート、起動が速い |
| **JIT** | `ilang run --jit path.il` | Cranelift ネイティブコード、interp の数十〜数百倍 |
| **REPL** | `ilang` (引数なし) | 1 行ずつ評価、interpreter のみ |

JIT のみ未対応:
- ネイティブ extern (`@extern("libname")`) は **JIT 専用** (interp は逆に未対応)
- 静的フィールドは `i64` / `f64` / `bool` のみ (string / object 等は Phase 2)
- ジェネリッククラスでの **継承** / **静的メンバー** / **プロパティ** は型パラメータ解決の制約により未対応

## ワークスペース構成

```
crates/
├── ilang-ast/       # AST 定義 (Span 含む)
├── ilang-lexer/     # 字句解析 (Token, leading_newline, numeric_suffix)
├── ilang-parser/    # Pratt 構文解析 + loader (use 解決) + normalize + const 折りたたみ
├── ilang-types/     # 型チェッカー (overload resolution / mangle / inheritance / closures)
├── ilang-eval/      # ツリーウォーク インタプリタ (REPL 状態は Interpreter が保持)
├── ilang-codegen/   # Cranelift JIT — 役割別ファイル群:
│   ├── compiler.rs        # JitCompiler 本体 + jit_run_with エントリ
│   ├── runtime.rs         # extern "C" ヘルパ (alloc, retain/release, str/array fns ほか)
│   ├── ty.rs              # JitTy (JIT 内部型)、ClassLayout
│   ├── env.rs             # Env / LowerCtx
│   ├── arc.rs             # retain/release ディスパッチ
│   ├── lower_op.rs        # 二項/単項/coerce
│   ├── lower_ctrl.rs      # while / loop / for-in
│   ├── lower_stmt.rs      # let / block ARC release
│   ├── lower_expr.rs      # メイン (Var / Call / Field / Method / Closure / etc)
│   ├── monomorphize.rs    # generic mono + hoist anon fns + 各種 walker
│   ├── drops.rs           # per-class / per-array / per-enum / per-closure drop fn 生成
│   ├── native_extern.rs   # `@extern("lib")` の dlopen + シンボル登録
│   ├── math_externs.rs    # 組み込み math.* シンボル登録
│   ├── test_externs.rs    # 組み込み test.* シンボル登録
│   └── value.rs           # JitValue (host から見た戻り値表現)
└── ilang-cli/       # `ilang` バイナリ (REPL + run)
docs/syntax.md       # ユーザー向け構文一覧 (常に最新に保つ)
crates/ilang-cli/tests/programs/  # 106 個の .il fixture (interp + JIT 両方で実行)
```

各 crate は `lib.rs` がほぼ re-export だけ。実体は役割別ファイル。**新コードを書くときも役割別モジュールを維持** すること。テストは `crates/<crate>/tests/<name>.rs` の統合テスト + `crates/ilang-cli/tests/programs/*.il` の言語レベル fixture。

### .il fixture の書き方
- `crates/ilang-cli/tests/programs/<カテゴリ>/<名前>.il` に `.il` ファイルを 1 つ置けば自動で interp + JIT 両方で実行される
- マジックコメント:
  - `// expect: <line>` — stdout の行を順序通りマッチ
  - `// expect-error: <substr>` — 失敗を期待、stderr に substr が含まれること
  - `// jit: skip` — JIT 実行をスキップ
  - `// interp: skip` — interp 実行をスキップ
- interp + JIT が両方走った場合、stdout 一致も検証 (divergence 防止)
- アサーションは `use test; test.expect(actual, expected)` などで書く

## JIT メモリレイアウト (重要)

| 値 | レイアウト | ヘッダサイズ |
| --- | --- | --- |
| Object | `[strong_rc | weak_rc | drop_fn | vtable_ptr | fields...]` | 32 byte |
| String | `Box<StringRc { rc, s }>` (リテラルは saturated rc) | — |
| Array | `[rc | drop_fn | len | cap | data_ptr]` ヘッダ + 別領域データバッファ | 40 byte |
| Optional<heap> | T と同じ i64 ポインタ (0 = none) | — |
| Optional<primitive> | ヒープ box (rc + value) | 16 byte |
| Weak<T> | T と同じ i64 ポインタ。retain/release は weak 側 helper | — |
| Map | Box<HashMap<MapKey, i64>> | — |
| EnumHeap | `[rc | weak_rc | drop_fn | vtable | tag(i32) | padding | payload...]` | object と同じ |
| **Closure** | `[rc | drop_fn | total_size]` ヘッダ + `[fn_ptr | env_field0 | ...]` | 24 byte |
| Function value | closure ptr (top-level fn は trampoline closure に自動 wrap) | — |

### vtable (継承)
- per-class `Box<[i64]>` (typechecker が assign した slot に関数ポインタ)
- 子クラスの vtable は親の prefix を含む (slot 共有)
- `obj.method()` は `vtable_ptr → vtable[slot] → call_indirect`
- `super.method()` は親の特定関数への直接呼び出し (no indirection)

## 重要な設計決定 (引き継ぎ要)

| 領域 | 決定 | 理由 |
| --- | --- | --- |
| 所有権 / `mut` | 採用しない | ARC 前提、全変数再代入可 |
| 戻り値型構文 | `): T { ... }` | TS 風、`->` トークンなし |
| 例外 | 採用しない | Result + panic、Rust/Go/Zig 流 |
| ファイル拡張子 | `.il` | 確定 |
| capability | アノテーション + capability 値 (案C) | enforce はまだ |
| ネイティブコード | Cranelift 第一候補 | 軽量、ビルドコスト優先 |
| クラス継承 | 単一継承 + 仮想ディスパッチ | Phase B 採用済み |
| コンストラクタ名 | `init` (Swift 風) | 特殊メソッド名、キーワードではない |
| オブジェクト等価性 | 参照等価 (`Rc::ptr_eq`) | structural equality は将来トレイト経由 |
| クロージャキャプチャ | by value (Rust の `move` 相当) | by ref はまだ未対応 |

## ARC まわりの核心ルール (JIT)

- **caller 側で aliased な heap 引数を retain**、callee 側で param を関数出口で release (deinit の `this` は除外 — `release_object` が lifecycle を持つので二重 release で無限再帰)
- ブロック終了時に新規 heap binding を **LIFO 順** で release
- `let y = x` / 関数引数 / `obj.field = x` / `a[i] = x` で borrowed 元 (Var/Field/Index/This) なら retain (fresh 値はそのまま rc=1 を譲渡)
- `emit_bind_retain` ヘルパが bind 時 rc 調整を集約 (heap 一般則 + fresh strong → weak 変換時の特殊ケース含む)
- 代入上書きは **新値 retain → store → 旧値 release** の順
- ブロック / `__main` の **tail retain は aliased heap 値のときだけ**
- 文字列 fresh オペランドは concat / eq 後に release
- `release_object` は drop_fn 中 weak_rc を sentinel +1 (back-edge weak の早期解放回避)
- **クラス宣言は 2 段階**: `declare_class_name` で名前→id だけ登録 → `finalize_class_layout` でフィールド型解決

### Closure ARC
- closure 専用のヘッダ (`[rc | drop_fn | total_size]`) + 専用 retain/release helper
- 各 closure wrapper に対し heap captures を release する drop fn を自動生成 (`__drop_closure_<name>`)
- `JitTy::Fn` は `is_heap()` に含まれる → 既存の let-bind retain / scope release 経路で自動管理
- 入れ子 closure: 内側 closure の construct site が外側の `closure_capture_env` を見て env から load して再 capture

## 開発フロー

```sh
# 全テスト
~/.cargo/bin/cargo test --workspace

# REPL (let / fn / class が永続化)
~/.cargo/bin/cargo run -p ilang-cli

# ファイル実行
~/.cargo/bin/cargo run -p ilang-cli -- run path.il
~/.cargo/bin/cargo run -p ilang-cli -- run --jit path.il

# 1 つの fixture を直接実行
./target/debug/ilang run --jit crates/ilang-cli/tests/programs/01_basics/closures_jit.il
```

`source "$HOME/.cargo/env"` を使うと権限プロンプトが出る (settings.local.json の Bash allow が `Bash` 単独だと効かない)。**`~/.cargo/bin/cargo` を直接呼ぶこと**。

### コミット方針
- 機能単位で 1 コミット
- メッセージは英語、末尾に `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
- ユーザーが「コミットして」と言うまでコミットしない (頻出パターン)

### コードスタイル
- コメントは「なぜ」だけ。「なに」は書かない
- 公開 API のみ `pub use` で再エクスポート
- 警告ゼロを維持
- 役割別モジュール分割を維持 (`lib.rs` を肥大化させない)
- 大きな機能を追加するときは fixture を `tests/programs/` に必ず追加 (interp + JIT 両方で動くか自動検証されるため)

## 次の候補

ユーザーと相談して選ぶこと。

### A. 言語機能 (重要度高)
- **タプル** `(i64, string)` — 匿名 product 型。複数戻り値 / 一時的なペアに毎回クラスを定義するのが冗長
- **`?` 演算子** — `let v = parse(s)?` で `Result.err` を即 return。Result 連鎖が match ネストにならない
- **文字列補間** — `"hello, ${name}"`。syntax.md に「未実装」と既に明記済み
- **Iterator プロトコル** — ユーザ型に `next()` を実装させて for-in に乗せる仕組み。ジェネレータの基礎
- **デフォルト引数 / 名前付き引数** — `fn open(path: string, mode: string = "r")`

### B. 言語機能 (重要度中)
- 演算子オーバーロード (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- Trait / Interface — type-by-shape 抽象化、継承と直交
- デストラクチャリング (`let (a, b) = pair`)
- async/await
- ジェネリック制約 (bounds)

### C. capability の enforce ← **言語ビジョンの核**
- 呼び出し側にも `@requires(...)` を要求するチェック
- `use http_client with cap(net = env.net)` のような import 構文
- クラスに capability を持たせる構文設計
- MIR を挟むタイミングで一緒に入れるのが自然 (関数呼び出しグラフが扱いやすい)

### D. JIT 補完
- 静的フィールドの string / object 対応
- ジェネリッククラスでの継承 / 静的メンバー / プロパティ
- ネイティブ extern の interpreter 対応 (現状 JIT 専用)

### E. `ilang-mir` 中間表現 (未着手)
現状は AST → Cranelift IR を直接 lowering。MIR を挟むと:
- 複数バックエンド (LLVM / wasm / バイトコード VM) を後付けしやすい
- `lower.rs` の「signed?/float?/widen?」分岐を 1 度で解決
- constant folding / dead code elim 等の最適化を載せる足場
- capability enforce の検査箇所として最適
- tree-walk より速い軽量バイトコード VM が書ける

## 会話のトーンと言語

- ユーザーへの返答は **日本語** (このファイルも日本語)
- コード/識別子/コミットメッセージは英語
- 提案する場合は 2-3 案のトレードオフを示し、推奨を 1 つ明示
- ユーザーが選んだ後は実装まで進める (確認は重要なところだけ)
- 大きな機能追加では「Phase A→B→C と段階的に」を提案するパターンが多い (継承、static、closure はこの形で着地済み)

## 既知の細かい落とし穴

- **JIT の内部 typecheck**: `jit_run_inner` の中で TypeChecker をもう一度動かす (`define_main` 内)。第 2 パス用の side table (`closure_wrapper_captures`, `loop_break_types`, `class_method_slots`, `class_vtable_lens`) を毎回最新に保つこと
- **Hoist pass は JIT 専用**: `crates/ilang-codegen/src/monomorphize.rs::hoist_anon_fns` は FnExpr → Closure 変換を行うが、interpreter は通らない。`ExprKind::Closure` は interpreter / typechecker からは `unreachable!` で除外
- **`ExprKind` を追加したら walker を全部更新**: monomorphize.rs に 6+ の walker (hoist_in_expr / scan_expr / subst_expr / rewrite_expr / walk_expr_children / map_expr_children + rewrite_calls_in_expr / rewrite_enum_refs_in_expr) があり、interp / checker / mangle / loader / normalize にも match 漏れチェックが効く
- **AST の `is_override`**: `FnDecl.is_override` は override メソッドのときだけ `true`。クローンする箇所が monomorphize に多数あるので忘れずに `f.is_override` を伝播 (Python script で大量パッチした経緯あり)
- **`extends` 周りの class_signature**: parent をオーバーレイしてから子の declarations をマージ。`init` / `deinit` は per-class なので override 必須チェックの対象外 (特殊条件あり)
- **closures.il vs closures_jit.il vs closures_heap.il**: 旧名残のため 3 ファイルあり。整理は気にせず、同じシナリオを test するくらいの感覚
- **`docs/syntax.md` を最新に保つ**: 機能追加するたびに必ず更新する (ユーザーが頻繁に参照)
