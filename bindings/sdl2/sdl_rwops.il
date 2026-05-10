// SDL2 RWops — abstract IO stream used by `SDL_LoadBMP_RW`,
// `SDL_LoadWAV_RW`, etc. We expose just enough to construct one
// from a file path / memory buffer and dispose of it. The
// individual `read` / `write` / `seek` callbacks aren't surfaced
// because real users of this binding always go through one of the
// loader fns that consume the RWops in one shot.

@extern(C) {
    /// Opaque SDL_RWops. Created via `RWFromFile` / `RWFromMem` and
    /// destroyed by `freeRW` (or by a loader that takes ownership).
    pub struct SDL_RWops {}

    @lib("SDL2") pub fn SDL_RWFromFile(
        file: *const char, mode: *const char
    ): *SDL_RWops
    @lib("SDL2") pub fn SDL_RWFromConstMem(
        mem: *const void, size: i32
    ): *SDL_RWops
    @lib("SDL2") pub fn SDL_RWFromMem(mem: *const void, size: i32): *SDL_RWops
    @lib("SDL2") pub fn SDL_RWclose(context: *SDL_RWops): i32

    /// Open a file as an RWops. Wraps `fopen` on most platforms;
    /// pass `freesrc = 1` to a loader for it to close the stream
    /// after consuming it.
    ///
    /// Args:
    ///   path — filesystem path.
    ///   mode — fopen-style mode (`"rb"`, `"wb"`, `"r+b"`, …).
    ///
    /// Returns:
    ///   raw `*SDL_RWops` cast to `i64`, or `0` on failure (call
    ///   `sdl.getError()`).
    pub fn rwFromFile(path: string, mode: string): i64 {
        SDL_RWFromFile(cstrFromString(path), cstrFromString(mode)) as i64
    }

    /// Wrap a read-only byte buffer as an RWops. The buffer must
    /// outlive the RWops — SDL doesn't copy it.
    ///
    /// Args:
    ///   bytes — the source bytes.
    ///
    /// Returns:
    ///   raw `*SDL_RWops` cast to `i64`, or `0` on failure.
    pub fn rwFromConstMem(bytes: u8[]): i64 {
        let p: *u8 = bytes
        SDL_RWFromConstMem(p as *const void, bytes.length as i32) as i64
    }

    /// Wrap a read/write byte buffer as an RWops. Same lifetime
    /// rule as `rwFromConstMem`.
    pub fn rwFromMem(bytes: u8[]): i64 {
        let p: *u8 = bytes
        SDL_RWFromMem(p as *const void, bytes.length as i32) as i64
    }

    /// Close + free an RWops obtained from one of the `rwFrom*`
    /// helpers. Loaders that take `freesrc != 0` already close on
    /// the caller's behalf — don't double-free.
    ///
    /// Args:
    ///   handle — pointer cast to i64 (the value returned by
    ///            `rwFromFile` / `rwFromMem`).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on error.
    pub fn rwClose(handle: i64): i32 {
        if handle == 0 { return 0 }
        SDL_RWclose(handle as *SDL_RWops)
    }
}
