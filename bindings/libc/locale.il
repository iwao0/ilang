/// C standard `<locale.h>` — global locale + numeric formatting hints.
///
/// `LC_*` category constants (pass these to `setlocale`):
///   LC_ALL       0
///   LC_COLLATE   1
///   LC_CTYPE     2
///   LC_MONETARY  3
///   LC_NUMERIC   4
///   LC_TIME      5
///
/// (Values match glibc / musl; Windows ucrt uses the same numbering.)

@extern(C, "c") {
    /// Locale-specific numeric / monetary formatting. Fields are
    /// pointers into the locale's static buffers — do not free.
    pub struct lconv {
        decimal_point:     *const char
        thousands_sep:     *const char
        grouping:          *const char
        int_curr_symbol:   *const char
        currency_symbol:   *const char
        mon_decimal_point: *const char
        mon_thousands_sep: *const char
        mon_grouping:      *const char
        positive_sign:     *const char
        negative_sign:     *const char
        int_frac_digits:   i8
        frac_digits:       i8
        p_cs_precedes:     i8
        p_sep_by_space:    i8
        n_cs_precedes:     i8
        n_sep_by_space:    i8
        p_sign_posn:       i8
        n_sign_posn:       i8
        int_p_cs_precedes: i8
        int_p_sep_by_space: i8
        int_n_cs_precedes: i8
        int_n_sep_by_space: i8
        int_p_sign_posn:   i8
        int_n_sign_posn:   i8
    }

    /// Query or set the global locale. Pass NULL for `locale` to
    /// query; `""` to inherit from the environment; a specific name
    /// like `"en_US.UTF-8"` to switch. Returns the active name.
    @lib pub fn setlocale(category: i32, locale: *const char): *char

    /// Snapshot of the current locale's formatting. The returned
    /// pointer is owned by the C runtime; do not free.
    @lib pub fn localeconv(): *lconv
}
