//! Where a small pop-up window goes: above the spot that was clicked (the tray icon), kept inside
//! the usable part of the screen. A port of the Python app's `_place` / `fit`. All sizes and
//! positions are in physical pixels, as the screen reports them.

/// The usable part of a screen (the screen minus the taskbar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Space kept between a pop-up and the edge of the screen (logical pixels; scale it first on
/// high-resolution screens).
pub const SCREEN_MARGIN: i32 = 8;

/// `value` limited to `low..=high`. If the range is upside down (a window bigger than the room it
/// has), `low` wins, so the window's top-left corner stays visible.
pub fn clamp(value: i64, low: i64, high: i64) -> i64 {
    value.min(high).max(low)
}

/// The top-left corner for a window of `size` whose bottom edge rests on `anchor`'s height and
/// whose left edge starts at `anchor`'s x, kept `margin` pixels inside `area`.
pub fn place(anchor: (i32, i32), size: (u32, u32), area: Area, margin: i32) -> (i32, i32) {
    let (anchor_x, anchor_y) = (anchor.0 as i64, anchor.1 as i64);
    let (width, height) = (size.0 as i64, size.1 as i64);
    let (area_x, area_y) = (area.x as i64, area.y as i64);
    let margin = margin as i64;

    let min_x = area_x + margin;
    let max_x = area_x + area.width as i64 - width - margin;
    let min_y = area_y + margin;
    let max_y = area_y + area.height as i64 - height - margin;
    (clamp(anchor_x, min_x, max_x) as i32, clamp(anchor_y - height, min_y, max_y) as i32)
}

/// The top-left corner that centres a window of `size` in `area` (never outside its top-left).
pub fn centered(size: (u32, u32), area: Area) -> (i32, i32) {
    let x = area.x as i64 + (area.width as i64 - size.0 as i64) / 2;
    let y = area.y as i64 + (area.height as i64 - size.1 as i64) / 2;
    (x.max(area.x as i64) as i32, y.max(area.y as i64) as i32)
}