//! Recursive-descent parser for the plain form (§7.1), lowering
//! straight to the Core IR. Statements are newline-separated; inside
//! parentheses newlines are insignificant.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use officium_core::env::{Fugue, Program, Versus};
use officium_core::ir::Expr;
use officium_core::types::{Dissonance, Final, Mode, Value};

use crate::lexer::{lex, rhyme_label, Tok};

pub fn parse(src: &str) -> Result<Program, Dissonance> {
    let lexed = lex(src)?;
    let mut p = Parser { toks: lexed.toks, pos: 0, paren_depth: 0, expr_depth: 0 };
    p.program()
}

struct Parser {
    toks: Vec<(Tok, u32)>,
    pos: usize,
    paren_depth: u32,
    /// Recursion guard: the parser descends on the native stack, and
    /// kernel stacks are finite — refuse pathological nesting politely.
    expr_depth: u32,
}

const MAX_EXPR_DEPTH: u32 = 96;

impl Parser {
    fn err(&self, msg: &str) -> Dissonance {
        let line = self.toks.get(self.pos).or(self.toks.last()).map(|(_, l)| *l).unwrap_or(1);
        Dissonance::Parse { line, msg: String::from(msg) }
    }

    fn peek(&mut self) -> Option<&Tok> {
        // inside parentheses newlines are just whitespace
        while self.paren_depth > 0 && matches!(self.toks.get(self.pos), Some((Tok::Newline, _))) {
            self.pos += 1;
        }
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.peek().cloned()?;
        self.pos += 1;
        Some(t)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.toks.get(self.pos), Some((Tok::Newline, _))) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, want: Tok, what: &str) -> Result<(), Dissonance> {
        if self.eat(&want) {
            Ok(())
        } else {
            Err(self.err(what))
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, Dissonance> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            _ => Err(self.err(what)),
        }
    }

    fn keyword(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    // --- program structure ---

    fn program(&mut self) -> Result<Program, Dissonance> {
        let mut prog = Program::default();
        loop {
            self.skip_newlines();
            if self.peek().is_none() {
                return Ok(prog);
            }
            if self.keyword("fuga") {
                prog.fugues.push(self.fugue()?);
            } else if self.keyword("versus") {
                prog.verses.push(self.versus()?);
            } else {
                return Err(self.err("expected 'fuga' or 'versus'"));
            }
        }
    }

    fn final_and_mode(&mut self) -> Result<(Final, Mode), Dissonance> {
        let name = self.ident("expected a final (Re/Mi/Fa/Sol, lowercase = plagal)")?;
        let (f, plagal) = match name.as_str() {
            "Re" => (Final::Re, false),
            "Mi" => (Final::Mi, false),
            "Fa" => (Final::Fa, false),
            "Sol" => (Final::Sol, false),
            "re" => (Final::Re, true),
            "mi" => (Final::Mi, true),
            "fa" => (Final::Fa, true),
            "sol" => (Final::Sol, true),
            _ => return Err(self.err("unknown final (want Re/Mi/Fa/Sol or re/mi/fa/sol)")),
        };
        Ok((f, Mode::from_final(f, plagal)))
    }

    fn fugue(&mut self) -> Result<Fugue, Dissonance> {
        let name = self.ident("expected fugue name")?;
        if !self.keyword("in") {
            return Err(self.err("expected 'in' after fugue name"));
        }
        let (final_, _) = self.final_and_mode()?;
        let mut fugue = Fugue::new(name, final_);
        self.skip_newlines();
        self.expect(Tok::LBrace, "expected '{' opening the fugue")?;
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace) {
                return Ok(fugue);
            }
            if self.keyword("pedale") {
                let name = self.ident("expected pedal name")?;
                self.expect(Tok::Eq, "expected '=' after pedal name")?;
                fugue.pedals.push((name, self.literal()?));
            } else if self.keyword("subiectum") {
                let name = self.ident("expected subject name")?;
                self.expect(Tok::LParen, "expected '(' after subject name")?;
                self.paren_depth += 1;
                let param = self.ident("expected subject parameter")?;
                self.paren_depth -= 1;
                self.expect(Tok::RParen, "expected ')' after subject parameter")?;
                self.expect(Tok::Eq, "expected '=' after subject header")?;
                let body = self.expr()?;
                fugue.subject = Some((name, param, body));
            } else if self.keyword("reale") {
                let name = self.ident("expected overload name")?;
                self.expect(Tok::At, "expected '@' before genus")?;
                let genus = self.ident("expected genus")?;
                self.expect(Tok::Eq, "expected '=' after overload header")?;
                fugue.real.push(((name, genus), self.expr()?));
            } else if self.keyword("tonale") {
                let name = self.ident("expected overload name")?;
                self.expect(Tok::At, "expected '@' before genus")?;
                let genus = self.ident("expected genus")?;
                self.expect(Tok::Eq, "expected '=' after overload header")?;
                fugue.tonal.push(((name, genus), self.expr()?));
            } else if self.keyword("contra") {
                fugue.contra.push(self.expr()?);
            } else {
                return Err(self.err(
                    "expected pedale/subiectum/reale/tonale/contra or '}' in fugue",
                ));
            }
        }
    }

    fn literal(&mut self) -> Result<Value, Dissonance> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Value::Num(n)),
            Some(Tok::Minus) => match self.next() {
                Some(Tok::Num(n)) => Ok(Value::Num(-n)),
                _ => Err(self.err("expected a number after '-'")),
            },
            Some(Tok::Str(s)) => Ok(Value::Str(s)),
            Some(Tok::Ident(s)) if s == "verum" => Ok(Value::Bool(true)),
            Some(Tok::Ident(s)) if s == "falsum" => Ok(Value::Bool(false)),
            _ => Err(self.err("expected a literal")),
        }
    }

    fn versus(&mut self) -> Result<Versus, Dissonance> {
        let name = self.ident("expected verse name")?;
        if !self.keyword("in") {
            return Err(self.err("expected 'in' after verse name"));
        }
        let (_, mode) = self.final_and_mode()?;
        self.skip_newlines();
        self.expect(Tok::LBrace, "expected '{' opening the verse")?;
        let rhymes = self.liturgical_strip()?;
        let body = self.stmts(mode)?;
        Ok(Versus { name, mode, body, rhymes })
    }

    /// §7.3 lowering. If the verse body opens with `label "text" ;` it
    /// is the sung form: walk the verse's tokens, strip each line's
    /// label + text + separator (recording the rhyme scheme), and leave
    /// a plain-form token stream for `stmts`. A statement may spill
    /// over onto the next sung line (its braces keep the parser going),
    /// and unlabeled continuation lines are left untouched.
    fn liturgical_strip(&mut self) -> Result<Vec<(char, String)>, Dissonance> {
        let mut i = self.pos;
        while matches!(self.toks.get(i), Some((Tok::Newline, _))) {
            i += 1;
        }
        let opens_sung = matches!(
            (self.toks.get(i), self.toks.get(i + 1)),
            (Some((Tok::Ident(l), _)), Some((Tok::Str(_), _))) if rhyme_label(l).is_some()
        );
        if !opens_sung {
            return Ok(Vec::new()); // plain form
        }

        let mut rhymes: Vec<(char, String)> = Vec::new();
        let mut depth = 1u32; // the verse's '{' is already consumed
        let mut at_line_start = true;
        let mut j = i;
        while j < self.toks.len() {
            match &self.toks[j].0 {
                Tok::LBrace => depth += 1,
                Tok::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Tok::Newline => {
                    at_line_start = true;
                    j += 1;
                    continue;
                }
                Tok::Ident(l) if at_line_start && rhyme_label(l).is_some() => {
                    if let Some((Tok::Str(text), line)) = self.toks.get(j + 1).cloned() {
                        if !matches!(self.toks.get(j + 2), Some((Tok::Semi, _))) {
                            return Err(Dissonance::Parse {
                                line,
                                msg: String::from("expected ';' after the sung text"),
                            });
                        }
                        rhymes.push((rhyme_label(l).unwrap(), text));
                        self.toks.drain(j..j + 3);
                        at_line_start = false;
                        continue; // the statement now starts at j
                    }
                }
                _ => {}
            }
            at_line_start = false;
            j += 1;
        }

        let line = self.toks.get(self.pos).map(|(_, l)| *l).unwrap_or(1);
        let err = |msg: String| Dissonance::Parse { line, msg };
        if rhymes.len() != 8 {
            return Err(err(format!(
                "a verse is sung in eight lines (this one has {})",
                rhymes.len()
            )));
        }
        // the volta couplet (lines 7-8) must rhyme with itself
        if rhymes[6].0 != rhymes[7].0 {
            return Err(err(String::from(
                "the volta couplet must rhyme (lines 7 and 8 share a label)",
            )));
        }
        // every rhyme must be answered by at least one other line
        for (l, _) in &rhymes {
            if rhymes.iter().filter(|(m, _)| m == l).count() < 2 {
                return Err(err(format!("rhyme '{l}' is never answered")));
            }
        }
        Ok(rhymes)
    }

    /// Parse statements up to the closing '}' and lower them to a bind
    /// chain ending in a cadence (§4.3). An `si` block must be the final
    /// statement (its branches carry their own cadences).
    fn stmts(&mut self, mode: Mode) -> Result<Expr, Dissonance> {
        self.skip_newlines();

        // cadences terminate the statement list
        if self.keyword("amen") {
            let e = self.expr()?;
            self.close_stmts()?;
            return Ok(Expr::Return(mode, Box::new(e)));
        }
        if self.keyword("nihil") {
            self.close_stmts()?;
            return Ok(Expr::Nihil);
        }
        if self.keyword("clama") {
            let e = self.expr()?;
            self.close_stmts()?;
            return Ok(Expr::Clama(Box::new(e)));
        }
        if self.keyword("perage") {
            let e = self.expr()?;
            self.close_stmts()?;
            return Ok(Expr::Perage(Box::new(e)));
        }

        if self.keyword("si") {
            let cond = self.expr()?;
            if !self.keyword("tunc") {
                return Err(self.err("expected 'tunc' after si-condition"));
            }
            self.skip_newlines();
            self.expect(Tok::LBrace, "expected '{' after tunc")?;
            let then_e = self.stmts(mode)?;
            self.skip_newlines();
            if !self.keyword("aliter") {
                return Err(self.err("expected 'aliter' after tunc-block"));
            }
            self.skip_newlines();
            self.expect(Tok::LBrace, "expected '{' after aliter")?;
            let else_e = self.stmts(mode)?;
            self.close_stmts()?;
            return Ok(Expr::If(Box::new(cond), Box::new(then_e), Box::new(else_e)));
        }

        // non-terminal statements: bind / pone / mitte, then recurse
        let m = if self.keyword("pone") {
            let e = self.expr()?;
            Expr::Bind(mode, Box::new(Expr::Pone(Box::new(e))), String::from("_"), Box::new(Expr::Lit(Value::Unit)))
        } else if self.keyword("mitte") {
            let e = self.expr()?;
            Expr::Bind(mode, Box::new(Expr::Mitte(Box::new(e))), String::from("_"), Box::new(Expr::Lit(Value::Unit)))
        } else {
            let name = self.ident("expected a statement or cadence")?;
            self.expect(Tok::BindArrow, "expected '<-' after binder")?;
            let rhs = self.expr()?;
            Expr::Bind(mode, Box::new(rhs), name, Box::new(Expr::Lit(Value::Unit)))
        };

        let rest = self.stmts(mode)?;
        // splice `rest` in place of the placeholder continuation
        Ok(match m {
            Expr::Bind(md, mval, name, _) => Expr::Bind(md, mval, name, Box::new(rest)),
            _ => unreachable!(),
        })
    }

    /// After a cadence: expect '}' closing the statement block.
    fn close_stmts(&mut self) -> Result<(), Dissonance> {
        self.skip_newlines();
        self.expect(Tok::RBrace, "expected '}' after the cadence")
    }

    // --- expressions ---

    fn expr(&mut self) -> Result<Expr, Dissonance> {
        self.expr_depth += 1;
        let r = if self.expr_depth > MAX_EXPR_DEPTH {
            Err(self.err("expression nests too deeply"))
        } else {
            self.additive()
        };
        self.expr_depth -= 1;
        r
    }

    fn additive(&mut self) -> Result<Expr, Dissonance> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => "add",
                Some(Tok::Minus) => "sub",
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.multiplicative()?;
            lhs = call2(op, lhs, rhs);
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, Dissonance> {
        let mut lhs = self.application()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => "mul",
                Some(Tok::Slash) => "div",
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.application()?;
            lhs = call2(op, lhs, rhs);
        }
    }

    /// Left-associative application by juxtaposition: `solve f r`,
    /// `mass(corpus)`. Stops at operators, newlines (outside parens),
    /// statement keywords and closing delimiters.
    fn application(&mut self) -> Result<Expr, Dissonance> {
        let mut e = self.atom()?;
        while self.starts_atom() {
            let a = self.atom()?;
            e = Expr::App(Box::new(e), Box::new(a));
        }
        Ok(e)
    }

    fn starts_atom(&mut self) -> bool {
        match self.peek() {
            Some(Tok::Num(_)) | Some(Tok::Str(_)) | Some(Tok::LParen) | Some(Tok::Lambda) => true,
            Some(Tok::Ident(s)) => !matches!(
                s.as_str(),
                "tunc" | "aliter" | "amen" | "nihil" | "clama" | "perage" | "si"
                    | "pone" | "mitte" | "fuga" | "versus" | "in"
                    | "pedale" | "subiectum" | "reale" | "tonale" | "contra"
            ),
            _ => false,
        }
    }

    fn atom(&mut self) -> Result<Expr, Dissonance> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::Num(n)))
            }
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::Str(s)))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                self.paren_depth += 1;
                let e = self.expr()?;
                self.paren_depth -= 1;
                self.expect(Tok::RParen, "expected ')'")?;
                Ok(e)
            }
            Some(Tok::Lambda) => {
                self.pos += 1;
                let param = self.ident("expected a parameter after '\\'")?;
                self.expect(Tok::Arrow, "expected '->' in lambda")?;
                let body = self.expr()?;
                Ok(Expr::Lam(param, Box::new(body)))
            }
            Some(Tok::Minus) => {
                // unary minus: -e  =>  0 - e
                self.pos += 1;
                let e = self.atom()?;
                Ok(call2("sub", Expr::Lit(Value::Num(0.0)), e))
            }
            Some(Tok::Ident(s)) => {
                self.pos += 1;
                match s.as_str() {
                    "arg" => Ok(Expr::Arg),
                    "lege" => Ok(Expr::Lege),
                    "verum" => Ok(Expr::Lit(Value::Bool(true))),
                    "falsum" => Ok(Expr::Lit(Value::Bool(false))),
                    "ask" => {
                        let name = self.name_arg("expected a name after 'ask'")?;
                        Ok(Expr::Ask(name))
                    }
                    "resolve" => {
                        let name = self.name_arg("expected a name after 'resolve'")?;
                        let genus = self.atom()?;
                        Ok(Expr::Resolve(name, Box::new(genus)))
                    }
                    "pure" => {
                        // mode tag is patched by the verse lowering; use a
                        // neutral tag here (Pure ignores it at eval)
                        let e = self.atom()?;
                        Ok(Expr::Pure(Mode::Dorian, Box::new(e)))
                    }
                    "leva" => {
                        // the transformer's lift (M9): run the enclosed
                        // computation one layer down the tower
                        let e = self.atom()?;
                        Ok(Expr::Lift(Box::new(e)))
                    }
                    "pone" => {
                        // pone as an expression (yields unit), so a put
                        // can live inside `leva (pone e)`
                        let e = self.atom()?;
                        Ok(Expr::Pone(Box::new(e)))
                    }
                    _ => Ok(Expr::Var(s)),
                }
            }
            _ => Err(self.err("expected an expression")),
        }
    }

    /// `ask "G"` or `ask G`; `resolve "trahere" ...` or `resolve trahere ...`.
    fn name_arg(&mut self, what: &str) -> Result<String, Dissonance> {
        match self.next() {
            Some(Tok::Str(s)) | Some(Tok::Ident(s)) => Ok(s),
            _ => Err(self.err(&format!("{what}"))),
        }
    }
}

fn call2(name: &str, a: Expr, b2: Expr) -> Expr {
    Expr::App(
        Box::new(Expr::App(Box::new(Expr::Var(String::from(name))), Box::new(a))),
        Box::new(b2),
    )
}
