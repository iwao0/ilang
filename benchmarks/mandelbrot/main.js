const W = 1024, H = 1024, MAX_ITER = 1000;
let escaped = 0;
for (let py = 0; py < H; py++) {
    const y0 = py / H * 2.0 - 1.0;
    for (let px = 0; px < W; px++) {
        const x0 = px / W * 3.0 - 2.0;
        let x = 0.0, y = 0.0, it = 0;
        while (x * x + y * y <= 4.0 && it < MAX_ITER) {
            const xt = x * x - y * y + x0;
            y = 2.0 * x * y + y0;
            x = xt;
            it++;
        }
        if (it < MAX_ITER) escaped++;
    }
}
console.log(escaped);
