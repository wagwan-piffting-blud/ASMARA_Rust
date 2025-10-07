(function () {
    const LOG_LIMIT = parseInt(window.MONITORING_MAX_LOGS, 10) || 500;
    const LOG_FETCH_TAIL = Math.min(LOG_LIMIT, 500);

    const state = {
        streams: new Map(),
        activeAlerts: [],
        logs: [],
    };

    const elements = {
        wsStatus: document.getElementById("wsStatus"),
        streamGrid: document.getElementById("streamGrid"),
        streamCount: document.getElementById("streamCount"),
        alertList: document.getElementById("alertList"),
        alertCount: document.getElementById("alertCount"),
        logList: document.getElementById("logList"),
        logCount: document.getElementById("logCount"),
    };

    function setWsStatus(text, statusClass) {
        elements.wsStatus.textContent = text;
        elements.wsStatus.className = `ws-status ${statusClass || ""}`.trim();
    }

    function formatTimestamp(ts, withTime = true) {
        if (ts === null || ts === undefined) return "—";
        const date = new Date(ts);
        if (Number.isNaN(date.getTime())) return "—";
        const options = withTime
        ? {
            year: "numeric",
            month: "short",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
            second: "2-digit",
            }
        : {
            year: "numeric",
            month: "short",
            day: "numeric",
            };
        return new Intl.DateTimeFormat(undefined, options).format(date);
    }

    function formatDuration(seconds) {
        if (!seconds || seconds <= 0) return "—";
        const abs = Math.floor(seconds);
        const hrs = Math.floor(abs / 3600);
        const mins = Math.floor((abs % 3600) / 60);
        const secs = abs % 60;
        if (hrs > 0) {
            return `${hrs}h ${mins.toString().padStart(2, "0")}m`;
        }
        if (mins > 0) {
            return `${mins}m ${secs.toString().padStart(2, "0")}s`;
        }
        return `${secs}s`;
    }

    function applyStatusPayload(payload) {
        if (payload.streams) {
            state.streams.clear();
            payload.streams.forEach((stream) => {
                state.streams.set(stream.stream_url, stream);
            });
        }
        if (payload.active_alerts) {
            state.activeAlerts = payload.active_alerts.slice();
        }
        renderStreams();
        renderAlerts();
    }

    function applyLogs(logs) {
        if (!Array.isArray(logs)) return;
        const combined = [...logs, ...state.logs];
        combined.sort((a, b) => b.id - a.id);
        state.logs = combined.slice(0, LOG_LIMIT);
        renderLogs();
    }

    function renderStreams() {
        const container = elements.streamGrid;
        container.innerHTML = "";
        const streams = Array.from(state.streams.values()).sort((a, b) =>
            a.stream_url.localeCompare(b.stream_url)
        );
        elements.streamCount.textContent = `${streams.length} tracked`;

        if (!streams.length) {
            container.innerHTML = '<div class="empty-state">No streams configured.</div>';
            return;
        }

        for (const stream of streams) {
            const card = document.createElement("article");
            card.className = `stream-card ${stream.is_connected ? "online" : "offline"}`;

            const receivingText = stream.is_receiving_audio
                ? "Receiving audio"
                : "No audio activity";
            const statusLabel = stream.is_connected ? "Connected" : "Disconnected";
            const uptime = stream.uptime_seconds
                ? formatDuration(stream.uptime_seconds)
                : "—";

            const lastActivity = stream.last_activity
                ? formatTimestamp(stream.last_activity * 1000)
                : "Never";

            const lastDisconnect = stream.last_disconnect
                ? formatTimestamp(stream.last_disconnect * 1000)
                : "—";

            const connectedSince = stream.connected_since
                ? formatTimestamp(stream.connected_since * 1000)
                : "—";

            card.innerHTML = `
                <div class="stream-header">
                <div class="status-tag">${statusLabel}</div>
                <div class="stream-url">${stream.stream_url}</div>
                </div>
                <div class="stream-meta">
                    <div><strong>Audio:</strong> ${receivingText}</div>
                    <div><strong>Uptime:</strong> ${uptime}</div>
                    <div><strong>Connected since:</strong> ${connectedSince}</div>
                    <div><strong>Last audio:</strong> ${lastActivity}</div>
                    <div><strong>Last disconnect:</strong> ${lastDisconnect}</div>
                    <div><strong>Attempts:</strong> ${stream.connection_attempts}</div>
                    <div><strong>Last error:</strong> ${stream.last_error || "—"}</div>
                </div>
            `;
            container.appendChild(card);
        }
    }

    function secondsToHM(totalSeconds) {
        if (totalSeconds < 0 || isNaN(totalSeconds)) {
            return "Invalid input";
        }

        const hours = Math.floor(totalSeconds / 3600);
        const minutes = Math.floor((totalSeconds % 3600) / 60);

        const hoursPart = hours > 0 ? `${hours}h` : '';
        const minutesPart = minutes >= 0 ? `${minutes}m` : '';

        if (hours === 0 && minutes === 0) {
            return "0m";
        }

        return `${hoursPart}${hoursPart && minutesPart ? ' ' : ''}${minutesPart}`;
    }

    function renderAlerts() {
        const container = elements.alertList;
        container.innerHTML = "";
        const alerts = state.activeAlerts.slice().sort((a, b) =>
            b.received_at - a.received_at
        );
        elements.alertCount.textContent = alerts.length ? `${alerts.length} active` : "None";

        if (!alerts.length) {
            container.innerHTML = '<div class="empty-state">No active alerts.</div>';
            return;
        }

        for (const alert of alerts) {
            const card = document.createElement("article");
            const severity = RegExp(/(warning|watch|advisory|emergency|test)/i).exec(alert.data.event_text)?.[1]?.toLowerCase();
            card.className = `alert-card ${severity || "unknown"}`;
            card.innerHTML = `
                <div class="event-code">${alert.data.event_code}</div>
                <div class="headline">${alert.data.event_text}</div>
                <div class="meta">
                    <div>${alert.data.eas_text || "Alert received."}</div>
                    <br>
                    <div><strong>Originator:</strong> ${alert.data.originator}</div>
                    <br>
                    <div><strong>Severity:</strong> ${severity.toUpperCase()}</div>
                    <br>
                    <div><strong>Locations:</strong> ${alert.data.locations || "—"}</div>
                    <br>
                    <div><strong>Received:</strong> ${formatTimestamp(alert.received_at * 1000)}</div>
                    <br>
                    <div><strong>Expires:</strong> ${formatTimestamp(alert.expires_at * 1000)}</div>
                    <br>
                    <div><strong>Length:</strong> ${alert.purge_time.secs ? secondsToHM(alert.purge_time.secs) : "—"}</div>
                </div>
            `;
            container.appendChild(card);
        }
    }

    function renderLogs() {
        const container = elements.logList;
        container.innerHTML = "";
        const logs = state.logs;
        elements.logCount.textContent = `${logs.length} entries`;

        if (!logs.length) {
            container.innerHTML = '<div class="empty-state">No log entries captured yet.</div>';
            return;
        }

        for (const log of logs) {
            const entry = document.createElement("article");
            entry.className = "log-entry";
            entry.dataset.level = log.level || "INFO";

            const time = formatTimestamp(log.timestamp);
            const fields = Object.keys(log.fields || {}).length
                ? JSON.stringify(log.fields, null, 2)
                : "";

            entry.innerHTML = `
                <div class="log-meta">
                    <span>${log.level}</span>
                    <span>${time}</span>
                    <span>${log.target}</span>
                </div>
                <div class="log-message">${log.message || ""}</div>
                ${fields ? `<pre>${fields}</pre>` : ""}
            `;
            container.appendChild(entry);
        }
    }

    async function fetchJson(path) {
        try {
            const protocol = window.location.protocol === "https:" ? "https" : "http";
            const response = await fetch(`${protocol}://${window.API_BASE}${path}`, {
                headers: {
                    Accept: "application/json",
                    Authorization: `Bearer ${window.TOKEN}`,
                },
            });
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            return await response.json();
        } catch (err) {
            console.error(`Failed to fetch ${path}:`, err);
            return null;
        }
    }

    async function loadInitialData() {
        const [statusResponse, logResponse] = await Promise.all([
            fetchJson(`/api/status`),
            fetchJson(`/api/logs?tail=${LOG_FETCH_TAIL}`),
        ]);

        if (statusResponse) {
            applyStatusPayload(statusResponse);
        }
        if (logResponse && Array.isArray(logResponse.logs)) {
            state.logs = logResponse.logs
                .slice()
                .sort((a, b) => b.id - a.id)
                .slice(0, LOG_LIMIT);
            renderLogs();
        }
    }

    function handleWsMessage(event) {
        try {
            const payload = JSON.parse(event.data);
            if (!payload || typeof payload !== "object") return;

            switch (payload.type) {
                case "Snapshot":
                    applyStatusPayload(payload.payload);
                    if (Array.isArray(payload.payload.logs)) {
                        state.logs = payload.payload.logs
                        .slice()
                        .sort((a, b) => b.id - a.id)
                        .slice(0, LOG_LIMIT);
                        renderLogs();
                    }
                    break;
                case "Stream":
                    if (payload.payload?.stream_url) {
                        state.streams.set(payload.payload.stream_url, payload.payload);
                        renderStreams();
                    }
                    break;
                case "Log":
                    if (payload.payload) {
                        applyLogs([payload.payload]);
                    }
                    break;
                case "Alerts":
                    if (Array.isArray(payload.payload)) {
                        state.activeAlerts = payload.payload.slice();
                        renderAlerts();
                    }
                    break;
                default:
                    console.warn("Unhandled WS message type", payload.type);
            }
        } catch (err) {
            console.error("Failed to parse WS message", err);
        }
    }

    let ws;
    let reconnectDelay = 2000;
    const MAX_DELAY = 30000;

    function connectWebSocket() {
        const protocol = window.location.protocol === "https:" ? "wss" : "ws";
        const url = `${protocol}://${window.API_BASE}/ws?auth=${encodeURIComponent(window.TOKEN)}`;
        setWsStatus("Connecting…", "");

        try {
            ws = new WebSocket(url);
        } catch (err) {
            console.error("WebSocket init failed", err);
            scheduleReconnect();
            return;
        }

        ws.addEventListener("open", () => {
            setWsStatus("Live updates", "connected");
            reconnectDelay = 2000;
        });

        ws.addEventListener("message", handleWsMessage);

        ws.addEventListener("close", () => {
            setWsStatus("Disconnected", "disconnected");
            scheduleReconnect();
        });

        ws.addEventListener("error", (err) => {
            console.error("WebSocket error", err);
            ws.close();
        });
    }

    function scheduleReconnect() {
        setWsStatus(`Reconnecting in ${Math.round(reconnectDelay / 1000)}s…`, "reconnecting");
        setTimeout(connectWebSocket, reconnectDelay);
        reconnectDelay = Math.min(reconnectDelay * 1.8, MAX_DELAY);
    }

    function bootstrap() {
        loadInitialData().finally(connectWebSocket);
        setInterval(loadInitialData, 60000);
    }

    document.addEventListener("visibilitychange", () => {
        if (!document.hidden && (!ws || ws.readyState === WebSocket.CLOSED)) {
            reconnectDelay = 2000;
            connectWebSocket();
        }
    });

    bootstrap();
})();
