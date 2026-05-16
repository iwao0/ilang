// Fixed-length array (`i64[3]`) as an enum variant payload used to
// return wrong values on match (3, 3, garbage) because the bare
// array literal `[10, 20, 30]` lowered to the dynamic-array layout
// (48-byte header + separate data buffer), then the dynamic↔fixed
// identity-coerce silently handed a header pointer to a consumer
// that treated it as inline data. `lower_enum_ctor` now passes the
// payload's len hint to `lower_array_literal_with_hint` when the
// arg is a bare literal.
//
// expect: 10
// expect: 20
// expect: 30
// expect: 1
// expect: 2
// expect: 3

enum BoxT { hold: (i64[3]) }
enum BoxS { hold: { data: i64[3] } }

let bt = BoxT.hold([10, 20, 30])
match bt {
    hold(d) {
        console.log(d[0].toString())
        console.log(d[1].toString())
        console.log(d[2].toString())
    }
}

let bs = BoxS.hold { data: [1, 2, 3] }
match bs {
    hold { data } {
        console.log(data[0].toString())
        console.log(data[1].toString())
        console.log(data[2].toString())
    }
}
let _ = 0
