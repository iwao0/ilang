import sys
sys.setrecursionlimit(1 << 20)

N = 1_000_000

class Node:
    __slots__ = ("value", "next")
    def __init__(self, value, next):
        self.value = value
        self.next = next

head = None
for i in range(N):
    head = Node(i, head)

s = 0
p = head
while p is not None:
    s += p.value
    p = p.next
print(s)
