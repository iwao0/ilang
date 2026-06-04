/// Minimal cairo 2D-graphics bindings.
///
/// Scoped to what `libs/gui/linux` needs to drive a `GtkDrawingArea`
/// backed by a persistent image surface: create a surface, draw into
/// it through a context, and blit the surface in the widget's
/// draw-func. Pointer-shaped cairo objects (`cairo_t*`,
/// `cairo_surface_t*`) are modelled as `@handle` types — they flow as
/// opaque, pointer-sized values and are never dereferenced ilang-side.
///
/// Library name "cairo" → `libcairo.so` on Linux.

@extern(C, "cairo") {
    @handle pub struct cairo_t {}
    @handle pub struct cairo_surface_t {}

    // ─── surfaces ─────────────────────────────────────────────
    // `format` uses cairo_format_t: ARGB32 = 0, RGB24 = 1, A8 = 2.
    @lib pub fn cairo_image_surface_create(
        format: i32, width: i32, height: i32
    ): cairo_surface_t
    // Wrap an existing pixel buffer as a cairo image surface — the
    // caller owns the storage; cairo only borrows it. `stride` MUST
    // match `cairo_format_stride_for_width(format, width)` for that
    // (format, width) pair, else cairo asserts / draws garbage.
    @lib pub fn cairo_image_surface_create_for_data(
        data: *void, format: i32, width: i32, height: i32, stride: i32
    ): cairo_surface_t
    @lib pub fn cairo_format_stride_for_width(format: i32, width: i32): i32
    // Call BEFORE mutating the pixel buffer behind cairo's back.
    // Detaches any snapshots cairo holds (e.g. ones attached when
    // the surface was used as a source in a previous draw); without
    // this, the following `mark_dirty` aborts with
    // `! _cairo_surface_has_snapshots (surface)`.
    @lib pub fn cairo_surface_flush(surface: cairo_surface_t)
    // After mutating the underlying pixel buffer behind cairo's
    // back (e.g. memcpy from a wgpu staging buffer), call this so
    // cairo invalidates its render cache. Must be preceded by
    // `cairo_surface_flush` if the surface was used as a source.
    @lib pub fn cairo_surface_mark_dirty(surface: cairo_surface_t)
    @lib pub fn cairo_surface_destroy(surface: cairo_surface_t)

    // ─── contexts ─────────────────────────────────────────────
    @lib pub fn cairo_create(target: cairo_surface_t): cairo_t
    @lib pub fn cairo_destroy(cr: cairo_t)
    @lib pub fn cairo_save(cr: cairo_t)
    @lib pub fn cairo_restore(cr: cairo_t)

    // ─── source / state ───────────────────────────────────────
    @lib pub fn cairo_set_source_rgba(
        cr: cairo_t, r: f64, g: f64, b: f64, a: f64
    )
    @lib pub fn cairo_set_source_surface(
        cr: cairo_t, surface: cairo_surface_t, x: f64, y: f64
    )
    @lib pub fn cairo_set_line_width(cr: cairo_t, width: f64)
    // `op` uses cairo_operator_t: CLEAR = 0, SOURCE = 1, OVER = 2.
    @lib pub fn cairo_set_operator(cr: cairo_t, op: i32)

    // ─── painting ─────────────────────────────────────────────
    @lib pub fn cairo_paint(cr: cairo_t)
    @lib pub fn cairo_fill(cr: cairo_t)
    @lib pub fn cairo_stroke(cr: cairo_t)

    // ─── path construction ────────────────────────────────────
    @lib pub fn cairo_new_path(cr: cairo_t)
    @lib pub fn cairo_close_path(cr: cairo_t)
    @lib pub fn cairo_move_to(cr: cairo_t, x: f64, y: f64)
    @lib pub fn cairo_line_to(cr: cairo_t, x: f64, y: f64)
    @lib pub fn cairo_rectangle(
        cr: cairo_t, x: f64, y: f64, width: f64, height: f64
    )
    // Angles in radians; positive sweep is clockwise in cairo's
    // y-down device space — matches the HTML Canvas `arc` default.
    @lib pub fn cairo_arc(
        cr: cairo_t, xc: f64, yc: f64, radius: f64,
        angle1: f64, angle2: f64
    )

    // ─── text / fonts ─────────────────────────────────────────
    // `slant` uses cairo_font_slant_t: NORMAL=0, ITALIC=1, OBLIQUE=2.
    // `weight` uses cairo_font_weight_t: NORMAL=0, BOLD=1.
    @lib pub fn cairo_select_font_face(
        cr: cairo_t, family: *const char, slant: i32, weight: i32
    )
    @lib pub fn cairo_set_font_size(cr: cairo_t, size: f64)
    // Draws `text` (NUL-terminated UTF-8) at the current point on
    // the baseline; advances the current point by the run's
    // x-advance. Wrap in `save`/`restore` if the side effect on
    // the path's current point would interfere with later drawing.
    @lib pub fn cairo_show_text(cr: cairo_t, text: *const char)
    // Appends the glyph outlines for `text` to the current path
    // without painting them. Follow with `cairo_stroke` or
    // `cairo_fill` to render.
    @lib pub fn cairo_text_path(cr: cairo_t, text: *const char)

    /// 48 bytes. Per-string typographic extents — what
    /// `cairo_text_extents` populates. `x_advance` is the width
    /// most callers want; `width` / `height` are the ink bounds.
    pub struct cairo_text_extents_t {
        x_bearing: f64
        y_bearing: f64
        width:     f64
        height:    f64
        x_advance: f64
        y_advance: f64
    }
    /// 40 bytes. Per-font typographic extents (ascent, descent,
    /// line height) for the current font face / size pair.
    pub struct cairo_font_extents_t {
        ascent:        f64
        descent:       f64
        height:        f64
        max_x_advance: f64
        max_y_advance: f64
    }
    @lib pub fn cairo_text_extents(
        cr: cairo_t, text: *const char, extents: *cairo_text_extents_t
    )
    @lib pub fn cairo_font_extents(
        cr: cairo_t, extents: *cairo_font_extents_t
    )
}
