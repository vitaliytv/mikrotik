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

- **RouterOS scheduler** `DUALWAN-health-every-5s` — симетричний контролер
  failover. Кожні 5 с він тестує лише поточний WAN через його інтерфейс
  (`ether3` для LMT, `ether1` для BITE) до `212.93.105.242` та `1.1.1.1`.
  Канал нормальний, якщо хоча б один target відповідає щонайменше на 2 з 3
  ping. Після трьох поганих циклів поспіль контролер перемикається на інший
  WAN; головного каналу, резерву і автоматичного повернення немає. Обидва DHCP
  default-маршрути існують постійно, тому під час перемикання немає стану без
  маршруту.
- **IPv6:** не налаштований. IPv6 DHCPv6-клієнт LMT видалено 2026-07-15,
  оскільки modem не видавав адресу або delegated prefix.
- **backups/routeros-current.rsc** — актуальний санітизований текстовий
  export RouterOS; password/passphrase/secret значення замінені на
  `<redacted>`.
- **app/** — десктопний Tauri-застосунок (macOS), увесь бекенд на Rust
  (`app/src-tauri/src/routeros.rs` + `lib.rs`), без жодного python:
  - Read-only перегляд фактичного rx/tx-трафіку `ether3` (LMT) та `ether1`
    (BITE) кожні 15 с, згладженого ковзним середнім за 1 хвилину. Це не тест
    швидкості й не створює трафіку.
  - Панель scheduler — читає стан `DUALWAN-health`, DHCP route priorities,
    probes активного WAN та перемикання напряму з RouterOS.
  - Панель "Агент" — локальний LLM-агент (omlx) лише з інструментами читання
    стану та логу; він не може змінювати конфігурацію RouterOS.

```
cd app
bun install
bun run tauri dev     # запуск у dev-режимі
bun run tauri build    # зібрати .app / .dmg
```
