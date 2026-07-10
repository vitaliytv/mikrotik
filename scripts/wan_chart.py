#!/usr/bin/env python3
# Побудувати HTML-графік з wan_log.csv і відкрити в браузері.
# Запустити: python3 ~/wan_chart.py
import csv, os, json, subprocess
from datetime import datetime

CSV_PATH = os.path.expanduser("~/wan_log.csv")
HTML_PATH = os.path.expanduser("~/wan_chart.html")

if not os.path.exists(CSV_PATH):
    print(f"Файл не знайдено: {CSV_PATH}"); raise SystemExit(1)

labels     = []
zte_avg    = []; zte_loss    = []; zte_active    = []
soyea_avg  = []; soyea_loss  = []; soyea_active  = []

with open(CSV_PATH) as f:
    for row in csv.DictReader(f):
        try:
            ts = datetime.fromisoformat(row['timestamp'])
            labels.append(ts.strftime("%d.%m %H:%M"))
            zte_avg.append(float(row['zte_avg_ms'])    if row.get('zte_avg_ms')    else None)
            zte_loss.append(float(row['zte_loss_pct']) if row.get('zte_loss_pct')  else 100)
            soyea_avg.append(float(row['soyea_avg_ms'])    if row.get('soyea_avg_ms')    else None)
            soyea_loss.append(float(row['soyea_loss_pct']) if row.get('soyea_loss_pct')  else 100)
            # zte_active/soyea_active columns added in v2 — default to 1 if absent
            zte_active.append(int(row.get('zte_active', 1)   or 1))
            soyea_active.append(int(row.get('soyea_active', 1) or 1))
        except: pass

n = len(labels)
print(f"Точок даних: {n}  ({labels[0] if labels else '?'} → {labels[-1] if labels else '?'})")

# Build disabled-period annotations for Chart.js
def disabled_annotations(active_list, color):
    anns = {}
    i = 0
    while i < len(active_list):
        if active_list[i] == 0:
            start = i
            while i < len(active_list) and active_list[i] == 0:
                i += 1
            end = i - 1
            key = f"box_{color}_{start}"
            anns[key] = {
                "type": "box",
                "xMin": start, "xMax": end,
                "backgroundColor": color,
                "borderWidth": 0,
                "label": {"display": False}
            }
        else:
            i += 1
    return anns

ann_zte   = disabled_annotations(zte_active,   "rgba(239,68,68,0.18)")
ann_soyea = disabled_annotations(soyea_active, "rgba(251,146,60,0.18)")
all_ann   = {**ann_zte, **ann_soyea}

html = f"""<!DOCTYPE html>
<html lang="uk">
<head>
<meta charset="UTF-8">
<title>WAN Monitor — LMT vs BITE</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/chartjs-plugin-annotation@3/dist/chartjs-plugin-annotation.min.js"></script>
<style>
  body {{ background:#111; color:#eee; font-family:system-ui; margin:20px }}
  h2   {{ color:#7dd3fc; margin-bottom:4px }}
  .sub {{ color:#888; font-size:13px; margin-bottom:20px }}
  .charts {{ display:grid; gap:24px }}
  .box  {{ background:#1e1e2e; border-radius:12px; padding:16px }}
  canvas {{ max-height:280px }}
  .legend {{ display:flex; gap:20px; font-size:12px; color:#aaa; margin-top:8px }}
  .leg {{ display:flex; align-items:center; gap:6px }}
  .sq  {{ width:14px; height:14px; border-radius:2px }}
</style>
</head>
<body>
<h2>WAN якість: LMT vs BITE ({n} вимірювань)</h2>
<div class="sub">Червона зона = LMT вимкнено авто-failover &nbsp;|&nbsp; Помаранчева зона = BITE вимкнено</div>
<div class="charts">
  <div class="box"><canvas id="rtt"></canvas></div>
  <div class="box"><canvas id="loss"></canvas></div>
</div>
<script>
Chart.register(window['chartjs-plugin-annotation']);
const L = {json.dumps(labels)};
const ANN = {json.dumps(all_ann)};

const baseCfg = (title, datasets, yLabel, extraAnn) => ({{
  type: 'line',
  data: {{ labels: L, datasets }},
  options: {{
    animation: false,
    plugins: {{
      title:  {{ display: true, text: title, color: '#7dd3fc', font: {{ size: 14 }} }},
      legend: {{ labels: {{ color: '#ccc' }} }},
      annotation: {{ annotations: {{ ...ANN, ...extraAnn }} }}
    }},
    scales: {{
      x: {{ ticks: {{ color: '#888', maxTicksLimit: 24, maxRotation: 0 }}, grid: {{ color: '#2a2a3e' }} }},
      y: {{ ticks: {{ color: '#ccc' }}, grid: {{ color: '#2a2a3e' }},
            title: {{ display: true, text: yLabel, color: '#aaa' }} }}
    }},
    elements: {{ point: {{ radius: L.length > 100 ? 0 : 2 }} }}
  }}
}});

new Chart(document.getElementById('rtt'), baseCfg(
  'Затримка (ping) мс — менше краще',
  [
    {{ label: 'LMT',   data: {json.dumps(zte_avg)},
       borderColor: '#60a5fa', backgroundColor: '#60a5fa22', tension: 0.3, spanGaps: true }},
    {{ label: 'BITE', data: {json.dumps(soyea_avg)},
       borderColor: '#34d399', backgroundColor: '#34d39922', tension: 0.3, spanGaps: true }},
    {{ label: 'avg-поріг вимкнення (150мс)', data: Array(L.length).fill(150),
       borderColor: '#f8717160', borderDash: [6,4], borderWidth: 1,
       pointRadius: 0, fill: false }},
    {{ label: 'spike-поріг вимкнення (160мс)', data: Array(L.length).fill(160),
       borderColor: '#fb923c60', borderDash: [4,3], borderWidth: 1,
       pointRadius: 0, fill: false }},
    {{ label: 'avg-поріг відновлення (100мс)', data: Array(L.length).fill(100),
       borderColor: '#34d39950', borderDash: [3,4], borderWidth: 1,
       pointRadius: 0, fill: false }}
  ], 'мс', {{}}
));

new Chart(document.getElementById('loss'), baseCfg(
  'Втрати пакетів % — менше краще',
  [
    {{ label: 'LMT loss%',   data: {json.dumps(zte_loss)},
       borderColor: '#f87171', backgroundColor: '#f8717122', tension: 0.3, spanGaps: true }},
    {{ label: 'BITE loss%', data: {json.dumps(soyea_loss)},
       borderColor: '#fb923c', backgroundColor: '#fb923c22', tension: 0.3, spanGaps: true }}
  ], '%', {{}}
));
</script>
</body>
</html>"""

with open(HTML_PATH, 'w') as f: f.write(html)
print(f"Графік: {HTML_PATH}")
subprocess.run(["open", HTML_PATH])
