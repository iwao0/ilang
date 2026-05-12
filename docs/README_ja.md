# 🚀 ilang

[English](../README.md) | 日本語

> 🦀 **ARC** メモリ安全 · ⚡ **MIR → Cranelift JIT** · 🔬 **リーク検出が標準** · 🔗 **C FFI** · 🎮 **SDL2 ゲームデモ** 同梱

新しいプログラミング言語 **ilang** の処理系。MIR ベースの
Cranelift JIT で起動も定常もネイティブ速度、ARC による
deterministic な解放(GC ポーズなし)、Rust 風の文法で
C ライブラリとも連携できます。

<p align="center">
  <a><img src="https://github.com/iwao0/ilang/releases/download/demo-assets/breakout.gif" alt="ilang で書いた Breakout — Cranelift JIT + SDL2 で動作" width="600"></a>
</p>

> ilang で書いた Breakout — クラス・ARC・クロージャ・SDL2 バインディング、JIT コンパイル実行。
> ソース: [`examples/sdl_breakout/`](../examples/sdl_breakout/)

## ✨ 構文サンプル: 再帰で書く Fibonacci

```rust
fn fib(n: i64): i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
console.log(fib(20))    // → 6765
```

## 🔬 リーク検出が言語の標準機能

メモリのリークチェックは、多くの言語で外部ツール
(Valgrind / ASan / Xcode Instruments / heap snapshot)が必要です。
ilang は標準 `test` モジュールにアロケータ計測を組み込んでいて、
リーク検証は **普通の `expect` と同じ書き味** で書けます:

```rust
use test

let baseline = test.liveAllocBytes()

let i = 0
while i < 1000 {
    let _ = "x" + i.toString()   // 中間文字列、毎回 heap 確保
    i = i + 1
}

// ループ中で確保された中間ヒープはすべて解放されているはず。
test.expectTrue(test.liveAllocBytes() - baseline < 1024)
```

`test.liveAllocBytes()` / `liveAllocCount()` / `liveStringCount()`
はランタイムが現在保持している量を返します。
[`tests/programs/05_edge_cases/leak_*.il`](../crates/ilang-cli/tests/programs/05_edge_cases/)
にはこのパターンで構成毎の「メモリ契約」を固定する fixture が
30 件以上あり、文字列連結 / 配列 push / クロージャセル /
enum payload / Map 削除 / weak upgrade などを網羅しています。

ilang の中で他言語と最も差別化できる部分はおそらくここです。
すべての PR が leak テストを自動で走らせる — 外部ツールも、
サンプリングプロファイラも、環境設定も不要です。

---

## ✨ ビジョン

- 🛡️ **capability ベースのセキュリティ**: ライブラリ/クラスごとに
  `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する
  *(現状は属性のパースのみ。enforce 化が次の大きなマイルストーン)*
- 🦀 **ARC** によるメモリ安全性
- 🦀 **Rust 風** の関数宣言・型名構文
- 🌐 四則演算規則は **C / JavaScript** とほぼ同一

## 📚 構文一覧

実装済みの構文・型・組み込みは **[syntax.md](syntax.md)** に
チートシートとしてまとめてあります。

## ✅ 現在の状態

| カテゴリ | 状態 |
| --- | :---: |
| 数値型 (i8–i64 / u8–u64 / f32 / f64) + `as` キャスト | ✅ |
| `bool` / 比較 / 短絡論理 | ✅ |
| `let` / 代入 / 複合代入 (`+=` ほか) | ✅ |
| 制御構造 (`if` / `else` / `while` / `loop` / `break` / `continue` / 早期 `return`) | ✅ |
| 文字列 / 配列 (動的・固定長) | ✅ |
| `class` / `new` / `init` / `this` / `deinit` (JS 風) | ✅ |
| 継承 (`class Sub: Parent` / `super`) + 仮想ディスパッチ | ✅ |
| インタフェース (`interface I { ... }`) + 動的ディスパッチ | ✅ |
| `console.log` | ✅ |
| Optional (`T?` / `some` / `none` / `if let`) | ✅ |
| 弱参照 (`T.weak` / `.get()`) | ✅ |
| `enum` + `match` (組み込み `Result<T, E>` 含む) | ✅ |
| `Map<K, V>` / Tuple | ✅ |
| クロージャ (キャプチャ込み) | ✅ |
| ジェネリクス (関数 / クラス / enum) | ✅ |
| 関数オーバーロード | ✅ |
| ARC によるメモリ管理 | ✅ |
| 型チェック | ✅ |
| MIR → Cranelift JIT (`ilang run`、デフォルト) | ✅ |
| MIR → Cranelift AOT (`ilang build foo.il -o foo`、macOS) | ✅ |
| リーク検出ヘルパー (`test.liveAllocBytes` / `liveAllocCount` / `liveStringCount`) | ✅ |
| FFI (`@extern(C) {}` ブロックで C ライブラリ呼び出し) | ✅ |
| capability アノテーション | 🚧 パースのみ |

所有権 / `mut` / 借用は採用せず、変数はすべて再代入可能。
エラーは `filename [row:col]: message` の統一形式。

## ⏱️ ベンチマーク

C / Rust / Node.js / Lua / Python と同じアルゴリズムで実装したマイクロベンチ 6本。3 回の中央値。`ilang JIT` 列は `ilang run` の所要時間 (パース + コンパイル + 実行) を含むので、短いベンチでは compile 時間に支配されます。ソースとランナーは [benchmarks/](../benchmarks/) を参照してください。

著者の環境 (M シリーズ Mac, macOS 15, `cc` Xcode CLT 16, `rustc 1.x -O`, Node.js 22, Python 3.13):

| ベンチ          | C     | Rust  | ilang AOT | ilang JIT | Node.js | Lua | Python |
|-----------------|-------|-------|-----------|-----------|---------|-----|--------|
| `fib(40)`       | 0.15s | 0.15s | 0.34s     | 0.34s     | 0.66s   | --  | 13.43s |
| `mandelbrot`    | 0.45s | 0.52s | 0.68s     | 0.58s     | 0.61s   | --  | 62.38s |
| `sort` (20万)   | 0.01s | 0.01s | 0.02s     | 0.01s     | 0.05s   | --  | 0.22s  |
| `linked_list`   | 0.03s | 0.02s | 0.03s     | 0.03s     | 0.05s   | --  | 0.42s  |
| `string_concat` | 0.02s | 0.00s | 0.51s     | 0.47s     | 0.02s   | --  | 0.02s  |
| `ffi`           | 0.01s | 0.01s | 0.03s     | 0.01s     | --      | --  | 2.08s  |

各自の環境で再現するには `bash benchmarks/run.sh`。未インストールの処理系は `--` になります (上の表生成時、著者の環境には Lua が未インストールでした。`lua` を入れて再実行すれば埋まります)。

要点:

- **整数 / 浮動小数のホットループ** (`fib`, `mandelbrot`) は C の 1.3〜2.3 倍。Cranelift は LLVM ではないですが、同じ土俵に乗っています。
- **ヒープ確保が中心** のワークロード (`linked_list`) は C と同等。ARC のノードあたりオーバーヘッドが小さく、GC pause もありません。
- `string_concat` は明確な弱点。毎回 ARC レジストリ経由で新しい文字列を確保しているため遅いです。配列ビルダーや将来の `StringBuilder` を使ってください。
- `ilang JIT` ≈ `ilang AOT` なのは、これらのベンチが compile 時間を相対的に小さく押し下げる長さだから。1 秒未満のスクリプトは JIT > AOT になります。

## 🔧 セットアップ

ilang は Rust で書かれているので **Rust toolchain** (rustup /
cargo) が必要です。インストールされていなければ
<https://rustup.rs> から:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

リポジトリをクローン:

```sh
git clone https://github.com/iwao0/ilang
cd ilang
cargo build           # 初回のみ依存解決 + ビルド (~1 分)
```

## 🚀 使い方

```sh
# 💬 REPL (let / fn が永続化、JIT バック)
cargo run -p ilang-cli

# 📄 ファイル実行 (`;` は省略可、改行が文の区切りになる JS 風 ASI)
#    MIR → Cranelift JIT パイプラインを使う。
cargo run -p ilang-cli -- run path/to/script.il

# 📦 ネイティブ実行可能ファイルを生成 (macOS、Cranelift AOT)。
#    システムの `cc` が `libilang_runtime.a` とプログラムが参照する
#    `@lib("X")` を一緒にリンクするので、生成バイナリは Rust toolchain
#    が入っていない環境でも動きます。
cargo run -p ilang-cli -- build path/to/script.il -o path/to/script
./path/to/script
```

JIT は `@extern(C) {}` の C シンボルを起動時 dlsym で解決し、AOT は
リンク時に解決します (見つからない `@optional` lib はスキップして
abort stub に差し替え)。

### 🧮 サンプル: 1〜100 で 3 か 5 の倍数を数える

```sh
cat > sample.il <<'EOF'
fn count_div(n: i64): i64 {
    let i = 1
    let count = 0
    while i <= n {
        if i % 3 == 0 || i % 5 == 0 {
            count = count + 1
        }
        i = i + 1
    }
    count
}
count_div(100)
EOF
cargo run -p ilang-cli -- run sample.il   # => 47
```

### 🧱 クラス

`class` / `new` / `init` (コンストラクタ) / `this` を備えた JS
風オブジェクト。メソッド本体ではフィールド/メソッドの `this.` を
省略可 (ローカル/引数があればそちらが優先)。

```sh
cat > counter.il <<'EOF'
class Counter {
    count: i64
    init(start: i64) { this.count = start }
    bump(): i64 {
        count += 1     // `this.count += 1` と同義
        count
    }
}

let c = new Counter(10)
let i = 0
loop {
    if i >= 5 { break }
    c.bump()
    i += 1
}
c.bump()
EOF
cargo run -p ilang-cli -- run counter.il   # => 16
```

### 🎮 サンプル: SDL2 でゲーム画面を出す

`examples/sdl_breakout/` に SDL2 を使ったデモが入っています。
ネオン調のブロック崩しで、パドル / ブロック / パーティクル /
ゲームパッド対応つき。矢印キー / `A` `D` (または十字キー / 左
スティック) でパドルを動かし、`Space` でボール発射、`F` で
フルスクリーン切替、`R` でゲームオーバー後にリスタート、`ESC`
で終了。

事前に SDL2 をインストール:

```sh
# 🍎 macOS (Homebrew)
brew install sdl2

# 🐧 Debian/Ubuntu
sudo apt install libsdl2-dev libsdl2-2.0-0
```

実行:

```sh
# JIT
cargo run -p ilang-cli -- run examples/sdl_breakout/main.il

# AOT — SDL2 を実行時にロードする 735 KB ほどの Mach-O 実行
# ファイルを生成。
cargo run -p ilang-cli -- build examples/sdl_breakout/main.il -o breakout
./breakout
```

このサンプルは `bindings/sdl2/` 以下に置かれた SDL2 用バインディング
を `use sdl` で取り込んでいます。仕組み(`ilang.toml` の `[deps]`
欄)については [../bindings/sdl2/README.md](../bindings/sdl2/README.md)
を参照。

自分のプロジェクトで SDL2 バインディングを使うには、エントリ
ファイルと同じ階層(またはその上の階層)に `ilang.toml` を置きます:

```toml
[package]
name = "my_game"

[deps]
sdl2 = "/path/to/ilang/bindings/sdl2"
```

CLI が起動時にこのファイルを探し、`[deps]` で指定された path を
`use` の探索先に追加します。

## 🧩 VSCode

`vscode-extension/` にシンタックスハイライト + language server
(`ilang-lsp`) による診断 / hover / 定義ジャンプが入っています。
ローカルインストール:

```sh
# 1. language server をビルド
cargo build -p ilang-lsp

# 2. extension クライアントをビルド
cd vscode-extension
npm install
npm run compile

# 3. VSCode の拡張機能ディレクトリにシンボリックリンク
ln -s "$(pwd)" ~/.vscode/extensions/ilang
```

VSCode を再起動すれば反映されます。設定 (`ilang.serverPath`) や
現在の制限は
[vscode-extension/README_ja.md](../vscode-extension/README_ja.md)
を参照。

## 🛠️ 開発

ワークスペース全体のテストを実行します。各 crate の Rust ユニット
テストに加えて、`crates/ilang-cli/tests/programs/` 以下にある言語
レベルの fixture (各 `.il` ファイルが MIR → Cranelift JIT で実行
され、`expect:` / `expect-error:` のマジックコメントで結果を検証)
も走ります:

```sh
cargo test --workspace

# AOT パスでも全 fixture を回す場合 (build + 実行 + stdout を JIT と
# 比較)。所要時間 +80 秒程度。
ILANG_TEST_AOT=1 cargo test --workspace
```

## 📄 ライセンス

MIT OR Apache-2.0
