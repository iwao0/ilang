// Call libc `abs()` 10_000_000 times. C inlines it but we route
// through a `volatile` to defeat constant folding — keeps the call
// observable like the other languages' FFI paths.
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#define N 10000000

int main(void) {
    volatile int64_t sum = 0;
    for (int64_t i = 0; i < N; i++) {
        sum += abs((int)(i - (N / 2)));
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
