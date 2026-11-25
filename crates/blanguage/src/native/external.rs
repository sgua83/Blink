//! Windows x64普通B与C ABI：实际机器参数先进入调用槽，再交给参数寄存器。
use super::operands::{IndexedAddressHost, MoveLocation, ScalarLocations, value_local};
use crate::ir::dataflow::{Node, Span};
use super::*;
use crate::builtin_types::ScalarEncoding;
use allocation::Location;
use std::ffi::c_void;
#[repr(C)]
pub(super) struct AbiType { size: u32, alignment: u32, float_size: u32, value: u32 }
impl AbiType {
    pub(super) fn value(ty: &Ty) -> Self {
        // Built-in scalar names are canonical. Declared types carry their qualified names.
        let float_size = match ty.name.as_str() {
            "f32" => 4,
            "f64" => 8,
            _ => 0,
        };
        Self { size: ty.size, alignment: ty.align, float_size, value: 1 }
    }

    pub(super) fn reference() -> Self { Self { size: 0, alignment: 1, float_size: 0, value: 0 } }

    pub(super) fn result(ty: Option<&Ty>) -> Self {
        ty.map_or_else(Self::reference, Self::value)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub(super) enum ResultLocation { None, Integer, Float, Indirect }

type ArgumentType = unsafe extern "C" fn(*const c_void, usize) -> AbiType;

#[repr(C)]
struct CallValueLoan {
    components: Span,
    values: Span,
    locations: *const c_void,
}

impl CallValueLoan {
    fn new(ty: Option<&Ty>, values: &[Reg], locations: &ScalarLocations<'_>) -> Self {
        Self {
            components: Span::slice(ty.map_or(&[], |ty| ty.components.as_slice())),
            values: Span::slice(values),
            locations: std::ptr::from_ref(locations).cast(),
        }
    }
}

#[repr(C)]
struct CallInput {
    source: Node,
    result: AbiType,
    values: CallValueLoan,
    error: Reg,
    flags: u32,
}

#[repr(C)]
struct CallHost {
    addresses: IndexedAddressHost,
    local: unsafe extern "C" fn(*const c_void, usize) -> MoveLocation,
    argument: ArgumentType,
    loan: unsafe extern "C" fn(*const c_void, usize) -> CallValueLoan,
    component: AbiComponent,
    reserve: unsafe extern "C" fn(*mut c_void, usize) -> *mut u8,
}

#[repr(C)]
struct SymbolEncoding { length: u64, field: u64 }
#[repr(u32)]
pub(super) enum AbiMemoryMode {
    StackLoad,
    StackStore,
    IndirectParameter,
    IndirectResult,
    FallibleResult,
    PackIntegerResult,
    UnpackIntegerResult,
    RangeSummary,
}

#[repr(C)]
struct AbiValueComponent {
    location: MoveLocation,
    scalar: ScalarEncoding,
    field_scalar: ScalarEncoding,
    offset: u32,
}

struct RangeSummary<'a> {
    allocation: &'a Allocation,
    first_slot: usize,
    fields: &'a [(Scalar, u32)],
    payload_words: usize,
}

unsafe extern "C" fn summary_component(context: *const c_void, index: u64) -> AbiValueComponent {
    let input = unsafe { &*context.cast::<RangeSummary<'_>>() };
    let index = index as usize;
    let (scalar, offset) = match input.fields.get(index) {
        Some(&field) => field,
        None => (Scalar::U32, (input.payload_words + index - input.fields.len()) as u32 * 4),
    };
    let location = input.allocation.location(PrivateSlot::Local(input.first_slot + index));
    AbiValueComponent {
        location: location.into(), scalar: scalar.into(), field_scalar: scalar.into(), offset,
    }
}

type AbiComponent = unsafe extern "C" fn(*const c_void, u64) -> AbiValueComponent;

fn append_abi_value(
    code: &mut Vec<u8>, slot: u32, count: usize, mode: AbiMemoryMode,
    context: *const c_void, component: AbiComponent,
) {
    let per_component = match mode {
        AbiMemoryMode::PackIntegerResult | AbiMemoryMode::UnpackIntegerResult => 48,
        _ => 24,
    };
    code.reserve(16 + per_component * count);
    let start = code.len();
    unsafe {
        let length = encode_abi_value(slot, count as u64, mode, context, component,
            code.as_mut_ptr().add(start));
        code.set_len(start + length as usize);
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::abi_value"]
    fn encode_abi_value(slot: u32, count: u64, mode: AbiMemoryMode, context: *const c_void,
        component: AbiComponent, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::argument_register"]
    fn encode_argument_register(slot: u8, float_size: u32, to_home: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::print_call"]
    fn emit_print_call(dst: MoveLocation, src: MoveLocation, value: ScalarEncoding,
        result: ScalarEncoding, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::symbol_target"]
    fn encode_symbol_target(invoke: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.call_layout::outgoing_bytes"]
    fn call_stack_size(context: *const c_void, count: usize, result: AbiType, flags: u32,
        argument: ArgumentType) -> u64;
    #[link_name = "bootstrap.call_layout::emit_call"]
    fn emit_call(context: *mut c_void, input: CallInput, host: CallHost) -> SymbolEncoding;
}

fn argument_abi(call: &Call, index: usize) -> AbiType {
    match &call.arguments[index] {
        CallArgument::Value { ty, .. } => AbiType::value(ty),
        CallArgument::Reference { .. } => AbiType::reference(),
    }
}

unsafe extern "C" fn argument_type(context: *const c_void, index: usize) -> AbiType {
    argument_abi(unsafe { &*context.cast::<Call>() }, index)
}

fn call_flags(call: &Call) -> u32 {
    u32::from(call.error.is_some()) | (u32::from(matches!(call.target, CallTarget::Indirect(_))) << 1)
}

pub(super) fn outgoing_bytes(call: &Call) -> u64 {
    let context = std::ptr::from_ref(call).cast();
    unsafe { call_stack_size(context, call.arguments.len(), AbiType::result(call.result.as_ref()),
        call_flags(call), argument_type) }
}

struct CallEmission<'a, 'b> {
    call: &'a Call,
    locations: &'a ScalarLocations<'b>,
    code: &'a mut Vec<u8>,
}

unsafe extern "C" fn emission_argument(context: *const c_void, index: usize) -> AbiType {
    let state = unsafe { &*context.cast::<CallEmission<'_, '_>>() };
    argument_abi(state.call, index)
}

unsafe extern "C" fn call_loan(context: *const c_void, index: usize) -> CallValueLoan {
    let state = unsafe { &*context.cast::<CallEmission<'_, '_>>() };
    match &state.call.arguments[index] {
        CallArgument::Value { ty, values, .. } => CallValueLoan::new(Some(ty), values, state.locations),
        CallArgument::Reference { .. } => CallValueLoan::new(None, &[], state.locations),
    }
}

unsafe extern "C" fn abi_component(context: *const c_void, index: u64) -> AbiValueComponent {
    let loan = unsafe { &*context.cast::<CallValueLoan>() };
    let locations = unsafe { &*loan.locations.cast::<ScalarLocations<'_>>() };
    let reg = unsafe { loan.values.as_slice::<Reg>() }[index as usize];
    let (scalar, offset) = unsafe { loan.components.as_slice::<(Scalar, u32)>() }[index as usize];
    AbiValueComponent {
        location: locations.allocation.location(PrivateSlot::Register(reg)).into(),
        scalar: locations.stage.registers[reg].into(),
        field_scalar: scalar.into(),
        offset,
    }
}

unsafe extern "C" fn reserve_call(context: *mut c_void, capacity: usize) -> *mut u8 {
    let state = unsafe { &mut *context.cast::<CallEmission<'_, '_>>() };
    let code = &mut *state.code;
    code.reserve(capacity);
    unsafe { code.as_mut_ptr().add(code.len()) }
}

impl Emitter<'_> {
    pub(super) fn abi_value(
        &mut self, slot: u32, ty: &Ty, values: &[Reg], mode: AbiMemoryMode,
    ) {
        let count = ty.components.len().min(values.len());
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let input = CallValueLoan::new(Some(ty), values, &locations);
        append_abi_value(&mut self.code, slot, count, mode, std::ptr::from_ref(&input).cast(),
            abi_component);
    }

    pub(super) fn write_range_summary(
        &mut self, stack_bytes: u32, first_slot: usize, ty: &Ty, payload_words: usize,
        summary_words: usize,
    ) {
        let input = RangeSummary {
            allocation: &self.allocation, first_slot, fields: &ty.components, payload_words,
        };
        let count = ty.components.len() + (summary_words - payload_words);
        append_abi_value(&mut self.code, stack_bytes, count, AbiMemoryMode::RangeSummary,
            std::ptr::from_ref(&input).cast(), summary_component);
    }

    pub(super) fn argument_register(&mut self, slot: usize, float_size: u32, to_home: bool) {
        self.emit_bytes(9, |output| unsafe {
            encode_argument_register(slot as u8, float_size, u32::from(to_home), output)
        });
    }
    pub(super) fn print_call(&mut self, dst: Reg, value: Reg) {
        let scalar = self.stage.registers[value];
        let target = self.location(dst).into();
        let source = self.location(value).into();
        let start = self.code.len();
        self.code.reserve(84);
        let field = unsafe {
            let emission = emit_print_call(target, source, scalar.into(),
                self.stage.registers[dst].into(), self.code.as_mut_ptr().add(start));
            self.code.set_len(start + emission.length as usize);
            emission.field as usize
        };
        self.record_symbol_target(start + field, NativeSymbol::Print(scalar), true);
    }

    pub(super) fn emit_symbol_target(&mut self, target: NativeSymbol, invoke: bool) {
        self.code.reserve(12);
        let start = self.code.len();
        let encoding = unsafe {
            let encoding = encode_symbol_target(u32::from(invoke), self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoding.length as usize);
            encoding
        };
        self.record_symbol_target(start + encoding.field as usize, target, invoke);
    }
    pub(super) fn record_symbol_target(&mut self, offset: usize, target: NativeSymbol, invoke: bool) {
        self.relocations.push(Relocation { offset, target });
        if invoke {
            self.calls.push(Some(offset));
        }
    }
    pub(super) fn native_call(&mut self, call: &Call) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = CallHost {
            addresses: locations.indexed_host(), local: value_local,
            argument: emission_argument, loan: call_loan, component: abi_component, reserve: reserve_call,
        };
        let input = CallInput {
            source: Node::call(call), result: AbiType::result(call.result.as_ref()),
            values: CallValueLoan::new(call.result.as_ref(), &call.values, &locations),
            error: call.error.unwrap_or_default(), flags: call_flags(call),
        };
        let start = self.code.len();
        let mut context = CallEmission { call, locations: &locations, code: &mut self.code };
        let encoded = unsafe { emit_call(std::ptr::from_mut(&mut context).cast(), input, host) };
        unsafe { self.code.set_len(start + encoded.length as usize) };
        match &call.target {
            CallTarget::External(name) => self.record_symbol_target(start + encoded.field as usize, NativeSymbol::External(name.clone()), true),
            CallTarget::Defined(name) => self.record_symbol_target(start + encoded.field as usize, NativeSymbol::Function(name.clone()), true),
            CallTarget::Indirect(_) => self.calls.push(None),
        }
    }
}
