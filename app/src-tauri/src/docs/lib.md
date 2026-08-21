---
type: Rust Module
title: lib.rs
resource: app/src-tauri/src/lib.rs
docgen:
  crc: e9a281ad
  model: openai-codex/gpt-5.5
  score: 100
  issues: judge:inaccurate:0.99
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

Файл надає read-only операції для читання швидкості WAN, якості WAN, діагностики маршрутизатора та журналу без запису у файлову систему чи базу даних. Операції працюють fail-safe: перехоплюють помилки, не викидають винятків назовні та за частини збоїв повертають порожнє значення замість помилки. Також запускає застосунок і виконує WAN speed test через `https://speed.cloudflare.com/__down?bytes=50000000`, щоб споживач міг отримувати актуальні виміри або безпечно обробляти відсутність даних.

## Поведінка

- `read_wan_speed_impl` читає поточні RX/TX швидкості WAN-каналів із RouterOS і повертає JSON-знімок із локальним часом.
- `read_wan_speed` відкриває Tauri-команді читання поточних швидкостей WAN через той самий JSON-результат.
- `run_wan_speed_test_impl` виконує послідовний download speed test для LMT і BITE через RouterOS fetch із `https://speed.cloudflare.com/__down?bytes=50000000`, повертає JSON із вимірами або зрозумілу помилку, якщо RouterOS не дозволяє fetch чи тест не передав дані.
- `run_wan_speed_test` відкриває Tauri-команді запуск WAN speed test і повертає той самий JSON-результат.
- `read_router_diagnostic_impl` збирає короткий діагностичний знімок доступності RouterOS, активного WAN, scheduler/script стану та поточних script jobs; за помилок конфігурації або підключення повертає fail-safe JSON із порожніми чи `unknown` полями.
- `read_router_diagnostic` відкриває Tauri-команді читання діагностики RouterOS.
- `read_wan_quality_impl` читає постійну історію якості обох WAN із файлів `wan-quality.*.txt` на диску RouterOS і повертає JSON зі зразками та кількістю знайдених history-файлів.
- `read_wan_quality` відкриває Tauri-команді читання історії якості WAN.
- `read_router_log_impl` збирає стан dual-WAN controller, DHCP, default routes, історію перемикань із дискових history-файлів і останні рядки RouterOS log; повертає JSON без запису у ФС чи БД.
- `read_router_log` відкриває Tauri-команді читання стану controller і журналів RouterOS.
- `run` запускає Tauri-застосунок, підключає потрібні plugins, стартує фоновий моніторинг WAN, додає версію до заголовка головного вікна та реєструє команди для UI.

## Публічний API

- read_wan_speed_impl — зчитує з RouterOS збережені результати вимірювання швидкості WAN, отримані через тестове завантаження з https://speed.cloudflare.com/__down?bytes=50000000.
- read_wan_speed — повертає поточну історію вимірювань швидкості WAN у форматі, придатному для зовнішнього читання.
- run_wan_speed_test_impl — запускає на роутері вимірювання швидкості WAN через контрольне завантаження з https://speed.cloudflare.com/__down?bytes=50000000 і фіксує результат.
- run_wan_speed_test — ініціює тест швидкості WAN та повертає його підсумок для користувача або автоматики.
- read_router_diagnostic_impl — збирає діагностичний стан роутера, щоб швидко оцінити доступність, маршрутизацію й роботу WAN.
- read_router_diagnostic — надає узагальнену діагностику роутера для перегляду без ручного виконання команд RouterOS.
- read_wan_quality_impl — читає постійну історію якості обох WAN із `wan-quality.*.txt` на диску RouterOS.
- read_wan_quality — повертає накопичену якість WAN-каналів для аналізу стабільності підключень.
- read_router_log_impl — читає журнал RouterOS, щоб побачити події роутера, помилки та зміни стану мережі.
- read_router_log — надає останні записи журналу роутера для діагностики інцидентів.
- run — виконує запитану дію з читання діагностики, логів, якості WAN або запуску тесту швидкості.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
- Перехоплює помилки і не пропускає винятків назовні (fail-safe).
- За певних помилок повертає порожнє значення (напр. `null`) замість винятку.
