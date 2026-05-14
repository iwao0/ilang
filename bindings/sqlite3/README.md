# SQLite3 bindings for ilang

[日本語](./README_ja.md)

ARC-managed `Database` / `Statement` wrappers around `libsqlite3`.
JIT-only — the binding goes through `@extern(C) { @lib("sqlite3") }`
which needs dlsym.

## Modules

| File | Provides |
| --- | --- |
| `sqlite3.il` | `Database` / `Statement` classes, `SqliteError`, `StepResult`, `ColumnType`, and all `sqlite3_*` C entry points |

## Using these bindings from your project

Create an `ilang.toml` next to your entry file:

```toml
[package]
name = "my_app"

[deps]
sqlite3 = "/absolute/or/relative/path/to/bindings/sqlite3"
```

Then:

```rust
use sqlite3

match sqlite3.Database.open(":memory:") {
    ok(db) {
        match db.exec("CREATE TABLE t (k INTEGER, v TEXT)") {
            ok(_) { console.log("created") }
            err(e) { console.log("create failed:", e.message) }
        }

        match db.prepare("INSERT INTO t VALUES (?, ?)") {
            ok(stmt) {
                let _ = stmt.bindInt(1, 42)
                let _ = stmt.bindText(2, "answer")
                let _ = stmt.step()
            }
            err(e) { console.log("prep failed:", e.message) }
        }

        match db.prepare("SELECT k, v FROM t ORDER BY k") {
            ok(stmt) {
                let going = true
                while going {
                    match stmt.step() {
                        ok(state) {
                            match state {
                                row {
                                    console.log("row:",
                                        stmt.columnInt(0),
                                        stmt.columnText(1))
                                }
                                done { going = false }
                            }
                        }
                        err(e) {
                            console.log("step error:", e.message)
                            going = false
                        }
                    }
                }
            }
            err(e) { console.log("query failed:", e.message) }
        }
    }
    err(e) { console.log("open failed:", e.message) }
}
```

## API surface

Every fallible call returns `Result<T, SqliteError>`. `SqliteError`
carries the raw SQLite result `code: i32` and the `errmsg`-derived
`message: string`.

### Connection (`Database`)

- `Database.open(path: string): Result<Database, SqliteError>`
  Open a database file. `":memory:"` for an in-memory DB, `""`
  for an unnamed on-disk temp DB.
- `db.exec(sql: string): Result<bool, SqliteError>` — run one or
  more `;`-separated statements. No row callback.
- `db.prepare(sql: string): Result<Statement, SqliteError>` —
  compile a single statement. The returned `Statement` is
  independent of the `Database` for ARC purposes — keep the
  `Database` alive yourself until every `Statement` is finalized.
- `db.lastError(rc: i32): SqliteError` — build an error from the
  connection's current `errmsg` state.
- `deinit` calls `sqlite3_close` automatically.

### Statement (`Statement`)

- `stmt.bindInt(idx: i32, value: i32): Result<bool, SqliteError>`
- `stmt.bindInt64(idx: i32, value: i64): Result<bool, SqliteError>`
- `stmt.bindDouble(idx: i32, value: f64): Result<bool, SqliteError>`
- `stmt.bindText(idx: i32, value: string): Result<bool, SqliteError>`
- `stmt.bindNull(idx: i32): Result<bool, SqliteError>`
- `stmt.step(): Result<StepResult, SqliteError>` — advance the
  statement. `Result.ok(StepResult.row)` = a row is ready;
  `Result.ok(StepResult.done)` = finished.
- `stmt.reset(): Result<bool, SqliteError>` — rewind for re-execution
  (often with fresh bindings).
- `stmt.columnCount(): i32`
- `stmt.columnType(idx: i32): ColumnType` — `integer` / `float` /
  `text` / `blob` / `null_`.
- `stmt.columnInt(idx: i32): i32`
- `stmt.columnInt64(idx: i32): i64`
- `stmt.columnDouble(idx: i32): f64`
- `stmt.columnText(idx: i32): string`
- `deinit` calls `sqlite3_finalize` automatically.

Bind indices are **1-based** (matching SQL `?N`); column indices
are **0-based** (matching `sqlite3_column_*`).

### Raw C surface

`sqlite3_open`, `sqlite3_exec`, `sqlite3_prepare_v2`, etc. are also
re-exported from the `@extern(C)` block for callers who need to
reach past the wrapper.

## Notes / known limitations

- BLOB and binary parameter support is not yet wrapped (only
  `int` / `int64` / `double` / `text` / `null`). Calling
  `sqlite3_bind_blob` directly from the `@extern(C)` re-exports
  works.
- The binding does not retain a reference from `Statement` to its
  `Database`. Finalize statements before closing the connection
  (or let the ARC drop order handle it; `deinit` calls finalize on
  every `Statement` first).
