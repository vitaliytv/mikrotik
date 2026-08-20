---
type: JS Module
title: transport-cli.js
resource: app/src/tool/transport-cli.js
docgen:
  crc: 21e282c2
  model: openai-codex/gpt-5.4-mini
  tier: cloud-min
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

`cliTransport` забезпечує read-only transport для виклику каталожного інструмента з одного спільного шляху для CLI/MCP-інтеграції. Він повертає результат виконання або помилку виклику, не змінюючи файлову систему чи базу даних.

## Поведінка

1. `cliTransport` підбирає доступний `wan_cli` у build-артефактах Tauri, щоб запустити catalog tool як окремий процес, коли прямий Tauri invoke недоступний.
2. Якщо передано вхідні дані й вони не порожні, `cliTransport` додає їх до запуску як JSON-рядок, щоб передати контекст виконання tool.
3. `cliTransport` запускає `wan_cli` з потрібною командою та чекає завершення процесу, бо цей шлях працює синхронно для Node MCP/CLI entrypoint.
4. Якщо процес завершується з помилкою, `cliTransport` повертає помилку з повідомленням від `stderr` або з кодом завершення, щоб збій був видимим для викликача.
5. Якщо запуск успішний, `cliTransport` повертає `stdout` як результат виконання tool.
6. `cliTransport` не змінює файлову систему чи базу даних.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
