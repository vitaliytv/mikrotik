const { invoke } = window.__TAURI__.core;

Chart.register(window["chartjs-plugin-annotation"]);

const RTT_AVG_BAD = 150;
const RTT_MAX_BAD = 160;
const RTT_AVG_GOOD = 100;

let rttChart, lossChart;
const statusEl = () => document.querySelector("#status");

function parseCsv(text) {
  const lines = text.trim().split("\n");
  const rows = lines.slice(1).map((line) => line.split(","));
  const labels = [];
  const zteAvg = [];
  const zteLoss = [];
  const zteActive = [];
  const soyeaAvg = [];
  const soyeaLoss = [];
  const soyeaActive = [];
  for (const r of rows) {
    const [ts, za, , zl, sa, , sl, zact, sact] = r;
    if (!ts) continue;
    const d = new Date(ts);
    labels.push(
      `${String(d.getDate()).padStart(2, "0")}.${String(d.getMonth() + 1).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`,
    );
    zteAvg.push(za ? parseFloat(za) : null);
    zteLoss.push(zl ? parseFloat(zl) : 100);
    zteActive.push(zact !== undefined && zact !== "" ? parseInt(zact, 10) : 1);
    soyeaAvg.push(sa ? parseFloat(sa) : null);
    soyeaLoss.push(sl ? parseFloat(sl) : 100);
    soyeaActive.push(sact !== undefined && sact !== "" ? parseInt(sact, 10) : 1);
  }
  return { labels, zteAvg, zteLoss, zteActive, soyeaAvg, soyeaLoss, soyeaActive };
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

function baseCfg(title, datasets, yLabel, annotations, pointCount) {
  return {
    type: "line",
    data: { labels: window.__labels, datasets },
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

function render(data) {
  window.__labels = data.labels;
  const n = data.labels.length;

  const annZte = disabledAnnotations(data.zteActive, "rgba(239,68,68,0.18)");
  const annSoyea = disabledAnnotations(data.soyeaActive, "rgba(251,146,60,0.18)");
  const allAnn = { ...annZte, ...annSoyea };

  const rttCfg = baseCfg(
    "Затримка (ping) мс — менше краще",
    [
      { label: "ZTE", data: data.zteAvg, borderColor: "#60a5fa", backgroundColor: "#60a5fa22", tension: 0.3, spanGaps: true },
      { label: "Soyea", data: data.soyeaAvg, borderColor: "#34d399", backgroundColor: "#34d39922", tension: 0.3, spanGaps: true },
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
    [
      { label: "ZTE loss%", data: data.zteLoss, borderColor: "#f87171", backgroundColor: "#f8717122", tension: 0.3, spanGaps: true },
      { label: "Soyea loss%", data: data.soyeaLoss, borderColor: "#fb923c", backgroundColor: "#fb923c22", tension: 0.3, spanGaps: true },
    ],
    "%",
    {},
    n,
  );

  if (rttChart) rttChart.destroy();
  if (lossChart) lossChart.destroy();
  rttChart = new Chart(document.getElementById("rtt"), rttCfg);
  lossChart = new Chart(document.getElementById("loss"), lossCfg);
}

async function loadCsv() {
  try {
    const csv = await invoke("read_wan_csv");
    const data = parseCsv(csv);
    render(data);
    const last = data.labels.length - 1;
    if (last >= 0) {
      statusEl().textContent =
        `Точок: ${data.labels.length} | Останнє: ${data.labels[last]} ` +
        `ZTE=${data.zteAvg[last] ?? "?"}мс Soyea=${data.soyeaAvg[last] ?? "?"}мс`;
    } else {
      statusEl().textContent = "Немає даних";
    }
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

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#refresh").addEventListener("click", loadCsv);
  document.querySelector("#measure").addEventListener("click", measureNow);
  loadCsv();
  setInterval(loadCsv, 30000);
});
