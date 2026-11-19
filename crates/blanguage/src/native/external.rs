//! Windows x64普通B与C ABI：实际机器参数先进入调用槽，再交给参数寄存器。
use super::operands::MoveLocation;
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

#[repr(C)]
struct CallPlace { index: usize, home: u64, payload: u64, copied: u32 }

#[allow(dead_code)]
#[repr(u32)]
enum CallAction {
    SaveTarget, ResultAddress, LoadRegister, Invoke,
    CaptureError, ReadIndirect, ReadFloat, ReadInteger, CloseError,
}

type ArgumentType = unsafe extern "C" fn(*const c_void, usize) -> AbiType;

#[repr(C)]
struct CallHost {
    argument: ArgumentType,
    place: unsafe extern "C" fn(*mut c_void, CallPlace) -> bool,
    action: unsafe extern "C" fn(*mut c_void, CallAction, u64, u32) -> u64,
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
}

#[repr(C)]
struct AbiValueComponent {
    location: MoveLocation,
    scalar: ScalarEncoding,
    field_scalar: ScalarEncoding,
    offset: u32,
}

struct AbiValue<'a> {
    allocation: &'a Allocation,
    registers: &'a [Scalar],
    components: &'a [(Scalar, u32)],
    values: &'a [Reg],
}

unsafe extern "C" fn abi_component(context: *const c_void, index: u64) -> AbiValueComponent {
    let input = unsafe { &*context.cast::<AbiValue<'_>>() };
    let index = index as usize;
    let reg = input.values[index];
    AbiValueComponent {
        location: input.allocation.location(PrivateSlot::Register(reg)).into(),
        scalar: input.registers[reg].into(),
        field_scalar: input.components[index].0.into(),
        offset: input.components[index].1,
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::abi_value"]
    fn encode_abi_value(slot: u32, count: u64, mode: AbiMemoryMode, context: *const c_void,
        component: unsafe extern "C" fn(*const c_void, u64) -> AbiValueComponent, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::abi_stack_pointer"]
    fn encode_abi_stack_pointer(offset: u32, slot: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::argument_register"]
    fn encode_argument_register(slot: u8, float_size: u32, to_home: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::symbol_target"]
    fn encode_symbol_target(invoke: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::call_slot"]
    fn encode_call_slot(offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.call_layout::outgoing_bytes"]
    fn call_stack_size(context: *const c_void, count: usize, result: AbiType, flags: u32,
        argument: ArgumentType) -> u64;
    #[link_name = "bootstrap.call_layout::emit_call"]
    fn emit_call(context: *mut c_void, count: usize, result: AbiType, flags: u32, host: CallHost);
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

struct CallEmission<'a, 'ir> {
    emitter: &'a mut Emitter<'ir>,
    call: &'a Call,
    result: *const Ty,
    indirect_target: Reg,
    failure: Option<&'static str>,
}

unsafe extern "C" fn emission_argument(context: *const c_void, index: usize) -> AbiType {
    let state = unsafe { &*context.cast::<CallEmission<'_, '_>>() };
    argument_abi(state.call, index)
}

unsafe extern "C" fn place_argument(context: *mut c_void, place: CallPlace) -> bool {
    let state = unsafe { &mut *context.cast::<CallEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    let home = place.home as u32;
    match &state.call.arguments[place.index] {
        CallArgument::Reference { address, .. } => {
            if let Err(reason) = e.reference_address(address) {
                state.failure = Some(reason);
                return false;
            }
            e.copy_scalar(Location::Stack(home), Location::Gpr(0), Scalar::U64);
        }
        CallArgument::Value { ty, values, .. } => {
            let payload = place.payload as u32;
            e.abi_value(payload, ty, values, AbiMemoryMode::StackStore);
            if place.copied != 0 {
                e.abi_stack_pointer(payload, home);
            }
        }
    }
    true
}

unsafe extern "C" fn call_action(context: *mut c_void, action: CallAction, value: u64, width: u32) -> u64 {
    let state = unsafe { &mut *context.cast::<CallEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    let call = state.call;
    let offset = value as u32;
    match action {
        CallAction::SaveTarget =>
            e.copy_scalar(Location::Stack(offset), e.location(state.indirect_target), Scalar::U64),
        CallAction::ResultAddress => e.abi_stack_pointer(offset, 0),
        CallAction::LoadRegister => e.argument_register(value as usize, width, false),
        CallAction::Invoke => match &call.target {
            CallTarget::External(name) => e.emit_symbol_target(NativeSymbol::External(name.clone()), true),
            CallTarget::Defined(name) => e.emit_symbol_target(NativeSymbol::Function(name.clone()), true),
            CallTarget::Indirect(_) => {
                e.emit_bytes(7, |output| unsafe { encode_call_slot(offset, output) });
                e.calls.push(None);
            }
        },
        CallAction::CaptureError => {
            // The B call emits this action only when the original IR has an error result.
            e.store(call.error.unwrap_or_default());
            return e.jump(Branch::Nonzero64) as u64;
        }
        CallAction::ReadIndirect | CallAction::ReadInteger => {
            // A payload action is reachable only for the nonempty original result type.
            let ty = unsafe { &*state.result };
            let mode = if matches!(action, CallAction::ReadIndirect) { AbiMemoryMode::StackLoad }
                else { AbiMemoryMode::UnpackIntegerResult };
            e.abi_value(offset, ty, &call.values, mode);
        }
        CallAction::ReadFloat => e.store_float(call.values[0]),
        CallAction::CloseError => e.patch(value as usize, e.code.len()),
    }
    0
}

const CALL_HOST: CallHost = CallHost {
    argument: emission_argument, place: place_argument, action: call_action,
};

impl Emitter<'_> {
    pub(super) fn abi_value(
        &mut self, slot: u32, ty: &Ty, values: &[Reg], mode: AbiMemoryMode,
    ) {
        let count = ty.components.len().min(values.len());
        let per_component = match mode {
            AbiMemoryMode::PackIntegerResult | AbiMemoryMode::UnpackIntegerResult => 48,
            _ => 24,
        };
        self.code.reserve(16 + per_component * count);
        let start = self.code.len();
        let input = AbiValue {
            allocation: &self.allocation,
            registers: &self.stage.registers,
            components: &ty.components,
            values,
        };
        let context = (&input as *const AbiValue<'_>).cast();
        unsafe {
            let output = self.code.as_mut_ptr().add(start);
            let length = encode_abi_value(slot, count as u64, mode, context, abi_component, output);
            self.code.set_len(start + length as usize);
        }
    }

    fn abi_stack_pointer(&mut self, offset: u32, slot: u32) {
        self.emit_bytes(16, |output| unsafe { encode_abi_stack_pointer(offset, slot, output) });
    }

    pub(super) fn argument_register(&mut self, slot: usize, float_size: u32, to_home: bool) {
        self.emit_bytes(9, |output| unsafe {
            encode_argument_register(slot as u8, float_size, u32::from(to_home), output)
        });
    }
    pub(super) fn reference_address(&mut self, address: &CallReference) -> Result<(), &'static str> {
        match &address.storage {
            CallStorage::Local { base, len } => {
                let offset = if *len == 0 { 0 } else {
                    let Location::Stack(offset) = self.allocation.location(PrivateSlot::Local(*base))
                    else { return Err("C ABI 局部借用没有实际栈地址"); };
                    offset
                };
                self.borrow_address(4, offset + address.word, address.offset);
            }
            CallStorage::Indexed(source) => {
                let word = if let Some(offset) = address.offset {
                    self.indexed_address(source, offset, address.word);
                    0
                } else {
                    if matches!(source, IndexedSource::Tile { .. }) {
                        return Err("C ABI 不接受块内分量存储");
                    }
                    self.indexed_base(source);
                    address.word
                };
                self.borrow_address(15, word, None);
            }
        }
        Ok(())
    }
    pub(super) fn emit_symbol_target(&mut self, target: NativeSymbol, invoke: bool) {
        self.code.reserve(12);
        let start = self.code.len();
        let encoding = unsafe {
            let encoding = encode_symbol_target(u32::from(invoke), self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoding.length as usize);
            encoding
        };
        let offset = start + encoding.field as usize;
        self.relocations.push(Relocation { offset, target });
        if invoke {
            self.calls.push(Some(offset));
        }
    }
    pub(super) fn native_call(&mut self, call: &Call) -> Result<(), &'static str> {
        let indirect_target = match call.target { CallTarget::Indirect(reg) => reg, _ => 0 };
        let mut state = CallEmission {
            emitter: self, call, result: call.result.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
            indirect_target, failure: None,
        };
        unsafe { emit_call(std::ptr::from_mut(&mut state).cast(), call.arguments.len(),
            AbiType::result(call.result.as_ref()), call_flags(call), CALL_HOST); }
        state.failure.map_or(Ok(()), Err)
    }
}
