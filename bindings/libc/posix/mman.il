/// POSIX `<sys/mman.h>` — memory mapping + locking.
///
/// `mmap` protection flags (PROT_*):
///   PROT_NONE   0
///   PROT_READ   1
///   PROT_WRITE  2
///   PROT_EXEC   4
///
/// `mmap` `flags` argument (MAP_*):
///   MAP_SHARED    0x01
///   MAP_PRIVATE   0x02
///   MAP_FIXED     0x10
///   MAP_ANONYMOUS 0x20  (alias MAP_ANON)
///
/// `MAP_FAILED` is `(void *)-1` — check `addr as i64 == -1` after
/// the call (NULL `0 as *void` is a valid mapping address).

@flags
pub enum MmapProt: i32 {
    none  = 0
    read  = 1
    write = 2
    exec  = 4
}

@flags
pub enum MmapFlags: i32 {
    shared    = 0x01
    private_  = 0x02
    fixed     = 0x10
    anonymous = 0x20
}

@extern(C, "c") {
    /// `mmap(addr, length, prot, flags, fd, offset)`. Pass
    /// `0 as *void` for `addr` to let the kernel pick.
    @lib @optional pub fn mmap(
        addr:   *void,
        length: size_t,
        prot:   i32,
        flags:  i32,
        fd:     i32,
        offset: i64
    ): *void

    @lib @optional pub fn munmap(addr: *void, length: size_t): i32
    @lib @optional pub fn mprotect(addr: *void, length: size_t, prot: i32): i32

    /// Flush dirty pages back to the backing file. `flags`:
    ///   MS_ASYNC      1
    ///   MS_INVALIDATE 2
    ///   MS_SYNC       4
    @lib @optional pub fn msync(addr: *void, length: size_t, flags: i32): i32

    /// Advise the kernel about expected access patterns. `advice`:
    /// 0 NORMAL, 1 RANDOM, 2 SEQUENTIAL, 3 WILLNEED, 4 DONTNEED.
    @lib @optional pub fn madvise(addr: *void, length: size_t, advice: i32): i32

    /// Lock pages into RAM (no swap-out).
    @lib @optional pub fn mlock(addr: *const void, length: size_t): i32
    @lib @optional pub fn munlock(addr: *const void, length: size_t): i32

    /// Lock the entire process address space.
    @lib @optional pub fn mlockall(flags: i32): i32
    @lib @optional pub fn munlockall(): i32

    /// Shared-memory object (POSIX shm_open / shm_unlink).
    @lib @optional pub fn shm_open(name: *const char, flags: i32, mode: u32): i32
    @lib @optional pub fn shm_unlink(name: *const char): i32
}
