// Map keys preserve their original integer width through `keys()`
// — pre-fix the eval `MapKey` collapsed all signed widths to i64
// and all unsigned to u64, so `Map<u8, V>.keys()[0]` came back
// as u64 and `+ 1u8` produced 256 instead of u8 wraparound.
// (eval/value.rs MapKey commit 889a7e4.)
use test

let m = new Map<u8, string>()
m.set(5u8, "five")
m.set(7u8, "seven")
let ks = m.keys()
test.expect(ks.length, 2)
let total: u8 = ks[0] + ks[1]
test.expect(total as i64, 12)
