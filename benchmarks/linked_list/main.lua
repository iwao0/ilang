local N = 10000000
local head = nil
for i = 0, N - 1 do
    head = { value = i, next = head }
end
local sum = 0
local p = head
while p do
    sum = sum + p.value
    p = p.next
end
print(sum)
