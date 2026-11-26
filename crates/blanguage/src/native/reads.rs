//! B owns read placement. The output borrows the original block until placement changes it.
use super::*;
use crate::ir::dataflow::{self, Node, Span};
use std::{borrow::Cow, ffi::c_void, ptr, slice, vec::IntoIter};

struct Pending<'code> {
    code: Vec<Option<Cow<'code, Inst>>>,
    original: &'code [Inst],
    out: Cow<'code, [Inst]>,
}

impl<'code> Pending<'code> {
    fn push(&mut self, instruction: Cow<'code, Inst>) {
        if let (Cow::Borrowed(prefix), Cow::Borrowed(next)) = (&self.out, &instruction) {
            let index = prefix.len();
            // This prefix has consumed exactly the original instructions before index.
            if ptr::eq(*next, &self.original[index]) {
                self.out = Cow::Borrowed(&self.original[..index + 1]);
                return;
            }
        }
        self.out.to_mut().push(instruction.into_owned());
    }
}

enum Remaining<'code> {
    Borrowed(slice::Iter<'code, Inst>),
    Owned(IntoIter<Inst>),
}

impl<'code> Remaining<'code> {
    fn len(&self) -> usize {
        match self {
            Self::Borrowed(code) => code.len(),
            Self::Owned(code) => code.len(),
        }
    }

    fn next(&mut self) -> Option<Cow<'code, Inst>> {
        match self {
            Self::Borrowed(code) => code.next().map(Cow::Borrowed),
            Self::Owned(code) => code.next().map(Cow::Owned),
        }
    }
}

struct Input<'code, 'schedule> {
    remaining: Remaining<'code>,
    current: Option<Cow<'code, Inst>>,
    expanded: IntoIter<Inst>,
    registers: &'schedule mut Cow<'code, [Scalar]>,
    writes: &'schedule [usize],
}

#[repr(C)]
struct ReadPhases {
    ends: Span,
    instructions: usize,
    registers: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    ir: dataflow::Host,
    math_outgoing: unsafe extern "C" fn(u32) -> u64,
    next: unsafe extern "C" fn(*mut Input<'_, '_>, usize) -> Node,
    sample: unsafe extern "C" fn(*mut Input<'_, '_>) -> usize,
    children: unsafe extern "C" fn(*mut Input<'_, '_>, *mut c_void),
    store: for<'code, 'schedule> unsafe extern "C" fn(*mut Input<'code, 'schedule>, *mut Pending<'code>, bool),
    queued: unsafe extern "C" fn(*const Pending<'_>, usize) -> Node,
    emit: unsafe extern "C" fn(*mut Pending<'_>, usize),
    clear: unsafe extern "C" fn(*mut Pending<'_>),
    written: unsafe extern "C" fn(*const Pending<'_>) -> usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::place_reads"]
    fn place_reads(input: *mut c_void, pending: *mut c_void, writes: Span, phases: ReadPhases, host: Host);
    #[link_name = "bootstrap.ir_dataflow::place_read_block"]
    fn place_read_block(input: *mut c_void, pending: *mut c_void,
        scratch: *mut c_void, writes: Span, tail: usize, host: Host);
}

unsafe extern "C" fn next(input: *mut Input<'_, '_>, tail: usize) -> Node {
    let input = unsafe { &mut *input };
    input.current = input.expanded.next().map(Cow::Owned).or_else(|| {
        if input.remaining.len() == tail { None } else { input.remaining.next() }
    });
    input.current.as_deref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn sample(input: *mut Input<'_, '_>) -> usize {
    let input = unsafe { &mut *input };
    let source = unsafe { input.current.take().unwrap_unchecked() };
    let InstKind::TextureSample { result, texture, coordinate, lod } = &source.kind else {
        // B requests expansion only after inspecting OP_TEXTURE_SAMPLE.
        unsafe { std::hint::unreachable_unchecked() }
    };
    let origin = OperationOrigin::from_instruction(&source);
    let registers = input.registers.to_mut();
    input.expanded = texture::sample(registers, *texture, coordinate, *lod, result, origin).into_iter();
    registers.len()
}

fn changed_pair<'code>(
    first: Cow<'code, [Inst]>, second: Cow<'code, [Inst]>,
) -> Option<(Vec<Inst>, Vec<Inst>)> {
    if matches!((&first, &second), (Cow::Borrowed(_), Cow::Borrowed(_))) {
        None
    } else {
        Some((first.into_owned(), second.into_owned()))
    }
}

fn place_children<'code>(
    instruction: Cow<'code, Inst>,
    mut place: impl FnMut(Cow<'code, [Inst]>) -> Cow<'code, [Inst]>,
) -> Cow<'code, Inst> {
    match instruction {
        Cow::Borrowed(original) => {
            let changed = match &original.kind {
                InstKind::If { condition, yes, no } => {
                    let yes = place(Cow::Borrowed(yes));
                    let no = place(Cow::Borrowed(no));
                    changed_pair(yes, no).map(|(yes, no)| InstKind::If { condition: *condition, yes, no })
                }
                InstKind::Loop { condition, test, body } => {
                    let condition = place(Cow::Borrowed(condition));
                    let body = place(Cow::Borrowed(body));
                    changed_pair(condition, body).map(|(condition, body)| InstKind::Loop { condition, test: *test, body })
                }
                InstKind::Scope(body) => match place(Cow::Borrowed(body)) {
                    Cow::Borrowed(_) => None,
                    Cow::Owned(body) => Some(InstKind::Scope(body)),
                },
                _ => unsafe { std::hint::unreachable_unchecked() },
            };
            changed.map_or(Cow::Borrowed(original), |kind| Cow::Owned(Inst {
                kind,
                float_preserve: original.float_preserve,
                source_site: original.source_site.clone(),
                generated_origin: original.generated_origin.clone(),
            }))
        }
        Cow::Owned(mut original) => {
            let mut place_owned = |body: &mut Vec<Inst>| {
                *body = place(Cow::Owned(std::mem::take(body))).into_owned();
            };
            match &mut original.kind {
                InstKind::If { yes, no, .. } => { place_owned(yes); place_owned(no); }
                InstKind::Loop { condition, body, .. } => { place_owned(condition); place_owned(body); }
                InstKind::Scope(body) => place_owned(body),
                _ => unsafe { std::hint::unreachable_unchecked() },
            }
            Cow::Owned(original)
        }
    }
}

unsafe extern "C" fn children(input: *mut Input<'_, '_>, scratch: *mut c_void) {
    let input = unsafe { &mut *input };
    let original = unsafe { input.current.take().unwrap_unchecked() };
    let placed = place_children(original, |code| {
        place_block(code, input.registers, input.writes, |input, pending, writes| unsafe {
            place_read_block(input.cast(), pending.cast(), scratch, writes, 0, HOST)
        })
    });
    input.current = Some(placed);
}

unsafe extern "C" fn store<'code>(input: *mut Input<'code, '_>, pending: *mut Pending<'code>, defer: bool) {
    let inst = unsafe { (&mut *input).current.take().unwrap_unchecked() };
    let pending = unsafe { &mut *pending };
    if defer {
        pending.code.push(Some(inst));
    } else {
        pending.push(inst);
    }
}

unsafe extern "C" fn queued(pending: *const Pending<'_>, id: usize) -> Node {
    let code = unsafe { &(*pending).code };
    code[id].as_deref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn emit(pending: *mut Pending<'_>, id: usize) {
    let pending = unsafe { &mut *pending };
    let inst = unsafe { pending.code[id].take().unwrap_unchecked() };
    pending.push(inst);
}

unsafe extern "C" fn clear(pending: *mut Pending<'_>) {
    unsafe { &mut (*pending).code }.clear();
}

unsafe extern "C" fn written(pending: *const Pending<'_>) -> usize {
    unsafe { &*pending }.out.len()
}

const HOST: Host = Host {
    ir: dataflow::HOST,
    math_outgoing: operands::scalar_math_outgoing,
    next, sample, children, store, queued, emit, clear, written,
};

pub(super) fn place<'code>(
    code: &'code [Inst], registers: &'code [Scalar], writes: &[usize],
) -> (Cow<'code, [Inst]>, Cow<'code, [Scalar]>) {
    let mut registers = Cow::Borrowed(registers);
    let code = place_outer(Cow::Borrowed(code), &mut registers, writes, &mut []);
    (code, registers)
}

pub(super) fn place_phases(
    code: &mut Vec<Inst>, registers: &mut Vec<Scalar>, writes: &[usize], phase_ends: &mut [usize],
) {
    let mut scalars = Cow::Owned(std::mem::take(registers));
    *code = place_outer(Cow::Owned(std::mem::take(code)), &mut scalars, writes, phase_ends).into_owned();
    *registers = scalars.into_owned();
}

fn place_outer<'code>(
    code: Cow<'code, [Inst]>, registers: &mut Cow<'code, [Scalar]>, writes: &[usize], phase_ends: &mut [usize],
) -> Cow<'code, [Inst]> {
    let phases = ReadPhases {
        ends: Span::mutable(phase_ends), instructions: code.len(), registers: registers.len(),
    };
    place_block(code, registers, writes, |input, pending, writes| unsafe {
        place_reads(input.cast(), pending.cast(), writes, phases, HOST)
    })
}

fn place_block<'code>(
    code: Cow<'code, [Inst]>, registers: &mut Cow<'code, [Scalar]>, writes: &[usize],
    schedule: impl FnOnce(*mut Input<'code, '_>, *mut Pending<'code>, Span),
) -> Cow<'code, [Inst]> {
    let (remaining, original, out) = match code {
        Cow::Borrowed(code) => (Remaining::Borrowed(code.iter()), code, Cow::Borrowed(&code[..0])),
        Cow::Owned(code) => (Remaining::Owned(code.into_iter()), &[][..], Cow::Owned(Vec::new())),
    };
    let mut input = Input {
        remaining, current: None, expanded: Vec::new().into_iter(), registers, writes,
    };
    let mut pending = Pending { original, out, code: Vec::new() };
    schedule(ptr::from_mut(&mut input), ptr::from_mut(&mut pending), Span::slice(writes));
    pending.out
}
