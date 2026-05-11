// By-value struct passing on `@extern(C)` packs the struct into 1–2
// i64 GPR chunks. A float field would actually flow through FP
// registers per AArch64 AAPCS64 HFA / x86_64 SysV SSE classification,
// so silently using the GPR path would violate the ABI. Reject this
// at registration.
// expect-error: supported shapes are integer/bool fields or homogeneous float aggregates

@extern(C) {
    struct mixed {
        x: i32
        y: f32
    }

    fn takes_mixed(m: mixed): i64
}

let m = new mixed()
m.x = 1 as i32
m.y = 2.0 as f32
let _ = takes_mixed(m)
