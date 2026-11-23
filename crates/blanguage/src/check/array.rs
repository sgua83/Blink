//! 固定数组与数值向量按原逻辑分量索引；动态位置直接访问真实存储。
use super::*;
use std::{borrow::Cow, ffi::c_void};
impl Place {
    pub(super) fn read_only(self) -> Self {
        match self {
            Self::Local(ty, slots, storage) => Self::ReadOnlyLocal(ty, slots, storage),
            Self::Indexed { ty, initialization, locations, base, offset, words } => Self::Indexed {
                ty, initialization, locations, base: Box::new((*base).read_only()), offset, words,
            },
            mut place => {
                match &mut place {
                    Self::Reference { access, .. } | Self::ApplicationElement { access, .. } => {
                        if *access != Access::Atomic { *access = Access::Read; }
                    }
                    Self::Element { writable, .. } => *writable = false,
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
                access,
                ..
            } => Self::Reference {
                ty,
                address,
                scope,
                access,
                words: components.iter().map(|i| words[*i]).collect(),
            },
            Self::ApplicationElement {
                handle,
                identity,
                sources,
                resource,
                initialization,
                index,
                index_value,
                index_expression,
                parent,
                element_layout,
                index_range,
                stride,
                words,
                access,
                ..
            } => Self::ApplicationElement {
                ty,
                handle,
                identity,
                sources,
                resource,
                initialization,
                index,
                index_value,
                index_expression,
                parent,
                element_layout,
                index_range,
                stride,
                access,
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
                source,
                resource,
                element_layout,
                address,
                index,
                stride,
                words,
                ..
            } => Self::ViewElement {
                ty,
                source,
                resource,
                element_layout,
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
#[repr(u32)]
enum FixedProjectionKind {
    Components,
    Vector,
    Array,
}

#[repr(C)]
struct FixedProjection {
    kind: FixedProjectionKind,
    first: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::fixed_projection"]
    fn fixed_projection(array: u32, width: usize, known: u32, index: usize) -> FixedProjection;
}

impl FixedProjection {
    fn of(source: &Ty, element: &Ty, index: &Value) -> Self {
        let constant = index.constant.as_ref().map(|bits| bits[0] as usize);
        unsafe {
            fixed_projection(
                u32::from(source.array.is_some()),
                element.components.len(),
                u32::from(constant.is_some()),
                constant.unwrap_or(0),
            )
        }
    }
}

#[repr(C)]
struct ComponentChoice { key: u32, registers: [Reg; 2] }

#[repr(C)]
struct SelectedComponents { registers: [Reg; 2] }

#[repr(C)]
struct ComponentEmitter {
    context: *mut c_void,
    choice: unsafe extern "C" fn(*mut c_void, usize) -> ComponentChoice,
    equal: unsafe extern "C" fn(*mut c_void, Reg, u32) -> Reg,
    select: unsafe extern "C" fn(*mut c_void, Reg, Reg, Reg) -> Reg,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::select_indexed_components"]
    fn select_indexed_components(index: Reg, count: usize, paired: u32, emitter: ComponentEmitter) -> SelectedComponents;
}

enum ComponentChoices<'a> {
    Values(Scalar, &'a [Reg]),
    Offsets(&'a [u32]),
    Addresses(&'a [u32], &'a BTreeMap<u32, [Reg; 2]>),
}

struct ComponentSelection<'a, 'w> {
    builder: &'a mut Builder<'w>,
    choices: ComponentChoices<'a>,
    scalar: Scalar,
}

impl ComponentChoices<'_> {
    fn select(self, builder: &mut Builder<'_>, index: Reg) -> [Reg; 2] {
        let (scalar, count, paired) = match self {
            Self::Values(scalar, values) => (scalar, values.len(), 0),
            Self::Offsets(words) => (Scalar::U32, words.len(), 0),
            Self::Addresses(words, _) => (Scalar::U32, words.len(), 1),
        };
        let mut state = ComponentSelection { builder, choices: self, scalar };
        let emitter = ComponentEmitter {
            context: std::ptr::from_mut(&mut state).cast(),
            choice: component_choice, equal: component_equal, select: component_select,
        };
        unsafe { select_indexed_components(index, count, paired, emitter) }.registers
    }
}

unsafe extern "C" fn component_choice(context: *mut c_void, ordinal: usize) -> ComponentChoice {
    let state = unsafe { &mut *context.cast::<ComponentSelection<'_, '_>>() };
    match state.choices {
        ComponentChoices::Values(_, values) => ComponentChoice { key: ordinal as u32, registers: [values[ordinal], 0] },
        ComponentChoices::Offsets(words) => {
            let word = state.builder.constant(Scalar::U32, Bits::from(words[ordinal])).regs[0];
            ComponentChoice { key: ordinal as u32, registers: [word, 0] }
        }
        ComponentChoices::Addresses(words, addresses) => {
            let key = words[ordinal];
            ComponentChoice { key, registers: addresses[&key] }
        }
    }
}

unsafe extern "C" fn component_equal(context: *mut c_void, index: Reg, key: u32) -> Reg {
    let state = unsafe { &mut *context.cast::<ComponentSelection<'_, '_>>() };
    let b = &mut *state.builder;
    let number = b.constant(Scalar::U32, Bits::from(key)).regs[0];
    let dst = b.reg(Scalar::Bool);
    b.emit(InstKind::Binary {
        dst, op: BinaryOp::Compare(ir::Comparison::Equal), a: index, b: number,
    });
    dst
}

unsafe extern "C" fn component_select(context: *mut c_void, condition: Reg, a: Reg, b: Reg) -> Reg {
    let state = unsafe { &mut *context.cast::<ComponentSelection<'_, '_>>() };
    let dst = state.builder.reg(state.scalar);
    state.builder.emit(InstKind::Select { dst, condition, a, b });
    dst
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
            let trigger = match &self.scope {
                LexicalScope::Parallel{site,..} => site.clone().map(GenerationAnchor::Syntax),
                _ => match self.scope.function().map(|instance| &instance.definition.syntax_origin) {
                Some(crate::frontend::DefinitionSyntax::Source) => self.scope.parameters()
                    .find(|parameter| parameter.name() == name)
                    .and_then(|parameter| self.source_at(parameter.span(), SourceSyntax::Parameter))
                    .map(GenerationAnchor::Syntax),
                Some(crate::frontend::DefinitionSyntax::CompiledInterface) | None => None,
                },
            };
            let origin = Arc::new(GeneratedOrigin { reason: GenerationReason::ArrayParameterStore, trigger });
            let mut owner = self.materialize_array(value, Some(origin));
            owner.place = owner.place.read_only();
            self.env.insert(name, Binding::Owned(owner));
        }
    }

    pub(super) fn materialize_array(&mut self, value: Value, generated_origin: Option<Arc<GeneratedOrigin>>) -> OwnedBinding {
        let proof = bounds::Proof::capture(&value);
        let uniform = value.uniform;
        let (slots, storage) = self.local_slots(&value.ty);
        let source_site = if generated_origin.is_none() { self.source_site.clone() } else { None };
        for (slot, r) in slots.iter().zip(&value.regs) {
            self.code.push(Inst { source_site: source_site.clone(), generated_origin: generated_origin.clone(), ..Inst::new(InstKind::StoreLocal(*slot, *r)) });
        }
        let place = Place::Local(value.ty, slots, storage);
        self.initialize(&place);
        OwnedBinding { place, proof, uniform, initialization: None }
    }
    pub(super) async fn array_place(&mut self, place: Place, expr: &Expr) -> Result<Option<Place>> {
        let held = matches!(place.storage(), Place::ApplicationElement { .. })
            .then(|| self.hold_binding(Binding::AtomicBorrow(place.clone())));
        let index = unsafe { self.world.call(self.expr(expr)).await }?;
        if let Some(name) = held {
            self.env.remove(&name);
        }
        let Some(index) = index else {
            return Ok(None);
        };
        self.array_index(place, index, expr.span()).map(Some)
    }
    pub(super) async fn array_value_index(&mut self,value:Value,expr:&Expr)->Result<Option<Value>> {
        unsafe { self.world.call(self.array_value_at(value,expr.span(),async |builder|unsafe { builder.world.call(builder.expr(expr)).await })).await }
    }
    pub(super) async fn array_value_at(&mut self,value:Value,span:Span,index:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>> {
        let held = value.ty.contains_borrows().then(|| self.hold_binding(Binding::Value(value.clone())));
        let evaluated = index(self).await;
        if let Some(name) = held { self.env.remove(&name); }
        let Some(index) = evaluated? else {
            return Ok(None);
        };
        let (ty, count) = value.ty.indexed_element().unwrap();
        self.check_fixed_index(&index, count, span, "固定值下标有效性无法静态证明")?;
        let projection = FixedProjection::of(&value.ty, &ty, &index);
        match projection.kind {
            FixedProjectionKind::Components => {
                let first = projection.first;
                let components: Vec<_> = (first..first + ty.components.len()).collect();
                Ok(Some(Value {
                    proof: value.projected_proof(&components),
                    literal: false,
                    ty,
                    regs: components.iter().map(|i| value.regs[*i]).collect(),
                    uniform: value.uniform,
                    constant: value.constant.map(|b| components.iter().map(|i| b[*i]).collect()),
                }))
            }
            FixedProjectionKind::Vector => {
                let proof = bounds::Proof::capture(&value);
                let [reg, _] = ComponentChoices::Values(ty.components[0].0, &value.regs)
                    .select(self, index.regs[0]);
                let selected = ownership::ApplicationIndex::of(&index);
                let proof = self.array_value_proof(&proof, &value.ty, &ty, count, selected, &[reg]);
                let constant = proof.known_value(&ty);
                let uniform = value.uniform && index.uniform;
                Ok(Some(Value { ty, regs: vec![reg], proof, uniform, constant, literal: false }))
            }
            FixedProjectionKind::Array => {
                let array = value.ty.clone();
                let selected = ownership::ApplicationIndex::of(&index);
                let uniform = value.uniform && index.uniform;
                let OwnedBinding { place, proof, .. } = self.materialize_array(value, None);
                self.observe_bounds(&index);
                let place = self.lower_array_storage(place, index, ty, span)?;
                let mut result = unsafe { self.world.call(self.read_place(&place, span, None)).await }?;
                result.proof = self.array_value_proof(&proof, &array, &result.ty, count, selected, &result.regs);
                result.uniform = uniform;
                result.constant = result.proof.known_value(&result.ty);
                Ok(Some(result))
            }
        }
    }

    fn check_fixed_index(&self, index: &Value, count: u32, span: Span, message: &str) -> Result<()> {
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, span)?;
        if !self.index_in(index, IndexDomain::Fixed(count), count, None) {
            return self.fail(span, "B137", message);
        }
        Ok(())
    }

    pub(super) fn array_index(&mut self, place: Place, index: Value, span: Span) -> Result<Place> {
        let (element, count) = place.ty().indexed_element().unwrap();
        self.check_fixed_index(
            &index, count, span,
            "固定值下标有效性无法静态证明；使用常量下标、有界循环或实际长度条件",
        )?;
        self.observe_bounds(&index);
        let projection = FixedProjection::of(place.ty(), &element, &index);
        match projection.kind {
            FixedProjectionKind::Components => {
                let first = projection.first;
                let components: Vec<_> = (first..first + element.components.len()).collect();
                Ok(place.project(element, &components))
            }
            FixedProjectionKind::Vector => Ok(self.vector_index(place, index, element)),
            FixedProjectionKind::Array => self.lower_array_storage(place, index, element, span),
        }
    }

    fn lower_array_storage(&mut self, place: Place, index: Value, element: Ty, span: Span) -> Result<Place> {
        if element.components.is_empty() {
            return Ok(place.project(element, &[]));
        }
        let initialization = self.indexed_positions(&place, &index, element.components.len());
        let locations = Position::index(place.data_positions(), ownership::ApplicationIndex::of(&index), element.components.len());
        let stride = ir::align_up(element.size, element.align);
        let factor = self.constant(Scalar::U32, Bits::from(stride));
        let relative = self.binary("*", index, factor, span,None)?.regs[0];
        let (base, offset, first) = match place {
            Place::Indexed {
                base,
                offset,
                words,
                ..
            } => {
                let dst = self.reg(Scalar::U32);
                self.emit(InstKind::Binary {
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
    fn vector_index(&mut self, mut place: Place, index: Value, ty: Ty) -> Place {
        let initialization = self.indexed_positions(&place, &index, 1);
        let locations = Position::index(place.data_positions(), ownership::ApplicationIndex::of(&index), 1);
        let [relative, _] = {
            let offsets: Cow<'_, [u32]> = match &mut place {
                Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                    let layout = &self.locals[&storage.end];
                    let mut offsets: Vec<_> = slots.iter().map(|slot| layout.components[*slot - storage.start].1).collect();
                    let first = *offsets.iter().min().unwrap();
                    slots.sort_unstable();
                    for offset in &mut offsets { *offset -= first; }
                    Cow::Owned(offsets)
                }
                Place::Reference { words, .. } | Place::ApplicationElement { words, .. }
                | Place::Element { words, .. } | Place::ViewElement { words, .. }
                | Place::TileElement { words, .. } | Place::Indexed { words, .. } => Cow::Borrowed(words),
                Place::AtomicParameter(ty) => Cow::Owned(ty.components.iter().map(|(_, offset)| *offset).collect()),
            };
            ComponentChoices::Offsets(&offsets).select(self, index.regs[0])
        };
        let (base, offset) = match place {
            Place::Indexed { base, offset, .. } => {
                let dst = self.reg(Scalar::U32);
                self.emit(InstKind::Binary { dst, op: BinaryOp::IAdd, a: offset, b: relative });
                (base, dst)
            }
            base => (Box::new(base), relative),
        };
        Place::Indexed { ty, initialization, locations, base, offset, words: vec![0] }
    }
    pub(super) fn array_memory(&mut self, base: &Place, offset: Reg, word: u32, scalar: Scalar, write: bool, span: Span) -> Result<(IndexedSource, Reg, u32)> {
        if let Place::Reference { address: ReferenceAddress::Components(addresses), words, .. } = base {
            let selected = if word == 0 { offset } else {
                let number = self.constant(Scalar::U32, Bits::from(word)).regs[0];
                let dst = self.reg(Scalar::U32);
                self.emit(InstKind::Binary { dst, op: BinaryOp::IAdd, a: offset, b: number });
                dst
            };
            let address = ComponentChoices::Addresses(words, addresses).select(self, selected);
            let zero = self.constant(Scalar::U32, 0).regs[0];
            return Ok((IndexedSource::View { address, index: zero, stride: ElementStride::Constant(0) }, zero, 0));
        }
        let source = match base {
            Place::Reference { address, .. } => {
                let zero=self.constant(Scalar::U32,0).regs[0];
                address.source_at(0,zero).0
            },
            Place::ApplicationElement {
                handle,
                identity,
                index,
                stride,
                ..
            } => {
                let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,write,Some(offset),[(scalar,word)],span)?;
                IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))}
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
    pub(super) async fn read_array_place(&mut self, place: &Place, span: Span) -> Result<Value> {
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
                unsafe { self.world.call(self.require_initialized(place, span)).await }?
            }
            Place::Element { binding, index, .. } => {
                self.reads.insert(*binding);
                if *index != self.global_index {
                    self.cross_access.insert(*binding);
                }
                if self.view_access[binding].writes() {
                    unsafe { self.world.call(self.require_initialized(place, span)).await }?;
                }
            }
            Place::TileElement { tile, .. } => {
                if let TileOrigin::Input { binding, .. } =
                    &self.tiles[*tile].source
                {
                    self.reads.insert(*binding);
                    self.cross_access.insert(*binding);
                }
            }
            _ => {}
        }
        self.data_access(place, Access::Read, None);
        let reads=self.dataset_effect(place,Access::Read,false,span)?;
        let mut regs = Vec::new();
        for ((scalar, _), word) in ty.components.iter().zip(words) {
            let dst = self.reg(*scalar);
            let instruction = match &**base {
                Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                    Inst::new(InstKind::LoadLocalIndexed {
                        dst,
                        base: storage.start,
                        len: storage.len(),
                        index: *offset,
                        word: self.locals[&storage.end].components[slots[0] - storage.start].1 + word,
                    })
                }
                _ => {
                    let (source, offset, word) = self.array_memory(base, *offset, *word, *scalar, false, span)?;
                    Inst::new(InstKind::LoadIndexed { dst, source, offset, word })
                },
            };
            self.emit(instruction.kind);
            regs.push(dst);
        }
        let (proof, uniform, _) = unsafe { self.world.call(self.read_owned_proof(place, &regs, &reads)).await }?;
        Ok(Value {
            proof,
            literal: false,
            ty: ty.clone(),
            regs,
            uniform,
            constant: None,
        })
    }
    pub(super) async fn write_array_place(
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
        unsafe { self.world.call(self.writable_storage(place, span, Some(bounds::Proof::default()))).await }?;
        if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
            return self.fail(span, "B143", "赋值位置包含重复分量");
        }
        self.data_access(place, Access::Write, None);
        self.dataset_effect(place,Access::Write,false,span)?;
        unsafe { self.world.call(self.assign_owned_proof(place, bounds::Proof::capture(&value), false)).await }?;
        self.initialize(place);
        for ((word, &(scalar, _)), value) in words.iter().zip(&value.ty.components).zip(value.regs) {
            let instruction = match &**base {
                Place::Local(_, slots, storage) => Inst::new(InstKind::StoreLocalIndexed {
                    base: storage.start,
                    len: storage.len(),
                    index: *offset,
                    word: self.locals[&storage.end].components[slots[0] - storage.start].1 + word,
                    value,
                }),
                Place::Reference { .. }
                | Place::Element { .. }
                | Place::ApplicationElement { .. } => {
                    if let Place::Element { binding, .. } = &**base {
                        self.writes.insert(*binding);
                    }
                    let (source, offset, word) = self.array_memory(base, *offset, *word, scalar, true, span)?;
                    Inst::new(InstKind::StoreIndexed { source, offset, word, value })
                }
                _ => unreachable!(),
            };
            self.emit(instruction.kind);
        }
        Ok(())
    }
}
