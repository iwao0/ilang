// The attribute set is closed: an unknown attribute name is a
// parse error listing the known set. Before this check a typo'd
// attribute (`@derieve`, `@bogus`) parsed successfully and was
// silently dropped — the program ran with the attribute's intended
// effect missing.
//
// expect-error: unknown attribute @derieve

@derieve(Eq, Hash)
class Key {
    id: i64
    init(v: i64) { this.id = v }
}
