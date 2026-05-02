// Smoke fixture for the .il-file test harness. Now exercises the
// `test` built-in too — if these break, every other fixture is suspect.
use test
test.expect(3 + 4, 7)
test.expectStr("ab" + "c", "abc")
test.expectBool(false, false)
test.expectF64(2.5 + 0.5, 3.0)
test.expectTrue(1 < 2)
test.expectFalse(1 > 2)
