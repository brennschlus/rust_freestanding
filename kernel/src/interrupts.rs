use crate::gdt;
use conquer_once::spin::OnceCell;
use pic8259::ChainedPics;
use spinning_top::Spinlock;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// remap the PICs: their default vectors 0-15 collide with CPU exceptions,
// so move them to 32-47 (right after the 32 exception slots)
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

static PICS: Spinlock<ChainedPics> =
    Spinlock::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET, // IRQ 0
    Keyboard,             // IRQ 1
}

/// IRQ line of the PCI sound card, set before the PICs are unmasked.
/// The firmware routes PCI interrupts to one of the free ISA lines; we
/// pre-register handlers for the plausible ones (9, 10, 11) and unmask
/// only the line the card actually got.
static PCI_SOUND_IRQ: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

pub fn register_pci_sound_irq(line: u8) -> Result<(), &'static str> {
    if !(9..=11).contains(&line) {
        return Err("PCI IRQ line outside the expected 9-11 range");
    }
    PCI_SOUND_IRQ.store(line, core::sync::atomic::Ordering::Relaxed);
    Ok(())
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
}

// The IDT must live for the rest of the program ('static), because the CPU
// keeps reading it on every interrupt. A static OnceCell gives us that
// without needing `static mut`.
static IDT: OnceCell<InterruptDescriptorTable> = OnceCell::uninit();

/// Build the IDT and tell the CPU to use it (via the `lidt` instruction).
pub fn init_idt() {
    let idt = IDT.get_or_init(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        // unsafe: the caller must guarantee the IST index is valid and
        // not used by another exception, otherwise stacks get reused
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        idt[PIC_1_OFFSET + 9].set_handler_fn(pci_sound_irq9_handler);
        idt[PIC_1_OFFSET + 10].set_handler_fn(pci_sound_irq10_handler);
        idt[PIC_1_OFFSET + 11].set_handler_fn(pci_sound_irq11_handler);
        idt
    });
    idt.load();
}

/// Initialize the PICs and let the CPU start receiving hardware
/// interrupts (`sti`). The IDT must be loaded before this is called.
pub fn init_pics() {
    // deterministic masks: only IRQ 0 (timer), 1 (keyboard), 2 (cascade)
    // and, if a card was found, the PCI sound line; the rest stays masked
    let mut mask = !0b0000_0111u16;
    let sound_irq = PCI_SOUND_IRQ.load(core::sync::atomic::Ordering::Relaxed);
    if sound_irq != 0 {
        mask &= !(1 << sound_irq);
    }
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        pics.write_masks(mask as u8, (mask >> 8) as u8);
    }
    x86_64::instructions::interrupts::enable();
}

// `x86-interrupt` makes the compiler generate the special entry/exit code
// (saving registers, `iretq`) that the CPU expects from an interrupt handler.
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log::warn!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

// diverging (`-> !`): x86-64 does not allow returning from a double fault
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64, // always 0 for double faults
) -> ! {
    log::error!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
    loop {}
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    log::error!("EXCEPTION: PAGE FAULT");
    // the CPU stores the accessed address in the CR2 register
    log::error!("Accessed Address: {:?}", Cr2::read());
    log::error!("Error Code: {:?}", error_code);
    log::error!("{:#?}", stack_frame);

    // we cannot fix the mapping yet, and returning would just retry the
    // faulting instruction and fault again, so halt instead
    loop {
        x86_64::instructions::hlt();
    }
}

static TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Timer ticks since the PICs were initialized (PIT default: ~18.2 Hz,
/// i.e. one tick every ~55 ms).
pub fn ticks() -> u64 {
    TICKS.load(core::sync::atomic::Ordering::Relaxed)
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::task::timer::on_tick();

    // safety net: if a keyboard interrupt edge got lost (the i8042 line
    // stuck high), pick up the stranded bytes by polling
    crate::task::keyboard::drain_controller();

    // the PIC needs an EOI, otherwise it will never send the next interrupt
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

// PCI interrupts are level-triggered: the handler must make the card
// drop its line (ack in ac97::handle_irq) before the EOI, or the same
// interrupt fires again immediately.
fn pci_sound_irq(line: u8) {
    crate::ac97::handle_irq();

    unsafe {
        PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET + line);
    }
}

extern "x86-interrupt" fn pci_sound_irq9_handler(_stack_frame: InterruptStackFrame) {
    pci_sound_irq(9);
}

extern "x86-interrupt" fn pci_sound_irq10_handler(_stack_frame: InterruptStackFrame) {
    pci_sound_irq(10);
}

extern "x86-interrupt" fn pci_sound_irq11_handler(_stack_frame: InterruptStackFrame) {
    pci_sound_irq(11);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // decoding happens in the async keyboard task; the handler only
    // drains the controller into the scancode queue
    crate::task::keyboard::drain_controller();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}
