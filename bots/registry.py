from bots import deals

#A registry for all the bots in the app, easy to maintain and check in

ALL_BOTS = [
    deals,
]

BOT_MODULES = {bot.BOT_ID: bot for bot in ALL_BOTS}