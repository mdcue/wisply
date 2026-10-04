let bridge = null;
let expandedId = null;
let channelSortMode = {};
let currentTab = "dashboard";
const THEME_KEY = "wisply-theme";

initTheme();

new QWebChannel(qt.webChannelTransport, function (channel) {
    bridge = channel.objects.bridge;
    bridge.dataChanged.connect(function () {
        const shell = document.getElementById("app-shell");
        if (shell && shell.style.display !== "none") {
            if (currentTab === "wisps") {
                renderWisps();
            } else if (currentTab === "discover") {
                renderGallery("discover-gallery");
            }
        }
    });
    start();
});

function initTheme() {
    let theme = localStorage.getItem(THEME_KEY);
    if (!theme) {
        theme = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
    }
    applyTheme(theme);
}

function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem(THEME_KEY, theme);
    const btn = document.getElementById("theme-toggle");
    if (btn) {
        btn.textContent = theme === "light" ? "☀️" : "🌙";
    }
}

function toggleTheme()  {
    const current = document.documentElement.getAttribute("data-theme") || "dark";
    applyTheme(current === "dark" ? "light" : "dark");
}

function start() {
    bridge.account_exists(function (exists) {
        if (exists) {
            showMainHub();
        } else {
            showOnboarding();
        }
    });
}

function showOnboarding() {
    document.getElementById("onboarding").style.display = "block";
    document.getElementById("app-shell").style.display = "none";

    document.getElementById("name-continue").onclick = function () {
        const name = document.getElementById("name-input").value;
        bridge.create_account(name, function (ok) {
            if (ok) {
                document.getElementById("step-name").style.display = "none";
                document.getElementById("step-wisps").style.display = "block";
                renderGallery();
            }
        });
    };

    document.getElementById("onboarding-finish").onclick = function () {
        showMainHub();
    };
}

function renderGallery(containerId) {
    containerId = containerId || "gallery";
    bridge.get_wisps(function (json) {
        const wisps = JSON.parse(json);
        const gallery = document.getElementById(containerId);
        gallery.innerHTML = "";

        wisps.forEach(function (wisp) {
            const card = document.createElement("div");
            card.className = "gallery-card";

            const img = document.createElement("img");
            img.src = wisp.icon;
            card.appendChild(img);

            const info = document.createElement("div");
            info.className = "gallery-card-info";
            const name = document.createElement("div");
            name.className = "gallery-card-name";
            name.textContent = wisp.name;
            const desc = document.createElement("div");
            desc.className = "gallery-card-desc";
            desc.textContent = wisp.description;
            info.appendChild(name);
            info.appendChild(desc);
            card.appendChild(info);

            const btn = document.createElement("button");
            btn.className = "cta-btn" + (wisp.active ? " added" : "");
            btn.textContent = wisp.active ? "Added" : "Add";
            btn.onclick = function () {
                bridge.toggle_active(wisp.id, function (newState) {
                    btn.textContent = newState ? "Added" : "Add";
                    btn.className = "cta-btn" + (newState ? " added" : "");
                });
            };
            card.appendChild(btn);

            gallery.appendChild(card);
        });
    });
}

function showMainHub() {
    document.getElementById("onboarding").style.display = "none";
    document.getElementById("app-shell").style.display = "block";
    document.getElementById("theme-toggle").onclick = toggleTheme;
    setupTabs();
    switchTab(currentTab);
}

function setupTabs() {
    document.querySelectorAll(".tab-btn").forEach(function (btn) {
        btn.onclick = function() {
            switchTab(btn.dataset.tab);
        };
    });
}

function switchTab(tab) {
    currentTab = tab;

    document.querySelectorAll(".tab-btn").forEach(function (btn) {
        btn.classList.toggle("active", btn.dataset.tab === tab);
    });
    document.querySelectorAll(".tab-view").forEach(function (view) {
        view.style.display = "none";
    });
    document.getElementById("view-" + tab).style.display = "block";

    if (tab === "wisps") {
        renderWisps();
    } else if (tab === "discover") {
        renderGallery("discover-gallery");
    } else if (tab === "settings") {
        renderSettings();
    }
}

function renderWisps() {
    bridge.get_wisps(function (json) {
        const wisps = JSON.parse(json);
        const list = document.getElementById("wisp-list");
        list.innerHTML = "";

        wisps.forEach(function (wisp) {
            const row = document.createElement("div");
            row.className = "wisp-row" + (wisp.active ? "" : " inactive");
            row.style.borderLeftColor = wisp.color;

            const top = document.createElement("div");
            top.className = "wisp-row-top";

            const img = document.createElement("img");
            img.src = wisp.icon;
            top.appendChild(img);

            const name = document.createElement("span");
            name.className = "wisp-name";
            name.textContent = wisp.name;
            top.appendChild(name);

            const toggle = document.createElement("button");
            toggle.className = "toggle-btn " + (wisp.active ? "on" : "off");
            toggle.textContent = wisp.active ? "On" : "Off";
            toggle.onclick = function(e) {
                e.stopPropagation()
                bridge.toggle_active(wisp.id, function (newState) {
                    if (!newState && expandedId === wisp.id) {
                        expandedId = null;
                    }
                    renderWisps();
                });
            };
            top.appendChild(toggle);

            row.appendChild(top);

            if (wisp.active) {
                row.onclick = function () {
                    expandedId = (expandedId == wisp.id) ? null : wisp.id;
                    renderWisps()
                };
            }

            if (expandedId === wisp.id) {
                const panel = buildManagePanel(wisp);
                panel.onclick = function (e) { e.stopPropagation(); };
                row.appendChild(panel);
            }

            list.appendChild(row);
        });
    });
}

const INTERVAL_OPTIONS = [
    [0, "Only on startup"],
    [900, "15 minutes"],
    [1800, "30 minutes"],
    [3600, "1 hour"],
    [7200, "2 hours"],
    [14400, "4 hours"],
];

function renderSettings() {
    bridge.get_settings_wisps(function (json) {
        const wisps = JSON.parse(json);
        const list = document.getElementById("settings-list");
        list.innerHTML = "";

        if (wisps.length === 0) {
            const empty = document.createElement("p");
            empty.className = "placeholder-text";
            empty.textContent = "No active wisps yet - activate one from the Wisps tab first.";
            list.appendChild(empty);
            return;
        }

        wisps.forEach(function (wisp) {
            const card = document.createElement("div");
            card.className = "settings-card";

            const img = document.createElement("img");
            img.src = wisp.icon;
            card.appendChild(img);

            const info = document.createElement("div");
            info.className = "settings-card-info";
            const name = document.createElement("div");
            name.className = "settings-card-name";
            name.textContent = wisp.name;
            const meta = document.createElement("div");
            meta.className = "settings-card-meta";
            if (wisp.fetch_interval === 0) {
                meta.textContent = wisp.last_fetched
                    ? ("Checked at startup: " + wisp.last_fetched)
                    : "Will check on next app launch";
            } else {
                meta.textContent = wisp.last_fetched ? ("Last checked: " + wisp.last_fetched) : "Not checked yet";
            }
            info.appendChild(name);
            info.appendChild(meta);
            card.appendChild(info);

            const select = document.createElement("select");
            select.className = "settings-interval";
            INTERVAL_OPTIONS.forEach(function ([seconds, label]) {
                const o = document.createElement("option");
                o.value = seconds;
                o.textContent = label;
                if (wisp.fetch_interval === seconds) o.selected = true;
                select.appendChild(o);
            });
            select.onchange = function() {
                bridge.set_fetch_interval(wisp.id, parseInt(select.value, 10));
            };
            card.appendChild(select);

            list.appendChild(card);
        });
    });
}

function buildManagePanel(wisp) {
    const panel = document.createElement("div");
    panel.className = "manage-panel";

    //Renaming
    const renameField = document.createElement("div");
    renameField.className = "manage-field";
    const renameLabel = document.createElement("label");
    renameLabel.textContent = "Name";
    const renameInput = document.createElement("input");
    renameInput.type = "text";
    renameInput.value = wisp.name;
    const renameBtn = document.createElement("button");
    renameBtn.className = "small-btn";
    renameBtn.textContent = "Save";
    renameBtn.onclick = function () {
        bridge.rename_wisp(wisp.id, renameInput.value, function () { renderWisps(); });
    };
    renameField.append(renameLabel, renameInput, renameBtn);
    panel.appendChild(renameField);

    //Color
    const colorField = document.createElement("div");
    colorField.className = "manage-field";
    const colorLabel = document.createElement("label");
    colorLabel.textContent = "Color";
    const colorInput = document.createElement("input");
    colorInput.type = "color";
    colorInput.value = wisp.color;
    colorInput.onchange = function () {
        bridge.recolor_wisp(wisp.id, colorInput.value, function () { renderWisps(); });
    };
    colorField.append(colorLabel, colorInput);
    panel.appendChild(colorField);

    //Expression
    const exprField = document.createElement("div");
    exprField.className = "manage-field";
    const exprLabel = document.createElement("label");
    exprLabel.textContent = "Expression";
    const exprSelect = document.createElement("select");
    exprField.append(exprLabel, exprSelect);
    panel.appendChild(exprField);

    bridge.get_expressions(function (json) {
        const options = JSON.parse(json);
        options.forEach(function (opt) {
            const o = document.createElement("option");
            o.value = opt;
            o.textContent = opt;
            if (opt === wisp.expression) o.selected = true;
            exprSelect.appendChild(o);
        });
    });
    exprSelect.onchange = function () {
        bridge.set_expression(wisp.id, exprSelect.value, function () { renderWisps(); });
    };

    //Active channels
    if (wisp.active) {
        bridge.get_channels(wisp.id, function (json) {
            const channels = JSON.parse(json);
            channels.forEach(function (ch) {
                panel.appendChild(buildChannelBlock(ch));
            });
        });
    }

    return panel;
}

function buildChannelBlock(channel) {
    const block = document.createElement("div");
    block.className = "channel-block";

    const head = document.createElement("div");
    head.className = "channel-head";
    const title = document.createElement("span");
    title.className = "channel-title";
    title.style.color = channel.color;
    title.textContent = channel.title + "  .  " + channel.unread + " unread";
    head.appendChild(title);

    const sortSelect = document.createElement("select");
    sortSelect.className = "channel-sort";
    const sortOptions = [
        ["unread", "Unread first"],
        ["price_asc", "Price: low to high"],
    ];
    if (channel.id === "deals") {
        sortOptions.push(["discount_desc", "Biggest discount"]);
    }
    sortOptions.forEach(function ([value, label]) {
        const o = document.createElement("option");
        o.value = value;
        o.textContent = label;
        if ((channelSortMode[channel.id] || "unread") === value) o.selected = true;
        sortSelect.appendChild(o);
    });
    sortSelect.onchange = function () {
        channelSortMode[channel.id] = sortSelect.value;
        renderWisps();
    };
    head.appendChild(sortSelect);
    
    const markBtn = document.createElement("button");
    markBtn.className = "small-btn";
    markBtn.textContent = "Mark all read";
    markBtn.disabled = channel.unread === 0;
    markBtn.onclick = function () {
        bridge.mark_all_read(channel.id, function () { renderWisps(); });
    };
    head.appendChild(markBtn);
    block.appendChild(head);

    const items = sortChannelItems(channel.items, channelSortMode[channel.id] || "unread");
    
    if (items.length === 0) {
        const empty = document.createElement("div");
        empty.className = "channel-item";
        empty.textContent = "Nothing here yet.";
        block.appendChild(empty);
    }

    items.forEach(function (item) {
        const row = document.createElement("div");
        row.className = "channel-item" + (item.read ? " read" : "");
        if (item.url) {
            const a = document.createElement("a");
            a.href = "#";
            a.textContent = item.text;
            a.onclick = function (e) {
                e.preventDefault(); //stops the embedded page from navigating away
                bridge.open_item(item.id, item.url, function () { renderWisps(); });
            };
            row.appendChild(a);
        } else {
            row.textContent = item.text;
        }
        block.appendChild(row);
    });

    return block;
}

function sortChannelItems(items, mode) {
    const copy = items.slice();

    if (mode === "price_asc") {
        copy.sort(function (a, b) {
            const pa = (a.price === null || a.price === undefined) ? Infinity : a.price;
            const pb = (b.price === null || b.price === undefined) ? Infinity : b.price;
            return pa - pb;
        });
    } else if (mode === "discount_desc") {
        copy.sort(function (a, b) {
            const da = (a.discount === null || a.discount === undefined) ? -1 : a.discount;
            const db_ = (b.discount === null || b.discount === undefined) ? -1 : b.discount;
            return db_ - da;
        });
    } else {
        copy.sort(function (a, b) {
            return (a.read === b.read) ? 0 : (a.read ? 1 : -1);
        });
    }
    return copy;
}
