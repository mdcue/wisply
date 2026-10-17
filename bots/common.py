from core import db, appearance

#sent with every web request so the sites we call can tell who is asking. Replace the
#contact before release; this is the only place it lives.
USER_AGENT = "wisply/1.0 (+https://github.com/mdcue/wisply)"

def register_wisp(bot_id, name, color, channels, expression = "smiling"):
    """Adds a wisp and its channels. `channels` is a list of (id, title, description, color),
    shown in that order. A wisp's look is only set the first time it is added, so changes
    the user makes are kept."""
    is_new = db.get_bot(bot_id) is None
    db.add_bot(bot_id, name, color)
    if is_new:
        appearance.set_bot_appearance(bot_id, color = color, expression = expression)
    for position, (channel_id, title, description, channel_color) in enumerate(channels):
        db.add_channel(channel_id, title, description, channel_color, bot_id = bot_id, position = position)