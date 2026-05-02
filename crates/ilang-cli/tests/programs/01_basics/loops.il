// while / loop with break / continue, including nested loops.
use test

// while: count to 10.
let i = 0
let sum = 0
while i < 10 {
    sum += i
    i += 1
}
test.expect(sum, 45)
test.expect(i, 10)

// while with continue: skip even numbers.
let j = 0
let oddsum = 0
while j < 10 {
    j += 1
    if j % 2 == 0 { continue }
    oddsum += j
}
test.expect(oddsum, 1 + 3 + 5 + 7 + 9)

// while with break: stop when threshold hit.
let k = 0
loop {
    if k >= 5 { break }
    k += 1
}
test.expect(k, 5)

// loop + continue.
let m = 0
let collected = 0
loop {
    if m >= 6 { break }
    m += 1
    if m % 2 == 0 { continue }
    collected += 1
}
test.expect(collected, 3)   // 1, 3, 5

// Nested loops: break only the inner.
let pairs = 0
let a = 0
while a < 3 {
    let b = 0
    while b < 3 {
        if b == 2 { break }
        pairs += 1
        b += 1
    }
    a += 1
}
test.expect(pairs, 6)        // 3 outer * 2 inner

// `loop` body's tail expression is allowed but discarded.
let counter = 0
loop {
    counter += 1
    if counter >= 3 { break }
}
test.expect(counter, 3)
