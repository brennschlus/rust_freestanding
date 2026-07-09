---
tags: [kernel/async, kernel/input]
---
# ScancodeStream

Мост между прерыванием и async-миром: обработчик [[Клавиатура i8042|IRQ 1]] пишет скан-код в статическую `ArrayQueue` и дёргает `AtomicWaker`; `ScancodeStream` реализует `Stream<Item = u8>` — `poll_next` читает очередь, а при пустой регистрирует waker.

Потребители по очереди владеют потоком: [[Шелл]] читает его в главном цикле и *передаёт* инструментам (`piano`/`organ`/`transcribe`) и читалке скоров `celebrare -`, потому что тем нужны сырые события.

Тот же паттерн (очередь + AtomicWaker) использует [[Async sleep]] и synth-задача [[AC97 драйвер|AC97]].
