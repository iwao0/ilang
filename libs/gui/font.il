/// Font description value type — family / size / weight / italic
/// in one bundle so dialogs and any future font-aware widget can
/// pass them around without an N-tuple. Mirrors the per-property
/// shape that `Canvas` exposes inline (`fontFamily` / `fontSize`
/// / `fontWeight` / `italic`).

/// `gui.Font` is a plain data carrier — no native resource is held
/// here. Backends consume the four fields and build a native font
/// at draw time (Canvas) or as the picker's seed value
/// (`Window.fontPicker`).
pub class Font {
    pub family: string
    pub size:   f64
    pub weight: i32
    pub italic: bool

    pub init(family: string, size: f64, weight: i32, italic: bool) {
        this.family = family
        this.size   = size
        this.weight = weight
        this.italic = italic
    }
}
