//! Windows x64普通B与C ABI：实际机器参数先进入调用槽，再交给参数寄存器。
use super::operands::MoveLocation;
use super::*;
use crate::builtin_types::ScalarEncoding;
use allocation::Location;
use std::ffi::c_void;
pub(super) struct Layout {
    pub bytes: u64,
    pub result: Option<u64>,
    copies: u64,
    pub hidden: bool,
    target: Option<u64>,
}
#[repr(C)]
pub(super) struct AbiType { size: u32, alignment: u32, value: u64 }
impl AbiType {
    fn value(ty: &Ty) -> Self { Self { size: ty.size, alignment: ty.align, value: 1 } }

    pub(super) fn result(ty: Option<&Ty>) -> Self {
        ty.map_or(Self { size: 0, alignment: 1, value: 0 }, Self::value)
    }
}
#[repr(C)]
struct CallPlan { bytes: u64, result: u64, copies: u64, target: u64, hidden: u64 }
#[repr(C)]
struct ValueCopy { offset: u64, end: u64, present: u64 }
#[repr(C)]
struct SymbolEncoding { length: u64, field: u64 }
#[repr(u32)]
pub(super) enum AbiMemoryMode {
    StackLoad,
    StackStore,
    IndirectParameter,
    IndirectResult,
    FallibleResult,
}

#[repr(C)]
struct AbiMemoryComponent {
    location: MoveLocation,
    scalar: ScalarEncoding,
    offset: u32,
}

struct MemoryValue<'a> {
    allocation: &'a Allocation,
    registers: &'a [Scalar],
    components: &'a [(Scalar, u32)],
    values: &'a [Reg],
}

unsafe extern "C" fn memory_component(context: *const c_void, index: u64) -> AbiMemoryComponent {
    let input = unsafe { &*context.cast::<MemoryValue<'_>>() };
    let index = index as usize;
    let reg = input.values[index];
    AbiMemoryComponent {
        location: input.allocation.location(PrivateSlot::Register(reg)).into(),
        scalar: input.registers[reg].into(),
        offset: input.components[index].1,
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::abi_memory_value"]
    fn encode_abi_memory_value(slot: u32, count: u64, mode: AbiMemoryMode, context: *const c_void,
        component: unsafe extern "C" fn(*const c_void, u64) -> AbiMemoryComponent, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::abi_stack_pointer"]
    fn encode_abi_stack_pointer(offset: u32, slot: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.call_layout::indirect"]
    fn indirect_value(size: u32, alignment: u32) -> bool;
    #[link_name = "bootstrap.call_layout::copy_value"]
    fn copy_value(start: u64, size: u32, alignment: u32) -> ValueCopy;
    #[link_name = "bootstrap.x64_instruction::argument_register"]
    fn encode_argument_register(slot: u8, float_size: u32, to_home: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::symbol_target"]
    fn encode_symbol_target(invoke: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::call_slot"]
    fn encode_call_slot(offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::pack_integer_component"]
    fn encode_pack_integer_component(scalar: ScalarEncoding, offset: u32,
        merge: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::unpack_integer_component"]
    fn encode_unpack_integer_component(scalar: ScalarEncoding, offset: u32,
        packed: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.call_layout::plan"]
    fn call_plan(context: *const Call, count: u64, result: AbiType, flags: u32,
        argument: unsafe extern "C" fn(*const Call, u64) -> AbiType) -> CallPlan;
}
unsafe extern "C" fn argument_type(call: *const Call, index: u64) -> AbiType {
    match &unsafe { &*call }.arguments[index as usize] {
        CallArgument::Value { ty, .. } => AbiType::value(ty),
        CallArgument::Reference { .. } => AbiType { size: 0, alignment: 1, value: 0 },
    }
}
pub(super) fn indirect(ty: &Ty) -> bool {
    unsafe { indirect_value(ty.size, ty.align) }
}
pub(super) fn float_size(ty: &Ty) -> u32 {
    if *ty == Ty::scalar(Scalar::F32) {
        4
    } else if *ty == Ty::scalar(Scalar::F64) {
        8
    } else {
        0
    }
}
impl Layout {
    pub(super) fn new(call: &Call) -> Self {
        let result = AbiType::result(call.result.as_ref());
        let flags = u32::from(call.error.is_some()) | (u32::from(matches!(call.target, CallTarget::Indirect(_))) << 1);
        let plan = unsafe { call_plan(call, call.arguments.len() as u64, result, flags, argument_type) };
        let hidden = plan.hidden != 0;
        Self { bytes: plan.bytes, result: hidden.then_some(plan.result), copies: plan.copies,
            hidden, target: (flags & 2 != 0).then_some(plan.target) }
    }
}
impl Emitter<'_> {
    pub(super) fn abi_memory_value(
        &mut self, slot: u32, ty: &Ty, values: &[Reg], mode: AbiMemoryMode,
    ) {
        let count = ty.components.len().min(values.len());
        self.code.reserve(16 + 24 * count);
        let start = self.code.len();
        let input = MemoryValue {
            allocation: &self.allocation,
            registers: &self.stage.registers,
            components: &ty.components,
            values,
        };
        let context = (&input as *const MemoryValue<'_>).cast();
        unsafe {
            let output = self.code.as_mut_ptr().add(start);
            let length = encode_abi_memory_value(slot, count as u64, mode, context, memory_component, output);
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
    pub(super) fn pack_integer_result(&mut self, ty: &Ty, outputs: &[Reg]) {
        let components = ty.components.iter().zip(outputs);
        let count = components.len();
        if count == 0 {
            self.copy_scalar(Location::Gpr(0), Location::Immediate(0), Scalar::U32);
            return;
        }
        for (index, ((scalar, offset), reg)) in components.enumerate() {
            self.load(*reg);
            self.emit_bytes(12, |output| unsafe {
                encode_pack_integer_component((*scalar).into(), *offset, u32::from(index != 0), output)
            });
            if index == 0 && count > 1 {
                self.copy_scalar(Location::Gpr(1), Location::Gpr(0), Scalar::U64);
            }
        }
        if count > 1 {
            self.copy_scalar(Location::Gpr(0), Location::Gpr(1), Scalar::U64);
        }
    }
    fn unpack_integer_result(&mut self, ty: &Ty, outputs: &[Reg]) {
        let components = ty.components.iter().zip(outputs);
        let packed = components.len() > 1;
        if packed {
            self.copy_scalar(Location::Gpr(1), Location::Gpr(0), Scalar::U64);
        }
        for ((scalar, offset), reg) in components {
            self.emit_bytes(15, |output| unsafe {
                encode_unpack_integer_component((*scalar).into(), *offset, u32::from(packed), output)
            });
            self.store(*reg);
        }
    }
    pub(super) fn native_call(&mut self, call: &Call) -> Result<(), &'static str> {
        let layout = Layout::new(call);
        if let CallTarget::Indirect(reg) = call.target {
            self.copy_scalar(Location::Stack(layout.target.unwrap() as u32), self.location(reg), Scalar::U64);
        }
        if let Some(at) = layout.result {
            self.abi_stack_pointer(at as u32, 0);
        }
        let arguments = call.arguments.iter()
            .filter(|arg| !matches!(arg, CallArgument::Value { ty, .. } if ty.size == 0));
        let mut copies = layout.copies;
        for (slot, arg) in arguments.clone().enumerate() {
            let slot = (slot + usize::from(layout.hidden)) as u32 * 8;
            match arg {
                CallArgument::Reference { address, .. } => {
                    self.reference_address(address)?;
                    self.copy_scalar(Location::Stack(slot), Location::Gpr(0), Scalar::U64);
                }
                CallArgument::Value { ty, values, .. } => {
                    let copy = unsafe { copy_value(copies, ty.size, ty.align) };
                    copies = copy.end;
                    let base = if copy.present != 0 { copy.offset as u32 } else { slot };
                    self.abi_memory_value(base, ty, values, AbiMemoryMode::StackStore);
                    if copy.present != 0 {
                        self.abi_stack_pointer(copy.offset as u32, slot);
                    }
                }
            }
        }
        if layout.hidden {
            self.argument_register(0, 0, false);
        }
        for (slot, arg) in arguments.enumerate().take(4 - usize::from(layout.hidden)) {
            let slot = slot + usize::from(layout.hidden);
            let size = match arg {
                CallArgument::Value { ty, .. } => float_size(ty),
                CallArgument::Reference { .. } => 0,
            };
            self.argument_register(slot, size, false);
        }
        match &call.target {
            CallTarget::Indirect(_) => {
                let offset = layout.target.unwrap() as u32;
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
        if let Some(ty) = call.result.as_ref().filter(|ty| ty.size != 0) {
            if let Some(at) = layout.result {
                self.abi_memory_value(at as u32, ty, &call.values, AbiMemoryMode::StackLoad);
            } else if float_size(ty) != 0 {
                self.store_float(call.values[0]);
            } else {
                self.unpack_integer_result(ty, &call.values);
            }
        }
        if let Some(at) = failed { self.patch(at, self.code.len()); }
        Ok(())
    }
}
