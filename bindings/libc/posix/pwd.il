/// POSIX `<pwd.h>` + `<grp.h>` — user / group database lookups.
///
/// Both `struct passwd` and `struct group` use pointer fields into
/// internal static storage; the strings remain valid until the
/// next call to the same family.

@extern(C, "c") {
    /// `getpwuid` / `getpwnam` result.
    pub struct passwd {
        pw_name:   *const char
        pw_passwd: *const char
        pw_uid:    u32
        pw_gid:    u32
        pw_gecos:  *const char
        pw_dir:    *const char
        pw_shell:  *const char
    }

    /// `getgrgid` / `getgrnam` result.
    pub struct group {
        gr_name:   *const char
        gr_passwd: *const char
        gr_gid:    u32
        gr_mem:    **const char    // NULL-terminated list of member names
    }

    @lib @optional pub fn getpwuid(uid: u32): *passwd
    @lib @optional pub fn getpwnam(name: *const char): *passwd
    @lib @optional pub fn getgrgid(gid: u32): *group
    @lib @optional pub fn getgrnam(name: *const char): *group

    /// Iterate the entire user database.
    @lib @optional pub fn setpwent()
    @lib @optional pub fn getpwent(): *passwd
    @lib @optional pub fn endpwent()

    @lib @optional pub fn setgrent()
    @lib @optional pub fn getgrent(): *group
    @lib @optional pub fn endgrent()

    /// Re-entrant variants — caller-supplied storage. Returns 0 on
    /// success, an errno value otherwise. `*result` points to the
    /// caller's struct on hit, NULL on no-such-entry.
    @lib @optional pub fn getpwuid_r(
        uid:    u32,
        pwd:    *passwd,
        buf:    *char,
        buflen: size_t,
        result: **passwd
    ): i32

    @lib @optional pub fn getpwnam_r(
        name:   *const char,
        pwd:    *passwd,
        buf:    *char,
        buflen: size_t,
        result: **passwd
    ): i32
}
