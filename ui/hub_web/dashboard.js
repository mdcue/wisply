let bridge = null

new QWebChannel(qt.webChannelTransport, function (channel) {
    bridge = channel.objects.bridge;
    bridge.dataChanged.connect(renderWisps);
    renderWisps();
});

function renderWisps() {
    bridge.get_wisps(function (json){
        const wisps = JSON.parse(json);
        const list = document.getElementById("wisp-list");
        list.innerHTML = "";

        wisps.forEach(function (wisp){
            const row = document.createElement("div");
            row.className = "wisp-row";
            row.style.borderLeft = "4px solid " + wisp.color;

            const img = document.createElement("img");
            img.src = wisp.icon;
            row.appendChild(img);

            const name = document.createElement("span");
            name.className = "wisp-name";
            name.textContent = wisp.name;
            row.appendChild(name);

            const toggle = document.createElement("button");
            toggle.textContent = wisp.active ? "On" : "Off";
            toggle.onclick = function () {
                bridge.toggle_active(wisp.id, function (){
                    renderWisps()
                });
            };
            row.appendChild(toggle)

            list.appendChild(row);
        });
    });
}