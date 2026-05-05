# vscode-ilang

ilang 用の VSCode extension。Stage A はシンタックスハイライトのみ。
F12 / hover などの言語サーバ機能は Stage B で別途実装する。

English: [README.md](README.md)

## ローカルインストール

VSCode (or Cursor) からこの拡張機能を有効にする方法は 2 通り。

### 方法 1: 開発用シンボリックリンク (推奨)

```sh
ln -s "$(pwd)/vscode-extension" ~/.vscode/extensions/ilang
```

VSCode を再起動すると `.il` ファイルでハイライトが効くようになる。
編集後はもう一度再起動すれば反映される。

### 方法 2: `.vsix` パッケージとしてインストール

```sh
npm install -g @vscode/vsce
cd vscode-extension
vsce package          # ilang-0.1.0.vsix が生成される
code --install-extension ilang-0.1.0.vsix
```

## 機能 (Stage A)

- `.il` ファイルの認識
- キーワード / 型 / 数値リテラル / 文字列 / コメント / 属性 (`@flags` 等)
  のハイライト
- ブラケット自動補完 / コメントトグル

## 今後 (Stage B)

- 別 crate `ilang-lsp` を立て、`tower-lsp` で LSP を実装
- 機能: 定義ジャンプ (F12) / hover / 診断 (赤波線) / 補完
