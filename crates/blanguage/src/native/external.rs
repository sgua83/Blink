//! Windows x64 C ABI：参数先进入实际调用槽，避免设置参数寄存器时覆盖尚未消费的值。
use super::operands::Operand;
use super::*;
use allocation::Location;
pub(super) struct Layout {
    pub bytes: u64,
    pub result: Option<u64>,
    pub copies: Vec<Option<u64>>,
    pub hidden: bool,
    target: Option<u64>,
}
pub(super) fn indirect(ty: &Ty) -> bool {
    !matches!(align_up(ty.size, ty.align), 1 | 2 | 4 | 8)
}
pub(super) fn floating(ty: &Ty) -> bool {
    *ty == Ty::scalar(Scalar::F32) || *ty == Ty::scalar(Scalar::F64)
}
impl Layout {
    pub(super) fn new(call: &Call) -> Self {
        let hidden = call.result.as_ref().is_some_and(|ty| call.error.is_some() || indirect(ty));
        let mut bytes =
            ((call.arguments.len() + usize::from(hidden)).max(4) as u64 * 8).div_ceil(16) * 16;
        let result = if hidden {
            let at = bytes;
            let ty = call.result.as_ref().unwrap();
            bytes += align_up(ty.size, ty.align) as u64;
            Some(at)
        } else {
            None
        };
        let mut copies = Vec::new();
        for arg in &call.arguments {
            copies.push(
                if let CallArgument::Value { ty, .. } = arg
                    && indirect(ty)
                {
                    bytes = bytes.div_ceil(16) * 16;
                    let at = bytes;
                    bytes += align_up(ty.size, ty.align) as u64;
                    Some(at)
                } else {
                    None
                },
            );
        }
        let target = if matches!(call.target, CallTarget::Indirect(_)) {
            bytes = bytes.div_ceil(8) * 8;
            let at = bytes;
            bytes += 8;
            Some(at)
        } else { None };
        Self {
            bytes: bytes.div_ceil(16) * 16,
            result,
            copies,
            hidden,
            target,
        }
    }
}
impl Emitter<'_> {
    fn stack_address(&mut self, offset: u32) {
        self.instruction(None, &[0x8d], true, 0, Operand::memory(4, offset));
    }
    pub(super) fn reference_address(&mut self, address: &CallReference) -> Result<(), &'static str> {
        match &address.storage {
            CallStorage::Local { base, len } => {
                let offset = if *len == 0 { 0 } else {
                    let Location::Stack(offset) = self.allocation.location(PrivateSlot::Local(*base))
                    else { return Err("C ABI 局部借用没有实际栈地址"); };
                    offset
                };
                self.stack_address(offset + address.word);
                if let Some(index) = address.offset {
                    // ABI argument registers are populated only after every call slot is ready.
                    self.bytes(&[0x48, 0x89, 0xc1]);
                    self.load(index);
                    self.bytes(&[0x48, 0x01, 0xc8]);
                }
            }
            CallStorage::Indexed(source) => {
                if let Some(offset) = address.offset {
                    self.indexed_address(source, offset, address.word);
                } else {
                    match source {
                        IndexedSource::Data { binding, index } => {
                            self.address(*binding, *index);
                            if address.word != 0 {
                                self.bytes(&[0x49, 0x81, 0xc7]);
                                self.u32(address.word);
                            }
                        }
                        IndexedSource::View { .. }
                        | IndexedSource::Pointer { .. } => {
                            self.indexed_base(source);
                            if address.word != 0 {
                                self.bytes(&[0x49, 0x81, 0xc7]);
                                self.u32(address.word);
                            }
                        }
                        IndexedSource::Tile { .. } => return Err("C ABI 不接受块内分量存储"),
                    }
                }
                self.bytes(&[0x4c, 0x89, 0xf8]);
            }
        }
        Ok(())
    }
    pub(super) fn native_code_address(&mut self, target: NativeSymbol) -> usize {
        self.bytes(&[0x48, 0xb8]);
        let at = self.code.len();
        self.relocations.push(Relocation { offset: at, target });
        self.bytes(&[0; 8]);
        at
    }
    pub(super) fn invoke(&mut self, reference: Option<usize>) {
        self.calls.push(reference);
        self.bytes(&[0xff, 0xd0]);
    }
    pub(super) fn native_call(&mut self, call: &Call) -> Result<(), &'static str> {
        let layout = Layout::new(call);
        if let CallTarget::Indirect(reg) = call.target {
            self.load(reg);
            self.instruction(None, &[0x89], true, 0, Operand::memory(4, layout.target.unwrap() as u32));
        }
        if let Some(at) = layout.result {
            self.stack_address(at as u32);
            self.instruction(None, &[0x89], true, 0, Operand::memory(4, 0));
        }
        for (i, arg) in call.arguments.iter().enumerate() {
            let slot = (i + usize::from(layout.hidden)) as u32 * 8;
            match arg {
                CallArgument::Reference { address, .. } => {
                    self.reference_address(address)?;
                    self.instruction(None, &[0x89], true, 0, Operand::memory(4, slot));
                }
                CallArgument::Value { ty, values, .. } => {
                    let base = layout.copies[i].map_or(slot, |at| at as u32);
                    for ((_, offset), value) in ty.components.iter().zip(values) {
                        self.write_scalar(*value, 4, base + *offset);
                    }
                    if let Some(at) = layout.copies[i] {
                        self.stack_address(at as u32);
                        self.instruction(None, &[0x89], true, 0, Operand::memory(4, slot));
                    }
                }
            }
        }
        if layout.hidden {
            self.instruction(None, &[0x8b], true, 1, Operand::memory(4, 0));
        }
        for (i, arg) in call.arguments.iter().enumerate() {
            let slot = i + usize::from(layout.hidden);
            if slot >= 4 {
                continue;
            }
            let memory = Operand::memory(4, slot as u32 * 8);
            if let CallArgument::Value { ty, .. } = arg && floating(ty) {
                self.instruction(Some(if ty == &Ty::scalar(Scalar::F64) { 0xf2 } else { 0xf3 }), &[0x0f, 0x10], false, slot as u8, memory);
            } else {
                self.instruction(None, &[0x8b], true, [1, 2, 8, 9][slot], memory);
            }
        }
        let reference = match &call.target {
            CallTarget::Indirect(_) => { self.instruction(None, &[0x8b], true, 0, Operand::memory(4, layout.target.unwrap() as u32)); None }
            CallTarget::External(name) => Some(self.native_code_address(NativeSymbol::External(name.clone()))),
            CallTarget::Defined(name) => Some(self.native_code_address(NativeSymbol::Function(name.clone()))),
        };
        self.invoke(reference);
        let failed = if let Some(error) = call.error {
            self.store(error);
            self.bytes(&[0x48, 0x85, 0xc0]);
            Some(self.jump(Some(0x85)))
        } else { None };
        if let Some(ty) = &call.result {
            if let Some(at) = layout.result {
                for ((_, offset), reg) in ty.components.iter().zip(&call.values) {
                    self.read_scalar(*reg, 4, at as u32 + *offset);
                }
            } else if floating(ty) {
                self.store_float(call.values[0]);
            } else {
                // After the call, RCX preserves all packed return bits while RAX delivers each word.
                self.bytes(&[0x48, 0x89, 0xc1]);
                for ((scalar, offset), reg) in ty.components.iter().zip(&call.values) {
                    self.bytes(&[0x48, 0x89, 0xc8]);
                    if *offset != 0 {
                        self.bytes(&[0x48, 0xc1, 0xe8, (*offset * 8) as u8]);
                    }
                    if *scalar == Scalar::Bool {
                        self.bytes(&[0x85, 0xc0, 0x0f, 0x95, 0xc0, 0x0f, 0xb6, 0xc0]);
                    }
                    self.store(*reg);
                }
            }
        }
        if let Some(at) = failed { self.patch(at, self.code.len()); }
        Ok(())
    }
}
