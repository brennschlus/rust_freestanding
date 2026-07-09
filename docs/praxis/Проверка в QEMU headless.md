---
tags: [praxis, qemu]
---
# Проверка в QEMU headless

Запуск без окна: `qemu-system-x86_64 -display none -monitor unix:<короткий-путь>.sock,server,nowait` + команда `screendump` в мониторе, чтобы увидеть [[Framebuffer-консоль|консоль]]. Путь сокета должен быть **< 108 байт**.

## Образ
`target/debug/build/rust_freestanding-*/out/blog_os-bios.img`. Хэш-каталогов может быть несколько (смена зависимостей kernel меняет хэш — [[Bootloader и старт ядра]]); брать самый свежий:

```sh
find target -name 'blog_os-bios.img' -printf '%T@ %p\n' | sort -rn | head -1
```

Не `ls` — у пользователя алиас с иконками, ломает пути в скриптах.

## Клавиши
`sendkey` через монитор. Грабли — в [[Грабли sendkey]]. С флагом `-d int` QEMU грузится в разы дольше: ждать 20–30 с перед sendkey, иначе клавиши уходят в никуда (ложная тревога «клавиатура сломана», 2026-07-06).

Звук в headless: [[Ловушка wav-бэкенда]]. Завис — [[Диагностика зависшего QEMU]].
