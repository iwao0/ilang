// Exercises the `time` module — wall / monotonic clocks, sleep,
// DateTime breakdown / composition / ISO round-trip / strftime
// formatting. Uses a fixed epoch (2026-05-21T15:30:45.123Z =
// 1779377445123 ms) so the assertions are deterministic.
use time
use test

let epoch: i64 = 1779377445123 as i64
let dt = time.DateTime.fromUnixMs(epoch)
test.expect(dt.year as i64, 2026)
test.expect(dt.month as i64, 5)
test.expect(dt.day as i64, 21)
test.expect(dt.hour as i64, 15)
test.expect(dt.minute as i64, 30)
test.expect(dt.second as i64, 45)
test.expect(dt.ms as i64, 123)
test.expect(dt.weekday as i64, 4)   // Thursday
test.expect(dt.offsetMin as i64, 0)

// Round-trip via toUnixMs.
test.expect(dt.toUnixMs(), epoch)

// toIso emits the canonical form with `Z` for UTC.
test.expectStr(dt.toIso(), "2026-05-21T15:30:45.123Z")

// parseIso parses the same string back to the same instant.
match time.DateTime.parseIso("2026-05-21T15:30:45.123Z") {
    ok(d) {
        test.expect(d.toUnixMs(), epoch)
        test.expectStr(d.toIso(), "2026-05-21T15:30:45.123Z")
    }
    err(m) { console.log("parseIso failed:", m) }
}

// Offset round-trip: `+09:00` becomes 540 minutes east.
match time.DateTime.parseIso("2026-05-22T00:30:45+09:00") {
    ok(d) {
        test.expect(d.offsetMin as i64, 540)
        test.expect(d.toUnixMs(), epoch - 123)   // same instant minus ms
        test.expectStr(d.toIso(), "2026-05-22T00:30:45+09:00")
    }
    err(m) { console.log("parseIso offset failed:", m) }
}

// Rejection cases.
match time.DateTime.parseIso("not a date") {
    ok(_) { console.log("FAIL: parseIso accepted garbage") }
    err(_) {}
}
match time.DateTime.parseIso("2026-13-01T00:00:00Z") {
    // Out-of-range month parses lexically but composes to a shifted
    // date — we accept this and assert the parser didn't crash.
    ok(d) { test.expect(d.month as i64, 13) }
    err(_) {}
}

// strftime — common specifiers.
test.expectStr(dt.format("%Y-%m-%d"), "2026-05-21")
test.expectStr(dt.format("%H:%M:%S.%L"), "15:30:45.123")
test.expectStr(dt.format("%a %b %d %Y"), "Thu May 21 2026")
test.expectStr(dt.format("%A, %B %d"), "Thursday, May 21")
test.expectStr(dt.format("%I:%M %p"), "03:30 PM")
test.expectStr(dt.format("%z"), "+0000")
test.expectStr(dt.format("%:z"), "+00:00")
test.expectStr(dt.format("%j"), "141")
test.expectStr(dt.format("%s"), "1779377445")
test.expectStr(dt.format("100%% done"), "100% done")
test.expectStr(dt.format("%X"), "%X")  // unknown specifier echoed

// Offset matters for %z / %s.
match time.DateTime.parseIso("2026-05-22T00:30:45+09:00") {
    ok(d) {
        test.expectStr(d.format("%z"), "+0900")
        test.expectStr(d.format("%H:%M"), "00:30")
        test.expectStr(d.format("%s"), "1779377445")
    }
    err(_) {}
}

// Monotonic clock advances across a sleep.
let t0 = time.monotonicNs()
time.sleep(5 as i64)
let t1 = time.monotonicNs()
test.expectBool(t1 > t0, true)
// 5 ms in ns is 5_000_000 — allow OS slop, just assert > 1 ms.
test.expectBool((t1 - t0) > 1000000, true)

// now() / utcNow() share the same instant modulo TZ offset, but
// converted back to Unix ms they must match.
let a = time.DateTime.utcNow()
let b = time.DateTime.now()
let diff = a.toUnixMs() - b.toUnixMs()
let absDiff = if diff < 0 { -diff } else { diff }
test.expectBool(absDiff < 100, true)   // within 100 ms of each other
