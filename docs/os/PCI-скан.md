---
tags: [kernel/sound, kernel/pci]
---
# PCI-скан

`kernel/src/pci.rs` — минимальный доступ к конфиг-пространству через порты **0xCF8/0xCFC**:

- поиск устройства по vendor/device id;
- чтение I/O BAR'ов и interrupt line;
- включение бит IO + bus-master в командном регистре.

Единственный клиент — [[AC97 драйвер]] (Intel 82801AA, `8086:2415`, в QEMU `-device AC97`). SeaBIOS роутит его на IRQ 11 — level-triggered, см. [[PIC и маски прерываний]].
