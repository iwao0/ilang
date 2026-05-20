/// kernel32 — SYSTEMTIME / FILETIME round-trips.

use test
use windows {
    SYSTEMTIME, FILETIME,
    GetSystemTime, GetLocalTime,
    SystemTimeToFileTime, FileTimeToSystemTime,
    FileTimeToLocalFileTime, LocalFileTimeToFileTime,
    CompareFileTime,
    GetSystemTimeAsFileTime,
}

@extern(C) {
    pub fn run() {
        // GetSystemTime fills a sensible struct (year ≥ 2000).
        let st = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        }
        GetSystemTime(st)
        test.expectTrue(st.wYear >= 2000)
        test.expectTrue(st.wMonth >= 1 && st.wMonth <= 12)
        test.expectTrue(st.wDay >= 1 && st.wDay <= 31)
        test.expectTrue(st.wHour <= 23)
        test.expectTrue(st.wMinute <= 59)
        test.expectTrue(st.wSecond <= 59)

        // GetLocalTime → SystemTimeToFileTime → FileTimeToSystemTime
        // round-trip.
        let lt = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        }
        GetLocalTime(lt)

        let ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        let ok = SystemTimeToFileTime(lt, ft)
        test.expectTrue(ok != 0)

        let lt2 = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        }
        let ok2 = FileTimeToSystemTime(ft, lt2)
        test.expectTrue(ok2 != 0)

        test.expect(lt.wYear as i64, lt2.wYear as i64)
        test.expect(lt.wMonth as i64, lt2.wMonth as i64)
        test.expect(lt.wDay as i64, lt2.wDay as i64)
        test.expect(lt.wHour as i64, lt2.wHour as i64)
        test.expect(lt.wMinute as i64, lt2.wMinute as i64)
        test.expect(lt.wSecond as i64, lt2.wSecond as i64)

        // Local FILETIME → UTC FILETIME → Local FILETIME.
        let localFt = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        LocalFileTimeToFileTime(ft, localFt)
        let roundFt = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        FileTimeToLocalFileTime(localFt, roundFt)
        test.expect(ft.dwLowDateTime as i64, roundFt.dwLowDateTime as i64)
        test.expect(ft.dwHighDateTime as i64, roundFt.dwHighDateTime as i64)

        // CompareFileTime: identical times compare as 0.
        test.expect(CompareFileTime(ft, ft) as i64, 0)

        // GetSystemTimeAsFileTime: nonzero high or low.
        let ft2 = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        GetSystemTimeAsFileTime(ft2)
        test.expectTrue(ft2.dwHighDateTime > 0)
    }
}

run()
