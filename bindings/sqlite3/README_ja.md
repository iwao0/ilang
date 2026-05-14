# ilang 向け SQLite3 バインディング

[English](./README.md)

`libsqlite3` を `Database` / `Statement` の ARC 管理ラッパに包んだバインディング。`@extern(C) { @lib("sqlite3") }` で dlsym するため **JIT 専用**。

## モジュール

| ファイル | 内容 |
| --- | --- |
| `sqlite3.il` | `Database` / `Statement` クラス、`SqliteError`、`StepResult`、`ColumnType`、および `sqlite3_*` の C エントリポイント一式 |

## 自分のプロジェクトから使う

エントリファイルの隣に `ilang.toml` を作成:

```toml
[package]
name = "my_app"

[deps]
sqlite3 = "/absolute/or/relative/path/to/bindings/sqlite3"
```

その上で:

```rust
use sqlite3

// Result を返すヘルパに包む — `?` が最初のエラーを `run` の
// 呼び出し元まで短絡してくれるので、本体は match のネストなしで
// 上から下に読める。
fn run(): Result<i32, sqlite3.SqliteError> {
    let db = sqlite3.Database.open(":memory:")?
    db.exec("CREATE TABLE t (k INTEGER, v TEXT)")?

    let insert = db.prepare("INSERT INTO t VALUES (?, ?)")?
    insert.bindInt(1, 42)?
    insert.bindText(2, "answer")?
    insert.step()?

    let select = db.prepare("SELECT k, v FROM t ORDER BY k")?
    let going = true
    while going {
        match select.step()? {
            row {
                console.log("row:",
                    select.columnInt(0),
                    select.columnText(1))
            }
            done { going = false }
        }
    }
    Result.ok(0 as i32)
}

match run() {
    ok(_) { console.log("done") }
    err(e) { console.log("error:", e.message) }
}
```

## API

失敗しうる呼び出しはすべて `Result<T, SqliteError>` を返す。`SqliteError` は SQLite の生の結果コード `code: i32` と、`errmsg` 由来の `message: string` を保持。

### コネクション (`Database`)

- `Database.open(path: string): Result<Database, SqliteError>` — DB ファイルを開く。`":memory:"` でインメモリ、`""` で名前なしのオンディスク一時 DB
- `db.exec(sql: string): Result<bool, SqliteError>` — `;` で区切られた SQL を一括実行。行コールバックなし
- `db.prepare(sql: string): Result<Statement, SqliteError>` — 1 ステートメントをコンパイル。返ってきた `Statement` は ARC 上 `Database` から独立しているので、すべての `Statement` を finalize するまでは `Database` を生かしておくこと (SQLite が DB を閉じてくれない)
- `db.lastError(rc: i32): SqliteError` — 現在のコネクションの `errmsg` からエラーを作る
- `deinit` で `sqlite3_close` が自動で呼ばれる

### ステートメント (`Statement`)

- `stmt.bindInt(idx: i32, value: i32): Result<bool, SqliteError>`
- `stmt.bindInt64(idx: i32, value: i64): Result<bool, SqliteError>`
- `stmt.bindDouble(idx: i32, value: f64): Result<bool, SqliteError>`
- `stmt.bindText(idx: i32, value: string): Result<bool, SqliteError>`
- `stmt.bindNull(idx: i32): Result<bool, SqliteError>`
- `stmt.step(): Result<StepResult, SqliteError>` — 1 ステップ進める。`Result.ok(StepResult.row)` で行が読み出せる状態、`Result.ok(StepResult.done)` で完了
- `stmt.reset(): Result<bool, SqliteError>` — バインディングを差し替えての再実行のために巻き戻す
- `stmt.columnCount(): i32`
- `stmt.columnType(idx: i32): ColumnType` — `integer` / `float` / `text` / `blob` / `null_`
- `stmt.columnInt(idx: i32): i32`
- `stmt.columnInt64(idx: i32): i64`
- `stmt.columnDouble(idx: i32): f64`
- `stmt.columnText(idx: i32): string`
- `deinit` で `sqlite3_finalize` が自動で呼ばれる

バインドインデックスは **1 始まり** (SQL の `?N` に合わせる)、カラムインデックスは **0 始まり** (`sqlite3_column_*` に合わせる)。

### 生の C API 面

`sqlite3_open` / `sqlite3_exec` / `sqlite3_prepare_v2` などのオリジナル関数も `@extern(C)` ブロックから re-export しているので、ラッパを通さず直接呼びたい場合に使える。

## メモ / 既知の制限

- BLOB / バイナリパラメータのラッパは未対応 (現状は `int` / `int64` / `double` / `text` / `null` のみ)。`sqlite3_bind_blob` を `@extern(C)` の re-export 経由で直接呼ぶ手は使える
- `Statement` は `Database` への参照を保持しない。ステートメントは DB を閉じる前に finalize すること (または ARC のドロップ順に任せる — `Statement.deinit` が先に走る)
