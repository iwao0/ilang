// Integer modulo-by-zero shares the same panic path as division-by-zero
// (the `division_by_zero_int.il` comment hints at this — but no test
// actually exercised the `%` operator, so a regression there could
// slip through). Both backends report "division by zero".
// expect-error: division by zero
let n = 10
let _ = n % 0
