use conquer_once::spin::OnceCell;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

// Which entry of the Interrupt Stack Table the double fault handler uses.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: OnceCell<TaskStateSegment> = OnceCell::uninit();
static GDT: OnceCell<(GlobalDescriptorTable, Selectors)> = OnceCell::uninit();

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

/// Load our own GDT with a TSS. The TSS holds a separate stack for the
/// double fault handler: a double fault is often caused by a stack
/// overflow, and without a known-good stack the CPU could not even call
/// the handler (-> triple fault -> reboot).
pub fn init() {
    let tss = TSS.get_or_init(|| {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            // no guard page: overflowing this stack corrupts the bytes
            // below it, but for now that is good enough
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            // stacks grow downwards, so pass the end address
            stack_start + STACK_SIZE as u64
        };
        tss
    });

    let (gdt, selectors) = GDT.get_or_init(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss));
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                tss_selector,
            },
        )
    });

    gdt.load();
    // the new GDT is only in effect once the segment registers are
    // reloaded; the old ones still point into the bootloader's GDT.
    // SS matters even in long mode: iretq validates the saved SS
    // against the current GDT when returning from an interrupt.
    unsafe {
        CS::set_reg(selectors.code_selector);
        SS::set_reg(selectors.data_selector);
        DS::set_reg(selectors.data_selector);
        ES::set_reg(selectors.data_selector);
        load_tss(selectors.tss_selector);
    }
}
