// Overloading by parameter count.
use test

fn make(): string { "default" }
fn make(s: string): string { s }
fn make(s: string, suffix: string): string { s + suffix }

test.expectStr(make(), "default")
test.expectStr(make("hello"), "hello")
test.expectStr(make("hello", "!"), "hello!")
