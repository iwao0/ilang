// Out-pointer pattern: a C function writes the resulting handle
// into a caller-provided slot, and the return value is the error
// code. We declare the slot as `*Buf` (an opaque struct pointer)
// stored in a 1-element `i64[]` to give libc somewhere to write.
//
// `posix_memalign(memptr, alignment, size)` — POSIX equivalent of
// aligned malloc. Writes the allocation pointer into *memptr and
// returns 0 on success or an errno code on failure.
use test

@extern(C) {
    struct Buf {}
    @lib("c") fn posix_memalign(memptr: *i64, alignment: size_t, size: size_t): i32
    @lib("c") fn free(ptr: *Buf)

    // Convenience wrapper so the user never has to cast i64 → *Buf
    // outside the block (raw pointer types are not nameable there).
    fn freeRaw(p: i64) {
        free(p as *Buf)
    }

    // `@lib` calls only resolve through the extern-C codegen path,
    // so the actual `posix_memalign(...)` invocation has to live
    // inside an `@extern(C) { ... }` block. The wrapper takes a
    // regular `i64[]` (array-to-pointer coercion happens at the
    // C boundary inside this block).
    pub fn callPosixMemalign(slot: i64[], alignment: size_t, size: size_t): i32 {
        posix_memalign(slot, alignment, size)
    }
}

let slot: i64[] = [0]
let rc = callPosixMemalign(slot, 64 as u64, 1024 as u64)
test.expect(rc as i64, 0)

// Slot now holds the raw `void *`. The pointer can be passed to
// `free` (which takes `*Buf`); the i64 → *Buf coercion happens
// at the C-boundary.
let raw = slot[0]
test.expectTrue(raw != 0)
freeRaw(raw)
