# mikrotik — dual-WAN quality failover + monitor app

Інструменти для MikroTik hAP ax² з двома WAN-каналами: LMT (WAN1, ether3) і
BITE (WAN2, ether1). Весь failover тепер живе виключно на роутері
(RouterOS `netwatch`) — жодних python-скриптів чи cron-задач на Mac.

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

- **RouterOS netwatch** — єдиний механізм failover: перевіряє лише LMT
  (`212.93.105.242`) кожні 25 с (12 пінгів, поріг втрат 55%). При втраті
  LMT RouterOS вимикає `LB-w1*` та одразу вмикає `LB-w2*`; після відновлення
  повертає LMT. BITE є сліпим резервом і не отримує health-check ping.
- **app/** — десктопний Tauri-застосунок (macOS), увесь бекенд на Rust
  (`app/src-tauri/src/routeros.rs` + `lib.rs`), без жодного python:
  - Read-only перегляд фактичного rx/tx-трафіку `ether3` (LMT) та `ether1`
    (BITE) кожні 15 с. Це не тест швидкості й не створює трафіку.
  - Панель "Журнал failover LMT" — читає netwatch-статус і flap-події напряму з
    системного логу роутера.
  - Панель "Агент" — локальний LLM-агент (omlx) лише з інструментами читання
    стану та логу; він не може змінювати конфігурацію RouterOS.

```
cd app
bun install
bun run tauri dev     # запуск у dev-режимі
bun run tauri build    # зібрати .app / .dmg
```
