use super::*;
use crate::application::{ElementCompletion, Resources};
use crate::ir::dataflow::{self, Host, Node, Span as IrSpan};
use std::{ffi::c_void, mem, ptr};

#[repr(C)]
struct Input {
    functions: IrSpan,
    root: IrSpan,
    programs: *mut c_void,
    program_count: usize,
    completions: *const c_void,
}

#[repr(C)]
struct Body {
    registers: *mut c_void,
    locals: *mut c_void,
    code: *mut c_void,
}

#[repr(C)]
struct StageBodies {
    main: Body,
    project: Body,
    identity: Body,
    combine: Body,
}

#[repr(C)]
struct Layout {
    uniforms: IrSpan,
    datasets: usize,
    textures: usize,
}

#[repr(C)]
struct Completion {
    offset: u32,
    mask: u64,
    bits: u64,
    lower: IrSpan,
    upper: IrSpan,
    parameters: IrSpan,
    distances: IrSpan,
}

#[repr(C)]
struct Slots {
    register: usize,
    local: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum Emit {
    Constant, Bitcast, Cast, NotEqual, Less, Add, Subtract, Multiply, Select,
    Store, StoreIndexed,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum ArgumentKind {
    Original, U32, U64, Datasets, Uniforms, Textures, Intervals, Limits,
}

#[repr(C)]
struct Argument {
    kind: ArgumentKind,
    value: usize,
    count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum ResourceCall {
    Allocate, Run, Complete,
}

struct Block<'a> {
    remaining: std::vec::IntoIter<Inst>,
    code: &'a mut Vec<Inst>,
    current: Option<Inst>,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ResourceHost {
    ir: Host,
    function: unsafe extern "C" fn(*mut c_void) -> Body,
    stages: unsafe extern "C" fn(*mut c_void, usize) -> IrSpan,
    stage: unsafe extern "C" fn(*mut c_void) -> StageBodies,
    layout: unsafe extern "C" fn(*mut c_void, usize) -> Layout,
    completions: unsafe extern "C" fn(*const c_void, usize) -> IrSpan,
    completion: unsafe extern "C" fn(*const c_void) -> Completion,
    slots: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Slots,
    block: unsafe extern "C" fn(*mut c_void, *mut State, *const Input, ResourceHost),
    next: unsafe extern "C" fn(*mut c_void) -> Node,
    child: unsafe extern "C" fn(*mut c_void, u32) -> *mut c_void,
    keep: unsafe extern "C" fn(*mut c_void),
    emit: unsafe extern "C" fn(*const State, *mut c_void, Emit, u32, usize, u64, usize, usize),
    call: unsafe extern "C" fn(*const State, *mut c_void, ResourceCall, *const Argument, usize),
}

#[repr(C)]
struct State {
    registers: *mut c_void,
    locals: *mut c_void,
    slots: Slots,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::lower_sdk_resources"]
    fn lower(input: *const Input, host: ResourceHost);
    #[link_name = "bootstrap.ir_dataflow::sdk_resource_block"]
    fn lower_block(state: *mut State, input: *const Input, host: ResourceHost, block: *mut c_void);
}

pub(in crate::check) fn lower_sdk_resources(
    program: &mut Program, functions: &mut [FunctionBody], resources: &mut Resources,
) {
    let input = Input {
        functions: IrSpan::mutable(functions),
        root: stages(program),
        programs: resources.programs.as_mut_ptr().cast(),
        program_count: resources.programs.len(),
        completions: resources.completions.as_ptr().cast(),
    };
    unsafe { lower(&input, HOST); }
}

impl Body {
    const NONE: Self = Self {
        registers: ptr::null_mut(), locals: ptr::null_mut(), code: ptr::null_mut(),
    };

    fn value(body: &mut ValueCode) -> Self {
        Self {
            registers: ptr::from_mut(&mut body.registers).cast(),
            locals: ptr::from_mut(&mut body.locals).cast(),
            code: ptr::from_mut(&mut body.code).cast(),
        }
    }
}

fn stages(program: &mut Program) -> IrSpan {
    if program.native_frame.is_some() {
        IrSpan::mutable(&mut program.stages)
    } else {
        IrSpan::EMPTY
    }
}

unsafe extern "C" fn function_body(function: *mut c_void) -> Body {
    Body::value(unsafe { &mut (*function.cast::<FunctionBody>()).body })
}

unsafe extern "C" fn program_stages(programs: *mut c_void, index: usize) -> IrSpan {
    stages(unsafe { &mut (*programs.cast::<(Target, Program)>().add(index)).1 })
}

unsafe extern "C" fn stage_bodies(stage: *mut c_void) -> StageBodies {
    let stage = unsafe { &mut *stage.cast::<Stage>() };
    let main = Body {
        registers: ptr::from_mut(&mut stage.registers).cast(),
        locals: ptr::from_mut(&mut stage.locals).cast(),
        code: ptr::from_mut(&mut stage.code).cast(),
    };
    let values = match &mut stage.operation {
        Operation::CustomReduce(values) => values,
        Operation::CustomScan(scan) => &mut scan.values,
        Operation::CustomSegmentedReduce(reduction) => &mut reduction.values,
        _ => return StageBodies { main, project: Body::NONE, identity: Body::NONE, combine: Body::NONE },
    };
    StageBodies {
        main,
        project: values.project.as_mut().map_or(Body::NONE, Body::value),
        identity: Body::value(&mut values.identity),
        combine: Body::value(&mut values.combine),
    }
}

// Only borrow layout fields. A nested stage can be lowered while its own
// program's layout is read to form an outgoing call.
unsafe extern "C" fn layout(programs: *mut c_void, index: usize) -> Layout {
    let program = unsafe { ptr::addr_of!((*programs.cast::<(Target, Program)>().add(index)).1) };
    unsafe {
        Layout {
            uniforms: IrSpan::slice(&(*program).uniforms),
            datasets: (*program).buffers.len(),
            textures: (*program).textures.len(),
        }
    }
}

unsafe extern "C" fn completions(entries: *const c_void, index: usize) -> IrSpan {
    IrSpan::slice(unsafe { &*entries.cast::<Vec<ElementCompletion>>().add(index) })
}

unsafe extern "C" fn completion(field: *const c_void) -> Completion {
    let field = unsafe { &*field.cast::<ElementCompletion>() };
    Completion {
        offset: field.offset, mask: field.known_bits.0, bits: field.known_bits.1,
        lower: IrSpan::field(&field.intervals, |pair| &pair.0),
        upper: IrSpan::field(&field.intervals, |pair| &pair.1),
        parameters: IrSpan::field(&field.limits, |pair| &pair.0),
        distances: IrSpan::field(&field.limits, |pair| &pair.1),
    }
}

unsafe extern "C" fn slots(registers: *mut c_void, locals: *mut c_void) -> Slots {
    Slots {
        register: unsafe { &*registers.cast::<Vec<Scalar>>() }.len(),
        local: unsafe { &*locals.cast::<BTreeMap<usize, Ty>>() }.last_key_value().map_or(0, |(&end, _)| end),
    }
}

unsafe extern "C" fn block(
    code: *mut c_void, state: *mut State, input: *const Input, host: ResourceHost,
) {
    let code = unsafe { &mut *code.cast::<Vec<Inst>>() };
    let remaining = mem::take(code).into_iter();
    let mut block = Block { remaining, code, current: None };
    unsafe { lower_block(state, input, host, ptr::from_mut(&mut block).cast()); }
}

unsafe extern "C" fn next(block: *mut c_void) -> Node {
    let block = unsafe { &mut *block.cast::<Block<'_>>() };
    block.current = block.remaining.next();
    block.current.as_ref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn child(block: *mut c_void, index: u32) -> *mut c_void {
    let kind = &mut unsafe { &mut *block.cast::<Block<'_>>() }.current.as_mut().unwrap().kind;
    let code = match kind {
        InstKind::Scope(code) => code,
        InstKind::If { yes, no, .. } => if index == 0 { yes } else { no },
        InstKind::Loop { condition, body, .. } => if index == 0 { condition } else { body },
        _ => unreachable!("SDK lowering borrows a child of the selected IR block"),
    };
    ptr::from_mut(code).cast()
}

unsafe extern "C" fn keep(block: *mut c_void) {
    let block = unsafe { &mut *block.cast::<Block<'_>>() };
    block.code.push(block.current.take().unwrap());
}

fn instruction(original: &Inst, kind: InstKind) -> Inst {
    Inst {
        source_site: original.source_site.clone(),
        generated_origin: original.generated_origin.clone(),
        ..Inst::new(kind)
    }
}

fn scalar(tag: u32) -> Scalar {
    use crate::resource::*;
    match tag {
        U32 => Scalar::U32, BOOL => Scalar::Bool,
        U64 => Scalar::U64,
        _ => unreachable!("SDK lowering uses the original scalar tags"),
    }
}

unsafe extern "C" fn emit(
    state: *const State, block: *mut c_void, op: Emit, tag: u32,
    dst: usize, a: u64, b: usize, c: usize,
) {
    let state = unsafe { &*state };
    let block = block.cast::<Block<'_>>();
    let binary = |op| InstKind::Binary { dst, op, a: a as usize, b };
    let kind = match op {
        Emit::Constant => InstKind::Constant(dst, scalar(tag), a),
        Emit::Bitcast => InstKind::Unary { dst, op: UnaryOp::Bitcast, a: a as usize },
        Emit::Cast => InstKind::Cast { dst, src: a as usize },
        Emit::NotEqual => binary(ir::BinaryOp::Compare(Comparison::NotEqual)),
        Emit::Less => binary(ir::BinaryOp::Compare(Comparison::Less)),
        Emit::Add => binary(ir::BinaryOp::IAdd),
        Emit::Subtract => binary(ir::BinaryOp::ISub),
        Emit::Multiply => binary(ir::BinaryOp::IMul),
        Emit::Select => InstKind::Select { dst, condition: a as usize, a: b, b: c },
        Emit::Store => InstKind::StoreLocal(dst, a as usize),
        Emit::StoreIndexed => InstKind::StoreLocalIndexed {
            base: dst, len: a as usize, index: b, word: 0, value: c,
        },
    };
    if !matches!(op, Emit::Store | Emit::StoreIndexed) {
        unsafe { &mut *state.registers.cast::<Vec<Scalar>>() }.push(scalar(tag));
    }
    unsafe { (*block).code.push(instruction((*block).current.as_ref().unwrap(), kind)); }
}

fn record(name: &str, size: u32, align: u32, fields: &[(&str, Scalar, u32)]) -> Ty {
    let mut ty = Ty::unit();
    ty.name = name.into();
    ty.size = size;
    ty.align = align;
    ty.fields = fields.iter().map(|&(name, scalar, offset)| {
        (name.into(), offset, Box::new(Ty::scalar(scalar)))
    }).collect();
    ty.fill_components();
    ty
}

fn packet_element(kind: ArgumentKind) -> Ty {
    use crate::resource::{DatasetBinding, ElementInterval};
    match kind {
        ArgumentKind::Datasets => record(
            "@blink.DatasetBinding", size_of::<DatasetBinding>() as u32,
            align_of::<DatasetBinding>() as u32, &[
                ("handle", Scalar::U32, mem::offset_of!(DatasetBinding, handle) as u32),
                ("offset", Scalar::U32, mem::offset_of!(DatasetBinding, offset) as u32),
            ],
        ),
        ArgumentKind::Uniforms => record(
            "@blink.UniformValue", 16, 8, &[("kind", Scalar::U32, 0), ("bits", Scalar::U64, 8)],
        ),
        ArgumentKind::Textures => Ty::scalar(Scalar::U64),
        ArgumentKind::Intervals => record(
            "@blink.ElementInterval", size_of::<ElementInterval>() as u32,
            align_of::<ElementInterval>() as u32, &[
                ("lower", Scalar::U64, mem::offset_of!(ElementInterval, lower) as u32),
                ("upper", Scalar::U64, mem::offset_of!(ElementInterval, upper) as u32),
            ],
        ),
        ArgumentKind::Limits => Ty::scalar(Scalar::U32),
        _ => unreachable!("only resource packets own an array layout"),
    }
}

fn packet(locals: &mut BTreeMap<usize, Ty>, argument: &Argument) -> CallArgument {
    let element = packet_element(argument.kind);
    let count = argument.count as u32;
    let mut ty = Ty::unit();
    ty.name = format!("array<{},{}>", element.name, count);
    ty.size = unsafe { ir::fixed_array_layout(element.size, element.align, count) }.size as u32;
    ty.align = element.align;
    ty.values = vec![count];
    ty.arguments = vec![element.clone()];
    ty.array = Some(Box::new(element));
    ty.fill_components();
    let base = argument.value;
    let len = ty.components.len();
    if len != 0 {
        locals.insert(base + len, ty.clone());
    }
    CallArgument::Reference {
        origin: None, ty, access: Access::Read,
        address: CallReference { storage: CallStorage::Local { base, len }, offset: None, word: 0 },
    }
}

unsafe extern "C" fn call(
    state: *const State, block: *mut c_void, target: ResourceCall, arguments: *const Argument,
    count: usize,
) {
    let block = block.cast::<Block<'_>>();
    let original = unsafe { &(*block).current }.as_ref().unwrap();
    let InstKind::Call(source) = &original.kind else { unreachable!() };
    let locals = unsafe { &mut *(*state).locals.cast::<BTreeMap<usize, Ty>>() };
    let arguments = unsafe { std::slice::from_raw_parts(arguments, count) }.iter().map(|argument| match argument.kind {
        ArgumentKind::Original => source.arguments[argument.value].clone(),
        ArgumentKind::U32 | ArgumentKind::U64 => CallArgument::Value {
            origin: None, dataset_borrow: None,
            ty: Ty::scalar(if matches!(argument.kind, ArgumentKind::U32) { Scalar::U32 } else { Scalar::U64 }),
            values: vec![argument.value],
        },
        _ => packet(locals, argument),
    }).collect();
    let symbol = match target {
        ResourceCall::Allocate => "blink_allocate",
        ResourceCall::Run => "blink_run",
        ResourceCall::Complete => "blink_complete_field",
    };
    let target_name = CallTarget::External(ExternalName {
        library: crate::resource::RUNTIME_LIBRARY.into(), symbol: symbol.into(),
    });
    if matches!(target, ResourceCall::Complete) {
        let call = Call {
            target: target_name, arguments, failure: source.failure.clone(), error: source.error,
            memory: source.memory.clone(), memory_use: source.memory_use,
            result: source.result.clone(), values: source.values.clone(),
        };
        unsafe { (*block).code.push(instruction(original, InstKind::Call(call))); }
    } else {
        let block = unsafe { &mut *block };
        let mut original = block.current.take().unwrap();
        let InstKind::Call(source) = &mut original.kind else { unreachable!() };
        source.target = target_name;
        source.arguments = arguments;
        block.code.push(original);
    }
}

const HOST: ResourceHost = ResourceHost {
    ir: dataflow::HOST, function: function_body, stages: program_stages, stage: stage_bodies,
    layout, completions, completion, slots, block, next, child, keep, emit, call,
};
