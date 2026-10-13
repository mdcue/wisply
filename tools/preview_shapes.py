import os
import tempfile
os.environ["WISPLY_DATA_DIR"] = tempfile.mkdtemp() #keeps test icons out of the real data folder

from PIL import Image
from core import icons, shapes

SHAPE_NAMES = ["circle", "rounded_square", "hexagon", "diamond"]
TILE = 120

expressions = icons.get_available_expressions()
sheet = Image.new("RGBA", (TILE * len(SHAPE_NAMES), TILE * len(expressions)), (45, 45, 45, 255))
for row, expression in enumerate(expressions):
    for col, shape_name in enumerate(SHAPE_NAMES):
        shapes.SHAPES["preview"] = shape_name
        icons.generate_bot_icon("preview", expression, "#4CAF50")
        icon = Image.open(icons.BOTS_DIR / "preview" / "icon.png").resize((TILE - 10, TILE - 10), Image.LANCZOS)
        sheet.alpha_composite(icon, (col * TILE + 5, row * TILE + 5))
    print(row + 1, expression)
sheet.save("shape_preview.png")
print("Saved shape_preview.png")