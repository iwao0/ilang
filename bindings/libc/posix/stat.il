/// POSIX `<sys/stat.h>` — file metadata.
///
/// `struct stat` layout varies between Linux (64-bit) and macOS,
/// and even between glibc / musl. The layout below matches Linux
/// glibc x86_64 (the most common). For exact cross-platform code,
/// inspect each field separately rather than memcpy'ing whole
/// structs.
///
/// File-type bits (`st_mode & S_IFMT`):
///   S_IFREG  0o100000  regular file
///   S_IFDIR  0o040000  directory
///   S_IFLNK  0o120000  symbolic link
///   S_IFCHR  0o020000  character device
///   S_IFBLK  0o060000  block device
///   S_IFIFO  0o010000  fifo / named pipe
///   S_IFSOCK 0o140000  socket
///   S_IFMT   0o170000  mask for the above
///
/// Permission bits:
///   S_IRWXU 0o700  S_IRUSR 0o400  S_IWUSR 0o200  S_IXUSR 0o100
///   S_IRWXG 0o070  S_IRGRP 0o040  S_IWGRP 0o020  S_IXGRP 0o010
///   S_IRWXO 0o007  S_IROTH 0o004  S_IWOTH 0o002  S_IXOTH 0o001
///   S_ISUID 0o4000 S_ISGID 0o2000 S_ISVTX 0o1000

@extern(C, "c") {
    /// Linux glibc x86_64 `struct stat` — 144 bytes. On macOS the
    /// equivalent struct is 144 bytes too but with reordered
    /// fields; this binding is portable for the field NAMES, not
    /// for whole-struct memcpy semantics across platforms.
    pub struct stat {
        st_dev:     u64
        st_ino:     u64
        st_nlink:   u64
        st_mode:    u32
        st_uid:     u32
        st_gid:     u32
        _pad0:      u32
        st_rdev:    u64
        st_size:    i64
        st_blksize: i64
        st_blocks:  i64
        st_atime:   i64
        st_atime_nsec: i64
        st_mtime:   i64
        st_mtime_nsec: i64
        st_ctime:   i64
        st_ctime_nsec: i64
        _unused:    u8[24]
    }

    @lib @optional pub fn stat(path: *const char, buf: *stat): i32
    @lib @optional pub fn fstat(fd: i32, buf: *stat): i32
    @lib @optional pub fn lstat(path: *const char, buf: *stat): i32

    /// `fstatat(dirfd, path, buf, flags)` — stat relative to a
    /// directory fd. Pass AT_FDCWD (-100 on Linux) for "current
    /// working directory".
    @lib @optional pub fn fstatat(
        dirfd: i32,
        path:  *const char,
        buf:   *stat,
        flags: i32
    ): i32

    @lib @optional pub fn mkdir(path: *const char, mode: u32): i32
    @lib @optional pub fn mkfifo(path: *const char, mode: u32): i32

    /// `umask(mask)` — set the process-wide creation-mode mask,
    /// returning the previous value.
    @lib @optional pub fn umask(mask: u32): u32

    @lib @optional pub fn chmod(path: *const char, mode: u32): i32
    @lib @optional pub fn fchmod(fd: i32, mode: u32): i32
}
