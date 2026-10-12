from core import db, icons, shapes

def set_bot_appearance(bot_id, color = None, expression = None):
    """Updates a bot's color and/or expression, then regenerates its icon files to match."""
    bot = db.get_bot(bot_id)
    if not bot:
        return
    
    current_color = bot[2]
    current_expression = bot[5]
    
    new_color = color if color is not None else current_color
    new_expression = expression if expression is not None else current_expression
    
    icon_path = icons.generate_bot_icon(bot_id, new_expression, new_color)
    
    db.update_bot(bot_id, color = new_color, expression = new_expression, icon_path = icon_path)

def ensure_icons():
    """Regenerates any wisp icon that is missing, whose stored path points somewhere
    other than the current icons folder, or whose shape no longer matches the wisp's
    assigned shape. Safe to call on every startup: wisps whose icons are already in place
    are skipped."""
    for row in db.get_all_bots():
        bot_id = row[0]
        stored_path = row[4]
        expected = icons.BOTS_DIR / bot_id / "icon.png"
        expected_badge = icons.BOTS_DIR / bot_id / "icon_badge.png"
        shape_is_current = icons.read_icon_shape(bot_id) == shapes.shape_for(bot_id)
        if stored_path == str(expected) and expected.exists() and expected_badge.exists() and shape_is_current:
            continue
        try:
            set_bot_appearance(bot_id) #no color/expression passed, keeps the wisp's current color/expression
        except (ValueError, OSError) as e:
            print(f"Could not regenerate icon for {bot_id}: {e}") #this way one single icon wouldn't soft lock starting up
