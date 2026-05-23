/// GTK 4 input handling — event controllers + the GdkKeyval table
/// libs/gui needs to drive its key / mouse callbacks.
///
/// GTK 4 dropped the old `key-press-event` signals on
/// `GtkWidget`. Instead you attach `GtkEventController*` subclass
/// instances to a widget; each controller emits its own signals
/// (`key-pressed` / `key-released` on `GtkEventControllerKey`,
/// `pressed` / `released` / `motion` on `GtkGestureClick` /
/// `GtkEventControllerMotion`).

use core { GtkWidget }

@extern(C, "gtk-4") {
    @handle pub struct GtkEventController {}
    @handle pub struct GtkEventControllerKey {}
    @handle pub struct GtkEventControllerMotion {}
    @handle pub struct GtkGestureClick {}

    @lib pub fn gtk_event_controller_key_new(): GtkEventControllerKey
    @lib pub fn gtk_event_controller_motion_new(): GtkEventControllerMotion
    @lib pub fn gtk_gesture_click_new(): GtkGestureClick
    /// `button`: 0 = any button, 1 = primary, 2 = middle, 3 = right.
    @lib pub fn gtk_gesture_single_set_button(
        g: GtkGestureClick, button: u32
    )

    /// Attach the controller to a widget so its signals start
    /// firing.
    @lib pub fn gtk_widget_add_controller(
        w: GtkWidget, controller: GtkEventController
    )
}

// ─── Gdk modifier mask bits ──────────────────────────────────
// Mirrors GDK 4's `GdkModifierType` — the `state` argument the
// key / button signals pass alongside the keyval.
@flags
pub enum GdkModifierType: u32 {
    shift   = 0x00000001
    lock    = 0x00000002
    control = 0x00000004
    alt     = 0x00000008
    button1 = 0x00000100
    button2 = 0x00000200
    button3 = 0x00000400
    button4 = 0x00000800
    button5 = 0x00001000
    /// Cmd on macOS / Win key on Linux. Cocoa calls this `command`,
    /// win32 calls it `meta`; libs/gui's event signal also uses
    /// "meta" so authors don't have to know which they're on.
    meta    = 0x10000000
}

// ─── Gdk keyvals ─────────────────────────────────────────────
// Only the subset libs/gui maps to W3C `KeyboardEvent.code`. Add
// more here as needed. Values from `<gdk/gdkkeysyms.h>`.
pub const GDK_KEY_a: u32         = 0x061 as u32
pub const GDK_KEY_b: u32         = 0x062 as u32
pub const GDK_KEY_c: u32         = 0x063 as u32
pub const GDK_KEY_d: u32         = 0x064 as u32
pub const GDK_KEY_e: u32         = 0x065 as u32
pub const GDK_KEY_f: u32         = 0x066 as u32
pub const GDK_KEY_g: u32         = 0x067 as u32
pub const GDK_KEY_h: u32         = 0x068 as u32
pub const GDK_KEY_i: u32         = 0x069 as u32
pub const GDK_KEY_j: u32         = 0x06a as u32
pub const GDK_KEY_k: u32         = 0x06b as u32
pub const GDK_KEY_l: u32         = 0x06c as u32
pub const GDK_KEY_m: u32         = 0x06d as u32
pub const GDK_KEY_n: u32         = 0x06e as u32
pub const GDK_KEY_o: u32         = 0x06f as u32
pub const GDK_KEY_p: u32         = 0x070 as u32
pub const GDK_KEY_q: u32         = 0x071 as u32
pub const GDK_KEY_r: u32         = 0x072 as u32
pub const GDK_KEY_s: u32         = 0x073 as u32
pub const GDK_KEY_t: u32         = 0x074 as u32
pub const GDK_KEY_u: u32         = 0x075 as u32
pub const GDK_KEY_v: u32         = 0x076 as u32
pub const GDK_KEY_w: u32         = 0x077 as u32
pub const GDK_KEY_x: u32         = 0x078 as u32
pub const GDK_KEY_y: u32         = 0x079 as u32
pub const GDK_KEY_z: u32         = 0x07a as u32
pub const GDK_KEY_0: u32         = 0x030 as u32
pub const GDK_KEY_1: u32         = 0x031 as u32
pub const GDK_KEY_2: u32         = 0x032 as u32
pub const GDK_KEY_3: u32         = 0x033 as u32
pub const GDK_KEY_4: u32         = 0x034 as u32
pub const GDK_KEY_5: u32         = 0x035 as u32
pub const GDK_KEY_6: u32         = 0x036 as u32
pub const GDK_KEY_7: u32         = 0x037 as u32
pub const GDK_KEY_8: u32         = 0x038 as u32
pub const GDK_KEY_9: u32         = 0x039 as u32
pub const GDK_KEY_Return: u32    = 0xff0d as u32
pub const GDK_KEY_Escape: u32    = 0xff1b as u32
pub const GDK_KEY_BackSpace: u32 = 0xff08 as u32
pub const GDK_KEY_Tab: u32       = 0xff09 as u32
pub const GDK_KEY_space: u32     = 0x020 as u32
pub const GDK_KEY_Left: u32      = 0xff51 as u32
pub const GDK_KEY_Up: u32        = 0xff52 as u32
pub const GDK_KEY_Right: u32     = 0xff53 as u32
pub const GDK_KEY_Down: u32      = 0xff54 as u32
