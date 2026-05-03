// `slice_return` flag: the C side returns a `{ T* ptr; size_t len; }`
// 16 B struct, the JIT receives the (ptr, len) pair in 2 GPRs,
// allocates a fresh ilang `T[]` of length `len`, and memcpys the
// bytes into it. User code never sees a raw pointer — the value
// flowing through ilang is a normal heap-allocated array.
//
// interp: skip
use test

@extern(slice_return) fn get_byte_slice(): u8[]
@extern(slice_return) fn get_i32_slice(): i32[]

let bytes = get_byte_slice()
test.expect(bytes.length as i64, 5)
test.expect(bytes[0] as i64, 10)
test.expect(bytes[1] as i64, 20)
test.expect(bytes[2] as i64, 30)
test.expect(bytes[3] as i64, 40)
test.expect(bytes[4] as i64, 50)

let nums = get_i32_slice()
test.expect(nums.length as i64, 4)
test.expect(nums[0] as i64, -1)
test.expect(nums[1] as i64, 100)
test.expect(nums[2] as i64, 200)
test.expect(nums[3] as i64, 300)

// Calls are independent — copying means each call returns a fresh
// owned array; mutating one doesn't affect the other.
let nums2 = get_i32_slice()
nums2[0] = 999 as i32
test.expect(nums[0] as i64, -1)
test.expect(nums2[0] as i64, 999)

// Failure case: when the C side returns a (NULL, 0) slice, the
// declared `T[]?` return becomes `None` — no segfault on the
// would-be memcpy.
@extern(slice_return) fn maybe_byte_slice(ok: i32): u8[]?

if let some(ok_arr) = maybe_byte_slice(1 as i32) {
    test.expect(ok_arr.length as i64, 5)
    test.expect(ok_arr[0] as i64, 10)
} else {
    test.fail("expected Some on ok=1")
}

if let some(err_arr) = maybe_byte_slice(0 as i32) {
    test.fail("expected None on ok=0")
    // Touch a field so the compiler doesn't drop the binding.
    test.expect(err_arr.length as i64, -1)
}
