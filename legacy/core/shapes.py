"""Which outline shape each wisp uses. The face (expression) is the same for every wisp;
only the shape around it changes, so wisps stay easy to tell apart even when their
colours are similar. Wisps not listed here use the default circle."""

DEFAULT_SHAPE = "circle"

SHAPES = {
    "game_deals": "rounded_square",
    "github": "hexagon",
    "companion": "circle",
}

def shape_for(bot_id):
    return SHAPES.get(bot_id, DEFAULT_SHAPE)