//! 外部调用的 ABI 声明是信任边界；实参借用统一判定别名，ABI 只投影实际位置。
use super::*;
use cooperative::Argument;
pub(crate) fn validate_signature(d: &Definition) -> Result<()> {
    let f = &d.function;
    if f.kind() != FunctionKind::Value
        || f.target() != Target::Cpu
        || (f.external().is_some() && f.body().is_some())
        || (f.external().is_some() && (!f.types().is_empty() || !f.values().is_empty()))
        || f.contracts().iter().any(|contract| !match contract.predicate().kind() {
            crate::syntax::PredicateKind::Initialized { .. } => contract.kind() == ContractKind::Ensures,
            crate::syntax::PredicateKind::Nonzero(_) => contract.kind() == ContractKind::Requires,
            crate::syntax::PredicateKind::Memory(_) => contract.kind() == ContractKind::Ensures && f.external().is_some(),
            _ => false,
        })
        || f.params().iter().any(|p| {
            p.ty().dataset()
                || p.ty().tile()
                || p.access() == Access::Atomic
        })
        || f.result().dataset()
        || f.result().tile()
    {
        return d.source.fail(f.span(),"B263","extern C 需要 on cpu 普通C签名，只接收普通值及 read／write／inout 位置；from导入是无正文的固定符号，B定义可实例化，输入可声明 requires nonzero，输出可声明 ensures initialized，不透明内存效果可声明 ensures memory");
    }
    Ok(())
}
async fn value_type(world: &World, d: &Definition, ty: &TypeName, span: Span) -> Result<Ty> {
    if ty.dataset || ty.tile || ty.is_acceleration() || ty.is_texture_resource() {
        return d.source.fail(span,"B263","extern C 参数与结果需要普通C签名，不能传递dataset、tile或设备资源");
    }
    let ty = unsafe { world.call(world.ty(ty, &d.source, span, &d.module, &d.imports)).await }?;
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
pub(crate) async fn check_declaration(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let d=&instance.definition;
    let mut parameters=Vec::new();
    for param in d.function.params().iter() {
        let ty=unsafe { world.call(instance.bindings.type_name(world,&param.ty(),(&d.source,&d.module,&d.imports),param.span())).await }?;
        parameters.push(unsafe { world.call(value_type(world,d,&ty,param.span())).await }?);
    }
    let result_name=unsafe { world.call(instance.bindings.type_name(world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())).await }?;
    let result=if result_name.name=="void"||result_name.is_never(){None}
        else{Some(unsafe { world.call(value_type(world,d,&result_name,d.function.span())).await }?)};
    unsafe { world.call(outputs::validate_named(world,instance,&parameters,result.as_ref())).await }
}

impl Builder<'_> {
    pub(super) async fn read_argument(&mut self, expr: &Expr, expected:Option<&Ty>) -> Result<Option<cooperative::Argument>> {
        use cooperative::Argument;
        if let Some(data)=unsafe { self.world.call(self.application_reference(expr)).await }?{
            return Ok(Some(Argument::BorrowedData(data)));
        }
        fn root(expr: &Expr) -> Option<String> {
            match expr.kind() {
                ExprKind::Name(name) => Some(name.into()),
                ExprKind::Member(base, _) | ExprKind::Index(base, _) => root(&base),
                _ => None,
            }
        }
        if (root(expr).is_some_and(|name| self.env.contains_key(&name))
            || self.is_place(expr) && !self.temporary_projection(expr))
            && !unsafe { self.world.call(self.read_expression_type(expr)).await }?.is_some_and(|ty|ty.owns_resources()) {
            if let Some(name) = root(expr)
                && let Some(Binding::Value(value)) = self.env.get(&name).cloned() {
                let mut owner = self.materialize_array(value,None);
                owner.place = owner.place.read_only();
                self.env.insert(name.clone(), Binding::Owned(owner));
            }
            let Some(place) = unsafe { self.world.call(self.place(expr)).await }? else {
                return Ok(None);
            };
            match place.storage() {
                Place::Local(..) | Place::Reference { .. } | Place::ApplicationElement { .. } => {
                    unsafe { self.world.call(self.require_initialized(&place, expr.span())).await }?
                }
                Place::Element { binding, .. } if self.view_access[binding].writes() => {
                    unsafe { self.world.call(self.require_initialized(&place, expr.span())).await }?
                }
                _ => {}
            }
            Ok(Some(Argument::Reference(self.borrow_owned(place))))
        } else {
            let Some((value,owner))=unsafe { self.world.call(self.read_aggregate(expr,expected)).await }?else{return Ok(None)};
            Ok(Some(match (value,owner) {
                (returns::Returned::OwnedValue(value),None)
                    if value.place.as_ref().is_some_and(|place|matches!(place.storage(),Place::ApplicationElement{..})) =>
                    Argument::OwnedReference{place:value.place().unwrap(),value,owner:None},
                (value,Some(owner)) => Argument::Scoped{value,owner},
                (value,None) => Argument::resource(value),
            }))
        }
    }
    pub(super) fn call_reference(
        &mut self,
        place: &Place,
        access: Access,
        expose_writes: bool,
        span: Span,
    ) -> Result<CallReference> {
        if self.atomic_storage(place) && access != Access::Atomic {
            return self.fail(span, "B214", "原子位置只能转借给atomic参数");
        }
        let (storage, offset, words) = match place {
            Place::Reference { address, words, .. } => {
                let zero=self.constant(Scalar::U32,0).regs[0];
                let (source,word)=address.source_at(words[0],zero);
                (
                    CallStorage::Indexed(source),
                    None,
                    vec![word],
                )
            }
            Place::Local(_, _, storage) | Place::ReadOnlyLocal(_, _, storage) => (
                CallStorage::Local {
                    base: storage.start,
                    len: storage.len(),
                },
                None,
                reference_words(place, &self.locals).unwrap(),
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
            } => {
                let components: Vec<_> = place.ty().components.iter().zip(words).map(|(&(scalar, _), &word)| (scalar, word)).collect();
                let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,expose_writes,None,&components,span)?;
                (CallStorage::Indexed(IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))}),None,words.clone())
            },
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
                ..
            } => {
                let storage = match &**base {
                    Place::Reference { address, .. } => {
                        let zero=self.constant(Scalar::U32,0).regs[0];
                        CallStorage::Indexed(address.source_at(0,zero).0)
                    },
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
                    } => {
                        let words = reference_words(place, &self.locals).unwrap();
                        let components: Vec<_> = place.ty().components.iter().zip(words).map(|(&(scalar, _), word)| (scalar, word)).collect();
                        let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,expose_writes,Some(*offset),&components,span)?;
                        CallStorage::Indexed(IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))})
                    },
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
                (
                    storage,
                    Some(*offset),
                    reference_words(place, &self.locals).unwrap(),
                )
            }
            _ => return self.fail(span, "B264", "外部引用需要 CPU 普通值的实际存储"),
        };
        self.data_access(place, access, None);
        Ok(CallReference {
            storage,
            offset,
            word: words[0],
        })
    }
    pub(super) async fn external_reference(
        &mut self,
        place: &Place,
        access: Access,
        expose_writes: bool,
        span: Span,
    ) -> Result<CallReference> {
        if !unsafe { self.world.call(self.reference_is_contiguous(place, span)).await }? {
            return self.fail(span,"B264","C ABI 借用需要按参数布局连续、顺序匹配且对齐的真实存储；符号位置需由 requires contiguous 指明这一前提");
        }
        self.call_reference(place, access, expose_writes, span)
    }
    #[inline(never)]
    pub(super) async fn external_call(&mut self, instance: &crate::generics::Instance, args: Vec<Argument>, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Option<Value>> {
        let d=&instance.definition;
        let mut parameters=Vec::new();
        for param in d.function.params().iter() {
            let name=unsafe { self.world.call(instance.bindings.type_name(self.world,&param.ty(),
                (&d.source,&d.module,&d.imports),param.span())).await }?;
            let ty=unsafe { self.world.call(value_type(self.world,d,&name,param.span())).await }?;
            parameters.push(if param.access()==Access::Value {CallParameter::value(ty)}
                else {CallParameter::reference(ty,param.access())});
        }
        let result_name=unsafe { self.world.call(instance.bindings.type_name(self.world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())).await }?;
        let result = if result_name.name == "void" || result_name.is_never() { None }
            else { Some(unsafe { self.world.call(value_type(self.world, d, &result_name, d.function.span())).await }?) };
        unsafe { self.world.call(self.c_call(CallTarget::External({let name=d.function.external().unwrap();ExternalName{library:name.library().into(),symbol:name.symbol().into()}}), &parameters, args, result,
            result_name.is_never(), Some(instance.into()), span, generated_origin)).await }
    }
    pub(super) async fn function_value_call(&mut self, callee_span: Span, args: &Expressions, span: Span,
        lower: impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<Value>>) -> Result<Option<returns::Returned>> {
        unsafe { self.world.call(self.function_value_arguments(callee_span,span,lower,async |builder,parameters|
            unsafe { builder.world.call(builder.call_arguments(cooperative::CallParameters::CFunction(parameters),args,span)).await })).await }
    }
    pub(super) async fn function_value_arguments(&mut self,callee_span:Span,span:Span,
        lower:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>,
        prepare:impl std::ops::AsyncFnOnce(&mut Self,&[Param])->Result<Option<Vec<Argument>>>)->Result<Option<returns::Returned>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B265", "原生C函数地址只能在CPU位置调用");
        }
        let Some(value) = lower(self).await? else { return Ok(None); };
        let Some(function) = value.ty.function.as_deref() else {
            return self.fail(callee_span, "B263", "间接调用目标需要cfn原生C签名值");
        };
        if !value.is_nonzero() {
            return self.fail(callee_span, "B137", "调用cfn需要已证明的非零目标；使用同签名空值比较或requires nonzero");
        }
        let (parameters, result, never) = unsafe { self.world.call(self.world.c_function_signature(
            function, self.scope.source(), span, self.scope.module(), self.scope.imports())).await }?;
        let source_parameters = parameters.iter().enumerate().map(|(index, parameter)| {
            let (ty, access) = match &parameter.kind {
                CallParameterKind::Value(ty) => (ty, Access::Value),
                CallParameterKind::Reference { ty, access } => (ty, *access),
            };
            Param { name: format!("argument{index}"), ty: ty.syntax(), access, span }
        }).collect::<Vec<_>>();
        let Some(arguments) = prepare(self,&source_parameters).await? else { return Ok(None); };
        let scope = self.scope.clone();
        let contract = if function.contracts.is_empty() { None } else {
            Some(outputs::Contract::signature(scope.source(), scope.module(),
                scope.imports(), &source_parameters, &function.contracts, span))
        };
        unsafe { self.world.call(self.c_call(CallTarget::Indirect(value.regs[0]), &parameters, arguments, result, never, contract, span, None)).await }
            .map(|value| value.map(returns::Returned::value))
    }
    #[inline(never)]
    async fn c_call(&mut self, target: CallTarget, parameters: &[CallParameter], args: Vec<Argument>, result: Option<Ty>,
        never: bool, definition: Option<outputs::Contract<'_>>, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Option<Value>> {
        if self.program.target != Target::Cpu { return self.fail(span, "B265", "C ABI调用只能在CPU位置执行"); }
        let mut arguments = Vec::new();
        let mut written = Vec::new();
        let mut held = Vec::new();
        let mut output_inputs = BTreeMap::new();
        let opaque=definition.map(|contract|contract.external_memory()).unwrap_or(Some(Access::InOut));
        let mut memory=vec![(view::Borrow::source(view::Source::External),opaque)];
        for (index, (parameter, arg)) in parameters.iter().zip(args).enumerate() {
            let arg = if let Argument::Scoped { value, owner } = arg { held.push(owner); Argument::resource(value) } else { arg };
            let arg = match arg {
                Argument::Value(value) if matches!(&parameter.kind, CallParameterKind::Reference { access: Access::Read, .. }) =>
                    Argument::Reference(self.materialize_array(value,None)),
                other => other,
            };
            arguments.push(match (&parameter.kind, arg) {
                (CallParameterKind::Value(ty), Argument::Value(value)) => {
                    self.require_same(ty, &value.ty, span)?;
                    memory[0].0.union(&self.address_contents(&value.ty,&value.proof));
                    if let Some(d) = definition { output_inputs.insert(d.parameter_name(index), value.clone()); }
                    CallArgument::Value { #[cfg(feature = "compiler")] origin: None, dataset_borrow:None, ty: ty.clone(), values: value.regs }
                }
                (CallParameterKind::Reference { ty, access }, Argument::Reference(owner)) => {
                    if access.reads(){memory[0].0.union(&self.address_contents(ty,&owner.proof));}
                    let place = owner.place;
                    self.require_same(ty, place.ty(), span)?;
                    let origin=unsafe { self.world.call(self.position_origin(&place,span)).await }?;
                    memory.push((view::Borrow::source(view::Source::Position(Arc::new(origin))),Some(*access)));
                    let address = unsafe { self.world.call(self.external_reference(&place, *access, access.writes(), span)).await }?;
                    if access.writes() { written.push((index, place)); }
                    CallArgument::Reference { #[cfg(feature = "compiler")] origin: None, ty: ty.clone(), access: *access, address }
                }
                _ => return self.fail(span, "B263", "C ABI实参只接收普通值及实际位置"),
            });
        }
        if let Some(contract) = definition {
            unsafe { self.world.call(contracts::prove_c_inputs(self, contract, &output_inputs, span)).await }?;
        }
        let values = result.iter().flat_map(|ty| ty.components.iter().map(|(scalar, _)| *scalar))
            .map(|scalar| self.reg(scalar)).collect::<Vec<_>>();
        let memory_use=unsafe { self.world.call(self.call_address_memory(memory,span)).await }?;
        let kind=InstKind::Call(Call { failure:None, error:None, target, arguments, memory: Vec::new(), memory_use:Some(memory_use), result: result.clone(), values: values.clone() });
        let first = self.code.len();
        if let Some(origin)=generated_origin { self.code.push(Inst::generated(kind,origin)); }
        else { self.emit(kind); }
        if let Some(declaration) = definition.and_then(outputs::Contract::declaration_origin) {
            let declaration = Arc::new(declaration);
            for instruction in &mut self.code[first..] { instruction.reference_declaration(declaration.clone()); }
        }
        if never { self.flow = Flow::STOP; return Ok(None); }
        let returned = result.clone().map(|ty| Value {
            proof: bounds::Proof::default().with_external_borrows(&ty),
            ty, regs: values.clone(), literal: false, uniform: false, constant: None,
        });
        for (index, place) in written {
            let guards = if let Some(d) = definition {
                let guards = unsafe { self.world.call(outputs::guards(self, d, index, place.ty(), &output_inputs, returned.as_ref())).await }?;
                if outputs::declared(d, index) { self.forget_output_initialization(&place, span)?; }
                guards
            } else { vec![owned_presence::Presence::All; place.ty().components.len()] };
            unsafe { self.world.call(self.assign_owned_proof(&place, bounds::Proof::default().with_external_borrows(place.ty()), false)).await }?;
            self.initialize_output(&place, &guards);
        }
        for name in held { unsafe { self.world.call(self.finish_scoped_argument(&name, span)).await }?; }
        Ok(returned)
    }

}

// These are views of the existing Place mapping, not another storage owner.
pub(super) fn reference_words(place: &Place, locals: &BTreeMap<usize, Ty>) -> Option<Vec<u32>> {
    Some(match place {
        Place::Reference { words, .. }
        | Place::Element { words, .. }
        | Place::ApplicationElement { words, .. }
        | Place::ViewElement { words, .. } => words.clone(),
        Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => slots
            .iter()
            .map(|slot| locals[&storage.end].components[*slot - storage.start].1)
            .collect(),
        Place::Indexed { base, words, .. } => {
            let first = match &**base {
                Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                    locals[&storage.end].components[slots[0] - storage.start].1
                }
                _ => 0,
            };
            words.iter().map(|word| first + word).collect()
        }
        _ => return None,
    })
}
impl Builder<'_> {
    pub(super) async fn native_references(
        &mut self,
        place: &Place,
        access: Access,
        expose_writes: bool,
        span: Span,
        c_abi: bool,
    ) -> Result<Vec<(Ty, CallReference)>> {
        let expose_writes = access.writes() && expose_writes;
        if c_abi { return Ok(vec![(place.ty().clone(), unsafe { self.world.call(self.external_reference(place, access, expose_writes, span)).await }?)]); }
        let parameters = native_function::reference_parameters(place.ty());
        if parameters.is_empty() { return Ok(Vec::new()); }
        if parameters.len() == 1 {
            return Ok(vec![(
                parameters[0].clone(),
                self.call_reference(place, access, expose_writes, span)?,
            )]);
        }
        let Some(words) = reference_words(place, &self.locals) else {
            return self.fail(span, "B264", "原生借用需要CPU普通值的实际位置");
        };
        if let Place::Reference { address: ReferenceAddress::Components(_), .. } = place {
            return parameters.into_iter().enumerate().map(|(component, ty)| {
                let part = place.clone().project(ty.clone(), &[component]);
                Ok((ty, self.call_reference(&part, access, expose_writes, span)?))
            }).collect();
        }
        let base = self.call_reference(place, access, expose_writes, span)?;
        Ok(parameters
            .into_iter()
            .zip(words)
            .map(|(ty, word)| {
                let mut address = base.clone();
                address.word = word;
                (ty, address)
            })
            .collect())
    }
    pub(super) async fn reference_is_contiguous(&self, place: &Place, span: Span) -> Result<bool> {
        let ty = place.ty();
        if let Place::Reference {
            address: ReferenceAddress::Components(_),
            scope: Some(scope),
            words,
            ..
        } = place
        {
            if ty.components.len() == 1 {
                return Ok(true);
            }
            return unsafe { self.world.call(contracts::contiguous_assumed(self, *scope, words, ty, span)).await };
        }
        let Some(words) = reference_words(place, &self.locals) else {
            return self.fail(span, "B264", "C ABI借用需要CPU普通值的实际位置");
        };
        let first = words[0];
        Ok(first % ty.align == 0
            && words
                .iter()
                .zip(&ty.components)
                .all(|(word, (_, offset))| word.checked_sub(first) == Some(*offset)))
    }
}
