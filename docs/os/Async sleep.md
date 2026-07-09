---
tags: [kernel/async]
---
# Async sleep

`kernel/src/task/timer.rs`: `sleep_ms(ms)` — future, который засыпает на тиках [[Таймер PIT|PIT]] (гранулярность ~55 мс). Один спящий, `AtomicWaker`: обработчик таймера будит, poll сверяет дедлайн по `ticks()`.

Используется командами `beep`/`play` [[Шелл|шелла]] и проигрыванием нот в [[celebrare]] (`play()` шагает по событиям [[Рендер|рендера]] с шагом `TICK_US`).
