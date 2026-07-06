use crate::gdt;
use conquer_once::spin::OnceCell;
use pic8259::ChainedPics;
use spinning_top::Spinlock;
use x86_64::instructions::port::Port;
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
        idt
    });
    idt.load();
}

/// Initialize the PICs and let the CPU start receiving hardware
/// interrupts (`sti`). The IDT must be loaded before this is called.
pub fn init_pics() {
    unsafe { PICS.lock().initialize() };
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

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // nothing to do on ticks yet, but the PIC still needs an EOI,
    // otherwise it will never send the next interrupt
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // the PS/2 controller won't fire the next interrupt until the
    // current scancode is read from its data port
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // decoding happens in the async keyboard task; the handler only
    // queues the raw scancode and returns as fast as possible
    crate::task::keyboard::add_scancode(scancode);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}
