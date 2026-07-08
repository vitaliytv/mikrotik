const { invoke } = window.__TAURI__.core;

Chart.register(window["chartjs-plugin-annotation"]);

const RTT_AVG_BAD = 150;
const RTT_MAX_BAD = 160;
const RTT_AVG_GOOD = 100;

let rttChart, lossChart, flapsChart;
let csvRows = []; // {ts: Date, zteAvg, zteLoss, zteActive, soyeaAvg, soyeaLoss, soyeaActive}
let flapEvents = []; // {ts: Date, time, channel, action}
let netwatchCache = [];

const statusEl = () => document.querySelector("#status");
const routerStatusEl = () => document.querySelector("#router-status");

// ---------- період ----------

function getPeriodRange() {
  const sel = document.querySelector("#period").value;
  const now = new Date();
  if (sel === "all") return { from: null, to: null };
  if (sel === "custom") {
    const fromV = document.querySelector("#range-from").value;
    const toV = document.querySelector("#range-to").value;
    return {
      from: fromV ? new Date(fromV) : null,
      to: toV ? new Date(toV) : now,
    };
  }
  const hours = parseFloat(sel);
  return { from: new Date(now.getTime() - hours * 3600 * 1000), to: now };
}

function inRange(ts, range) {
  if (range.from && ts < range.from) return false;
  if (range.to && ts > range.to) return false;
  return true;
}

function renderAll() {
  const range = getPeriodRange();
  renderCsvCharts(csvRows.filter((r) => inRange(r.ts, range)));
  renderFlapsChart(flapEvents.filter((e) => inRange(e.ts, range)));
  renderEventsTable(flapEvents.filter((e) => inRange(e.ts, range)));
}

// ---------- wan_log.csv ----------

function parseCsv(text) {
  const lines = text.trim().split("\n");
  const rows = lines.slice(1).map((line) => line.split(","));
  const out = [];
  for (const r of rows) {
    const [ts, za, , zl, sa, , sl, zact, sact] = r;
    if (!ts) continue;
    out.push({
      ts: new Date(ts),
      label: ts,
      zteAvg: za ? parseFloat(za) : null,
      zteLoss: zl ? parseFloat(zl) : 100,
      zteActive: zact !== undefined && zact !== "" ? parseInt(zact, 10) : 1,
      soyeaAvg: sa ? parseFloat(sa) : null,
      soyeaLoss: sl ? parseFloat(sl) : 100,
      soyeaActive: sact !== undefined && sact !== "" ? parseInt(sact, 10) : 1,
    });
  }
  return out;
}

function fmtLabel(d) {
  return `${String(d.getDate()).padStart(2, "0")}.${String(d.getMonth() + 1).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function disabledAnnotations(activeList, color) {
  const anns = {};
  let i = 0;
  while (i < activeList.length) {
    if (activeList[i] === 0) {
      const start = i;
      while (i < activeList.length && activeList[i] === 0) i++;
      const end = i - 1;
      anns[`box_${color}_${start}`] = {
        type: "box",
        xMin: start,
        xMax: end,
        backgroundColor: color,
        borderWidth: 0,
        label: { display: false },
      };
    } else {
      i++;
    }
  }
  return anns;
}

function baseCfg(title, labels, datasets, yLabel, annotations, pointCount) {
  return {
    type: "line",
    data: { labels, datasets },
    options: {
      animation: false,
      plugins: {
        title: { display: true, text: title, color: "#7dd3fc", font: { size: 14 } },
        legend: { labels: { color: "#ccc" } },
        annotation: { annotations },
      },
      scales: {
        x: { ticks: { color: "#888", maxTicksLimit: 24, maxRotation: 0 }, grid: { color: "#2a2a3e" } },
        y: {
          ticks: { color: "#ccc" },
          grid: { color: "#2a2a3e" },
          title: { display: true, text: yLabel, color: "#aaa" },
        },
      },
      elements: { point: { radius: pointCount > 100 ? 0 : 2 } },
    },
  };
}

function renderCsvCharts(rows) {
  const labels = rows.map((r) => fmtLabel(r.ts));
  const n = rows.length;

  const annZte = disabledAnnotations(rows.map((r) => r.zteActive), "rgba(239,68,68,0.18)");
  const annSoyea = disabledAnnotations(rows.map((r) => r.soyeaActive), "rgba(251,146,60,0.18)");
  const allAnn = { ...annZte, ...annSoyea };

  const rttCfg = baseCfg(
    "Затримка (ping) мс — менше краще",
    labels,
    [
      { label: "ZTE", data: rows.map((r) => r.zteAvg), borderColor: "#60a5fa", backgroundColor: "#60a5fa22", tension: 0.3, spanGaps: true },
      { label: "Soyea", data: rows.map((r) => r.soyeaAvg), borderColor: "#34d399", backgroundColor: "#34d39922", tension: 0.3, spanGaps: true },
      { label: `avg-поріг вимкнення (${RTT_AVG_BAD}мс)`, data: Array(n).fill(RTT_AVG_BAD), borderColor: "#f8717160", borderDash: [6, 4], borderWidth: 1, pointRadius: 0, fill: false },
      { label: `spike-поріг вимкнення (${RTT_MAX_BAD}мс)`, data: Array(n).fill(RTT_MAX_BAD), borderColor: "#fb923c60", borderDash: [4, 3], borderWidth: 1, pointRadius: 0, fill: false },
      { label: `avg-поріг відновлення (${RTT_AVG_GOOD}мс)`, data: Array(n).fill(RTT_AVG_GOOD), borderColor: "#34d39950", borderDash: [3, 4], borderWidth: 1, pointRadius: 0, fill: false },
    ],
    "мс",
    allAnn,
    n,
  );

  const lossCfg = baseCfg(
    "Втрати пакетів % — менше краще",
    labels,
    [
      { label: "ZTE loss%", data: rows.map((r) => r.zteLoss), borderColor: "#f87171", backgroundColor: "#f8717122", tension: 0.3, spanGaps: true },
      { label: "Soyea loss%", data: rows.map((r) => r.soyeaLoss), borderColor: "#fb923c", backgroundColor: "#fb923c22", tension: 0.3, spanGaps: true },
    ],
    "%",
    {},
    n,
  );

  if (rttChart) rttChart.destroy();
  if (lossChart) lossChart.destroy();
  rttChart = new Chart(document.getElementById("rtt"), rttCfg);
  lossChart = new Chart(document.getElementById("loss"), lossCfg);

  const last = rows.length - 1;
  if (last >= 0) {
    statusEl().textContent =
      `Точок: ${rows.length} | Останнє: ${labels[last]} ` +
      `ZTE=${rows[last].zteAvg ?? "?"}мс Soyea=${rows[last].soyeaAvg ?? "?"}мс`;
  } else {
    statusEl().textContent = "Немає даних за цей період";
  }
}

async function loadCsv() {
  try {
    const csv = await invoke("read_wan_csv");
    csvRows = parseCsv(csv);
    renderAll();
  } catch (e) {
    statusEl().textContent = `Помилка читання CSV: ${e}`;
  }
}

async function measureNow() {
  statusEl().textContent = "Виконую вимір...";
  try {
    const out = await invoke("run_wan_monitor");
    statusEl().textContent = out.trim().split("\n").pop() || "Готово";
  } catch (e) {
    statusEl().textContent = `Помилка: ${e}`;
  }
  await loadCsv();
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
  const nameFor = { zte: "ZTE (WAN1)", soyea: "Soyea (WAN2)" };
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
        { label: "ZTE флапи/год", data: zte, backgroundColor: "#f87171" },
        { label: "Soyea флапи/год", data: soyea, backgroundColor: "#fb923c" },
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
  const nameFor = { zte: "ZTE", soyea: "Soyea" };
  const recent = events.slice(-300).reverse();
  for (const ev of recent) {
    const tr = document.createElement("tr");
    tr.className = ev.action;
    tr.innerHTML = `<td>${ev.time}</td><td>${nameFor[ev.channel] || ev.channel}</td><td>${ev.action === "down" ? "⛔ вимкнено" : "✅ відновлено"}</td>`;
    body.appendChild(tr);
  }
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
    renderNetwatchCards(netwatchCache);
    renderAll();
    const range = getPeriodRange();
    const downCount = flapEvents.filter((e) => e.action === "down" && inRange(e.ts, range)).length;
    routerStatusEl().textContent = `Проаналізовано ${data.log_total_lines} рядків логу | flap-подій за період: ${downCount}`;
  } catch (e) {
    routerStatusEl().textContent = `Помилка: ${e}`;
  }
}

// ---------- init ----------

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#refresh").addEventListener("click", loadCsv);
  document.querySelector("#measure").addEventListener("click", measureNow);
  document.querySelector("#refresh-router").addEventListener("click", loadRouterLog);

  const periodSel = document.querySelector("#period");
  const customRange = document.querySelector("#custom-range");
  periodSel.addEventListener("change", () => {
    customRange.hidden = periodSel.value !== "custom";
    if (periodSel.value !== "custom") renderAll();
  });
  document.querySelector("#apply-range").addEventListener("click", renderAll);

  loadCsv();
  loadRouterLog();
  setInterval(loadCsv, 30000);
  setInterval(loadRouterLog, 60000);
});
