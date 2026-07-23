<template>
  <div class="agent-toolbar">
    <q-btn flat dense round icon="sym_o_smart_toy" title="Агент" @click="agentOpen = true" />
    <q-btn flat dense round icon="sym_o_history" title="Журнал запитів агента" @click="auditOpen = true" />
  </div>

  <section class="diagnostic-mode" aria-label="Діагностичний режим">
    <header>
      <div>
        <h1>Діагностичний режим</h1>
        <div class="controls"><span>{{ diagnosticStatus }}</span></div>
      </div>
      <div class="diagnostic-actions">
        <q-btn
          dense
          unelevated
          :color="diagnosticRunning ? 'negative' : 'primary'"
          :icon="diagnosticRunning ? 'sym_o_pause_circle' : 'sym_o_monitor_heart'"
          :label="diagnosticRunning ? 'Зупинити' : 'Почати очікування'"
          @click="toggleDiagnostic"
        >
          <q-tooltip>{{ diagnosticRunning ? 'Зупинити перевірку кожні 5 секунд' : 'Перевіряти RouterOS кожні 5 секунд' }}</q-tooltip>
        </q-btn>
        <q-btn flat dense round icon="sym_o_refresh" title="Перевірити зараз" @click="pollDiagnostic" />
        <q-btn
          dense
          outline
          icon="sym_o_summarize"
          label="Зібрати звіт"
          :loading="reportBusy"
          @click="captureDiagnosticReport()"
        >
          <q-tooltip>Зберегти повний read-only звіт для передачі в чат</q-tooltip>
        </q-btn>
        <q-btn
          dense
          outline
          color="warning"
          icon="sym_o_build"
          label="Виправити scheduler"
          :disable="!diagnosticSnapshot?.api_reachable"
          @click="repairDialog = true"
        >
          <q-tooltip>Виправити підрахунок timeout у DUALWAN-health</q-tooltip>
        </q-btn>
        <q-btn dense unelevated color="negative" icon="sym_o_emergency" label="Зафіксувати BITE" :disable="!diagnosticSnapshot?.api_reachable" @click="holdBiteDialog = true">
          <q-tooltip>Зробити BITE primary і зупинити scheduler</q-tooltip>
        </q-btn>
        <q-btn dense unelevated color="positive" icon="sym_o_restart_alt" label="Увімкнути sticky авто" :disable="!diagnosticSnapshot?.api_reachable" @click="resumeFailoverDialog = true">
          <q-tooltip>LMT стартує primary; перевіряється тільки активний WAN, без auto-failback</q-tooltip>
        </q-btn>
        <q-btn dense outline color="warning" icon="sym_o_swap_horiz" label="Наступний WAN" :disable="!diagnosticSnapshot?.api_reachable" @click="forceNextDialog = true">
          <q-tooltip>Негайно зробити резервний WAN primary без перевірки</q-tooltip>
        </q-btn>
      </div>
    </header>
    <div class="diagnostic-grid">
      <div :class="['diagnostic-cell', diagnosticLevel]">
        <span class="diagnostic-label">RouterOS API</span>
        <strong>{{ diagnosticSnapshot?.api_reachable ? 'Доступний' : 'Недоступний' }}</strong>
        <small>{{ diagnosticSnapshot?.endpoint || 'Очікую перевірку' }}</small>
      </div>
      <div class="diagnostic-cell">
        <span class="diagnostic-label">Контролер</span>
        <strong>{{ diagnosticControllerLabel }}</strong>
        <small>{{ diagnosticSchedulerLabel }}</small>
      </div>
      <div class="diagnostic-cell">
        <span class="diagnostic-label">Остання перевірка</span>
        <strong>{{ diagnosticLastChecked }}</strong>
        <small>{{ diagnosticLatencyLabel }}</small>
      </div>
    </div>
    <div v-if="diagnosticError" class="diagnostic-error">{{ diagnosticError }}</div>
    <table class="events diagnostic-history">
      <thead>
        <tr><th>Час</th><th>Подія</th><th>Деталі</th></tr>
      </thead>
      <tbody>
        <tr v-for="entry in diagnosticHistory" :key="entry.id" :class="entry.level">
          <td>{{ entry.time }}</td><td>{{ entry.label }}</td><td>{{ entry.detail }}</td>
        </tr>
        <tr v-if="!diagnosticHistory.length"><td colspan="3">Подій ще немає.</td></tr>
      </tbody>
    </table>
    <div v-if="diagnosticReport" class="diagnostic-report">
      <div class="diagnostic-report-toolbar">
        <span>Автономний звіт для чату</span>
        <q-btn flat dense icon="sym_o_content_copy" label="Скопіювати" @click="copyDiagnosticReport" />
      </div>
      <textarea ref="diagnosticReportEl" readonly :value="diagnosticReport" aria-label="Автономний звіт діагностики"></textarea>
    </div>
  </section>

  <q-dialog v-model="repairDialog">
    <q-card class="repair-dialog">
      <q-card-section>
        <div class="text-subtitle1">Виправити DUALWAN-health?</div>
        <div class="text-body2">Будуть змінені лише два ping-лічильники: timeout більше не рахуватиметься як успішна відповідь. Маршрути та DHCP leases зараз не змінюються.</div>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="Скасувати" v-close-popup />
        <q-btn color="warning" unelevated label="Виправити" :loading="repairBusy" @click="repairFailoverPing" />
      </q-card-actions>
    </q-card>
  </q-dialog>

  <q-dialog v-model="holdBiteDialog">
    <q-card class="repair-dialog">
      <q-card-section>
        <div class="text-subtitle1">Зафіксувати BITE як primary?</div>
        <div class="text-body2">BITE отримає main distance 1, LMT стане резервом, а scheduler буде вимкнений. Це аварійна стабілізація, щоб прибрати flapping.</div>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="Скасувати" v-close-popup />
        <q-btn color="negative" unelevated label="Зафіксувати BITE" :loading="holdBiteBusy" @click="holdBitePrimary" />
      </q-card-actions>
    </q-card>
  </q-dialog>

  <q-dialog v-model="resumeFailoverDialog">
    <q-card class="repair-dialog">
      <q-card-section>
        <div class="text-subtitle1">Увімкнути sticky авто-перемикання?</div>
        <div class="text-body2">LMT стане primary, але після перемикання на BITE LMT більше не перевіряється. Лише коли активний BITE впаде, primary безумовно перейде на LMT. Автоматичного повернення на LMT немає.</div>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="Скасувати" v-close-popup />
        <q-btn color="positive" unelevated label="Увімкнути" :loading="resumeFailoverBusy" @click="resumeAutoFailover" />
      </q-card-actions>
    </q-card>
  </q-dialog>

  <q-dialog v-model="forceNextDialog">
    <q-card class="repair-dialog">
      <q-card-section>
        <div class="text-subtitle1">Перемкнути на наступний WAN?</div>
        <div class="text-body2">Поточний primary буде змінено на інший WAN негайно, без перевірки його доступності. Sticky scheduler після цього контролюватиме новий primary.</div>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="Скасувати" v-close-popup />
        <q-btn color="warning" unelevated label="Перемкнути" :loading="forceNextBusy" @click="forceNextWan" />
      </q-card-actions>
    </q-card>
  </q-dialog>

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
import { useQuasar } from "quasar";
import { AgentDialog, AuditDialog } from "@7n/tauri-components/components";
import { useUpdater } from "@7n/tauri-components/vue";
import { useAcpAgent } from "./composables/use-acp-agent.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $q = useQuasar();

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

const agent = useAcpAgent();
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
let diagnosticTimer = null;
let diagnosticBusy = false;
const DIAGNOSTIC_INTERVAL_MS = 5000;
const DIAGNOSTIC_HISTORY_KEY = "mymikrotik.diagnostic-history.v1";
const DIAGNOSTIC_ACTIVE_KEY = "mymikrotik.diagnostic-active.v1";

const diagnosticRunning = ref(localStorage.getItem(DIAGNOSTIC_ACTIVE_KEY) === "true");
const diagnosticSnapshot = ref(null);
const diagnosticError = ref("");
const diagnosticHistory = ref(loadDiagnosticHistory());
const diagnosticReportEl = ref(null);
const diagnosticReport = ref(localStorage.getItem("mymikrotik.diagnostic-report.v1") || "");
const reportBusy = ref(false);
const repairDialog = ref(false);
const repairBusy = ref(false);
const holdBiteDialog = ref(false);
const holdBiteBusy = ref(false);
const resumeFailoverDialog = ref(false);
const resumeFailoverBusy = ref(false);
const forceNextDialog = ref(false);
const forceNextBusy = ref(false);

function loadDiagnosticHistory() {
  try {
    const value = JSON.parse(localStorage.getItem(DIAGNOSTIC_HISTORY_KEY) || "[]");
    return Array.isArray(value) ? value.slice(0, 80) : [];
  } catch {
    return [];
  }
}

const diagnosticLevel = computed(() => {
  if (!diagnosticSnapshot.value) return "unknown";
  return diagnosticSnapshot.value.api_reachable ? "healthy" : "failed";
});

const diagnosticStatus = computed(() => {
  if (diagnosticRunning.value) return "Очікування активне: перевірка RouterOS кожні 5 секунд";
  return "Режим очікування зупинений";
});

const diagnosticControllerLabel = computed(() => {
  const snapshot = diagnosticSnapshot.value;
  if (!snapshot?.api_reachable) return "—";
  return snapshot.controller_state ? `Primary: ${snapshot.controller_state.toUpperCase()}` : "State невідомий";
});

const diagnosticSchedulerLabel = computed(() => {
  const snapshot = diagnosticSnapshot.value;
  if (!snapshot?.api_reachable) return "Немає з'єднання з RouterOS";
  const enabled = snapshot.scheduler_enabled === "true" ? "scheduler активний" : "scheduler неактивний";
  return `${enabled}, запусків: ${snapshot.scheduler_runs || "?"}`;
});

const diagnosticLastChecked = computed(() => {
  const checkedAt = diagnosticSnapshot.value?.checked_at;
  return checkedAt ? fmtClock(new Date(checkedAt)) : "—";
});

const diagnosticLatencyLabel = computed(() => {
  const latency = diagnosticSnapshot.value?.latency_ms;
  return latency == null ? "—" : `API ${latency} ms`;
});

function addDiagnosticEvent(level, label, detail) {
  const entry = { id: `${Date.now()}-${Math.random()}`, time: nowIsoLabel(), level, label, detail };
  diagnosticHistory.value = [entry, ...diagnosticHistory.value].slice(0, 80);
  localStorage.setItem(DIAGNOSTIC_HISTORY_KEY, JSON.stringify(diagnosticHistory.value));
}

function nowIsoLabel() {
  const now = new Date();
  return `${now.toLocaleDateString("sv-SE")} ${fmtClock(now)}`;
}

function snapshotKey(snapshot) {
  if (!snapshot.api_reachable) return `failed:${snapshot.error}`;
  return `ok:${snapshot.controller_state}:${snapshot.scheduler_enabled}:${snapshot.script_invalid}`;
}

async function pollDiagnostic() {
  if (diagnosticBusy) return;
  diagnosticBusy = true;
  const previous = diagnosticSnapshot.value;
  try {
    const snapshot = JSON.parse(await invoke("read_router_diagnostic"));
    diagnosticSnapshot.value = snapshot;
    diagnosticError.value = snapshot.error || "";
    if (snapshotKey(previous || {}) !== snapshotKey(snapshot)) {
      if (snapshot.api_reachable) {
        addDiagnosticEvent("up", "RouterOS доступний", `${snapshot.identity || snapshot.endpoint}; primary ${snapshot.controller_state || "?"}`);
        if (previous && !previous.api_reachable) {
          $q.notify({ type: "positive", message: "RouterOS API відновився", position: "top" });
          loadRouterLog();
          await captureDiagnosticReport(true);
        }
      } else {
        addDiagnosticEvent("down", "RouterOS недоступний", `${snapshot.endpoint}: ${snapshot.error || "невідома помилка"}`);
        $q.notify({ type: "negative", message: "Втрачено доступ до RouterOS API", position: "top", timeout: 7000 });
      }
    }
  } catch (error) {
    const snapshot = { api_reachable: false, endpoint: "RouterOS API", error: String(error), checked_at: new Date().toISOString() };
    diagnosticSnapshot.value = snapshot;
    diagnosticError.value = snapshot.error;
    if (!previous || previous.api_reachable) {
      addDiagnosticEvent("down", "Помилка діагностики", snapshot.error);
      $q.notify({ type: "negative", message: "Не вдалося виконати діагностику RouterOS", position: "top", timeout: 7000 });
    }
  } finally {
    diagnosticBusy = false;
  }
}

function reportLines(snapshot, routerData, routerError) {
  const lines = [
    "# MyMikroTik diagnostic report",
    `Generated: ${nowIsoLabel()}`,
    "",
    "## RouterOS API",
    `- Endpoint: ${snapshot.endpoint || "?"}`,
    `- Reachable: ${snapshot.api_reachable ? "yes" : "no"}`,
    `- Latency: ${snapshot.latency_ms == null ? "?" : `${snapshot.latency_ms} ms`}`,
    `- Identity: ${snapshot.identity || "?"}`,
    `- Error: ${snapshot.error || "none"}`,
    "",
    "## Controller",
    `- State: ${snapshot.controller_state || "unknown"}`,
    `- Scheduler: ${snapshot.scheduler_enabled || "unknown"}; runs: ${snapshot.scheduler_runs || "?"}`,
    `- Scheduler last started: ${snapshot.scheduler_last_started || "?"}`,
    `- Scheduler on-event: ${snapshot.scheduler_on_event || "?"}`,
    `- Scheduler policy: ${snapshot.scheduler_policy || "?"}`,
    `- DUALWAN-health invalid: ${snapshot.script_invalid || "unknown"}`,
    `- DUALWAN-health runs: ${snapshot.script_runs || "?"}; last started: ${snapshot.script_last_started || "?"}`,
    `- dwActiveBad: ${snapshot.lmt_bad_cycles || "?"}; last decision: ${snapshot.last_decision || "?"}`,
    `- Active script jobs: ${(snapshot.script_jobs || []).join(", ") || "none"}`,
  ];
  if (routerError) {
    lines.push("", "## Router data", `- Unavailable: ${routerError}`);
  } else if (routerData) {
    lines.push("", "## DHCP leases");
    for (const lease of routerData.dhcp || []) {
      lines.push(`- ${lease.channel}: ${lease.status || "?"}; ${lease.address || "no address"}; gw ${lease.gateway || "?"}; routes ${lease.default_route_tables || "?"}`);
    }
    lines.push("", "## Default routes");
    for (const route of routerData.routes || []) {
      lines.push(`- ${route.channel}/${route.table}: distance ${route.distance || "?"}; active ${route.active || "false"}; gw ${route.gateway || "?"}`);
    }
    lines.push("", "## LMT probes");
    for (const probe of routerData.probes || []) {
      lines.push(`- ${probe.target}: ${probe.received || "0"}/3; loss ${probe.loss_percent || "?"}%; avg ${probe.avg_rtt || "?"}`);
    }
    const important = (routerData.raw_log || [])
      .filter((entry) => /DUALWAN|scheduler.*failed|syntax error|bad parameter|dhcp.*error/i.test(entry.message || ""))
      .slice(-30);
    lines.push("", "## Relevant RouterOS log");
    lines.push(...(important.length ? important.map((entry) => `- ${entry.time} [${entry.topics}] ${entry.message}`) : ["- No matching records"]));
  }
  lines.push("", "## Local diagnostic history");
  lines.push(...(diagnosticHistory.value.slice(0, 20).map((entry) => `- ${entry.time}: ${entry.label}; ${entry.detail}`) || ["- No local events"]));
  return lines.join("\n");
}

async function captureDiagnosticReport(quiet = false) {
  if (reportBusy.value) return;
  reportBusy.value = true;
  try {
    const snapshot = diagnosticSnapshot.value || JSON.parse(await invoke("read_router_diagnostic"));
    let routerData = null;
    let routerError = "";
    if (snapshot.api_reachable) {
      try {
        routerData = JSON.parse(await invoke("read_router_log"));
        if (routerData.error) routerError = routerData.error;
      } catch (error) {
        routerError = String(error);
      }
    } else {
      routerError = snapshot.error || "RouterOS API недоступний";
    }
    diagnosticReport.value = reportLines(snapshot, routerData, routerError);
    localStorage.setItem("mymikrotik.diagnostic-report.v1", diagnosticReport.value);
    if (!quiet) $q.notify({ type: "positive", message: "Діагностичний звіт збережено локально", position: "top" });
  } catch (error) {
    if (!quiet) $q.notify({ type: "negative", message: `Не вдалося зібрати звіт: ${error}`, position: "top" });
  } finally {
    reportBusy.value = false;
  }
}

async function copyDiagnosticReport() {
  if (!diagnosticReport.value) return;
  try {
    await navigator.clipboard.writeText(diagnosticReport.value);
  } catch {
    diagnosticReportEl.value?.focus();
    diagnosticReportEl.value?.select();
    document.execCommand("copy");
  }
  $q.notify({ type: "positive", message: "Звіт скопійовано в буфер", position: "top" });
}

async function repairFailoverPing() {
  repairBusy.value = true;
  try {
    const message = await invoke("repair_failover_ping");
    repairDialog.value = false;
    addDiagnosticEvent("up", "Scheduler виправлено", message);
    $q.notify({ type: "positive", message, position: "top", timeout: 7000 });
    await pollDiagnostic();
    setTimeout(() => {
      pollDiagnostic();
      captureDiagnosticReport(true);
      loadRouterLog();
    }, 16000);
  } catch (error) {
    $q.notify({ type: "negative", message: `Не вдалося виправити scheduler: ${error}`, position: "top", timeout: 9000 });
  } finally {
    repairBusy.value = false;
  }
}

async function holdBitePrimary() {
  holdBiteBusy.value = true;
  try {
    const message = await invoke("hold_bite_primary");
    holdBiteDialog.value = false;
    addDiagnosticEvent("down", "BITE зафіксовано primary", message);
    $q.notify({ type: "warning", message, position: "top", timeout: 9000 });
    await pollDiagnostic();
    await captureDiagnosticReport(true);
    await loadRouterLog();
  } catch (error) {
    $q.notify({ type: "negative", message: `Не вдалося зафіксувати BITE: ${error}`, position: "top", timeout: 9000 });
  } finally {
    holdBiteBusy.value = false;
  }
}

async function resumeAutoFailover() {
  resumeFailoverBusy.value = true;
  try {
    const message = await invoke("resume_auto_failover");
    resumeFailoverDialog.value = false;
    addDiagnosticEvent("up", "Sticky авто-перемикання увімкнено", message);
    $q.notify({ type: "positive", message, position: "top", timeout: 9000 });
    await pollDiagnostic();
    await captureDiagnosticReport(true);
    await loadRouterLog();
  } catch (error) {
    $q.notify({ type: "negative", message: `Не вдалося відновити авто-перемикання: ${error}`, position: "top", timeout: 9000 });
  } finally {
    resumeFailoverBusy.value = false;
  }
}

async function forceNextWan() {
  forceNextBusy.value = true;
  try {
    const message = await invoke("force_next_wan");
    forceNextDialog.value = false;
    addDiagnosticEvent("down", "Примусово перемкнено WAN", message);
    $q.notify({ type: "warning", message, position: "top", timeout: 9000 });
    await pollDiagnostic();
    await captureDiagnosticReport(true);
    await loadRouterLog();
  } catch (error) {
    $q.notify({ type: "negative", message: `Не вдалося перемкнути WAN: ${error}`, position: "top", timeout: 9000 });
  } finally {
    forceNextBusy.value = false;
  }
}

function startDiagnostic() {
  if (diagnosticTimer) clearInterval(diagnosticTimer);
  diagnosticRunning.value = true;
  localStorage.setItem(DIAGNOSTIC_ACTIVE_KEY, "true");
  pollDiagnostic();
  diagnosticTimer = setInterval(pollDiagnostic, DIAGNOSTIC_INTERVAL_MS);
}

function stopDiagnostic() {
  if (diagnosticTimer) clearInterval(diagnosticTimer);
  diagnosticTimer = null;
  diagnosticRunning.value = false;
  localStorage.setItem(DIAGNOSTIC_ACTIVE_KEY, "false");
}

function toggleDiagnostic() {
  if (diagnosticRunning.value) stopDiagnostic();
  else startDiagnostic();
}

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
  pollDiagnostic();
  if (diagnosticRunning.value) startDiagnostic();
});

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer);
  if (diagnosticTimer) clearInterval(diagnosticTimer);
});
</script>
