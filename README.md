# ilang

新しいプログラミング言語 **ilang** の処理系。

## ビジョン

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する
- **ARC** によるメモリ安全性
- **Rust 風** の関数宣言・型名構文
- 四則演算規則は **C / JavaScript** とほぼ同一

## 構文一覧

実装済みの構文・型・組み込みは **[docs/syntax.md](docs/syntax.md)** にチートシートとしてまとめてあります。

## 現在の状態

`let` / `fn` / 型チェック / capability アノテーション (パースのみ) に加えて、`bool`・比較演算子・短絡論理演算子・`if`/`else`/`while` / `loop` / `break` / `continue`・代入・複合代入・全 10 数値型 + `as` キャスト・文字列・配列 (動的/固定長)・JS 風クラス (`class` / `new` / `this` / `init` / `deinit`)・`console.log`・Optional (`T?` / `some` / `none` / `if let`)・弱参照 (`T.weak` / `.get()`)・FFI (`@extern(C) {}` ブロックで C ライブラリ呼び出し) を実装済み。Cranelift JIT (`ilang run --jit`) も全機能対応。所有権/`mut`/借用は採用せず、変数はすべて再代入可能。エラーは `filename [row:col]: message` の統一形式。

詳細な経緯: [docs/phase1-plan.md](docs/phase1-plan.md), [docs/phase2-plan.md](docs/phase2-plan.md), [docs/phase3-plan.md](docs/phase3-plan.md), [docs/phase4-plan.md](docs/phase4-plan.md), [docs/HANDOFF.md](docs/HANDOFF.md)

## セットアップ

ilang は Rust で書かれているので **Rust toolchain** (rustup / cargo) が必要です。インストールされていなければ <https://rustup.rs> から:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

リポジトリをクローン:

```sh
git clone https://github.com/<your>/ilang
cd ilang
cargo build           # 初回のみ依存解決 + ビルド (~1 分)
```

## 使い方

```sh
# REPL (let / fn が永続化、interpreter モード)
cargo run -p ilang-cli

# ファイル実行 (`;` は省略可、改行が文の区切りになる JS 風 ASI)
cargo run -p ilang-cli -- run path/to/script.il

# JIT で実行 (Cranelift ネイティブコード — interpreter の数十〜数百倍速)
cargo run -p ilang-cli -- run --jit path/to/script.il
```

`@extern(C) {}` で C ライブラリを呼ぶコード(後述の SDL2 サンプルなど)は **JIT 専用** です。dlsym 経由で関数を解決するので interpreter からは呼べません。

### サンプル: 1〜100 で 3 か 5 の倍数を数える

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

### クラス

`class` / `new` / `init` (コンストラクタ) / `this` を備えた JS 風オブジェクト。メソッド本体ではフィールド/メソッドの `this.` を省略可 (ローカル/引数があればそちらが優先)。

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

### サンプル: SDL2 でゲーム画面を出す

`examples/sdl_bouncing_ball/` に SDL2 を使ったデモが入っています。動くボールが壁に当たるたびにビープ音が鳴り、矢印キー / `A` / `D` でパドルを左右に動かせます。`ESC` で早期終了。

事前に SDL2 をインストール:

```sh
# macOS (Homebrew)
brew install sdl2

# Debian/Ubuntu
sudo apt install libsdl2-dev libsdl2-2.0-0
```

実行:

```sh
cargo run -p ilang-cli -- run --jit examples/sdl_bouncing_ball/main.il
```

このサンプルは `bindings/sdl2/` 以下に置かれた SDL2 用バインディングを `use sdl` で取り込んでいます。仕組み(`ilang.toml` の `[deps]` 欄)については [bindings/sdl2/README.md](bindings/sdl2/README.md) を参照。

自分のプロジェクトで SDL2 バインディングを使うには、エントリファイルと同じ階層(またはその上の階層)に `ilang.toml` を置きます:

```toml
[package]
name = "my_game"

[deps]
sdl2 = "/path/to/ilang/bindings/sdl2"
```

CLI が起動時にこのファイルを探し、`[deps]` で指定された path を `use` の探索先に追加します。

## 開発

```sh
cargo test --workspace
```

## ライセンス

MIT OR Apache-2.0
