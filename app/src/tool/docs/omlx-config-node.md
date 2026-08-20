---
type: JS Module
title: omlx-config-node.js
resource: app/src/tool/omlx-config-node.js
docgen:
  crc: b8177aed
  model: openai-codex/gpt-5.4-mini
  tier: cloud-min
  score: 100
  issues: judge:inaccurate:0.96
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

Файл формує OMLX-налаштування для Node MCP/CLI на основі `settings.json` і endpoint `http://127.0.0.1:8000/v1`, щоб `resolveOmlxConfig` повертала готовий набір значень для підключення до локального in-app agent. Якщо читання конфігурації або звернення до мережі не вдається, поведінка fail-safe: помилка перехоплюється, а назовні нічого не кидається.

## Поведінка

1. resolveOmlxConfig формує єдиний набір OMLX-параметрів для Node MCP/CLI, щоб він працював узгоджено з in-app agent.
2. Спершу бере пріоритетні значення з environment variables, якщо вони задані.
3. Якщо env-змінних немає, звіряється з `settings.json` у домашній теці користувача та бере звідти `baseUrl` і `apiKey`.
4. Для `baseUrl` використовує адресу за замовчуванням `http://127.0.0.1:8000/v1`, якщо ні env, ні `settings.json` не дали придатного значення.
5. Для `model` завжди підставляє стандартну модель, якщо її не перевизначено через env.
6. Для `apiKey` повертає значення з env або з `settings.json`; якщо ключа немає, залишає його порожнім для безпечної роботи.
7. Під час читання конфігурації поводиться fail-safe: помилки не виходять назовні, а результат все одно повертається.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
- Перехоплює помилки і не пропускає винятків назовні (fail-safe).
