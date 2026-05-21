/// dwrite.dll — DirectWrite text shaping + glyph rendering.
///
/// Bring-up:
///   1. DWriteCreateFactory(SharedFactory, &IID_IDWriteFactory, &factory)
///   2. factory.CreateTextFormat(...)
///   3. factory.CreateTextLayout(text, format, maxW, maxH, &layout)
///   4. (Direct2D) renderTarget.DrawTextLayout(...)
///   or  (custom)  layout.Draw(client, &renderer, x, y)
///
/// Heavy COM surface (IDWriteFactory / TextFormat / TextLayout /
/// GdiInterop / FontCollection) is deferred — this file gives you
/// the factory entry point + constants.

// ─── DWRITE_FACTORY_TYPE ─────────────────────────────────────────
pub enum DWriteFactoryType: i32 {
    shared   = 0
    isolated = 1
}

// ─── DWRITE_FONT_WEIGHT ──────────────────────────────────────────
pub enum DWriteFontWeight: i32 {
    thin       = 100
    extraLight = 200
    light      = 300
    semiLight  = 350
    normal     = 400
    medium     = 500
    semiBold   = 600
    bold       = 700
    extraBold  = 800
    black      = 900
    extraBlack = 950
}

// ─── DWRITE_FONT_STYLE ───────────────────────────────────────────
pub enum DWriteFontStyle: i32 {
    normal  = 0
    oblique = 1
    italic  = 2
}

// ─── DWRITE_FONT_STRETCH ─────────────────────────────────────────
pub enum DWriteFontStretch: i32 {
    undefined        = 0
    ultraCondensed   = 1
    extraCondensed   = 2
    condensed        = 3
    semiCondensed    = 4
    normal           = 5
    semiExpanded     = 6
    expanded         = 7
    extraExpanded    = 8
    ultraExpanded    = 9
}

// ─── DWRITE_TEXT_ALIGNMENT ───────────────────────────────────────
pub enum DWriteTextAlignment: i32 {
    leading   = 0
    trailing  = 1
    center    = 2
    justified = 3
}

// ─── DWRITE_PARAGRAPH_ALIGNMENT ──────────────────────────────────
pub enum DWriteParagraphAlignment: i32 {
    near_  = 0
    far_   = 1
    center = 2
}

// ─── DWRITE_WORD_WRAPPING ────────────────────────────────────────
pub enum DWriteWordWrapping: i32 {
    wrap            = 0
    noWrap          = 1
    emergencyBreak  = 2
    wholeWord       = 3
    character       = 4
}

// ─── DWRITE_READING_DIRECTION ────────────────────────────────────
pub enum DWriteReadingDirection: i32 {
    leftToRight = 0
    rightToLeft = 1
    topToBottom = 2
    bottomToTop = 3
}

@extern(C, "dwrite") {
    /// `DWriteCreateFactory(type, iid, factory)`. `iid` is a
    /// pointer to the IDWriteFactory IID — pass an `ole32.GUID`.
    @lib pub fn DWriteCreateFactory(
        factoryType: i32,
        iid:         *const void,
        factory:     **void
    ): i32
}
