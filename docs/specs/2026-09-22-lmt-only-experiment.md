# Семиденний LMT-only експеримент

**Дата:** 2026-09-22
**Статус:** активовано вручну 2026-09-22 о 09:15:40 (RouterOS time)
**Зв’язані документи:** `README.md`, `scripts/DUALWAN-health.rsc`, `scripts/WAN-quality-sync.rsc`

## 1. Проблема / Мета

Потрібно протягом семи діб користуватися лише LMT і перевірити, чи достатньо
цього каналу без резервного BITE. Dual-WAN controller та активні BITE probes не
мають створювати перемикання або регулярний тестовий трафік. Експеримент не
завершується автоматично: повернення dual-WAN виконується лише вручну.

Щоб результат можна було оцінити, на диску RouterOS залишаються тільки
monitor-only LMT quality probes та пасивні interface counters.

## 2. Ухвалені рішення

| # | Питання | Рішення |
| --- | --- | --- |
| А | Тривалість | Мінімум сім діб; автоматичного завершення немає. |
| Б | Маршрутизація | `client1` (BITE) вимикається, `client2` (LMT) залишається єдиним default route. |
| В | Controller | `DUALWAN-health-every-5s`, decision probes і combined minute logger вимикаються. |
| Г | Телеметрія | Залишаються п’ять 15-minute LMT probes, disk logging, hourly passive counters і source-address sync. |
| Д | DNS | RouterOS використовує тільки LMT modem resolver `192.168.0.1`, щоб не чекати timeout від вимкненого BITE. |
| Е | Відкат | Окремий ручний stop script спочатку відновлює BITE lease, а controller вмикає останнім. За відсутності BITE modem відкат fail-safe лишає LMT активним. |

## 3. Деталі реалізації

- `scripts/LMT-only-experiment-start.rsc` перевіряє унікальність усіх потрібних
  RouterOS objects і `bound`-стан LMT до першої зміни.
- Start script переводить DNS на LMT, вимикає controller і combined telemetry,
  вимикає decision/BITE Netwatch probes та `client1`, після чого пише
  `LMTONLY state=start` у `dualwan-history`.
- `scripts/WAN-quality-sync.rsc` пропускає disabled DHCP client. Це дозволяє
  scheduler підтримувати актуальну адресу LMT без помилок через вимкнений BITE.
- `scripts/LMT-only-experiment-stop.rsc` вмикає `client1`, до 30 секунд чекає
  `bound`, запускає source sync, відновлює probes/DNS/logging і лише потім
  dual-WAN controller. Якщо BITE не отримав lease, script завершується помилкою
  до ввімкнення controller, а LMT і LMT DNS залишаються активними.
- Фізично вимикати BITE modem необов’язково: disabled `client1` виключає його з
  маршрутизації. За бажанням modem можна знеструмити після перевірки start state.
- Для аналізу користувач фіксує точний час суб’єктивних погіршень. Після семи
  діб оцінюються hard outages, loss, RTT/jitter, TCP/DNS failures, CAKE queue
  drops і відповідність проблем користувацьким спостереженням.

## Відкриті питання

- Немає. Повернення dual-WAN виконується лише після окремої команди користувача.

## Операційний стан

- Перед активацією на RouterOS створено санітизований export
  `lmt-only-before-20260922-090747.rsc`.
- Ручний rollback перевірено end-to-end перед фінальною активацією.
- Після активації LMT має `main:1`, BITE DHCP client вимкнений, DNS налаштований
  тільки на `192.168.0.1`, а LMT ICMP/DNS telemetry продовжує записуватися у
  `wan-quality.0.txt`.
