//! Icon tests. The files in `tests/fixtures/icons` were made by the Python app's own icon code
//! (Pillow); the Rust icons must look the same (edges may differ by a hair, so pictures are
//! compared with a small tolerance, not byte for byte).

use image::RgbaImage;
use wisply_core::icons::{self, IconError, Shape};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/icons");

fn reference(name: &str) -> RgbaImage {
    image::open(format!("{FIXTURES}/{name}.png")).unwrap().to_rgba8()
}

/// Colours weighted by how visible they are, so the hidden colour of see-through pixels (which
/// differs between programs and means nothing) is ignored.
fn visible(p: &image::Rgba<u8>) -> [f64; 4] {
    let a = p.0[3] as f64 / 255.0;
    [p.0[0] as f64 * a, p.0[1] as f64 * a, p.0[2] as f64 * a, p.0[3] as f64]
}

/// (average difference per channel, share of pixels that differ clearly)
fn difference(a: &RgbaImage, b: &RgbaImage) -> (f64, f64) {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut total, mut clear) = (0.0, 0u64);
    for (p, q) in a.pixels().zip(b.pixels()) {
        let (p, q) = (visible(p), visible(q));
        let diffs: Vec<f64> = (0..4).map(|i| (p[i] - q[i]).abs()).collect();
        total += diffs.iter().sum::<f64>();
        if diffs.iter().cloned().fold(0.0, f64::max) > 40.0 {
            clear += 1;
        }
    }
    let pixels = (a.width() * a.height()) as f64;
    (total / (pixels * 4.0), clear as f64 / pixels)
}

fn matches_python(expression: &str, color: &str, shape: Shape, file: &str) {
    let made = icons::generate(expression, color, shape).unwrap().plain;
    let (average, clear) = difference(&made, &reference(file));
    println!("{file}: average {average:.4}, clear {clear:.5}");
    assert!(average < 0.6, "{file}: average difference {average}");
    assert!(clear < 0.01, "{file}: {clear} of pixels clearly differ");
}

#[test]
fn circle_icons_match_python() {
    matches_python("happy", "#4CAF50", Shape::Circle, "happy_4CAF50_circle");
    matches_python("smiling", "#ff7a59", Shape::Circle, "smiling_ff7a59_circle");
}

#[test]
fn dark_colour_gets_white_lines_like_python() {
    matches_python("blank", "#222222", Shape::Circle, "blank_222222_circle");
    let icon = icons::generate("blank", "#222222", Shape::Circle).unwrap().plain;
    assert!(icon.pixels().any(|p| p.0 == [255, 255, 255, 255]));
}

#[test]
fn rounded_square_icons_match_python() {
    matches_python("surprised", "#2196f3", Shape::RoundedSquare, "surprised_2196f3_rounded_square");
    matches_python("kiss", "#101010", Shape::RoundedSquare, "kiss_101010_rounded_square");
}

#[test]
fn hexagon_icons_match_python() {
    matches_python("happy", "#4CAF50", Shape::Hexagon, "happy_4CAF50_hexagon");
    matches_python("nervous", "#8957e5", Shape::Hexagon, "nervous_8957e5_hexagon");
}

#[test]
fn diamond_icons_match_python() {
    matches_python("laughing", "#ffcc00", Shape::Diamond, "laughing_ffcc00_diamond");
    matches_python("tongue", "#ff7a59", Shape::Diamond, "tongue_ff7a59_diamond");
}

#[test]
fn the_badge_is_a_red_dot_in_the_corner_and_changes_nothing_else() {
    let icon = icons::generate("happy", "#4CAF50", Shape::Hexagon).unwrap();
    let (w, _) = icon.plain.dimensions();
    assert_eq!(icon.badge.get_pixel(w - 70, 70).0, [230, 60, 60, 255]);
    assert_eq!(icon.badge.get_pixel(w - 70 + 60, 70).0, [230, 60, 60, 255], "the dot is 66 px across from its centre");
    assert!(icon.badge.get_pixel(w - 1, 0).0[3] == 0 || icon.badge.get_pixel(w - 1, 0) == icon.plain.get_pixel(w - 1, 0));
    for (x, y, p) in icon.plain.enumerate_pixels() {
        let far = ((x as f64 - (w - 70) as f64).hypot(y as f64 - 70.0)) > 68.0;
        if far {
            assert_eq!(p, icon.badge.get_pixel(x, y));
        }
    }
}

#[test]
fn every_face_makes_every_shape() {
    // every face, each in a different shape in turn (all combinations would be slow and add little)
    let shapes = [Shape::Circle, Shape::RoundedSquare, Shape::Hexagon, Shape::Diamond];
    for (n, face) in wisply_core::faces::NAMES.iter().enumerate() {
        for shape in [shapes[n % 4], shapes[(n + 1) % 4]] {
            let icon = icons::generate(face, "#8957e5", shape).unwrap();
            assert_eq!(icon.plain.dimensions(), (512, 512));
            assert!(icon.plain.pixels().any(|p| p.0[3] > 200), "{face} {shape:?} is empty");
            // the corners of the picture stay see-through
            assert_eq!(icon.plain.get_pixel(0, 0).0[3], 0);
        }
    }
}

#[test]
fn wisps_have_their_own_shapes() {
    assert_eq!(Shape::for_wisp("game_deals"), Shape::RoundedSquare);
    assert_eq!(Shape::for_wisp("github"), Shape::Hexagon);
    assert_eq!(Shape::for_wisp("companion"), Shape::Circle);
    assert_eq!(Shape::for_wisp("something_new"), Shape::Circle);
    for s in [Shape::Circle, Shape::RoundedSquare, Shape::Hexagon, Shape::Diamond] {
        assert_eq!(Shape::from_name(s.name()), s);
    }
    assert_eq!(Shape::from_name("blob"), Shape::Circle);
}

#[test]
fn bad_input_is_an_error_not_a_crash() {
    assert!(matches!(icons::generate("grumpy", "#ffffff", Shape::Circle), Err(IconError::UnknownExpression(_))));
    for bad in ["", "#fff", "#12345g", "zzzzzz", "#1234567", "#é2345"] {
        assert!(matches!(icons::generate("happy", bad, Shape::Circle), Err(IconError::BadColor(_))), "{bad}");
    }
    assert!(icons::generate("happy", "4CAF50", Shape::Circle).is_ok(), "the # is optional");
}

#[test]
fn every_known_face_has_art_and_nothing_else_does() {
    assert_eq!(wisply_core::faces::NAMES.len(), 10);
    for name in wisply_core::faces::NAMES {
        assert!(wisply_core::faces::art(name).is_some());
    }
    assert!(wisply_core::faces::art("../happy").is_none());
}

#[test]
fn write_icons_makes_three_files_and_remembers_the_shape() {
    let dir = tempfile::tempdir().unwrap();
    let icon = icons::generate_for_wisp("github", "happy", "#8957e5").unwrap();
    icons::write_icons(dir.path(), "github", &icon, Shape::Hexagon).unwrap();
    let folder = dir.path().join("icons").join("github");
    for file in ["icon.png", "icon_badge.png", "shape.txt"] {
        assert!(folder.join(file).is_file(), "{file}");
    }
    assert!(!folder.join("icon.png.tmp").exists());
    assert_eq!(image::open(folder.join("icon.png")).unwrap().to_rgba8(), icon.plain);
    assert_eq!(image::open(folder.join("icon_badge.png")).unwrap().to_rgba8(), icon.badge);
    assert_eq!(icons::read_shape(dir.path(), "github"), Shape::Hexagon);
    assert_eq!(icons::read_shape(dir.path(), "never_made"), Shape::Circle);
}

#[test]
fn wisp_names_that_could_escape_the_folder_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let icon = icons::generate("happy", "#ffffff", Shape::Circle).unwrap();
    for bad in ["", "..", "../x", "a/b", "a\\b", "a.b", "c:"] {
        assert!(matches!(icons::write_icons(dir.path(), bad, &icon, Shape::Circle), Err(IconError::BadWispName(_))), "{bad}");
    }
    assert!(!dir.path().join("icons").exists());
    assert_eq!(icons::read_shape(dir.path(), "../x"), Shape::Circle);
}
