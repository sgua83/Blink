//! 固定数组按值布局；动态下标由范围事实证明，直接生成地址访问。
use super::*;
impl Place {
    pub(super) fn read_only(self) -> Self {
        match self {
            Self::Local(ty, slots, storage) => Self::ReadOnlyLocal(ty, slots, storage),
            Self::Indexed { ty, initialization, base, offset, words } => Self::Indexed {
                ty, initialization, base: Box::new((*base).read_only()), offset, words,
            },
            mut place => {
                match &mut place {
                    Self::Reference { writable, .. } | Self::Element { writable, .. }
                    | Self::ApplicationElement { writable, .. } => *writable = false,
                    _ => {}
                }
                place
            }
        }
    }
    pub(super) fn project(self, ty: Ty, components: &[usize]) -> Self {
        match self {
            Self::Reference {
                address,
                scope,
                words,
                writable,
                ..
            } => Self::Reference {
                ty,
                address,
                scope,
                writable,
                words: components.iter().map(|i| words[*i]).collect(),
            },
            Self::ApplicationElement {
                handle,
                identity,
                sources,
                initialization,
                index,
                index_range,
                stride,
                words,
                writable,
                ..
            } => Self::ApplicationElement {
                ty,
                handle,
                identity,
                sources,
                initialization,
                index,
                index_range,
                stride,
                writable,
                words: components.iter().map(|i| words[*i]).collect(),
            },
            Self::AtomicParameter(_) => Self::AtomicParameter(ty),
            Self::Local(_, slots, storage) => {
                Self::Local(ty, components.iter().map(|i| slots[*i]).collect(), storage)
            }
            Self::ReadOnlyLocal(_, slots, storage) => {
                Self::ReadOnlyLocal(ty, components.iter().map(|i| slots[*i]).collect(), storage)
            }
            Self::Indexed {
                base,
                offset,
                words,
                initialization,
                ..
            } => Self::Indexed {
                ty,
                base,
                offset,
                words: components.iter().map(|i| words[*i]).collect(),
                initialization: if initialization.is_empty() {
                    initialization
                } else {
                    components
                        .iter()
                        .map(|i| initialization[*i].clone())
                        .collect()
                },
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
    pub(super) fn local_slots(&mut self, ty: &Ty) -> (Vec<usize>, std::ops::Range<usize>) {
        let base = self.locals.last_key_value().map_or(0, |(end, _)| *end);
        let end = base + ty.components.len();
        if end != base { self.locals.insert(end, ty.clone()); }
        ((base..end).collect(), base..end)
    }
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
            let owner = self.materialize_array(value);
            self.env.insert(name, Binding::Owned(owner));
        }
    }

    pub(super) fn materialize_array(&mut self, value: Value) -> OwnedBinding {
        let proof = bounds::Proof::capture(&value);
        let uniform = value.uniform;
        let (slots, storage) = self.local_slots(&value.ty);
        for (slot, r) in slots.iter().zip(&value.regs) {
            self.code.push(Inst::StoreLocal(*slot, *r));
        }
        let place = Place::ReadOnlyLocal(value.ty, slots, storage);
        self.initialize(&place);
        OwnedBinding { place, proof, uniform, initialization: None }
    }
    pub(super) fn array_place(&mut self, place: Place, expr: &Expr) -> Result<Option<Place>> {
        let held = matches!(place.storage(), Place::ApplicationElement { .. })
            .then(|| self.hold_binding(Binding::AtomicBorrow(place.clone())));
        let index = self.expr(expr)?;
        if let Some(name) = held {
            self.env.remove(&name);
        }
        let Some(index) = index else {
            return Ok(None);
        };
        self.array_index(place, index, expr.span).map(Some)
    }
    pub(super) fn array_value_index(&mut self, value: Value, expr: &Expr) -> Result<Option<Value>> {
        let held = value.ty.contains_views().then(|| self.hold_binding(Binding::Value(value.clone())));
        let evaluated = self.expr(expr);
        if let Some(name) = held { self.env.remove(&name); }
        let Some(index) = evaluated? else {
            return Ok(None);
        };
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
            return Ok(Some(Value {
                proof: value.projected_proof(&components),
                literal: false,
                ty,
                regs: components.iter().map(|i| value.regs[*i]).collect(),
                uniform: value.uniform,
                constant: value
                    .constant
                    .map(|b| components.iter().map(|i| b[*i]).collect()),
            }));
        }
        let proof = bounds::Proof::capture(&value);
        let array = value.ty.clone();
        let selected = ownership::ApplicationIndex::of(&index);
        let uniform = value.uniform && index.uniform;
        let place = self.materialize_array(value).place;
        let place = self.array_index(place, index, expr.span)?;
        let mut result = self.read_place(&place, expr.span)?;
        result.proof = self.array_value_proof(&proof, &array, selected, &result.regs);
        result.uniform = uniform;
        result.constant = result.proof.known_value(&result.ty);
        Ok(Some(result))
    }
    pub(super) fn array_index(&mut self, place: Place, index: Value, span: Span) -> Result<Place> {
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
        self.observe_bounds(&index);
        if let Some(bits) = &index.constant {
            let first = bits[0] as usize * element.components.len();
            let components: Vec<_> = (first..first + element.components.len()).collect();
            return Ok(place.project(element, &components));
        }
        let initialization = self.indexed_positions(&place, &index, element.components.len());
        let stride = ir::align_up(element.size, element.align);
        let factor = self.constant(Scalar::U32, Bits::from(stride));
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
                    Place::Reference { words, .. }
                    | Place::Element { words, .. }
                    | Place::ApplicationElement { words, .. }
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
            .map(|(_, o)| first + o)
            .collect();
        Ok(Place::Indexed {
            ty: element,
            initialization,
            base,
            offset,
            words,
        })
    }
    fn array_memory(&mut self, base: &Place, write: bool, span: Span) -> Result<IndexedSource> {
        Ok(match base {
            Place::Reference { address, .. } => IndexedSource::View {
                address: address.component(0).0,
                index: self.constant(Scalar::U32, 0).regs[0],
                stride: 0,
            },
            Place::ApplicationElement {
                handle,
                identity,
                index,
                stride,
                ..
            } => IndexedSource::View {
                address: self.application_address(*handle, *identity, write, span)?,
                index: *index,
                stride: *stride,
            },
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
        })
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
            ..
        } = place
        else {
            unreachable!()
        };
        match &**base {
            Place::Local(..) | Place::Reference { .. } | Place::ApplicationElement { .. } => {
                self.require_initialized(place, span)?
            }
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
            let instruction = match &**base {
                Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                    Inst::LoadLocalIndexed {
                        dst,
                        base: storage.start,
                        len: storage.len(),
                        index: *offset,
                        word: self.locals[&storage.end].components[slots[0] - storage.start].1 + word,
                    }
                }
                _ => Inst::LoadIndexed {
                    dst,
                    source: self.array_memory(base, false, span)?,
                    offset: *offset,
                    word: *word,
                },
            };
            self.code.push(instruction);
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
            let instruction = match &**base {
                Place::Local(_, slots, storage) => Inst::StoreLocalIndexed {
                    base: storage.start,
                    len: storage.len(),
                    index: *offset,
                    word: self.locals[&storage.end].components[slots[0] - storage.start].1 + word,
                    value,
                },
                Place::Reference { .. }
                | Place::Element { .. }
                | Place::ApplicationElement { .. } => {
                    if let Place::Element { binding, .. } = &**base {
                        self.writes.insert(*binding);
                    }
                    Inst::StoreIndexed {
                        source: self.array_memory(base, true, span)?,
                        offset: *offset,
                        word: *word,
                        value,
                    }
                }
                _ => unreachable!(),
            };
            self.code.push(instruction);
        }
        Ok(())
    }
}
