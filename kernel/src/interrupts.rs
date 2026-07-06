use crate::gdt;
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
        // unsafe: the caller must guarantee the IST index is valid and
        // not used by another exception, otherwise stacks get reused
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt
    });
    idt.load();
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
