# ilang

新しいプログラミング言語 **ilang** の処理系。

## ビジョン

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する
- **ARC** によるメモリ安全性
- **Rust 風** の関数宣言・型名構文
- 四則演算規則は **C / JavaScript** とほぼ同一

## 現在の状態 (フェーズ1)

数値の四則演算のみが評価できる最小インタプリタ。詳細は [docs/phase1-plan.md](docs/phase1-plan.md)。

```sh
# REPL
cargo run -p ilang-cli

# ファイル実行
echo '1 + 2 * 3' > sample.il
cargo run -p ilang-cli -- run sample.il
```

## 開発

```sh
cargo test --workspace
```

## ライセンス

MIT OR Apache-2.0
