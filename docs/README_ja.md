# 🚀 ilang

[English](../README.md) | 日本語

> 🦀 **ARC** メモリ安全 · ⚡ **Cranelift JIT** · 🔗 **C FFI** · 🎮 **SDL2 ゲームデモ** 同梱

新しいプログラミング言語 **ilang** の処理系。ツリーウォーク
インタプリタで素早いフィードバック、Cranelift JIT で高速実行、
Rust 風の文法で C ライブラリとも連携できます。

```rust
fn fib(n: i64): i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
console.log(fib(20))    // → 6765
```

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
| 継承 (`extends` / `super`) + 仮想ディスパッチ | ✅ |
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
| Cranelift JIT (`ilang run --jit`) | ✅ |
| FFI (`@extern(C) {}` ブロックで C ライブラリ呼び出し) | ✅ |
| capability アノテーション | 🚧 パースのみ |

所有権 / `mut` / 借用は採用せず、変数はすべて再代入可能。
エラーは `filename [row:col]: message` の統一形式。

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
# 💬 REPL (let / fn が永続化、interpreter モード)
cargo run -p ilang-cli

# 📄 ファイル実行 (`;` は省略可、改行が文の区切りになる JS 風 ASI)
cargo run -p ilang-cli -- run path/to/script.il

# ⚡ JIT で実行 (Cranelift ネイティブコード — interpreter の数十〜数百倍速)
cargo run -p ilang-cli -- run --jit path/to/script.il
```

`@extern(C) {}` で C ライブラリを呼ぶコード(後述の SDL2
サンプルなど)は **JIT 専用** です。dlsym 経由で関数を解決するので
interpreter からは呼べません。

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
cargo run -p ilang-cli -- run --jit examples/sdl_breakout/main.il
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
レベルの fixture(各 `.il` ファイルが interpreter と JIT 両方で実行
され、`expect:` / `expect-error:` のマジックコメントで結果を検証)も
全部走ります:

```sh
cargo test --workspace
```

## 📄 ライセンス

MIT OR Apache-2.0
