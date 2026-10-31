//! 固定数组与数值向量按原逻辑分量索引；动态位置直接访问真实存储。
use super::*;
impl Place {
    pub(super) fn read_only(self) -> Self {
        match self {
            Self::Local(ty, slots, storage) => Self::ReadOnlyLocal(ty, slots, storage),
            Self::Indexed { ty, initialization, locations, base, offset, words } => Self::Indexed {
                ty, initialization, locations, base: Box::new((*base).read_only()), offset, words,
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
                locations,
                ..
            } => Self::Indexed {
                ty,
                base,
                offset,
                words: components.iter().map(|i| words[*i]).collect(),
                locations: if locations.is_empty() { locations } else { components.iter().map(|i| locations[*i].clone()).collect() },
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
        let (ty, count) = value.ty.indexed_element().unwrap();
        if !self.index_in(
            &index,
            IndexDomain::Fixed(count),
            count,
        ) {
            return self.fail(expr.span, "B137", "固定值下标有效性无法静态证明");
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
        if array.array.is_none() {
            let choices: Vec<_> = value.regs.iter().enumerate().map(|(i, r)| (i as u32, *r)).collect();
            let reg = self.select_component(index.regs[0], ty.components[0].0, &choices);
            let proof = self.array_value_proof(&proof, &array, selected, &[reg]);
            let constant = proof.known_value(&ty);
            return Ok(Some(Value { ty, regs: vec![reg], proof, uniform, constant, literal: false }));
        }
        let place = self.materialize_array(value).place;
        let place = self.array_index(place, index, expr.span)?;
        let mut result = self.read_place(&place, expr.span)?;
        result.proof = self.array_value_proof(&proof, &array, selected, &result.regs);
        result.uniform = uniform;
        result.constant = result.proof.known_value(&result.ty);
        Ok(Some(result))
    }
    pub(super) fn array_index(&mut self, place: Place, index: Value, span: Span) -> Result<Place> {
        let (element, count) = place.ty().indexed_element().unwrap();
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, span)?;
        if !self.index_in(&index, IndexDomain::Fixed(count), count) {
            return self.fail(
                span,
                "B137",
                "固定值下标有效性无法静态证明；使用常量下标、有界循环或实际长度条件",
            );
        }
        self.observe_bounds(&index);
        if let Some(bits) = &index.constant {
            let first = bits[0] as usize * element.components.len();
            let components: Vec<_> = (first..first + element.components.len()).collect();
            return Ok(place.project(element, &components));
        }
        if place.ty().array.is_none() {
            return Ok(self.vector_index(place, index, element));
        }
        let initialization = self.indexed_positions(&place, &index, element.components.len());
        let locations = Position::index(place.data_positions(), ownership::ApplicationIndex::of(&index), element.components.len());
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
            locations,
            base,
            offset,
            words,
        })
    }
    fn select_component(&mut self, index: Reg, scalar: Scalar, choices: &[(u32, Reg)]) -> Reg {
        let mut selected = choices[0].1;
        for (component, value) in &choices[1..] {
            let number = self.constant(Scalar::U32, Bits::from(*component)).regs[0];
            let condition = self.reg(Scalar::Bool);
            self.code.push(Inst::Binary {
                dst: condition, op: BinaryOp::Compare(ir::Comparison::Equal), a: index, b: number,
            });
            let dst = self.reg(scalar);
            self.code.push(Inst::Select { dst, condition, a: *value, b: selected });
            selected = dst;
        }
        selected
    }
    fn vector_index(&mut self, mut place: Place, index: Value, ty: Ty) -> Place {
        let initialization = self.indexed_positions(&place, &index, 1);
        let locations = Position::index(place.data_positions(), ownership::ApplicationIndex::of(&index), 1);
        let offsets: Vec<_> = match &mut place {
            Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                let layout = &self.locals[&storage.end];
                let offsets: Vec<_> = slots.iter().map(|slot| layout.components[*slot - storage.start].1).collect();
                let first = *offsets.iter().min().unwrap();
                slots.sort_unstable();
                offsets.into_iter().map(|offset| offset - first).collect()
            }
            Place::Reference { words, .. } | Place::ApplicationElement { words, .. }
            | Place::Element { words, .. } | Place::ViewElement { words, .. }
            | Place::TileElement { words, .. } | Place::Indexed { words, .. } => words.clone(),
            Place::AtomicParameter(ty) => ty.components.iter().map(|(_, offset)| *offset).collect(),
        };
        let choices: Vec<_> = offsets.iter().enumerate()
            .map(|(i, offset)| (i as u32, self.constant(Scalar::U32, Bits::from(*offset)).regs[0])).collect();
        let relative = self.select_component(index.regs[0], Scalar::U32, &choices);
        let (base, offset) = match place {
            Place::Indexed { base, offset, .. } => {
                let dst = self.reg(Scalar::U32);
                self.code.push(Inst::Binary { dst, op: BinaryOp::IAdd, a: offset, b: relative });
                (base, dst)
            }
            base => (Box::new(base), relative),
        };
        Place::Indexed { ty, initialization, locations, base, offset, words: vec![0] }
    }
    fn array_memory(&mut self, base: &Place, offset: Reg, word: u32, write: bool, span: Span) -> Result<(IndexedSource, Reg, u32)> {
        if let Place::Reference { address: ReferenceAddress::Components(addresses), words, .. } = base {
            let selected = if word == 0 { offset } else {
                let number = self.constant(Scalar::U32, Bits::from(word)).regs[0];
                let dst = self.reg(Scalar::U32);
                self.code.push(Inst::Binary { dst, op: BinaryOp::IAdd, a: offset, b: number });
                dst
            };
            let mut address = [0; 2];
            for (part, reg) in address.iter_mut().enumerate() {
                let choices: Vec<_> = words.iter().map(|word| (*word, addresses[word][part])).collect();
                *reg = self.select_component(selected, Scalar::U32, &choices);
            }
            let zero = self.constant(Scalar::U32, 0).regs[0];
            return Ok((IndexedSource::View { address, index: zero, stride: 0 }, zero, 0));
        }
        let source = match base {
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
            } => {
                let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,write,span)?;
                IndexedSource::View{address,index,stride}
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
        };
        Ok((source, offset, word))
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
        self.data_access(place, Access::Read);
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
                _ => {
                    let (source, offset, word) = self.array_memory(base, *offset, *word, false, span)?;
                    Inst::LoadIndexed { dst, source, offset, word }
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
        self.data_access(place, Access::Write);
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
                    let (source, offset, word) = self.array_memory(base, *offset, *word, true, span)?;
                    Inst::StoreIndexed { source, offset, word, value }
                }
                _ => unreachable!(),
            };
            self.code.push(instruction);
        }
        Ok(())
    }
}
