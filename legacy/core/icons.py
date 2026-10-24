import math

from PIL import Image, ImageChops, ImageDraw, ImageOps

from core import shapes
from core.paths import BASE_DIR, DATA_DIR

EXPRESSIONS_DIR = BASE_DIR / "ui" / "assets" / "expressions"
BOTS_DIR = DATA_DIR / "ui" / "assets" / "bots"

BADGE_COLOR = (230, 60, 60, 255)
BADGE_RADIUS = 44
BADGE_MARGIN = 50

SHAPE_FILE = "shape.txt" #this is to remember what shape an icon was last built with
SUPERSAMPLE = 4 #shapes are drawn 4 times larger, then shrunk, this will give us smooth edges
SHAPE_MARGIN = 4 #px kept clear around a shape so its edge isn't clipped
RING_RATIO = 0.9 #shape outline thickness compared with the artwork's own outline
FACE_FIT = 0.80 #shape of the shape's inner radius the face is allowed to use
SQUARE_RATIO = 0.90 #rounded square half-width relative to the shape radius
HEX_ROUNDING = 0.12 #corner rounding, as a share of the shape radius
DIAMOND_ROUNDING = 0.18

def _hex_to_rgba(hex_color):
    hex_color = hex_color.lstrip("#")
    r = int(hex_color[0:2], 16)
    g = int(hex_color[2:4], 16)
    b = int(hex_color[4:6], 16)
    return (r, g, b, 255)

def _recolor(source_path, color_rgba):
    """Fills the inside of a black-line-art PNG with a color. The outline switches to
    white when the color is dark, so a near-black fill doesn't swallow the line-art."""
    img = Image.open(source_path).convert("RGBA")
    w, h = img.size
    lines = _line_mask(img)
    
    fill_mask = ImageOps.invert(lines)
    ImageDraw.floodfill(fill_mask, (w // 2, h // 2), 128, thresh = 10)
    interior = fill_mask.point(lambda v: 255 if v == 128 else 0)
    
    fill_layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    fill_layer.paste(color_rgba, mask = interior)
    outline_layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    outline_layer.paste(_outline_rgba(color_rgba), mask = lines)
    return Image.alpha_composite(fill_layer, outline_layer)

def _outline_rgba(color_rgba):
    """Same rule _recolor uses: white lines on dark colors, near-black lines otherwise."""
    r, g, b = color_rgba[0], color_rgba[1], color_rgba[2]
    luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
    return (255, 255, 255, 255) if luminance < 0.3 else (15, 15, 15, 255)

def _line_mask(img):
    """255 wherever the artwork has a dark line, 0 elsewhere."""
    r, g, b, a = img.split()
    dark = ImageChops.add(ImageChops.add(r, g), b).point(lambda v:255 if v < 200 else 0)
    solid = a.point(lambda v: 255 if v > 30 else 0)
    return ImageChops.multiply(dark, solid)

def _split_outline(line):
    """Splits the line art into the head outline and the face features. The outline is
    the connected line that touches the leftmost edge of the artwork (the head is always
    the widest thing); every other line is a feature. Returns (outline, features), or
    None if the artwork has no lines."""
    box = line.getbbox()
    if box is None:
        return None
    lp = line.load()
    left = box[0]
    seed = next(((left, y) for y in range(line.height) if lp[left, y]), None)
    if seed is None:
        return None
    marked = line.copy()
    ImageDraw.floodfill(marked, seed, 128, thresh = 0)
    outline = marked.point(lambda v: 255 if v == 128 else 0)
    features = marked.point(lambda v: 255 if v == 255 else 0)
    return outline, features

def _stroke_width(outline, box):
    """Thickness of the artwork's outline, measured across its left side."""
    y = (box[1] + box[3]) // 2
    op = outline.load()
    x = box[0]
    run = 0
    while x < box[2] and op[x, y]:
        run += 1
        x += 1
    return max(run, 8)

def _feature_reach(features, cx, cy):
    """Distance from the head centre to the farthest face-feature pixel."""
    box = features.getbbox()
    fp = features.load()
    far = 0.0
    for y in range(box[1], box[3]):
        for x in range(box[0], box[2]):
            if fp[x, y]:
                far = max(far, math.hypot(x - cx, y - cy))
    return far

def _rounded_polygon(draw, centre, sides, radius, rotation, rounding):
    """Draws a filled regular polygon with rounded corners that reaches `radius` px from
    the centre at its corners."""
    inset = radius - rounding
    points = [
        (centre + inset * math.cos(rotation + 2 * math.pi * i / sides),
        centre + inset * math.sin(rotation + 2 * math.pi * i / sides))
        for i in range(sides)
    ]
    draw.polygon(points, fill = 255)
    draw.line(points + [points[0]], fill = 255, width = int(rounding * 2), joint = "curve")
    for x, y in points:
        draw.ellipse([x - rounding, y - rounding, x + rounding, y + rounding], fill = 255)

def _shape_mask(shape, size, radius):
    """A size x size greyscale mask (255 inside) of the named shape."""
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    c = size / 2
    if shape == "rounded_square":
        half = radius * SQUARE_RATIO
        draw.rounded_rectangle([c - half, c- half, c + half, c + half], radius = half * 0.45, fill = 255)
    elif shape == "hexagon":
        _rounded_polygon(draw, c, 6, radius, 0, radius * HEX_ROUNDING)
    elif shape == "diamond":
        _rounded_polygon(draw, c, 4, radius, -math.pi / 2, radius * DIAMOND_ROUNDING)
    else:
        draw.ellipse([c - radius, c - radius, c + radius, c + radius], fill = 255)
    return mask

def _inradius_ratio(shape):
    """Radius of the biggest circle that fits inside the shape, as a shape of the shape radius."""
    if shape == "rounded_square":
        return SQUARE_RATIO
    if shape == "hexagon":
        return (1 - HEX_ROUNDING) * math.cos(math.pi / 6) + HEX_ROUNDING
    if shape == "diamond":
        return (1 - DIAMOND_ROUNDING) * math.cos(math.pi / 4) + DIAMOND_ROUNDING
    return 1.0

def _shaped(source_path, color_rgba, shape):
    """Builds an icon with the same face as the source artwork, but inside `shape` instead
    of the artwork's own circle. Returns None if the artwork can't be split into outline
    and face, so the caller can fall back to the plain circle."""
    img = Image.open(source_path).convert("RGBA")
    w, h = img.size
    parts = _split_outline(_line_mask(img))
    if parts is None:
        return None
    outline, features = parts
    out_box = outline.getbbox()
    if features.getbbox() is None or out_box is None:
        return None
    
    cx = (out_box[0] + out_box[2]) / 2
    cy = (out_box[1] + out_box[3]) / 2
    ring = _stroke_width(outline, out_box) * RING_RATIO
    reach = _feature_reach(features, cx, cy)
    
    big = w * SUPERSAMPLE
    radius_out = (w / 2 - SHAPE_MARGIN) * SUPERSAMPLE
    in_out = radius_out * _inradius_ratio(shape)
    in_ring = in_out - ring * SUPERSAMPLE
    radius_in = radius_out * (in_ring / in_out)
    
    outer = _shape_mask(shape, big, radius_out).resize((w, h), Image.LANCZOS)
    inner = _shape_mask(shape, big, radius_in).resize((w, h), Image.LANCZOS)
    ring_mask = ImageChops.subtract(outer, inner)
    
    scale = min(1.0, FACE_FIT * (in_ring / SUPERSAMPLE) / reach)
    scaled = features.resize((round(w * scale), round(h * scale)), Image.LANCZOS)
    face_mask = Image.new("L", (w, h), 0)
    face_mask.paste(scaled, (round(w / 2 - cx * scale), round(h / 2 - cy * scale)))
    
    line_rgba = _outline_rgba(color_rgba)
    fill_layer = Image.new("RGBA", (w, h), color_rgba)
    fill_layer.putalpha(outer)
    ring_layer = Image.new("RGBA", (w, h), line_rgba)
    ring_layer.putalpha(ring_mask)
    face_layer = Image.new("RGBA", (w, h), line_rgba)
    face_layer.putalpha(face_mask)
    
    return Image.alpha_composite(Image.alpha_composite(fill_layer, ring_layer), face_layer)

def _add_badge(img):
    img = img.copy()
    draw = ImageDraw.Draw(img)
    w = img.size[0]
    cx, cy = w - BADGE_MARGIN, BADGE_MARGIN
    draw.ellipse(
        [cx - BADGE_RADIUS, cy - BADGE_RADIUS, cx + BADGE_RADIUS, cy + BADGE_RADIUS],
        fill = BADGE_COLOR
    )
    return img

def generate_bot_icon(bot_id, expression, color_hex):
    """Builds icon.png and icon_badge.png for a bot from its expression + color, and returns icon.png's path."""
    source_path = EXPRESSIONS_DIR / f"{expression}.png"
    if not source_path.exists():
        raise ValueError(f"Unknown expression: {expression}")
    
    out_dir  = BOTS_DIR / bot_id
    out_dir.mkdir(parents = True, exist_ok = True)
    
    color_rgba = _hex_to_rgba(color_hex)
    shape = shapes.shape_for(bot_id)
    colored = _shaped(source_path, color_rgba, shape) if shape != "circle" else None
    if colored is None:
        colored = _recolor(source_path, color_rgba)
    colored.save(out_dir / "icon.png")
    
    badged = _add_badge(colored)
    badged.save(out_dir / "icon_badge.png")
    
    (out_dir / SHAPE_FILE).write_text(shape)
    
    return str(out_dir / "icon.png")

def get_available_expressions():
    """Returns the list of expression names available to pick from, based on files present"""
    return sorted(p.stem for p in EXPRESSIONS_DIR.glob("*.png"))

def read_icon_shape(bot_id):
    """The shape a wisp's icon was last built with. Icons made before shapes existed have
    no record and were all circles."""
    try:
        return (BOTS_DIR / bot_id / SHAPE_FILE).read_text().strip()
    except OSError:
        return "circle"