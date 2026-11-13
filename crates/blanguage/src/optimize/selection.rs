//! 只对可安全提前求值的计算／当前元素读取消除分支；外部访问与协作保留控制流。
use super::*;

#[repr(u32)]
enum SelectionKind { Pure, LocalRead, LocalWrite, DataRead, Arithmetic, Ordered }

#[repr(C)]
struct SelectionNode {
    first: usize,
    second: usize,
    kind: SelectionKind,
}

#[repr(C)]
struct SelectionValue {
    slot: usize,
    destination: Reg,
    yes: Reg,
    no: Reg,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct SelectionSlot {
    yes: usize,
    no: usize,
}

#[repr(C)]
struct SelectionPlan {
    yes: FusionBlock,
    no: FusionBlock,
    following: FusionBlock,
    domain: usize,
    indices: *const u8,
    slots: *mut SelectionSlot,
    values: *mut SelectionValue,
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::plan_selection"]
    fn plan_selection(
        input: SelectionPlan, context: *const std::ffi::c_void,
        read: unsafe extern "C" fn(*const std::ffi::c_void, FusionBlock, usize) -> SelectionNode,
    ) -> usize;
}

unsafe extern "C" fn read_node(
    context: *const std::ffi::c_void, block: FusionBlock, index: usize,
) -> SelectionNode {
    use SelectionKind::*;
    let registers = unsafe { &*context.cast::<&[Scalar]>() };
    let code = unsafe { std::slice::from_raw_parts(block.instructions.cast::<Inst>(), block.count) };
    let (kind, first, second) = match &code[index].kind {
        InstKind::Constant(..)
        | InstKind::Index(_)
        | InstKind::Uniform(..)
        | InstKind::BufferStride(..)
        | InstKind::IntegerDivide { divisor: IntegerDivisor::Constant(_), .. }
        | InstKind::Select { .. }
        | InstKind::Unary { .. }
        | InstKind::Math { op: MathOp::Abs, .. } => (Pure, 0, 0),
        InstKind::LoadLocal(dst, slot) => (LocalRead, *slot, *dst),
        InstKind::StoreLocal(slot, value) => (LocalWrite, *slot, *value),
        InstKind::Load { binding, index, .. } => (DataRead, *binding, *index),
        InstKind::Binary { a, .. } => (Arithmetic, usize::from(registers[*a].is_float()), 0),
        InstKind::Cast { dst, src } =>
            (Arithmetic, usize::from(registers[*dst].is_float() || registers[*src].is_float()), 0),
        _ => (Ordered, 0, 0),
    };
    SelectionNode { first, second, kind }
}

fn plan(
    yes: &[Inst], no: &[Inst], following: &[Inst], registers: &[Scalar], domain: usize,
    indices: &[u8], slots: &mut [SelectionSlot],
) -> Vec<SelectionValue> {
    let mut values = Vec::with_capacity(yes.len().min(no.len()).min(following.len()));
    slots.fill(SelectionSlot::default());
    let input = SelectionPlan {
        yes: yes.into(), no: no.into(), following: following.into(), domain,
        indices: indices.as_ptr(), slots: slots.as_mut_ptr(), values: values.as_mut_ptr(),
    };
    // The B planner borrows the original IR and initializes exactly the returned rows.
    unsafe {
        let count = plan_selection(input, (&registers as *const &[Scalar]).cast(), read_node);
        values.set_len(count);
    }
    values
}

pub(super) fn run(
    code: &mut Vec<Inst>, registers: &[Scalar], domain: usize, local_count: usize,
) -> bool {
    let mut indices = vec![0; registers.len()];
    visit(code, &mut |i| {
        if let InstKind::Index(reg) = i.kind {
            indices[reg] = 1;
        }
    });
    let mut slots = vec![SelectionSlot::default(); local_count];
    block(code, registers, domain, &indices, &mut slots)
}

fn block(
    code: &mut Vec<Inst>, registers: &[Scalar], domain: usize,
    indices: &[u8], slots: &mut [SelectionSlot],
) -> bool {
    let mut changed = false;
    let mut source = std::mem::take(code).into_iter();
    while let Some(mut inst) = source.next() {
        match &mut inst.kind {
            InstKind::Scope(body) => changed |= block(body, registers, domain, indices, slots),
            InstKind::If { yes, no, .. } => {
                changed |= block(yes, registers, domain, indices, slots);
                changed |= block(no, registers, domain, indices, slots);
            }
            InstKind::Loop { condition, body, .. } => {
                changed |= block(condition, registers, domain, indices, slots);
                changed |= block(body, registers, domain, indices, slots);
            }
            _ => {}
        }
        if let InstKind::If { condition, yes, no } = &mut inst.kind {
            let values = plan(yes, no, source.as_slice(), registers, domain, indices, slots);
            if !values.is_empty() {
                code.extend(std::mem::take(yes).into_iter().chain(std::mem::take(no))
                    .filter(|inst| !matches!(inst.kind, InstKind::StoreLocal(..))));
                for value in values {
                    let dst = value.destination;
                    code.push(Inst::new(InstKind::Select {
                        dst, condition: *condition, a: value.yes, b: value.no,
                    }));
                    // A later loop can still read this mutable slot.
                    code.push(Inst::new(InstKind::StoreLocal(value.slot, dst)));
                    source.next();
                }
                changed = true;
                continue;
            }
        }
        code.push(inst);
    }
    changed
}
