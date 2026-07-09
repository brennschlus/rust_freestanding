---
tags: [officium/impl]
---
# Воркспейс officium

`officium/` — **свой** cargo-workspace (не часть корневого); ядро берёт крейты по path-dependency с `default-features = false` (`no_std + alloc`, фича `std` только для hosted-тестов).

| крейт | что в нём |
|---|---|
| `officium-core` | [[Core IR]], [[CEK-машина и топливо|машина]], [[Билтины]], [[Dissonance]], `Env`/`Program`/`Versus` ([[Фуга как Reader]]), [[Тайпчекер — рифма это унификация|check.rs]], трейт `Platform` |
| `officium-parse` | лексер + парсер [[Плоская форма|плоской]] и [[Литургическая форма|литургической]] форм |
| `officium-audio` | [[Таблица степеней|degree_op.rs]], [[Рендер|render.rs]], [[Транскрипция|transcribe.rs]], трейт `Organ`, `pitch_hz` (через libm) |
| `officium-host` | [[Хост-раннер|dev-раннер]] + интеграционные тесты вех m3–m8 |

Скоры: `officium/scores/meteor.off` и `meteor_cantus.off` ([[Метеорный скор]]).

58 hosted-тестов (`cargo test` из `officium/`): законы монад как property-тесты («литургия должна быть законной»), golden-тесты машины, маска способностей, охраны чистоты, TC-свидетель (факториал через Fix+State), фуззинг парсера, вечная фуга (`per omnia saecula`) с топливом, раундтрипы транскрипции.

f64 в ядре работает **без** настройки FPU: таргет soft-float, математика через compiler builtins.
