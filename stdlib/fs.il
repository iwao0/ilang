/// Built-in `fs` module — sync filesystem helpers, Node.js-inspired.
///
/// Every fallible call returns `Result<T, FsError>`. `Result.ok(v)`
/// carries the success value; `Result.err(e)` carries an `FsError`
/// whose `code` matches the platform errno name (`ENOENT`, `EACCES`,
/// etc.) and whose `message` is the OS-level description.
///
/// Predicates (`exists`, `isFile`, `isDir`) return plain `bool` —
/// "not present" / "wrong type" isn't an error in their model, it's
/// the answer.
///
/// Relative paths resolve against the process's current working
/// directory, the same as `open(2)` / Node.js's default `cwd()`.

/// Failure payload returned inside `Result.err(...)` by every
/// fallible `fs` call. `code` is the platform errno name
/// (`"ENOENT"`, `"EACCES"`, ...); `message` is the OS-level
/// description.
pub class FsError {
    /// Platform errno name (`"ENOENT"`, `"EACCES"`, ...).
    pub code: string
    /// OS-level human-readable description of the failure.
    pub message: string

    init(code: string, message: string) {
        this.code = code
        this.message = message
    }
}

@extern(C) {
    pub fn __hasError(): bool
    pub fn __errorCode(): string
    pub fn __errorMessage(): string

    pub fn __readFile(path: string): string
    pub fn __readFileBytes(path: string): u8[]
    pub fn __writeFile(path: string, contents: string)
    pub fn __writeFileBytes(path: string, bytes: u8[])
    pub fn __appendFile(path: string, contents: string)

    pub fn __exists(path: string): bool
    pub fn __isFile(path: string): bool
    pub fn __isDir(path: string): bool

    pub fn __mkdir(path: string, recursive: bool)
    pub fn __rm(path: string)
    pub fn __rmdir(path: string, recursive: bool)
    pub fn __rename(from: string, to: string)

    pub fn __readDir(path: string): string[]
    pub fn __size(path: string): i64
}

fn lastError(): FsError {
    new FsError(__errorCode(), __errorMessage())
}

/// Read the entire file at `path` as a UTF-8 string. Returns
/// `Result.err(FsError)` if the file is missing, unreadable, or
/// contains invalid UTF-8.
pub fn readFile(path: string): Result<string, FsError> {
    let s = __readFile(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(s) }
}

/// Read the entire file at `path` as a raw byte array. Use this
/// instead of `readFile` when the contents aren't valid UTF-8 or
/// when you need to preserve bytes exactly.
pub fn readFileBytes(path: string): Result<u8[], FsError> {
    let b = __readFileBytes(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(b) }
}

/// Write `contents` to `path`, replacing any existing file.
/// Creates the file if it doesn't exist; parent directories must
/// already exist (use `mkdir(..., recursive: true)` first).
pub fn writeFile(path: string, contents: string): Result<bool, FsError> {
    __writeFile(path, contents)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Write raw `bytes` to `path`, replacing any existing file.
/// Same semantics as `writeFile` but takes a `u8[]` payload.
pub fn writeFileBytes(path: string, bytes: u8[]): Result<bool, FsError> {
    __writeFileBytes(path, bytes)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Append `contents` to `path`. Creates the file if it doesn't
/// exist; otherwise leaves existing data intact and writes at the
/// end.
pub fn appendFile(path: string, contents: string): Result<bool, FsError> {
    __appendFile(path, contents)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// `true` if `path` refers to an existing filesystem entry of any
/// kind (file, directory, symlink target, ...). Returns `false`
/// for missing paths *and* permission errors — use `readFile`
/// itself if you need to distinguish the two.
pub fn exists(path: string): bool { __exists(path) }

/// `true` if `path` exists and is a regular file. `false` for
/// directories, missing paths, or anything the process can't
/// stat.
pub fn isFile(path: string): bool { __isFile(path) }

/// `true` if `path` exists and is a directory. `false` for
/// regular files, missing paths, or anything the process can't
/// stat.
pub fn isDir(path: string): bool { __isDir(path) }

/// Create directory `path`. With `recursive: true` behaves like
/// `mkdir -p` (creates missing parents, succeeds when the
/// directory already exists). With `recursive: false` fails with
/// `EEXIST` if `path` exists.
pub fn mkdir(path: string, recursive: bool): Result<bool, FsError> {
    __mkdir(path, recursive)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Remove the file at `path`. Use `rmdir` for directories.
pub fn rm(path: string): Result<bool, FsError> {
    __rm(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Remove the directory at `path`. With `recursive: true` removes
/// the entire subtree (`rm -rf`); with `recursive: false` fails
/// if the directory is non-empty.
pub fn rmdir(path: string, recursive: bool): Result<bool, FsError> {
    __rmdir(path, recursive)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Move / rename `from` to `to`. Atomic when both paths are on
/// the same filesystem; falls back to copy-then-delete otherwise.
pub fn rename(from: string, to: string): Result<bool, FsError> {
    __rename(from, to)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// List the entries of directory `path`. Returns just the entry
/// names (not full paths), excluding `.` and `..`.
pub fn readDir(path: string): Result<string[], FsError> {
    let entries = __readDir(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(entries) }
}

/// Size of `path` in bytes. Reports the file size for regular
/// files; for directories the value is platform-dependent.
pub fn size(path: string): Result<i64, FsError> {
    let n = __size(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(n) }
}
