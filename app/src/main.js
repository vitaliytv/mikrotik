import { requestAgent, respondAgent, approveAgent, hadToolActivity } from "./agent-gateway.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

Chart.register(window["chartjs-plugin-annotation"]);

const LIVE_MAX_POINTS = 240; // 240 * 15с ≈ 1 година в пам'яті, поки застосунок відкритий

let rttChart, lossChart, flapsChart, speedChart;
let liveSamples = []; // {ts, zteAvg, zteLoss, soyeaAvg, soyeaLoss, zteRx, zteTx, soyeaRx, soyeaTx} — тільки в пам'яті, без persist
let flapEvents = []; // {ts: Date, time, channel, action}
let netwatchCache = [];
let rawLogCache = []; // {time, topics, message}

const statusEl = () => document.querySelector("#status");
const routerStatusEl = () => document.querySelector("#router-status");
const speedStatusEl = () => document.querySelector("#speed-status");

// ---------- live-моніторинг (RTT + втрати, кожні 15с, поки застосунок запущений) ----------

function fmtClock(d) {
  const pad = (x) => String(x).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function baseCfg(title, labels, datasets, yLabel, pointCount) {
  return {
    type: "line",
    data: { labels, datasets },
    options: {
      animation: false,
      plugins: {
        title: { display: true, text: title, color: "#7dd3fc", font: { size: 14 } },
        legend: { labels: { color: "#ccc" } },
      },
      scales: {
        x: { ticks: { color: "#888", maxTicksLimit: 20, maxRotation: 0 }, grid: { color: "#2a2a3e" } },
        y: {
          ticks: { color: "#ccc" },
          grid: { color: "#2a2a3e" },
          title: { display: true, text: yLabel, color: "#aaa" },
        },
      },
      elements: { point: { radius: pointCount > 60 ? 0 : 2 } },
    },
  };
}

function renderLiveCharts() {
  const labels = liveSamples.map((s) => fmtClock(s.ts));
  const n = liveSamples.length;

  const rttCfg = baseCfg(
    "Затримка (ping) мс — кожні 15с — менше краще",
    labels,
    [
      { label: "LMT", data: liveSamples.map((s) => s.zteAvg), borderColor: "#60a5fa", backgroundColor: "#60a5fa22", tension: 0.3, spanGaps: true },
      { label: "BITE", data: liveSamples.map((s) => s.soyeaAvg), borderColor: "#34d399", backgroundColor: "#34d39922", tension: 0.3, spanGaps: true },
    ],
    "мс",
    n,
  );

  const lossCfg = baseCfg(
    "Втрати пакетів % — кожні 15с — менше краще",
    labels,
    [
      { label: "LMT loss%", data: liveSamples.map((s) => s.zteLoss), borderColor: "#f87171", backgroundColor: "#f8717122", tension: 0.3, spanGaps: true },
      { label: "BITE loss%", data: liveSamples.map((s) => s.soyeaLoss), borderColor: "#fb923c", backgroundColor: "#fb923c22", tension: 0.3, spanGaps: true },
    ],
    "%",
    n,
  );

  if (rttChart) rttChart.destroy();
  if (lossChart) lossChart.destroy();
  rttChart = new Chart(document.getElementById("rtt"), rttCfg);
  lossChart = new Chart(document.getElementById("loss"), lossCfg);

  const last = liveSamples[liveSamples.length - 1];
  if (last) {
    statusEl().textContent =
      `Точок у пам'яті: ${n} | Останнє: ${fmtClock(last.ts)} ` +
      `LMT=${last.zteAvg ?? "?"}мс/${last.zteLoss}% BITE=${last.soyeaAvg ?? "?"}мс/${last.soyeaLoss}%`;
  } else {
    statusEl().textContent = "Очікую перший вимір (до 15с)...";
  }
}

function toMbps(bps) {
  return bps == null ? null : Math.round((bps / 1e6) * 100) / 100;
}

function onWanSample(sample) {
  liveSamples.push({
    ts: new Date(sample.ts),
    zteAvg: sample.zte_avg,
    zteLoss: sample.zte_loss,
    soyeaAvg: sample.soyea_avg,
    soyeaLoss: sample.soyea_loss,
    zteRx: toMbps(sample.zte_rx_bps),
    zteTx: toMbps(sample.zte_tx_bps),
    soyeaRx: toMbps(sample.soyea_rx_bps),
    soyeaTx: toMbps(sample.soyea_tx_bps),
    zteActiveMbps: sample.zte_active_mbps,
    soyeaActiveMbps: sample.soyea_active_mbps,
  });
  if (liveSamples.length > LIVE_MAX_POINTS) liveSamples.shift();
  renderLiveCharts();
  renderSpeedChart();
}

async function measureNow() {
  statusEl().textContent = "Виконую позачерговий вимір...";
  try {
    const out = await invoke("measure_now");
    statusEl().textContent = out;
  } catch (e) {
    statusEl().textContent = `Помилка: ${e}`;
  }
}

// ---------- швидкість (той самий live-потік, що й RTT/loss) ----------

function fmtMbps(v) {
  if (v == null) return "?";
  return v >= 100 ? Math.round(v) : v.toFixed(1);
}

function speedDatasets() {
  return [
    { key: "zteRx", label: "LMT ↓ (реальний)", borderColor: "#60a5fa", backgroundColor: "#60a5fa22", fill: true },
    { key: "zteTx", label: "LMT ↑ (реальний)", borderColor: "#60a5fa", borderDash: [5, 4], fill: false },
    { key: "soyeaRx", label: "BITE ↓ (реальний)", borderColor: "#34d399", backgroundColor: "#34d39922", fill: true },
    { key: "soyeaTx", label: "BITE ↑ (реальний)", borderColor: "#34d399", borderDash: [5, 4], fill: false },
    { key: "zteActiveMbps", label: "LMT (пінг-проба, не пропускна здатність)", borderColor: "#f87171", borderDash: [2, 2], fill: false },
    { key: "soyeaActiveMbps", label: "BITE (пінг-проба, не пропускна здатність)", borderColor: "#fb923c", borderDash: [2, 2], fill: false },
  ];
}

function channelActive(chan) {
  const nw = netwatchCache.find((n) => n.channel === chan);
  return nw ? nw.status === "up" : true;
}

function renderSpeedChart() {
  const mode = document.querySelector("#speed-mode").value;
  const labels = liveSamples.map((s) => fmtClock(s.ts));
  const n = liveSamples.length;

  const datasets = speedDatasets()
    .filter((d) => {
      if (mode !== "active") return true;
      return channelActive(d.key.startsWith("zte") ? "zte" : "soyea");
    })
    .map((d) => ({
      label: d.label + (channelActive(d.key.startsWith("zte") ? "zte" : "soyea") ? "" : " (вимкнено)"),
      data: liveSamples.map((s) => s[d.key]),
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
          title: { display: true, text: "Швидкість WAN, Mbps — кожні 15с", color: "#7dd3fc", font: { size: 14 } },
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
    speedStatusEl().textContent =
      `LMT ↓${fmtMbps(last.zteRx)} ↑${fmtMbps(last.zteTx)} | ` +
      `BITE ↓${fmtMbps(last.soyeaRx)} ↑${fmtMbps(last.soyeaTx)} Mbps`;
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

function renderNetwatchCards(netwatch) {
  const el = document.getElementById("netwatch-cards");
  el.innerHTML = "";
  const nameFor = { zte: "LMT (WAN1)", soyea: "BITE (WAN2)" };
  for (const nw of netwatch) {
    const card = document.createElement("div");
    card.className = `nw-card ${nw.status === "up" ? "up" : "down"}`;
    card.innerHTML = `<div class="title">${nameFor[nw.channel] || nw.channel} — ${nw.status}</div>
      <div class="detail">з ${nw.since || "?"} | ${nw.packet_count} пінгів / ${nw.interval} / поріг ${nw.thr_loss_percent}%</div>`;
    el.appendChild(card);
  }
}

function renderFlapsChart(events) {
  const buckets = {};
  for (const ev of events) {
    if (ev.action !== "down") continue; // рахуємо тільки моменти падіння каналу
    const b = hourBucket(ev.ts);
    buckets[b] = buckets[b] || { zte: 0, soyea: 0 };
    buckets[b][ev.channel] = (buckets[b][ev.channel] || 0) + 1;
  }
  const labels = Object.keys(buckets).sort();
  const zte = labels.map((l) => buckets[l].zte || 0);
  const soyea = labels.map((l) => buckets[l].soyea || 0);

  if (flapsChart) flapsChart.destroy();
  flapsChart = new Chart(document.getElementById("flaps"), {
    type: "bar",
    data: {
      labels,
      datasets: [
        { label: "LMT флапи/год", data: zte, backgroundColor: "#f87171" },
        { label: "BITE флапи/год", data: soyea, backgroundColor: "#fb923c" },
      ],
    },
    options: {
      animation: false,
      plugins: {
        title: { display: true, text: "Частота netwatch flap-подій по годинах", color: "#7dd3fc", font: { size: 14 } },
        legend: { labels: { color: "#ccc" } },
      },
      scales: {
        x: { stacked: true, ticks: { color: "#888", maxRotation: 60, minRotation: 60 }, grid: { color: "#2a2a3e" } },
        y: { stacked: true, ticks: { color: "#ccc" }, grid: { color: "#2a2a3e" }, title: { display: true, text: "к-сть", color: "#aaa" } },
      },
    },
  });
}

function renderEventsTable(events) {
  const body = document.getElementById("events-body");
  body.innerHTML = "";
  const nameFor = { zte: "LMT", soyea: "BITE" };
  const recent = events.slice(-300).reverse();
  for (const ev of recent) {
    const tr = document.createElement("tr");
    tr.className = ev.action;
    tr.innerHTML = `<td>${ev.time}</td><td>${nameFor[ev.channel] || ev.channel}</td><td>${ev.action === "down" ? "⛔ вимкнено" : "✅ відновлено"}</td>`;
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
    netwatchCache = data.netwatch || [];
    flapEvents = (data.flap_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    rawLogCache = data.raw_log || [];
    renderNetwatchCards(netwatchCache);
    renderFlapsChart(flapEvents);
    renderEventsTable(flapEvents);
    if (!document.getElementById("raw-log-box").hidden) renderRawLog();
    const downCount = flapEvents.filter((e) => e.action === "down").length;
    routerStatusEl().textContent = `Проаналізовано ${data.log_total_lines} рядків логу | flap-подій: ${downCount}`;
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

function showApproval(requestId, pendingApproval) {
  const box = document.getElementById("agent-approval");
  const text = document.getElementById("agent-approval-text");
  box.hidden = false;
  text.textContent = `Потрібне підтвердження: ${pendingApproval.tool}(${JSON.stringify(pendingApproval.input)})`;
  box.dataset.requestId = requestId;
}

function hideApproval() {
  const box = document.getElementById("agent-approval");
  box.hidden = true;
  delete box.dataset.requestId;
}

function renderAgentResult(result) {
  if (result.error) {
    appendAgentMessage("error", result.error);
  } else if (result.status === "needs_clarification" && result.question) {
    appendAgentMessage("agent", result.question);
  } else if (result.status === "needs_approval" && result.pendingApproval) {
    appendAgentMessage("agent", "Потрібне підтвердження людини для деструктивної дії.");
    showApproval(result.requestId, result.pendingApproval);
  } else if (result.summary) {
    appendAgentMessage("agent", result.summary);
  } else {
    appendAgentMessage("agent", `(статус: ${result.status})`);
  }

  lastRequestId = result.requestId ?? lastRequestId;
  lastStatus = result.status;

  if (result.status !== "needs_approval") hideApproval();
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

async function decideApproval(approve) {
  const box = document.getElementById("agent-approval");
  const requestId = box.dataset.requestId;
  if (!requestId) return;
  hideApproval();
  const result = await approveAgent(requestId, approve);
  renderAgentResult(result);
}

// ---------- init ----------

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#measure").addEventListener("click", measureNow);
  document.querySelector("#refresh-router").addEventListener("click", loadRouterLog);

  document.getElementById("toggle-raw-log").addEventListener("click", () => {
    const box = document.getElementById("raw-log-box");
    box.hidden = !box.hidden;
    document.getElementById("toggle-raw-log").textContent = box.hidden
      ? "Показати лог MikroTik"
      : "Сховати лог MikroTik";
    // Force a fresh fetch on open instead of rendering whatever's cached —
    // otherwise opening it right after launch (before the first 60s tick)
    // shows a stale/empty "0 рядків".
    if (!box.hidden) loadRouterLog();
  });
  document.getElementById("raw-log-filter").addEventListener("input", renderRawLog);
  document.querySelector("#speed-mode").addEventListener("change", renderSpeedChart);

  renderSpeedChart();
  renderLiveCharts();

  document.getElementById("agent-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const input = document.getElementById("agent-text");
    const text = input.value.trim();
    if (!text) return;
    input.value = "";
    sendAgentMessage(text);
  });
  document.getElementById("agent-approve").addEventListener("click", () => decideApproval(true));
  document.getElementById("agent-reject").addEventListener("click", () => decideApproval(false));

  listen("wan-sample", (event) => onWanSample(event.payload));

  loadRouterLog();
  setInterval(loadRouterLog, 60000);
});
