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
- **hover**: カーソル位置の識別子のシグネチャを表示
- **定義ジャンプ (F12)**: 宣言箇所にジャンプ

## 索引化される範囲

LSP が同一ファイル内で解決するもの:

- **トップレベル宣言** — fn / class / enum / const
- **ローカル変数 / パラメータ** — `let` / fn パラメータ / fn-expr / `for x in ...`
- **`this`** — 囲みクラスへ解決
- **`this.field` / `this.method(...)`** — クラスメンバへ解決
- **`obj.field` / `obj.method(...)`** ただし `obj` が明示型注釈付きの
  ローカルか `new ClassName(...)` の結果のとき

診断は `ilang run` と同じ loader パイプラインを使う:
`use module` / `ilang.toml` の `[deps]` パスを解決し、トップ
レベル `const` を inline してから型チェックする。診断はディスクの
内容を基準にするので、未保存の編集は保存するまで反映されない。

他ファイルへの F12 / hover (`use module` 経由のジャンプ) は未対応で、
索引化は現在開いているファイルだけ。
