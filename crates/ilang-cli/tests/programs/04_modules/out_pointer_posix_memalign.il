// Out-pointer pattern: a C function writes the resulting handle
// into a caller-provided slot, and the return value is the error
// code. We pass a 1-element `i64[]` as the slot, then cast the
// stored pointer to an opaque class.
//
// `posix_memalign(memptr, alignment, size)` — POSIX equivalent of
// aligned malloc. Writes the allocation pointer into *memptr and
// returns 0 on success or an errno code on failure.
// interp: skip
use test

@extern("c") class Buf {}
@extern("c") fn posix_memalign(memptr: i64[], alignment: u64, size: u64): i32
@extern("c") fn free(ptr: Buf)

let slot: i64[] = [0]
let rc = posix_memalign(slot, 64, 1024)
test.expect(rc as i64, 0)

// Slot now holds the raw `void *`. Cast turns it into a typed
// `Buf` handle so we can pass it to `free` (which the marshalling
// layer treats as an opaque-class arg, no further conversion).
let buf = slot[0] as Buf
free(buf)

// Going the other direction: opaque → i64 just exposes the raw
// pointer. Useful for diagnostics.
let slot2: i64[] = [0]
test.expect(posix_memalign(slot2, 8, 16) as i64, 0)
let buf2 = slot2[0] as Buf
let raw: i64 = buf2 as i64
test.expectTrue(raw != 0)
free(buf2)
