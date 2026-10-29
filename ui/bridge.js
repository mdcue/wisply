// Lets the Hub page (dashboard.js) talk to Wisply's Rust core. The page was written for the old
// Python app, where a "bridge" object had one function per request, each taking its arguments and
// then a function to call with the answer. This file rebuilds that object on top of Tauri, so the
// page itself did not have to change:
//   bridge.get_wisps(function (json) { ... });
// It also stands in for the old QWebChannel start-up call, which dashboard.js still makes.
(function () {
    "use strict";

    var invoke = window.__TAURI__.core.invoke;
    var listen = window.__TAURI__.event.listen;

    var OOPS = JSON.stringify({ ok: false, error: "Something went wrong. Please try again." });

    // name: [command, [names of the arguments, in the order the page passes them], kind, answer if it fails]
    // kind "json": the answer goes to the page as a JSON string, as before. "plain": as it is.
    var METHODS = {
        account_exists:         ["account_exists", [], "plain", false],
        create_account:         ["create_account", ["name"], "plain", false],
        get_expressions:        ["get_expressions", [], "json", "[]"],
        get_wisps:              ["get_wisps", [], "json", "[]"],
        get_settings_wisps:     ["get_settings_wisps", [], "json", "[]"],
        get_activity_feed:      ["get_activity_feed", [], "json", "[]"],
        get_channels:           ["get_channels", ["wispId"], "json", "[]"],
        toggle_active:          ["toggle_active", ["wispId"], "plain", false],
        rename_wisp:            ["rename_wisp", ["wispId", "newName"], "plain", false],
        recolor_wisp:           ["recolor_wisp", ["wispId", "color"], "plain", false],
        set_expression:         ["set_expression", ["wispId", "expression"], "plain", false],
        set_fetch_interval:     ["set_fetch_interval", ["wispId", "seconds"], "plain", null],
        fetch_now:              ["fetch_now", ["wispId"], "plain", null],
        mark_item_read:         ["mark_item_read", ["itemId"], "plain", null],
        mark_all_read:          ["mark_all_read", ["channelId"], "plain", null],
        dismiss_item:           ["dismiss_item", ["itemId"], "plain", null],
        clear_read:             ["clear_read", ["channelId"], "plain", null],
        open_item:              ["open_item", ["itemId"], "plain", null],   // the link is looked up by Wisply, not taken from the page
        companion_notes:        ["companion_notes", [], "json", "{}"],
        companion_mood:         ["companion_mood", [], "json", "{}"],
        companion_add_reminder: ["companion_add_reminder", ["text", "dateText", "timeText", "repeat"], "json", OOPS],
        companion_add_idea:     ["companion_add_idea", ["text"], "json", OOPS],
        companion_update:       ["companion_update", ["noteId", "text", "dateText", "timeText", "repeat"], "json", OOPS],
        companion_complete:     ["companion_complete", ["noteId"], "json", OOPS],
        companion_snooze:       ["companion_snooze", ["noteId", "option"], "json", OOPS],
        companion_delete:       ["companion_delete", ["noteId"], "json", OOPS],
        open_data_folder:       ["open_data_folder", [], "plain", null],
        quit_app:               ["quit_app", [], "plain", null]
    };

    function signal() {
        var listeners = [];
        return {
            connect: function (fn) { listeners.push(fn); },
            emit: function () {
                var args = arguments;
                listeners.forEach(function (fn) { fn.apply(null, args); });
            }
        };
    }

    var bridge = {
        dataChanged: signal(),
        newItemsFound: signal(),
        fetchFinished: signal()
    };

    Object.keys(METHODS).forEach(function (name) {
        var spec = METHODS[name];
        bridge[name] = function () {
            var args = Array.prototype.slice.call(arguments);
            var done = typeof args[args.length - 1] === "function" ? args.pop() : null;
            var payload = {};
            spec[1].forEach(function (key, i) { payload[key] = args[i]; });
            invoke(spec[0], payload).then(function (answer) {
                if (done) done(spec[2] === "json" ? JSON.stringify(answer) : answer);
            }, function (error) {
                console.error("Wisply: " + name + " failed:", error);
                if (done && spec[3] !== null) done(spec[3]);
            });
        };
    });

    // What the page used to hear from Python as signals, now events from the Rust side.
    var listening = Promise.all([
        listen("data-changed", function () { bridge.dataChanged.emit(); }),
        listen("new-items", function (event) { bridge.newItemsFound.emit(event.payload.name, event.payload.count); }),
        listen("fetch-finished", function (event) {
            var p = event.payload;
            bridge.fetchFinished.emit(p.wispId, p.ok, p.newItems);
        })
    ]);

    // dashboard.js starts with: new QWebChannel(qt.webChannelTransport, function (channel) { ... })
    window.qt = { webChannelTransport: {} };
    window.QWebChannel = function (transport, ready) {
        listening.then(function () { ready({ objects: { bridge: bridge } }); });
    };
})();
