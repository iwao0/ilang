use std.ffi { cstrFromString, stringFromCstr }
/// SQLite 3 bindings for ilang.
///
/// ARC-managed `Database` / `Statement` classes wrapping `libsqlite3`.
/// Open with `Database.open(path)`, run statements via `db.exec(sql)`
/// or `db.prepare(sql)` + `stmt.bind*` + `stmt.step()` + `stmt.column*`.
///
/// Every fallible call returns `Result<T, SqliteError>`. Successful
/// step / done states are distinguished via `StepResult`. JIT-only —
/// the binding goes through `@extern(C) { @lib("sqlite3") fn ... }`
/// which needs dlsym.

// ── SQLite result-code subset we care about ────────────────────────

/// `sqlite3_step` outcome: a row is ready (`row`, SQLITE_ROW = 100)
/// or the statement has finished (`done`, SQLITE_DONE = 101).
/// Any other code is surfaced as `Result.err(SqliteError)`.
pub enum StepResult: i32 {
    row  = 100
    done = 101
}

/// `sqlite3_column_type` result. Mirrors the public SQLITE_*
/// constants (SQLITE_INTEGER = 1 … SQLITE_NULL = 5).
pub enum ColumnType: i32 {
    integer = 1
    float   = 2
    text    = 3
    blob    = 4
    null_   = 5
}

/// Error carrier returned from every fallible call. `code` is the
/// raw SQLite result code (`sqlite3_errcode`); `message` is the
/// human-readable description (`sqlite3_errmsg`).
pub class SqliteError {
    pub code: i32
    pub message: string

    pub init(code: i32, message: string) {
        this.code = code
        this.message = message
    }
}

@extern(C) {
    // Opaque handles. The C API works with pointers; we store
    // each handle as `i64` on the wrapper class side.
    pub struct sqlite3 {}
    pub struct sqlite3_stmt {}

    // ── Raw C entry points ────────────────────────────────────────
    @lib("sqlite3") pub fn sqlite3_open(filename: *const char, db: *i64): i32
    @lib("sqlite3") pub fn sqlite3_close(db: *sqlite3): i32

    @lib("sqlite3") pub fn sqlite3_errcode(db: *sqlite3): i32
    @lib("sqlite3") pub fn sqlite3_errmsg(db: *sqlite3): *const char

    @lib("sqlite3") pub fn sqlite3_exec(
        db: *sqlite3, sql: *const char,
        callback: *const void, ctx: *const void,
        errmsg: *const void
    ): i32

    @lib("sqlite3") pub fn sqlite3_prepare_v2(
        db: *sqlite3, sql: *const char, n_byte: i32,
        stmt: *i64, tail: *i64
    ): i32
    @lib("sqlite3") pub fn sqlite3_finalize(stmt: *sqlite3_stmt): i32
    @lib("sqlite3") pub fn sqlite3_step(stmt: *sqlite3_stmt): i32
    @lib("sqlite3") pub fn sqlite3_reset(stmt: *sqlite3_stmt): i32

    @lib("sqlite3") pub fn sqlite3_bind_int(stmt: *sqlite3_stmt, idx: i32, value: i32): i32
    @lib("sqlite3") pub fn sqlite3_bind_int64(stmt: *sqlite3_stmt, idx: i32, value: i64): i32
    @lib("sqlite3") pub fn sqlite3_bind_double(stmt: *sqlite3_stmt, idx: i32, value: f64): i32
    @lib("sqlite3") pub fn sqlite3_bind_text(
        stmt: *sqlite3_stmt, idx: i32,
        value: *const char, n: i32, dtor: *const void
    ): i32
    @lib("sqlite3") pub fn sqlite3_bind_null(stmt: *sqlite3_stmt, idx: i32): i32

    @lib("sqlite3") pub fn sqlite3_column_count(stmt: *sqlite3_stmt): i32
    @lib("sqlite3") pub fn sqlite3_column_type(stmt: *sqlite3_stmt, idx: i32): i32
    @lib("sqlite3") pub fn sqlite3_column_int(stmt: *sqlite3_stmt, idx: i32): i32
    @lib("sqlite3") pub fn sqlite3_column_int64(stmt: *sqlite3_stmt, idx: i32): i64
    @lib("sqlite3") pub fn sqlite3_column_double(stmt: *sqlite3_stmt, idx: i32): f64
    @lib("sqlite3") pub fn sqlite3_column_text(stmt: *sqlite3_stmt, idx: i32): *const char

    /// ARC-managed handle around a `sqlite3 *`. Always construct
    /// via `Database.open(path)` — the bare `init(i64)` is for
    /// the binding's own use.
    pub class Database {
        /// `sqlite3 *` cast to i64. `0` means the connection is
        /// already closed.
        pub handle: i64

        pub init(handle: i64) {
            this.handle = handle
        }

        deinit() {
            if this.handle != 0 {
                sqlite3_close(this.handle as *sqlite3)
                this.handle = 0
            }
        }

        /// Open a database file. Use `":memory:"` for an in-memory
        /// DB or `""` for a private on-disk temp DB. The path is
        /// interpreted relative to the process's current working
        /// directory.
        ///
        /// On failure, SQLite still allocates the handle so it can
        /// report the error; we close it before returning so the
        /// caller never sees a partly-initialised `Database`.
        pub static open(path: string): Result<Database, SqliteError> {
            let slot: i64[] = [0]
            let rc = sqlite3_open(cstrFromString(path), slot)
            let h = slot[0]
            if rc != 0 {
                let msg = if h != 0 {
                    stringFromCstr(sqlite3_errmsg(h as *sqlite3))
                } else {
                    "out of memory while opening database"
                }
                if h != 0 {
                    sqlite3_close(h as *sqlite3)
                }
                return Result.err(new SqliteError(rc, msg))
            }
            Result.ok(new Database(h))
        }

        /// Run one or more semicolon-separated SQL statements.
        /// `Result.ok(true)` on success; no row callback (use
        /// `prepare` for queries that return rows).
        pub exec(sql: string): Result<bool, SqliteError> {
            let rc = sqlite3_exec(
                this.handle as *sqlite3, cstrFromString(sql),
                0 as *const void, 0 as *const void, 0 as *const void
            )
            if rc != 0 { return Result.err(this.lastError(rc)) }
            Result.ok(true)
        }

        /// Compile a single SQL statement. The returned `Statement`
        /// is independent of this `Database` for ARC purposes —
        /// keep the `Database` alive yourself until every
        /// `Statement` is finalized, otherwise SQLite refuses to
        /// close the DB.
        pub prepare(sql: string): Result<Statement, SqliteError> {
            let stmt_slot: i64[] = [0]
            let tail_slot: i64[] = [0]
            let rc = sqlite3_prepare_v2(
                this.handle as *sqlite3, cstrFromString(sql), -1 as i32,
                stmt_slot, tail_slot
            )
            if rc != 0 { return Result.err(this.lastError(rc)) }
            Result.ok(new Statement(stmt_slot[0]))
        }

        /// Build a `SqliteError` from the connection's current
        /// error state. Caller passes the `rc` they observed; we
        /// pair it with the matching `errmsg` for a complete
        /// error.
        pub lastError(rc: i32): SqliteError {
            new SqliteError(
                rc,
                stringFromCstr(sqlite3_errmsg(this.handle as *sqlite3))
            )
        }
    }

    /// Compiled SQL statement. `deinit` calls `sqlite3_finalize`
    /// automatically. Reuse a statement across multiple parameter
    /// sets by calling `reset()` between runs.
    pub class Statement {
        /// `sqlite3_stmt *` cast to i64.
        pub handle: i64

        pub init(handle: i64) {
            this.handle = handle
        }

        deinit() {
            if this.handle != 0 {
                sqlite3_finalize(this.handle as *sqlite3_stmt)
                this.handle = 0
            }
        }

        /// Reset the statement so it can be re-executed (often
        /// with fresh bindings). Equivalent to `sqlite3_reset`.
        pub reset(): Result<bool, SqliteError> {
            let rc = sqlite3_reset(this.handle as *sqlite3_stmt)
            if rc != 0 { Result.err(new SqliteError(rc, "reset failed")) }
            else { Result.ok(true) }
        }

        /// Execute the statement one step. `Result.ok(StepResult.row)`
        /// means a row is ready — read columns via `columnInt` /
        /// `columnText` / etc. `Result.ok(StepResult.done)` means
        /// the statement has finished.
        pub step(): Result<StepResult, SqliteError> {
            let rc = sqlite3_step(this.handle as *sqlite3_stmt)
            if rc == 100 { Result.ok(StepResult.row) }
            elif rc == 101 { Result.ok(StepResult.done) }
            else { Result.err(new SqliteError(rc, "step failed")) }
        }

        // ── Bind parameters (1-based index — same as the SQL ?N) ──

        pub bindInt(idx: i32, value: i32): Result<bool, SqliteError> {
            bindResult(sqlite3_bind_int(this.handle as *sqlite3_stmt, idx, value))
        }
        pub bindInt64(idx: i32, value: i64): Result<bool, SqliteError> {
            bindResult(sqlite3_bind_int64(this.handle as *sqlite3_stmt, idx, value))
        }
        pub bindDouble(idx: i32, value: f64): Result<bool, SqliteError> {
            bindResult(sqlite3_bind_double(this.handle as *sqlite3_stmt, idx, value))
        }
        pub bindText(idx: i32, value: string): Result<bool, SqliteError> {
            // SQLITE_TRANSIENT = ((sqlite3_destructor_type)-1).
            // Inline the cast — exposing it through a helper fn
            // would require returning `*const void`, and
            // ilang-side wrappers may not have raw pointers in
            // their signature.
            let transient: i64 = -1
            bindResult(sqlite3_bind_text(
                this.handle as *sqlite3_stmt, idx,
                cstrFromString(value), -1 as i32,
                transient as *const void
            ))
        }
        pub bindNull(idx: i32): Result<bool, SqliteError> {
            bindResult(sqlite3_bind_null(this.handle as *sqlite3_stmt, idx))
        }

        // ── Column read (0-based index) ──────────────────────────

        pub columnCount(): i32 {
            sqlite3_column_count(this.handle as *sqlite3_stmt)
        }
        pub columnType(idx: i32): ColumnType {
            let t = sqlite3_column_type(this.handle as *sqlite3_stmt, idx)
            if t == 1 { ColumnType.integer }
            elif t == 2 { ColumnType.float }
            elif t == 3 { ColumnType.text }
            elif t == 4 { ColumnType.blob }
            else { ColumnType.null_ }
        }
        pub columnInt(idx: i32): i32 {
            sqlite3_column_int(this.handle as *sqlite3_stmt, idx)
        }
        pub columnInt64(idx: i32): i64 {
            sqlite3_column_int64(this.handle as *sqlite3_stmt, idx)
        }
        pub columnDouble(idx: i32): f64 {
            sqlite3_column_double(this.handle as *sqlite3_stmt, idx)
        }
        pub columnText(idx: i32): string {
            stringFromCstr(sqlite3_column_text(this.handle as *sqlite3_stmt, idx))
        }
    }
}

// Shared helper — wrap a 0 / non-0 bind result code into a Result.
fn bindResult(rc: i32): Result<bool, SqliteError> {
    if rc == 0 { Result.ok(true) }
    else { Result.err(new SqliteError(rc, "bind failed")) }
}
