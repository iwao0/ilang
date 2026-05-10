// `@flags` is incompatible with `: string`-repr enums —
// bitwise ops are int-only, and ORing two strings has no
// meaning. The MIR lowerer rejects the combination.
//
// jit: skip
// expect-error: @flags is not allowed on `: string`-repr enums

@flags
pub enum F: string {
    a = "A"
    b = "B"
}
