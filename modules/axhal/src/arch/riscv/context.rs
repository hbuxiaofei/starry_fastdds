use core::arch::asm;
use memory_addr::VirtAddr;
use riscv::register::sstatus::{self, Sstatus};
include_asm_marcos!();

/// General registers of RISC-V.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct GeneralRegisters {
    pub ra: usize,
    pub sp: usize,
    pub gp: usize, // only valid for user traps
    pub tp: usize, // only valid for user traps
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
}

/// Saved registers when a trap (interrupt or exception) occurs.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct TrapFrame {
    /// All general registers.
    pub regs: GeneralRegisters,
    /// Supervisor Exception Program Counter.
    pub sepc: usize,
    /// Supervisor Status Register.
    pub sstatus: usize,
    /// 浮点数寄存器
    pub fs: [usize; 2],
}

impl TrapFrame {
    pub fn set_user_sp(&mut self, user_sp: usize) {
        self.regs.sp = user_sp;
    }

    /// 用于第一次进入应用程序时的初始化
    pub fn app_init_context(app_entry: usize, user_sp: usize) -> Self {
        let sstatus = sstatus::read();
        // 当前版本的riscv不支持使用set_spp函数，需要手动修改
        // 修改当前的sstatus为User，即是第8位置0
        let mut trap_frame = TrapFrame::default();
        trap_frame.set_user_sp(user_sp);
        trap_frame.sepc = app_entry;
        trap_frame.sstatus =
            unsafe { (*(&sstatus as *const Sstatus as *const usize) & !(1 << 8)) & !(1 << 1) };
        unsafe {
            // a0为参数个数
            // a1存储的是用户栈底，即argv
            trap_frame.regs.a0 = *(user_sp as *const usize);
            trap_frame.regs.a1 = *(user_sp as *const usize).add(1);
        }
        trap_frame
    }

    /// 设置返回值
    pub fn set_ret_code(&mut self, ret_value: usize) {
        self.regs.a0 = ret_value;
    }

    /// 设置TLS
    pub fn set_tls(&mut self, tls_value: usize) {
        self.regs.tp = tls_value;
    }

    /// 获取 sp
    pub fn get_sp(&self) -> usize {
        self.regs.sp
    }

    /// 设置 pc
    pub fn set_pc(&mut self, pc: usize) {
        self.sepc = pc;
    }

    /// 设置 arg0
    pub fn set_arg0(&mut self, arg: usize) {
        self.regs.a0 = arg;
    }

    /// 设置 arg1
    pub fn set_arg1(&mut self, arg: usize) {
        self.regs.a1 = arg;
    }

    /// 设置 arg2
    pub fn set_arg2(&mut self, arg: usize) {
        self.regs.a2 = arg;
    }

    /// 获取 pc
    pub fn get_pc(&self) -> usize {
        self.sepc
    }

    /// 获取 ret
    pub fn get_ret_code(&self) -> usize {
        self.regs.a0
    }

    /// 设置返回地址
    pub fn set_ra(&mut self, ra: usize) {
        self.regs.ra = ra;
    }

    /// 获取所有 syscall 参数
    pub fn get_syscall_args(&self) -> [usize; 6] {
        [
            self.regs.a0,
            self.regs.a1,
            self.regs.a2,
            self.regs.a3,
            self.regs.a4,
            self.regs.a5,
        ]
    }

    /// 获取 syscall id
    pub fn get_syscall_num(&self) -> usize {
        self.regs.a7 as _
    }
}

/// Saved hardware states of a task.
///
/// The context usually includes:
///
/// - Callee-saved registers
/// - Stack pointer register
/// - Thread pointer register (for thread-local storage, currently unsupported)
/// - FP/SIMD registers
///
/// On context switch, current task saves its context from CPU to memory,
/// and the next task restores its context from memory to CPU.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug, Default)]
pub struct TaskContext {
    pub ra: usize, // return address (x1)
    pub sp: usize, // stack pointer (x2)

    pub s0: usize, // x8-x9
    pub s1: usize,

    pub s2: usize, // x18-x27
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,

    pub tp: usize,
    // TODO: FP states
}

impl TaskContext {
    /// Creates a new default context for a new task.
    pub const fn new() -> Self {
        unsafe { core::mem::MaybeUninit::zeroed().assume_init() }
    }

    pub fn new_empty() -> *mut TaskContext {
        let task_ctx = TaskContext::new();
        &task_ctx as *const TaskContext as *mut TaskContext
    }

    /// Initializes the context for a new task, with the given entry point and
    /// kernel stack.
    pub fn init(&mut self, entry: usize, kstack_top: VirtAddr, tls_area: VirtAddr) {
        self.sp = kstack_top.as_usize();
        self.ra = entry;
        self.tp = tls_area.as_usize();
    }

    /// Switches to another task.
    ///
    /// It first saves the current task's context from CPU to this place, and then
    /// restores the next task's context from `next_ctx` to CPU.
    pub fn switch_to(&mut self, next_ctx: &Self) {
        #[cfg(feature = "tls")]
        {
            self.tp = super::read_thread_pointer();
            unsafe { super::write_thread_pointer(next_ctx.tp) };
        }
        unsafe {
            // TODO: switch FP states
            context_switch(self, next_ctx)
        }
    }
}

#[naked]
unsafe extern "C" fn context_switch(_current_task: &mut TaskContext, _next_task: &TaskContext) {
    asm!(
        "
        // save old context (callee-saved registers)
        STR     ra, a0, 0
        STR     sp, a0, 1
        STR     s0, a0, 2
        STR     s1, a0, 3
        STR     s2, a0, 4
        STR     s3, a0, 5
        STR     s4, a0, 6
        STR     s5, a0, 7
        STR     s6, a0, 8
        STR     s7, a0, 9
        STR     s8, a0, 10
        STR     s9, a0, 11
        STR     s10, a0, 12
        STR     s11, a0, 13

        // restore new context
        LDR     s11, a1, 13
        LDR     s10, a1, 12
        LDR     s9, a1, 11
        LDR     s8, a1, 10
        LDR     s7, a1, 9
        LDR     s6, a1, 8
        LDR     s5, a1, 7
        LDR     s4, a1, 6
        LDR     s3, a1, 5
        LDR     s2, a1, 4
        LDR     s1, a1, 3
        LDR     s0, a1, 2
        LDR     sp, a1, 1
        LDR     ra, a1, 0

        ret",
        options(noreturn),
    )
}
