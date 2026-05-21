/// POSIX `<dirent.h>` — directory enumeration.
///
/// Pattern:
///   let dir = opendir("path") ; ARC: we own the *DIR until closedir
///   loop { let e = readdir(dir); if e == 0 as *dirent { break } ... }
///   closedir(dir)
///
/// `struct dirent` layout is implementation-defined past `d_name`;
/// reading only `d_name` (and optionally `d_ino` / `d_type`) is
/// portable. We expose a minimal prefix matching glibc + macOS.

@extern(C, "c") {
    /// Opaque DIR* — open directory stream.
    pub struct DIR {}

    /// Directory entry. The portable prefix is the first ~3 fields;
    /// `d_name` is a NUL-terminated path basename. On Linux + macOS
    /// the inline character array is 256 bytes; we model the whole
    /// thing as a fixed 280-byte struct that covers both layouts.
    pub struct dirent {
        d_ino:    u64
        d_off:    i64
        d_reclen: u16
        d_type:   u8
        d_name:   u8[256]
        _pad:     u8[5]
    }

    @lib @optional pub fn opendir(path: *const char): *DIR
    @lib @optional pub fn fdopendir(fd: i32): *DIR
    @lib @optional pub fn closedir(dirp: *DIR): i32

    /// Next entry, or NULL at end-of-stream / error. The returned
    /// pointer aliases internal storage and is invalidated by the
    /// next call.
    @lib @optional pub fn readdir(dirp: *DIR): *dirent

    /// Re-entrant variant — caller-supplied entry storage.
    @lib @optional pub fn readdir_r(
        dirp:   *DIR,
        entry:  *dirent,
        result: **dirent
    ): i32

    @lib @optional pub fn rewinddir(dirp: *DIR)
    @lib @optional pub fn seekdir(dirp: *DIR, loc: i64)
    @lib @optional pub fn telldir(dirp: *DIR): i64

    @lib @optional pub fn dirfd(dirp: *DIR): i32
}
