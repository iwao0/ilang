// Smoke test that the expanded libc umbrella parses + links. Picks
// one symbol from each new sub-module so a regression in any of
// them surfaces here before it bites a downstream binding.
use test
use libc {
    // string.il additions
    strtok,
    // stdlib.il additions
    div, div_t, atoll,
    // stdio.il additions
    puts, vsnprintf, fpos_t,
    // cmath.il additions
    fmin, fmax, log1p,
    // time.il additions (existing)
    time,
    // errno.il
    _errno,
    // locale.il
    setlocale, lconv,
    // signal.il
    raise,
    // setjmp.il
    jmp_buf,
    // wctype.il (wide ctype — symbol may be missing at runtime)
    iswalpha,
    // fenv.il
    FeRound, FeExcept,
    // inttypes.il
    imaxabs, imaxdiv_t,
    // threads.il
    ThrdResult, MtxType,
    // uchar.il
    mbstate_t,
}

@extern(C) {
    pub fn run() {
        // Basic math: fmin / fmax / log1p exercise the new symbols.
        test.expectF64(fmin(2.0, 5.0), 2.0)
        test.expectF64(fmax(2.0, 5.0), 5.0)
        // log1p(0) == 0.
        test.expectF64(log1p(0.0), 0.0)

        // div(7, 2) — quot 3, rem 1.
        let d: div_t = div(7 as i32, 2 as i32)
        test.expect(d.quot as i64, 3 as i64)
        test.expect(d.rem  as i64, 1 as i64)

        // time() should return something positive on every host.
        let t = time(0 as *i64)
        test.expectTrue(t > 0 as i64)

        // inttypes — imaxabs of a negative literal.
        test.expect(imaxabs(-7 as i64), 7 as i64)

        // fenv — round-mode constants.
        test.expect(FeRound.toNearest as i64, 0 as i64)
        test.expect(FeRound.upward    as i64, 0x800 as i64)
        // C11 thread result enum values match the C surface.
        test.expect(ThrdResult.success as i64, 0 as i64)
        test.expect(ThrdResult.busy    as i64, 3 as i64)
    }
}

run()
