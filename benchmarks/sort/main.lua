local N = 2000000

local function qsort(a, lo, hi)
    if lo >= hi then return end
    local pivot = a[math.floor((lo + hi) / 2)]
    local i, j = lo, hi
    while i <= j do
        while a[i] < pivot do i = i + 1 end
        while a[j] > pivot do j = j - 1 end
        if i <= j then
            a[i], a[j] = a[j], a[i]
            i = i + 1
            j = j - 1
        end
    end
    qsort(a, lo, j)
    qsort(a, i, hi)
end

local a = {}
for i = 0, N - 1 do
    a[i] = (i * 2654435761) % 1000003
end
qsort(a, 0, N - 1)
print(a[0], a[N - 1])
