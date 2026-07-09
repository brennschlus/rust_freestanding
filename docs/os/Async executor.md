---
tags: [kernel/async]
---
# Async executor

`kernel/src/task/` — кооперативная многозадачность из туториала Оппермана:

- **Task** — `Pin<Box<dyn Future<Output = ()>>>` с TaskId;
- **Executor** — очередь готовых задач (`ArrayQueue`), **TaskWaker** кладёт TaskId обратно при пробуждении;
- **сон без гонок**: перед `hlt` прерывания выключаются, очередь перепроверяется, затем `sti; hlt` одной парой — пробуждение между проверкой и hlt не теряется.

На executor'е живут: [[Шелл]], декодер [[ScancodeStream|клавиатуры]], [[Async sleep]] и длинные вычисления [[celebrare|Officium]] (между fuel-слайсами — `yield_now()` из `task/mod.rs`, система остаётся живой даже под вечной фугой — см. [[CEK-машина и топливо]]).
