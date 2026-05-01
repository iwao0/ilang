# ilang

新しいプログラミング言語 **ilang** の処理系。

## ビジョン

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する
- **ARC** によるメモリ安全性
- **Rust 風** の関数宣言・型名構文
- 四則演算規則は **C / JavaScript** とほぼ同一

## 現在の状態 (フェーズ2)

`let`、関数定義、最小の型チェック、capability アノテーション (`#[requires(net)]` のパースのみ、enforcement はフェーズ3) まで実装済み。

詳細: [docs/phase1-plan.md](docs/phase1-plan.md), [docs/phase2-plan.md](docs/phase2-plan.md)

```sh
# REPL (let と fn が永続化)
cargo run -p ilang-cli

# ファイル実行
cat > sample.il <<'EOF'
fn double(x: i64) -> i64 { x * 2 }
let n = 21;
double(n)
EOF
cargo run -p ilang-cli -- run sample.il   # => 42
```

## 開発

```sh
cargo test --workspace
```

## ライセンス

MIT OR Apache-2.0
