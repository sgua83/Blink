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
struct CallPlan {
    bytes: u64,
    result: u64,
    target: u64,
    arguments: usize,
    result_location: ResultLocation,
}

#[repr(C)]
struct ArgumentSlot { index: usize, home: u64, payload: u64, indirect: u32, float_size: u32 }
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
    #[link_name = "bootstrap.call_layout::plan"]
    fn call_plan(context: *const c_void, count: usize, result: AbiType, flags: u32,
        argument: unsafe extern "C" fn(*const c_void, usize) -> AbiType, positions: *mut ArgumentSlot) -> CallPlan;
}
unsafe extern "C" fn argument_type(context: *const c_void, index: usize) -> AbiType {
    match &unsafe { &*context.cast::<Call>() }.arguments[index] {
        CallArgument::Value { ty, .. } => AbiType::value(ty),
        CallArgument::Reference { .. } => AbiType::reference(),
    }
}
fn plan_call(call: &Call, positions: *mut ArgumentSlot) -> CallPlan {
    let result = AbiType::result(call.result.as_ref());
    let flags = u32::from(call.error.is_some()) | (u32::from(matches!(call.target, CallTarget::Indirect(_))) << 1);
    let context = (call as *const Call).cast();
    unsafe { call_plan(context, call.arguments.len(), result, flags, argument_type, positions) }
}

pub(super) fn outgoing_bytes(call: &Call) -> u64 {
    plan_call(call, std::ptr::null_mut()).bytes
}
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
        let mut arguments = Vec::with_capacity(call.arguments.len());
        let layout = plan_call(call, arguments.as_mut_ptr());
        // B initialized only the rows with physical argument slots.
        unsafe { arguments.set_len(layout.arguments); }
        if let CallTarget::Indirect(reg) = call.target {
            self.copy_scalar(Location::Stack(layout.target as u32), self.location(reg), Scalar::U64);
        }
        if layout.result_location == ResultLocation::Indirect {
            self.abi_stack_pointer(layout.result as u32, 0);
        }
        for row in &arguments {
            let home = row.home as u32;
            match &call.arguments[row.index] {
                CallArgument::Reference { address, .. } => {
                    self.reference_address(address)?;
                    self.copy_scalar(Location::Stack(home), Location::Gpr(0), Scalar::U64);
                }
                CallArgument::Value { ty, values, .. } => {
                    self.abi_value(row.payload as u32, ty, values, AbiMemoryMode::StackStore);
                    if row.indirect != 0 {
                        self.abi_stack_pointer(row.payload as u32, home);
                    }
                }
            }
        }
        if layout.result_location == ResultLocation::Indirect {
            self.argument_register(0, 0, false);
        }
        for row in arguments.iter().take_while(|row| row.home < 32) {
            self.argument_register((row.home / 8) as usize, row.float_size, false);
        }
        match &call.target {
            CallTarget::Indirect(_) => {
                let offset = layout.target as u32;
                self.emit_bytes(7, |output| unsafe { encode_call_slot(offset, output) });
                self.calls.push(None);
            }
            CallTarget::External(name) => self.emit_symbol_target(NativeSymbol::External(name.clone()), true),
            CallTarget::Defined(name) => self.emit_symbol_target(NativeSymbol::Function(name.clone()), true),
        }
        let failed = if let Some(error) = call.error {
            self.store(error);
            Some(self.jump(Branch::Nonzero64))
        } else { None };
        if let Some(ty) = &call.result {
            match layout.result_location {
                ResultLocation::Indirect =>
                    self.abi_value(layout.result as u32, ty, &call.values, AbiMemoryMode::StackLoad),
                ResultLocation::Float => self.store_float(call.values[0]),
                ResultLocation::Integer =>
                    self.abi_value(0, ty, &call.values, AbiMemoryMode::UnpackIntegerResult),
                ResultLocation::None => {}
            }
        }
        if let Some(at) = failed { self.patch(at, self.code.len()); }
        Ok(())
    }
}
