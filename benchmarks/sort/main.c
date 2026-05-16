// Quicksort on 2_000_000 i64 values seeded by a Fibonacci-hash PRNG.
// Output is the first + last element after sort as a sanity sum.
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#define N 2000000

static int64_t a[N];

static void qsort_i64(int64_t lo, int64_t hi) {
    if (lo >= hi) return;
    int64_t pivot = a[(lo + hi) / 2];
    int64_t i = lo, j = hi;
    while (i <= j) {
        while (a[i] < pivot) i++;
        while (a[j] > pivot) j--;
        if (i <= j) {
            int64_t t = a[i]; a[i] = a[j]; a[j] = t;
            i++; j--;
        }
    }
    qsort_i64(lo, j);
    qsort_i64(i, hi);
}

int main(void) {
    for (int64_t i = 0; i < N; i++) {
        a[i] = (int64_t)((uint64_t)i * 2654435761ULL % 1000003ULL);
    }
    qsort_i64(0, N - 1);
    printf("%lld %lld\n", (long long)a[0], (long long)a[N - 1]);
    return 0;
}
