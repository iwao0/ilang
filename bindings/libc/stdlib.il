/// C standard `<stdlib.h>` — memory + integer / process helpers.

@extern(C) {
    /// Allocate `size` bytes. Returns NULL on failure. Free with `free`.
    @lib("c") pub fn malloc(size: size_t): *void

    /// Allocate `nmemb * size` bytes initialised to zero.
    @lib("c") pub fn calloc(nmemb: size_t, size: size_t): *void

    /// Resize the block at `ptr` to `size` bytes. May return a
    /// different pointer; the caller must replace the original.
    @lib("c") pub fn realloc(ptr: *void, size: size_t): *void

    /// Release a block previously returned by malloc / calloc /
    /// realloc / strdup. Passing NULL is defined and a no-op.
    @lib("c") pub fn free(ptr: *void)

    /// Absolute value of an `int`. Defined for non-INT_MIN inputs;
    /// `abs(INT_MIN)` is UB by spec.
    @lib("c") pub fn abs(x: i32): i32

    /// Terminate the process with `status` as the exit code.
    /// Runs `atexit` handlers, flushes / closes stdio streams.
    @lib("c") pub fn exit(status: i32)

    /// Abrupt termination — no atexit / no stdio flush. Raises
    /// SIGABRT on POSIX; on Windows it pops the crash dialog.
    @lib("c") pub fn abort()
}
