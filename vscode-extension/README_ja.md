# vscode-ilang

ilang 用の VSCode extension。シンタックスハイライトに加えて
language server (`ilang-lsp`) を同梱する。

English: [README.md](README.md)

## ローカルインストール

```sh
# 1. language server をビルド (release 推奨)
cargo build --release -p ilang-lsp

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
3. `<workspace>/target/release/ilang-lsp`
4. `<workspace>/target/debug/ilang-lsp`

## エディタ機能

- `.il` ファイルの認識
- VSCode のファイルアイコンテーマが許す場合、`.il` に既定の
  言語アイコンを設定
- キーワード / 型 / 数値リテラル / 文字列 / コメント / 属性
  (`@flags` 等) のハイライト
- ブラケット自動補完 / コメントトグル

## language server 機能

- **診断**: パーサ / 型チェッカのエラーを赤波線で表示。
  保存前のバッファ内容を基準に更新する
- **hover**: カーソル位置の識別子のシグネチャと `///` doc
  コメントを表示。ローカル / パラメータ / フィールド /
  メソッド / getter/setter / enum variant / `use module` で
  取り込んだ名前 / 配列・文字列・Map のビルトインメソッドに
  対応
- **実装ジャンプ (Cmd/Ctrl+F12)**: interface 名の上では実装
  クラス一覧、クラス名の上では直接のサブクラス一覧、
  interface メソッドの上では各クラスの実装、親クラスの
  メソッドの上では override しているサブクラスのメソッドに
  ジャンプする。スキャン範囲は参照検索 / リネームと同じ
  `ilang.toml` 配下
- **定義ジャンプ (F12)**
  - 同一ファイルの宣言
  - `use module` 経由の別ファイル (stdlib や `ilang.toml` の
    `[deps]` パスを含む)
  - `use super.M` — dep DAG 上の親パッケージへ
  - クラスの親 / インターフェース名 / 型注釈
- **ドキュメントハイライト**: 識別子の上にカーソルを置くと、
  同じファイル内の同じ宣言を指す出現箇所をすべてハイライト
  する。ターゲット解決は参照検索と同じだがバッファ内に限定
- **参照検索 (Find All References)**: ワークスペース全体
  (ファイルの `ilang.toml` から辿れる `.il` と開いている
  バッファ全部)
- **リネーム**: 参照検索と同じ範囲。`textDocument/prepareRename` で
  `this` / キーワード / ローカル宣言に紐付かない識別子 (builtin や
  外部 import) は事前に拒否する。新しい名前は ASCII 識別子かつ
  キーワードでないかを検証したうえで、編集前にスコープ衝突
  チェックを走らせる: 同一ブロック内の `let`、同一関数の
  パラメータ、クラスメンバ (field / method / property / getter /
  setter)、トップレベル宣言、selective import 名、enum variant
  との衝突は `invalid_params` エラーとしてエディタ側に返し、
  パースできないソースが残らないようにする
- **補完**
  - トップレベル宣言 / ローカル / パラメータ / enum variant
  - `obj.` のメンバ補完 (フィールド / メソッド / getter /
    setter / 配列・文字列・Map のビルトイン)
  - `:` / `,` / `<` の後の型補完
  - `@` の後の属性補完
  - `use M { … }` の selective import 名
  - インターフェース実装時のメソッドスタブ
  - キーワード補完 (`super` も含む)
- **シグネチャヘルプ**: `(` / `,` でパラメータヒント。
  `<` で再トリガし、オーバーロード切替も対応
- **ドキュメントフォーマット**: ファイル全体の整形
- **コードアクション**
  - `source.organizeImports` — `use` 行のソート / 重複削除
  - 代表的な診断に対する quick fix
  - クラスのフィールドから `init(...)` を生成
  - 未実装のインターフェースメソッドを生成
  - match のアームを enum の全 variant で埋める
- **ワークスペースシンボル**: `Cmd/Ctrl+T` でワークスペース
  (ilang.toml 配下の `.il`) 全体からシンボル検索する。トップ
  レベルの fn / class / interface / enum / const / struct /
  union と、クラスメンバ (field / method / property / static) /
  enum variant が対象。クエリは case-insensitive な subsequence
  マッチ。最大 2000 件。ファイルごとの結果は mtime をキーに
  キャッシュし、ソースが実際に変わったファイルだけ再パース
  する (open バッファは常にライブテキストを使用)
- **ドキュメントシンボル (アウトライン)**: トップレベルの
  fn / class (フィールド・メソッド・プロパティ・static を
  ぶら下げる) / interface / enum (variant をぶら下げる) /
  const / `@extern(C)` の項目を階層化して返す
- **コードレンズ (CodeLens)**: トップレベル宣言の上にインラインの
  アクション行を表示する。fn / class / interface / enum /
  クラスメソッドには「N references」、class / interface には
  「N implementations」。空コマンドのレンズだけ返し、件数は
  `codeLens/resolve` で遅延計算するので、可視範囲のレンズだけが
  ワークスペーススキャンのコストを払う。References レンズの
  クリックで参照リストを開き、Implementations レンズのクリックで
  実装ジャンプを開く
- **折り畳み範囲 (Folding Range)**: トップレベル宣言 (fn / class /
  interface / enum / struct / union / `@extern(C)` ブロック) と
  ソース内の複数行 `{ ... }` ブロックを fold 可能にする。複数行
  にまたがる `use M { … }` は `kind: imports` を付けて返す。
  AST 走査ではなく `{` / `}` トークンのペアリングで実装
- **選択範囲 (Selection Range)**: 拡張選択チェーン: カーソル位置の
  識別子 → 包含する `(` / `[` / `{` ペア → 上位のペア → ファイル
  全体。ブラケットベース実装なので、parser が一部のノードにしか
  埋めていない `end_line` / `end_col` に依存しない
- **インレイヒント**: 2種類提供する。型ヒントは型注釈のない
  `let x = expr` / `for x in iter` の後ろに推論型を `: T` として
  表示。パラメータ名ヒントは関数呼び出しのリテラル引数 (数値 /
  文字列 / bool / `none` / 配列リテラル) の前に `name:` を出す。
  識別子引数はもとから名前を持っているのでヒントを出さない。
  パラメータ名解決は同一ファイル内の fn / method / `init` 限定で、
  別ファイルの呼び出しはスキップする
- **コールヒエラルキー**: fn / method / static method 上で
  `Show Call Hierarchy` を呼ぶと、呼び出し元 (incoming) と
  呼び出し先 (outgoing) のツリーが開く。呼び出し元は
  ワークスペースの `ilang.toml` から辿れる `.il` 全体を、呼び
  出し先は関数本体内の解決済み参照を集める。型ヒエラルキーは
  ピン留めされている `lsp-types 0.94` が LSP 3.17 の
  `typeHierarchyProvider` に対応していないため未提供
- **セマンティックトークン**: 識別子を class / interface /
  enum / enumMember / struct / function / method / property /
  parameter / variable / namespace に分類し、該当箇所に
  `declaration` / `static` / `readonly` modifier を付与する。
  TextMate grammar の上に重ねて、構文だけでは区別できない
  使い方 (例: 関数呼び出しかローカル変数か) を色分けする
  (full document のみ。range / delta 要求は未対応)

## 索引化の範囲

LSP は開いているファイルとその `ilang.toml` から辿れる範囲を
解析する: `use module` の対象、`[deps]` (`target` フィルタ
含む) と推移的な依存。バッファのテキストが基準なので、診断 /
hover / 補完などは保存前の編集内容を反映する。
