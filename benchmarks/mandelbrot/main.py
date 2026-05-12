W, H, MAX_ITER = 1024, 1024, 1000
escaped = 0
for py in range(H):
    y0 = py / H * 2.0 - 1.0
    for px in range(W):
        x0 = px / W * 3.0 - 2.0
        x = 0.0
        y = 0.0
        it = 0
        while x * x + y * y <= 4.0 and it < MAX_ITER:
            xt = x * x - y * y + x0
            y = 2.0 * x * y + y0
            x = xt
            it += 1
        if it < MAX_ITER:
            escaped += 1
print(escaped)
