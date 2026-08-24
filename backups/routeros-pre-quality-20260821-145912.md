# Backup перед quality-aware DUALWAN

Backup створено 2026-08-21 до зміни сценарію `DUALWAN-health` на RouterOS
7.23.3. На момент знімка активним був LMT, scheduler
`DUALWAN-health-every-5s` був увімкнений з інтервалом 5 секунд.

- Санітизований export: `routeros-pre-quality-20260821-145912.rsc`, 24946 bytes,
  SHA-256 `701eec5c3c84a8779fd5ee54b0342f016b8b2e367daa05c21360d9ebf8dd4d8a`.
- Локальний зашифрований binary backup:
  `backups/private/routeros-pre-quality-20260821-145912.backup`, 192474 bytes,
  SHA-256 `9e0b48719b2a3c3581e0c01c27bf8513cebe1206d651828a88b0642d3a93d305`.
- Копії обох файлів також залишені на диску RouterOS під іменем
  `routeros-pre-quality-20260821-145912`.

Binary backup зашифрований RouterOS credentials і виключений із Git. Text
export створений з `show-sensitive=no`; перевірка не знайшла відкритих
`password`, `passphrase`, `secret` або `private-key` значень.
