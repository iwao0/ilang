// Default arguments coexist with method overloading — a candidate
// whose arity matches the call exactly wins over a default-filled
// candidate (the latter takes a +1000 score penalty per
// syntax.md §6 "Default arguments").
use std.test as test

class Greeter {
    // Two-arg variant with default `count = 1`. A call site that
    // explicitly passes both args still picks this one (exact match);
    // a 1-arg call also lands here, with `count` defaulted.
    hi(name: string, count: i64 = 1): string {
        name + ":" + count.toString()
    }

    // Separate 0-arg overload — disambiguated purely by arity, the
    // 2-arg variant's default doesn't make it eligible for a 0-arg
    // call (a default-filled candidate scores 1000 worse).
    hi(): string { "noname" }
}

let g = new Greeter()

// 0-arg call → 0-arg overload.
test.expectStr(g.hi(), "noname")

// 1-arg call → 2-arg overload with `count` defaulted to 1.
test.expectStr(g.hi("ada"), "ada:1")

// 2-arg call → 2-arg overload, `count` filled explicitly.
test.expectStr(g.hi("ada", 5), "ada:5")
