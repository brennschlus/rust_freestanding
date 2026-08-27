//! Userspace (Ring 3 + syscalls). A layman's process runs outside the
//! monastery walls and reaches the world only through the gate: the
//! `int 0x80` syscall. The three syscalls are exactly the three methods
//! of Officium's `Platform` trait — `say`, `now`, `gravity_sink` — so
//! the audited effect boundary that the interpreter was designed around
//! becomes the ring boundary. `perage` from ring 3 lands in the very
//! same gravity sink the kernel uses in ring 0.
//!
//! The transition is a hand-written trampoline: `enter_user` builds an
//! `iretq` frame (user SS/RSP/RFLAGS/CS/RIP) and drops to ring 3, saving
//! the kernel RSP; a `SYS_EXIT` syscall reloads it and returns as if
//! `enter_user` had simply returned. The payload is position-independent
//! machine code copied into a `USER_ACCESSIBLE` page.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::println;

// Syscall numbers = the Platform trait, in order.
const SYS_SAY: u64 = 0;
const SYS_NOW: u64 = 1;
const SYS_COMMIT: u64 = 2;
const SYS_EXIT: u64 = 3;

// Where the layman's process lives in its (user-accessible) address
// space: one page of code, one page of stack, a guard gap between.
const USER_CODE: u64 = 0x4000_0000;
const USER_STACK: u64 = 0x4000_3000;
const USER_STACK_TOP: u64 = 0x4000_4000;

/// The saved general-purpose registers, in the order the syscall
/// trampoline pushes them (`rax` last, so lowest address). `rax` holds
/// the syscall number on entry and the return value on exit.
#[repr(C)]
struct Regs {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

unsafe extern "C" {
    fn enter_user(entry: u64, stack_top: u64, ucode: u64, udata: u64);
    fn return_to_kernel() -> !;
    fn syscall_entry();
    fn user_payload_start();
    fn user_payload_end();
}

/// Address of the `int 0x80` trampoline, for the IDT (installed with
/// DPL 3 so ring 3 may invoke it).
pub fn syscall_handler_addr() -> u64 {
    syscall_entry as *const () as u64
}

/// The kernel side of one `int 0x80`. Runs on the RSP0 stack with the
/// caller's registers spilled into `regs`; the result goes back in
/// `regs.rax`.
#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch(regs: &mut Regs) {
    match regs.rax {
        SYS_SAY => {
            let line = read_user_cstr(regs.rdi);
            println!("{}", line);
            regs.rax = 0;
        }
        SYS_NOW => {
            // microseconds since boot — the same clock Platform::now uses
            regs.rax = crate::interrupts::ticks().wrapping_mul(55_000);
        }
        SYS_COMMIT => {
            // the single impure act, performed here at the audited seam:
            // dv arrives scaled by 1000 (userspace has no float ABI need)
            let dv = regs.rsi as i64 as f64 / 1000.0;
            crate::celebrare::gravity_sink_line(regs.rdi, dv, regs.rdx);
            regs.rax = 0;
        }
        SYS_EXIT => unsafe { return_to_kernel() },
        _ => regs.rax = u64::MAX,
    }
}

/// Read a NUL-terminated string from a user pointer (bounded). The
/// kernel may read user pages directly (SMAP is off); volatile keeps
/// the compiler from assuming anything about foreign memory.
fn read_user_cstr(ptr: u64) -> alloc::string::String {
    let mut bytes = alloc::vec::Vec::new();
    let p = ptr as *const u8;
    for i in 0..256 {
        let b = unsafe { core::ptr::read_volatile(p.add(i)) };
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    alloc::string::String::from_utf8_lossy(&bytes).into_owned()
}

static MAPPED: AtomicBool = AtomicBool::new(false);

/// Lay down the user code and stack pages (once).
fn ensure_mapped() -> Result<(), &'static str> {
    if MAPPED.load(Ordering::Relaxed) {
        return Ok(());
    }
    crate::memory::map_user_page(USER_CODE, true)?;
    crate::memory::map_user_page(USER_STACK, true)?;
    MAPPED.store(true, Ordering::Relaxed);
    Ok(())
}

/// Run the layman's liturgy in ring 3: map its pages, copy the payload
/// into user memory, and drop privilege. Returns when the process calls
/// `SYS_EXIT`.
pub fn run() {
    if let Err(e) = ensure_mapped() {
        println!("userspace: {}", e);
        return;
    }

    // copy the position-independent payload into the user code page
    let (start, end) = (
        user_payload_start as *const () as usize,
        user_payload_end as *const () as usize,
    );
    unsafe {
        core::ptr::copy_nonoverlapping(start as *const u8, USER_CODE as *mut u8, end - start);
    }

    let (ucode, udata) = crate::gdt::user_selectors();
    println!("mundanus: descending to ring 3 (CPL 3) ...");
    unsafe {
        enter_user(USER_CODE, USER_STACK_TOP, ucode as u64, udata as u64);
    }
    println!("mundanus: rediit ad monasterium (back in ring 0)");
}

// --- the trampolines ---
//
// syscall_entry: spill the GP registers, call the dispatcher, restore,
// and `iretq` back to ring 3. On SYS_EXIT the dispatcher never returns
// here — it jumps through return_to_kernel instead.
//
// enter_user: save the kernel context and `iretq` down to ring 3.
// return_to_kernel: reload that context and `ret`, so from the caller's
// point of view enter_user simply returned.
core::arch::global_asm!(
    r#"
.globl syscall_entry
.globl enter_user
.globl return_to_kernel

syscall_entry:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov  rdi, rsp
    call syscall_dispatch
    pop  rax
    pop  rbx
    pop  rcx
    pop  rdx
    pop  rsi
    pop  rdi
    pop  rbp
    pop  r8
    pop  r9
    pop  r10
    pop  r11
    pop  r12
    pop  r13
    pop  r14
    pop  r15
    iretq

enter_user:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov  [rip + KERNEL_RSP], rsp
    push rcx                 // SS  = user data | 3
    push rsi                 // RSP = user stack top
    push 0x202               // RFLAGS (IF set, IOPL 0)
    push rdx                 // CS  = user code | 3
    push rdi                 // RIP = user entry
    iretq

return_to_kernel:
    mov  rsp, [rip + KERNEL_RSP]
    pop  r15
    pop  r14
    pop  r13
    pop  r12
    pop  rbx
    pop  rbp
    ret

.section .data
.p2align 3
KERNEL_RSP:
    .quad 0
.section .text
"#
);

// The layman's program: position-independent, self-contained. It says a
// line, asks the time, commits one gravity command, and exits — each an
// `int 0x80`. Copied into USER_CODE before it runs, so the rip-relative
// string load resolves wherever it lands.
core::arch::global_asm!(
    r#"
.globl user_payload_start
.globl user_payload_end
.p2align 4
user_payload_start:
    lea     rdi, [rip + user_payload_msg]
    xor     eax, eax            // SYS_SAY
    int     0x80
    mov     eax, 1              // SYS_NOW
    int     0x80
    mov     edi, 1              // target body
    mov     esi, 334            // dv * 1000  (~0.334)
    xor     edx, edx            // at = 0
    mov     eax, 2              // SYS_COMMIT
    int     0x80
    mov     eax, 3              // SYS_EXIT
    int     0x80
1:  jmp     1b
.p2align 3
user_payload_msg:
    .asciz "mundanus extra muros: officium per portam celebro"
user_payload_end:
"#
);
