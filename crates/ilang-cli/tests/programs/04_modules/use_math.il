// Built-in `math` module — shipped inside the compiler, no on-disk
// `math.il` needed.
//
// IMPORTANT: every `math.X` intrinsic the runtime exports needs at
// least one call here so the AOT linker pulls in (and resolves) the
// corresponding `$math.X` symbol. The runtime exported `math.sin`
// without the `$` sigil for months before anyone noticed because no
// fixture exercised it under `ILANG_TEST_AOT=1`. Keep coverage
// exhaustive.
use test
use math

// --- Powers / roots ---
test.expectF64(math.sqrt(16.0), 4.0)
test.expectF64(math.sqrt(25), 5.0)  // integer arg via implicit promotion
test.expectF64(math.pow(2.0, 10.0), 1024.0)
test.expectF64(math.exp(0.0), 1.0)

// --- Logarithms ---
test.expectF64(math.ln(1.0), 0.0)
test.expectF64(math.ln(math.e), 1.0)
test.expectF64(math.log10(100.0), 2.0)
test.expectF64(math.log10(1.0), 0.0)
test.expectF64(math.log2(8.0), 3.0)
test.expectF64(math.log2(1.0), 0.0)

// --- Rounding / abs ---
test.expectF64(math.floor(3.9), 3.0)
test.expectF64(math.ceil(3.1), 4.0)
test.expectF64(math.round(2.4), 2.0)
test.expectF64(math.round(2.6), 3.0)
test.expectF64(math.abs(-7.5), 7.5)
test.expectF64(math.abs(7.5), 7.5)

// --- Trig (using `math.pi` const from stdlib) ---
test.expectF64(math.sin(0.0), 0.0)
test.expectF64(math.sin(math.pi / 2.0), 1.0)
test.expectF64(math.cos(0.0), 1.0)
test.expectF64(math.cos(math.pi), -1.0)
test.expectF64(math.tan(0.0), 0.0)

// --- Inverse trig ---
test.expectF64(math.asin(0.0), 0.0)
test.expectF64(math.asin(1.0), math.pi / 2.0)
test.expectF64(math.acos(1.0), 0.0)
test.expectF64(math.atan(0.0), 0.0)
test.expectF64(math.atan(1.0), math.pi / 4.0)
test.expectF64(math.atan2(0.0, 1.0), 0.0)
test.expectF64(math.atan2(1.0, 0.0), math.pi / 2.0)
