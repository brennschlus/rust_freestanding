---
tags: [kernel/console]
---
# Framebuffer-консоль

`kernel/src/console.rs` — свой текстовый вывод прямо в framebuffer от [[Bootloader и старт ядра|бутлоадера]]:

- шрифт **font8x8**, SCALE=2 → сетка **80×45** знакомест;
- `print!` / `println!`, скроллинг, backspace, `clear`;
- адаптер `log::Log` — обычные `log::info!` идут туда же;
- паника печатается на экран (важно: QEMU headless — экран это всё, что есть).

Родной `LockedLogger` и `background_paint` из ранних глав удалены вместе с зависимостью `bootloader-x86_64-common` — консоль полностью своя.

Поверх консоли живёт [[Шелл]].
