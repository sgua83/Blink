//! 外部调用的 ABI 声明是信任边界；实参借用统一判定别名，ABI 只投影实际位置。
use super::*;
use cooperative::Argument;
pub(super) fn validate_signature(d: &Definition) -> Result<()> {
    let f = &d.function;
    if f.kind != FunctionKind::Value
        || f.target != Target::Cpu
        || f.body.is_some()
        || !f.types.is_empty()
        || !f.values.is_empty()
        || !f.contracts.is_empty()
        || f.params.iter().any(|p| {
            p.ty.dataset
                || p.ty.tile
                || p.ty.is_acceleration()
                || p.ty.is_texture_resource()
                || p.access == Access::Atomic
        })
        || f.result.dataset
        || f.result.tile
    {
        return d.source.fail(f.span,"B263","extern C 需要无正文、无泛型／契约的 on cpu 普通函数，只接收普通值及 read／write／inout 位置");
    }
    Ok(())
}
fn value_type(world: &World, d: &Definition, ty: &TypeName, span: Span) -> Result<Ty> {
    let ty = world.ty(ty, &d.source, span, &d.module, &d.imports)?;
    if ty.owns_resources() {
        return d.source.fail(
            span,
            "B224",
            "拥有结果需要Frame所有权协议，不能跨当前普通C ABI",
        );
    }
    if ty.contains_views() || ty.components.is_empty() {
        return d.source.fail(
            span,
            "B263",
            "C ABI 需要非空普通值布局，不能传递 Blink 资源引用",
        );
    }
    Ok(ty)
}
pub(super) fn check_declaration(world: &World, d: &Definition) -> Result<()> {
    validate_signature(d)?;
    for param in &d.function.params {
        value_type(world, d, &param.ty, param.span)?;
    }
    if d.function.result.name != "void" {
        value_type(world, d, &d.function.result, d.function.span)?;
    }
    Ok(())
}
impl Builder<'_> {
    pub(super) fn external_read_place(&mut self, expr: &Expr) -> Result<Option<Place>> {
        fn root(expr: &Expr) -> Option<&String> {
            match &expr.kind {
                ExprKind::Name(name) => Some(name),
                ExprKind::Member(base, _) | ExprKind::Index(base, _) => root(base),
                _ => None,
            }
        }
        if let Some(name) = root(expr) {
            if let Some(Binding::Value(value)) = self.env.get(name).cloned() {
                let proof = bounds::Proof::capture(&value);
                let uniform = value.uniform;
                let place = self.materialize_array(value);
                self.env.insert(
                    name.clone(),
                    Binding::Owned(OwnedBinding {
                        place,
                        proof,
                        uniform,
                        initialization: None,
                    }),
                );
            }
            let Some(place) = self.place(expr)? else {
                return Ok(None);
            };
            match place.storage() {
                Place::Local(..) | Place::ApplicationElement { .. } => {
                    self.require_initialized(&place, expr.span)?
                }
                Place::Element { binding, .. } if self.view_access[binding].writes() => {
                    self.require_initialized(&place, expr.span)?
                }
                _ => {}
            }
            Ok(Some(place))
        } else {
            let Some(value) = self.expr(expr)? else {
                return Ok(None);
            };
            Ok(Some(self.materialize_array(value)))
        }
    }
    pub(super) fn call_reference(
        &mut self,
        place: &Place,
        access: Access,
        span: Span,
    ) -> Result<(CallReference,Vec<u32>)> {
        let (storage, offset, words) = match place {
            Place::Reference{address,words,..}=>(CallStorage::Indexed(IndexedSource::View{address:*address,index:self.constant(Scalar::U32,0).regs[0],stride:0}),None,words.clone()),
            Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => (
                CallStorage::Local {
                    base: storage.start,
                    len: storage.len(),
                },
                None,
                slots
                    .iter()
                    .map(|s| (*s - storage.start) as u32)
                    .collect::<Vec<_>>(),
            ),
            Place::Element {
                binding,
                index,
                words,
                ..
            } => {
                if access.reads() {
                    self.reads.insert(*binding);
                }
                if access.writes() {
                    self.writes.insert(*binding);
                }
                if *index != self.global_index {
                    self.cross_access.insert(*binding);
                }
                (
                    CallStorage::Indexed(IndexedSource::Data {
                        binding: *binding,
                        index: *index,
                    }),
                    None,
                    words.clone(),
                )
            }
            Place::ApplicationElement {
                handle,
                identity,
                index,
                stride,
                words,
                ..
            } => (
                CallStorage::Indexed(IndexedSource::View {
                    address: self.application_address(*handle, *identity, access.writes(), span)?,
                    index: *index,
                    stride: *stride,
                }),
                None,
                words.clone(),
            ),
            Place::ViewElement {
                address,
                index,
                stride,
                words,
                ..
            } => (
                CallStorage::Indexed(IndexedSource::View {
                    address: *address,
                    index: *index,
                    stride: *stride,
                }),
                None,
                words.clone(),
            ),
            Place::Indexed {
                base,
                offset,
                words,
                ..
            } => {
                let storage = match &**base {
                    Place::Reference{address,..}=>CallStorage::Indexed(IndexedSource::View{address:*address,index:self.constant(Scalar::U32,0).regs[0],stride:0}),
                    Place::Local(_, _, storage) | Place::ReadOnlyLocal(_, _, storage) => {
                        CallStorage::Local {
                            base: storage.start,
                            len: storage.len(),
                        }
                    }
                    Place::Element { binding, index, .. } => {
                        if access.reads() {
                            self.reads.insert(*binding);
                        }
                        if access.writes() {
                            self.writes.insert(*binding);
                        }
                        if *index != self.global_index {
                            self.cross_access.insert(*binding);
                        }
                        CallStorage::Indexed(IndexedSource::Data {
                            binding: *binding,
                            index: *index,
                        })
                    }
                    Place::ApplicationElement {
                        handle,
                        identity,
                        index,
                        stride,
                        ..
                    } => CallStorage::Indexed(IndexedSource::View {
                        address: self.application_address(
                            *handle,
                            *identity,
                            access.writes(),
                            span,
                        )?,
                        index: *index,
                        stride: *stride,
                    }),
                    Place::ViewElement {
                        address,
                        index,
                        stride,
                        ..
                    } => CallStorage::Indexed(IndexedSource::View {
                        address: *address,
                        index: *index,
                        stride: *stride,
                    }),
                    _ => return self.fail(span, "B264", "外部引用需要 CPU 普通值的实际存储"),
                };
                let first = match &**base {
                    Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                        (slots[0] - storage.start) as u32
                    }
                    _ => 0,
                };
                (
                    storage,
                    Some(*offset),
                    words.iter().map(|w| first + *w).collect(),
                )
            }
            _ => return self.fail(span, "B264", "外部引用需要 CPU 普通值的实际存储"),
        };
        let first=*words.iter().min().unwrap();
        let layout=words.iter().map(|word|word-first).collect();
        Ok((CallReference{storage,offset,word:first},layout))
    }
    pub(super) fn external_reference(&mut self,place:&Place,access:Access,span:Span)->Result<CallReference>{
        let ty=place.ty();
        let (address,words)=self.call_reference(place,access,span)?;
        if address.word*4%ty.align!=0{return self.fail(span,"B264","C ABI 引用的实际地址偏移不满足参数对齐");}
        if !words.iter().zip(&ty.components).all(|(word,(_,offset))|*word==offset/4){
            return self.fail(span,"B264","C ABI 引用要求连续且顺序匹配的实际布局，不能借用重排或离散 swizzle");
        }
        Ok(address)
    }
    pub(super) fn external_call(
        &mut self,
        d: &Arc<Definition>,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        if self.program.target != Target::Cpu {
            return self.fail(
                span,
                "B265",
                "C ABI 外部函数只能从 on cpu 执行，GPU／compute 不能调用宿主函数",
            );
        }
        check_declaration(self.world, d)?;
        let mut arguments = Vec::new();
        let mut written = Vec::new();
        for (param, arg) in d.function.params.iter().zip(args) {
            let ty = value_type(self.world, d, &param.ty, param.span)?;
            arguments.push(match arg {
                Argument::Value(value) if param.access == Access::Value => {
                    self.require_same(&ty, &value.ty, span)?;
                    CallArgument::Value {
                        ty,
                        values: value.regs,
                    }
                }
                Argument::Exclusive(place) if param.access != Access::Value => {
                    self.require_same(&ty, place.ty(), span)?;
                    let address = self.external_reference(&place, param.access, span)?;
                    if param.access.writes() {
                        written.push(place);
                    }
                    CallArgument::Reference {
                        ty,
                        access: param.access,
                        address,
                    }
                }
                _ => return self.fail(span, "B263", "C ABI 实参只接收普通值及实际位置"),
            });
        }
        let result = if d.function.result.name == "void" {
            None
        } else {
            Some(value_type(
                self.world,
                d,
                &d.function.result,
                d.function.span,
            )?)
        };
        let values = result
            .iter()
            .flat_map(|t| t.components.iter().map(|(s, _)| *s))
            .map(|s| self.reg(s))
            .collect::<Vec<_>>();
        self.code.push(Inst::Call(Call {
            target: CallTarget::External(d.function.external.clone().unwrap()),
            arguments,
            result: result.clone(),
            values: values.clone(),
        }));
        for place in written {
            self.assign_owned_proof(&place, bounds::Proof::default(), false);
            self.initialize(&place);
        }
        Ok(result.map(|ty| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs: values,
            uniform: false,
            constant: None,
        }))
    }
}
