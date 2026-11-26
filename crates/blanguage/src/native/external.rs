//! Windows x64普通B与C ABI：实际机器参数先进入调用槽，再交给参数寄存器。
use super::operands::{IndexedAddressHost, MoveLocation, ScalarLocations, value_local};
use crate::ir::dataflow::{Node, Span};
use super::*;
use crate::builtin_types::ScalarEncoding;
use allocation::Location;
use std::ffi::c_void;
#[repr(C)]
pub(super) struct CallValueLoan {
    components: Span,
    values: Span,
    allocation: *const Allocation,
    registers: *const c_void,
}

impl CallValueLoan {
    pub(super) fn new(fields: &[(Scalar, u32)], values: &[Reg], allocation: &Allocation, registers: &&[Scalar]) -> Self {
        Self {
            components: Span::slice(fields),
            values: Span::slice(values),
            allocation: std::ptr::from_ref(allocation),
            registers: std::ptr::from_ref(registers).cast(),
        }
    }
}

#[repr(C)]
struct CallInput {
    source: Node,
    results: Span,
    allocation: *const Allocation,
    registers: *const c_void,
}

#[repr(C)]
struct CallHost {
    addresses: IndexedAddressHost,
    local: unsafe extern "C" fn(*const c_void, usize) -> MoveLocation,
    component: AbiComponent,
    reserve: unsafe extern "C" fn(*mut c_void, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(*mut c_void, usize),
}

#[repr(C)]
struct SymbolEncoding { length: u64, field: u64 }

#[repr(C)]
pub(super) struct AbiValueComponent {
    pub(super) location: MoveLocation,
    pub(super) scalar: ScalarEncoding,
    pub(super) field_scalar: ScalarEncoding,
    pub(super) offset: u32,
}

pub(super) type AbiComponent = unsafe extern "C" fn(*const c_void, u64) -> AbiValueComponent;

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::print_call"]
    fn emit_print_call(dst: MoveLocation, src: MoveLocation, value: ScalarEncoding,
        result: ScalarEncoding, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::symbol_target"]
    fn encode_symbol_target(invoke: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.call_layout::emit_call"]
    fn emit_call(context: *mut c_void, input: CallInput, host: CallHost) -> usize;
}

pub(super) unsafe extern "C" fn abi_component(context: *const c_void, index: u64) -> AbiValueComponent {
    let loan = unsafe { &*context.cast::<CallValueLoan>() };
    let allocation = unsafe { &*loan.allocation };
    let reg = unsafe { loan.values.as_slice::<Reg>() }[index as usize];
    let (scalar, offset) = unsafe { loan.components.as_slice::<(Scalar, u32)>() }[index as usize];
    AbiValueComponent {
        location: allocation.location(PrivateSlot::Register(reg)).into(),
        scalar: unsafe { *loan.registers.cast::<&[Scalar]>() }[reg].into(),
        field_scalar: scalar.into(),
        offset,
    }
}

unsafe extern "C" fn reserve_call(context: *mut c_void, capacity: usize) -> NativeCodeBuffer {
    NativeCodeBuffer::reserve(unsafe { &mut *context.cast::<Vec<u8>>() }, capacity)
}

unsafe extern "C" fn commit_call(context: *mut c_void, end: usize) {
    unsafe { (*context.cast::<Vec<u8>>()).set_len(end) };
}

impl Emitter<'_> {
    pub(super) fn print_call(&mut self, dst: Reg, value: Reg) {
        let scalar = self.registers[value];
        let target = self.location(dst).into();
        let source = self.location(value).into();
        let start = self.code.len();
        self.code.reserve(84);
        let field = unsafe {
            let emission = emit_print_call(target, source, scalar.into(),
                self.registers[dst].into(), self.code.as_mut_ptr().add(start));
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
        let locations = ScalarLocations {
            allocation: &self.allocation, registers: self.registers, tiles: self.tiles,
            frame: &self.frame, workgroup: self.workgroup,
        };
        let host = CallHost {
            addresses: locations.indexed_host(), local: value_local,
            component: abi_component, reserve: reserve_call, commit: commit_call,
        };
        let input = CallInput {
            source: Node::call(call),
            results: Span::slice(&call.values),
            allocation: &self.allocation,
            registers: std::ptr::from_ref(&self.registers).cast(),
        };
        let field = unsafe { emit_call(std::ptr::from_mut(&mut self.code).cast(), input, host) };
        match &call.target {
            CallTarget::External(name) => self.record_symbol_target(field, NativeSymbol::External(name.clone()), true),
            CallTarget::Defined(name) => self.record_symbol_target(field, NativeSymbol::Function(name.clone()), true),
            CallTarget::Indirect(_) => self.calls.push(None),
        }
    }
}
