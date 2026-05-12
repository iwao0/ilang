const N: i64 = 200000

fn qsort(a: i64[], lo: i64, hi: i64) {
    if lo >= hi { return }
    let pivot: i64 = a[((lo + hi) / 2) as i64]
    let mut_i: i64 = lo
    let mut_j: i64 = hi
    while mut_i <= mut_j {
        while a[mut_i] < pivot { mut_i = mut_i + 1 }
        while a[mut_j] > pivot { mut_j = mut_j - 1 }
        if mut_i <= mut_j {
            let t: i64 = a[mut_i]
            a[mut_i] = a[mut_j]
            a[mut_j] = t
            mut_i = mut_i + 1
            mut_j = mut_j - 1
        }
    }
    qsort(a, lo, mut_j)
    qsort(a, mut_i, hi)
}

let a: i64[] = []
for i in 0..N {
    a.push((i * 2654435761) % 1000003)
}
qsort(a, 0, N - 1)
console.log(a[0], a[N - 1])
