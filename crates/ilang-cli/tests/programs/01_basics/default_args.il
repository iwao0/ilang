// Default-value parameters: trailing parameters can supply a default
// value that's substituted at call sites where the caller omits the
// argument.
use test

fn open(path: string, mode: string = "r"): string {
    path + ":" + mode
}

// Two trailing defaults — caller can omit one or both.
fn greet(who: string = "world", excited: bool = false): string {
    if excited { "hi, " + who + "!" } else { "hi, " + who }
}

// All-defaults function — callable with zero args.
fn motto(): string {
    "ship it"
}

test.expectStr(open("README"), "README:r")
test.expectStr(open("README", "w"), "README:w")

test.expectStr(greet(), "hi, world")
test.expectStr(greet("Alice"), "hi, Alice")
test.expectStr(greet("Alice", true), "hi, Alice!")

test.expectStr(motto(), "ship it")
