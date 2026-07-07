use std::{
    env,
    process::{self, Command},
};

fn main() {
    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.arg("-drive");
    qemu.arg(format!("format=raw,file={}", env!("BIOS_IMAGE")));

    // audio: pc speaker + sound blaster 16 on the same backend.
    // AUDIO_BACKEND overrides the host side, e.g. pipewire, alsa or
    // "wav,path=out.wav" to record instead of play.
    let backend = env::var("AUDIO_BACKEND").unwrap_or_else(|_| "pa".into());
    qemu.arg("-audiodev");
    qemu.arg(format!("{},id=snd", backend));
    qemu.arg("-machine");
    qemu.arg("pcspk-audiodev=snd");
    qemu.arg("-device");
    qemu.arg("sb16,audiodev=snd");

    println!("bios image:{:?}", env!("BIOS_IMAGE"));
    let exit_status = qemu.status().unwrap();
    process::exit(exit_status.code().unwrap_or(-1));
}
