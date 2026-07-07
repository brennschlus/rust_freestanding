//! Hosted driver: parse a score, celebrate the office, print what the
//! gravity sink receives. Dev/test mirror of the kernel's `celebrare`.
//!
//! Usage:
//!   officium-host [score.off] [--fuel N] [--render]
//!
//! With no score argument the baked-in meteor score (§14) is used.

use officium_core::builtins::make_corpus;
use officium_core::machine::{empty_state, resume, run_verse};
use officium_core::types::{Dissonance, Mode, Plan, Value};
use officium_core::{Env, Platform, Program, Versus};
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");

struct HostPlatform {
    committed: Vec<Plan>,
}

impl Platform for HostPlatform {
    fn gravity_sink(&mut self, plan: &Plan) -> Result<(), Dissonance> {
        for cmd in &plan.0 {
            println!(
                "GRAVITAS  target={}  dv=({:+.6e}, {:+.6e}, {:+.6e})  at={}us",
                cmd.target, cmd.dv.x, cmd.dv.y, cmd.dv.z, cmd.at
            );
        }
        self.committed.push(plan.clone());
        Ok(())
    }
    fn now(&self) -> u64 {
        0
    }
    fn say(&mut self, line: &str) {
        println!("{line}");
    }
}

/// Run one verse to completion, resuming across fuel slices (the same
/// loop the kernel runs, minus the await).
fn run_full(
    env: &Env,
    v: &Versus,
    arg: Value,
    fuel_slice: u64,
) -> Result<officium_core::VerseOutcome, Dissonance> {
    let mut r = run_verse(env, v.mode, &v.body, arg, empty_state(), fuel_slice);
    loop {
        match r {
            Err(Dissonance::OutOfFuel(cont)) => r = resume(env, *cont, fuel_slice),
            other => return other,
        }
    }
}

fn loud(d: &Dissonance) {
    match d {
        Dissonance::Premature => {
            eprintln!("!!! DISSONANTIA: PREMATURE COMMIT — the field is not yet computed.");
            eprintln!("!!! Singing the seventh mode too early would tear the Earth apart.");
        }
        Dissonance::Unconsecrated => {
            eprintln!("!!! DISSONANTIA: UNCONSECRATED COMMIT — that value is no Plan.");
        }
        other => eprintln!("dissonantia: {other:?}"),
    }
}

/// The §14 wiring: correctio(body) -> consilium(field) -> executio(plan).
fn celebrate_body(
    platform: &mut HostPlatform,
    env: &Env,
    prog: &Program,
    label: &str,
    body: Value,
    fuel: u64,
) {
    println!("--- corpus: {label}");
    let correctio = match prog.verse("correctio") {
        Some(v) => v,
        None => {
            println!("no verse 'correctio'; nothing to sing");
            return;
        }
    };
    let field = match run_full(env, correctio, body, fuel) {
        Ok(out) => match out.value {
            Some(Value::Field(f)) => Value::Field(f),
            other => {
                println!("correctio cadenced with {other:?}; no deflection to plan");
                return;
            }
        },
        Err(Dissonance::Silent) => {
            println!("nihil — the body is already safe; no correction");
            return;
        }
        Err(d) => return loud(&d),
    };

    let consilium = match prog.verse("consilium") {
        Some(v) => v,
        None => return,
    };
    let planned = match run_full(env, consilium, field, fuel) {
        Ok(out) => out.plan,
        Err(d) => return loud(&d),
    };
    println!("consilium accumulated a Plan of {} command(s) — still pure", planned.0.len());

    let executio = match prog.verse("executio") {
        Some(v) => v,
        None => return,
    };
    match run_full(env, executio, Value::Plan(planned), fuel) {
        Ok(out) => {
            if let Some(plan) = out.committed {
                // the driver performs the single impure act (§6.4)
                if let Err(d) = platform.gravity_sink(&plan) {
                    loud(&d);
                }
            } else {
                println!("executio cadenced without committing");
            }
        }
        Err(d) => loud(&d),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut fuel: u64 = 100_000;
    let mut render = false;
    let mut path: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fuel" => fuel = it.next().and_then(|s| s.parse().ok()).unwrap_or(fuel),
            "--render" => render = true,
            other => path = Some(other.to_string()),
        }
    }

    let src = match &path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {p}: {e}");
                std::process::exit(1);
            }
        },
        None => METEOR.to_string(),
    };

    let prog = match parse(&src) {
        Ok(p) => p,
        Err(d) => {
            loud(&d);
            std::process::exit(1);
        }
    };
    let env = Env::from_fugues(&prog.fugues);
    println!(
        "score: {} fugue(s), {} verse(s), {} voice(s) in the environment",
        prog.fugues.len(),
        prog.verses.len(),
        env.voices()
    );

    if render {
        let events = officium_audio::render_program(&prog);
        println!("render: {} note event(s)", events.len());
        let mut organ = officium_audio::mock_organ();
        officium_audio::play(&events, &mut organ);
        for line in &organ.log {
            println!("  {line}");
        }
    }

    let mut platform = HostPlatform { committed: Vec::new() };

    // three bodies: an unsafe asteroid, an unsafe comet (tonal answer
    // with the outgassing countersubject), and a safe pebble (nihil)
    let bodies = [
        ("asteroides periculosus", make_corpus(1, "asteroides", 1e20, 1e5)),
        ("cometes periculosus", make_corpus(2, "cometes", 5e19, 2e5)),
        ("lapillus tutus", make_corpus(3, "asteroides", 1.0, 1e9)),
    ];
    for (label, body) in bodies {
        celebrate_body(&mut platform, &env, &prog, label, body, fuel);
    }
    println!(
        "officium consummatum: {} plan(s) reached the gravity sink",
        platform.committed.len()
    );
    let _ = Mode::Dorian; // keep the import honest under cfg changes
}
