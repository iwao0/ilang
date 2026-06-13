// `@flags` enum `has` and `~` were checker-accepted but mis-lowered:
//   * `f.has(other)` — the synthetic bool method `(f & other) == other`
//     had no MIR lowering, so any `.has(..)` call hit "method call on this
//     type / unhandled builtin".
//   * `~f` — bit-NOT lowered `UnOp::Not` straight onto the BOXED enum
//     value (a pointer), typed the result as the enum, then the next tag
//     read (`~f as u32`) dereferenced garbage -> SIGSEGV.
//
// Both now extract the tag, do the int op, and (for `~`) re-box via
// `$enum.box`, mirroring the existing `|` / `&` / `^` lowering.

// expect: flags has/not ok

use std.test as test

@flags
enum Perm: u32 {
    read = 1
    write = 2
    exec = 4
}

// --- has ---
let rw = Perm.read | Perm.write
test.expectTrue(rw.has(Perm.read))
test.expectTrue(rw.has(Perm.write))
test.expectTrue(!rw.has(Perm.exec))
// has against a compound flag (all queried bits must be present).
test.expectTrue(rw.has(Perm.read | Perm.write))
test.expectTrue(!rw.has(Perm.read | Perm.exec))
// self / single / empty-intersection.
test.expectTrue(Perm.read.has(Perm.read))
test.expectTrue(!Perm.read.has(Perm.write))
let empty = Perm.read & Perm.write          // no overlap -> 0
test.expectTrue(!empty.has(Perm.read))

// --- NOT (~) ---
let all = Perm.read | Perm.write | Perm.exec
// `~read` clears the read bit; AND with the known flags leaves write|exec.
test.expect((~Perm.read & all as u32) as i64, 6)
// `~read` over the full u32 repr = 0xFFFFFFFE.
test.expect((~Perm.read as u32) as i64, 4294967294)
// `~` round-trips back into a flags value usable in `has`.
test.expectTrue((~Perm.read).has(Perm.write))
test.expectTrue(!(~Perm.read).has(Perm.read))

console.log("flags has/not ok")
