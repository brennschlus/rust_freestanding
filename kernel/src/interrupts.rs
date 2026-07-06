use conquer_once::spin::OnceCell;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

// The IDT must live for the rest of the program ('static), because the CPU
// keeps reading it on every interrupt. A static OnceCell gives us that
// without needing `static mut`.
static IDT: OnceCell<InterruptDescriptorTable> = OnceCell::uninit();

/// Build the IDT and tell the CPU to use it (via the `lidt` instruction).
pub fn init_idt() {
    let idt = IDT.get_or_init(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt
    });
    idt.load();
}

// `x86-interrupt` makes the compiler generate the special entry/exit code
// (saving registers, `iretq`) that the CPU expects from an interrupt handler.
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log::warn!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}
