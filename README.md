# mikrotik — dual-WAN scheduler failover + monitor app

Інструменти для MikroTik hAP ax² з двома WAN-каналами: LMT (WAN1, ether3) і
BITE (WAN2, ether1). Весь failover тепер живе виключно на роутері
(RouterOS scheduler) — жодних python-скриптів чи cron-задач на Mac.

## Налаштування

Застосунок підключається до RouterOS API (порт 8728). Дані для підключення
беруться з env-змінних або з файлу `~/.mikrotik.env` (поза репозиторієм,
не комітиться):

```
MIKROTIK_HOST=192.168.88.1
MIKROTIK_USER=admin
MIKROTIK_PASS=твій_пароль
```

## Архітектура

- **RouterOS scheduler** `DUALWAN-health-every-5s` — єдиний контролер
  failover. Він вимірює LMT (`212.93.105.242`) та BITE (`84.15.67.179`)
  через окремі прив'язані `/32` probe-маршрути. LMT залишається primary,
  поки відповідає щонайменше на 2 з 3 ping; після трьох невдалих циклів
  router змінює пріоритет DHCP default-маршрутів на користь BITE. Повернення до LMT
  потребує 30 с стабільних відповідей. Обидва DHCP default-маршрути існують
  постійно, тому під час перемикання немає стану без маршруту.
- **backups/routeros-current.rsc** — актуальний санітизований текстовий
  export RouterOS; password/passphrase/secret значення замінені на
  `<redacted>`.
- **app/** — десктопний Tauri-застосунок (macOS), увесь бекенд на Rust
  (`app/src-tauri/src/routeros.rs` + `lib.rs`), без жодного python:
  - Read-only перегляд фактичного rx/tx-трафіку `ether3` (LMT) та `ether1`
    (BITE) кожні 15 с, згладженого ковзним середнім за 1 хвилину. Це не тест
    швидкості й не створює трафіку.
  - Панель scheduler — читає стан `DUALWAN-health`, DHCP route priorities,
    обидва probe-виміри та перемикання primary WAN напряму з RouterOS.
  - Панель "Агент" — локальний LLM-агент (omlx) лише з інструментами читання
    стану та логу; він не може змінювати конфігурацію RouterOS.

```
cd app
bun install
bun run tauri dev     # запуск у dev-режимі
bun run tauri build    # зібрати .app / .dmg
```
