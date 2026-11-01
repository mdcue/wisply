// The "Add idea" / "Add reminder" window. One page for both: Rust says which one is wanted
// (`quick_add_kind`), the core says what to write on it (`quick_add_info`), and saving goes through
// the same commands the Hub uses. When the form is drawn the page tells Rust how big it is
// (`quick_add_ready`), and Rust centres the window and shows it.
(function () {
    "use strict";

    var CLOSE_AFTER_SAVING_MS = 1400;

    var card = document.getElementById("card");
    var bridge = null;
    var token = 0;            // each redraw gets a number; an answer for an older one is dropped
    var closeTimer = null;
    var noteInput = null;     // the field to focus, when there is one

    function make(tag, className, text) {
        var node = document.createElement(tag);
        if (className) node.className = className;
        if (text !== undefined) node.textContent = text;   // text only, never HTML
        return node;
    }

    function empty(node) {
        while (node.firstChild) node.removeChild(node.firstChild);
    }

    function pad(number) {
        return (number < 10 ? "0" : "") + number;
    }

    // The next full hour, as the old window offered (date as 2026-10-30, time as 14:00).
    function nextFullHour() {
        var soon = new Date(Date.now() + 3600 * 1000);
        return {
            date: soon.getFullYear() + "-" + pad(soon.getMonth() + 1) + "-" + pad(soon.getDate()),
            time: pad(soon.getHours()) + ":00"
        };
    }

    function settle() {
        var box = card.getBoundingClientRect();
        bridge.quick_add_ready(Math.ceil(box.width), Math.ceil(box.height));
    }

    function close() {
        if (closeTimer) {
            clearTimeout(closeTimer);
            closeTimer = null;
        }
        bridge.quick_add_hide();
    }

    // ---- drawing ----

    function load() {
        var mine = ++token;
        if (closeTimer) {
            clearTimeout(closeTimer);
            closeTimer = null;
        }
        bridge.quick_add_kind(function (kind) {
            if (mine !== token) return;
            bridge.quick_add_info(function (json) {
                if (mine !== token) return;
                var info = JSON.parse(json);
                draw(kind === "reminder" ? "reminder" : "idea", info);
            });
        });
    }

    function draw(kind, info) {
        empty(card);
        noteInput = null;
        card.appendChild(make("h1", "", kind === "reminder" ? info.reminder_title : info.idea_title));

        if (!info.companion_on) {
            // pip is switched off, so nothing can be kept
            card.appendChild(make("div", "off-text", info.off_message));
            var buttons = make("div", "buttons");
            var hub = make("button", "", "Open Hub");
            hub.onclick = function () { bridge.open_hub_window(); };
            var done = make("button", "primary", "Close");
            done.onclick = close;
            buttons.appendChild(hub);
            buttons.appendChild(done);
            card.appendChild(buttons);
            settle();
            return;
        }

        var note = make("input");
        note.type = "text";
        note.maxLength = info.max_text;
        note.placeholder = kind === "reminder" ? info.reminder_placeholder : info.idea_placeholder;
        noteInput = note;
        card.appendChild(row("Note", note));

        var date = null, time = null, repeat = null;
        if (kind === "reminder") {
            var soon = nextFullHour();
            date = make("input");
            date.type = "date";
            date.value = soon.date;
            time = make("input");
            time.type = "time";
            time.value = soon.time;
            var when = row("When", date);
            when.appendChild(time);
            card.appendChild(when);

            repeat = make("select");
            info.repeats.forEach(function (choice) {
                var option = make("option", "", choice.label);
                option.value = choice.value;
                repeat.appendChild(option);
            });
            card.appendChild(row("Repeat", repeat));
        }

        var status = make("div", "status");
        card.appendChild(status);

        var buttons2 = make("div", "buttons");
        var cancel = make("button", "", "Cancel");
        cancel.onclick = close;
        var save = make("button", "primary", "Save");
        buttons2.appendChild(cancel);
        buttons2.appendChild(save);
        card.appendChild(buttons2);

        var fields = [note, date, time, repeat].filter(Boolean);

        function lock(locked) {
            fields.concat([cancel, save]).forEach(function (node) { node.disabled = locked; });
        }

        function saved(json) {
            var result = JSON.parse(json);
            if (result.ok) {
                status.className = "status good";
                status.textContent = result.message || "Saved.";
                closeTimer = setTimeout(close, CLOSE_AFTER_SAVING_MS);
                return;
            }
            status.className = "status error";
            status.textContent = result.error || "Something went wrong. Please try again.";
            lock(false);
            note.focus();
            settle();
        }

        function submit() {
            if (save.disabled) return;
            lock(true);
            status.className = "status";
            status.textContent = "";
            if (kind === "reminder") {
                bridge.companion_add_reminder(note.value, date.value, time.value, repeat.value, saved);
            } else {
                bridge.companion_add_idea(note.value, saved);
            }
        }

        save.onclick = submit;
        fields.forEach(function (field) {
            if (field.tagName === "INPUT") {
                field.addEventListener("keydown", function (event) {
                    if (event.key === "Enter") {
                        event.preventDefault();
                        submit();
                    }
                });
            }
        });

        note.focus();
        settle();
    }

    function row(label, field) {
        var line = make("div", "row");
        line.appendChild(make("label", "", label));
        line.appendChild(field);
        return line;
    }

    // ---- start ----

    document.addEventListener("keydown", function (event) {
        if (event.key === "Escape" && bridge) close();
    });

    // The window may only get the keyboard after the form was drawn; hand it to the note field then.
    window.addEventListener("focus", function () {
        if (noteInput && !noteInput.disabled && document.activeElement === document.body) noteInput.focus();
    });

    // No browser menu anywhere.
    document.addEventListener("contextmenu", function (event) {
        if (event.target.tagName !== "INPUT") event.preventDefault();
    });

    new QWebChannel(qt.webChannelTransport, function (channel) {
        bridge = channel.objects.bridge;
        // Rust asks for a fresh form each time the menu is used.
        bridge.quickAddOpen.connect(load);
        load();
    });
})();
