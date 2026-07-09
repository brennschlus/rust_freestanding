//! M7 — the static checker (§7.4): rhyme = unification.
//!
//! Two passes folded into one walk. First, plain Hindley-Milner-style
//! inference over a verse's lowered IR, mirroring the machine's
//! semantics exactly (variable lookup falls through scope → builtins →
//! pedals; `mitte` takes a cmd or a plan; `perage` demands a Plan and
//! cadences with unit; the capability mask is checked statically too).
//! Second, for sung verses (§7.3), the rhyme constraints: lines sharing
//! a label carry the same type variable and must unify; the final
//! couplet (lines 7-8) carries the verse's result type.
//!
//! Every diagnostic is wrapped in `Dissonance::Discors` naming the
//! verse, so a broken score is refused loudly before a single note.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::env::Program;
use crate::ir::Expr;
use crate::types::{Dissonance, Mode, OpKind, Sym, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Var(usize),
    Num,
    Str,
    Bool,
    Unit,
    Genus,
    Corpus, // a body record (and any other record value)
    Field,
    Plan,
    Cmd,
    Fun(alloc::boxed::Box<Type>, alloc::boxed::Box<Type>),
}

impl Type {
    /// The same names `Value::ty()` uses in runtime diagnostics.
    fn hint(&self) -> &'static str {
        match self {
            Type::Var(_) => "?",
            Type::Num => "num",
            Type::Str => "str",
            Type::Bool => "bool",
            Type::Unit => "unit",
            Type::Genus => "genus",
            Type::Corpus => "record",
            Type::Field => "field",
            Type::Plan => "plan",
            Type::Cmd => "cmd",
            Type::Fun(_, _) => "closure",
        }
    }
}

fn fun(a: Type, r: Type) -> Type {
    Type::Fun(alloc::boxed::Box::new(a), alloc::boxed::Box::new(r))
}

fn fun2(a: Type, b: Type, r: Type) -> Type {
    fun(a, fun(b, r))
}

/// A deferred "one of these" constraint for the few union-typed spots
/// (`mitte` eats a cmd or a whole plan; `corpus` takes genus or str).
struct Pred {
    ty: Type,
    allowed: &'static [&'static str],
    want: &'static str,
}

/// Type clash detail before it is wrapped into `Discors`.
enum Clash {
    Types(&'static str, &'static str),
    Infinite,
    Unbound(Sym),
    Mode(Mode, OpKind),
}

impl Clash {
    fn describe(&self) -> String {
        match self {
            Clash::Types(want, got) => format!("type clash: want {want}, got {got}"),
            Clash::Infinite => String::from("infinite type (the stretto swallows itself)"),
            Clash::Unbound(n) => format!("unbound name '{n}'"),
            Clash::Mode(m, op) => format!("op {op:?} is illegal in {m:?}"),
        }
    }
}

struct Checker<'p> {
    prog: &'p Program,
    subst: Vec<Option<Type>>,
    preds: Vec<Pred>,
    /// Per-verse type variables: the input, the Tritus state cell, the
    /// Deuterus thrown value, and the cadence result (`M a`).
    arg: Type,
    state: Type,
    throw: Type,
    result: Type,
}

/// Check every verse of a program. Fugues are not sung and are typed
/// only where a verse touches them (pedals via `ask`/free variables,
/// overloads at their `resolve` site — in the caller's lexical scope,
/// exactly as the machine evaluates them).
pub fn check_program(prog: &Program) -> Result<(), Dissonance> {
    for v in &prog.verses {
        check_versus(prog, v)?;
    }
    Ok(())
}

pub fn check_versus(prog: &Program, v: &crate::env::Versus) -> Result<(), Dissonance> {
    let mut c = Checker::new(prog);
    let discors = |msg: String| Dissonance::Discors { versus: v.name.clone(), msg };

    let mut gamma: Vec<(Sym, Type)> = Vec::new();
    if v.rhymes.is_empty() {
        c.infer(v.mode, &mut gamma, &v.body).map_err(|e| discors(e.describe()))?;
    } else {
        // walk the top-level bind spine so each statement's type can be
        // pinned to its sung line
        let mut line_tys: Vec<Type> = Vec::new();
        let mut cur = &v.body;
        loop {
            match cur {
                Expr::Bind(_, m, x, k) => {
                    let (line, bound) = c
                        .infer_stmt(v.mode, &mut gamma, m)
                        .map_err(|e| discors(e.describe()))?;
                    line_tys.push(line);
                    gamma.push((x.clone(), bound));
                    cur = k;
                }
                terminal => {
                    c.infer(v.mode, &mut gamma, terminal)
                        .map_err(|e| discors(e.describe()))?;
                    break;
                }
            }
        }

        // rhyme = unification: line k carries its statement's type; the
        // volta couplet (7-8) carries the verse's result instead (§7.4)
        let sung: Vec<Type> = (1..=v.rhymes.len())
            .map(|k| {
                if k >= 7 || k > line_tys.len() {
                    c.result.clone()
                } else {
                    line_tys[k - 1].clone()
                }
            })
            .collect();
        for (i, (label, _)) in v.rhymes.iter().enumerate() {
            for (j, (other, _)) in v.rhymes.iter().enumerate().skip(i + 1) {
                if label != other {
                    continue;
                }
                let (a, b) = (sung[i].clone(), sung[j].clone());
                c.unify(&a, &b).map_err(|e| {
                    discors(format!(
                        "lines {} and {} rhyme on '{}' but do not unify — {}",
                        i + 1,
                        j + 1,
                        label,
                        e.describe()
                    ))
                })?;
            }
        }
    }

    c.check_preds().map_err(|e| discors(e.describe()))
}

impl<'p> Checker<'p> {
    fn new(prog: &'p Program) -> Checker<'p> {
        let mut c = Checker {
            prog,
            subst: Vec::new(),
            preds: Vec::new(),
            arg: Type::Unit,
            state: Type::Unit,
            throw: Type::Unit,
            result: Type::Unit,
        };
        c.arg = c.fresh();
        c.state = c.fresh();
        c.throw = c.fresh();
        c.result = c.fresh();
        c
    }

    fn fresh(&mut self) -> Type {
        self.subst.push(None);
        Type::Var(self.subst.len() - 1)
    }

    /// Follow the substitution to the representative of `t`.
    fn shallow(&self, mut t: Type) -> Type {
        while let Type::Var(i) = t {
            match &self.subst[i] {
                Some(next) => t = next.clone(),
                None => return Type::Var(i),
            }
        }
        t
    }

    fn occurs(&self, var: usize, t: &Type) -> bool {
        match self.shallow(t.clone()) {
            Type::Var(i) => i == var,
            Type::Fun(a, r) => self.occurs(var, &a) || self.occurs(var, &r),
            _ => false,
        }
    }

    fn unify(&mut self, a: &Type, b: &Type) -> Result<(), Clash> {
        let a = self.shallow(a.clone());
        let b = self.shallow(b.clone());
        match (a, b) {
            (Type::Var(i), Type::Var(j)) if i == j => Ok(()),
            (Type::Var(i), t) | (t, Type::Var(i)) => {
                if self.occurs(i, &t) {
                    return Err(Clash::Infinite);
                }
                self.subst[i] = Some(t);
                Ok(())
            }
            (Type::Fun(a1, r1), Type::Fun(a2, r2)) => {
                self.unify(&a1, &a2)?;
                self.unify(&r1, &r2)
            }
            (x, y) if x == y => Ok(()),
            (x, y) => Err(Clash::Types(x.hint(), y.hint())),
        }
    }

    fn check_mode(&self, mode: Mode, op: OpKind) -> Result<(), Clash> {
        if mode.allows(op) {
            Ok(())
        } else {
            Err(Clash::Mode(mode, op))
        }
    }

    fn check_preds(&mut self) -> Result<(), Clash> {
        for k in 0..self.preds.len() {
            let t = self.shallow(self.preds[k].ty.clone());
            if matches!(t, Type::Var(_)) {
                continue; // never constrained: any runtime value may fit
            }
            if !self.preds[k].allowed.contains(&t.hint()) {
                return Err(Clash::Types(self.preds[k].want, t.hint()));
            }
        }
        Ok(())
    }

    /// One statement of the bind spine: returns (sung-line type, type
    /// bound into scope). `pone e`/`mitte e` sing about `e` but bind
    /// unit — the machine threads unit through their binder too.
    fn infer_stmt(
        &mut self,
        mode: Mode,
        gamma: &mut Vec<(Sym, Type)>,
        m: &Expr,
    ) -> Result<(Type, Type), Clash> {
        match m {
            Expr::Pone(e) => {
                self.check_mode(mode, OpKind::Pone)?;
                let te = self.infer(mode, gamma, e)?;
                self.unify(&te, &self.state.clone())?;
                Ok((te, Type::Unit))
            }
            Expr::Mitte(e) => {
                self.check_mode(mode, OpKind::Mitte)?;
                let te = self.infer(mode, gamma, e)?;
                self.preds.push(Pred { ty: te.clone(), allowed: &["cmd", "plan"], want: "cmd" });
                Ok((te, Type::Unit))
            }
            other => {
                let t = self.infer(mode, gamma, other)?;
                Ok((t.clone(), t))
            }
        }
    }

    /// The machine's `Var` fallback chain, statically: lexical scope,
    /// then builtins, then the pedals of every fugue in the program.
    fn lookup(&mut self, gamma: &[(Sym, Type)], name: &str) -> Result<Type, Clash> {
        if let Some((_, t)) = gamma.iter().rev().find(|(n, _)| n == name) {
            return Ok(t.clone());
        }
        if let Some(t) = self.builtin(name) {
            return Ok(t);
        }
        self.pedal(name).ok_or_else(|| Clash::Unbound(String::from(name)))
    }

    fn pedal(&self, name: &str) -> Option<Type> {
        for f in &self.prog.fugues {
            if let Some((_, v)) = f.pedals.iter().find(|(n, _)| n == name) {
                return Some(type_of_value(v));
            }
        }
        None
    }

    /// Builtin signatures — keep in sync with `builtins::arity`/`call`.
    fn builtin(&mut self, name: &str) -> Option<Type> {
        use Type::*;
        Some(match name {
            "add" | "sub" | "mul" | "div" => fun2(Num, Num, Num),
            "le" | "lt" => fun2(Num, Num, Bool),
            "eq" => {
                let a = self.fresh();
                fun2(a.clone(), a, Bool)
            }
            "not" => fun(Bool, Bool),
            "dist" | "mass" | "outgassing" => fun(Corpus, Num),
            "genus" => fun(Corpus, Genus),
            "safe" => fun2(Corpus, Num, Bool),
            // solve applies the resolved force law, then Δv (post-op
            // demands the law lands on a num)
            "solve" => {
                let a = self.fresh();
                fun2(fun(a.clone(), Num), a, Num)
            }
            "deflect" => fun(Num, Field),
            "target" | "dv_of" | "at" => fun(Field, Num),
            "cmd" => fun2(Num, Num, fun(Num, Cmd)),
            "plan_of" => fun(Field, Plan),
            "corpus" => {
                let g = self.fresh();
                self.preds.push(Pred {
                    ty: g.clone(),
                    allowed: &["genus", "str"],
                    want: "genus",
                });
                fun2(Num, g, fun2(Num, Num, Corpus))
            }
            _ => return None,
        })
    }

    fn infer(
        &mut self,
        mode: Mode,
        gamma: &mut Vec<(Sym, Type)>,
        e: &Expr,
    ) -> Result<Type, Clash> {
        match e {
            Expr::Lit(v) => Ok(type_of_value(v)),
            Expr::Var(name) => self.lookup(gamma, name),

            Expr::Lam(x, body) => {
                let a = self.fresh();
                gamma.push((x.clone(), a.clone()));
                let r = self.infer(mode, gamma, body)?;
                gamma.pop();
                Ok(fun(a, r))
            }

            Expr::App(f, a) => {
                let tf = self.infer(mode, gamma, f)?;
                let ta = self.infer(mode, gamma, a)?;
                let r = self.fresh();
                self.unify(&tf, &fun(ta, r.clone()))?;
                Ok(r)
            }

            Expr::If(c, t, e2) => {
                let tc = self.infer(mode, gamma, c)?;
                self.unify(&tc, &Type::Bool)?;
                let tt = self.infer(mode, gamma, t)?;
                let te = self.infer(mode, gamma, e2)?;
                self.unify(&tt, &te)?;
                Ok(tt)
            }

            // fix f = f (fix f): the operand maps a thing to itself
            Expr::Fix(f) => {
                let tf = self.infer(mode, gamma, f)?;
                let a = self.fresh();
                self.unify(&tf, &fun(a.clone(), a.clone()))?;
                Ok(a)
            }

            Expr::Pure(_, e2) | Expr::Lift(e2) => {
                if matches!(e, Expr::Lift(_)) {
                    self.check_mode(mode, OpKind::Lift)?;
                    // the lifted body runs as the authentic mode
                    let base = Mode::from_final(mode.final_(), false);
                    return self.infer(base, gamma, e2);
                }
                self.infer(mode, gamma, e2)
            }

            Expr::Return(_, e2) => {
                let t = self.infer(mode, gamma, e2)?;
                self.unify(&t, &self.result.clone())?;
                Ok(self.fresh()) // nothing follows a cadence
            }

            Expr::Bind(_, m, x, k) => {
                let (_, bound) = self.infer_stmt(mode, gamma, m)?;
                gamma.push((x.clone(), bound));
                let t = self.infer(mode, gamma, k)?;
                gamma.pop();
                Ok(t)
            }

            Expr::Arg => Ok(self.arg.clone()),

            Expr::Ask(name) => {
                self.pedal(name).ok_or_else(|| Clash::Unbound(String::from(name)))
            }

            // the overload is evaluated in the caller's lexical scope,
            // so it is typed there too: every real/tonal answer for the
            // name must fit one type at this site
            Expr::Resolve(name, genus_expr) => {
                let tg = self.infer(mode, gamma, genus_expr)?;
                self.unify(&tg, &Type::Genus)?;
                let r = self.fresh();
                let prog = self.prog;
                for f in &prog.fugues {
                    for ((n, _), overload) in f.real.iter().chain(f.tonal.iter()) {
                        if n == name {
                            let to = self.infer(mode, gamma, overload)?;
                            self.unify(&to, &r)?;
                        }
                    }
                }
                Ok(r)
            }

            Expr::Nihil => {
                self.check_mode(mode, OpKind::Nihil)?;
                Ok(self.fresh())
            }

            Expr::Clama(e2) => {
                self.check_mode(mode, OpKind::Clama)?;
                let t = self.infer(mode, gamma, e2)?;
                self.unify(&t, &self.throw.clone())?;
                Ok(self.fresh())
            }

            Expr::Recipe(body, x, handler) => {
                self.check_mode(mode, OpKind::Recipe)?;
                let tb = self.infer(mode, gamma, body)?;
                gamma.push((x.clone(), self.throw.clone()));
                let th = self.infer(mode, gamma, handler)?;
                gamma.pop();
                self.unify(&tb, &th)?;
                Ok(tb)
            }

            Expr::Lege => {
                self.check_mode(mode, OpKind::Lege)?;
                Ok(self.state.clone())
            }

            Expr::Pone(e2) => {
                self.check_mode(mode, OpKind::Pone)?;
                let t = self.infer(mode, gamma, e2)?;
                self.unify(&t, &self.state.clone())?;
                Ok(Type::Unit)
            }

            Expr::Mitte(e2) => {
                self.check_mode(mode, OpKind::Mitte)?;
                let t = self.infer(mode, gamma, e2)?;
                self.preds.push(Pred { ty: t, allowed: &["cmd", "plan"], want: "cmd" });
                Ok(Type::Unit)
            }

            Expr::Perage(e2) => {
                self.check_mode(mode, OpKind::Perage)?;
                let t = self.infer(mode, gamma, e2)?;
                self.unify(&t, &Type::Plan)?;
                // the machine cadences a committed verse with unit
                self.unify(&Type::Unit, &self.result.clone())?;
                Ok(self.fresh())
            }
        }
    }
}

fn type_of_value(v: &Value) -> Type {
    match v {
        Value::Unit => Type::Unit,
        Value::Num(_) => Type::Num,
        Value::Bool(_) => Type::Bool,
        Value::Genus(_) => Type::Genus,
        Value::Str(_) => Type::Str,
        Value::Cmd(_) => Type::Cmd,
        Value::Plan(_) => Type::Plan,
        Value::Field(_) => Type::Field,
        Value::Record(_) => Type::Corpus,
        // not constructible from a score; typed opaquely
        Value::Closure { .. } | Value::Builtin { .. } => Type::Unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;

    /// Every builtin the machine knows must have a signature here whose
    /// arity agrees — otherwise the checker would reject a valid score
    /// (or type a call wrongly).
    #[test]
    fn builtin_signatures_cover_the_machine_table() {
        let names = [
            "add", "sub", "mul", "div", "eq", "le", "lt", "not", "dist", "mass", "genus",
            "outgassing", "safe", "solve", "deflect", "target", "dv_of", "at", "cmd",
            "plan_of", "corpus",
        ];
        let prog = Program::default();
        let mut c = Checker::new(&prog);
        for name in names {
            let want = builtins::arity(name).expect("machine builtin missing");
            let mut t = c.builtin(name).expect("checker signature missing");
            let mut n = 0;
            while let Type::Fun(_, r) = t {
                n += 1;
                t = *r;
            }
            assert_eq!(n, want, "arity of '{name}'");
        }
    }

    #[test]
    fn occurs_check_refuses_the_self_swallowing_stretto() {
        let prog = Program::default();
        let mut c = Checker::new(&prog);
        let a = c.fresh();
        let f = fun(a.clone(), Type::Num);
        assert!(matches!(c.unify(&a, &f), Err(Clash::Infinite)));
    }
}
