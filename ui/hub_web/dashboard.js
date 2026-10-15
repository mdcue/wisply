let bridge = null;
let expandedId = null;
let channelSortMode = {};
let currentTab = "dashboard";
let bannerTimeout = null;
let cachedExpressions = null;
let fetchStatus = {};
const THEME_KEY = "wisply-theme";

initTheme();

new QWebChannel(qt.webChannelTransport, function (channel) {
    bridge = channel.objects.bridge;
    bridge.dataChanged.connect(function () {
        const shell = document.getElementById("app-shell");
        if (shell && shell.style.display !== "none") {
            if (currentTab === "dashboard") {
                renderActivityFeed();
            } else if (currentTab === "wisps") {
                renderWisps();
            } else if (currentTab === "discover") {
                renderGallery("discover-gallery");
            } else if (currentTab === "settings") {
                renderSettings();
            }
        }
    });
    bridge.newItemsFound.connect(showNotificationBanner);
    bridge.fetchFinished.connect(onFetchFinished);
    start();
});

function initTheme() {
    let theme = localStorage.getItem(THEME_KEY);
    if (!theme) {
        theme = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
    }
    applyTheme(theme);
}

function onFetchFinished(wispId, ok, newCount) {
    if (!ok) {
        fetchStatus[wispId] = "Couldn't reach the source, try again later";
    } else if (newCount > 0) {
        fetchStatus[wispId] = `Checked just now: ${newCount} new ${newCount === 1 ? "item" : "items"}`;
    } else {
        fetchStatus[wispId] = "Checked just now: nothing new";
    }
    if (currentTab === "settings") renderSettings();

    setTimeout(function () {
        delete fetchStatus[wispId];
        if (currentTab === "settings") renderSettings();
    }, 15000);
}

function showNotificationBanner(wispName, count) {
    const banner = document.getElementById("notification-banner");
    if (!banner) return;

    const itemWord = count === 1 ? "item" : "items";
    banner.textContent = `${wispName} found ${count} new ${itemWord}`;
    banner.style.display = "block";

    if (bannerTimeout) clearTimeout(bannerTimeout);
    bannerTimeout = setTimeout(function () {
        banner.style.display = "none";
    }, 5000);

    if (currentTab === "wisps") {
        renderWisps();
    } else if (currentTab === "dashboard") {
        renderActivityFeed();
    }
}

function readableTextColor(hex) {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    const theme = document.documentElement.getAttribute("data-theme") || "dark";

    if (theme === "light" && luminance > 0.55) {
        return `rgb(${Math.round(r * 0.55)}, ${Math.round(g * 0.55)}, ${Math.round(b * 0.55)})`;
    }
    if (theme === "dark" && luminance < 0.3) {
        return `rgb(${Math.min(255, Math.round(r + (255 - r) * 0.6))}, ${Math.min(255, Math.round(g + (255 - g) * 0.6))}, ${Math.min(255, Math.round(b + (255 - b) * 0.6))})`;
    }
    return hex;
}

function textOnColor(hex) {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    return luminance > 0.6 ? "#1a1a1a" : "#ffffff";
}

function colorButton(button, color) {
    button.style.backgroundColor = color;
    button.style.color = textOnColor(color);
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
    document.getElementById("open-data-btn").onclick = function () { bridge.open_data_folder(); };
    setupQuitButtons();;
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

    if (tab === "dashboard"){
        renderActivityFeed();
    } else if (tab === "wisps") {
        renderWisps();
    } else if (tab === "discover") {
        renderGallery("discover-gallery");
    } else if (tab === "settings") {
        renderSettings();
    }
}

function setupQuitButtons() {
    const quitBtn = document.getElementById("quit-btn");
    const confirmBox = document.getElementById("quit-confirm");

    quitBtn.onclick = function () {
        quitBtn.style.display = "none";
        confirmBox.style.display = "flex";
    };
    document.getElementById("quit-no").onclick = function () {
        confirmBox.style.display = "none";
        quitBtn.style.display = "inline-block";
    };
    document.getElementById("quit-yes").onclick = function () {
        bridge.quit_app();
    };
}

function renderActivityFeed() {
    bridge.get_activity_feed(function (json) {
        const feed = JSON.parse(json);
        const container = document.getElementById("activity-feed");
        container.innerHTML = "";

        if (feed.length === 0) {
            const empty = document.createElement("p");
            empty.className = "placeholder-text";
            empty.textContent = "Nothing yet - activate a wisp to start seeing activity here.";
            container.appendChild(empty);
            return;
        }

        feed.forEach(function (entry) {
            const row = document.createElement("div");
            row.className = "feed-row" + (entry.read ? " read" : "");

            const img = document.createElement("img");
            img.src = entry.wisp_icon;
            row.appendChild(img);

            const content = document.createElement("div");
            content.className = "feed-row-content";

            const wispLabel = document.createElement("div");
            wispLabel.className = "feed-row-wisp";
            wispLabel.textContent = entry.wisp_name;
            content.appendChild(wispLabel)

            const textE1 = document.createElement("div");
            textE1.className = "feed-row-text";

            if (entry.url) {
                const a = document.createElement("a");
                a.href = "#";
                a.textContent = entry.text;
                a.onclick = function (e) {
                    e.preventDefault();
                    bridge.open_item(entry.item_id, entry.url, function () {
                        renderActivityFeed();
                    });
                };
                textE1.appendChild(a);
            } else {
                textE1.textContent = entry.text;
            }

            content.appendChild(textE1);
            row.appendChild(content);
            container.appendChild(row);
        });
    });
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
            row.style.boxShadow = `0 2px 10px ${wisp.color}33`;

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
            
            let metaText;
            if (wisp.fetch_interval === 0) {
                metaText = wisp.last_fetched
                ? ("Checked at startup: " + wisp.last_fetched)
                : "Will check on next app launch";
            } else {
                metaText = wisp.last_fetched ? ("Last checked: " + wisp.last_fetched) : "Not checked yet";
            }
            meta.textContent = fetchStatus[wisp.id] ? (fetchStatus[wisp.id] + " . " + metaText) : metaText;
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

            const fetchBtn = document.createElement("button");
            fetchBtn.className = "small-btn";
            fetchBtn.textContent = "Check now";
            colorButton(fetchBtn, wisp.color);
            fetchBtn.onclick = function () {
                fetchBtn.disabled = true;
                fetchBtn.textContent = "Checking...";
                bridge.fetch_now(wisp.id);
            };
            card.appendChild(fetchBtn);

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
    colorButton(renameBtn, wisp.color);
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
        bridge.recolor_wisp(wisp.id, colorInput.value, function () {
            renderWisps();
        });
    };
    colorField.append(colorLabel, colorInput);
    panel.appendChild(colorField);

    //Expression
    if (wisp.id !== "companion") {
        const exprField = document.createElement("div");
        exprField.className = "manage-field";
        const exprLabel = document.createElement("label");
        exprLabel.textContent = "Expression";
        const exprSelect = document.createElement("select");
        exprField.append(exprLabel, exprSelect);
        panel.appendChild(exprField);

        const populateExpressionOptions = function (options) {
            options.forEach(function (opt) {
                const o = document.createElement("option");
                o.value = opt;
                o.textContent = opt;
                if (opt === wisp.expression) o.selected = true;
                exprSelect.appendChild(o);
            });
        };
        if (cachedExpressions) {
            populateExpressionOptions(cachedExpressions);
        } else {
            bridge.get_expressions(function (json) {
                cachedExpressions = JSON.parse(json);
                populateExpressionOptions(cachedExpressions);
            });
        }

        exprSelect.onchange = function () {
            bridge.set_expression(wisp.id, exprSelect.value, function () { renderWisps(); });
        };
    }

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
    title.style.color = readableTextColor(channel.color);
    title.textContent = channel.title + "  .  " + channel.unread + " unread";
    head.appendChild(title);

    const sortOptions = sortOptionsFor(channel);
    const sortMode = currentSortMode(channel, sortOptions);

    if (sortOptions.length > 1) {
        const sortSelect = document.createElement("select");
        sortSelect.className = "channel-sort";
        sortOptions.forEach(function ([value, label]) {
            const o = document.createElement("option");
            o.value = value;
            o.textContent = label;
            if (sortMode === value) o.selected = true;
            sortSelect.appendChild(o);
        });
        sortSelect.onchange = function () {
            channelSortMode[channel.id] = sortSelect.value;
            renderWisps();
        };
        head.appendChild(sortSelect);
    }
    
    const markBtn = document.createElement("button");
    markBtn.className = "small-btn";
    markBtn.textContent = "Mark all read";
    markBtn.disabled = channel.unread === 0;
    colorButton(markBtn, channel.color);
    markBtn.onclick = function () {
        bridge.mark_all_read(channel.id, function () { renderWisps(); });
    };
    head.appendChild(markBtn);

    const clearBtn = document.createElement("button");
    clearBtn.className = "small-btn ghost";
    clearBtn.textContent = "Clear read";
    clearBtn.disabled = !channel.items.some(function (item) { return item.read; });
    clearBtn.onclick = function () {
        bridge.clear_read(channel.id, function () { renderWisps(); });
    };
    head.appendChild(clearBtn);
    block.appendChild(head);

    const items = sortChannelItems(channel.items, sortMode);
    
    if (items.length === 0) {
        const empty = document.createElement("div");
        empty.className = "channel-item";
        empty.textContent = "Nothing here yet.";
        block.appendChild(empty);
    }

    items.forEach(function (item) {
        const row = document.createElement("div");
        row.className = "channel-item" + (item.read ? " read" : "");

        const body = document.createElement("div")
        body.className = "channel-item-text";
        if (item.url) {
            const a = document.createElement("a");
            a.href = "#";
            a.textContent = item.text;
            a.onclick = function (e) {
                e.preventDefault(); //this will stop the embedded page from navigating away
                bridge.open_item(item.id, item.url, function () { renderWisps(); });
            };
            body.appendChild(a);
        } else {
            body.textContent = item.text;
        }
        row.appendChild(body);

        const dismiss = document.createElement("button");
        dismiss.className = "item-dismiss";
        dismiss.textContent = "×";
        dismiss.title = "Remove";
        dismiss.onclick = function () {
            bridge.dismiss_item(item.id, function () { renderWisps(); });
        };
        row.appendChild(dismiss);
        block.appendChild(row);
    });

    return block;
}

function sortOptionsFor(channel) {
    const options = [["unread", "Unread first"]];
    const hasPrice = channel.items.some(function (item) {
        return item.price !== null && item.price !== undefined;
    });
    if (hasPrice) {
        options.push(["price_asc", "Price: low to high"]);
    }
    if (channel.id === "deals") {
        options.push(["discount_desc", "Biggest discount"]);
    }
    if (channel.id.startsWith("gh_")) {
        options.push(["stars_desc", "Most stars"]); //GitHub items keep their star count in the discount field
    }
    return options;
}

function currentSortMode(channel, options) {
    const saved = channelSortMode[channel.id];
    const stillOffered = options.some(function ([value]) { return value === saved; });
    return stillOffered ? saved : "unread";
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
    } else if (mode === "stars_desc") {
        copy.sort(function (a, b) {
            const sa = (a.discount === null || a.discount === undefined) ? -1 : a.discount;
            const sb = (b.discount === null || b.discount === undefined) ? -1 : b.discount;
            return sb - sa;
        });
    } else {
        copy.sort(function (a, b) {
            return (a.read === b.read) ? 0 : (a.read ? 1 : -1);
        });
    }
    return copy;
}
