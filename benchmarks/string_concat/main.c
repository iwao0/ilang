// Append a 1-byte string `N` times via realloc — the same O(n²)
// shape ilang's `s = s + "x"` produces. Print final length.
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define N 500000

int main(void) {
    char *s = malloc(1);
    size_t len = 0;
    s[0] = '\0';
    for (int64_t i = 0; i < N; i++) {
        char *t = malloc(len + 2);
        memcpy(t, s, len);
        t[len] = 'x';
        t[len + 1] = '\0';
        free(s);
        s = t;
        len++;
    }
    printf("%zu\n", len);
    free(s);
    return 0;
}
