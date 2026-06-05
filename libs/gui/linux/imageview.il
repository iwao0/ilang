/// Image display widget — `GtkPicture` (GTK 4.8+) wraps the
/// loaded PNG / JPEG / SVG and renders it scaled to fit the
/// widget's allocation. Input events are read off the standard
/// gesture / motion controllers the rest of libs/gui uses.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkPicture, GtkFixed, GObject,
    GtkEventController, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkGestureSingle, GtkGestureClick,
    gtk_picture_new, gtk_picture_set_filename,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_add_controller,
    gtk_event_controller_motion_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_get_widget,
    gtk_event_controller_get_current_event_state,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button, gtk_gesture_single_get_current_button,
    g_signal_connect_data, g_signal_connect_motion, g_signal_connect_click,
    g_signal_connect_scroll,
}
use core { WINDOWS, resolveGtkParent, hasMod, gdkBtnToW3c }

class ImageViewSlot {
    pub pic:   GtkPicture
    pub fixed: GtkFixed
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(pic: GtkPicture, fixed: GtkFixed) {
        this.pic = pic
        this.fixed = fixed
        this.onMouseEnter  = ivNoOpEnterLeave
        this.onMouseLeave  = ivNoOpEnterLeave
        this.onMouseDown   = ivNoOpClickMod
        this.onMouseUp     = ivNoOpClickMod
        this.onContextMenu = ivNoOpClickMod
        this.onMouseWheel  = ivNoOpWheel
    }
}

fn ivNoOpEnterLeave(x: f64, y: f64) {}
fn ivNoOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn ivNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32

let IMAGE_VIEWS = new Map<i64, ImageViewSlot>()

@extern(C) {
    pub fn linux_imageview_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = IMAGE_VIEWS.get(w as i64) {
            slot.onMouseEnter(x, y)
        }
    }
    pub fn linux_imageview_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = IMAGE_VIEWS.get(w as i64) {
            slot.onMouseLeave(0.0, 0.0)
        }
    }
    pub fn linux_imageview_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = IMAGE_VIEWS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let w3c = gdkBtnToW3c(btn)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            let ck = hasMod(st, MOD_CTRL)
            let sk = hasMod(st, MOD_SHIFT)
            let ak = hasMod(st, MOD_ALT)
            let mk = hasMod(st, MOD_META)
            slot.onMouseDown(x, y, w3c, ck, sk, ak, mk)
            if btn == 3 as u32 {
                let ctx = slot.onContextMenu
                ctx(x, y, w3c, ck, sk, ak, mk)
            }
        }
    }
    pub fn linux_imageview_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = IMAGE_VIEWS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            slot.onMouseUp(x, y, gdkBtnToW3c(btn),
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT),  hasMod(st, MOD_META))
        }
    }
    pub fn linux_imageview_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = IMAGE_VIEWS.get(w as i64) {
            slot.onMouseWheel(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeImageView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let pic = gtk_picture_new() as GtkPicture
    gtk_widget_set_size_request(pic as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(pic as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(pic as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, pic as GtkWidget, x, y)

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(pic as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_motion(
        motionCtl as GObject, cstrFromString("enter"),
        linux_imageview_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_imageview_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(pic as GtkWidget, click as GtkEventController)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_imageview_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_imageview_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(pic as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_imageview_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = pic as i64
    IMAGE_VIEWS[handle] = new ImageViewSlot(pic, parent.fixed)
    handle
}

pub fn setImageViewSrc(handle: i64, path: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        if path == "" {
            gtk_picture_set_filename(slot.pic, 0 as *const char)
        } else {
            gtk_picture_set_filename(slot.pic, cstrFromString(path))
        }
    }
}

pub fn setImageViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        gtk_widget_set_size_request(slot.pic as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.pic as GtkWidget, x, y)
    }
}

pub fn setImageViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        gtk_widget_set_sensitive(slot.pic as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setImageViewVisible(handle: i64, visible: bool) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        gtk_widget_set_visible(slot.pic as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setImageViewTooltip(handle: i64, text: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.pic as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.pic as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnImageViewMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnImageViewMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnImageViewMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnImageViewMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnImageViewContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnImageViewMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseWheel = cb }
}

} // @extern(C)
