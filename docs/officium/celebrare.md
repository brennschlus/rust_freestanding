---
tags: [officium, kernel]
---
# celebrare

`kernel/src/celebrare.rs` — интерпретатор Officium в ядре (§10). Юзерспейса нет: интерпретатор — no_std-библиотека, весь контакт с миром через трейты `Platform` и `Organ`.

## Команда шелла
`celebrare meteor | cantus | -` ([[Шелл]]); `-` — набор скора с клавиатуры до пустой строки; `meteor`/`cantus` запечены `include_str!` ([[Метеорный скор]]).

## Конвейер
парс ([[Плоская форма]]) → [[Тайпчекер — рифма это унификация|check_program]] (Discors = отказ) → печать поэмы для спетых стихов → аудио-«прочтение» ([[Рендер]] + `play` на [[Async sleep|sleep_ms]]) → исполнение: если есть correctio — §14-проводка по процессии тел (астероид/комета/камешек), иначе первый стих с аргументом `Num(0)`.

## Детали
- **KernelPlatform**: gravity_sink = println «GRAVITAS …» (v1-железо — консоль; шов останется единственным при подмене на настоящее), now = тики×55000, say = println.
- **KernelOrgan**: note_on/off → [[AC97 драйвер|ac97]] по частоте из pitch_hz.
- **Топливо**: FUEL_SLICE = 20 000 шагов, между слайсами `yield_now()` ([[Async executor]]); MAX_SLICES = 500 — вечная фуга сдаётся с сообщением «versus … numquam cadenzat», система цела ([[CEK-машина и топливо]]).
- `loud()` — тематические сообщения [[Dissonance|диссонансов]] (PREMATURE COMMIT — «Singing mode VII too early would tear the Earth apart»).

Hosted-зеркало — [[Хост-раннер]].
