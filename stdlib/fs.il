// Built-in `fs` module — sync filesystem helpers, Node.js-inspired.
//
// Every fallible call returns `Result<T, FsError>`. `Result.ok(v)`
// carries the success value; `Result.err(e)` carries an `FsError`
// whose `code` matches the platform errno name (`ENOENT`, `EACCES`,
// etc.) and whose `message` is the OS-level description.
//
// Predicates (`exists`, `isFile`, `isDir`) return plain `bool` —
// "not present" / "wrong type" isn't an error in their model, it's
// the answer.
//
// Relative paths resolve against the process's current working
// directory, the same as `open(2)` / Node.js's default `cwd()`.

pub class FsError {
    pub code: string
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

pub fn readFile(path: string): Result<string, FsError> {
    let s = __readFile(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(s) }
}

pub fn readFileBytes(path: string): Result<u8[], FsError> {
    let b = __readFileBytes(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(b) }
}

pub fn writeFile(path: string, contents: string): Result<bool, FsError> {
    __writeFile(path, contents)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn writeFileBytes(path: string, bytes: u8[]): Result<bool, FsError> {
    __writeFileBytes(path, bytes)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn appendFile(path: string, contents: string): Result<bool, FsError> {
    __appendFile(path, contents)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn exists(path: string): bool { __exists(path) }
pub fn isFile(path: string): bool { __isFile(path) }
pub fn isDir(path: string): bool { __isDir(path) }

pub fn mkdir(path: string, recursive: bool): Result<bool, FsError> {
    __mkdir(path, recursive)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn rm(path: string): Result<bool, FsError> {
    __rm(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn rmdir(path: string, recursive: bool): Result<bool, FsError> {
    __rmdir(path, recursive)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn rename(from: string, to: string): Result<bool, FsError> {
    __rename(from, to)
    if __hasError() { Result.err(lastError()) } else { Result.ok(true) }
}

pub fn readDir(path: string): Result<string[], FsError> {
    let entries = __readDir(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(entries) }
}

pub fn size(path: string): Result<i64, FsError> {
    let n = __size(path)
    if __hasError() { Result.err(lastError()) } else { Result.ok(n) }
}
