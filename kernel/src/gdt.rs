use conquer_once::spin::OnceCell;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::{PrivilegeLevel, VirtAddr};

// Which entry of the Interrupt Stack Table the double fault handler uses.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: OnceCell<TaskStateSegment> = OnceCell::uninit();
static GDT: OnceCell<(GlobalDescriptorTable, Selectors)> = OnceCell::uninit();

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
    /// Ring 3 code/data (§userspace): the selectors an `iretq` into the
    /// layman's process loads, with RPL already set to 3.
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
}

/// The ring-3 (code, data) selectors, RPL 3, for building an `iretq`
/// frame into userspace. Panics if the GDT is not initialized yet.
pub fn user_selectors() -> (u16, u16) {
    let (_, s) = GDT.get().expect("gdt not initialized");
    (s.user_code_selector.0, s.user_data_selector.0)
}

/// Load our own GDT with a TSS. The TSS holds a separate stack for the
/// double fault handler: a double fault is often caused by a stack
/// overflow, and without a known-good stack the CPU could not even call
/// the handler (-> triple fault -> reboot). It also holds the privilege
/// stack (RSP0) the CPU switches to when an interrupt or `int 0x80`
/// arrives from ring 3.
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
        // RSP0: the kernel stack a trap from ring 3 lands on. Must be
        // distinct from the task stack we `iretq` out of, or a timer
        // interrupt during userspace would clobber it. Kept 16-aligned
        // so the C ABI holds through the syscall trampoline.
        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut KSTACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let start = VirtAddr::from_ptr(&raw const KSTACK);
            (start + STACK_SIZE as u64).align_down(16u64)
        };
        tss
    });

    let (gdt, selectors) = GDT.get_or_init(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss));
        // user descriptors carry DPL 3; force RPL 3 on the selectors so
        // they are ready to drop straight into an iretq frame
        let uc = gdt.append(Descriptor::user_code_segment());
        let ud = gdt.append(Descriptor::user_data_segment());
        let user_code_selector = SegmentSelector::new(uc.index(), PrivilegeLevel::Ring3);
        let user_data_selector = SegmentSelector::new(ud.index(), PrivilegeLevel::Ring3);
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                tss_selector,
                user_code_selector,
                user_data_selector,
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
