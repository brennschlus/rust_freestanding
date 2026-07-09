---
tags: [officium/impl]
---
# Core IR

`officium-core/src/ir.rs` — нетипизированное дерево `Expr`, значения теггируются в рантайме:

- λ-ядро: `Var`, `Lam`, `App`, `Lit`, `If`, **`Fix`** (стретто: разворачивается лениво как `f (\x -> Fix f x)` через трамплин — рекурсия не ест стек);
- монадическое: `Pure(mode, e)` (значение каданса, не останавливает), `Return(mode, e)` (= `amen`), `Bind(mode, m, x, k)`;
- Reader: `Arg`, `Ask(name)`, `Resolve(name, genus)` ([[Фуга как Reader]]);
- модальные: `Nihil`, `Clama`, `Recipe(body, x, handler)`, `Lege`, `Pone`, `Mitte`, `Perage`, `Lift` — легальность решает [[Лады как монады|маска]].

`Value` (`types.rs`): Unit, Num (f64), Bool, Genus, Str, Closure (Rc-скоуп), Builtin (каррируемый), Cmd (GravityCmd), Plan, Field, Record (тела-corpus'ы и Tritus-состояние St — решение §15: St = Record, Scalar = f64, nihil = `Dissonance::Silent`).

`Scope` — персистентный связный список на Rc: дёшево захватывается замыканиями.

Исполнение: [[CEK-машина и топливо]]. Статика: [[Тайпчекер — рифма это унификация]].
