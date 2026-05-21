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

    pub fn __stat(path: string): i64[]
    pub fn __lstat(path: string): i64[]
    pub fn __access(path: string, mode: i32)
    pub fn __copyFile(src: string, dst: string, overwrite: bool)
    pub fn __cp(src: string, dst: string, recursive: bool)
    pub fn __realpath(path: string): string
    pub fn __chmod(path: string, mode: i32)
    pub fn __truncate(path: string, len: i64)
    pub fn __utimes(path: string, atimeMs: i64, mtimeMs: i64)
    pub fn __symlink(target: string, linkPath: string)
    pub fn __readlink(path: string): string
    pub fn __link(existing: string, newPath: string)
    pub fn __mkdtemp(prefix: string): string
}

/// `access(...)` mode flag: file exists. Bit-OR with the others
/// to test multiple permissions at once.
pub const F_OK: i32 = 0
/// `access(...)` mode flag: executable / searchable by the
/// process (POSIX only; treated as always allowed on Windows).
pub const X_OK: i32 = 1
/// `access(...)` mode flag: writable by the process. On Windows
/// only the read-only attribute is consulted.
pub const W_OK: i32 = 2
/// `access(...)` mode flag: readable by the process (POSIX only;
/// always allowed on Windows since ACLs don't map cleanly).
pub const R_OK: i32 = 4

/// Filesystem entry metadata returned by `stat` / `lstat`.
/// Mirrors the relevant subset of Node.js's `fs.Stats`: timestamps
/// are Unix-epoch milliseconds (`0` when the host doesn't supply
/// the value), and the entry kind is read via `isFile()` /
/// `isDir()` / `isSymlink()`.
pub class Stats {
    /// File size in bytes. For directories the value is
    /// platform-dependent (typically the directory's own block
    /// size, not the recursive content total).
    pub size: i64
    /// POSIX mode bits (permissions + type). On Windows a
    /// synthesised value: `0o444` when read-only, `0o644` otherwise.
    pub mode: i32
    /// Last modification time as Unix-epoch milliseconds.
    pub mtimeMs: i64
    /// Last access time as Unix-epoch milliseconds.
    pub atimeMs: i64
    /// Last inode-change time as Unix-epoch milliseconds (POSIX);
    /// on Windows this mirrors `birthtimeMs`.
    pub ctimeMs: i64
    /// File creation time as Unix-epoch milliseconds. `0` on
    /// platforms / filesystems that don't track it.
    pub birthtimeMs: i64
    /// Internal entry-kind discriminator: 1=file, 2=dir,
    /// 3=symlink, 0=other (FIFO, socket, ...). Use the predicate
    /// methods instead of comparing this directly.
    fileType: i32

    pub init(sz: i64, md: i32, mt: i64, at: i64, ct: i64, bt: i64, ft: i32) {
        this.size = sz
        this.mode = md
        this.mtimeMs = mt
        this.atimeMs = at
        this.ctimeMs = ct
        this.birthtimeMs = bt
        this.fileType = ft
    }

    /// `true` when the entry is a regular file.
    pub isFile(): bool { this.fileType == 1 }
    /// `true` when the entry is a directory.
    pub isDir(): bool { this.fileType == 2 }
    /// `true` when the entry is a symbolic link. Only set by
    /// `lstat` — `stat` follows links and reports the target's
    /// type.
    pub isSymlink(): bool { this.fileType == 3 }
}

fn buildStats(arr: i64[]): Stats {
    new Stats(arr[0], arr[1] as i32, arr[2], arr[3], arr[4], arr[5], arr[6] as i32)
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

/// Fetch a `Stats` snapshot for `path`. Symbolic links are
/// followed, so the returned values describe the link's *target*
/// (use `lstat` to inspect the link itself).
pub fn stat(path: string): Result<Stats, FsError> {
    let arr = __stat(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(buildStats(arr)) }
}

/// `stat` that does **not** follow symbolic links — if `path`
/// names a symlink, the returned `Stats` describes the link
/// itself (`isSymlink()` will be `true`).
pub fn lstat(path: string): Result<Stats, FsError> {
    let arr = __lstat(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(buildStats(arr)) }
}

/// Check whether the current process can access `path` under the
/// given mode (bit-OR of `F_OK` / `R_OK` / `W_OK` / `X_OK`). On
/// success returns `Result.ok(true)`; on denial returns
/// `Result.err(FsError)` with an `EACCES` / `ENOENT` code.
///
/// Windows note: R_OK / X_OK are not enforced (POSIX ACLs don't
/// map cleanly); only F_OK and W_OK (read-only attribute) are
/// honoured there.
pub fn access(path: string, mode: i32): Result<bool, FsError> {
    __access(path, mode)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Copy file `src` to `dst`. With `overwrite: false` fails with
/// `EEXIST` when `dst` already exists; with `overwrite: true`
/// replaces it. Symbolic links in `src` are followed (the link
/// target's contents are copied).
pub fn copyFile(src: string, dst: string, overwrite: bool): Result<bool, FsError> {
    __copyFile(src, dst, overwrite)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Copy `src` to `dst`. With `recursive: false` copies a single
/// file (use `copyFile` for finer control over overwriting). With
/// `recursive: true` walks `src` like `cp -r`, creating
/// directories at `dst` and preserving symlinks as symlinks.
pub fn cp(src: string, dst: string, recursive: bool): Result<bool, FsError> {
    __cp(src, dst, recursive)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Canonicalise `path` to an absolute path with all `.` / `..`
/// segments resolved and every symbolic link followed. Fails if
/// `path` (or any component) doesn't exist.
pub fn realpath(path: string): Result<string, FsError> {
    let r = __realpath(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(r) }
}

/// Change the permission bits of `path` to `mode`. On POSIX
/// `mode` is the standard `chmod(2)` bitmask (use the constants
/// in `os` such as `S_IRUSR`); on Windows only the user-write
/// bit (`0o200`) is honoured — clearing it sets the read-only
/// attribute.
pub fn chmod(path: string, mode: i32): Result<bool, FsError> {
    __chmod(path, mode)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Set the length of `path` to exactly `len` bytes. Truncates
/// when shrinking; pads with NUL bytes when growing. `len` must
/// be non-negative — pass `0` to empty the file in place.
pub fn truncate(path: string, len: i64): Result<bool, FsError> {
    __truncate(path, len)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Set the access and modification timestamps of `path`. Both
/// values are Unix-epoch milliseconds (the same units `stat()`
/// reports). Negative values are clamped at the platform's
/// minimum — practical use means "now or after 1970".
pub fn utimes(path: string, atimeMs: i64, mtimeMs: i64): Result<bool, FsError> {
    __utimes(path, atimeMs, mtimeMs)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Create a symbolic link at `linkPath` pointing to `target`.
/// `target` is stored verbatim and may be relative; it is not
/// required to exist. On Windows the link type (file vs.
/// directory) is chosen by inspecting `target` — for a missing
/// target a file symlink is created.
pub fn symlink(target: string, linkPath: string): Result<bool, FsError> {
    __symlink(target, linkPath)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Read the target of the symbolic link at `path`. The returned
/// string is whatever was stored in the link (may be relative).
/// Fails with `EINVAL` if `path` is not a symlink.
pub fn readlink(path: string): Result<string, FsError> {
    let r = __readlink(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(r) }
}

/// Create a hard link at `newPath` referencing the same inode as
/// `existing`. Both paths must be on the same filesystem.
pub fn link(existing: string, newPath: string): Result<bool, FsError> {
    __link(existing, newPath)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

/// Create a uniquely-named directory by appending six random
/// characters to `prefix`. The full directory path is returned.
/// The caller owns the new directory and is responsible for
/// removing it (e.g. via `rmdir(path, recursive: true)`).
pub fn mkdtemp(prefix: string): Result<string, FsError> {
    let p = __mkdtemp(prefix)
    if __hasError() { Result.err(lastError()) } else { Result.ok(p) }
}
