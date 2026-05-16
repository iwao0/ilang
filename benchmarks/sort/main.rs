const N: usize = 2_000_000;

fn qsort(a: &mut [i64], lo: i64, hi: i64) {
    if lo >= hi { return; }
    let pivot = a[((lo + hi) / 2) as usize];
    let (mut i, mut j) = (lo, hi);
    while i <= j {
        while a[i as usize] < pivot { i += 1; }
        while a[j as usize] > pivot { j -= 1; }
        if i <= j {
            a.swap(i as usize, j as usize);
            i += 1;
            j -= 1;
        }
    }
    qsort(a, lo, j);
    qsort(a, i, hi);
}

fn main() {
    let mut a = vec![0i64; N];
    for i in 0..N {
        a[i] = ((i as u64).wrapping_mul(2654435761) % 1000003) as i64;
    }
    let last_ix = (N - 1) as i64;
    qsort(&mut a, 0, last_ix);
    println!("{} {}", a[0], a[N - 1]);
}
