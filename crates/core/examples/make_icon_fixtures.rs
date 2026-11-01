//! Developer tool: redraws the reference pictures in `crates/core/tests/fixtures/icons` from the
//! current face art. Run it after changing the art or the drawing code ON PURPOSE, look at the
//! pictures, then commit them. The icon tests compare against these files.
//!
//!     cargo run -p wisply-core --example make_icon_fixtures

use wisply_core::icons::{self, Shape};

fn main() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/icons");
    let pictures = [
        ("blank", "#222222", Shape::Circle, "blank_222222_circle"),
        ("happy", "#4CAF50", Shape::Circle, "happy_4CAF50_circle"),
        ("happy", "#4CAF50", Shape::Hexagon, "happy_4CAF50_hexagon"),
        ("kiss", "#101010", Shape::RoundedSquare, "kiss_101010_rounded_square"),
        ("laughing", "#ffcc00", Shape::Diamond, "laughing_ffcc00_diamond"),
        ("nervous", "#8957e5", Shape::Hexagon, "nervous_8957e5_hexagon"),
        ("smiling", "#ff7a59", Shape::Circle, "smiling_ff7a59_circle"),
        ("surprised", "#2196f3", Shape::RoundedSquare, "surprised_2196f3_rounded_square"),
        ("tongue", "#ff7a59", Shape::Diamond, "tongue_ff7a59_diamond"),
    ];
    for (face, color, shape, file) in pictures {
        let icon = icons::generate(face, color, shape).expect("the icon could not be drawn").plain;
        icon.save(format!("{dir}/{file}.png")).expect("the picture could not be saved");
        println!("wrote {file}.png");
    }
}
