/// POSIX additions to `<stdlib.h>`.

@extern(C, "c") {
    /// Aligned allocation. `alignment` must be a power of two
    /// and a multiple of `sizeof(void*)`. Free via `free`.
    /// Returns 0 on success and writes the new block to `*memptr`.
    @lib @optional pub fn posix_memalign(
        memptr:    *i64,
        alignment: size_t,
        size:      size_t
    ): i32

    /// C11 / POSIX aligned allocation. Returns NULL on failure.
    @lib @optional pub fn aligned_alloc(alignment: size_t, size: size_t): *void

    /// Create a unique temporary file: `template` ends with `XXXXXX`,
    /// which `mkstemp` replaces in-place. Returns an open fd or -1.
    @lib @optional pub fn mkstemp(template: *char): i32
    @lib @optional pub fn mkdtemp(template: *char): *char

    /// Resolve `path` against the filesystem. Pass NULL `resolved`
    /// to have the result `malloc`'d (free it yourself).
    @lib @optional pub fn realpath(path: *const char, resolved: *char): *char

    /// `putenv(s)` — `s` is `"NAME=value"`. The string is shared
    /// with the environment; do not free it after the call.
    /// (`setenv` / `unsetenv` already live in the libc `stdlib`
    /// module.)
    @lib @optional pub fn putenv(s: *char): i32

    /// 48-bit linear-congruential PRNG.
    @lib @optional pub fn drand48(): f64
    @lib @optional pub fn lrand48(): i64
    @lib @optional pub fn mrand48(): i64
    @lib @optional pub fn srand48(seed: i64)

    /// `random()` — better PRNG than `rand`.
    @lib @optional pub fn random(): i64
    @lib @optional pub fn srandom(seed: u32)

    /// `arc4random()` family — secure RNG (BSD / macOS / glibc 2.36+).
    @lib @optional pub fn arc4random(): u32
    @lib @optional pub fn arc4random_uniform(upper_bound: u32): u32
    @lib @optional pub fn arc4random_buf(buf: *void, nbytes: size_t)
}
