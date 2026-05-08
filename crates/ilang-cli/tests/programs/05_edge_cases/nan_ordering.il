// NaN ordering: every comparison involving NaN — including the
// reflexive `<=` / `>=` — must return false. `division_by_zero_float.il`
// covers `==`/`!=` reflexivity; this one covers the four order operators.
use test

let nan = 0.0 / 0.0
let one = 1.0

test.expectBool(nan < one, false)
test.expectBool(nan > one, false)
test.expectBool(nan <= one, false)
test.expectBool(nan >= one, false)

// NaN vs NaN — order operators all false too.
test.expectBool(nan < nan, false)
test.expectBool(nan > nan, false)
test.expectBool(nan <= nan, false)
test.expectBool(nan >= nan, false)

// finite vs NaN — same rule, the other direction.
test.expectBool(one < nan, false)
test.expectBool(one > nan, false)
test.expectBool(one <= nan, false)
test.expectBool(one >= nan, false)
