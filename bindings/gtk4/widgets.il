/// Widget bindings — one `@handle` per GTK widget subclass libs/gui
/// uses, plus the constructor / setter functions for each.
///
/// GTK 4 doesn't expose absolute positioning on most containers —
/// we host every child inside a `GtkFixed` so the cross-platform
/// `(x, y, width, height)` layout maps cleanly.

use core { GtkWidget }

@extern(C, "gtk-4") {
    // ─── Container ───────────────────────────────────────────

    /// `GtkFixed` — children placed by absolute (x, y) coords.
    /// Modern GTK style prefers `GtkBox` / `GtkGrid`, but the
    /// cross-platform libs/gui surface speaks pixels, so we use
    /// this for the contentView equivalent.
    @handle pub struct GtkFixed {}
    @lib pub fn gtk_fixed_new(): GtkFixed
    @lib pub fn gtk_fixed_put(
        f: GtkFixed, child: GtkWidget, x: f64, y: f64
    )
    @lib pub fn gtk_fixed_move(
        f: GtkFixed, child: GtkWidget, x: f64, y: f64
    )
    @lib pub fn gtk_fixed_remove(f: GtkFixed, child: GtkWidget)

    // ─── Button ──────────────────────────────────────────────

    @handle pub struct GtkButton {}
    @lib pub fn gtk_button_new_with_label(label: *const char): GtkButton
    @lib pub fn gtk_button_set_label(b: GtkButton, label: *const char)
    @lib pub fn gtk_button_get_label(b: GtkButton): *const char

    // ─── Label ───────────────────────────────────────────────

    @handle pub struct GtkLabel {}
    @lib pub fn gtk_label_new(text: *const char): GtkLabel
    @lib pub fn gtk_label_set_text(l: GtkLabel, text: *const char)
    @lib pub fn gtk_label_get_text(l: GtkLabel): *const char

    // ─── Entry (single-line text input) ──────────────────────

    /// `GtkEntry` holds the actual editing buffer through a
    /// nested `GtkEntryBuffer`. Use `gtk_editable_get_text` /
    /// `gtk_editable_set_text` for the simple string API.
    @handle pub struct GtkEntry {}
    @lib pub fn gtk_entry_new(): GtkEntry
    @lib pub fn gtk_editable_set_text(
        entry: GtkEntry, text: *const char
    )
    @lib pub fn gtk_editable_get_text(entry: GtkEntry): *const char

    // ─── Check / radio buttons ───────────────────────────────

    /// `GtkCheckButton` covers both checkboxes and radio buttons
    /// in GTK 4. For radio groups call `gtk_check_button_set_group`
    /// on every member, passing the same "leader" button.
    @handle pub struct GtkCheckButton {}
    @lib pub fn gtk_check_button_new_with_label(
        label: *const char
    ): GtkCheckButton
    @lib pub fn gtk_check_button_set_label(
        b: GtkCheckButton, label: *const char
    )
    @lib pub fn gtk_check_button_set_active(
        b: GtkCheckButton, active: i32
    )
    @lib pub fn gtk_check_button_get_active(b: GtkCheckButton): i32
    @lib pub fn gtk_check_button_set_group(
        b: GtkCheckButton, group: GtkCheckButton
    )

    // ─── Scale (slider) ──────────────────────────────────────

    @handle pub struct GtkScale {}

    /// `orientation`: 0 = horizontal, 1 = vertical (GtkOrientation).
    @lib pub fn gtk_scale_new_with_range(
        orientation: i32, min: f64, max: f64, step: f64
    ): GtkScale
    @lib pub fn gtk_range_set_value(s: GtkScale, value: f64)
    @lib pub fn gtk_range_get_value(s: GtkScale): f64
    @lib pub fn gtk_range_set_range(s: GtkScale, min: f64, max: f64)

    // ─── Progress bar ────────────────────────────────────────

    @handle pub struct GtkProgressBar {}
    @lib pub fn gtk_progress_bar_new(): GtkProgressBar
    /// `fraction` is the filled portion in `[0.0, 1.0]`.
    @lib pub fn gtk_progress_bar_set_fraction(
        p: GtkProgressBar, fraction: f64
    )
    @lib pub fn gtk_progress_bar_get_fraction(p: GtkProgressBar): f64

    // ─── Combo (drop-down) ───────────────────────────────────

    /// `GtkDropDown` is the modern combo equivalent. For simple
    /// string entries `gtk_drop_down_new_from_strings` takes a
    /// NULL-terminated `*const *const char` and builds the model
    /// in one call.
    @handle pub struct GtkDropDown {}
    @lib pub fn gtk_drop_down_new_from_strings(
        strings: *const *const char
    ): GtkDropDown
    @lib pub fn gtk_drop_down_set_selected(d: GtkDropDown, index: u32)
    @lib pub fn gtk_drop_down_get_selected(d: GtkDropDown): u32

    /// `GtkComboBoxText` — deprecated in GTK 4.10 but the only
    /// drop-down that supports dynamic `append` / `remove_all`
    /// without writing a GtkStringList model. libs/gui uses it
    /// for now; swap to GtkDropDown + GtkStringList when the
    /// binding grows out.
    @handle pub struct GtkComboBoxText {}
    @lib pub fn gtk_combo_box_text_new(): GtkComboBoxText
    @lib pub fn gtk_combo_box_text_append_text(
        c: GtkComboBoxText, text: *const char
    )
    @lib pub fn gtk_combo_box_text_remove_all(c: GtkComboBoxText)
    @lib pub fn gtk_combo_box_set_active(c: GtkComboBoxText, index: i32)
    @lib pub fn gtk_combo_box_get_active(c: GtkComboBoxText): i32

    // ─── ListBox (selectable list) ───────────────────────────

    @handle pub struct GtkListBox {}
    @handle pub struct GtkListBoxRow {}
    @lib pub fn gtk_list_box_new(): GtkListBox
    @lib pub fn gtk_list_box_append(lb: GtkListBox, child: GtkWidget)
    @lib pub fn gtk_list_box_remove(lb: GtkListBox, child: GtkWidget)
    @lib pub fn gtk_list_box_get_selected_row(lb: GtkListBox): GtkListBoxRow
    @lib pub fn gtk_list_box_row_get_index(row: GtkListBoxRow): i32
    @lib pub fn gtk_list_box_select_row(lb: GtkListBox, row: GtkListBoxRow)

    /// `GtkScrolledWindow` wraps a child so it scrolls when the
    /// content overflows. libs/gui's ListBox typically wraps the
    /// GtkListBox in one of these.
    @handle pub struct GtkScrolledWindow {}
    @lib pub fn gtk_scrolled_window_new(): GtkScrolledWindow
    @lib pub fn gtk_scrolled_window_set_child(
        sw: GtkScrolledWindow, child: GtkWidget
    )

    // ─── Frame (GroupBox) ────────────────────────────────────

    @handle pub struct GtkFrame {}
    @lib pub fn gtk_frame_new(label: *const char): GtkFrame
    @lib pub fn gtk_frame_set_label(f: GtkFrame, label: *const char)
    @lib pub fn gtk_frame_set_child(f: GtkFrame, child: GtkWidget)
}
