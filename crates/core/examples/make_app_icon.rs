//! Developer tool: draws the app icon (a Wisply face in a rounded square) as a 512 pixel picture.
//! Feed the result to `cargo tauri icon` to make every icon file the installer and Windows need.
//!
//!     cargo run -p wisply-core --example make_app_icon -- app_icon.png

use wisply_core::icons::{self, Shape};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "app_icon.png".to_string());
    let icon = icons::generate("smiling", "#8957e5", Shape::RoundedSquare).expect("the icon could not be drawn").plain;
    icon.save(&out).expect("the picture could not be saved");
    println!("wrote {out}");
}
