<template>
  <div class="agent-toolbar">
    <q-btn flat dense round icon="sym_o_smart_toy" title="Агент" @click="agentOpen = true" />
    <q-btn flat dense round icon="sym_o_history" title="Журнал запитів агента" @click="auditOpen = true" />
  </div>

  <header style="margin-top: 28px">
    <h1>Трафік на WAN-інтерфейсах (середнє за 1 хв)</h1>
    <div class="controls">
      <span>{{ speedStatus }}</span>
    </div>
  </header>
  <div class="charts">
    <div class="box"><canvas ref="speedCanvasEl"></canvas></div>
  </div>

  <header style="margin-top: 28px">
    <h1>Стан dual-WAN scheduler</h1>
    <div class="controls">
      <span>{{ routerStatus }}</span>
    </div>
  </header>
  <div class="charts">
    <div class="box"><canvas ref="flapsCanvasEl"></canvas></div>
    <div class="box"><canvas ref="qualityCanvasEl"></canvas></div>
    <div class="box">
      <div class="netwatch-cards">
        <div v-for="card in controllerCards" :key="card.channel" :class="['nw-card', card.status]">
          <div class="title">{{ card.title }}</div>
          <div class="detail">{{ card.detail1 }}</div>
          <div class="detail">{{ card.detail2 }}</div>
        </div>
      </div>
      <table class="events">
        <thead>
          <tr>
            <th>Час</th>
            <th>Primary WAN</th>
            <th>Причина</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(ev, i) in recentEvents" :key="i" :class="ev.state === 'bite' ? 'down' : 'up'">
            <td>{{ ev.time }}</td>
            <td>{{ ev.state === "bite" ? "BITE" : "LMT" }}</td>
            <td>{{ ev.reason || "—" }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
  <div class="box raw-log">
    <div class="raw-log-toolbar">
      <span>Сирий лог MikroTik (останні <span>{{ filteredRawLog.length }}</span> рядків)</span>
      <button @click="loadRouterLog">Оновити лог</button>
      <button @click="toggleRawLog">{{ rawLogVisible ? "Сховати лог MikroTik" : "Показати лог MikroTik" }}</button>
      <input type="text" v-model="rawLogFilter" placeholder="Фільтр по тексту…" autocomplete="off" />
    </div>
    <pre class="raw-log-view" v-show="rawLogVisible">{{ rawLogText }}</pre>
  </div>

  <AgentDialog v-model="agentOpen" @ran="loadRouterLog" :agent="agent" prompt-hint="наприклад: чи є зараз проблеми зі швидкістю?" />
  <AuditDialog v-model="auditOpen" :agent="agent" />
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { AgentDialog, AuditDialog } from "@7n/tauri-components/components";
import { useUpdater } from "@7n/tauri-components/vue";
import { useAgent } from "./composables/use-agent.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

useUpdater();

Chart.register(window["chartjs-plugin-annotation"]);

const LIVE_MAX_POINTS = 240; // 240 * 15с ≈ 1 година в пам'яті, поки застосунок відкритий
const TRAFFIC_AVERAGE_WINDOW = 4; // 4 * 15с = 1 хвилина

const speedCanvasEl = ref(null);
const flapsCanvasEl = ref(null);
const qualityCanvasEl = ref(null);
let flapsChart, qualityChart, speedChart;

let liveSamples = []; // Passive interface counters only; history lives in memory while the viewer is open.
let qualityEvents = []; // {ts: Date, time, status} — only feeds the quality chart, not displayed directly

const speedStatus = ref("—");
const routerStatus = ref("—");
const controllerCards = ref([]);
const events = ref([]); // switch events: {ts, time, state, reason}
const rawLogCache = ref([]); // {time, topics, message}
const rawLogFilter = ref("");
const rawLogVisible = ref(false);

const agent = useAgent();
const agentOpen = ref(false);
const auditOpen = ref(false);

const filteredRawLog = computed(() => {
  const filter = rawLogFilter.value.trim().toLowerCase();
  return filter
    ? rawLogCache.value.filter((r) => `${r.time} ${r.topics} ${r.message}`.toLowerCase().includes(filter))
    : rawLogCache.value;
});

const rawLogText = computed(() =>
  filteredRawLog.value
    .slice()
    .reverse()
    .map((r) => `${r.time}  [${r.topics}]  ${r.message}`)
    .join("\n"),
);

const recentEvents = computed(() => events.value.slice(-300).reverse());

let refreshTimer = null;

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

  const datasets = speedDatasets().map((d) => ({
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
    speedChart = new Chart(speedCanvasEl.value, {
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
    speedStatus.value =
      `LMT ↓${fmtMbps(average("zteRx"))} ↑${fmtMbps(average("zteTx"))} | ` +
      `BITE ↓${fmtMbps(average("soyeaRx"))} ↑${fmtMbps(average("soyeaTx"))} Mbps (середнє за 1 хв)`;
  } else {
    speedStatus.value = "Очікую перший вимір (до 15с)...";
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

function buildControllerCards(data) {
  const nameFor = { zte: "LMT (WAN1)", soyea: "BITE (WAN2)" };
  const controller = data.controller || {};
  const dhcp = Object.fromEntries((data.dhcp || []).map((row) => [row.channel, row]));
  const probes = data.probes || [];
  const routes = data.routes || [];
  return ["zte", "soyea"].map((channel) => {
    const channelProbes = probes.filter((probe) => probe.channel === channel);
    const lease = dhcp[channel] || {};
    const mainRoute = routes.find((route) => route.channel === channel && route.table === "main") || {};
    const primary = controller.state === (channel === "zte" ? "lmt" : "bite");
    const measured = channel === "zte";
    const lmtGood = channelProbes.some((probe) => Number(probe.received || 0) >= 2);
    const probeDetail = channelProbes
      .map((probe) => `${probe.target}: ${probe.received || "0"}/3, ${probe.loss_percent || "?"}%, ${probe.avg_rtt || "?"}`)
      .join(" | ");
    return {
      channel,
      status: measured && !lmtGood ? "down" : "up",
      title: `${nameFor[channel]} — ${primary ? "primary" : "reserve"}`,
      detail1: measured ? probeDetail : "blind reserve: доступність не вимірюється",
      detail2: `DHCP ${lease.status || "?"}, main distance ${mainRoute.distance || "?"}, gw ${lease.gateway || mainRoute.gateway || "?"}`,
    };
  });
}

function renderFlapsChart(evs) {
  const buckets = {};
  for (const ev of evs) {
    const b = hourBucket(ev.ts);
    buckets[b] = buckets[b] || { lmt: 0, bite: 0 };
    buckets[b][ev.state] = (buckets[b][ev.state] || 0) + 1;
  }
  const labels = Object.keys(buckets).sort();
  const lmt = labels.map((l) => buckets[l].lmt || 0);
  const bite = labels.map((l) => buckets[l].bite || 0);
  if (flapsChart) flapsChart.destroy();
  flapsChart = new Chart(flapsCanvasEl.value, {
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

function renderQualityChart(evs) {
  const buckets = {};
  for (const ev of evs) {
    const bucket = hourBucket(ev.ts);
    buckets[bucket] = buckets[bucket] || { degraded: 0, recovered: 0 };
    if (ev.status === "lmt-loss-degraded") buckets[bucket].degraded += 1;
    if (ev.status === "lmt-loss-recovered") buckets[bucket].recovered += 1;
  }
  const labels = Object.keys(buckets).sort();
  const degraded = labels.map((label) => buckets[label].degraded);
  const recovered = labels.map((label) => buckets[label].recovered);
  if (qualityChart) qualityChart.destroy();
  qualityChart = new Chart(qualityCanvasEl.value, {
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

async function loadRouterLog() {
  routerStatus.value = "Читаю лог роутера...";
  try {
    const raw = await invoke("read_router_log");
    const data = JSON.parse(raw);
    if (data.error) {
      routerStatus.value = `Помилка: ${data.error}`;
      return;
    }
    events.value = (data.switch_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    qualityEvents = (data.quality_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    rawLogCache.value = data.raw_log || [];
    controllerCards.value = buildControllerCards(data);
    renderFlapsChart(events.value);
    renderQualityChart(qualityEvents);
    const controller = data.controller || {};
    routerStatus.value = `scheduler ${controller.scheduler_enabled === "true" ? "активний" : "недоступний"} | ${controller.interval || "?"} | primary: ${(controller.state || "?").toUpperCase()} | запусків: ${controller.scheduler_runs || "?"}`;
  } catch (e) {
    routerStatus.value = `Помилка: ${e}`;
  }
}

function toggleRawLog() {
  rawLogVisible.value = !rawLogVisible.value;
  // Force a fresh fetch on open instead of showing whatever's cached — otherwise
  // opening it right after launch (before the first 60s tick) shows a stale/empty state.
  if (rawLogVisible.value) loadRouterLog();
}

// ---------- init ----------

onMounted(() => {
  renderSpeedChart();
  listen("wan-sample", (event) => onWanSample(event.payload));
  loadRouterLog();
  refreshTimer = setInterval(loadRouterLog, 60000);
});

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer);
});
</script>
