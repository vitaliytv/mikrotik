# mikrotik — dual-WAN quality failover + monitor app

Інструменти для MikroTik hAP ax² з двома WAN-каналами (PCC 50/50 load balancing
з авто-failover за якістю з'єднання, VOIP-пріоритезація для Zoom).

## Налаштування

Скрипти підключаються до RouterOS API (порт 8728). Дані для підключення
беруться з env-змінних або з файлу `~/.mikrotik.env` (поза репозиторієм,
не комітиться):

```
MIKROTIK_HOST=192.168.88.1
MIKROTIK_USER=admin
MIKROTIK_PASS=твій_пароль
```

## scripts/

- `wan_monitor.py` — вимірює якість обох WAN (RTT/loss), автоматично вимикає
  деградований канал і повертає його при відновленні. Запускається з cron
  кожні 3 хвилини:
  ```
  */3 * * * * /usr/bin/python3 ~/wan_monitor.py >> ~/wan_monitor.log 2>&1
  ```
- `wan_chart.py` — генерує HTML-графік (Chart.js) з `~/wan_log.csv`.
- `fix_mikrotik.py` — відновлює маршрути/netwatch після втрати конфігурації
  (запускати вручну, коли підключений напряму до роутера).

## app/

Десктопний Tauri-застосунок (macOS) — той самий графік, що й `wan_chart.py`,
але у вигляді нативного вікна з кнопками "Оновити" та "Виміряти зараз".

```
cd app
bun install
bun run tauri dev     # запуск у dev-режимі
bun run tauri build    # зібрати .app / .dmg
```

Застосунок читає `~/wan_log.csv` і запускає `~/wan_monitor.py` напряму —
тому файли `scripts/wan_monitor.py` мають бути встановлені в `~/` на Mac,
що керує роутером.
