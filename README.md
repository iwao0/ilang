# ilang

新しいプログラミング言語 **ilang** の処理系。

## ビジョン

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する
- **ARC** によるメモリ安全性
- **Rust 風** の関数宣言・型名構文
- 四則演算規則は **C / JavaScript** とほぼ同一

## 現在の状態 (フェーズ3)

`let` / `fn` / 型チェック / capability アノテーション (パースのみ) に加えて、`bool`・比較演算子・短絡論理演算子・`if`/`else`/`while`・代入を実装済み。所有権/`mut`/借用は採用せず、変数はすべて再代入可能。

詳細: [docs/phase1-plan.md](docs/phase1-plan.md), [docs/phase2-plan.md](docs/phase2-plan.md), [docs/phase3-plan.md](docs/phase3-plan.md)

```sh
# REPL (let / fn が永続化)
cargo run -p ilang-cli

# ファイル実行 (`;` は省略可、改行が文の区切りになる JS 風 ASI)
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

## 開発

```sh
cargo test --workspace
```

## ライセンス

MIT OR Apache-2.0
