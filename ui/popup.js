// The tray pop-up page. One window, two views:
//   "strip": a row of icons, one for each switched-on wisp;
//   "panel": one wisp, with its channels (one open at a time) and their items.
// It talks to Wisply through the same `bridge` the Hub uses (see bridge.js). Whenever it has drawn
// itself it tells Rust how big it is (`popup_ready`), and Rust places the window and shows it.
(function () {
    "use strict";

    var MAX_LIST_HEIGHT = 240;   // tallest an items list gets before it scrolls
    var ROW_ESTIMATE = 40;       // how tall one item is guessed to be
    var NAME_LIMIT = 40;         // the same limit the core uses
    var COLORS = ["#4CAF50", "#2196F3", "#8957e5", "#ff7a59", "#f5c542",
                  "#14b8a6", "#ec4899", "#f59e0b", "#e53935", "#9e9e9e"];

    var card = document.getElementById("card");
    var bridge = null;
    var token = 0;               // each redraw gets a number; an answer for an older one is dropped
    var redrawQueued = false;
    var cache = { wisp: null, channels: null };

    var state = { view: "strip", wispId: null, channelId: null, editing: false };

    // ---- small helpers ----

    function make(tag, className, text) {
        var node = document.createElement(tag);
        if (className) node.className = className;
        if (text !== undefined) node.textContent = text;   // text only, never HTML
        return node;
    }

    function empty(node) {
        while (node.firstChild) node.removeChild(node.firstChild);
    }

    function isColor(text) {
        return /^#[0-9a-fA-F]{6}$/.test(text);
    }

    function resetToStrip() {
        state.view = "strip";
        state.wispId = null;
        state.channelId = null;
        state.editing = false;
    }

    // Tells Rust how big the card is now. Layout is read directly (no timers or animation frames,
    // which a hidden window may pause).
    function settle() {
        var box = card.getBoundingClientRect();
        bridge.popup_ready(Math.ceil(box.width), Math.ceil(box.height));
    }

    // ---- drawing ----

    function redraw() {
        var mine = ++token;
        bridge.get_wisps(function (json) {
            if (mine !== token) return;
            var wisps = JSON.parse(json).filter(function (w) { return w.active; });
            if (wisps.length === 0) {
                resetToStrip();
                drawEmpty();
                return;
            }
            if (state.view === "panel") {
                var wisp = wisps.filter(function (w) { return w.id === state.wispId; })[0];
                if (wisp) {
                    bridge.get_channels(wisp.id, function (channelsJson) {
                        if (mine !== token) return;
                        cache.wisp = wisp;
                        cache.channels = JSON.parse(channelsJson);
                        drawPanel();
                    });
                    return;
                }
                resetToStrip();   // the wisp was switched off meanwhile
            }
            drawStrip(wisps);
        });
    }

    // Several things can ask for a redraw at once (an action, then the "data changed" event);
    // this makes them one.
    function queueRedraw() {
        if (redrawQueued) return;
        redrawQueued = true;
        Promise.resolve().then(function () {
            redrawQueued = false;
            redraw();
        });
    }

    function drawEmpty() {
        card.className = "empty";
        empty(card);
        card.appendChild(make("div", "empty-text", "No active wisps yet.\nOpen the Hub to activate one."));
        card.firstChild.style.whiteSpace = "pre-line";
        var open = make("button", "small-btn", "Open Hub");
        open.onclick = function () { bridge.open_hub_window(); };
        card.appendChild(open);
        settle();
    }

    function drawStrip(wisps) {
        card.className = "strip";
        empty(card);
        wisps.forEach(function (wisp) {
            var button = make("button", "wisp-btn");
            button.title = wisp.name;
            if (wisp.icon) {
                var image = make("img");
                image.src = wisp.icon;
                image.alt = wisp.name;
                button.appendChild(image);
            } else {
                button.appendChild(make("span", "initial", wisp.name.charAt(0).toUpperCase()));
            }
            button.onclick = function () {
                state.view = "panel";
                state.wispId = wisp.id;
                state.channelId = null;
                state.editing = false;
                redraw();
            };
            card.appendChild(button);
        });
        settle();
    }

    function totalUnread(channels) {
        return channels.reduce(function (sum, channel) { return sum + channel.unread; }, 0);
    }

    // Only one channel of a wisp is open at a time. Keeps the one you opened; otherwise the first
    // with something unread; otherwise the first.
    function channelToOpen(channels) {
        var ids = channels.map(function (c) { return c.id; });
        if (ids.indexOf(state.channelId) >= 0) return state.channelId;
        var unread = channels.filter(function (c) { return c.unread > 0; })[0];
        return unread ? unread.id : (ids[0] || null);
    }

    function drawPanel() {
        var wisp = cache.wisp;
        var channels = cache.channels;
        card.className = "panel";
        empty(card);

        card.appendChild(buildHead(wisp, channels));
        if (state.editing) card.appendChild(buildEditor(wisp));

        state.channelId = channelToOpen(channels);
        // every list is as tall as the tallest, so switching channels never changes the window's size
        var tallest = channels.reduce(function (most, c) { return Math.max(most, c.items.length); }, 1);
        var listHeight = Math.min(MAX_LIST_HEIGHT, ROW_ESTIMATE * tallest);
        channels.forEach(function (channel) {
            card.appendChild(buildChannel(channel, channel.id === state.channelId, listHeight));
        });
        settle();
    }

    function buildHead(wisp, channels) {
        var head = make("div", "panel-head");
        head.style.borderLeftColor = wisp.color;

        var back = make("button", "back-btn", "‹");
        back.title = "Back";
        back.onclick = function () {
            resetToStrip();
            redraw();
        };
        head.appendChild(back);

        if (wisp.icon) {
            var icon = make("img", "head-icon");
            icon.src = wisp.icon;
            icon.alt = "";
            head.appendChild(icon);
        }
        head.appendChild(make("span", "head-name", wisp.name));
        head.appendChild(make("span", "head-unread", totalUnread(channels) + " unread"));

        // right-click: rename or change the colour
        head.oncontextmenu = function (event) {
            event.preventDefault();
            state.editing = !state.editing;
            drawPanel();
        };
        return head;
    }

    function buildChannel(channel, isOpen, listHeight) {
        var block = make("div", "channel");
        var top = make("div", "channel-top");

        var title = make("button", "channel-title",
            (isOpen ? "▾ " : "▸ ") + channel.title + "  ·  " + channel.unread + " unread");
        title.style.color = channel.color;
        title.onclick = function () {
            state.channelId = channel.id;
            drawPanel();   // nothing to fetch again: only which list is open changes
        };
        top.appendChild(title);

        var mark = make("button", "small-btn", "Mark all read");
        mark.disabled = channel.unread === 0;
        mark.style.visibility = isOpen ? "visible" : "hidden";   // hidden, but it keeps its space
        mark.onclick = function () { bridge.mark_all_read(channel.id, queueRedraw); };
        top.appendChild(mark);
        block.appendChild(top);

        if (isOpen) block.appendChild(buildItems(channel, listHeight));
        return block;
    }

    function buildItems(channel, listHeight) {
        var list = make("div", "items");
        list.style.height = listHeight + "px";
        if (channel.items.length === 0) {
            list.appendChild(make("div", "nothing", "Nothing here yet."));
        }
        channel.items.forEach(function (item) {
            var row = make("div", "item " + (item.read ? "read" : "unread"), item.text);
            if (item.url) {
                // opens the link in the browser (Rust looks the link up itself) and marks it read
                row.className += " clickable";
                row.onclick = function () { bridge.open_item(item.id, queueRedraw); };
            } else if (!item.read) {
                row.className += " clickable";
                row.title = "Click to mark as read";
                row.onclick = function () { bridge.mark_item_read(item.id, queueRedraw); };
            }
            list.appendChild(row);
        });
        return list;
    }

    // ---- rename and colour ----

    function buildEditor(wisp) {
        var editor = make("div", "editor");

        var name = make("input");
        name.type = "text";
        name.value = wisp.name;
        name.maxLength = NAME_LIMIT;
        name.placeholder = "Name";
        editor.appendChild(name);

        var chosen = wisp.color;
        var hex = make("input");
        hex.type = "text";
        hex.value = wisp.color;
        hex.maxLength = 7;
        hex.placeholder = "#4CAF50";

        var swatches = make("div", "swatches");
        COLORS.forEach(function (color) {
            var swatch = make("button", "swatch" + (color.toLowerCase() === chosen.toLowerCase() ? " chosen" : ""));
            swatch.style.background = color;
            swatch.title = color;
            swatch.onclick = function () {
                hex.value = color;
                Array.prototype.forEach.call(swatches.children, function (s) { s.className = "swatch"; });
                swatch.className = "swatch chosen";
            };
            swatches.appendChild(swatch);
        });
        editor.appendChild(swatches);

        var row = make("div", "editor-row");
        row.appendChild(make("span", "", "Color"));
        row.appendChild(hex);
        editor.appendChild(row);

        var error = make("div", "editor-error");
        editor.appendChild(error);

        var buttons = make("div", "editor-buttons");
        var cancel = make("button", "small-btn", "Cancel");
        cancel.onclick = function () {
            state.editing = false;
            drawPanel();
        };
        var save = make("button", "small-btn", "Save");
        save.onclick = function () {
            var newName = name.value.trim();
            var newColor = hex.value.trim();
            if (!newName) {
                error.textContent = "The name can't be empty.";
                settle();
                return;
            }
            if (!isColor(newColor)) {
                error.textContent = "A color looks like #4CAF50.";
                settle();
                return;
            }
            saveLook(wisp, newName, newColor);
        };
        buttons.appendChild(cancel);
        buttons.appendChild(save);
        editor.appendChild(buttons);
        return editor;
    }

    // Renames and recolours one after the other, then draws again.
    function saveLook(wisp, newName, newColor) {
        function recolor() {
            if (newColor.toLowerCase() === wisp.color.toLowerCase()) return done();
            bridge.recolor_wisp(wisp.id, newColor, done);
        }
        function done() {
            state.editing = false;
            redraw();
        }
        if (newName === wisp.name) return recolor();
        bridge.rename_wisp(wisp.id, newName, recolor);
    }

    // ---- start ----

    document.addEventListener("keydown", function (event) {
        if (event.key !== "Escape" || !bridge) return;
        if (state.editing) {
            state.editing = false;
            drawPanel();
        } else {
            bridge.popup_hide();
        }
    });

    // No browser menu anywhere (the header has its own, above).
    document.addEventListener("contextmenu", function (event) { event.preventDefault(); });

    new QWebChannel(qt.webChannelTransport, function (channel) {
        bridge = channel.objects.bridge;
        // Rust asks for a fresh drawing each time the pop-up is about to open.
        bridge.popupOpen.connect(function () {
            resetToStrip();
            redraw();
        });
        // Something changed (a check finished, an item was read in the Hub...).
        bridge.dataChanged.connect(queueRedraw);
        redraw();
    });
})();
