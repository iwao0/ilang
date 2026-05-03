// Rust 風の `for i in a..b` (排他) と `for i in a..=b` (包含).
use test

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
