---
tags: [officium/impl]
---
# CEK-машина и топливо

`officium-core/src/machine.rs` — вычислитель с **явным стеком** (трамплин): никакой нативной рекурсии — стеки ядра малы, а стретто может быть бесконечным.

- `Control::Eval(expr, scope) | Value(v)` + стек кадров `Frame` (AppArg/AppCall, IfK, BindK, ReturnK, RecipeH, PoneK, MitteK, PerageK, ResolveK, FixK, ModeRestore, BuiltinPost);
- **топливо**: каждый шаг — единица; при нуле машина возвращает `OutOfFuel(Continuation)` — возобновляемое продолжение (`resume(env, cont, fuel)`). Вечная фуга сосуществует с живой системой: [[celebrare]] делает `yield_now()` между слайсами;
- `unwind`: `Dissonance::User` (clama) ловится ближайшим `RecipeH`, остальное прерывает стих;
- поиск `Var`: скоуп → [[Билтины|билтины]] → педали ([[Фуга как Reader]]) — «фуга звучит под всем, что поёт стих»;
- результат — `VerseOutcome { value, state, plan, committed }`: машина чиста, committed-план наружу несёт драйвер ([[Граница чистоты]]).

Цепочка bind'ов, иссякшая без каданса, отдаёт последнее значение как результат.
