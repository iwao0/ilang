// `.hashCode(): i64` is available on every numeric primitive and
// `bool`. Ints widen by their declared signedness; bool yields
// 0 / 1; floats bit-cast so distinct NaN payloads stay distinct
// (matches `Set<f64>`'s existing semantics).
use std.test as test

// ---- integer widths: signed widen with sextend, unsigned with uextend
test.expect(0i8.hashCode(), 0)
test.expect(127i8.hashCode(), 127)
test.expect((-1i8).hashCode(), -1)              // sextend
test.expect((-128i8).hashCode(), -128)

test.expect(0u8.hashCode(), 0)
test.expect(255u8.hashCode(), 255)             // zero-extend

test.expect(0i16.hashCode(), 0)
test.expect((-1i16).hashCode(), -1)
test.expect(65535u16.hashCode(), 65535)

test.expect(0i32.hashCode(), 0)
test.expect((-1i32).hashCode(), -1)
test.expect(4294967295u32.hashCode(), 4294967295)

test.expect(0i64.hashCode(), 0)
test.expect((-1i64).hashCode(), -1)
test.expect(9223372036854775807i64.hashCode(), 9223372036854775807)

test.expect(0u64.hashCode(), 0)
test.expect(18446744073709551615u64.hashCode(), -1)   // bit pattern wraps to -1 as i64

// ---- bool: false -> 0, true -> 1
test.expect(false.hashCode(), 0)
test.expect(true.hashCode(), 1)

// ---- floats: bit-cast so equal floats hash equal, different bits hash differently
test.expectBool(1.5f32.hashCode() == 1.5f32.hashCode(), true)
test.expectBool(1.5f32.hashCode() != 2.5f32.hashCode(), true)
test.expectBool(0.0f32.hashCode() != (-0.0f32).hashCode(), true)   // bit patterns differ

test.expectBool(1.5.hashCode() == 1.5.hashCode(), true)
test.expectBool(1.5.hashCode() != 2.5.hashCode(), true)
test.expectBool(0.0.hashCode() != (-0.0).hashCode(), true)
