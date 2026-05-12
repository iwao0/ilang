local W, H, MAX_ITER = 1024, 1024, 1000
local escaped = 0
for py = 0, H - 1 do
    local y0 = py / H * 2.0 - 1.0
    for px = 0, W - 1 do
        local x0 = px / W * 3.0 - 2.0
        local x, y = 0.0, 0.0
        local it = 0
        while x * x + y * y <= 4.0 and it < MAX_ITER do
            local xt = x * x - y * y + x0
            y = 2.0 * x * y + y0
            x = xt
            it = it + 1
        end
        if it < MAX_ITER then escaped = escaped + 1 end
    end
end
print(escaped)
