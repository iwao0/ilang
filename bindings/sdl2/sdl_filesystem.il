// SDL2 filesystem helpers — `SDL_GetBasePath` and `SDL_GetPrefPath`
// resolve standard install / per-user save directories. Both return
// SDL-malloc'd strings; the wrappers `SDL_free` the originals so
// callers see plain ilang strings.

use sdl_core { SDL_free }

@extern(C, "SDL2") {
    @lib pub fn SDL_GetBasePath(): *const char
    @lib pub fn SDL_GetPrefPath(org: *const char, app: *const char): *const char

    /// Read-only directory the program was launched from. Trails a
    /// platform-specific separator (`"/"` on Unix, `"\\"` on
    /// Windows). On macOS this points inside the .app bundle.
    ///
    /// Returns:
    ///   absolute base path, or empty when the platform doesn't
    ///   support the lookup.
    pub fn getBasePath(): string {
        let raw = SDL_GetBasePath()
        if raw as i64 == 0 { return "" }
        let copy = stringFromCstr(raw)
        SDL_free(raw as *const void)
        copy
    }

    /// Per-user, per-app preferences directory — created if it
    /// doesn't already exist. Use this for save data, settings,
    /// log files. Resolves to:
    ///   - macOS:   `~/Library/Application Support/<org>/<app>/`
    ///   - Linux:   `$XDG_DATA_HOME/<org>/<app>/`
    ///   - Windows: `%APPDATA%\<org>\<app>\`
    ///
    /// Args:
    ///   org — organisation name (e.g. `"acme"`).
    ///   app — application name (e.g. `"breakout"`).
    ///
    /// Returns:
    ///   absolute pref path with a trailing separator. Empty on
    ///   error (call `sdl.getError()`).
    pub fn getPrefPath(org: string, app: string): string {
        let raw = SDL_GetPrefPath(cstrFromString(org), cstrFromString(app))
        if raw as i64 == 0 { return "" }
        let copy = stringFromCstr(raw)
        SDL_free(raw as *const void)
        copy
    }
}
