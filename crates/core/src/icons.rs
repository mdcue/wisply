//! Draws a wisp's icon: its face, in its colour, inside its shape, plus a copy with a red badge for
//! when something is unread. This is a port of the Python app's `core/icons.py`, step by step.
//!
//! How an icon is made:
//! 1. The face picture is black line art. Dark pixels with some opacity count as "line".
//! 2. For the circle, the inside of the picture's own circle is filled with the colour (the fill is
//!    found by flooding outwards from the centre until it hits a line).
//! 3. For any other shape, the picture is split into the head outline and the face features (the
//!    outline is the line that touches the far left edge). A new outline in the wanted shape is
//!    drawn, and the features are shrunk to fit inside it.
//! 4. Lines turn white on dark colours so they don't vanish.
//! 5. The badge is a red dot in the top-right corner.
//!
//! Shapes are drawn at four times the size and shrunk, which gives smooth edges.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{GrayImage, ImageFormat, Luma, Rgba, RgbaImage};

use crate::faces;

const BADGE_COLOR: Rgba<u8> = Rgba([230, 60, 60, 255]);
const BADGE_RADIUS: f64 = 44.0;
const BADGE_MARGIN: f64 = 50.0;

/// Shapes are drawn this many times larger, then shrunk, for smooth edges.
const SUPERSAMPLE: u32 = 4;
/// Pixels kept clear around a shape so its edge isn't clipped.
const SHAPE_MARGIN: f64 = 4.0;
/// Shape outline thickness compared with the artwork's own outline.
const RING_RATIO: f64 = 0.9;
/// Share of the shape's inner radius the face is allowed to use.
const FACE_FIT: f64 = 0.80;
/// Rounded square half-width relative to the shape radius.
const SQUARE_RATIO: f64 = 0.90;
/// Corner rounding, as a share of the shape radius.
const HEX_ROUNDING: f64 = 0.12;
const DIAMOND_ROUNDING: f64 = 0.18;

/// The outline an icon is drawn in. The face is the same for every wisp; only the shape changes, so
/// wisps stay easy to tell apart even when their colours are similar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    RoundedSquare,
    Hexagon,
    Diamond,
}

impl Shape {
    pub fn name(self) -> &'static str {
        match self {
            Shape::Circle => "circle",
            Shape::RoundedSquare => "rounded_square",
            Shape::Hexagon => "hexagon",
            Shape::Diamond => "diamond",
        }
    }

    /// A shape from its name; anything unknown is the circle.
    pub fn from_name(name: &str) -> Shape {
        match name.trim() {
            "rounded_square" => Shape::RoundedSquare,
            "hexagon" => Shape::Hexagon,
            "diamond" => Shape::Diamond,
            _ => Shape::Circle,
        }
    }

    /// Which shape each wisp uses. Wisps not listed use the circle.
    pub fn for_wisp(wisp_id: &str) -> Shape {
        match wisp_id {
            "game_deals" => Shape::RoundedSquare,
            "github" => Shape::Hexagon,
            _ => Shape::Circle,
        }
    }

    /// Radius of the biggest circle that fits inside the shape, as a share of the shape's radius.
    fn inradius_ratio(self) -> f64 {
        use std::f64::consts::PI;
        match self {
            Shape::Circle => 1.0,
            Shape::RoundedSquare => SQUARE_RATIO,
            Shape::Hexagon => (1.0 - HEX_ROUNDING) * (PI / 6.0).cos() + HEX_ROUNDING,
            Shape::Diamond => (1.0 - DIAMOND_ROUNDING) * (PI / 4.0).cos() + DIAMOND_ROUNDING,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error("Unknown expression: {0}")]
    UnknownExpression(String),
    #[error("{0:?} is not a colour like #4CAF50")]
    BadColor(String),
    #[error("the face picture could not be read: {0}")]
    Picture(#[from] image::ImageError),
    #[error("{0:?} is not a usable wisp name for a folder")]
    BadWispName(String),
    #[error("file problem: {0}")]
    Io(#[from] std::io::Error),
}

/// A wisp's two icons: the plain one and the one with the red unread badge.
pub struct Icon {
    pub plain: RgbaImage,
    pub badge: RgbaImage,
}

/// Makes the icon for a face, a colour (`#rrggbb`) and a shape.
pub fn generate(expression: &str, color_hex: &str, shape: Shape) -> Result<Icon, IconError> {
    let bytes = faces::art(expression).ok_or_else(|| IconError::UnknownExpression(expression.to_string()))?;
    let art = image::load_from_memory_with_format(bytes, ImageFormat::Png)?.to_rgba8();
    let color = parse_color(color_hex)?;
    let plain = match shape {
        Shape::Circle => None,
        _ => shaped(&art, color, shape),
    }
    .unwrap_or_else(|| recolor(&art, color));
    let badge = add_badge(&plain);
    Ok(Icon { plain, badge })
}

/// Makes the icon for a wisp, using the shape that wisp always has.
pub fn generate_for_wisp(wisp_id: &str, expression: &str, color_hex: &str) -> Result<Icon, IconError> {
    generate(expression, color_hex, Shape::for_wisp(wisp_id))
}

fn parse_color(text: &str) -> Result<Rgba<u8>, IconError> {
    let bad = || IconError::BadColor(text.to_string());
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(bad());
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map_err(|_| bad());
    Ok(Rgba([channel(0)?, channel(2)?, channel(4)?, 255]))
}

/// Black lines on dark colours would vanish, so those get white lines.
fn outline_color(color: Rgba<u8>) -> Rgba<u8> {
    let [r, g, b, _] = color.0;
    let luminance = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0;
    if luminance < 0.3 {
        Rgba([255, 255, 255, 255])
    } else {
        Rgba([15, 15, 15, 255])
    }
}

// --- finding the lines of the artwork ---

/// 255 wherever the artwork has a dark line, 0 elsewhere.
fn line_mask(art: &RgbaImage) -> GrayImage {
    GrayImage::from_fn(art.width(), art.height(), |x, y| {
        let [r, g, b, a] = art.get_pixel(x, y).0;
        let darkness = (r as u16 + g as u16 + b as u16).min(255);
        Luma([if darkness < 200 && a > 30 { 255 } else { 0 }])
    })
}

/// Paints `value` over the area around `seed` made of pixels whose value is within `thresh` of the
/// seed's (neighbours up, down, left, right).
fn flood_fill(image: &mut GrayImage, seed: (u32, u32), value: u8, thresh: u8) {
    let (w, h) = image.dimensions();
    let background = image.get_pixel(seed.0, seed.1).0[0];
    let mut seen = vec![false; (w * h) as usize];
    let mut stack = vec![seed];
    seen[(seed.1 * w + seed.0) as usize] = true;
    while let Some((x, y)) = stack.pop() {
        image.put_pixel(x, y, Luma([value]));
        let around = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
        for (nx, ny) in around {
            if nx >= w || ny >= h || seen[(ny * w + nx) as usize] {
                continue;
            }
            seen[(ny * w + nx) as usize] = true;
            if image.get_pixel(nx, ny).0[0].abs_diff(background) <= thresh {
                stack.push((nx, ny));
            }
        }
    }
}

/// The smallest box around every non-zero pixel: (left, top, right, bottom), the last two not
/// included. `None` if the picture is empty.
fn bounding_box(mask: &GrayImage) -> Option<(u32, u32, u32, u32)> {
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, pixel) in mask.enumerate_pixels() {
        if pixel.0[0] != 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (right > 0).then_some((left, top, right, bottom))
}

/// Splits the line art into the head outline and the face features. The outline is the connected
/// line that touches the leftmost edge of the artwork (the head is always the widest thing); every
/// other line is a feature. `None` if the artwork has no lines.
fn split_outline(lines: &GrayImage) -> Option<(GrayImage, GrayImage)> {
    let (left, _, _, _) = bounding_box(lines)?;
    let seed_y = (0..lines.height()).find(|&y| lines.get_pixel(left, y).0[0] != 0)?;
    let mut marked = lines.clone();
    flood_fill(&mut marked, (left, seed_y), 128, 0);
    let pick = |wanted: u8| GrayImage::from_fn(marked.width(), marked.height(), |x, y| {
        Luma([if marked.get_pixel(x, y).0[0] == wanted { 255 } else { 0 }])
    });
    Some((pick(128), pick(255)))
}

/// Thickness of the artwork's outline, measured across its left side.
fn stroke_width(outline: &GrayImage, bounds: (u32, u32, u32, u32)) -> u32 {
    let y = (bounds.1 + bounds.3) / 2;
    let mut run = 0;
    let mut x = bounds.0;
    while x < bounds.2 && outline.get_pixel(x, y).0[0] != 0 {
        run += 1;
        x += 1;
    }
    run.max(8)
}

/// Distance from the head centre to the farthest face-feature pixel.
fn feature_reach(features: &GrayImage, cx: f64, cy: f64) -> f64 {
    features
        .enumerate_pixels()
        .filter(|(_, _, p)| p.0[0] != 0)
        .map(|(x, y, _)| (x as f64 - cx).hypot(y as f64 - cy))
        .fold(0.0, f64::max)
}

// --- drawing ---

/// Lays `top` over `bottom` the usual way (both have straight, not pre-multiplied, colours).
fn alpha_composite(bottom: &mut RgbaImage, top: &RgbaImage) {
    for (b, t) in bottom.pixels_mut().zip(top.pixels()) {
        let ta = t.0[3] as f64 / 255.0;
        let ba = b.0[3] as f64 / 255.0;
        let out_a = ta + ba * (1.0 - ta);
        if out_a <= 0.0 {
            *b = Rgba([0, 0, 0, 0]);
            continue;
        }
        let mix = |i: usize| {
            let value = (t.0[i] as f64 * ta + b.0[i] as f64 * ba * (1.0 - ta)) / out_a;
            value.round().clamp(0.0, 255.0) as u8
        };
        let alpha = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        *b = Rgba([mix(0), mix(1), mix(2), alpha]);
    }
}

/// A layer of one colour whose see-through-ness is `mask` (255 = solid).
fn layer(color: Rgba<u8>, mask: &GrayImage) -> RgbaImage {
    RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
        Rgba([color.0[0], color.0[1], color.0[2], mask.get_pixel(x, y).0[0]])
    })
}

/// The picture's own circle, filled with the colour.
fn recolor(art: &RgbaImage, color: Rgba<u8>) -> RgbaImage {
    let (w, h) = art.dimensions();
    let lines = line_mask(art);
    let mut fill_mask = GrayImage::from_fn(w, h, |x, y| Luma([255 - lines.get_pixel(x, y).0[0]]));
    flood_fill(&mut fill_mask, (w / 2, h / 2), 128, 10);
    let interior = GrayImage::from_fn(w, h, |x, y| Luma([if fill_mask.get_pixel(x, y).0[0] == 128 { 255 } else { 0 }]));
    let mut result = layer(color, &interior);
    alpha_composite(&mut result, &layer(outline_color(color), &lines));
    result
}

/// A `size` x `size` mask (255 inside) of the shape, centred, reaching `radius` from the centre.
fn shape_mask(shape: Shape, size: u32, radius: f64) -> GrayImage {
    let c = size as f64 / 2.0;
    let inside: Box<dyn Fn(f64, f64) -> bool> = match shape {
        Shape::Circle => Box::new(move |dx, dy| dx * dx + dy * dy <= radius * radius),
        Shape::RoundedSquare => {
            let half = radius * SQUARE_RATIO;
            let corner = half * 0.45;
            Box::new(move |dx, dy| {
                let (qx, qy) = (dx.abs() - (half - corner), dy.abs() - (half - corner));
                qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - corner <= 0.0
            })
        }
        Shape::Hexagon => rounded_polygon(6, radius, 0.0, radius * HEX_ROUNDING),
        Shape::Diamond => rounded_polygon(4, radius, -std::f64::consts::FRAC_PI_2, radius * DIAMOND_ROUNDING),
    };
    GrayImage::from_fn(size, size, |x, y| Luma([if inside(x as f64 - c, y as f64 - c) { 255 } else { 0 }]))
}

/// A regular polygon with rounded corners that reaches `radius` from the centre at its corners.
/// (Its straight sides are the polygon pulled in by `rounding`, then every point within `rounding`
/// of that smaller polygon is included.)
fn rounded_polygon(sides: usize, radius: f64, rotation: f64, rounding: f64) -> Box<dyn Fn(f64, f64) -> bool> {
    let inset = radius - rounding;
    let corners: Vec<(f64, f64)> = (0..sides)
        .map(|i| {
            let angle = rotation + 2.0 * std::f64::consts::PI * i as f64 / sides as f64;
            (inset * angle.cos(), inset * angle.sin())
        })
        .collect();
    Box::new(move |px, py| {
        let mut inside = true;
        let mut nearest = f64::MAX;
        for i in 0..corners.len() {
            let (ax, ay) = corners[i];
            let (bx, by) = corners[(i + 1) % corners.len()];
            if (bx - ax) * (py - ay) - (by - ay) * (px - ax) < 0.0 {
                inside = false;
            }
            let (ex, ey) = (bx - ax, by - ay);
            let along = (((px - ax) * ex + (py - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
            nearest = nearest.min((px - (ax + along * ex)).hypot(py - (ay + along * ey)));
        }
        inside || nearest <= rounding
    })
}

/// The face in a new shape. `None` if the artwork can't be split into outline and face, so the
/// caller can fall back to the plain circle.
fn shaped(art: &RgbaImage, color: Rgba<u8>, shape: Shape) -> Option<RgbaImage> {
    let (w, h) = art.dimensions();
    let (outline, features) = split_outline(&line_mask(art))?;
    let out_box = bounding_box(&outline)?;
    bounding_box(&features)?;

    let cx = (out_box.0 + out_box.2) as f64 / 2.0;
    let cy = (out_box.1 + out_box.3) as f64 / 2.0;
    let ring = stroke_width(&outline, out_box) as f64 * RING_RATIO;
    let reach = feature_reach(&features, cx, cy);

    let big = w * SUPERSAMPLE;
    let scale_up = SUPERSAMPLE as f64;
    let radius_out = (w as f64 / 2.0 - SHAPE_MARGIN) * scale_up;
    let in_out = radius_out * shape.inradius_ratio();
    let in_ring = in_out - ring * scale_up;
    let radius_in = radius_out * (in_ring / in_out);

    let shrink = |mask: GrayImage| imageops::resize(&mask, w, h, FilterType::Lanczos3);
    let outer = shrink(shape_mask(shape, big, radius_out));
    let inner = shrink(shape_mask(shape, big, radius_in));
    let ring_mask = GrayImage::from_fn(w, h, |x, y| {
        Luma([outer.get_pixel(x, y).0[0].saturating_sub(inner.get_pixel(x, y).0[0])])
    });

    let scale = if reach > 0.0 { (FACE_FIT * (in_ring / scale_up) / reach).min(1.0) } else { 1.0 };
    if scale <= 0.0 {
        return None;
    }
    let (sw, sh) = ((w as f64 * scale).round_ties_even() as u32, (h as f64 * scale).round_ties_even() as u32);
    if sw == 0 || sh == 0 {
        return None;
    }
    let scaled = imageops::resize(&features, sw, sh, FilterType::Lanczos3);
    let mut face_mask = GrayImage::new(w, h);
    let at_x = (w as f64 / 2.0 - cx * scale).round_ties_even() as i64;
    let at_y = (h as f64 / 2.0 - cy * scale).round_ties_even() as i64;
    for (x, y, pixel) in scaled.enumerate_pixels() {
        let (tx, ty) = (x as i64 + at_x, y as i64 + at_y);
        if tx >= 0 && ty >= 0 && (tx as u32) < w && (ty as u32) < h {
            face_mask.put_pixel(tx as u32, ty as u32, *pixel);
        }
    }

    let line = outline_color(color);
    let mut result = layer(color, &outer);
    alpha_composite(&mut result, &layer(line, &ring_mask));
    alpha_composite(&mut result, &layer(line, &face_mask));
    Some(result)
}

/// A copy with the red unread dot in the top-right corner. The dot is sized for a 512 pixel icon
/// and scaled for any other size.
pub fn with_badge(icon: &RgbaImage) -> RgbaImage {
    add_badge(icon)
}

/// The same as `with_badge` for raw picture bytes (4 per pixel: red, green, blue, see-through, row
/// by row). `None` if the length doesn't fit the width and height.
pub fn badge_from_rgba(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    RgbaImage::from_raw(width, height, rgba.to_vec()).map(|icon| add_badge(&icon).into_raw())
}

fn add_badge(icon: &RgbaImage) -> RgbaImage {
    let mut badged = icon.clone();
    let scale = icon.width() as f64 / 512.0;
    let (radius, margin) = (BADGE_RADIUS * scale, BADGE_MARGIN * scale);
    let (cx, cy) = (icon.width() as f64 - margin, margin);
    for (x, y, pixel) in badged.enumerate_pixels_mut() {
        if (x as f64 - cx).hypot(y as f64 - cy) <= radius {
            *pixel = BADGE_COLOR;
        }
    }
    badged
}

// --- files ---

/// The icon as a PNG file's bytes.
pub fn to_png(image: &RgbaImage) -> Result<Vec<u8>, IconError> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

/// Where a wisp's icon files live: `<data folder>/icons/<wisp>`.
pub fn folder(data_dir: &Path, wisp_id: &str) -> Result<std::path::PathBuf, IconError> {
    let safe = !wisp_id.is_empty() && wisp_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !safe {
        return Err(IconError::BadWispName(wisp_id.to_string()));
    }
    Ok(data_dir.join("icons").join(wisp_id))
}

/// Writes `icon.png`, `icon_badge.png` and `shape.txt` (which remembers the shape the icon was
/// built with) for a wisp. Each file is written whole or not at all.
pub fn write_icons(data_dir: &Path, wisp_id: &str, icon: &Icon, shape: Shape) -> Result<(), IconError> {
    let dir = folder(data_dir, wisp_id)?;
    fs::create_dir_all(&dir)?;
    let write = |name: &str, bytes: &[u8]| -> Result<(), IconError> {
        let temp = dir.join(format!("{name}.tmp"));
        fs::write(&temp, bytes)?;
        fs::rename(&temp, dir.join(name))?;
        Ok(())
    };
    write("icon.png", &to_png(&icon.plain)?)?;
    write("icon_badge.png", &to_png(&icon.badge)?)?;
    write("shape.txt", shape.name().as_bytes())
}

/// The shape a wisp's icon was last built with. Icons with no record count as circles.
pub fn read_shape(data_dir: &Path, wisp_id: &str) -> Shape {
    folder(data_dir, wisp_id)
        .ok()
        .and_then(|dir| fs::read_to_string(dir.join("shape.txt")).ok())
        .map(|text| Shape::from_name(&text))
        .unwrap_or(Shape::Circle)
}
