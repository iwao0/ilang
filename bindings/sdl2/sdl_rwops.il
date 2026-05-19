// SDL2 RWops — abstract IO stream used by `SDL_LoadBMP_RW`,
// `SDL_LoadWAV_RW`, etc. We expose just enough to construct one
// from a file path / memory buffer and dispose of it. The
// individual `read` / `write` / `seek` callbacks aren't surfaced
// because real users of this binding always go through one of
// the loader fns that consume the RWops in one shot.

@extern(C, "SDL2") {
    /// Opaque SDL_RWops. Created via `RWops.fromFile(...)` /
    /// `RWops.fromMem(...)` and freed by the class's `deinit`
    /// (or by a loader that took ownership — see `release()`).
    pub struct SDL_RWops {}

    @lib pub fn SDL_RWFromFile(
        file: *const char, mode: *const char
    ): *SDL_RWops
    @lib pub fn SDL_RWFromConstMem(
        mem: *const void, size: i32
    ): *SDL_RWops
    @lib pub fn SDL_RWFromMem(
        mem: *const void, size: i32
    ): *SDL_RWops
    @lib pub fn SDL_RWclose(context: *SDL_RWops): i32

    /// ARC-managed `SDL_RWops`. Construct via the static factories
    /// (`RWops.fromFile` / `RWops.fromConstMem` / `RWops.fromMem`);
    /// `deinit` calls `SDL_RWclose` so the stream is freed when
    /// the last reference goes out of scope.
    ///
    /// When handing the raw handle to a loader that takes
    /// ownership (e.g. `SDL_LoadBMP_RW(rw, /*freesrc=*/1)`), call
    /// `release()` first so the loader's `freesrc=1` close isn't
    /// double-freed by `deinit` afterwards.
    pub class RWops {
        /// Raw `*SDL_RWops` cast to i64. `0` if construction
        /// failed or after `release()`.
        pub handle: i64

        /// Open a file as an RWops. Wraps `fopen` on most
        /// platforms.
        ///
        /// Args:
        ///   path — filesystem path.
        ///   mode — fopen-style mode (`"rb"`, `"wb"`, `"r+b"`, …).
        ///
        /// Returns:
        ///   an `RWops` whose `handle` is `0` on failure (call
        ///   `sdl.getError()` for the message).
        pub static fromFile(path: string, mode: string): RWops {
            RWops.fromHandle(SDL_RWFromFile(
                cstrFromString(path), cstrFromString(mode)
            ) as i64)
        }

        /// Wrap a read-only byte buffer as an RWops. The buffer
        /// must outlive the RWops — SDL doesn't copy it.
        pub static fromConstMem(bytes: u8[]): RWops {
            let p: *u8 = bytes
            RWops.fromHandle(SDL_RWFromConstMem(
                p as *const void, bytes.length as i32
            ) as i64)
        }

        /// Wrap a read/write byte buffer as an RWops. Same
        /// lifetime rule as `fromConstMem`.
        pub static fromMem(bytes: u8[]): RWops {
            let p: *u8 = bytes
            RWops.fromHandle(SDL_RWFromMem(
                p as *const void, bytes.length as i32
            ) as i64)
        }

        /// Wrap an existing `*SDL_RWops` (cast to i64). Takes
        /// ownership: `deinit` will close it.
        pub static fromHandle(handle: i64): RWops {
            let r = new RWops()
            r.handle = handle
            r
        }

        deinit() {
            if this.handle != 0 {
                SDL_RWclose(this.handle as *SDL_RWops)
            }
        }

        /// Close the stream now, before this `RWops` goes out of
        /// scope. Subsequent reads / `deinit` are no-ops.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on error.
        pub close(): i32 {
            if this.handle == 0 { return 0 }
            let rc = SDL_RWclose(this.handle as *SDL_RWops)
            this.handle = 0
            rc
        }

        /// Give up ownership of the handle without closing. Use
        /// before passing the raw `handle` to a loader that takes
        /// `freesrc != 0` (`SDL_LoadBMP_RW`, `SDL_LoadWAV_RW`, …)
        /// — the loader will close it; calling `deinit` on a
        /// detached `RWops` is a no-op.
        ///
        /// Returns:
        ///   the previous handle (or `0` if already detached).
        pub release(): i64 {
            let h = this.handle
            this.handle = 0
            h
        }
    }
}
