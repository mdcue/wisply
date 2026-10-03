let bridge = null;
let expandedId = null;
let channelSortMode = {};
let currentTab = "dashboard";

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
    document.getElementById("main-hub").style.display = "none";

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
                bridge.toggle_active(wisp.id, function () {
                    renderWisps();
                });
            };
            top.appendChild(toggle);

            row.appendChild(top);

            row.onclick = function () {
                expandedId = (expandedId == wisp.id) ? null : wisp.id;
                renderWisps()
            };

            if (expandedId === wisp.id) {
                const panel = buildManagePanel(wisp);
                panel.onclick = function (e) { e.stopPropagation(); };
                row.appendChild(panel);
            }

            list.appendChild(row);
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
