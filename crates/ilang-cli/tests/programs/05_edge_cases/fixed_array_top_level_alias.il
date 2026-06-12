// Top-level `let` bindings live in `__main`, whose exit epilogue
// has its OWN release loop (decl/bodies.rs) separate from the
// in-fn scope sweep. A fixed-length-array alias bound at top level
// (`let m = teamA.members`) must be skipped there too: the buffer
// belongs to the object's field and the drop cascade frees it —
// releasing the alias as well double-freed the buffer and aborted
// at process exit (SIGABRT, found via a documentation example).
// Owned top-level fixed arrays (fresh literal) still release.
//
// expect: 1: 100
// expect: 2: 100
// expect: 3: teamA=100 teamB=777

class Box {
    n: i64
    init(x: i64) { this.n = x }
}
class Team {
    members: Box[2]
    init(a: Box, b: Box) { this.members = [a, b] }
}

// owned top-level fixed array + write through a fn param
fn change(x: Box[2]) { x[0] = new Box(100) }
let arr: Box[2] = [new Box(1), new Box(2)]
change(arr)
console.log("1: " + arr[0].n.toString())

// top-level alias of a field, written through
let teamA = new Team(new Box(1), new Box(2))
let m = teamA.members
m[0] = new Box(100)
console.log("2: " + teamA.members[0].n.toString())

// field-to-field copy stays independent
let teamB = new Team(new Box(8), new Box(9))
teamB.members = teamA.members
teamB.members[0] = new Box(777)
console.log("3: teamA=" + teamA.members[0].n.toString()
    + " teamB=" + teamB.members[0].n.toString())
