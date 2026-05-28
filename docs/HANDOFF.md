# ilang HANDOFF

新しいセッションへの引き継ぎ用。`/clear` 後にこのファイルを読めば現状の文脈が把握できる構成。

言語仕様の詳細は [`docs/syntax.md`](syntax.md) を参照。このファイルは **「実装の現状」と「次に何をやるか」の引き継ぎ** に絞る。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS / TypeScript / Rust 風** のハイブリッド構文。文末は **JS 風 ASI** (改行が `;` 代わり)
- 例外なし。失敗は `Result<T, E>`、回復不能エラーは panic

実装言語: **Rust 1.95**。実行モデル: AST → MIR (SSA) → **Cranelift JIT** が唯一の実行経路。ツリーウォーク インタプリタ (`ilang-eval`) と旧 ilang-codegen 経路は M1 Step 6 で撤去済み。AOT 経由のネイティブ実行は `ilang build` で行う。

## 現在地

最新コミット `37adff8` (`README: turn the status section into a feature table`)。**workspace の全テスト通過**、警告ゼロ。`crates/ilang-cli/tests/programs/` 配下に **150 個の .il fixture** (MIR JIT 経由、AOT ビルドとの parity も検証)。

直近の大きな仕事は **FFI の全面リファクタ** で、`@extern("libname") fn ...` 等の per-fn 構文を捨てて Rust 風の `@extern(C) { ... }` ブロック構文に統合した。仕上げとして実用的な SDL2 バインディングを `bindings/sdl2/` に整備し、`examples/sdl_breakout/` で動くゲーム画面サンプル(キーボード入力 + 効果音)を出している。`ilang.toml` プロジェクトファイルも導入し、外部バインディングを再利用可能な形にした。

次のフェーズ候補は **capability の enforce**、**未実装の言語機能 (タプル / `?` 演算子 / Iterator など)**、または **C ヘッダから .il 自動生成のミニ bindgen**。

## 実装済み機能 (一覧)

### コア
- 全 10 数値型 (`i8/i16/i32/i64/u8/u16/u32/u64/f32/f64`) + bool + string + Unit
- 整数リテラル: 10進 / 16進 (`0xff`) / 8進 (`0o755`) / 2進 (`0b1011`) + `_` 桁区切り
- 数値型サフィックス (`1_i32`, `1.5_f32`)
- 暗黙の型変換規則 (同符号整数間 / 整数→浮動 / 浮動↔浮動)、符号またぎと浮動→整数は `as` 必須
- **二項演算でのリテラル側型適応**: `u32_var != 0` のように相手の整数型にリテラルが収まれば自動でその型として扱う

### 制御フロー
- `if` / `elif` / `else` (式)
- `while` / `loop` / `for in`
- range 式 `a..b` (排他) / `a..=b` (包含) — for-in イテレータ位置のみ
- `break` / `continue` / `break v` (loop からの値付き脱出)
- `return` (値あり / なし) — **トップレベルでも使える** (早期 program exit、値は持てない)
- `match` (enum 上のパターンマッチ)
- `if let some(v) = x` (Optional 専用パターン)

### 関数
- `fn` 宣言 (引数型必須、戻り値型 `: T` (TS 風))
- ジェネリック関数 `fn id<T>(x: T): T { x }` — 推論ベースで JIT mono 化。`*const T` のような raw pointer 内の TypeVar も推論される
- 関数オーバーロード — best-match scoring、ambiguous エラー
- ファーストクラス関数 (`let f = add; f(2, 3)`)
- 匿名関数 `let inc = fn(x: i64): i64 { x + 1 }`
- クロージャ (value-capture、全 capture 型対応)
- `@requires(...)` 等の属性 (パースのみ、enforce は未実装)
- `@override` (継承メソッドの override マーカー、必須)

### FFI (`@extern(C) { ... }`)
ブロック構文に統一済み。`@lib(...)` の dlopen は JIT 起動時に解決される。

ブロック内で書ける item:
- `fn name(...): T` — 関数宣言
  - `@lib("libfoo", "libfoo-fallback.so")` — dlopen するライブラリ名(複数指定 = フォールバック)。省略すると host 登録形(stdlib の math/os/test がこの形)
  - `@optional` — ライブラリやシンボルが見つからなくても JIT 構築は失敗せず、呼ぶとアボートするスタブにバインド。`os.libLoaded(name)` で事前ガードする
  - `@symbol("c_name")` — C 側のシンボル名と ilang 側の fn 名を分離
  - 末尾の `...` — printf 系 variadic
- `fn name(...): T { body }` — ilang 本体を C ABI で公開する関数(callback / 内部 wrapper)
- `struct Name { ... }` — C 互換構造体。空 struct = opaque handle として使う
- `union Name { ... }` — C union (全フィールド offset 0)
- `static name: T` — C グローバル変数
- `class Name { ... }` — ARC-managed wrapper クラス。method 本体は in_extern_c 文脈で動くので、生 extern fn と FFI ヘルパーを直接呼べる(deinit で C ハンドル自動 close できる)
- `@packed` (struct のみ) / `@bits(N)` (フィールド) — C のレイアウト調整に対応

ブロック**内のみ**で使える型:
- `*T` / `*const T` (raw pointer)
- `char` / `void` / `size_t` / `ssize_t`

これらの型はブロック外の式・型注釈に書けない。値も漏らせない (let バインディングで受けたり、call 結果に C-only 型が含まれると型エラー)。

ブロック内のみで呼べる **マーシャリングヘルパー**(自動的にビルトイン登録):
- `cstrFromString(s: string): *const char`
- `stringFromCstr(p: *const char): string`
- `freeCstr(p: *const char)`
- `bytesFromBuffer(p: *const void, n: size_t): u8[]`
- `arrayFromCArray<T>(p: *const T, n: size_t): T[]` (T は数値プリミティブ / bool)
- `cstrArrayToStrings(p: *const *const char): string[]`
- `errnoCheck(rc: i32): i32?` / `errnoCheckI64(rc: i64): i64?`

その他のキャスト規則 (ブロック内):
- `*T ↔ *U` — type-pun (`*const u8 → *const void` 等)
- `*T ↔ i64` — pointer ↔ アドレス値
- `T[] → *T` (Array→RawPtr 暗黙変換、data ポインタを渡す)
- struct 値渡し(< 16 B = chunks / HFA / > 16 B = sret)を自動で適用(旧 `byValue` フラグ相当)

### モジュール / プロジェクトファイル
- `use module` (whole) / `use module { foo, bar }` (selective: bare 名 + 名前空間の両方が使える)
- **`use module as foo`** — 別名で名前空間を import (`foo.X` で参照、内部的には `module.X` に書き戻される)
- **`use module as _ { ... }`** — 名前空間を抑止し、selective 名のみ公開
- **`pub use module`** — re-export(umbrella module を作る用)。`as` の併用は不可
- **可視性**: top-level item とクラスメンバはデフォルトで module-private。`pub fn` / `pub class` / `pub enum` / `pub const` / `pub let` (top-level) / `pub` 付きの `@extern(C){}` 内アイテム、`pub init` / `pub <method>` / `pub <field>` / `pub static` / `pub get/set` で外部公開。loader は post-load の `validate_visibility` で selective import と `module.X` 参照を pub catalog に照合し、`pub use M` チェインを辿って可視性を伝播する。`pub use M` は M の **pub アイテムだけ** を再エクスポート
- **`ilang.toml`** プロジェクトファイル: `[deps] sdl2 = "path"` で `use` の探索パスを追加。CLI が entry file から上に辿って自動発見
- `const NAME: T = const_expr` — 算術 / ビット / 比較 / 論理 / `as` キャスト / 他の const 参照を**コンパイル時に折りたたみ**。型注釈付き const は substitute 時に Cast で wrap されて、参照箇所すべてに自動的に型が伝わる
- 同梱モジュール: `math` (sqrt/sin/cos/pi/e ほか) / `test` (expect/...)、`os` (errno / libLoaded / 定数群)

### クラス (OOP フル)
- `class C { fields; init(); methods; deinit() }` — `init` 可、`deinit` 可
- 暗黙 `this` (フィールド/メソッドを `this.` なしで参照可)
- `==` / `!=` は参照等値 (`Rc::ptr_eq`)
- ジェネリッククラス `class Box<T> { ... }` — JIT mono 化
- メソッド/init オーバーロード (best-match scoring)
- `get` / `set` プロパティ (`obj.x` がアクセサ呼び出し)
- `static` メソッド (`ClassName.method(args)`)
- `static` フィールド (`i64`/`f64`/`bool` のみ、定数式初期化、mutable)
- 継承 (`extends`): 単一継承、`@override` キーワード必須、`super.method(...)` / `super(...)` (init 連鎖)、仮想ディスパッチ (vtable)、サブタイプ

### コレクション
- 配列 `T[]` / `T[N]`: literal / index / push / pop / length / slice / indexOf / includes / map / filter / forEach
- Map `Map<K, V>` (K = string / int / bool): `m[k]` / `m[k] = v` / has / delete / size / keys / values / get
- Optional `T?`: `none` / `some(x)` / `if let` / `is_some` / `is_none` / `unwrap` / `T → T?` 自動 wrap
- Weak `T.weak`: `.get(): T?` / `Foo → Foo.weak` 自動 downgrade / 二重 rc
- enum + 構造体的 payload (`tuple` / `struct` / `unit` バリアント)
- Result<T, E>: 組み込みジェネリック enum、`Result.ok(v)` / `Result.err(e)` で構築

### 文字列
- リテラル + エスケープ (`\n` `\t` `\r` `\\` `\"` `\0`)
- `+` (連結)、`==` `!=` (構造的等値)
- メソッド: `length` (Unicode コードポイント) / `charAt` / `includes` / `startsWith` / `endsWith` / `toUpper` / `toLower` / `trim` / `split` / `replace` / `slice`
- 文字列補間は **未実装**

### メモリ管理 (ARC)
- 全ヒープ値は ref-counted: Object / String / Array / Optional / Weak / Map / closure / EnumHeap
- `deinit` がスコープ脱出時 / rc=0 時に発火
- 二重 rc (strong/weak) で循環参照を `T.weak` で解消可能
- フィールド / 配列要素 / capture の再帰 release

## 実行モデル

| モード | コマンド | 用途 |
| --- | --- | --- |
| **MIR JIT** | `ilang run path.il` | Cranelift ネイティブコード、唯一の実行経路 |
| **AOT** | `ilang build path.il -o out` | 同じ MIR→Cranelift 経路を ELF/Mach-O に焼き出す |
| **REPL** | `ilang` (引数なし) | 1 行ずつ評価 (MIR JIT を REPL スロット付きで実行) |

`ilang.toml` が entry の上の階層にあれば自動発見、`[deps]` のパスが `use` の探索先に追加される。`ilang run --mir-jit` は旧 CLI の互換フラグで現在はデフォルトと同じ。

現状の制約:
- 静的フィールドは `i64` / `f64` / `bool` のみ (string / object 等は未対応)
- ジェネリッククラスでの **継承** / **静的メンバー** / **プロパティ** は型パラメータ解決の制約により未対応

## ワークスペース構成

```
crates/
├── ilang-ast/       # AST 定義 (Span 含む)
├── ilang-lexer/     # 字句解析 (Token, leading_newline, numeric_suffix)
├── ilang-parser/    # Pratt 構文解析 + loader (use 解決 / pub use / ilang.toml dep paths) + normalize + const 折りたたみ
├── ilang-types/     # 型チェッカー (overload resolution / mangle / inheritance / closures / @extern(C) コンテキスト)
├── ilang-mir/       # AST→MIR (SSA + block-args)、モノモーフィゼーション、validator/printer
├── ilang-mir-codegen/ # MIR→Cranelift JIT 本体
│   ├── compile/           # ARC + FFI + REPL slot を含む lowering 一式
│   ├── aot/               # ELF/Mach-O を吐く `ilang build` 経路
│   └── ty.rs              # 内部 JIT 型 / クラスレイアウト
├── ilang-runtime/   # ランタイム (alloc, retain/release, str/array fns、math/os/test extern, native_extern)
├── ilang-lsp/       # LSP サーバー
└── ilang-cli/       # `ilang` バイナリ (REPL + run + build + ilang.toml 解決)

bindings/
├── cocoa/           # macOS Cocoa バインディング (foundation / appkit)
├── directx12/       # Windows DirectX 12 バインディング (テストフィクスチャ付き)
├── gtk4/            # Linux GTK 4 バインディング (テストフィクスチャ付き)
├── libc/            # POSIX libc バインディング
├── sdl2/            # 再利用可能な SDL2 バインディング (umbrella + 機能別 6 ファイル + README)
├── sqlite3/         # SQLite3 バインディング
└── windows/         # Windows Win32 バインディング

examples/
├── sdl_breakout/   # SDL2 を使ったゲーム画面サンプル (main.il + ilang.toml)
└── libs/gui/       # libs/gui のサンプル群 (controls / menus / window 等)

libs/
└── gui/             # クロスプラットフォーム GUI ライブラリ (cocoa/win32/linux backend)

docs/syntax.md       # ユーザー向け構文一覧 (常に最新に保つ)
crates/ilang-cli/tests/programs/  # 150 個の .il fixture (MIR JIT + AOT で実行、stdout parity 検証)
```

各 crate は `lib.rs` がほぼ re-export だけ。実体は役割別ファイル。**新コードを書くときも役割別モジュールを維持** すること。テストは `crates/<crate>/tests/<name>.rs` の統合テスト + `crates/ilang-cli/tests/programs/*.il` の言語レベル fixture。

### .il fixture の書き方
- `crates/ilang-cli/tests/programs/<カテゴリ>/<名前>.il` に `.il` ファイルを 1 つ置けば自動で MIR JIT + AOT ビルドで実行される
- マジックコメント:
  - `// expect: <line>` — stdout の行を順序通りマッチ
  - `// expect-error: <substr>` — 失敗を期待、stderr に substr が含まれること
  - `// jit: skip` — MIR JIT 実行をスキップ
  - `// aot: skip` — AOT ビルド経路をスキップ
- MIR JIT と AOT 両方が走った場合は stdout 一致も検証 (divergence 防止)
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
| Closure | `[rc | drop_fn | total_size]` ヘッダ + `[fn_ptr | env_field0 | ...]` | 24 byte |
| Function value | closure ptr (top-level fn は trampoline closure に自動 wrap) | — |
| `@extern(C)` struct | ARC ヘッダ付きヒープ Object と同じ — C には負オフセットの ARC ヘッダは見えず、ユーザポインタ = フィールド領域先頭 | 32 byte |

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
| クラス継承 | 単一継承 + 仮想ディスパッチ | 採用済み |
| コンストラクタ名 | `init` (Swift 風) | 特殊メソッド名、キーワードではない |
| オブジェクト等価性 | 参照等価 (`Rc::ptr_eq`) | structural equality は将来トレイト経由 |
| クロージャキャプチャ | by value (Rust の `move` 相当) | by ref はまだ未対応 |
| FFI 構文 | `@extern(C) { ... }` ブロック | per-fn フラグまみれの旧構文を捨てて Rust の extern "C" {} を踏襲 |
| FFI 型カプセル化 | raw pointer / C-only 型はブロック内のみ書ける + 値を外に漏らせない | 「ブロックの内側だけが unsafe」という Rust と同じ思想 |
| プロジェクトファイル | `ilang.toml` (Cargo 風) | binding 配布のため最小限の `[deps]` だけ |

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
# 全テスト (cargo-nextest 経由、~30 秒)。`.cargo/config.toml` の
# alias で `cargo t` = `nextest run --workspace`、`cargo tci` =
# `--profile ci` (リトライ + fail-fast オフ)。設定本体は
# `.config/nextest.toml`。doctest は別途 `cargo test --doc` 必要
# (~20 秒)
~/.cargo/bin/cargo t

# cargo-nextest が無いホスト用フォールバック
~/.cargo/bin/cargo test --workspace

# REPL (let / fn / class が永続化)
~/.cargo/bin/cargo run -p ilang

# ファイル実行 (MIR JIT)
~/.cargo/bin/cargo run -p ilang -- run path.il

# AOT ビルド (Mach-O / ELF を吐く)
~/.cargo/bin/cargo run -p ilang -- build path.il -o ./out

# 1 つの fixture を直接実行
./target/debug/ilang run crates/ilang-cli/tests/programs/04_modules/extern_cstr_array.il

# SDL サンプル (要 SDL2 インストール: brew install sdl2 / apt install libsdl2-dev)
./target/debug/ilang run examples/sdl_breakout/main.il

# cocoa バインディングテスト (macOS のみ。非 macOS では skip)
#   - foundation: NSString / NSArray / NSDate / NSURL / NSData / 他
#                 38 fixtures, 645/1989 selectors (32%), 136/179 classes (76%)
#   - appkit    : NSWindow / NSButton / NSColor / NSBezierPath / 他
#                 11 fixtures, 169/508 selectors (33%), 44/53 classes (83%)
# `-- --nocapture` でカバレッジレポートを stdout に流す
~/.cargo/bin/cargo test --release -p ilang --test cocoa_foundation -- --nocapture
~/.cargo/bin/cargo test --release -p ilang --test cocoa_appkit -- --nocapture

# 個別 fixture を直接実行
./target/release/ilang run bindings/cocoa/foundation/test/strings_test.il
./target/release/ilang run bindings/cocoa/appkit/test/drawing_test.il
```

`source "$HOME/.cargo/env"` を使うと権限プロンプトが出る (settings.local.json の Bash allow が `Bash` 単独だと効かない)。**`~/.cargo/bin/cargo` を直接呼ぶこと**。

### scanner / parser ベンチ

`crates/ilang-parser/benches/scan_parse.rs` に criterion ベンチがある。stdlib / `tests/programs` 全体 / 全プログラム連結 の 3 コーパスを lex 単独・lex+parse の 2 段で計測する。

```sh
# ベースライン保存 (最適化前に1回)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --save-baseline before

# 変更後の比較 (criterion が before との差分を出す)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --baseline before

# 単一グループだけ走らせる例
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse programs -- --baseline before
```

サンプル数や測定時間は `--sample-size 50 --warm-up-time 2 --measurement-time 5` 等で増やせる。デフォルトは短時間 (1秒ウォームアップ・3秒測定) なので、有意差判定が「noise threshold」になりがちな場合は増やす。

### type-check / MIR-lower ベンチ

`crates/ilang-mir/benches/check_lower.rs` に criterion ベンチがある。`tests/programs` の中で load+check+lower が成功する全プログラムを 1 ラウンドとして:

- `check_lower/check` — `ilang_types::check` のみ
- `check_lower/lower` — `ilang_types::check` + `ilang_mir::lower_program` (実パイプラインと同じ順序)

```sh
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --save-baseline before
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --baseline before
```

このベンチは 343 個の小プログラムを直列実行する都合上、scan_parse より run-to-run の variance が大きい。意味のある差分判定をしたいときは `--sample-size 50 --warm-up-time 3 --measurement-time 6` 程度を指定する。

### コミット方針
- 機能単位で 1 コミット
- メッセージは英語、末尾に `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
- ユーザーが「コミットして」と言うまでコミットしない (頻出パターン)

### コードスタイル
- コメントは「なぜ」だけ。「なに」は書かない
- 公開 API のみ `pub use` で再エクスポート
- 警告ゼロを維持
- 役割別モジュール分割を維持 (`lib.rs` を肥大化させない)
- 大きな機能を追加するときは fixture を `tests/programs/` に必ず追加 (MIR JIT + AOT 両方で動くか自動検証されるため)

## 次の候補

ユーザーと相談して選ぶこと。

### A. 言語機能 (重要度高)
- **タプル** `(i64, string)` — 匿名 product 型。複数戻り値 / 一時的なペアに毎回クラスを定義するのが冗長
- **`?` 演算子** — `let v = parse(s)?` で `Result.err` を即 return。Result 連鎖が match ネストにならない
- **文字列補間** — `"hello, ${name}"`
- **Iterator プロトコル** — ユーザ型に `next()` を実装させて for-in に乗せる仕組み。ジェネレータの基礎
- **デフォルト引数 / 名前付き引数** (デフォルト引数は実装済み、名前付き呼び出しは未)

### B. FFI / バインディング配布
- **C ヘッダから .il を自動生成するミニ bindgen** — 現状は手書き。3 段階(手動 YAML / clang JSON AST → スクリプト変換 / libclang フル統合)を [docs/syntax.md](syntax.md) ではなく相談済みのチャットで議論済み。当面は `bindings/sdl2/` を手書きでメンテ
- **`bindings/libc/`** など他のライブラリも同形式で整備
- **`bindings/sdl2/`** の拡張: SDL_Image (PNG 読み込み) / SDL_ttf (フォント) / イベントの SDL_PollEvent 構造体対応(現状は SDL_GetKeyboardState ベースのポーリングだけ)

### C. 言語機能 (重要度中)
- 演算子オーバーロード (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- Trait / Interface — type-by-shape 抽象化、継承と直交
- デストラクチャリング (`let (a, b) = pair`)
- async/await
- ジェネリック制約 (bounds)

### D. capability の enforce ← **言語ビジョンの核**
- 呼び出し側にも `@requires(...)` を要求するチェック
- `use http_client with cap(net = env.net)` のような import 構文
- クラスに capability を持たせる構文設計
- MIR を挟むタイミングで一緒に入れるのが自然 (関数呼び出しグラフが扱いやすい)

### E. JIT 補完
- 静的フィールドの string / object 対応
- ジェネリッククラスでの継承 / 静的メンバー / プロパティ
- **`&CONST` (トップレベル const のアドレス取得) の最適化**:
  現状の lowering (`crates/ilang-mir/src/lower/ops.rs`
  `lower_addr_of_decomposed`) は loader が demote した repl_slot
  経由で値をロードし、CRepr Object の場合のみポインタ値を再タグする
  形で `&IID_X` を通している。毎回 `__repl_load_slot` を呼ぶので、
  ホットパスでは無駄。.rodata 相当の静的データシンボルに焼いて
  `symbol_value` で参照する形にすれば呼び出しオーバーヘッドが消える。
  CRepr 以外 (i64/f64/string などのプリミティブ const) は現状未対応
  で `&` がエラーになる — 必要になったらスタックコピー経路も追加

### F. MIR まわりの拡張 (中間表現は導入済み)
AST → MIR (SSA + block-args) → Cranelift IR の経路は完成済み。次に乗せやすいもの:
- LLVM / wasm 等の別バックエンド (Cranelift 経路と並走できる)
- MIR レベルでの constant folding / dead code elim
- capability enforce の検査箇所として活用
- 軽量バイトコード VM (起動高速化やデバッガ統合の足場として)

## 会話のトーンと言語

- ユーザーへの返答は **日本語** (このファイルも日本語)
- コード/識別子/コミットメッセージは英語
- 提案する場合は 2-3 案のトレードオフを示し、推奨を 1 つ明示
- ユーザーが選んだ後は実装まで進める (確認は重要なところだけ)
- 大きな機能追加では「Phase A→B→C と段階的に」を提案するパターンが多い (継承、static、closure、FFI リファクタはこの形で着地済み)

## 既知の細かい落とし穴

- **JIT の内部 typecheck**: `jit_run_inner` の中で TypeChecker をもう一度動かす (`define_main` 内)。第 2 パス用の side table (`closure_wrapper_captures`, `loop_break_types`, `class_method_slots`, `class_vtable_lens`) を毎回最新に保つこと
- **Hoist pass は MIR mono の一部**: `crates/ilang-mir/src/monomorphize/hoist.rs::hoist_anon_fns` が FnExpr → Closure 変換を行う。`ExprKind::Closure` は typechecker からは `unreachable!` で除外され、hoist 後の MIR でのみ現れる
- **`ExprKind` を追加したら walker を全部更新**: monomorphize.rs に 6+ の walker (hoist_in_expr / scan_expr / subst_expr / rewrite_expr / walk_expr_children / map_expr_children + rewrite_calls_in_expr / rewrite_enum_refs_in_expr) があり、checker / mangle / loader / normalize にも match 漏れチェックが効く
- **AST の `is_override`**: `FnDecl.is_override` は override メソッドのときだけ `true`。クローンする箇所が monomorphize に多数あるので忘れずに `f.is_override` を伝播
- **`extends` 周りの class_signature**: parent をオーバーレイしてから子の declarations をマージ。`init` / `deinit` は per-class なので override 必須チェックの対象外 (特殊条件あり)
- **`docs/syntax.md` を最新に保つ**: 機能追加するたびに必ず更新する (ユーザーが頻繁に参照)
- **`@extern(C)` の synth パイプライン**: ブロック内の struct / fn / class は `synthesize_extern_c_classes` / `synthesize_extern_c_fns` で AST レベルでトップレベル相当に展開される。ここを通る `@lib` 付き fn には自動で `byValue` / `variadic` / `optional` 属性が付く。下流の native_extern.rs や class registration はこの synth 結果を読む
- **`@lib` fn のシンボル名と ilang 名**: ilang 名は loader の prefix で `module.fn_name` に化けるが、dlsym はオリジナルの C シンボルでなければならない。loader が `c_symbol` フィールドを自動でセット保存する仕組みになっている (`@symbol("...")` が明示されていなければ元の bare name)
- **FFI ヘルパー (`cstrFromString` 等) は loader の prefix から除外**: `prefix_block_calls` の `is_builtin_callee` 判定で組み込みヘルパーのリストを持つ。新ヘルパーを追加するときはここも更新
- **`ilang.toml` の検索**: CLI が entry file の親ディレクトリから上に辿って `ilang.toml` を探す。プロジェクトを横断する CLI 統合テストは現状ないので、変更時は `examples/sdl_breakout/` で動作確認するのが手軽

## WebGPU PoC (`examples/wgpu_triangle`) — 3 OS で動かす手順

ilang から `wgpu-native` (WebGPU の C 実装) を叩く PoC。**SDL2 で独立ウィンドウを作り、その上に wgpu サーフェスを張って WGSL シェーダで三角形を描く**。「シェーダを環境非依存(WGSL 1本)で 3 OS」を狙う検証。現状 **macOS のみ実機確認済み**。バインドは `bindings/wgpu/` (wgpu-native **v29.0.0.0** の `webgpu.h`/`wgpu.h` に固定)。

### 共通の準備

1. **ライブラリ取得**: `third_party/wgpu/fetch.sh` を実行 (`gh` CLI 必須)。OS/arch を自動判定して該当リリースを DL し、dylib/so/dll + ヘッダを `third_party/wgpu/<os-arch>/` に展開、巨大な `.a` と zip は削除する。バイナリは **未コミット** (`.gitignore` 済み)。
   - Windows で bash が無ければ git-bash で `fetch.sh` を実行するか、`gh release download v29.0.0.0 -R gfx-rs/wgpu-native -p "wgpu-windows-x86_64-msvc-release.zip"` を手動展開する。
2. **ライブラリ検索パス**を立てて実行 (バイナリは標準ディレクトリに無いため):
   - macOS:   `DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Linux:   `LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Windows: `wgpu_native.dll` を PATH に通すか、entry と同じディレクトリへ置く。`@extern(C, "wgpu_native")` の bare 名解決で拾われる。

### Windows / Linux を動かすのに必要なコード追加 (★ここが本題)

現状の `examples/wgpu_triangle/main.il` は **macOS の Metal サーフェス生成のみ実装**している (`SDL_Metal_CreateView` → `SDL_Metal_GetLayer` → `WGPUSurfaceSourceMetalLayer`)。Windows/Linux ではここを **OS 別の SurfaceSource に差し替える**だけで、それ以外 (adapter/device/pipeline/draw) はそのまま使える。

ネイティブハンドルは SDL から取得する (`bindings/sdl2/sdl_window.il`):
```
let info = new SysWMinfo()
info.versionMajor = 2; info.versionMinor = 0; info.versionPatch = 0   // SDL_VERSION を埋める
SDL_GetWindowWMInfo(win, &info)
// info.subsystem: windows=1, x11=2, wayland=6
// info.handle1..4 に各ハンドルが入る
```

各 OS の SurfaceSource を `WGPUSurfaceDescriptor.nextInChain` にチェインする (`bindings/wgpu/mod.il` に struct を追加):

- **Windows** (`info.subsystem == 1`): handle1=HWND, handle3=HINSTANCE
  ```
  // sType = WGPUSType_SurfaceSourceWindowsHWND = 5
  struct WGPUSurfaceSourceWindowsHWND { chain: WGPUChainedStruct; hinstance: *void; hwnd: *void }
  // hinstance = info.handle3 as *void, hwnd = info.handle1 as *void
  ```
- **Linux/X11** (`info.subsystem == 2`): handle1=Display*, handle2=Window(XID)
  ```
  // sType = WGPUSType_SurfaceSourceXlibWindow = 6
  struct WGPUSurfaceSourceXlibWindow { chain: WGPUChainedStruct; display: *void; window: u64 }
  // display = info.handle1 as *void, window = info.handle2 as u64
  ```
- **Linux/Wayland** (`info.subsystem == 6`): handle1=display, handle2=surface
  ```
  // sType = WGPUSType_SurfaceSourceWaylandSurface = 7
  struct WGPUSurfaceSourceWaylandSurface { chain: WGPUChainedStruct; display: *void; surface: *void }
  ```

SDL の wayland/x11 の選択は環境変数や `SDL_VIDEODRIVER` に依存するので、`info.subsystem` を見て分岐するのが堅い。`os.platform` で OS 分岐 + `@target` で SurfaceSource struct を出し分けてもよい。

### この PoC で判明した ilang FFI の落とし穴 (踏み直さないこと)

- **`@extern(C)` struct の `@handle` フィールドは値が直列化されない** (別タスクのバグ報告あり)。サーフェス/デバイス等の **ハンドル型フィールドは `*void` で宣言し、代入時に `handle as *void`、読み出し時に `... as WGPUXxx`** でキャストする。関数の引数/戻り値として直接渡すぶんには正常。
- **ハンドル型は必ず `@handle pub struct WGPUXxx {}` を宣言する**。戻り値で使うだけだと型は通っても `handle as i64` 等のキャストが「expected i64, got WGPUXxx」で落ちる。
- **構造体 out パラメータは `&local` で渡す** (値ローカル/`new` どちらでも可)。`new T()` を `*T` 引数へ**直接**渡すと型エラー (syntax.md の Object→*T 自動変換は現状効かない)。caps 取得・`getCurrentTexture` はこの `&` 方式で動く。
- **コマンドバッファ配列は `i64[]` で渡す** (`let cmds: i64[] = [cmd as i64]; wgpuQueueSubmit(queue, 1, cmds)`)。`&cmd` (`@handle` ローカルのアドレス) は不可。バインドの `commands` 引数は `*i64`。
- **値渡しの CallbackInfo + コールバックの `WGPUStringView`**: `wgpuInstanceRequestAdapter`/`RequestDevice` は CallbackInfo を**値渡し**、コールバックは `WGPUStringView` を**値渡し**で受ける。コールバックは `WGPUStringView` を **2つの i64 に展開** (`fn(i32, i64, i64, i64, i64, i64)`) して受けると ABI が合う。wgpu-native はコールバックを同期的に発火するので `wgpuInstanceProcessEvents` 後にスロットから読めばよい。
- **`WGPUStringView.data` は `*void`**。`cstrFromString(s) as *void` を入れ、`length` は `0xFFFFFFFFFFFFFFFF`(=WGPU_STRLEN, SIZE_MAX) にすると wgpu 側で strlen される。
- **enum 値はヘッダ準拠の flat 値**でOK (`fmt`/`alpha`/`present` は `wgpuSurfaceGetCapabilities` の実値を使うのが堅い)。`WGPUTextureUsage`/`WGPUColorWriteMask` は **64bit (WGPUFlags)** なので struct フィールドは `u64`。
- **ドローアブル取得**: `wgpuSurfaceGetCurrentTexture` はウィンドウが**画面に合成されるまで** texture=null を返す。`SDL_ShowWindow` + `SDL_RaiseWindow` でウィンドウを前面化し、`SDL_PumpEvents` を回しつつ **texture が取れるまでリトライ**する (PoC のループ参照)。

### 既知の未解決/保留

- 取得ライブラリと同梱ヘッダで **`WGPUSurfaceGetCurrentTextureStatus` の値が食い違って見える瞬間がある** (texture 取得失敗時に `0x00030001` が観測された)。texture 取得成功時は `status=1` で header と一致するので、**status の数値で判定せず `texture != 0` で判定**している。
- Windows/Linux は**未実機検証**。上記 SurfaceSource を追加したら、まず単色クリア → 三角形の順で確認するのが安全。
