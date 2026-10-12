from bots import deals, github, companion

#A registry for all the bots in the app, easy to maintain and check in

ALL_BOTS = [
    deals, github, companion
]

BOT_MODULES = {bot.BOT_ID: bot for bot in ALL_BOTS}
