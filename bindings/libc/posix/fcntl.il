/// POSIX `<fcntl.h>` — `open` flag constants + `fcntl(fd, cmd, ...)`.
///
/// `open` / `openat` flag values (`flags` argument). The lower
/// two bits are an access mode (RDONLY / WRONLY / RDWR); everything
/// else is OR-able:
///   O_RDONLY    0x0000  O_WRONLY  0x0001  O_RDWR    0x0002
///   O_CREAT     0x0040  O_EXCL    0x0080  O_NOCTTY  0x0100
///   O_TRUNC     0x0200  O_APPEND  0x0400  O_NONBLOCK 0x0800
///   O_DSYNC     0x1000  O_DIRECT  0x4000  O_DIRECTORY 0x10000
///   O_NOFOLLOW  0x20000 O_CLOEXEC 0x80000 O_PATH     0x200000
///   O_TMPFILE   0x410000
/// (Linux values; macOS uses different bits — consult its
/// `<fcntl.h>` when targetting macOS-specific flags.)
///
/// `fcntl` commands:
///   F_DUPFD       0   F_GETFD     1   F_SETFD     2
///   F_GETFL       3   F_SETFL     4
///   F_GETLK       5   F_SETLK     6   F_SETLKW    7
///   F_DUPFD_CLOEXEC 1030

@flags
pub enum OpenFlags: i32 {
    rdonly   = 0x0000
    wronly   = 0x0001
    rdwr     = 0x0002
    create   = 0x0040
    excl     = 0x0080
    noctty   = 0x0100
    trunc    = 0x0200
    append   = 0x0400
    nonblock = 0x0800
    dsync    = 0x1000
    direct   = 0x4000
    directory = 0x10000
    nofollow = 0x20000
    cloexec  = 0x80000
    path     = 0x200000
    tmpfile  = 0x410000
}

@extern(C, "c") {
    /// `fcntl(fd, cmd, arg)` — file-descriptor control. `arg` is
    /// command-specific; pass `0 as *void` when unused.
    @lib @optional pub fn fcntl(fd: i32, cmd: i32, arg: *void): i32

    /// `openat(dirfd, path, flags, mode)` — open relative to a dir
    /// fd. Pass AT_FDCWD (-100 on Linux) for "current dir".
    @lib @optional pub fn openat(
        dirfd: i32,
        path:  *const char,
        flags: i32,
        mode:  u32
    ): i32

    /// `creat(path, mode)` — equivalent to
    /// `open(path, O_WRONLY|O_CREAT|O_TRUNC, mode)`. Legacy.
    @lib @optional pub fn creat(path: *const char, mode: u32): i32

    /// `posix_fadvise` — hint the kernel about access patterns.
    /// Advice codes: 0 NORMAL, 1 RANDOM, 2 SEQUENTIAL, 3 WILLNEED,
    /// 4 DONTNEED, 5 NOREUSE.
    @lib @optional pub fn posix_fadvise(
        fd:     i32,
        offset: i64,
        len:    i64,
        advice: i32
    ): i32

    /// Reserve disk space for a file without writing it.
    @lib @optional pub fn posix_fallocate(fd: i32, offset: i64, len: i64): i32
}
