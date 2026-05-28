/// 2D drawing surface — GTK 4 backend. Each Canvas owns a
/// persistent cairo image surface sized to its pixel dimensions;
/// the public drawing primitives render into that surface through a
/// long-lived `cairo_t`. The `GtkDrawingArea`'s draw-func just blits
/// the surface into the visible widget, so GTK's repaint after a
/// resize / expose costs nothing in user code.
///
/// Coordinate system: top-left origin, y-down — matches the rest of
/// gui-core and cairo's device space, so angles passed to
/// `canvasArc` map to the HTML Canvas convention (clockwise from 3
/// o'clock).
///
/// `cairo_fill` / `cairo_stroke` clear the current path, so (unlike
/// the Cocoa backend) a single path can't be both filled and stroked
/// without rebuilding it — issue the path again between the two.

use gtk {
    GtkWidget, GtkDrawingArea, GtkFixed,
    gtk_drawing_area_new, gtk_drawing_area_set_draw_func,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_queue_draw,
}
use cairo {
    cairo_t, cairo_surface_t,
    cairo_image_surface_create, cairo_surface_destroy,
    cairo_create, cairo_destroy, cairo_save, cairo_restore,
    cairo_set_source_rgba, cairo_set_source_surface,
    cairo_set_line_width, cairo_set_operator,
    cairo_paint, cairo_fill, cairo_stroke,
    cairo_new_path, cairo_close_path,
    cairo_move_to, cairo_line_to, cairo_rectangle, cairo_arc,
}
use core { WINDOWS }

// cairo_format_t.ARGB32 / cairo_operator_t.SOURCE.
const CAIRO_FORMAT_ARGB32: i32 = 0
const CAIRO_OPERATOR_SOURCE: i32 = 1

/// Per-canvas state. Keyed in `CANVASES` by the GtkDrawingArea
/// pointer (cast to i64) — same value gui-core sees as the public
/// `handle: i64`. Colours are pre-decomposed into cairo's 0..1
/// component range.
class CanvasSlot {
    pub area:      GtkDrawingArea
    pub fixed:     GtkFixed
    pub surface:   cairo_surface_t
    pub cr:        cairo_t
    pub w:         f64
    pub h:         f64
    pub fillR:     f64
    pub fillG:     f64
    pub fillB:     f64
    pub fillA:     f64
    pub strokeR:   f64
    pub strokeG:   f64
    pub strokeB:   f64
    pub strokeA:   f64
    pub lineWidth: f64

    pub init(
        area: GtkDrawingArea, fixed: GtkFixed,
        surface: cairo_surface_t, cr: cairo_t, w: f64, h: f64
    ) {
        this.area      = area
        this.fixed     = fixed
        this.surface   = surface
        this.cr        = cr
        this.w         = w
        this.h         = h
        this.fillR     = 0.0
        this.fillG     = 0.0
        this.fillB     = 0.0
        this.fillA     = 1.0
        this.strokeR   = 0.0
        this.strokeG   = 0.0
        this.strokeB   = 0.0
        this.strokeA   = 1.0
        this.lineWidth = 1.0
    }
}

let CANVASES = new Map<i64, CanvasSlot>()

@extern(C) {

fn applyFill(slot: CanvasSlot) {
    cairo_set_source_rgba(slot.cr, slot.fillR, slot.fillG, slot.fillB, slot.fillA)
}
fn applyStroke(slot: CanvasSlot) {
    cairo_set_source_rgba(slot.cr, slot.strokeR, slot.strokeG, slot.strokeB, slot.strokeA)
    cairo_set_line_width(slot.cr, slot.lineWidth)
}

/// `void(GtkDrawingArea*, cairo_t* cr, int w, int h, gpointer data)`.
/// `data` is the canvas handle; blit the backing surface onto the
/// widget's draw context.
pub fn linux_canvas_draw(area: i64, cr: i64, width: i32, height: i32, data: i64) {
    if let some(slot) = CANVASES.get(data) {
        cairo_set_source_surface(cr as cairo_t, slot.surface, 0.0, 0.0)
        cairo_paint(cr as cairo_t)
    }
}

pub fn createNativeCanvas(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let area = gtk_drawing_area_new() as GtkDrawingArea
    gtk_widget_set_size_request(area as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        area as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, area as GtkWidget, x, y)

    let surface = cairo_image_surface_create(
        CAIRO_FORMAT_ARGB32, width as i32, height as i32
    )
    let cr = cairo_create(surface)
    // Start opaque white so the surface isn't undefined memory.
    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 1.0)
    cairo_paint(cr)

    let handle = area as i64
    CANVASES[handle] = new CanvasSlot(area, winSlot.fixed, surface, cr, width, height)
    gtk_drawing_area_set_draw_func(
        area, linux_canvas_draw, handle as *void, 0 as i64
    )
    handle
}

pub fn setCanvasFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        gtk_widget_set_size_request(slot.area as GtkWidget, w as i32, h as i32)
        gtk_fixed_move(slot.fixed, slot.area as GtkWidget, x, y)
        // Pixel dimensions changed — reallocate the surface so drawing
        // coords line up with what gets blit'd. Previous contents are
        // discarded; callers that want them preserved re-render after.
        if w != slot.w || h != slot.h {
            cairo_destroy(slot.cr)
            cairo_surface_destroy(slot.surface)
            slot.surface = cairo_image_surface_create(
                CAIRO_FORMAT_ARGB32, w as i32, h as i32
            )
            slot.cr = cairo_create(slot.surface)
            cairo_set_source_rgba(slot.cr, 1.0, 1.0, 1.0, 1.0)
            cairo_paint(slot.cr)
            slot.w = w
            slot.h = h
            gtk_widget_queue_draw(slot.area as GtkWidget)
        }
    }
}

pub fn setCanvasVisible(handle: i64, visible: bool) {
    if let some(slot) = CANVASES.get(handle) {
        gtk_widget_set_visible(
            slot.area as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setCanvasFillColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        let mask: u32 = 0xFF as u32
        slot.fillA = (((argb >> 24) & mask) as f64) / 255.0
        slot.fillR = (((argb >> 16) & mask) as f64) / 255.0
        slot.fillG = (((argb >>  8) & mask) as f64) / 255.0
        slot.fillB = (( argb        & mask) as f64) / 255.0
    }
}

pub fn setCanvasStrokeColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        let mask: u32 = 0xFF as u32
        slot.strokeA = (((argb >> 24) & mask) as f64) / 255.0
        slot.strokeR = (((argb >> 16) & mask) as f64) / 255.0
        slot.strokeG = (((argb >>  8) & mask) as f64) / 255.0
        slot.strokeB = (( argb        & mask) as f64) / 255.0
    }
}

pub fn setCanvasLineWidth(handle: i64, w: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.lineWidth = w
    }
}

pub fn canvasClear(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        let mask: u32 = 0xFF as u32
        let a = (((argb >> 24) & mask) as f64) / 255.0
        let r = (((argb >> 16) & mask) as f64) / 255.0
        let g = (((argb >>  8) & mask) as f64) / 255.0
        let b = (( argb        & mask) as f64) / 255.0
        cairo_save(slot.cr)
        cairo_set_source_rgba(slot.cr, r, g, b, a)
        // SOURCE replaces (vs. OVER blends) so a translucent argb
        // overwrites rather than compositing onto prior contents.
        cairo_set_operator(slot.cr, CAIRO_OPERATOR_SOURCE)
        cairo_paint(slot.cr)
        cairo_restore(slot.cr)
        gtk_widget_queue_draw(slot.area as GtkWidget)
    }
}

pub fn canvasFillRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        applyFill(slot)
        cairo_rectangle(slot.cr, x, y, w, h)
        cairo_fill(slot.cr)
        gtk_widget_queue_draw(slot.area as GtkWidget)
    }
}

pub fn canvasStrokeRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        applyStroke(slot)
        cairo_rectangle(slot.cr, x, y, w, h)
        cairo_stroke(slot.cr)
        gtk_widget_queue_draw(slot.area as GtkWidget)
    }
}

pub fn canvasBeginPath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        cairo_new_path(slot.cr)
    }
}

pub fn canvasMoveTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        cairo_move_to(slot.cr, x, y)
    }
}

pub fn canvasLineTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        cairo_line_to(slot.cr, x, y)
    }
}

pub fn canvasArc(
    handle: i64,
    cx: f64, cy: f64, r: f64, startAngle: f64, endAngle: f64
) {
    if let some(slot) = CANVASES.get(handle) {
        cairo_arc(slot.cr, cx, cy, r, startAngle, endAngle)
    }
}

pub fn canvasClosePath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        cairo_close_path(slot.cr)
    }
}

pub fn canvasFill(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        applyFill(slot)
        cairo_fill(slot.cr)
        gtk_widget_queue_draw(slot.area as GtkWidget)
    }
}

pub fn canvasStroke(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        applyStroke(slot)
        cairo_stroke(slot.cr)
        gtk_widget_queue_draw(slot.area as GtkWidget)
    }
}

} // @extern(C)
