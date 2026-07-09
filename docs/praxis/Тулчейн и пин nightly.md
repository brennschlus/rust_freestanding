---
tags: [praxis, toolchain]
---
# Тулчейн и пин nightly

**`nightly-2026-02-01` запинен намеренно — не менять на плавающий `nightly`.**

Bootloader 0.11.15 требует окно версий:
- **новее** — nightly переименовал `x86-softfloat` rustc-abi из его JSON-таргетов;
- **старее** — нет `cargo -Zjson-target-spec`.

См. [[Bootloader и старт ядра]]. Таргет ядра — `x86_64-unknown-none` (soft-float): f64 для [[Воркспейс officium|Officium]] работает через compiler builtins, FPU настраивать не пришлось.

⚠️ `[profile.dev] opt-level = 2` обязателен — под TCG неоптимизированное ядро неюзабельно медленно.
