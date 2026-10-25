//! 固定数组按值布局；动态下标由范围事实证明，直接生成地址访问。
use super::*;
impl Place {
    pub(super) fn project(self, ty: Ty, components: &[usize]) -> Self {
        match self {
            Self::AtomicParameter(_) => Self::AtomicParameter(ty),
            Self::Local(_, slots) => {
                Self::Local(ty, components.iter().map(|i| slots[*i]).collect())
            }
            Self::ReadOnlyLocal(_, slots) => {
                Self::ReadOnlyLocal(ty, components.iter().map(|i| slots[*i]).collect())
            }
            Self::Indexed {
                base,
                offset,
                words,
                ..
            } => Self::Indexed {
                ty,
                base,
                offset,
                words: components.iter().map(|i| words[*i]).collect(),
            },
            Self::Element {
                binding,
                index,
                words,
                writable,
                ..
            } => Self::Element {
                ty,
                binding,
                index,
                words: components.iter().map(|i| words[*i]).collect(),
                writable,
            },
            Self::ViewElement {
                address,
                index,
                stride,
                words,
                ..
            } => Self::ViewElement {
                ty,
                address,
                index,
                stride,
                words: components.iter().map(|i| words[*i]).collect(),
            },
            Self::TileElement {
                tile, index, words, ..
            } => Self::TileElement {
                ty,
                tile,
                index,
                words: components.iter().map(|i| words[*i]).collect(),
            },
        }
    }
    pub(super) fn storage(&self) -> &Place {
        if let Self::Indexed { base, .. } = self {
            base
        } else {
            self
        }
    }
}
impl Builder<'_> {
    pub(super) fn materialize_array_bindings(&mut self) {
        let values: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| {
                if let Binding::Value(value) = binding
                    && value.ty.contains_arrays()
                {
                    Some((name.clone(), value.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (name, value) in values {
            let proof = bounds::Proof::capture(&value);
            let uniform = value.uniform;
            let place = self.materialize_array(value);
            self.env.insert(
                name,
                Binding::Owned(OwnedBinding {
                    place,
                    proof,
                    uniform,
                    initialization: None,
                }),
            );
        }
    }

    pub(super) fn materialize_array(&mut self, value: Value) -> Place {
        let slots = value
            .regs
            .iter()
            .map(|r| {
                let slot = self.locals.len();
                self.locals.push(self.registers[*r]);
                self.code.push(Inst::StoreLocal(slot, *r));
                slot
            })
            .collect();
        Place::ReadOnlyLocal(value.ty, slots)
    }
    pub(super) fn array_place(&mut self, place: Place, expr: &Expr) -> Result<Place> {
        let index = self.expr(expr)?;
        self.array_index(place, index, expr.span)
    }
    pub(super) fn array_value_index(&mut self, value: Value, expr: &Expr) -> Result<Value> {
        let index = self.expr(expr)?;
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, expr.span)?;
        let ty = (**value.ty.array.as_ref().unwrap()).clone();
        if !self.index_in(
            &index,
            IndexDomain::Fixed(value.ty.values[0]),
            value.ty.values[0],
        ) {
            return self.fail(expr.span, "B137", "数组下标有效性无法静态证明");
        }
        if let Some(bits) = &index.constant {
            let begin = bits[0] as usize * ty.components.len();
            let components: Vec<_> = (begin..begin + ty.components.len()).collect();
            return Ok(Value {
                proof: value.projected_proof(&components),
                literal: false,
                ty,
                regs: components.iter().map(|i| value.regs[*i]).collect(),
                uniform: value.uniform,
                constant: value
                    .constant
                    .map(|b| components.iter().map(|i| b[*i]).collect()),
            });
        }
        let place = self.materialize_array(value);
        let place = self.array_index(place, index, expr.span)?;
        self.read_place(&place, expr.span)
    }
    fn array_index(&mut self, place: Place, index: Value, span: Span) -> Result<Place> {
        let element = (**place.ty().array.as_ref().unwrap()).clone();
        let count = place.ty().values[0];
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, span)?;
        if !self.index_in(&index, IndexDomain::Fixed(count), count) {
            return self.fail(
                span,
                "B137",
                "数组下标有效性无法静态证明；使用常量下标、有界循环或小于数组 length 的条件",
            );
        }
        if let Some(bits) = &index.constant {
            let first = bits[0] as usize * element.components.len();
            let components: Vec<_> = (first..first + element.components.len()).collect();
            return Ok(place.project(element, &components));
        }
        match place.storage() {
            Place::Local(..) => self.require_initialized(&place, span)?,
            Place::Element { binding, .. }
                if matches!(self.view_access[binding], Access::Write | Access::InOut) =>
            {
                self.require_initialized(&place, span)?
            }
            _ => {}
        }
        let packed = matches!(place.storage(), Place::Local(..) | Place::ReadOnlyLocal(..));
        let stride = if packed {
            element.components.len() as u32
        } else {
            ir::align_up(element.size, element.align) / 4
        };
        let factor = self.constant(Scalar::U32, stride);
        let relative = self.binary("*", index, factor, span)?.regs[0];
        let (base, offset, first) = match place {
            Place::Indexed {
                base,
                offset,
                words,
                ..
            } => {
                let dst = self.reg(Scalar::U32);
                self.code.push(Inst::Binary {
                    dst,
                    op: BinaryOp::IAdd,
                    a: offset,
                    b: relative,
                });
                (base, dst, words[0])
            }
            p => {
                let first = match &p {
                    Place::Element { words, .. }
                    | Place::ViewElement { words, .. }
                    | Place::TileElement { words, .. } => words[0],
                    _ => 0,
                };
                (Box::new(p), relative, first)
            }
        };
        let words = element
            .components
            .iter()
            .enumerate()
            .map(|(i, (_, o))| first + if packed { i as u32 } else { o / 4 })
            .collect();
        Ok(Place::Indexed {
            ty: element,
            base,
            offset,
            words,
        })
    }
    fn array_memory(&self, base: &Place) -> IndexedSource {
        match base {
            Place::Element { binding, index, .. } => IndexedSource::Data {
                binding: *binding,
                index: *index,
            },
            Place::ViewElement {
                address,
                index,
                stride,
                ..
            } => IndexedSource::View {
                address: *address,
                index: *index,
                stride: *stride,
            },
            Place::TileElement { tile, index, .. } => IndexedSource::Tile {
                tile: *tile,
                index: *index,
            },
            _ => unreachable!(),
        }
    }
    pub(super) fn read_array_place(&mut self, place: &Place, span: Span) -> Result<Value> {
        if self.atomic_storage(place) {
            return self.fail(span, "B214", "atomic 数组元素只能通过显式原子操作读取");
        }
        let Place::Indexed {
            ty,
            base,
            offset,
            words,
        } = place
        else {
            unreachable!()
        };
        match &**base {
            Place::Local(..) => self.require_initialized(place, span)?,
            Place::Element { binding, index, .. } => {
                self.reads.insert(*binding);
                if *index != self.global_index {
                    self.cross_access.insert(*binding);
                }
                if self.view_access[binding].writes() {
                    self.require_initialized(place, span)?;
                }
            }
            Place::TileElement { tile, .. } => {
                if let TileOrigin::Bound(TileSource::Input { binding, .. }) =
                    self.tiles[*tile].source
                {
                    self.reads.insert(binding);
                    self.cross_access.insert(binding);
                }
            }
            _ => {}
        }
        let mut regs = Vec::new();
        for ((scalar, _), word) in ty.components.iter().zip(words) {
            let dst = self.reg(*scalar);
            self.code.push(match &**base {
                Place::Local(_, slots) | Place::ReadOnlyLocal(_, slots) => Inst::LoadLocalIndexed {
                    dst,
                    base: slots[0],
                    len: slots.len(),
                    index: *offset,
                    word: *word,
                },
                _ => Inst::LoadIndexed {
                    dst,
                    source: self.array_memory(base),
                    offset: *offset,
                    word: *word,
                },
            });
            regs.push(dst);
        }
        let (proof, uniform) = self.read_owned_proof(place, &regs);
        Ok(Value {
            proof,
            literal: false,
            ty: ty.clone(),
            regs,
            uniform,
            constant: None,
        })
    }
    pub(super) fn write_array_place(
        &mut self,
        place: &Place,
        value: Value,
        span: Span,
    ) -> Result<()> {
        let Place::Indexed {
            base,
            offset,
            words,
            ..
        } = place
        else {
            unreachable!()
        };
        if self.atomic_storage(place) {
            return self.fail(span, "B214", "atomic 数组元素只能通过显式原子操作修改");
        }
        self.writable_storage(place, span)?;
        if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
            return self.fail(span, "B143", "赋值位置包含重复分量");
        }
        self.assign_owned_proof(place, bounds::Proof::capture(&value), false);
        self.initialize(place);
        for (word, value) in words.iter().zip(value.regs) {
            self.code.push(match &**base {
                Place::Local(_, slots) => Inst::StoreLocalIndexed {
                    base: slots[0],
                    len: slots.len(),
                    index: *offset,
                    word: *word,
                    value,
                },
                Place::Element { binding, .. } => {
                    self.writes.insert(*binding);
                    Inst::StoreIndexed {
                        source: self.array_memory(base),
                        offset: *offset,
                        word: *word,
                        value,
                    }
                }
                _ => unreachable!(),
            });
        }
        Ok(())
    }
}
