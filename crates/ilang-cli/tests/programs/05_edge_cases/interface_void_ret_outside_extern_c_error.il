// C-only return types (`void`, `size_t`, `char` proper) are also
// blocked in top-level interface signatures.
//
// expect-error: C-only type, nameable only inside

pub interface Bad {
    sample(): size_t
}
