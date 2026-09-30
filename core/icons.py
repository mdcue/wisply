from pathlib import Path
from PIL import Image, ImageDraw, ImageOps

EXPRESSIONS_DIR = Path(__file__).parent.parent / "ui" / "assets" / "expressions"
BOTS_DIR = Path(__file__).parent.parent / "ui" / "assets" / "bots"

BADGE_COLOR = (230, 60, 60, 255)
BADGE_RADIUS = 44
BADGE_MARGIN = 50

def _hex_to_rgba(hex_color):
    hex_color = hex_color.lstrip("#")
    r = int(hex_color[0:2], 16)
    g = int(hex_color[2:4], 16)
    b = int(hex_color[4:6], 16)
    return (r, g, b, 255)

def _recolor(source_path, color_rgba):
    """Flood-fills the interior of a black-line-art PNG with a color, keeping the line are black on top."""
    img = Image.open(source_path).convert("RGBA")
    w, h = img.size
    px = img.load
    
    line_mark = Image.new("L", (w,h), 0)
    lm = line_mark.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a > 30 and (r + g + b) < 200:
                lm[x, y] = 255
    
    walls = Image.Ops.invert(line_mask)
    
    base = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    base.paste((255, 255, 255, 255), (0, 0, w, h), mark = walls)
    
    seed = (w // 2, h // 2)
    ImageDraw.floodfill(base, seed, color_rgba, thresh = 10)
    
    bpx = base.load()
    for y in range(h):
        for x in range(w):
            if (bpx[x, y][0], bpx[x, y][1], bpx[x, y][2] == (255, 255, 255)):
                bpx[x, y] = (0, 0, 0, 0)
    
    result = Image.alpha_composite(Image.new("RGBA", (w, h), (0, 0, 0, 0)), base)
    result = Image.alpha_composite(result, img)
    return result

def _add_badge(img):
    img = img.copy()
    draw = ImageDraw.Draw(img)
    w = img.size[0]
    cx, cy = w - BADGE_MARGIN, BADGE_MARGIN
    draw.ellipse(
        [cx - BADGE_RADIUS, cy - BADGE_RADIUS, cx + BADGE_RADIUS, cy + BADGE_MARGIN],
        fill = BADGE_COLOR
    )
    return img

def generate_bot_icon(bot_id, expression, color_hex):
    """Builds icon.png and icon_badge.png for a bot from its expression + color, and returns icon.png's path."""
    source_path = EXPRESSIONS_DIR / f"{expression}.png"
    if not source_path.exists():
        raise ValueError(f"Unknown expression: {expression}")
    
    out_dir = BOTS_DIR / bot_id
    out_dir.mkdir(parents = True, exist_ok = True)
    
    colored = _recolor(source_path, _hex_to_rgba(color_hex))
    colored.save(out_dir / "icon.png")
    
    badged = _add_badge(colored)
    badged.save(out_dir / "icon_badge.png")
    
    return str(out_dir / "icon.png")

def get_available_expressions():
    """Returns the list of expression names available to pick from, based on files present."""
    return sorted(p.stem for p in EXPRESSIONS_DIR.glob("*.png"))
