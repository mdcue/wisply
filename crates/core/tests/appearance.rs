//! Tests for keeping the wisps' icon files in step with their colour and face.

use std::path::Path;

use wisply_core::appearance;
use wisply_core::icons::{self, Shape};
use wisply_core::runtime;
use wisply_core::schema;
use wisply_core::store::{self, WispChanges};

fn icon_file(dir: &Path, wisp: &str, name: &str) -> std::path::PathBuf {
    dir.join("icons").join(wisp).join(name)
}

fn setup() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = schema::open_in_memory().unwrap();
    runtime::register_wisps(&conn).unwrap();
    (dir, conn)
}

#[test]
fn every_wisp_gets_its_icons_once_and_is_then_left_alone() {
    let (dir, conn) = setup();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 3);
    for wisp in ["game_deals", "github", "companion"] {
        for file in ["icon.png", "icon_badge.png", "shape.txt", "look.txt"] {
            assert!(icon_file(dir.path(), wisp, file).is_file(), "{wisp}/{file}");
        }
    }
    assert_eq!(icons::read_shape(dir.path(), "game_deals"), Shape::RoundedSquare);
    assert_eq!(icons::read_shape(dir.path(), "github"), Shape::Hexagon);
    assert_eq!(icons::read_shape(dir.path(), "companion"), Shape::Circle);

    let before = std::fs::metadata(icon_file(dir.path(), "github", "icon.png")).unwrap().modified().unwrap();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 0);
    let after = std::fs::metadata(icon_file(dir.path(), "github", "icon.png")).unwrap().modified().unwrap();
    assert_eq!(before, after, "untouched icons are not rewritten");
}

#[test]
fn a_changed_face_or_colour_or_a_missing_file_gets_only_that_wisp_redrawn() {
    let (dir, conn) = setup();
    appearance::ensure_icons(&conn, dir.path()).unwrap();

    let face = WispChanges { expression: Some("happy".into()), ..Default::default() };
    store::update_wisp(&conn, "companion", &face).unwrap();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 1);
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 0);

    let colour = WispChanges { color: Some("#123456".into()), ..Default::default() };
    store::update_wisp(&conn, "github", &colour).unwrap();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 1);

    std::fs::remove_file(icon_file(dir.path(), "game_deals", "icon_badge.png")).unwrap();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 1);
    assert!(icon_file(dir.path(), "game_deals", "icon_badge.png").is_file());
}

#[test]
fn a_wisp_with_a_face_that_does_not_exist_is_skipped_and_the_others_still_get_icons() {
    let (dir, conn) = setup();
    let face = WispChanges { expression: Some("grumpy".into()), ..Default::default() };
    store::update_wisp(&conn, "github", &face).unwrap();
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 2);
    assert!(!icon_file(dir.path(), "github", "icon.png").exists());
    assert!(icon_file(dir.path(), "companion", "icon.png").is_file());
}

#[test]
fn setting_the_look_saves_it_and_redraws() {
    let (dir, conn) = setup();
    appearance::set_appearance(&conn, dir.path(), "companion", Some("#00aa88"), Some("kiss")).unwrap();
    let wisp = store::get_wisp(&conn, "companion").unwrap().unwrap();
    assert_eq!((wisp.color.as_str(), wisp.expression.as_str()), ("#00aa88", "kiss"));
    let made = image::open(icon_file(dir.path(), "companion", "icon.png")).unwrap().to_rgba8();
    assert_eq!(made, icons::generate("kiss", "#00aa88", Shape::Circle).unwrap().plain);
    assert_eq!(appearance::ensure_icons(&conn, dir.path()).unwrap(), 2, "the other two still need theirs, this one is done");
}

#[test]
fn a_bad_look_is_refused_and_nothing_changes() {
    let (dir, conn) = setup();
    let before = store::get_wisp(&conn, "companion").unwrap().unwrap();
    assert!(appearance::set_appearance(&conn, dir.path(), "companion", Some("red"), None).is_err());
    assert!(appearance::set_appearance(&conn, dir.path(), "companion", None, Some("grumpy")).is_err());
    assert!(appearance::set_appearance(&conn, dir.path(), "nobody", None, None).is_err());
    assert_eq!(store::get_wisp(&conn, "companion").unwrap().unwrap(), before);
    assert!(!dir.path().join("icons").join("companion").join("icon.png").exists());
}

#[test]
fn the_badge_is_scaled_for_other_icon_sizes() {
    let small = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
    let badged = icons::with_badge(&small);
    assert_eq!(badged.dimensions(), (64, 64));
    assert_eq!(badged.get_pixel(64 - 9, 9).0, [230, 60, 60, 255], "dot near the top-right corner");
    assert_eq!(badged.get_pixel(5, 60).0, [10, 20, 30, 255], "the rest is untouched");
    assert_eq!(badged.get_pixel(64 - 9, 9 + 10).0[0], 10, "the dot's radius is 66/512 of the width, no more");
}

#[test]
fn raw_picture_bytes_get_the_same_badge() {
    let small = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
    let bytes = icons::badge_from_rgba(small.as_raw(), 64, 64).unwrap();
    assert_eq!(bytes, icons::with_badge(&small).into_raw());
    assert!(icons::badge_from_rgba(&[0; 10], 64, 64).is_none(), "wrong length is refused, not a crash");
}
