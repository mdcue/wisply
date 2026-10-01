let bridge = null;

new QWebChannel(qt.webChannelTransport, function (channel) {
    bridge = channel.objects.bridge;
    bridge.dataChanged.connect(function () {
        if (document.getElementById("main-hub").style.display !== "none") {
            renderWisps();
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

function renderGallery() {
    bridge.get_wisps(function (json) {
        const wisps = JSON.parse(json);
        const gallery = document.getElementById("gallery");
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
    document.getElementById("main-hub").style.display = "block";
    renderWisps();
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

            const img = document.createElement("img");
            img.src = wisp.icon;
            row.appendChild(img);

            const name = document.createElement("span");
            name.className = "wisp-name";
            name.textContent = wisp.name;
            row.appendChild(name);

            const toggle = document.createElement("button");
            toggle.className = "toggle-btn " + (wisp.active ? "on" : "off");
            toggle.textContent = wisp.active ? "On" : "Off";
            toggle.onclick = function() {
                bridge.toggle_active(wisp.id, function (){
                    renderWisps();
                });
            };
            row.appendChild(toggle);

            list.appendChild(row);
        });
    });
}