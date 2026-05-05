# vscode-ilang

ilang 用の VSCode extension。シンタックスハイライトに加え、
language server (`ilang-lsp`) による診断 / hover / 定義ジャンプを
提供する。

English: [README.md](README.md)

## ローカルインストール

```sh
# 1. language server をビルド
cargo build -p ilang-lsp

# 2. extension クライアント (TypeScript) をビルド
cd vscode-extension
npm install
npm run compile

# 3. VSCode の拡張機能ディレクトリにシンボリックリンク
ln -s "$(pwd)" ~/.vscode/extensions/ilang
```

VSCode を再起動すれば `.il` ファイルでハイライトが効き、
language server も自動で起動する。

`ilang-lsp` バイナリの探索順:

1. 設定 `ilang.serverPath` (絶対パス)
2. 環境変数 `ILANG_LSP_PATH`
3. `<workspace>/target/debug/ilang-lsp` (開発時のデフォルト)

## 機能

- `.il` ファイルの認識
- キーワード / 型 / 数値リテラル / 文字列 / コメント / 属性 (`@flags` 等)
  のハイライト
- ブラケット自動補完 / コメントトグル
- **診断**: パーサ / 型チェッカのエラーを赤波線で表示
- **hover**: トップレベル fn / class / enum / const にカーソルを
  合わせるとシグネチャを表示
- **定義ジャンプ (F12)**: 宣言箇所にジャンプ

## 制限

LSP は現状 **同一ファイル内のトップレベル宣言のみ** を索引化する。
ローカル変数 / クラスメンバ / 他ファイルへの参照は未対応。
