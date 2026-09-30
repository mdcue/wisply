from core import db, icons

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
