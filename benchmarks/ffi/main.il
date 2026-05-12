@extern(C) {
    pub fn abs(x: i32): i32
}

const N: i64 = 10000000
let mut_sum: i64 = 0
for i in 0..N {
    mut_sum = mut_sum + (abs((i - N / 2) as i32) as i64)
}
console.log(mut_sum)
