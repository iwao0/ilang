// C side returns a `{ T* ptr; size_t len; }` 16 B struct, the
// caller deconstructs via `arrayFromCArray<T>(s.ptr, s.len)`.
// User code outside the @extern(C) block never sees a raw pointer
// — the value flowing through ilang is a normal heap-allocated
// `T[]`.
//
// jit: skip
use std.test

@extern(C) {
    struct ByteSlice {
        ptr: *const u8
        len: size_t
    }

    struct I32Slice {
        ptr: *const i32
        len: size_t
    }

    fn get_byte_slice(): ByteSlice
    fn get_i32_slice(): I32Slice
    fn maybe_byte_slice(ok: i32): ByteSlice

    fn collectBytes(): u8[] {
        let s = get_byte_slice()
        arrayFromCArray(s.ptr, s.len)
    }
    fn collectI32s(): i32[] {
        let s = get_i32_slice()
        arrayFromCArray(s.ptr, s.len)
    }
    fn collectMaybeBytes(ok: i32): u8[]? {
        let s = maybe_byte_slice(ok)
        if s.len == 0 as size_t {
            let empty: u8[]? = none
            empty
        } else {
            some(arrayFromCArray(s.ptr, s.len))
        }
    }
}

let bytes = collectBytes()
test.expect(bytes.length as i64, 5)
test.expect(bytes[0] as i64, 10)
test.expect(bytes[1] as i64, 20)
test.expect(bytes[2] as i64, 30)
test.expect(bytes[3] as i64, 40)
test.expect(bytes[4] as i64, 50)

let nums = collectI32s()
test.expect(nums.length as i64, 4)
test.expect(nums[0] as i64, -1)
test.expect(nums[1] as i64, 100)
test.expect(nums[2] as i64, 200)
test.expect(nums[3] as i64, 300)

// Calls are independent — copying means each call returns a fresh
// owned array; mutating one doesn't affect the other.
let nums2 = collectI32s()
nums2[0] = 999 as i32
test.expect(nums[0] as i64, -1)
test.expect(nums2[0] as i64, 999)

// Failure case: when the C side returns a (NULL, 0) slice, the
// helper returns None.
if let some(ok_arr) = collectMaybeBytes(1 as i32) {
    test.expect(ok_arr.length as i64, 5)
    test.expect(ok_arr[0] as i64, 10)
} else {
    test.fail("expected Some on ok=1")
}

if let some(err_arr) = collectMaybeBytes(0 as i32) {
    test.fail("expected None on ok=0")
    test.expect(err_arr.length as i64, -1)
}
