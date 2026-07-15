import { requestAgent, respondAgent, hadToolActivity } from "./agent-gateway.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

Chart.register(window["chartjs-plugin-annotation"]);

const LIVE_MAX_POINTS = 240; // 240 * 15с ≈ 1 година в пам'яті, поки застосунок відкритий
const TRAFFIC_AVERAGE_WINDOW = 4; // 4 * 15с = 1 хвилина

let flapsChart, qualityChart, speedChart;
let liveSamples = []; // Passive interface counters only; history lives in memory while the viewer is open.
let switchEvents = []; // {ts: Date, time, state, reason}
let qualityEvents = []; // {ts: Date, time, status}
let rawLogCache = []; // {time, topics, message}

const routerStatusEl = () => document.querySelector("#router-status");
const speedStatusEl = () => document.querySelector("#speed-status");

// ---------- пасивний моніторинг трафіку (кожні 15с, поки застосунок відкритий) ----------

function fmtClock(d) {
  const pad = (x) => String(x).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function toMbps(bps) {
  return bps == null ? null : Math.round((bps / 1e6) * 100) / 100;
}

function onWanSample(sample) {
  liveSamples.push({
    ts: new Date(sample.ts),
    zteRx: toMbps(sample.zte_rx_bps),
    zteTx: toMbps(sample.zte_tx_bps),
    soyeaRx: toMbps(sample.soyea_rx_bps),
    soyeaTx: toMbps(sample.soyea_tx_bps),
  });
  if (liveSamples.length > LIVE_MAX_POINTS) liveSamples.shift();
  renderSpeedChart();
}

// ---------- фактичний трафік інтерфейсів ----------

function fmtMbps(v) {
  if (v == null) return "?";
  return v >= 100 ? Math.round(v) : v.toFixed(1);
}

function movingAverage(values, windowSize = TRAFFIC_AVERAGE_WINDOW) {
  return values.map((_, index) => {
    const window = values.slice(Math.max(0, index - windowSize + 1), index + 1).filter((value) => value != null);
    if (!window.length) return null;
    return window.reduce((sum, value) => sum + value, 0) / window.length;
  });
}

function speedDatasets() {
  return [
    { key: "zteRx", label: "LMT ↓ (реальний)", borderColor: "#60a5fa", backgroundColor: "#60a5fa22", fill: true },
    { key: "zteTx", label: "LMT ↑ (реальний)", borderColor: "#60a5fa", borderDash: [5, 4], fill: false },
    { key: "soyeaRx", label: "BITE ↓ (реальний)", borderColor: "#34d399", backgroundColor: "#34d39922", fill: true },
    { key: "soyeaTx", label: "BITE ↑ (реальний)", borderColor: "#34d399", borderDash: [5, 4], fill: false },
  ];
}

function renderSpeedChart() {
  const labels = liveSamples.map((s) => fmtClock(s.ts));
  const n = liveSamples.length;

  const datasets = speedDatasets()
    .map((d) => ({
      label: d.label,
      data: movingAverage(liveSamples.map((s) => s[d.key])),
      borderColor: d.borderColor,
      backgroundColor: d.backgroundColor,
      borderDash: d.borderDash,
      fill: d.fill,
      tension: 0.3,
      borderWidth: 1.5,
      spanGaps: true,
    }));

  if (speedChart) {
    speedChart.data.labels = labels;
    speedChart.data.datasets = datasets;
    speedChart.options.elements.point.radius = n > 100 ? 0 : 2;
    speedChart.update("none");
  } else {
    speedChart = new Chart(document.getElementById("speed"), {
      type: "line",
      data: { labels, datasets },
      options: {
        animation: false,
        plugins: {
          title: { display: true, text: "Середній трафік інтерфейсів за 1 хв, Mbps", color: "#7dd3fc", font: { size: 14 } },
          legend: { labels: { color: "#ccc" } },
        },
        scales: {
          x: { ticks: { color: "#888", maxTicksLimit: 12, maxRotation: 0 }, grid: { color: "#2a2a3e" } },
          y: {
            beginAtZero: true,
            ticks: { color: "#ccc" },
            grid: { color: "#2a2a3e" },
            title: { display: true, text: "Mbps", color: "#aaa" },
          },
        },
        elements: { point: { radius: n > 100 ? 0 : 2 } },
      },
    });
  }

  const last = liveSamples[liveSamples.length - 1];
  if (last) {
    const average = (key) => movingAverage(liveSamples.map((sample) => sample[key])).at(-1);
    speedStatusEl().textContent =
      `LMT ↓${fmtMbps(average("zteRx"))} ↑${fmtMbps(average("zteTx"))} | ` +
      `BITE ↓${fmtMbps(average("soyeaRx"))} ↑${fmtMbps(average("soyeaTx"))} Mbps (середнє за 1 хв)`;
  } else {
    speedStatusEl().textContent = "Очікую перший вимір (до 15с)...";
  }
}

// ---------- лог роутера ----------

function parseLogTime(t) {
  const m = t.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})$/);
  if (m) return new Date(`${m[1]}T${m[2]}`);
  const m2 = t.match(/^(\d{2}:\d{2}:\d{2})$/);
  if (m2) {
    const now = new Date();
    const [h, mi, s] = m2[1].split(":").map(Number);
    return new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, mi, s);
  }
  return new Date(t);
}

function hourBucket(d) {
  const pad = (x) => String(x).padStart(2, "0");
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)} ${pad(d.getHours())}:00`;
}

function renderControllerCards(data) {
  const el = document.getElementById("controller-cards");
  el.innerHTML = "";
  const nameFor = { zte: "LMT (WAN1)", soyea: "BITE (WAN2)" };
  const controller = data.controller || {};
  const dhcp = Object.fromEntries((data.dhcp || []).map((row) => [row.channel, row]));
  const probes = data.probes || [];
  const routes = data.routes || [];
  for (const channel of ["zte", "soyea"]) {
    const channelProbes = probes.filter((probe) => probe.channel === channel);
    const lease = dhcp[channel] || {};
    const mainRoute = routes.find((route) => route.channel === channel && route.table === "main") || {};
    const primary = controller.state === (channel === "zte" ? "lmt" : "bite");
    const card = document.createElement("div");
    const measured = channel === "zte";
    const lmtGood = channelProbes.some((probe) => Number(probe.received || 0) >= 2);
    const probeDetail = channelProbes
      .map((probe) => `${probe.target}: ${probe.received || "0"}/3, ${probe.loss_percent || "?"}%, ${probe.avg_rtt || "?"}`)
      .join(" | ");
    card.className = `nw-card ${measured && !lmtGood ? "down" : "up"}`;
    card.innerHTML = `<div class="title">${nameFor[channel]} — ${primary ? "primary" : "reserve"}</div>
      <div class="detail">${measured ? probeDetail : "blind reserve: доступність не вимірюється"}</div>
      <div class="detail">DHCP ${lease.status || "?"}, main distance ${mainRoute.distance || "?"}, gw ${lease.gateway || mainRoute.gateway || "?"}</div>`;
    el.appendChild(card);
  }
}

function renderFlapsChart(events) {
  const buckets = {};
  for (const ev of events) {
    const b = hourBucket(ev.ts);
    buckets[b] = buckets[b] || { lmt: 0, bite: 0 };
    buckets[b][ev.state] = (buckets[b][ev.state] || 0) + 1;
  }
  const labels = Object.keys(buckets).sort();
  const lmt = labels.map((l) => buckets[l].lmt || 0);
  const bite = labels.map((l) => buckets[l].bite || 0);
  if (flapsChart) flapsChart.destroy();
  flapsChart = new Chart(document.getElementById("flaps"), {
    type: "bar",
    data: {
      labels,
      datasets: [
        { label: "LMT primary", data: lmt, backgroundColor: "#60a5fa" },
        { label: "BITE primary", data: bite, backgroundColor: "#34d399" },
      ],
    },
    options: {
      animation: false,
      plugins: {
          title: { display: true, text: "Перемикання primary WAN по годинах", color: "#7dd3fc", font: { size: 14 } },
        legend: { labels: { color: "#ccc" } },
      },
      scales: {
        x: { stacked: true, ticks: { color: "#888", maxRotation: 60, minRotation: 60 }, grid: { color: "#2a2a3e" } },
        y: { stacked: true, ticks: { color: "#ccc" }, grid: { color: "#2a2a3e" }, title: { display: true, text: "к-сть", color: "#aaa" } },
      },
    },
  });
}

function renderQualityChart(events) {
  const buckets = {};
  for (const ev of events) {
    const bucket = hourBucket(ev.ts);
    buckets[bucket] = buckets[bucket] || { degraded: 0, recovered: 0 };
    if (ev.status === "lmt-loss-degraded") buckets[bucket].degraded += 1;
    if (ev.status === "lmt-loss-recovered") buckets[bucket].recovered += 1;
  }
  const labels = Object.keys(buckets).sort();
  const degraded = labels.map((label) => buckets[label].degraded);
  const recovered = labels.map((label) => buckets[label].recovered);
  if (qualityChart) qualityChart.destroy();
  qualityChart = new Chart(document.getElementById("quality"), {
    type: "bar",
    data: {
      labels,
      datasets: [
        { label: "LMT loss degraded", data: degraded, backgroundColor: "#f59e0b" },
        { label: "LMT відновлено", data: recovered, backgroundColor: "#34d399" },
      ],
    },
    options: {
      animation: false,
      plugins: {
        title: { display: true, text: "LMT quality-події: втрата на обох probes 60 с", color: "#7dd3fc", font: { size: 14 } },
        legend: { labels: { color: "#ccc" } },
      },
      scales: {
        x: { stacked: true, ticks: { color: "#888", maxRotation: 60, minRotation: 60 }, grid: { color: "#2a2a3e" } },
        y: { stacked: true, ticks: { color: "#ccc", precision: 0 }, grid: { color: "#2a2a3e" }, title: { display: true, text: "події", color: "#aaa" } },
      },
    },
  });
}

function renderEventsTable(events) {
  const body = document.getElementById("events-body");
  body.innerHTML = "";
  const recent = events.slice(-300).reverse();
  for (const ev of recent) {
    const tr = document.createElement("tr");
    tr.className = ev.state === "bite" ? "down" : "up";
    tr.innerHTML = `<td>${ev.time}</td><td>${ev.state === "bite" ? "BITE" : "LMT"}</td><td>${ev.reason || "—"}</td>`;
    body.appendChild(tr);
  }
}

function renderRawLog() {
  const filter = document.getElementById("raw-log-filter").value.trim().toLowerCase();
  const rows = filter
    ? rawLogCache.filter((r) => `${r.time} ${r.topics} ${r.message}`.toLowerCase().includes(filter))
    : rawLogCache;
  document.getElementById("raw-log-count").textContent = rows.length;
  document.getElementById("raw-log-view").textContent = rows
    .slice()
    .reverse()
    .map((r) => `${r.time}  [${r.topics}]  ${r.message}`)
    .join("\n");
}

async function loadRouterLog() {
  routerStatusEl().textContent = "Читаю лог роутера...";
  try {
    const raw = await invoke("read_router_log");
    const data = JSON.parse(raw);
    if (data.error) {
      routerStatusEl().textContent = `Помилка: ${data.error}`;
      return;
    }
    switchEvents = (data.switch_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    qualityEvents = (data.quality_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    rawLogCache = data.raw_log || [];
    renderControllerCards(data);
    renderFlapsChart(switchEvents);
    renderQualityChart(qualityEvents);
    renderEventsTable(switchEvents);
    if (!document.getElementById("raw-log-view").hidden) renderRawLog();
    const controller = data.controller || {};
    routerStatusEl().textContent = `scheduler ${controller.scheduler_enabled === "true" ? "активний" : "недоступний"} | ${controller.interval || "?"} | primary: ${(controller.state || "?").toUpperCase()} | запусків: ${controller.scheduler_runs || "?"}`;
  } catch (e) {
    routerStatusEl().textContent = `Помилка: ${e}`;
  }
}

// ---------- агент ----------

let lastRequestId = null;
let lastStatus = null;

function agentMessagesEl() {
  return document.getElementById("agent-messages");
}

function appendAgentMessage(kind, text) {
  const el = document.createElement("div");
  el.className = `agent-msg ${kind}`;
  el.textContent = text;
  agentMessagesEl().appendChild(el);
  agentMessagesEl().scrollTop = agentMessagesEl().scrollHeight;
}

function renderAgentResult(result) {
  if (result.error) {
    appendAgentMessage("error", result.error);
  } else if (result.status === "needs_clarification" && result.question) {
    appendAgentMessage("agent", result.question);
  } else if (result.summary) {
    appendAgentMessage("agent", result.summary);
  } else {
    appendAgentMessage("agent", `(статус: ${result.status})`);
  }

  lastRequestId = result.requestId ?? lastRequestId;
  lastStatus = result.status;

  if (hadToolActivity(result)) {
    loadRouterLog();
  }
}

async function sendAgentMessage(text) {
  appendAgentMessage("user", text);
  const sendBtn = document.getElementById("agent-send");
  sendBtn.disabled = true;
  try {
    const result =
      lastStatus === "needs_clarification" && lastRequestId
        ? await respondAgent(lastRequestId, text)
        : await requestAgent(text);
    renderAgentResult(result);
  } catch (e) {
    appendAgentMessage("error", String(e?.message ?? e));
  } finally {
    sendBtn.disabled = false;
  }
}

// ---------- init ----------

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#refresh-router").addEventListener("click", loadRouterLog);

  document.getElementById("toggle-raw-log").addEventListener("click", () => {
    const view = document.getElementById("raw-log-view");
    view.hidden = !view.hidden;
    document.getElementById("toggle-raw-log").textContent = view.hidden
      ? "Показати лог MikroTik"
      : "Сховати лог MikroTik";
    // Force a fresh fetch on open instead of rendering whatever's cached —
    // otherwise opening it right after launch (before the first 60s tick)
    // shows a stale/empty "0 рядків".
    if (!view.hidden) loadRouterLog();
  });
  document.getElementById("raw-log-filter").addEventListener("input", renderRawLog);
  renderSpeedChart();

  document.getElementById("agent-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const input = document.getElementById("agent-text");
    const text = input.value.trim();
    if (!text) return;
    input.value = "";
    sendAgentMessage(text);
  });
  listen("wan-sample", (event) => onWanSample(event.payload));

  loadRouterLog();
  setInterval(loadRouterLog, 60000);
});
