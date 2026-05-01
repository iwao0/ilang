# ilang フェーズ4: クラス (JS風)

## Context

サプライチェーン対策の核となる capability システムは、最終的に「ライブラリ/クラス単位で `net`/`file` 権限を持たせる」設計。そのためまずクラスを導入する。Rust の `struct` ではなく、JS のクラスベース構文を採用 (`class`/`new`/`this`)。

## 実装した機能 (案A: 最小)

- `class Name { fields, methods }` 宣言 (TS 風フィールド宣言)
- `init(args)` をコンストラクタとして特別扱い (キーワードではなく、特殊な名前のメソッド)
- `new Name(args)` でインスタンス生成
- `obj.field` / `obj.field = value` でフィールド読み書き
- `obj.method(args)` でメソッド呼び出し
- `this` で自身を参照 (メソッド外で使うとコンパイルエラー)
- ARC: 内部表現は `Rc<RefCell<HashMap<String, Value>>>`
- 等価性: `==` `!=` は **参照等価**、その他比較はエラー

## スコープ外 (フェーズ5以降)

- 継承 (`extends`, `super`)
- `static` メンバー
- `get` / `set` (getter/setter)
- private (`#x`)
- 演算子オーバーロード
- `instanceof` / クラス階層

## 構文サンプル

```rust
class Counter {
    count: i64

    init(start: i64) {
        this.count = start
    }

    increment(): i64 {
        this.count = this.count + 1
        this.count
    }

    get(): i64 { this.count }
}

let c = new Counter(10)
c.increment()
c.increment()
c.get()           // => 12
```

メソッド呼び出しのチェーン:
```rust
class Calc {
    n: i64
    init(x: i64) { this.n = x }
    doubled(): i64 { this.n * 2 }
    quadrupled(): i64 { this.doubled() * 2 }
}
new Calc(5).quadrupled()   // => 20
```

## 実装上の決定事項

| 項目 | 採用 | 理由 |
| --- | --- | --- |
| コンストラクタ名 | `init` | Swift と同じ。`constructor` は冗長 |
| `this` | キーワード | `obj.field` と同じ syntax で扱える |
| `new` | キーワード必須 | 関数呼び出しと視覚的に区別 |
| フィールド宣言 | TS 風クラス本体で型注釈付き | 静的型付けと相性が良い |
| init 省略 | OK (引数なし `new C()`) | 純データ的な使い方を許す |
| 同名フィールド/メソッド | 未対応 | 名前空間を分けると混乱 |
| 親クラス | 未対応 (フェーズ5以降) | スコープ縮小 |
| 等価性 | 参照等価のみ | structural equality は将来トレイト経由 |

## 型システムへの影響

- `Type::Object(String)` を追加 (クラス名でインスタンス型を表現)
- `Type` から `Copy` を外し `Clone` 経由に統一
- 型チェッカーは `classes: HashMap<String, ClassSig>` を持つ
- メソッド本体は `in_class: Option<&str>` を引数に取り、`this` の型を解決

## 評価器への影響

- `Value::Object(Rc<RefCell<ObjectData>>)` を追加 (ARC 相当)
- `Interpreter` に `classes` テーブルと `this: Option<ObjectRef>` を追加
- メソッド呼び出し時に `this` を一時セット、終了後復元
- フィールドへの初回代入 = 作成 (型チェッカーが正当性を保証済み)

## 既知の制限

1. **未初期化フィールドの読み出しはランタイムエラー**: 型チェッカーは "definite initialization" を追跡しない。`init` でセットし忘れたフィールドを読むと `UnknownField`
2. **同一行に複数のクラスメンバーを並べると ASI が効かない**: `count: i64 init(...)` は `;` か改行で区切る必要あり
3. **属性 `#[requires(...)]` はクラスには付与不可** (メソッドには可、enforce はまだ未実装)

## オープン課題

### capability 実装の本丸 (フェーズ5)
- メソッドの `#[requires(net)]` を呼び出し時に enforce
- クラス自体に capability を持たせる構文 (`class Http with cap(net) { ... }` ?)
- ライブラリ境界での capability 強制

### その他
- 継承は本当に要るか? (composition 重視の設計もあり)
- 配列/辞書/文字列 (構文がまだ無い)
- `match` / ADT
