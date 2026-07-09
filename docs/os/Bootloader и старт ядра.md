---
tags: [kernel/boot]
---
# Bootloader и старт ядра

Ядро грузится через **bootloader 0.11.15** (не старый VGA-текстовый путь из классического blog_os — сразу framebuffer). Корневой крейт `rust_freestanding` в build.rs собирает BIOS-образ; ядро — отдельный крейт `kernel/`.

`BOOTLOADER_CONFIG` в `kernel/src/main.rs`:
- `mappings.physical_memory = Some(Mapping::Dynamic)` — вся физическая память замаплена по offset'у, это фундамент [[Пейджинг|пейджинга]];
- `kernel_stack_size = 512 * 1024` — дефолтных 80 КиБ не хватило рекурсивному парсеру [[Плоская форма|Officium]]: double fault со стековым указателем на дне (см. [[Диагностика зависшего QEMU]]).

Порядок инициализации: [[GDT и TSS]] → [[IDT и прерывания]] → [[Пейджинг]] → [[Куча ядра]] → [[Framebuffer-консоль]] → [[PCI-скан]] + [[AC97 драйвер]] → [[Async executor]] с задачами [[Шелл|шелла]] и клавиатуры.

Статики в стиле проекта — `conquer_once::OnceCell` + `spinning_top` (не lazy_static).

⚠️ При смене зависимостей kernel меняется хэш каталога с образом — свежий образ искать через `find ... -printf '%T@ %p\n' | sort -rn`, старый грузится молча ([[Проверка в QEMU headless]]).
