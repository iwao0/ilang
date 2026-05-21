// `@target("os")` / `@target(not "os")` — host-OS filter applied by
// the loader before merge. Same-name fns differentiated by OS leave
// exactly one survivor per host; same goes for class methods.
//
// `osName()` doubles as the ground-truth host name (it survives the
// same filter as everything else here), so the test stays self-
// contained without depending on the runtime `os.platform` probe.
//
// expect: OK

@target("macos")
fn osName(): string { "macos" }

@target("linux")
fn osName(): string { "linux" }

@target("windows")
fn osName(): string { "windows" }

@target(not "macos")
@target(not "linux")
@target(not "windows")
fn osName(): string { "other" }

@target("macos", "linux")
fn posixFlag(): i32 { 1 as i32 }

@target("windows")
fn posixFlag(): i32 { 0 as i32 }

@target(not "macos")
@target(not "linux")
@target(not "windows")
fn posixFlag(): i32 { 0 as i32 }

class Greeter {
    @target("macos")
    pub hello(): string { "hello mac" }

    @target("linux")
    pub hello(): string { "hello linux" }

    @target("windows")
    pub hello(): string { "hello win" }

    @target(not "macos")
    @target(not "linux")
    @target(not "windows")
    pub hello(): string { "hello other" }
}

let host = osName()
let g = new Greeter()
let expectedHello = if host == "macos" { "hello mac" }
    elif host == "linux" { "hello linux" }
    elif host == "windows" { "hello win" }
    else { "hello other" }
let expectedPosix = if host == "macos" || host == "linux" { 1 as i32 } else { 0 as i32 }

if posixFlag() == expectedPosix && g.hello() == expectedHello {
    console.log("OK")
} else {
    console.log("FAIL host=" + host + " posix=" + posixFlag().toString() + " hello=" + g.hello())
}
