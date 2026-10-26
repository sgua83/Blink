//! Windows x64 C ABI：参数先进入实际调用槽，避免设置参数寄存器时覆盖尚未消费的值。
use super::operands::Operand;
use super::*;
use allocation::Location;
pub(super) struct Layout {
    pub bytes: u64,
    pub result: Option<u64>,
    pub copies: Vec<Option<u64>>,
    pub hidden: bool,
}
fn indirect(ty: &Ty) -> bool {
    !matches!(align_up(ty.size, ty.align), 4 | 8)
}
fn floating(ty: &Ty) -> bool {
    *ty == Ty::scalar(Scalar::F32)
}
impl Layout {
    pub(super) fn new(call: &ExternalCall) -> Self {
        let hidden = call.result.as_ref().is_some_and(indirect);
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
                if let ExternalArgument::Value { ty, .. } = arg
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
        Self {
            bytes: bytes.div_ceil(16) * 16,
            result,
            copies,
            hidden,
        }
    }
}
impl Emitter<'_> {
    fn stack_address(&mut self, offset: u32) {
        self.instruction(None, &[0x8d], true, 0, Operand::memory(4, offset));
    }
    fn reference_address(&mut self, address: &ExternalReference) -> Result<(), &'static str> {
        match &address.storage {
            ExternalStorage::Local { base, .. } => {
                let Location::Stack(offset) = self.allocation.location(PrivateSlot::Local(*base))
                else {
                    return Err("C ABI 局部借用没有实际栈地址");
                };
                self.stack_address(offset + address.word * 4);
                if let Some(index) = address.offset {
                    self.bytes(&[0x49, 0x89, 0xc2]);
                    self.load(index);
                    self.bytes(&[0x48, 0xc1, 0xe0, 2, 0x4c, 0x01, 0xd0]);
                }
            }
            ExternalStorage::Indexed(source) => {
                if let Some(offset) = address.offset {
                    self.indexed_address(source, offset, address.word);
                } else {
                    match source {
                        IndexedSource::Data { binding, index } => {
                            self.address(*binding, *index);
                            if address.word != 0 {
                                self.bytes(&[0x49, 0x81, 0xc7]);
                                self.u32(address.word * 4);
                            }
                        }
                        IndexedSource::View {
                            address: parts,
                            index,
                            stride,
                        } => {
                            self.load(parts[1]);
                            self.bytes(&[0x41, 0x89, 0xc7, 0x49, 0xc1, 0xe7, 32]);
                            self.load(parts[0]);
                            self.bytes(&[0x49, 0x09, 0xc7]);
                            self.load(*index);
                            self.offset_address(*stride);
                            if address.word != 0 {
                                self.bytes(&[0x49, 0x81, 0xc7]);
                                self.u32(address.word * 4);
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
    pub(super) fn external_call(&mut self, call: &ExternalCall) -> Result<(), &'static str> {
        let layout = Layout::new(call);
        if let Some(at) = layout.result {
            self.stack_address(at as u32);
            self.instruction(None, &[0x89], true, 0, Operand::memory(4, 0));
        }
        for (i, arg) in call.arguments.iter().enumerate() {
            let slot = (i + usize::from(layout.hidden)) as u32 * 8;
            match arg {
                ExternalArgument::Reference { address, .. } => {
                    self.reference_address(address)?;
                    self.instruction(None, &[0x89], true, 0, Operand::memory(4, slot));
                }
                ExternalArgument::Value { ty, values } => {
                    let base = layout.copies[i].map_or(slot, |at| at as u32);
                    for ((_, offset), value) in ty.components.iter().zip(values) {
                        self.load(*value);
                        self.instruction(
                            None,
                            &[0x89],
                            false,
                            0,
                            Operand::memory(4, base + *offset),
                        );
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
            if matches!(arg,ExternalArgument::Value{ty,..} if floating(ty)) {
                self.instruction(Some(0xf3), &[0x0f, 0x10], false, slot as u8, memory);
            } else {
                self.instruction(None, &[0x8b], true, [1, 2, 8, 9][slot], memory);
            }
        }
        self.bytes(&[0x48, 0xb8]);
        self.relocations.push(Relocation {
            offset: self.code.len(),
            target: NativeSymbol::External(call.function.clone()),
        });
        self.bytes(&[0; 8]);
        self.bytes(&[0xff, 0xd0]);
        if let Some(ty) = &call.result {
            if let Some(at) = layout.result {
                for ((_, offset), reg) in ty.components.iter().zip(&call.values) {
                    self.read_scalar(*reg, 4, at as u32 + *offset);
                }
            } else if floating(ty) {
                self.store_float(call.values[0]);
            } else {
                self.bytes(&[0x49, 0x89, 0xc2]);
                for ((scalar, offset), reg) in ty.components.iter().zip(&call.values) {
                    self.bytes(&[0x4c, 0x89, 0xd0]);
                    if *offset != 0 {
                        self.bytes(&[0x48, 0xc1, 0xe8, 32]);
                    }
                    if *scalar == Scalar::Bool {
                        self.bytes(&[0x85, 0xc0, 0x0f, 0x95, 0xc0, 0x0f, 0xb6, 0xc0]);
                    }
                    self.store(*reg);
                }
            }
        }
        Ok(())
    }
}
