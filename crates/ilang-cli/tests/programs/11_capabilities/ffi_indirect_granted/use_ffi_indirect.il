// With `ffi` granted the indirect C call is allowed and runs to
// completion: abs(-7) == 7.
//
// expect: 7
@extern(C) {
    @lib("c") fn abs(x: i32): i32

    pub fn run() {
        let f = abs
        console.log(f(-7 as i32).toString())
    }
}

run()
