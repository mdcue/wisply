//! The face pictures (black line art on a transparent background, 512 x 512) that every wisp icon
//! is drawn from. They are built into the program, so icons can be made without any file lookups.
//! The same pictures are what the Hub shows, from `ui/assets/expressions`.

macro_rules! faces {
    ($($name:literal),* $(,)?) => {
        /// The names of the faces, in alphabetical order.
        pub const NAMES: &[&str] = &[$($name),*];

        /// The PNG file of a face, or `None` if there is no such face.
        pub fn art(name: &str) -> Option<&'static [u8]> {
            match name {
                $($name => Some(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"), "/../../ui/assets/expressions/", $name, ".png"
                ))),)*
                _ => None,
            }
        }
    };
}

faces!("blank", "content", "happy", "kiss", "laughing", "nervous", "shout", "smiling", "surprised", "tongue");
