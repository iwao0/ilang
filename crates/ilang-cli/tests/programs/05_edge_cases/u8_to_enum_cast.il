// `u8 as Enum` round-trips its bit pattern unchanged. Pre-fix
// the JIT used `sextend` so `255u8 as Code as u32` produced
// 4294967295 (the i32 bit pattern of -1) instead of 255.
// (codegen/lower_op.rs sextend → uextend for U8/Bool — ba3600d.)
use test

enum Code { a = 0, b = 1, c = 255 }

let x: u8 = 255u8
let e = x as Code
let n = e as u32
test.expect(n as i64, 255)
