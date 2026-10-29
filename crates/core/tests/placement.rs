//! Tests for where pop-up windows are put on the screen.
use wisply_core::placement::{centered, clamp, place, Area, SCREEN_MARGIN};

const SCREEN: Area = Area { x: 0, y: 0, width: 1920, height: 1040 }; // 1080 high minus a 40 px taskbar

#[test]
fn a_window_sits_above_the_clicked_spot_with_its_left_edge_there() {
    // tray icon near the right end of the taskbar; the window is 300 x 200 and keeps an 8 px gap
    // above the taskbar (1040 - 200 - 8 = 832)
    assert_eq!(place((1500, 1040), (300, 200), SCREEN, SCREEN_MARGIN), (1500, 832));
    // anchored higher up the screen it goes exactly one window height above the anchor
    assert_eq!(place((1500, 600), (300, 200), SCREEN, SCREEN_MARGIN), (1500, 400));
}

#[test]
fn it_is_pulled_back_in_from_the_right_and_bottom_edges() {
    // too far right: the window's right edge stops 8 px short of the screen's edge
    assert_eq!(place((1900, 1040), (300, 200), SCREEN, SCREEN_MARGIN), (1920 - 300 - 8, 832));
    // anchored below the usable area: lifted so the bottom keeps its margin
    assert_eq!(place((1500, 1200), (300, 200), SCREEN, SCREEN_MARGIN), (1500, 1040 - 200 - 8));
}

#[test]
fn it_is_pushed_down_and_right_from_the_top_and_left_edges() {
    // a taskbar at the top: the anchor is near y = 0, so the window would go off the top
    assert_eq!(place((10, 30), (300, 200), SCREEN, SCREEN_MARGIN), (10, 8));
    assert_eq!(place((-50, 500), (300, 200), SCREEN, SCREEN_MARGIN), (8, 300));
}

#[test]
fn a_second_screen_to_the_left_or_above_works_with_negative_coordinates() {
    let left = Area { x: -1920, y: 0, width: 1920, height: 1040 };
    assert_eq!(place((-400, 1040), (300, 200), left, SCREEN_MARGIN), (-400, 832));
    assert_eq!(place((-5, 1040), (300, 200), left, SCREEN_MARGIN), (-308, 832));
    let above = Area { x: 0, y: -1080, width: 1920, height: 1040 };
    assert_eq!(place((100, -40), (300, 200), above, SCREEN_MARGIN), (100, -248));
}

#[test]
fn a_window_bigger_than_the_screen_keeps_its_top_left_corner_visible() {
    let small = Area { x: 0, y: 0, width: 200, height: 150 };
    assert_eq!(place((100, 100), (300, 400), small, SCREEN_MARGIN), (8, 8));
}

#[test]
fn nothing_overflows_with_silly_numbers() {
    let (x, y) = place((i32::MAX, i32::MIN), (u32::MAX, u32::MAX), SCREEN, SCREEN_MARGIN);
    assert_eq!((x, y), (8, 8));
}

#[test]
fn clamping_prefers_the_low_end_when_the_range_is_upside_down() {
    assert_eq!(clamp(5, 0, 10), 5);
    assert_eq!(clamp(-5, 0, 10), 0);
    assert_eq!(clamp(50, 0, 10), 10);
    assert_eq!(clamp(5, 8, 2), 8);
}

#[test]
fn centring() {
    assert_eq!(centered((400, 300), SCREEN), (760, 370));
    assert_eq!(centered((4000, 3000), SCREEN), (0, 0), "too big: stays at the top-left");
    assert_eq!(centered((400, 300), Area { x: -1920, y: 100, width: 1920, height: 1000 }), (-1160, 450));
}
