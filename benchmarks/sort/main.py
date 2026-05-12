import sys
sys.setrecursionlimit(1 << 20)

N = 200_000

def qsort(a, lo, hi):
    if lo >= hi:
        return
    pivot = a[(lo + hi) // 2]
    i, j = lo, hi
    while i <= j:
        while a[i] < pivot:
            i += 1
        while a[j] > pivot:
            j -= 1
        if i <= j:
            a[i], a[j] = a[j], a[i]
            i += 1
            j -= 1
    qsort(a, lo, j)
    qsort(a, i, hi)

a = [(i * 2654435761) % 1000003 for i in range(N)]
qsort(a, 0, N - 1)
print(a[0], a[N - 1])
