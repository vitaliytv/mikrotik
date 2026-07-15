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
  failover. Він вимірює LMT через два незалежні прив'язані `/32` probe-маршрути:
  `212.93.105.242` та `1.1.1.1`. LMT залишається primary, поки хоча б один
  target відповідає щонайменше на 2 з 3 ping; після трьох циклів, де обидва
  недоступні, router без перевірки перемикається на BITE (blind fallback).
  Повернення до LMT потребує 30 с стабільних
  відповідей. Обидва DHCP default-маршрути існують постійно, тому під час
  перемикання немає стану без маршруту.
  Окремо scheduler лише логує quality warning після 60 с втрат на обох LMT
  targets; quality-події не змінюють маршрути.
- **IPv6:** RouterOS має DHCPv6 PD client лише на LMT (`ether3`) без default
  route. Станом на 2026-07-15 він очікує prefix від LMT/modem; доки prefix не
  отримано, LAN IPv6 та IPv6 через BITE не увімкнені.
- **backups/routeros-current.rsc** — актуальний санітизований текстовий
  export RouterOS; password/passphrase/secret значення замінені на
  `<redacted>`.
- **app/** — десктопний Tauri-застосунок (macOS), увесь бекенд на Rust
  (`app/src-tauri/src/routeros.rs` + `lib.rs`), без жодного python:
  - Read-only перегляд фактичного rx/tx-трафіку `ether3` (LMT) та `ether1`
    (BITE) кожні 15 с, згладженого ковзним середнім за 1 хвилину. Це не тест
    швидкості й не створює трафіку.
  - Панель scheduler — читає стан `DUALWAN-health`, DHCP route priorities,
    обидва LMT probes, quality-події та перемикання primary WAN напряму з
    RouterOS.
  - Панель "Агент" — локальний LLM-агент (omlx) лише з інструментами читання
    стану та логу; він не може змінювати конфігурацію RouterOS.

```
cd app
bun install
bun run tauri dev     # запуск у dev-режимі
bun run tauri build    # зібрати .app / .dmg
```
