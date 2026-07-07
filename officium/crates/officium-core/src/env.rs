//! The fugue and the Reader environment compiled from it (§4.2, §5).

use alloc::string::String;
use alloc::vec::Vec;

use crate::ir::Expr;
use crate::types::{Dissonance, Final, Genus, Mode, Sym, Value};

/// A fugue: what the organ holds while the verses are sung.
#[derive(Debug, Clone)]
pub struct Fugue {
    pub name: Sym,
    pub final_: Final,
    /// Pedal point constants (G = the gravitational constant, ...).
    pub pedals: Vec<(Sym, Value)>,
    /// The subject: main definition, with its parameter name.
    pub subject: Option<(Sym, Sym, Expr)>, // (name, param, body)
    /// Real answers: exact overloads keyed by (name, genus).
    pub real: Vec<((Sym, Genus), Expr)>,
    /// Tonal answers: coerced overloads (may inject a countersubject
    /// term, e.g. outgassing for comets).
    pub tonal: Vec<((Sym, Genus), Expr)>,
    /// Countersubjects — terms that always sound with the subject.
    pub contra: Vec<Expr>,
}

impl Fugue {
    pub fn new(name: Sym, final_: Final) -> Fugue {
        Fugue {
            name,
            final_,
            pedals: Vec::new(),
            subject: None,
            real: Vec::new(),
            tonal: Vec::new(),
            contra: Vec::new(),
        }
    }
}

/// A verse: an eight-line pipeline lowered to a bind chain.
#[derive(Debug, Clone)]
pub struct Versus {
    pub name: Sym,
    pub mode: Mode,
    pub body: Expr,
}

/// A parsed score: fugues + verses.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub fugues: Vec<Fugue>,
    pub verses: Vec<Versus>,
}

impl Program {
    pub fn verse(&self, name: &str) -> Option<&Versus> {
        self.verses.iter().find(|v| v.name == name)
    }
}

/// The environment a verse is evaluated against: pedals + resolver
/// tables. The number of voices = the environment size (sanity metric).
#[derive(Debug, Clone, Default)]
pub struct Env {
    pedals: Vec<(Sym, Value)>,
    real: Vec<((Sym, Genus), Expr)>,
    tonal: Vec<((Sym, Genus), Expr)>,
    contra: Vec<Expr>,
    subjects: Vec<(Sym, Sym, Expr)>,
}

impl Env {
    /// Compile one or more fugues into a Reader environment.
    pub fn from_fugues(fugues: &[Fugue]) -> Env {
        let mut env = Env::default();
        for f in fugues {
            env.pedals.extend(f.pedals.iter().cloned());
            env.real.extend(f.real.iter().cloned());
            env.tonal.extend(f.tonal.iter().cloned());
            env.contra.extend(f.contra.iter().cloned());
            if let Some(s) = &f.subject {
                env.subjects.push(s.clone());
            }
        }
        env
    }

    /// Voices = size of the environment.
    pub fn voices(&self) -> usize {
        self.pedals.len() + self.real.len() + self.tonal.len() + self.contra.len()
            + self.subjects.len()
    }

    /// `ask "G"`: read a pedal constant.
    pub fn ask(&self, s: &str) -> Result<Value, Dissonance> {
        self.pedals
            .iter()
            .find(|(n, _)| n == s)
            .map(|(_, v)| v.clone())
            .ok_or(Dissonance::UnboundVar { name: String::from(s) })
    }

    /// Overload resolution by genus (§4.2): real answer (exact) first,
    /// then tonal answer (coerced), else `Unresolved`. Returns the
    /// overload *expression*; the machine evaluates it in the current
    /// lexical scope so the subject parameter and pedals are visible.
    pub fn resolve(&self, name: &str, genus: &str) -> Result<&Expr, Dissonance> {
        if let Some((_, e)) = self
            .real
            .iter()
            .find(|((n, g), _)| n == name && g == genus)
        {
            return Ok(e);
        }
        if let Some((_, e)) = self
            .tonal
            .iter()
            .find(|((n, g), _)| n == name && g == genus)
        {
            return Ok(e);
        }
        Err(Dissonance::Unresolved {
            name: String::from(name),
            genus: String::from(genus),
        })
    }

    /// The subject definition by name (used by render and diagnostics).
    pub fn subject(&self, name: &str) -> Option<&(Sym, Sym, Expr)> {
        self.subjects.iter().find(|(n, _, _)| n == name)
    }

    pub fn pedals(&self) -> &[(Sym, Value)] {
        &self.pedals
    }

    pub fn contra(&self) -> &[Expr] {
        &self.contra
    }
}
