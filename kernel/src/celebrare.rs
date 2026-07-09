//! `celebrare` — the Officium interpreter wired into the kernel (§10).
//!
//! The interpreter is a `no_std` library the shell calls; the entire
//! contact with the outside world goes through the `Platform` trait
//! and the `Organ` trait. FP note: the kernel target is soft-float
//! (`x86_64-unknown-none`), so the interpreter's f64 math lowers to
//! compiler builtins — no FPU/XMM state exists to save or restore.

use alloc::string::String;

use officium_core::builtins::make_corpus;
use officium_core::machine::{empty_state, resume, run_verse};
use officium_core::types::{Dissonance, Plan, Value};
use officium_core::{Env, Platform, Program, VerseOutcome, Versus};

use crate::task::timer;
use crate::task::yield_now;
use crate::{ac97, interrupts, println};

const METEOR: &str = include_str!("../../officium/scores/meteor.off");
const CANTUS: &str = include_str!("../../officium/scores/meteor_cantus.off");

/// How many trampoline steps a verse may take between yields.
const FUEL_SLICE: u64 = 20_000;
/// Total budget per verse: a fugue that never cadences gives up here
/// instead of holding the shell hostage (per omnia saecula is real).
const MAX_SLICES: u32 = 500;

struct KernelPlatform;

impl Platform for KernelPlatform {
    fn gravity_sink(&mut self, plan: &Plan) -> Result<(), Dissonance> {
        // v1 gravity hardware: the console. Swapping in a real sink
        // keeps this the single audited effect boundary.
        for cmd in &plan.0 {
            println!(
                "GRAVITAS  target={}  dv.x={}  at={}us",
                cmd.target, cmd.dv.x, cmd.at
            );
        }
        Ok(())
    }

    fn now(&self) -> u64 {
        interrupts::ticks() * 55_000
    }

    fn say(&mut self, line: &str) {
        println!("{}", line);
    }
}

/// The AC'97 synth behind the `Organ` trait (§9): the interpreter emits
/// notes; the BDL ring and DMA stay the synth driver's business.
struct KernelOrgan;

impl officium_audio::Organ for KernelOrgan {
    fn note_on(&mut self, _voice: u8, pitch: u8, _stops: u8) {
        ac97::note_on(officium_audio::pitch_hz(pitch));
    }
    fn note_off(&mut self, _voice: u8, pitch: u8) {
        ac97::note_off(officium_audio::pitch_hz(pitch));
    }
    fn tick(&mut self, _dt: u64) {
        // the synth runs off its own DMA ring; time passes by awaiting
    }
}

/// Entry point for the shell command.
pub async fn celebrare(source_name: &str, typed: Option<String>) {
    let src: &str = match (source_name, &typed) {
        ("-", Some(s)) => s.as_str(),
        ("meteor", _) => METEOR,
        ("cantus", _) => CANTUS,
        _ => {
            println!(
                "unknown score '{}'; try: celebrare meteor | celebrare cantus | celebrare -",
                source_name
            );
            return;
        }
    };

    let prog = match officium_parse::parse(src) {
        Ok(p) => p,
        Err(d) => return loud(&d),
    };
    // M7: the rhyme must unify — a discordant score is refused before
    // a single note sounds
    if let Err(d) = officium_core::check_program(&prog) {
        return loud(&d);
    }
    let env = Env::from_fugues(&prog.fugues);
    println!(
        "officium: {} fugue(s), {} verse(s), {} voice(s)",
        prog.fugues.len(),
        prog.verses.len(),
        env.voices()
    );

    // sung verses (§7.3) show their poem: rhyme label + text per line
    for v in &prog.verses {
        if !v.rhymes.is_empty() {
            println!("versus {}:", v.name);
            for (label, text) in &v.rhymes {
                println!("  {}  {}", label, text);
            }
        }
    }

    // the office is read back aurally first: the organ holds the fugue
    if ac97::is_available() {
        let events = officium_audio::render_program(&prog);
        play(&events).await;
    }

    let mut platform = KernelPlatform;
    if prog.verse("correctio").is_some() {
        // the §14 wiring, over a small procession of bodies
        let bodies = [
            ("asteroides periculosus", make_corpus(1, "asteroides", 1e20, 1e5)),
            ("cometes periculosus", make_corpus(2, "cometes", 5e19, 2e5)),
            ("lapillus tutus", make_corpus(3, "asteroides", 1.0, 1e9)),
        ];
        for (label, body) in bodies {
            println!("--- corpus: {}", label);
            celebrate_body(&mut platform, &env, &prog, body).await;
        }
    } else if let Some(v) = prog.verses.first() {
        // no meteor wiring: a verse is a function of its input — feed
        // it zero (a num suits transcribed skeletons; unit suits none)
        match run_full(&env, v, Value::Num(0.0)).await {
            Ok(out) => println!("{} cadenced: {:?}", v.name, out.value),
            Err(d) => loud(&d),
        }
    } else {
        println!("the score has no verses; only the fugue sounds");
    }
}

async fn celebrate_body(platform: &mut KernelPlatform, env: &Env, prog: &Program, body: Value) {
    let correctio = prog.verse("correctio").unwrap();
    let field = match run_full(env, correctio, body).await {
        Ok(out) => match out.value {
            Some(v @ Value::Field(_)) => v,
            other => return println!("correctio cadenced with {:?}", other),
        },
        Err(Dissonance::Silent) => {
            return println!("nihil — the body is already safe; no correction")
        }
        Err(d) => return loud(&d),
    };

    let Some(consilium) = prog.verse("consilium") else { return };
    let planned = match run_full(env, consilium, field).await {
        Ok(out) => out.plan,
        Err(d) => return loud(&d),
    };
    println!("consilium: a Plan of {} command(s), still pure", planned.0.len());

    let Some(executio) = prog.verse("executio") else { return };
    match run_full(env, executio, Value::Plan(planned)).await {
        Ok(out) => {
            if let Some(plan) = out.committed {
                // the single impure act, performed by the driver (§6.4)
                if let Err(d) = platform.gravity_sink(&plan) {
                    loud(&d);
                }
            }
        }
        Err(d) => loud(&d),
    }
}

/// Run one verse, yielding to the executor between fuel slices so the
/// system stays live under a non-terminating fugue.
async fn run_full(env: &Env, v: &Versus, arg: Value) -> Result<VerseOutcome, Dissonance> {
    let mut r = run_verse(env, v.mode, &v.body, arg, empty_state(), FUEL_SLICE);
    let mut slices = 0;
    loop {
        match r {
            Err(Dissonance::OutOfFuel(cont)) => {
                slices += 1;
                if slices >= MAX_SLICES {
                    println!(
                        "versus {} numquam cadenzat ({} steps) — abandoned, system intact",
                        v.name,
                        FUEL_SLICE * MAX_SLICES as u64
                    );
                    return Err(Dissonance::OutOfFuel(cont));
                }
                yield_now().await;
                r = resume(env, *cont, FUEL_SLICE);
            }
            other => return other,
        }
    }
}

/// Walk the rendered events against the organ in real time.
async fn play(events: &officium_audio::Events) {
    use officium_audio::Organ;
    let mut organ = KernelOrgan;
    let Some(end) = events.iter().map(|n| n.onset + n.dur).max() else {
        return;
    };
    let tick_ms = (officium_audio::TICK_US / 1000) as u64;
    for t in 0..end {
        for n in events.iter().filter(|n| n.onset == t) {
            organ.note_on(n.voice, n.pitch, 0);
        }
        timer::sleep_ms(tick_ms).await;
        for n in events.iter().filter(|n| n.onset + n.dur == t + 1) {
            organ.note_off(n.voice, n.pitch);
        }
    }
    ac97::all_notes_off();
}

fn loud(d: &Dissonance) {
    match d {
        Dissonance::Premature => {
            println!("!!! DISSONANTIA: PREMATURE COMMIT — the field is not yet computed.");
            println!("!!! Singing mode VII too early would tear the Earth apart. Refused.");
        }
        Dissonance::Unconsecrated => {
            println!("!!! DISSONANTIA: UNCONSECRATED COMMIT — that value is no Plan. Refused.");
        }
        Dissonance::Parse { line, msg } => {
            println!("dissonantia (parse, line {}): {}", line, msg);
        }
        Dissonance::Discors { versus, msg } => {
            println!("!!! DISSONANTIA DISCORS in versus {}: {}", versus, msg);
            println!("!!! The rhyme does not unify; the score is refused unsung.");
        }
        other => println!("dissonantia: {:?}", other),
    }
}
