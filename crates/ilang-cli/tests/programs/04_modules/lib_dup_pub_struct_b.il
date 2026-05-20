// Companion to `lib_dup_pub_struct_a.il` — also declares
// `pub struct Shared`, so re-exporting both through an umbrella
// is what the load-time duplicate check must reject.

@extern(C) {
    pub struct Shared {
        y: i32
    }
}
