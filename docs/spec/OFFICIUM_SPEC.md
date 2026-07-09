# Officium — Interpreter Specification (for Claude Code)

> **Mission.** Build an interpreter for *Officium*, a Turing-complete functional
> language in which **musical structure is the semantics**, not decoration.
> Verses (sung, 8-line) are monadic pipelines; fugues (played on the organ)
> are the `Reader` environment. The target runtime is a bare-metal `no_std`
> Rust microkernel that already has interrupts, paging, a keyboard IRQ path,
> an async executor, an AC'97 (Intel 82801AA / ICH) organ synthesizer, and a
> 5-command shell. This document specifies **what to build and in what order**.
> The reference types below are a contract skeleton; refine as needed but do
> not change the semantics in §4 without saying so.

---

## 0. One-screen semantics (self-contained recap)

Officium binds four things that all happen to come in eights: the **eight
Gregorian modes**, the **eight-line verse**, the **monastery**, and the
**organ**. The load-bearing coincidence is that there are exactly eight church
modes, grouped around four *finals*.

| Final | Modes (authentic / plagal) | Monad family | Liturgical op names |
|---|---|---|---|
| **Re / D** (Protus) | Dorian / Hypodorian | `Maybe` / `MaybeT` | `nihil` = Nothing, `si…tunc…aliter` = branch |
| **Mi / E** (Deuterus) | Phrygian / Hypophrygian | `Either` / `ExceptT` | `clama` = throw, `recipe` = catch |
| **Fa / F** (Tritus) | Lydian / Hypolydian | `State` / `StateT` | `lege`/`pone` = get/put |
| **Sol / G** (Tetrardus) | Mixolydian / Hypomixolydian | `IO` / `Plan (Writer)` | `mitte` = emit cmd, `perage` = commit |

- **Authentic vs. plagal = base monad vs. its transformer.** Plagal "reaches
  below" the final → adds a layer *under* the stack. (In v1 this is a
  *capability distinction*, not a separate monad — see §6.)
- **Reader is the fugue.** It is *not sung*. The organ holds it: subject = main
  definition; **real answer** (subject an octave up, exact) = precise overload;
  **tonal answer** (subject with interval adjustment) = coerced overload;
  **countersubject** (always sounds with the subject) = an always-present term
  (`r²`); **voices** = size of the environment; **pedal point** (held in the
  organ's actual pedals) = a physical constant.
- **Verse = 8 lines.** Rhyme scheme = type constraints (rhyming lines unify).
  A-lines carry frame/coordinate, B-lines carry force, the final couplet (C)
  builds the result. **Cadence to the tonic = `return`/`pure` + halt** (`Amen`).
- **Turing completeness.** λ-abstraction (a verse is a function of its input);
  application (calling the subject); **recursion via stretto** (subject
  re-entering over itself before the previous statement ends = fixpoint);
  branching (Protus/Deuterus); unbounded memory (Tritus `State`). Untyped
  λ + `fix` alone is already TC; the modes only decide *which effects are legal*.
- **Purity discipline (the whole game).** Only **Mixolydian (VII)** moves real
  gravity. **Hypomixolydian (VIII)** *plans* it purely (accumulates a `Plan` of
  gravity commands, changes nothing). Everything else is pure computation.
  "Applying the field" = taking a `Plan` and singing it in Mixolydian
  (`perage`). Sing VII early, or over an uncomputed field → the real mass jerks
  on garbage → the meteor is not deflected, or Earth itself is torn. The
  **halting problem = whether a given fugue ever cadences to tonic** — which is
  theologically apt: only God knows the final `Amen`.

---

## 1. Scope

**v1 goals**
- A `no_std + alloc` core crate implementing the Core IR (§5) and the runtime
  monad (§6), with a `std` feature for hosted development/testing on Linux.
- A parser for the **plain surface form** (§7) — the machine-canonical form.
- An evaluator that runs verses against a fugue-derived environment, resolves
  overloads by *genus*, honors the mode capability mask, and surfaces errors
  as `Dissonance` (never `panic!` in kernel context).
- The **meteor-deflection example** (§14) runs end-to-end in hosted mode and
  produces a `Plan`; committing the `Plan` calls a mock gravity sink.
- Kernel integration seam: a `Platform` trait (§10) and a 6th shell command
  `celebrare` that runs a baked-in or typed-in score, routing `perage` to the
  synth/gravity sink.

**v1 non-goals** (explicitly out; do not gold-plate)
- The **liturgical surface form** (8-line pseudo-Latin with rhyme/meter). Ships
  as sugar over the plain form in a later milestone.
- Real-time **organ-as-input transcription** (parsing a live performance into
  the IR). Later milestone; the audio *render* direction lands first.
- A **static type checker** (rhyme = unification). v1 is dynamically checked;
  the checker is a later milestone with the rule already specified in §7.4.
- A full **monad-transformer tower**. v1 uses one concrete monad + capability
  mask (§6); the real tower is a stretch goal.
- Userspace / ELF loading / syscall ABI. Not needed for v1 (§10).

---

## 2. Target platform & constraints

- **`#![no_std]`, `alloc` only.** The kernel has paging, so it can provide a
  global allocator; if it does not yet, the core must run on a bump/arena
  allocator passed in. Gate `std` behind a feature for hosted tests.
- **x86_64 kernel.** The interpreter is linked into the kernel image (v1) and
  called from the shell task. No dependency on threads; cooperate with the
  existing async executor (long computations must be able to yield — see §6.5).
- **Floating point is a prerequisite and a known gotcha.** Gravity math wants
  `f64`. Before any FP in kernel context: ensure `CR0.EM=0`, `CR0.MP=1`,
  `CR4.OSFXSR=1`, `CR4.OSXMMEXCPT=1`. If the async executor can preempt a task
  mid-computation, `fxsave`/`fxrstor` the XMM state on switch — **or** confine
  all interpreter FP to the shell task and keep it off the interrupt path.
  Abstract numbers behind a `Scalar` trait so hosted mode uses `f64` and kernel
  mode can switch to soft-float if FP setup is deferred. **Call this out in the
  kernel-integration PR; it will bite otherwise.**
- **No panics escape.** All fallible paths return `Result<_, Dissonance>`. Set a
  `#[panic_handler]` if not present; the core itself must be panic-free on all
  well-formed and malformed input (fuzz the parser).
- **AC'97 is not reimplemented.** The existing synth already drives the ICH
  PCM-Out DMA engine (the BDL ring). The interpreter emits note events to that
  synth via the `Organ` trait (§9); it never touches NAMBAR/NABMBAR/BDL itself.

---

## 3. Crate layout & features

```
officium/
├─ Cargo.toml
├─ crates/
│  ├─ officium-core/      # no_std + alloc. IR, runtime monad, evaluator. THE language.
│  │   features = ["std"]     # std: enables hosting, std collections shims, test harness
│  ├─ officium-parse/     # no_std + alloc. Plain-form parser -> Core IR.
│  ├─ officium-audio/     # no_std + alloc. Organ trait + render (IR -> note events).
│  │                      #   Mock impl behind "std"; real impl provided by kernel.
│  └─ officium-host/      # std bin. REPL + file runner for Fedora. Dev/test driver.
└─ integration/
   └─ kernel-shim/        # Reference: how the kernel wires Platform + the `celebrare` cmd.
```

Feature flags:
- `std` — hosting, `std`-backed collections, the host REPL, golden-test harness.
- `audio-mock` (default under `std`) — `Organ`/gravity sink print events.
- `audio-ac97` — provided by the kernel; core does not implement it.

Everything in `officium-core` and `officium-parse` must compile under
`--no-default-features` for a bare `no_std + alloc` target.

---

## 4. Language semantics (normative)

This section is the contract. §5–§9 implement it.

### 4.1 Modes as monads
Each mode admits a fixed set of legal operations (its **capability set**).
Using an operation outside the current mode's capability set is a `Dissonance`
(`WrongMode`). Mode is carried on `Return` and `Bind` nodes and on the verse
header.

| Mode | Legal ops (beyond `pure`/`bind`/`ask`/`arg`/`if`/λ/app) |
|---|---|
| Dorian (Maybe) | `nihil` |
| Hypodorian (MaybeT) | `nihil`, `lift` |
| Phrygian (Either) | `clama`, `recipe` |
| Hypophrygian (ExceptT) | `clama`, `recipe`, `lift` |
| Lydian (State) | `lege`, `pone` |
| Hypolydian (StateT) | `lege`, `pone`, `lift` |
| Mixolydian (IO) | `perage` (commit a `Plan` — **impure**) |
| Hypomixolydian (Plan) | `mitte` (emit a gravity command into the `Plan`) |

`ask`, `arg`, `pure`, `bind`, `if`, λ and application are legal in **all**
modes. `ask` reads the fugue environment (Reader). `arg` binds the incoming
value the verse is a function of.

### 4.2 The fugue = Reader environment
Evaluating a **fugue** produces an `Env` (§5): pedal constants, the subject
definition, `real`/`tonal` answers keyed by *genus*, countersubjects, and the
voice count. A **verse** is evaluated *against* an `Env`. `ask "G"` returns a
pedal constant; `resolve name genus` performs overload resolution:

1. If a **real answer** for `(name, genus)` exists, use it (exact).
2. Else if a **tonal answer** for `(name, genus)` exists, use it (coerced —
   it may inject a countersubject term, e.g. an outgassing term for comets).
3. Else `Dissonance::Unresolved`.

Countersubjects are terms conjoined to whatever the subject computes (e.g. the
`r²` denominator is always present). Model them as functions the evaluator
folds into the resolved subject.

### 4.3 The verse
A verse lowers to a chain of monadic binds ending in a cadence.
- **Cadence to tonic** (`amen e` / `Amen`) = `Return(mode, e)` **and halts the
  verse**, yielding `e`.
- **`nihil`** (Protus) = the Maybe "no correction" short-circuit.
- The **volta** (turn into the final couplet) is where the result value is
  assembled. In the plain form it is just the last statements; in the
  liturgical form (later) it is the boundary between line 6 and line 7.

### 4.4 Turing completeness (mechanisms — must all be reachable)
- λ + application: `Lam`/`App`.
- Recursion: `Fix` (stretto). No structural bound; a fugue may run forever.
- Branching: `If` (+ `nihil`, `clama`).
- Unbounded memory: Tritus `State` (`lege`/`pone`).
- **Provide a self-hosting witness in tests**: encode a small recursive
  function (e.g. factorial or Ackermann-lite) using `Fix` + `State`.

### 4.5 Purity boundary
- A **Plan** is a monoid of gravity commands (§5). Building it is
  Hypomixolydian (`mitte`), pure.
- `perage plan` (Mixolydian) is the **only** operation that reaches the gravity
  sink on `Platform`. It is the sole impure primitive. Evaluator must refuse to
  `perage` a value that is not a fully-forced `Plan` (`Dissonance::Unconsecrated`).
- **Ordering rule (checked at eval, statically later):** a `perage` must be
  dominated by the binds that produced its `Plan`. Committing before the field
  is computed is `Dissonance::Premature`. This is the "sing VII too early →
  Earth is torn" guard, and it is the game's core failure mode; make it loud.

---

## 5. Core IR (reference types)

`officium-core`. Untyped; values are tagged at runtime.

```rust
pub enum Mode {
    Dorian, Hypodorian,       // Protus  — Maybe / MaybeT
    Phrygian, Hypophrygian,   // Deuterus— Either / ExceptT
    Lydian, Hypolydian,       // Tritus  — State / StateT
    Mixolydian, Hypomixolydian// Tetrardus—IO / Plan(Writer)
}
impl Mode {
    pub fn final_(&self) -> Final;        // D/E/F/G
    pub fn is_plagal(&self) -> bool;      // Hypo* => true
    pub fn allows(&self, op: OpKind) -> bool; // capability mask (§4.1)
}

pub type Sym = ...;   // interned string
pub type Genus = ...; // interned string; runtime "kind" of a body (asteroid/comet/...)

pub enum Expr {
    // λ-core
    Var(Sym),
    Lam(Sym, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Lit(Value),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Fix(Box<Expr>),                       // stretto / recursion

    // monadic core (mode-tagged)
    Pure(Mode, Box<Expr>),                // amen / cadence value (does NOT itself halt; see Return)
    Return(Mode, Box<Expr>),              // Amen: cadence to tonic + halt verse
    Bind(Mode, Box<Expr>, Sym, Box<Expr>),// m >>= \x. k   (one verse line)

    // Reader (the fugue)
    Arg,                                  // the value this verse is a function of
    Ask(Sym),                             // Dominus: read a pedal constant / env entry
    Resolve(Sym, Box<Expr>),              // overload resolution by genus (§4.2)

    // per-monad ops (legality enforced by Mode::allows)
    Nihil,                                // Protus: Nothing
    Clama(Box<Expr>),                     // Deuterus: throw
    Recipe(Box<Expr>, Sym, Box<Expr>),    // Deuterus: catch (handler binds the error)
    Lege,                                 // Tritus: get
    Pone(Box<Expr>),                      // Tritus: put
    Mitte(Box<Expr>),                     // Hypomixolydian: append a GravityCmd to the Plan
    Perage(Box<Expr>),                    // Mixolydian: commit a Plan to the world (impure)
    Lift(Box<Expr>),                      // authentic->plagal lift (v1: often identity, see §6)
}

pub enum Value {
    Unit,
    Num(Scalar),                          // Scalar: f64 hosted / soft-float kernel
    Bool(bool),
    Genus(Genus),
    Closure { param: Sym, body: Box<Expr>, env: ValEnv },
    Plan(Plan),                           // Hypomixolydian result
    Field(Field),                         // computed deflection field (pure)
    Record(...),                          // for structured bodies, coordinates, etc.
}

pub struct GravityCmd { pub target: BodyId, pub dv: Vec3, pub at: Micros }
pub struct Plan(pub Vec<GravityCmd>);     // monoid under concatenation
```

Fugue / environment:

```rust
pub struct Fugue {
    pub final_: Final,
    pub pedals: Vec<(Sym, Value)>,        // constants (G = grav. const, ...)
    pub subject: (Sym, Expr),             // main definition
    pub real:  Vec<((Sym, Genus), Expr)>, // exact overloads
    pub tonal: Vec<((Sym, Genus), Expr)>, // coerced overloads
    pub contra: Vec<Expr>,                // countersubjects (always-with terms)
    pub voices: usize,                    // = environment size (sanity metric)
}

pub struct Env { /* compiled from Fugue: pedals + resolver tables + contra folder */ }
impl Env {
    pub fn from_fugue(f: &Fugue, eval: &mut Evaluator) -> Result<Env, Dissonance>;
    pub fn ask(&self, s: &Sym) -> Result<Value, Dissonance>;
    pub fn resolve(&self, name: &Sym, g: &Genus) -> Result<Value/*callable*/, Dissonance>;
}
```

---

## 6. Runtime monad (v1 strategy: one concrete monad + capability mask)

Do **not** build a transformer library for v1. Represent every computation in a
single concrete monad; the `Mode` tag decides which operations are *legal*, not
which monad you are in.

### 6.1 The monad
```rust
// Reader (Env) + State (St) + Writer (Plan) + Either (Dissonance)
pub type St = Value;                       // Tritus state cell (Record for multiple slots)
pub struct Run<A>(/* fn(&Env, St) -> Result<(A, St, Plan), Dissonance> */);
```
- `pure a`  ≙ `\_ st -> Ok((a, st, Plan::empty))`
- `bind m k`≙ thread `st` and concatenate `Plan`s; propagate `Err` (Deuterus /
  `Dissonance`).
- `ask s`   ≙ read from `&Env`.
- `lege/pone` ≙ read/replace `st`.
- `mitte c` ≙ append `c` to the `Plan` component (pure — nothing leaves).
- `nihil`   ≙ a distinguished short-circuit. Represent Maybe as either an
  `Option`-typed `Value` **or** a `Dissonance::Silent` sentinel that
  `bind` propagates and the verse driver turns into "no correction". Prefer the
  sentinel so Protus short-circuit composes like the others.
- `clama e` ≙ `Err(Dissonance::User(e))`; `recipe` ≙ catch that `Err`.
- `perage p`≙ **the only escape**: hand `p` to `Platform::gravity_sink`. In the
  pure `Run`, model this as a request the driver fulfills (§6.4), so `Run`
  itself stays a pure function and testable.

### 6.2 Capability mask
Before evaluating an op node, check `mode.allows(op.kind())`. On violation →
`Dissonance::WrongMode { mode, op }`. This is what makes "you cannot `pone` in a
Dorian verse" true without separate monads.

### 6.3 Lift (authentic → plagal)
In v1 the concrete monad already contains all effects, so `Lift` is a
projection/no-op that merely *changes the mode tag* on the enclosed computation
(widening its capability set from base to plagal). Enforce that `Lift` only
moves base→plagal within the same final. (Stretch goal §12 replaces this with a
real transformer tower where `Lift` is the transformer's `lift`.)

### 6.4 Effects at the edge
`Run` stays pure. The **driver** (`Evaluator::run_verse`) executes a `Run<A>`
against a starting `Env`/`St`, obtains `(A, St, Plan)`, and only *then*, for a
verse whose cadence was a `Perage`, calls `Platform::gravity_sink(plan)`.
Equivalent framing: `perage` returns the `Plan` as the verse's committed
output; the driver performs the single impure act. Keep this seam narrow — it is
the audited boundary of the whole system.

### 6.5 Cooperating with the async executor
Long or non-terminating fugues (planetary defense is *meant* to loop forever)
must yield. Give the evaluator a **step budget**: `run_verse` takes a fuel
count and returns `Poll::Pending` with a resumable continuation when fuel is
exhausted, so the shell task can `await` and the executor stays responsive.
Reference the trampoline in §6.6.

### 6.6 Recursion & non-termination
Implement `Bind`/`App`/`Fix` via an explicit trampoline (heap-allocated
continuation stack) — **no native recursion** in the evaluator (kernel stacks
are small and a stretto can be deep/infinite). `Fix f = f (Fix f)` unrolled
lazily through the trampoline. The step budget (§6.5) is what makes an infinite
fugue coexist with a live system instead of hanging it.

---

## 7. Surface — plain form (v1 canonical)

Line-oriented, ASCII, easy to parse and to bake with `include_str!`. The
liturgical 8-line form is later sugar that lowers to exactly this.

### 7.1 Grammar (EBNF)
```
program   = { item } ;
item      = fugue | versus ;

fugue     = "fuga" ident "in" final "{" { fdecl } "}" ;
fdecl     = "pedale"  ident "=" literal
          | "subiectum" ident "(" ident ")" "=" expr
          | "reale"   ident "@" ident "=" expr        (* exact overload *)
          | "tonale"  ident "@" ident "=" expr        (* coerced overload *)
          | "contra"  expr                            (* countersubject *)
          ;

versus    = "versus" ident "in" final "{" stmts "}" ;
stmts     = { stmt } cadence ;
stmt      = ident "<-" expr        (* bind *)
          | "pone" expr            (* Tritus put; value threaded *)
          | "mitte" expr           (* Hypomixolydian emit *)
          | ifstmt ;
ifstmt    = "si" expr "tunc" "{" stmts "}" "aliter" "{" stmts "}" ;
cadence   = "amen" expr            (* Return: cadence + halt *)
          | "nihil"                (* Protus short-circuit *)
          | "clama" expr           (* Deuterus throw *)
          | "perage" expr ;        (* Mixolydian commit *)

final     = "Re" | "Mi" | "Fa" | "Sol"
          | "re" | "mi" | "fa" | "sol"     (* plagal = lowercase *) ;

expr      = ... (* λ-core: application, ident, literal, `\x -> expr`,
                   `ask "G"`, `arg`, `resolve name genus`, `lege`,
                   builtins: dist, safe, solve, deflect, genus, ... *) ;
```

Convention: **uppercase final = authentic mode, lowercase = plagal.** So
`in Re` = Dorian (Maybe), `in re` = Hypodorian (MaybeT); `in Sol` = Mixolydian
(commit), `in sol` = Hypomixolydian (plan). This single casing rule encodes the
authentic/plagal (monad/transformer, "reaches below") distinction in text.

### 7.2 Builtins (v1 minimal set)
Pure host functions callable from `expr`, sufficient for the example:
`dist(body) -> Num`, `safe(body, g) -> Bool`, `solve(f, r) -> Num` (Δv),
`deflect(dv) -> Field`, `genus(body) -> Genus`, `cmd(target, dv, at) -> GravityCmd`,
`plan_of(field) -> Plan`. Keep them behind a `Builtins` table so the kernel can
swap in real physics later.

### 7.3 Liturgical form (later milestone, specified now)
The 8-line sung form is sugar. Each line = `rhyme_label "text" ";" stmt`. The
parser strips `text` (flavor), reads `rhyme_label` for the type checker (§7.4),
and lowers the `stmt` list + trailing couplet to the plain form. Volta = the
6→7 line boundary. Two rhyming lines must produce unifiable statement types.

### 7.4 Type system (later milestone — rule fixed now)
Rhyme = unification. Assign each verse line a rhyme label. Lines sharing a label
carry the **same type variable**; the checker unifies them. A-lines are typed
`Frame` (coordinate), B-lines `Force`, the final couplet the verse's result
monad `M a`. Meter (syllable count) is an optional extension encoding arity /
numeric precision. Absent the checker (v1), the same mismatches surface at
eval time as `Dissonance::Dissonant` (type clash).

---

## 8. Surface — musical model (render + later transcription)

The IR is transport-agnostic. Define an equivalence between the IR and a
**musical event stream** so audio-in and audio-out share one mapping.

```rust
pub struct Note { pub pitch: Pitch, pub onset: Ticks, pub dur: Ticks, pub voice: u8 }
pub type Events = Vec<Note>;
```

### 8.1 Mode recovery (transcription — later)
- **Final** = the pitch a phrase cadences to (last sustained note of the
  phrase) → the monad family (D/E/F/G).
- **Ambitus** = whether the melody sits **above** the final (authentic) or
  **around/below** it (plagal) → base vs. plagal → capability set. Together
  `(final, ambitus)` recovers the `Mode`. Musically legitimate; this is exactly
  how the modes were historically distinguished.

### 8.2 Degree → operation (within a mode)
Scale degrees relative to the final map to ops via a **fixed, total table**
(the exact table is a tuning knob — keep it in one place, `degree_op.rs`):
- degree 1 (the final) = cadence / `amen`.
- reciting tone (degree 5 authentic / degree 3 plagal) = `ask` (Dominus / the
  pedal reciting note).
- remaining degrees = `bind`, the mode's own op (`nihil`/`clama`/`pone`/`mitte`),
  and `arg`. Document the assignment; totality matters more than the specifics.

### 8.3 Subject & stretto
The fugue **subject** is a fixed pitch-rhythm motif. Recognizing a subject =
matching that motif; a **real answer** = the motif transposed an octave (exact);
a **tonal answer** = the motif with adjusted intervals (coercion). **Stretto** —
the motif re-entering *before* the previous statement finishes — is the
recursion/`Fix` signal.

### 8.4 Render (audio-out — v1 after the evaluator)
Given an IR program (or a fugue/verse), produce `Events` and feed them to the
`Organ` (§9): pedal constants → the pedal voice (sustained), the subject → a
manual voice, verse ops → the degree table. This is how a program is "read back"
/ audited aurally. v1 render is enough to *hear* the meteor example; live
transcription (audio-in) is a separate later milestone.

---

## 9. Audio binding (`officium-audio`)

The synth already exists (AC'97 / 82801AA). Depend only on this trait; never
touch AC'97 registers here.

```rust
pub trait Organ {
    /// Start a voice at a pitch on a division (0 = pedal, 1.. = manuals).
    fn note_on(&mut self, voice: u8, pitch: Pitch, stops: Registration);
    fn note_off(&mut self, voice: u8, pitch: Pitch);
    /// Advance synthesis by dt; the kernel impl pushes PCM into the ICH BDL ring.
    fn tick(&mut self, dt: Micros);
}
```
- `officium-audio` provides `render(program) -> Events` and a driver that walks
  `Events` against an `Organ`.
- Under `std`/`audio-mock`, a mock `Organ` logs events (and optionally writes a
  WAV for ear-checking on Fedora).
- The kernel supplies the real `Organ` impl wrapping its existing synth. **The
  interpreter emits notes; it does not manage NAMBAR/NABMBAR, the BDL, LVI/CIV,
  or DMA.**

---

## 10. Kernel integration & the shell command

### 10.1 Why no userspace yet
v1 runs the interpreter **in-kernel** as a library the shell calls. This avoids
an ELF loader, a syscall ABI, and user/kernel memory separation — all large.
The entire outside-world contact is one trait:

```rust
pub trait Platform {
    fn organ(&mut self) -> &mut dyn Organ;
    fn keyboard(&mut self) -> Option<KeyEvent>;   // from the existing IRQ path
    fn now(&self) -> Micros;
    fn alloc(&self) -> &dyn GlobalAllocLike;       // if not using #[global_allocator]
    /// The ONLY effect escape: apply a committed Plan to the gravity hardware.
    fn gravity_sink(&mut self, plan: &Plan) -> Result<(), Dissonance>;
}
```

When you later build userspace, implement `Platform` **over syscalls** and
relink `officium-core` as a userspace program. Same crate, different transport —
not a rewrite. That is the payoff of routing everything through this trait now.

### 10.2 The 6th shell command
Add `celebrare` to the existing 5-command shell:
```
celebrare <name>        # run a baked-in score (include_str!) by name
celebrare -             # read a score typed on the keyboard until a blank line
```
- Parse → build `Env` from the fugue(s) → `run_verse` with a fuel budget,
  `await`ing across yields on the async executor.
- Route a Mixolydian cadence's `Plan` through `Platform::gravity_sink`.
- Print `Dissonance` diagnostics to the shell; never panic. A `Premature` or
  `Unconsecrated` commit must produce a **prominent** diagnostic (this is the
  "you nearly tore the Earth" moment — make it unmistakable).
- v1 score input: `include_str!`-baked examples + the typed `-` mode. Loading
  scores from a filesystem waits on userspace/VFS.

---

## 11. Errors — `Dissonance`

```rust
pub enum Dissonance {
    WrongMode { mode: Mode, op: OpKind },  // op illegal in this mode (capability mask)
    Unresolved { name: Sym, genus: Genus },// no real/tonal answer
    Silent,                                // Protus "no correction" (may be non-error)
    User(Value),                           // Deuterus `clama`
    Dissonant { want: TyHint, got: TyHint },// runtime type clash (pre-checker)
    Premature,                             // `perage` before its Plan was computed (§4.5)
    Unconsecrated,                         // `perage` of a non-Plan / unforced value
    OutOfFuel(Continuation),               // step budget hit; resumable (not a failure)
    Parse { line: u32, msg: &'static str },
}
```
Distinguish **failures** from **control signals**: `Silent` and `OutOfFuel` are
not errors. Everything else is a genuine `Err` surfaced to the shell.

---

## 12. Milestones (hosted-first; each independently testable)

- **M1 — Core + evaluator (hosted).** IR (§5), `Run` monad (§6), trampoline,
  capability mask, `Dissonance`. Unit tests + monad-law property tests (§13).
  *Exit:* factorial-via-`Fix`+`State` runs; monad laws pass.
- **M2 — Parser (plain form).** §7.1 grammar → IR; builtins §7.2. Fuzz for
  panic-freedom. *Exit:* the meteor score (§14) parses and type-clashes surface
  as `Dissonant`.
- **M3 — End-to-end deflection (hosted).** Run the meteor score to a `Plan`;
  mock `gravity_sink` receives it; `perage` ordering/consecration guards fire on
  deliberately broken scores. *Exit:* §14 produces the expected `Plan`; a
  premature-commit variant is rejected loudly.
- **M4 — Audio render (`Organ` + mock).** §8.4 render; mock organ logs / writes
  WAV. *Exit:* the meteor fugue+verse can be *heard* in hosted mode.
- **M5 — Kernel integration.** `Platform` trait, real `Organ` over the synth,
  `celebrare` command, FP setup (§2), fuel/yield on the async executor. *Exit:*
  `celebrare meteor` runs on hardware and drives the organ; commit reaches the
  (mock or real) gravity sink.
- **M6 — Liturgical surface (sugar).** §7.3 lowering; keeps §14 working through
  the 8-line form.
- **M7 (stretch) — Type checker.** §7.4 rhyme-unification.
- **M8 (stretch) — Transcription (audio-in).** §8.1–8.3 live parsing from the
  keyboard-as-manual.
- **M9 (stretch) — Real transformer tower.** Replace §6's single monad +
  capability mask with genuine `MaybeT/ExceptT/StateT/WriterT`; `Lift` becomes
  the transformer `lift`.

Bias: **M1–M5 is a shippable, runnable language.** Everything after is polish.

---

## 13. Testing

- **Monad laws as property tests (do this — it is both correct and thematic):**
  for the `Run` monad,
  `pure a >>= f  ≡  f a` (left identity),
  `m >>= pure   ≡  m`   (right identity),
  `(m >>= f) >>= g ≡ m >>= (\x -> f x >>= g)` (associativity).
  Frame them as "the liturgy must be lawful." Use `proptest`/`quickcheck` under
  `std`.
- **Golden tests** for the evaluator: small IR programs → expected
  `(Value, St, Plan)` / `Dissonance`.
- **Capability-mask tests:** each mode rejects each out-of-set op with
  `WrongMode`.
- **Purity-guard tests:** premature `perage` → `Premature`; `perage` of a
  non-`Plan` → `Unconsecrated`.
- **TC witness:** recursive function via `Fix`+`State` terminates with fuel and
  yields `OutOfFuel` (resumable) when starved — proving both recursion and the
  cooperative-yield path.
- **Parser fuzz:** malformed scores never panic; always `Dissonance::Parse`.
- **Non-termination:** a `per omnia saecula` fugue (planetary defense loop) runs
  N fuel steps, yields, resumes, and never cadences — asserting the system stays
  responsive under an intentionally infinite program.

---

## 14. Reference program (must run by M3)

Plain form. A three-mode pipeline embodying the purity boundary: **Protus**
computes a field (or `nihil`), **Hypomixolydian** turns it into a `Plan`,
**Mixolydian** commits it — the only line that touches the world.

```
fuga gravitas in Sol {
  pedale     G   = 6.674e-11            ; grav. constant, held in the pedals
  subiectum  trahere(corpus) = \r -> G * mass(corpus) / (r * r)   ; countersubject r² is folded in
  reale      trahere @ asteroides = \r -> G * mass(corpus) / (r * r)
  tonale     trahere @ cometes    = \r -> (G * mass(corpus) / (r * r)) + outgassing(corpus)
  contra     \r -> r * r               ; r² always sounds with the subject
}

versus correctio in Re {               ; Protus = Maybe. Plan a deflection, or nihil.
  corpus <- arg
  g      <- ask "G"
  r      <- pure (dist corpus)
  f      <- resolve "trahere" (genus corpus)   ; fugue picks the overload by genus
  si (safe corpus g) tunc {
    nihil                              ; already safe -> no correction (Protus branch)
  } aliter {
    dv    <- pure (solve f r)
    amen (deflect dv)                  ; cadence: return the pure Field
  }
}

versus consilium in sol {              ; Hypomixolydian = Plan. Pure. Nothing leaves.
  field <- arg
  mitte (cmd (target field) (dv_of field) (at field))
  amen field
}

versus executio in Sol {               ; Mixolydian = commit. The ONLY impure verse.
  plan <- arg
  perage plan                          ; apply to gravity hardware, once, deliberately
}
```

Driver wiring (host or `celebrare`): run `correctio(body)` → if `Field`, run
`consilium(field)` to accumulate a `Plan` → run `executio(plan)` to commit.
Multiple bodies = a stretto over `executio` that, by design, need not ever
cadence while bodies keep arriving: *per omnia saecula saeculorum*.

---

## 15. Open questions / tuning knobs (decide during M1–M2)

1. **`nihil` representation** — `Option` `Value` vs. `Dissonance::Silent`
   sentinel. Recommend the sentinel so Protus composes with the others.
2. **State shape (`St`)** — single cell vs. named-slot `Record`. Recommend a
   `Record` so a verse can hold several state slots without nesting.
3. **Degree → op table (§8.2)** — pick one, centralize it; render and future
   transcription must agree.
4. **`Scalar`** — `f64` hosted; kernel `f64` (after FP setup) vs. soft-float.
   Keep the trait so the decision is deferrable.
5. **Type-checker timing** — M7 as specified, or fold a lightweight check into
   M2. Recommend deferring; dynamic `Dissonant` is enough to build on.
6. **Transformer tower (M9)** — only if the single-monad model starts to leak;
   otherwise the capability mask is the shippable truth.
