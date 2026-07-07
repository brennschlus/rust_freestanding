use std::{
    env,
    process::{self, Command},
};

fn main() {
    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.arg("-drive");
    qemu.arg(format!("format=raw,file={}", env!("BIOS_IMAGE")));

    // audio: pc speaker + AC97 on the same backend. AUDIO_BACKEND
    // overrides the host side, e.g. alsa or "wav,path=out.wav" to
    // record instead of play. AC97 and not sb16: qemu's sb16 sits
    // behind the emulated ISA DMA controller whose transfer loop spins
    // the qemu main loop at 100% whenever the host audio buffer is
    // full, freezing the display.
    let backend = env::var("AUDIO_BACKEND").unwrap_or_else(|_| "pipewire".into());
    qemu.arg("-audiodev");
    qemu.arg(format!("{},id=snd", backend));
    qemu.arg("-machine");
    qemu.arg("pcspk-audiodev=snd");
    qemu.arg("-device");
    qemu.arg("AC97,audiodev=snd");

    println!("bios image:{:?}", env!("BIOS_IMAGE"));
    let exit_status = qemu.status().unwrap();
    process::exit(exit_status.code().unwrap_or(-1));
}
