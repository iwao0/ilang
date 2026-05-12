// 1024 × 1024 mandelbrot with max-iter = 1000. Output a single i64
// checksum (count of escaped pixels) so the optimiser can't elide
// the loop.
#include <stdio.h>
#include <stdint.h>

#define W 1024
#define H 1024
#define MAX_ITER 1000

int main(void) {
    int64_t escaped = 0;
    for (int py = 0; py < H; py++) {
        double y0 = (double)py / H * 2.0 - 1.0;
        for (int px = 0; px < W; px++) {
            double x0 = (double)px / W * 3.0 - 2.0;
            double x = 0.0, y = 0.0;
            int it = 0;
            while (x * x + y * y <= 4.0 && it < MAX_ITER) {
                double xt = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                it++;
            }
            if (it < MAX_ITER) escaped++;
        }
    }
    printf("%lld\n", (long long)escaped);
    return 0;
}
