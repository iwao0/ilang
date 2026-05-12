const N = 200000;

function qsort(a, lo, hi) {
    if (lo >= hi) return;
    const pivot = a[(lo + hi) >> 1];
    let i = lo, j = hi;
    while (i <= j) {
        while (a[i] < pivot) i++;
        while (a[j] > pivot) j--;
        if (i <= j) {
            const t = a[i]; a[i] = a[j]; a[j] = t;
            i++;
            j--;
        }
    }
    qsort(a, lo, j);
    qsort(a, i, hi);
}

const a = new Array(N);
for (let i = 0; i < N; i++) {
    a[i] = (i * 2654435761) % 1000003;
}
qsort(a, 0, N - 1);
console.log(`${a[0]} ${a[N - 1]}`);
