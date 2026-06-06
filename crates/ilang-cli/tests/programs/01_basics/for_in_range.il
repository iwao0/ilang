// Rust 風の `for i in a..b` (排他) と `for i in a..=b` (包含).
use std.test as test
let sum_excl = 0
for i in 1..5 {
    sum_excl = sum_excl + i
}
test.expect(sum_excl, 10)   // 1 + 2 + 3 + 4

let sum_incl = 0
for i in 1..=5 {
    sum_incl = sum_incl + i
}
test.expect(sum_incl, 15)   // 1 + 2 + 3 + 4 + 5

// 空レンジは0回。
let count = 0
for i in 5..5 {
    count = count + 1
}
test.expect(count, 0)

// 範囲式は変数も使える。
let a = 2
let b = 7
let s = 0
for i in a..b {
    s = s + i
}
test.expect(s, 20)   // 2+3+4+5+6

// break / continue が動く。
let collected = 0
for i in 0..100 {
    if i == 5 { break }
    if i % 2 == 0 { continue }
    collected = collected + i
}
test.expect(collected, 4)   // 1 + 3

// 範囲の境界は負の整数でもよい。
let neg_excl = 0
for i in -3..3 {
    neg_excl = neg_excl + i
}
test.expect(neg_excl, -3)   // -3 + -2 + -1 + 0 + 1 + 2

let neg_incl = 0
for i in -3..=3 {
    neg_incl = neg_incl + i
}
test.expect(neg_incl, 0)    // -3 + -2 + -1 + 0 + 1 + 2 + 3

let only_neg = 0
for i in -5..-1 {
    only_neg = only_neg + i
}
test.expect(only_neg, -14)  // -5 + -4 + -3 + -2
