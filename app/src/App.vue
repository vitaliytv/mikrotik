<template>
  <nav class="top-nav" aria-label="Розділи MyMikroTik">
    <div class="top-nav-brand">
      <strong>MyMikroTik</strong>
      <span :class="['api-status', diagnosticLevel]">{{ overviewStatus }}</span>
    </div>
    <div class="top-nav-tabs" role="tablist">
      <q-btn v-for="tab in navigationTabs" :key="tab.id" flat dense no-caps :class="{ active: activeTab === tab.id }" :label="tab.label" @click="activeTab = tab.id" />
    </div>
    <div class="agent-toolbar">
      <q-btn flat dense round icon="sym_o_smart_toy" title="Агент" @click="agentOpen = true" />
      <q-btn flat dense round icon="sym_o_history" title="Журнал запитів агента" @click="auditOpen = true" />
    </div>
  </nav>

  <section v-if="activeTab === 'overview'" class="page" aria-label="Огляд">
    <PageGuide title="Огляд" purpose="Швидко показує, чи доступний роутер і який WAN зараз обслуговує мережу." readings="LMT і BITE — рівноправні канали quality-aware controller; API latency — час відповіді RouterOS." actions="Кнопка оновлення повторює read-only перевірку; вона не змінює конфігурацію." when="Відкривайте першою, коли інтернет працює не так, як очікується." />
    <header>
      <div>
        <h1>Стан мережі</h1>
        <div class="controls"><span>{{ diagnosticStatus }}</span></div>
      </div>
      <div class="diagnostic-actions">
        <q-btn flat dense round icon="sym_o_refresh" title="Перевірити зараз" @click="pollDiagnostic" />
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
    <div class="box overview-cards">
      <div v-for="card in controllerCards" :key="card.channel" :class="['nw-card', card.status]">
        <div class="title">{{ card.title }}</div>
        <div class="detail">{{ card.detail1 }}</div>
        <div class="detail">{{ card.detail2 }}</div>
      </div>
    </div>
    <table class="events diagnostic-history">
      <thead>
        <tr><th>Час</th><th>Подія</th><th>Деталі</th></tr>
      </thead>
      <tbody>
        <tr v-for="entry in recentDiagnosticHistory" :key="entry.id" :class="entry.level">
          <td>{{ entry.time }}</td><td>{{ entry.label }}</td><td>{{ entry.detail }}</td>
        </tr>
        <tr v-if="!recentDiagnosticHistory.length"><td colspan="3">Подій ще немає.</td></tr>
      </tbody>
    </table>
  </section>

  <section v-if="activeTab === 'channels'" class="page" aria-label="Канали">
    <PageGuide title="Канали" purpose="Показує поточне використання LMT і BITE без створення тестового трафіку." readings="↓ — download, ↑ — upload; графік згладжений за одну хвилину. Нуль не означає несправність — можливо, зараз немає трафіку." actions="Дій на цій сторінці немає: дані оновлюються автоматично кожні 15 секунд." when="Використовуйте, щоб побачити, який канал реально навантажений." />
    <header><h1>Трафік на WAN-інтерфейсах</h1><div class="controls"><span>{{ speedStatus }}</span></div></header>
    <div class="charts"><div class="box"><canvas ref="speedCanvasEl"></canvas></div></div>
  </section>

  <section v-if="activeTab === 'quality'" class="page" aria-label="Якість каналів">
    <PageGuide title="Якість" purpose="Порівнює LMT і BITE незалежно від active WAN на рівнях ICMP, TCP, DNS та фізичного інтерфейсу." readings="RTT, jitter, stdev і loss описують ICMP; TCP показує connect time, DNS — успішність резолвінгу; counters фіксують drops і link-downs." actions="Оновити перечитує постійну історію без зміни маршрутів або active WAN." when="Використовуйте для пошуку нестабільного каналу та визначення рівня проблеми: маршрут, сервіс або інтерфейс." />
    <header>
      <div><h1>Якість WAN за розкладом</h1><div class="controls"><span>{{ qualityStatus }}</span></div></div>
      <q-btn flat dense round icon="sym_o_refresh" title="Оновити якість" :loading="qualityBusy" @click="loadWanQuality" />
    </header>
    <div class="box overview-cards">
      <div v-for="card in qualityCards" :key="card.wan" :class="['nw-card', card.status]">
        <div class="title">{{ card.title }}</div>
        <div class="detail">{{ card.detail1 }}</div>
        <div class="detail">{{ card.detail2 }}</div>
      </div>
    </div>
    <div class="charts quality-charts">
      <div class="box"><canvas ref="qualityLatencyCanvasEl"></canvas></div>
      <div class="box"><canvas ref="qualityLossCanvasEl"></canvas></div>
    </div>
    <div class="box">
      <h2>Decision metrics контролера</h2>
      <table class="events">
        <thead><tr><th>Час</th><th>WAN</th><th>Роль</th><th>Відповіді</th><th>Avg / Max RTT</th><th>Jitter</th><th>TCP</th><th>Streak hard / severe / soft</th></tr></thead>
        <tbody>
          <tr v-for="row in recentDecisionRows" :key="row.key" :class="row.statusClass">
            <td>{{ row.time }}</td><td>{{ row.wan }}</td><td>{{ row.role }}</td><td>{{ row.responses }}</td><td>{{ row.rtt }}</td><td>{{ row.jitter }}</td><td>{{ row.tcp }}</td><td>{{ row.streak }}</td>
          </tr>
          <tr v-if="!recentDecisionRows.length"><td colspan="8">Очікую перший хвилинний snapshot контролера.</td></tr>
        </tbody>
      </table>
    </div>
    <div class="box">
      <h2>TCP і DNS checks</h2>
      <table class="events">
        <thead><tr><th>Час</th><th>WAN</th><th>Тип</th><th>Target</th><th>Стан</th><th>Результат</th></tr></thead>
        <tbody>
          <tr v-for="row in recentServiceRows" :key="row.key" :class="row.statusClass">
            <td>{{ row.time }}</td><td>{{ row.wan }}</td><td>{{ row.kind }}</td><td>{{ row.target }}</td><td>{{ row.status }}</td><td>{{ row.result }}</td>
          </tr>
          <tr v-if="!recentServiceRows.length"><td colspan="6">Очікую перші TCP і DNS checks.</td></tr>
        </tbody>
      </table>
    </div>
    <div class="box">
      <h2>WAN interface counters</h2>
      <div class="overview-cards">
        <div v-for="card in interfaceQualityCards" :key="card.wan" :class="['nw-card', card.status]">
          <div class="title">{{ card.title }}</div>
          <div class="detail">{{ card.detail1 }}</div>
          <div class="detail">{{ card.detail2 }}</div>
        </div>
      </div>
    </div>
    <div class="box">
      <h2>ICMP history</h2>
      <table class="events">
        <thead><tr><th>Час</th><th>WAN</th><th>Target</th><th>Відповіді</th><th>Loss</th><th>Avg RTT</th><th>Max RTT</th><th>Jitter</th><th>Stdev</th></tr></thead>
        <tbody>
          <tr v-for="row in recentQualityRows" :key="row.key" :class="row.status">
            <td>{{ row.time }}</td><td>{{ row.wan }}</td><td>{{ row.target }}</td><td>{{ row.responses }}</td><td>{{ row.loss }}</td><td>{{ row.avg }}</td><td>{{ row.max }}</td><td>{{ row.jitter }}</td><td>{{ row.stdev }}</td>
          </tr>
          <tr v-if="!recentQualityRows.length"><td colspan="9">На диску RouterOS ще немає ICMP-вимірювань.</td></tr>
        </tbody>
      </table>
    </div>
  </section>

  <section v-if="activeTab === 'speed-test'" class="page speed-test" aria-label="Тест швидкості WAN">
    <PageGuide title="Тест швидкості" purpose="Порівнює практичну download-швидкість LMT і BITE з самого роутера." readings="Mbps — швидкість завантаження до Cloudflare; тривалість та обсяг пояснюють результат." actions="Тест завантажує по 50 MB через кожен канал, не змінює маршрути та видаляє тимчасові файли." when="Запускайте рідко: для порівняння каналів або перевірки скарги на повільний інтернет." />
    <header>
      <div>
        <h1>Тест швидкості WAN з роутера</h1>
        <div class="controls"><span>Завантажує 50 MB з Cloudflare окремо через LMT і BITE.</span></div>
      </div>
      <q-btn
        color="primary"
        icon="sym_o_speed"
        :loading="wanSpeedTestBusy"
        :disable="wanSpeedTestBusy"
        :label="wanSpeedTestBusy ? 'Тестування…' : 'Запустити тест'"
        @click="runWanSpeedTest"
      />
    </header>
    <div v-if="wanSpeedTestResult" class="speed-test-result box">
      <div v-for="measurement in wanSpeedTestResult.measurements" :key="measurement.channel" class="speed-test-measurement">
        <strong>{{ measurement.channel }}: {{ measurement.megabits_per_second.toFixed(1) }} Mbps</strong>
        <small>{{ formatMegabytes(measurement.downloaded_bytes) }} MB за {{ formatSeconds(measurement.duration_ms) }} с через {{ measurement.interface }}</small>
      </div>
      <small class="speed-test-time">Останній тест: {{ formatTestTime(wanSpeedTestResult.tested_at) }}</small>
    </div>
    <div v-if="wanSpeedTestError" class="diagnostic-error">{{ wanSpeedTestError }}</div>
  </section>

  <section v-if="activeTab === 'events'" class="page" aria-label="Перемикання">
    <PageGuide title="Перемикання" purpose="Показує на надійній часовій шкалі, коли змінювався active WAN і скільки часу працював кожен канал." readings="Timeline, тривалість і таблиця будуються з постійного `dualwan-history` на диску RouterOS; історія до виправлення source-routed probes не враховується." actions="Оновлення читає диск і актуальні RouterOS дані без зміни конфігурації. Діапазон перемикає відображення між 24 годинами та 7 днями." when="Відкривайте після перемикання каналу або коли потрібно звірити час роботи LMT і BITE." />
    <header>
      <div>
        <h1>Історія перемикань active WAN</h1>
        <div class="controls"><span>{{ routerStatus }}</span></div>
      </div>
      <div class="switch-actions">
        <q-btn v-for="option in eventRangeOptions" :key="option.hours" dense no-caps color="primary" :outline="eventRangeHours !== option.hours" :label="option.label" @click="setEventRange(option.hours)" />
        <q-btn flat dense round icon="sym_o_refresh" title="Оновити перемикання" @click="loadRouterLog" />
        <q-btn dense outline icon="sym_o_summarize" label="Зібрати звіт" :loading="reportBusy" @click="captureDiagnosticReport()" />
      </div>
    </header>
    <div class="box switch-summary">
      <div class="switch-summary-card lmt">
        <span>LMT</span>
        <strong>{{ switchTimelineSummary.lmtDuration }}</strong>
        <small>{{ switchTimelineSummary.lmtPercent }}</small>
      </div>
      <div class="switch-summary-card bite">
        <span>BITE</span>
        <strong>{{ switchTimelineSummary.biteDuration }}</strong>
        <small>{{ switchTimelineSummary.bitePercent }}</small>
      </div>
      <div class="switch-summary-card neutral">
        <span>Перемикання</span>
        <strong>{{ switchTimelineSummary.switchCount }}</strong>
        <small>{{ switchTimelineSummary.period }}</small>
      </div>
    </div>
    <p class="switch-history-note">Надійна історія починається {{ reliableHistoryStartLabel }} — після встановлення окремих source-routed probes для обох WAN.</p>
    <div class="charts">
      <div class="box"><canvas ref="wanTimelineCanvasEl"></canvas></div>
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
            <th>Активний WAN</th>
            <th>Причина</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="ev in recentEvents" :key="ev.key" :class="ev.state === 'bite' ? 'down' : 'up'">
            <td>{{ ev.time }}</td>
            <td>{{ ev.state === "bite" ? "BITE" : "LMT" }}</td>
            <td>{{ ev.reason || "—" }}</td>
          </tr>
          <tr v-if="!recentEvents.length"><td colspan="3">У вибраному періоді перемикань не було.</td></tr>
        </tbody>
      </table>
      </div>
    </div>
    <div v-if="diagnosticReport" class="diagnostic-report">
      <div class="diagnostic-report-toolbar"><span>Автономний звіт для чату</span><q-btn flat dense icon="sym_o_content_copy" label="Скопіювати" @click="copyDiagnosticReport" /></div>
      <textarea ref="diagnosticReportEl" readonly :value="diagnosticReport" aria-label="Автономний звіт діагностики"></textarea>
    </div>
  </section>

  <section v-if="activeTab === 'log'" class="page" aria-label="Лог">
    <PageGuide title="Лог" purpose="Дає повні RouterOS записи для глибокої діагностики." readings="Кожен рядок містить час, тему та текст RouterOS. Фільтр звужує список лише у вікні." actions="Оновити запитує останні записи з роутера; показати/сховати керує лише їх відображенням." when="Відкривайте, якщо `Огляд` або `Перемикання` не пояснили проблему." />
    <div class="box raw-log">
    <div class="raw-log-toolbar">
      <span>Сирий лог MikroTik (останні <span>{{ filteredRawLog.length }}</span> рядків)</span>
      <button @click="loadRouterLog">Оновити лог</button>
      <button @click="toggleRawLog">{{ rawLogVisible ? "Сховати лог MikroTik" : "Показати лог MikroTik" }}</button>
      <input type="text" v-model="rawLogFilter" placeholder="Фільтр по тексту…" autocomplete="off" />
    </div>
    <pre class="raw-log-view" v-show="rawLogVisible">{{ rawLogText }}</pre>
    </div>
  </section>

  <AgentDialog v-model="agentOpen" @ran="loadRouterLog" :agent="agent" prompt-hint="наприклад: чи є зараз проблеми зі швидкістю?" />
  <AuditDialog v-model="auditOpen" :agent="agent" />
</template>

<script setup>
import { computed, defineComponent, h, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useQuasar } from "quasar";
import { AgentDialog, AuditDialog } from "@7n/tauri-components/components";
import { useUpdater } from "@7n/tauri-components/vue";
import { useAcpAgent } from "./composables/use-acp-agent.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $q = useQuasar();

const PageGuide = defineComponent({
  name: "PageGuide",
  props: {
    title: { type: String, required: true },
    purpose: { type: String, required: true },
    readings: { type: String, required: true },
    actions: { type: String, required: true },
    when: { type: String, required: true },
  },
  setup(props) {
    const rows = [
      ["Навіщо", props.purpose],
      ["Показники", props.readings],
      ["Дії", props.actions],
      ["Коли", props.when],
    ];
    return () => h("aside", { class: "page-guide", "aria-label": `Довідка: ${props.title}` }, [
      h("strong", "Що тут"),
      h("dl", rows.map(([label, text]) => h("div", [h("dt", label), h("dd", text)]))),
    ]);
  },
});

useUpdater();

Chart.register(window["chartjs-plugin-annotation"]);

const LIVE_MAX_POINTS = 240; // 240 * 15с ≈ 1 година в пам'яті, поки застосунок відкритий
const TRAFFIC_AVERAGE_WINDOW = 4; // 4 * 15с = 1 хвилина

const speedCanvasEl = ref(null);
const wanTimelineCanvasEl = ref(null);
const qualityLatencyCanvasEl = ref(null);
const qualityLossCanvasEl = ref(null);
let wanTimelineChart, speedChart, qualityLatencyChart, qualityLossChart;

let liveSamples = []; // Passive interface counters only; history lives in memory while the viewer is open.
const speedStatus = ref("—");
const routerStatus = ref("—");
const controllerCards = ref([]);
const events = ref([]); // switch events: {ts, time, state, reason}
const rawLogCache = ref([]); // {time, topics, message}
const rawLogFilter = ref("");
const rawLogVisible = ref(false);
const qualitySamples = ref([]);
const qualityStatus = ref("Очікую історію з диска RouterOS...");
const qualityBusy = ref(false);

const agent = useAcpAgent();
const agentOpen = ref(false);
const auditOpen = ref(false);
const activeTab = ref("overview");
const navigationTabs = [
  { id: "overview", label: "Огляд" },
  { id: "channels", label: "Канали" },
  { id: "quality", label: "Якість" },
  { id: "speed-test", label: "Тест швидкості" },
  { id: "events", label: "Перемикання" },
  { id: "log", label: "Лог" },
];

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

const recentDiagnosticHistory = computed(() => diagnosticHistory.value.slice(0, 5));
const RELIABLE_SWITCH_HISTORY_START = new Date(2026, 7, 24, 16, 10, 27);
const RELIABLE_SWITCH_INITIAL_STATE = "bite";
const eventRangeOptions = [
  { hours: 24, label: "24 години" },
  { hours: 168, label: "7 днів" },
];
const eventRangeHours = ref(24);
const timelineNow = ref(new Date());
const currentWanState = ref("");
const reliableHistoryStartLabel = RELIABLE_SWITCH_HISTORY_START.toLocaleString("sv-SE");
const eventRangeStart = computed(() => {
  const selectedStart = new Date(timelineNow.value.getTime() - eventRangeHours.value * 60 * 60 * 1000);
  return selectedStart > RELIABLE_SWITCH_HISTORY_START ? selectedStart : RELIABLE_SWITCH_HISTORY_START;
});
const filteredEvents = computed(() => timelinePoints()
  .filter((point) => point.source === "disk")
  .map((point) => ({ ...point, key: `${point.time}-${point.state}-${point.reason || ""}` })));
const recentEvents = computed(() => filteredEvents.value.slice(-300).reverse());
const qualityCards = computed(() => [
  qualityCard("lmt", "LMT (WAN1)"),
  qualityCard("bite", "BITE (WAN2)"),
]);
const recentQualityRows = computed(() => qualitySamples.value.filter((sample) => sample.kind === "icmp").slice(-80).reverse().map((sample) => ({
  key: `${sample.time}-${sample.wan}-${sample.target}`,
  time: sample.time,
  wan: sample.wan.toUpperCase(),
  target: sample.target,
  responses: `${sample.received}/${sample.sent}`,
  loss: `${formatQualityNumber(sample.loss_percent)}%`,
  avg: formatQualityMilliseconds(sample.avg_rtt_ms),
  max: formatQualityMilliseconds(sample.max_rtt_ms),
  jitter: formatQualityMilliseconds(sample.jitter_ms),
  stdev: formatQualityMilliseconds(sample.stdev_rtt_ms),
  status: sample.loss_percent > 0 ? "down" : "up",
})));
const recentServiceRows = computed(() => qualitySamples.value
  .filter((sample) => ["tcp", "dns"].includes(sample.kind))
  .slice(-40)
  .reverse()
  .map((sample) => ({
    key: `${sample.time}-${sample.wan}-${sample.kind}-${sample.target}-${sample.dns_server || ""}`,
    time: sample.time,
    wan: sample.wan.toUpperCase(),
    kind: sample.kind.toUpperCase(),
    target: sample.kind === "dns" ? `${sample.target} @ ${sample.dns_server}` : sample.target,
    status: sample.status?.toUpperCase() || "—",
    statusClass: sample.status === "up" ? "up" : "down",
    result: sample.kind === "tcp"
      ? formatQualityMilliseconds(sample.tcp_connect_ms)
      : sample.dns_answer || "немає відповіді",
  })));
const recentDecisionRows = computed(() => qualitySamples.value
  .filter((sample) => sample.kind === "decision")
  .slice(-40)
  .reverse()
  .map((sample) => {
    const degraded = sample.loss_percent > 0
      || sample.status !== "up"
      || Number(sample.avg_rtt_ms) > 80
      || Number(sample.max_rtt_ms) > 180
      || Number(sample.jitter_ms) > 120
      || Number(sample.tcp_connect_ms) > 300;
    return {
      key: `${sample.time}-${sample.wan}-decision`,
      time: sample.time,
      wan: sample.wan.toUpperCase(),
      role: sample.active ? "ACTIVE" : "candidate",
      responses: `${sample.received}/${sample.sent} · loss ${formatQualityNumber(sample.loss_percent)}%`,
      rtt: `${formatQualityMilliseconds(sample.avg_rtt_ms)} / ${formatQualityMilliseconds(sample.max_rtt_ms)}`,
      jitter: formatQualityMilliseconds(sample.jitter_ms),
      tcp: `${sample.status?.toUpperCase() || "—"} · ${formatQualityMilliseconds(sample.tcp_connect_ms)}`,
      streak: sample.active ? `${sample.hard_bad_cycles ?? 0} / ${sample.severe_bad_cycles ?? 0} / ${sample.quality_bad_cycles ?? 0}` : "—",
      statusClass: degraded ? "down" : "up",
    };
  }));
const interfaceQualityCards = computed(() => [
  interfaceQualityCard("lmt", "LMT · ether3"),
  interfaceQualityCard("bite", "BITE · ether1"),
]);

let refreshTimer = null;
let diagnosticTimer = null;
let reconnectTimer = null;
let diagnosticBusy = false;
let routerLogBusy = false;
const DIAGNOSTIC_INTERVAL_MS = 5000;
const ROUTER_RECONNECT_INTERVAL_MS = 5000;
const DIAGNOSTIC_HISTORY_KEY = "mymikrotik.diagnostic-history.v1";
const DIAGNOSTIC_ACTIVE_KEY = "mymikrotik.diagnostic-active.v1";

const diagnosticRunning = ref(localStorage.getItem(DIAGNOSTIC_ACTIVE_KEY) === "true");
const diagnosticSnapshot = ref(null);
const diagnosticError = ref("");
const reconnecting = ref(false);
const diagnosticHistory = ref(loadDiagnosticHistory());
const diagnosticReportEl = ref(null);
const diagnosticReport = ref(localStorage.getItem("mymikrotik.diagnostic-report.v1") || "");
const reportBusy = ref(false);
const wanSpeedTestBusy = ref(false);
const wanSpeedTestResult = ref(null);
const wanSpeedTestError = ref("");

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

const overviewStatus = computed(() => {
  const snapshot = diagnosticSnapshot.value;
  if (!snapshot) return "Перевіряю RouterOS";
  if (!snapshot.api_reachable) return "RouterOS недоступний";
  const active = snapshot.controller_state?.toUpperCase() || "невідомий WAN";
  return `RouterOS доступний · ${active}`;
});

const diagnosticStatus = computed(() => {
  if (diagnosticRunning.value) return "Очікування активне: перевірка RouterOS кожні 5 секунд";
  if (reconnecting.value) return "RouterOS недоступний: повторне підключення кожні 5 секунд";
  return "Режим очікування зупинений";
});

const diagnosticControllerLabel = computed(() => {
  const snapshot = diagnosticSnapshot.value;
  if (!snapshot?.api_reachable) return "—";
  return snapshot.controller_state ? `Активний: ${snapshot.controller_state.toUpperCase()}` : "Стан невідомий";
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

function clearReconnectTimer() {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  reconnectTimer = null;
  reconnecting.value = false;
}

function scheduleReconnect() {
  if (reconnectTimer || diagnosticRunning.value) return;
  reconnecting.value = true;
  reconnectTimer = setTimeout(async () => {
    reconnectTimer = null;
    await pollDiagnostic();
  }, ROUTER_RECONNECT_INTERVAL_MS);
}

async function pollDiagnostic() {
  if (diagnosticBusy) return;
  diagnosticBusy = true;
  const previous = diagnosticSnapshot.value;
  try {
    const snapshot = JSON.parse(await invoke("read_router_diagnostic"));
    diagnosticSnapshot.value = snapshot;
    diagnosticError.value = snapshot.error || "";
    if (snapshot.api_reachable) clearReconnectTimer();
    else scheduleReconnect();
    if (snapshotKey(previous || {}) !== snapshotKey(snapshot)) {
      if (snapshot.api_reachable) {
        addDiagnosticEvent("up", "RouterOS доступний", `${snapshot.identity || snapshot.endpoint}; активний ${snapshot.controller_state || "?"}`);
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
    scheduleReconnect();
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
    `- dwActiveBad: ${snapshot.active_bad_cycles || "?"}; last decision: ${snapshot.last_decision || "?"}`,
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

function formatMegabytes(bytes) {
  return (bytes / 1_000_000).toFixed(1);
}

function formatSeconds(milliseconds) {
  return (milliseconds / 1000).toFixed(2);
}

function formatTestTime(value) {
  return value ? new Date(value).toLocaleString("sv-SE") : "—";
}

async function runWanSpeedTest() {
  if (wanSpeedTestBusy.value) return;
  wanSpeedTestBusy.value = true;
  wanSpeedTestError.value = "";
  try {
    wanSpeedTestResult.value = JSON.parse(await invoke("run_wan_speed_test"));
    const summary = wanSpeedTestResult.value.measurements.map((measurement) => `${measurement.channel} ${measurement.megabits_per_second.toFixed(1)} Mbps`).join("; ");
    $q.notify({ type: "positive", message: `Тест швидкості завершено: ${summary}`, position: "top", timeout: 7000 });
  } catch (error) {
    wanSpeedTestError.value = String(error);
    $q.notify({ type: "negative", message: "Тест швидкості WAN не виконався", position: "top", timeout: 7000 });
  } finally {
    wanSpeedTestBusy.value = false;
  }
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
  if (!speedCanvasEl.value) return;

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

// ---------- постійна історія якості WAN із диска RouterOS ----------

function formatQualityNumber(value) {
  if (value == null || Number.isNaN(Number(value))) return "—";
  const number = Number(value);
  return number >= 100 ? number.toFixed(0) : number.toFixed(1);
}

function formatQualityMilliseconds(value) {
  const formatted = formatQualityNumber(value);
  return formatted === "—" ? formatted : `${formatted} ms`;
}

function formatQualityMegabits(value) {
  if (value == null || !Number.isFinite(Number(value))) return "—";
  return `${(Number(value) / 1_000_000).toFixed(1)} Mbps`;
}

function qualityCard(wan, title) {
  const decisionSamples = qualitySamples.value.filter((sample) => sample.wan === wan && sample.kind === "decision");
  const wanSamples = decisionSamples.length
    ? decisionSamples
    : qualitySamples.value.filter((sample) => sample.wan === wan && sample.kind === "icmp");
  const latestTime = wanSamples.at(-1)?.time;
  const latest = wanSamples.filter((sample) => sample.time === latestTime);
  if (!latest.length) {
    return { wan, title, status: "unknown", detail1: "Ще немає disk-вимірювань", detail2: "Очікую перший Netwatch cycle" };
  }
  const sum = (field) => latest.reduce((total, sample) => total + Number(sample[field] || 0), 0);
  const average = (field) => {
    const values = latest.map((sample) => sample[field]).filter((value) => value != null && Number.isFinite(Number(value)));
    return values.length ? values.reduce((total, value) => total + Number(value), 0) / values.length : null;
  };
  const sent = sum("sent");
  const received = sum("received");
  const loss = sent > 0 ? ((sent - received) / sent) * 100 : 100;
  const telemetry = latest[0];
  const aqm = telemetry?.aqm?.startsWith("cake-") ? "CAKE" : telemetry?.aqm || "AQM —";
  return {
    wan,
    title,
    status: loss > 0 ? "down" : "up",
    detail1: `RTT ${formatQualityMilliseconds(average("avg_rtt_ms"))} · jitter ${formatQualityMilliseconds(average("jitter_ms"))}`,
    detail2: `loss ${formatQualityNumber(loss)}% · ${received}/${sent}${telemetry?.active == null ? "" : telemetry.active ? " · ACTIVE" : " · candidate"} · ↑ ${formatQualityMegabits(telemetry?.tx_bps)}/${formatQualityMegabits(telemetry?.capacity_bps)} · ${aqm} · ${latestTime}`,
  };
}

function counterDelta(latest, previous, field) {
  if (!previous || latest[field] == null || previous[field] == null) return 0;
  return latest[field] >= previous[field] ? latest[field] - previous[field] : latest[field];
}

function interfaceQualityCard(wan, title) {
  const samples = qualitySamples.value.filter((sample) => sample.wan === wan && sample.kind === "interface");
  const latest = samples.at(-1);
  const previous = samples.at(-2);
  if (!latest) {
    return { wan, title, status: "unknown", detail1: "Ще немає interface counters", detail2: "Очікую hourly sample" };
  }
  const queueDelta = counterDelta(latest, previous, "tx_queue_drop");
  const linkDelta = counterDelta(latest, previous, "link_downs");
  const l2Errors = Number(latest.rx_fcs_error || 0) + Number(latest.rx_align_error || 0) + Number(latest.tx_collision || 0);
  const previousL2Errors = previous
    ? Number(previous.rx_fcs_error || 0) + Number(previous.rx_align_error || 0) + Number(previous.tx_collision || 0)
    : l2Errors;
  const l2Delta = l2Errors >= previousL2Errors ? l2Errors - previousL2Errors : l2Errors;
  const healthy = latest.running && queueDelta === 0 && linkDelta === 0 && l2Delta === 0;
  return {
    wan,
    title,
    status: healthy ? "up" : "down",
    detail1: `running ${latest.running ? "yes" : "no"} · queue drops ${latest.tx_queue_drop} (+${queueDelta})`,
    detail2: `link-downs ${latest.link_downs} (+${linkDelta}) · L2 errors ${l2Errors} (+${l2Delta}) · ${latest.time}`,
  };
}

function qualityBuckets() {
  const buckets = new Map();
  const decisions = qualitySamples.value.filter((entry) => entry.kind === "decision");
  const chartSamples = decisions.length ? decisions : qualitySamples.value.filter((entry) => entry.kind === "icmp");
  for (const sample of chartSamples) {
    const minute = sample.time.slice(0, 16);
    const key = `${minute}|${sample.wan}`;
    const bucket = buckets.get(key) || { time: minute, wan: sample.wan, avg: 0, avgCount: 0, jitter: 0, jitterCount: 0, stdev: 0, stdevCount: 0, loss: 0, lossCount: 0 };
    if (sample.avg_rtt_ms != null) {
      bucket.avg += sample.avg_rtt_ms;
      bucket.avgCount += 1;
    }
    if (sample.jitter_ms != null) {
      bucket.jitter += sample.jitter_ms;
      bucket.jitterCount += 1;
    }
    if (sample.stdev_rtt_ms != null) {
      bucket.stdev += sample.stdev_rtt_ms;
      bucket.stdevCount += 1;
    }
    if (sample.loss_percent != null) {
      bucket.loss += sample.loss_percent;
      bucket.lossCount += 1;
    }
    buckets.set(key, bucket);
  }
  return [...buckets.values()]
    .map((bucket) => ({
      time: bucket.time,
      wan: bucket.wan,
      avg: bucket.avgCount ? bucket.avg / bucket.avgCount : null,
      jitter: bucket.jitterCount ? bucket.jitter / bucket.jitterCount : null,
      stdev: bucket.stdevCount ? bucket.stdev / bucket.stdevCount : null,
      loss: bucket.lossCount ? bucket.loss / bucket.lossCount : null,
    }))
    .sort((left, right) => left.time.localeCompare(right.time));
}

function qualityDataset(labels, buckets, wan, field) {
  const values = new Map(buckets.filter((bucket) => bucket.wan === wan).map((bucket) => [bucket.time, bucket[field]]));
  return labels.map((label) => values.get(label) ?? null);
}

function renderQualityCharts() {
  if (!qualityLatencyCanvasEl.value || !qualityLossCanvasEl.value) return;
  const buckets = qualityBuckets();
  const labels = [...new Set(buckets.map((bucket) => bucket.time))];
  const latencyDatasets = [
    { label: "LMT avg RTT", data: qualityDataset(labels, buckets, "lmt", "avg"), borderColor: "#60a5fa" },
    { label: "LMT jitter", data: qualityDataset(labels, buckets, "lmt", "jitter"), borderColor: "#60a5fa", borderDash: [5, 4] },
    { label: "LMT stdev", data: qualityDataset(labels, buckets, "lmt", "stdev"), borderColor: "#93c5fd", borderDash: [2, 3] },
    { label: "BITE avg RTT", data: qualityDataset(labels, buckets, "bite", "avg"), borderColor: "#34d399" },
    { label: "BITE jitter", data: qualityDataset(labels, buckets, "bite", "jitter"), borderColor: "#34d399", borderDash: [5, 4] },
    { label: "BITE stdev", data: qualityDataset(labels, buckets, "bite", "stdev"), borderColor: "#6ee7b7", borderDash: [2, 3] },
  ].map((dataset) => ({ ...dataset, tension: 0.25, borderWidth: 1.5, spanGaps: false, pointRadius: labels.length > 100 ? 0 : 2 }));
  qualityLatencyChart?.destroy();
  qualityLatencyChart = new Chart(qualityLatencyCanvasEl.value, {
    type: "line",
    data: { labels, datasets: latencyDatasets },
    options: {
      animation: false,
      plugins: { title: { display: true, text: "RTT, jitter та stdev, ms", color: "#7dd3fc" }, legend: { labels: { color: "#ccc" } } },
      scales: {
        x: { ticks: { color: "#888", maxTicksLimit: 10, maxRotation: 0 }, grid: { color: "#2a2a3e" } },
        y: { beginAtZero: true, ticks: { color: "#ccc" }, grid: { color: "#2a2a3e" }, title: { display: true, text: "ms", color: "#aaa" } },
      },
    },
  });

  const lossDatasets = [
    { label: "LMT loss", data: qualityDataset(labels, buckets, "lmt", "loss"), borderColor: "#60a5fa", backgroundColor: "#60a5fa33" },
    { label: "BITE loss", data: qualityDataset(labels, buckets, "bite", "loss"), borderColor: "#34d399", backgroundColor: "#34d39933" },
  ].map((dataset) => ({ ...dataset, tension: 0.2, borderWidth: 1.5, fill: true, spanGaps: true, pointRadius: labels.length > 100 ? 0 : 2 }));
  qualityLossChart?.destroy();
  qualityLossChart = new Chart(qualityLossCanvasEl.value, {
    type: "line",
    data: { labels, datasets: lossDatasets },
    options: {
      animation: false,
      plugins: { title: { display: true, text: "Packet loss, %", color: "#7dd3fc" }, legend: { labels: { color: "#ccc" } } },
      scales: {
        x: { ticks: { color: "#888", maxTicksLimit: 10, maxRotation: 0 }, grid: { color: "#2a2a3e" } },
        y: { beginAtZero: true, suggestedMax: 5, ticks: { color: "#ccc" }, grid: { color: "#2a2a3e" }, title: { display: true, text: "%", color: "#aaa" } },
      },
    },
  });
}

async function loadWanQuality() {
  if (qualityBusy.value) return;
  qualityBusy.value = true;
  try {
    const result = JSON.parse(await invoke("read_wan_quality"));
    qualitySamples.value = result.samples || [];
    const count = qualitySamples.value.length;
    const period = count ? `${qualitySamples.value[0].time} — ${qualitySamples.value.at(-1).time}` : "очікую перший запис";
    qualityStatus.value = `${count} вимірювань · disk-файлів: ${result.history_file_count || 0} · ${period}`;
    renderQualityCharts();
  } catch (error) {
    qualityStatus.value = `Не вдалося прочитати WAN quality: ${error}`;
  } finally {
    qualityBusy.value = false;
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

function formatWanDuration(milliseconds) {
  const totalMinutes = Math.max(0, Math.round(milliseconds / 60000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (!hours) return `${minutes} хв`;
  return minutes ? `${hours} год ${minutes} хв` : `${hours} год`;
}

function timelineDateLabel(value, includeDate = false) {
  const date = new Date(value);
  const time = date.toLocaleTimeString("uk-UA", { hour: "2-digit", minute: "2-digit" });
  if (!includeDate) return time;
  return `${date.toLocaleDateString("uk-UA", { day: "2-digit", month: "2-digit" })} ${time}`;
}

function timelinePoints() {
  const rangeStart = eventRangeStart.value;
  const rangeEnd = timelineNow.value;
  const points = events.value
    .filter((event) => !Number.isNaN(event.ts?.getTime()))
    .map((event) => ({ ...event, source: "disk" }))
    .filter((point) => ["lmt", "bite"].includes(point.state))
    .filter((point) => point.ts >= RELIABLE_SWITCH_HISTORY_START && point.ts <= rangeEnd)
    .sort((a, b) => a.ts - b.ts);

  let stateAtStart = RELIABLE_SWITCH_INITIAL_STATE;
  for (const point of points.filter((point) => point.ts <= rangeStart)) stateAtStart = point.state;

  const result = [{ ts: rangeStart, state: stateAtStart, source: "range-start" }];
  for (const point of points.filter((point) => point.ts > rangeStart)) {
    const previous = result.at(-1);
    if (previous?.state === point.state) continue;
    result.push(point);
  }
  const current = currentWanState.value || result.at(-1)?.state;
  if (["lmt", "bite"].includes(current) && result.at(-1)?.state !== current) result.push({ ts: rangeEnd, state: current, source: "current" });
  else result.push({ ts: rangeEnd, state: result.at(-1)?.state || current, source: "current" });
  return result;
}

const switchTimelineSummary = computed(() => {
  const totals = { lmt: 0, bite: 0 };
  const points = timelinePoints();
  for (let index = 0; index < points.length - 1; index += 1) {
    const duration = points[index + 1].ts - points[index].ts;
    if (duration > 0 && Object.hasOwn(totals, points[index].state)) totals[points[index].state] += duration;
  }
  const total = totals.lmt + totals.bite;
  const percentage = (value) => total ? `${(value / total * 100).toFixed(1)}% періоду` : "0% періоду";
  return {
    lmtDuration: formatWanDuration(totals.lmt),
    biteDuration: formatWanDuration(totals.bite),
    lmtPercent: percentage(totals.lmt),
    bitePercent: percentage(totals.bite),
    switchCount: filteredEvents.value.length,
    period: `від ${eventRangeStart.value.toLocaleString("sv-SE")}`,
  };
});

function setEventRange(hours) {
  eventRangeHours.value = hours;
  timelineNow.value = new Date();
  renderWanTimeline();
}

function renderWanTimeline() {
  if (!wanTimelineCanvasEl.value) return;
  const points = timelinePoints();
  const chartPoints = points.map((point) => ({ x: point.ts.getTime(), y: point.state === "lmt" ? 1 : 0 }));
  const colors = points.map((point) => point.state === "lmt" ? "#60a5fa" : "#34d399");
  if (wanTimelineChart) wanTimelineChart.destroy();
  wanTimelineChart = new Chart(wanTimelineCanvasEl.value, {
    type: "line",
    data: {
      datasets: [{
        label: "Активний WAN",
        data: chartPoints,
        parsing: false,
        borderColor: "#94a3b8",
        backgroundColor: "rgba(96, 165, 250, .14)",
        stepped: "before",
        fill: true,
        pointRadius: points.length > 80 ? 0 : 3,
        pointBackgroundColor: colors,
        pointBorderColor: colors,
        borderWidth: 2,
      }],
    },
    options: {
      animation: false,
      plugins: {
        title: { display: true, text: "Періоди роботи активного WAN", color: "#7dd3fc", font: { size: 14 } },
        legend: { display: false },
        tooltip: {
          callbacks: {
            title: (items) => items[0] ? new Date(items[0].raw.x).toLocaleString("sv-SE") : "",
            label: (context) => `${context.raw.y === 1 ? "LMT" : "BITE"} активний`,
          },
        },
      },
      scales: {
        x: {
          type: "linear",
          min: eventRangeStart.value.getTime(),
          max: timelineNow.value.getTime(),
          ticks: {
            color: "#888",
            maxTicksLimit: 8,
            maxRotation: 0,
            callback: (value) => timelineDateLabel(value, eventRangeHours.value > 24),
          },
          grid: { color: "#2a2a3e" },
        },
        y: {
          min: -0.15,
          max: 1.15,
          ticks: { color: "#ccc", stepSize: 1, callback: (value) => value === 1 ? "LMT" : value === 0 ? "BITE" : "" },
          grid: { color: "#2a2a3e" },
        },
      },
    },
  });
}

function buildControllerCards(data) {
  const nameFor = { zte: "LMT (WAN1)", soyea: "BITE (WAN2)" };
  const controller = data.controller || {};
  const dhcp = Object.fromEntries((data.dhcp || []).map((row) => [row.channel, row]));
  const routes = data.routes || [];
  return ["zte", "soyea"].map((channel) => {
    const lease = dhcp[channel] || {};
    const mainRoute = routes.find((route) => route.channel === channel && route.table === "main") || {};
    const active = controller.state === (channel === "zte" ? "lmt" : "bite");
    return {
      channel,
      status: lease.status === "bound" ? "up" : "down",
      title: `${nameFor[channel]} — ${active ? "активний" : "неактивний"}`,
      detail1: active ? "активний WAN контролюється scheduler" : "канал постійно перевіряється окремими source-routed probes",
      detail2: `DHCP ${lease.status || "?"}, main distance ${mainRoute.distance || "?"}, gw ${lease.gateway || mainRoute.gateway || "?"}`,
    };
  });
}

async function loadRouterLog() {
  if (routerLogBusy) return;
  routerLogBusy = true;
  routerStatus.value = "Читаю лог роутера...";
  try {
    const raw = await invoke("read_router_log");
    const data = JSON.parse(raw);
    if (data.error) {
      routerStatus.value = `Помилка: ${data.error}`;
      return;
    }
    events.value = (data.switch_events || []).map((ev) => ({ ...ev, ts: parseLogTime(ev.time) }));
    rawLogCache.value = data.raw_log || [];
    controllerCards.value = buildControllerCards(data);
    timelineNow.value = new Date();
    currentWanState.value = (data.controller?.state || "").toLowerCase();
    renderWanTimeline();
    const controller = data.controller || {};
    const history = `історія: disk (${data.history_file_count || 0} файл.)`;
    routerStatus.value = `scheduler ${controller.scheduler_enabled === "true" ? "активний" : "недоступний"} | ${controller.interval || "?"} | активний: ${(controller.state || "?").toUpperCase()} | ${history} | запусків: ${controller.scheduler_runs || "?"}`;
  } catch (e) {
    routerStatus.value = `RouterOS недоступний. Повторне підключення: ${e}`;
  } finally {
    routerLogBusy = false;
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
  loadWanQuality();
  refreshTimer = setInterval(() => {
    loadRouterLog();
    loadWanQuality();
  }, 60000);
  pollDiagnostic();
  if (diagnosticRunning.value) startDiagnostic();
});

watch(activeTab, async (tab, previousTab) => {
  if (previousTab === "channels" && speedChart) {
    speedChart.destroy();
    speedChart = null;
  }
  if (previousTab === "events" && wanTimelineChart) {
    wanTimelineChart.destroy();
    wanTimelineChart = null;
  }
  if (previousTab === "quality") {
    qualityLatencyChart?.destroy();
    qualityLossChart?.destroy();
    qualityLatencyChart = null;
    qualityLossChart = null;
  }

  await nextTick();
  if (tab === "channels") renderSpeedChart();
  if (tab === "events") renderWanTimeline();
  if (tab === "quality") renderQualityCharts();
});

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer);
  if (diagnosticTimer) clearInterval(diagnosticTimer);
  if (reconnectTimer) clearTimeout(reconnectTimer);
  speedChart?.destroy();
  wanTimelineChart?.destroy();
  qualityLatencyChart?.destroy();
  qualityLossChart?.destroy();
});
</script>
