import ctypes
import platform

if platform.system() == "Darwin":
    libc = ctypes.CDLL("libc.dylib")
else:
    libc = ctypes.CDLL("libc.so.6")
libc.abs.argtypes = [ctypes.c_int]
libc.abs.restype = ctypes.c_int

N = 100_000_000
s = 0
for i in range(N):
    s += libc.abs(i - N // 2)
print(s)
