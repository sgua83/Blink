//! 外部调用的 ABI 声明是信任边界；实参借用统一判定别名，ABI 只投影实际位置。
use super::*;
use cooperative::Argument;

pub(crate) async fn check_declaration(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let signature = unsafe { world.call(crate::function_types::named_signature(world, instance)).await }?;
    unsafe { world.call(outputs::validate_named(
        world, instance, |index| signature.parameter_type(index), &signature.result_name, signature.result.as_ref(),
    )).await }
}

impl Builder<'_> {
    pub(super) async fn read_argument(&mut self,expr:&Expr,expected:Option<&Ty>)->Result<Option<Argument>> {
        let mut cursor=ReadInputCursor {phase:0};let mut facts=ReadInputFacts::default();
        let mut reference=None;let mut root=None;let mut place=None;let mut item=None;let mut owner=None;
        loop {let step=unsafe{read_input_step(cursor,facts)};cursor=step.cursor;match step.need {
            ReadInputNeed::Reference=>{reference=unsafe{self.world.call(self.application_reference(expr)).await}?;facts.present=u32::from(reference.is_some());},
            ReadInputNeed::BorrowedData=>return Ok(Some(Argument::BorrowedData(reference.take().unwrap()))),
            ReadInputNeed::Root=>{let name=unsafe{read_input_root(expr.signature_node())};root=(name.present!=0).then(||expr.source_view().text(name.value));facts.bound=u32::from(root.is_some_and(|name|self.env.contains_key(name)));},
            ReadInputNeed::IsPlace=>facts.place=u32::from(self.is_place(expr)),
            ReadInputNeed::Temporary=>facts.temporary=u32::from(self.temporary_projection(expr)),
            ReadInputNeed::Type=>facts.owns=u32::from(unsafe{self.world.call(self.read_expression_type(expr)).await}?.is_some_and(|ty|ty.owns_resources())),
            ReadInputNeed::Binding=>facts.value=u32::from(root.is_some_and(|name|matches!(self.env.get(name),Some(Binding::Value(_))))),
            ReadInputNeed::Materialize=>{let(name,Binding::Value(value))=self.env.remove_entry(root.unwrap()).unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let mut owner=self.materialize_array(value,None);owner.place=owner.place.read_only();self.env.insert(name,Binding::Owned(owner));},
            ReadInputNeed::Place=>{place=unsafe{self.world.call(self.place(expr)).await}?;facts.present=u32::from(place.is_some());},
            ReadInputNeed::Storage=>facts.storage=match place.as_ref().unwrap().storage(){Place::Local(..)|Place::Reference{..}|Place::ApplicationElement{..}=>1,Place::Element{..}=>2,_=>0},
            ReadInputNeed::Writable=>{let Place::Element{binding,..}=place.as_ref().unwrap().storage()else{unsafe{std::hint::unreachable_unchecked()}};facts.writable=u32::from(self.view_access[binding].writes());},
            ReadInputNeed::Initialize=>unsafe{self.world.call(self.require_initialized(place.as_ref().unwrap(),expr.span())).await}?,
            ReadInputNeed::Borrow=>return Ok(Some(Argument::Reference(self.borrow_owned(place.take().unwrap())))),
            ReadInputNeed::Aggregate=>{if let Some((value,name))=unsafe{self.world.call(self.read_aggregate(expr,expected)).await}?{item=Some(value);owner=name;}facts.present=u32::from(item.is_some());facts.kind=item.as_ref().map_or(0,returns::Returned::kind);facts.owner=u32::from(owner.is_some());},
            ReadInputNeed::Application=>{let returns::Returned::OwnedValue(value)=item.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};facts.application=u32::from(value.place.as_ref().is_some_and(|place|matches!(place.storage(),Place::ApplicationElement{..})));},
            ReadInputNeed::OwnedReference=>{let returns::Returned::OwnedValue(value)=item.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};return Ok(Some(Argument::OwnedReference{place:value.place().unwrap(),value,owner:None}));},
            ReadInputNeed::Scoped=>return Ok(Some(Argument::Scoped{value:item.take().unwrap(),owner:owner.take().unwrap()})),
            ReadInputNeed::Resource=>return Ok(Some(Argument::resource(item.take().unwrap()))),
            ReadInputNeed::Empty=>return Ok(None),
        }}
    }
    pub(super) fn call_reference(&mut self,place:&Place,access:Access,expose_writes:bool,span:Span)->Result<CallReference> {
        fn kind(place:&Place)->ReferenceKind {match place {Place::Reference{..}=>ReferenceKind::Reference,Place::Local(..)|Place::ReadOnlyLocal(..)=>ReferenceKind::Local,Place::Element{..}=>ReferenceKind::Element,Place::ApplicationElement{..}=>ReferenceKind::Application,Place::ViewElement{..}=>ReferenceKind::View,Place::Indexed{..}=>ReferenceKind::Indexed,_=>ReferenceKind::Other}}
        let mut cursor=ReadInputCursor {phase:0};let mut facts=CallReferenceFacts {atomic:u32::from(self.atomic_storage(place)),access,kind:ReferenceKind::Other,base:ReferenceKind::Other,cross:0};
        let mut element=None;let mut result=None;
        loop {let step=unsafe{call_reference_step(cursor,facts)};cursor=step.cursor;match step.need {
            CallReferenceNeed::BadAtomic=>return self.fail(span,"B214","原子位置只能转借给atomic参数"),
            CallReferenceNeed::Kind=>facts.kind=kind(place),
            CallReferenceNeed::Base=>{let Place::Indexed{base,..}=place else{unsafe{std::hint::unreachable_unchecked()}};facts.base=kind(base);},
            CallReferenceNeed::ElementFacts=>{let source=match place{Place::Indexed{base,..}=>&**base,place=>place};let Place::Element{binding,index,..}=source else{unsafe{std::hint::unreachable_unchecked()}};element=Some((*binding,*index));facts.cross=u32::from(*index!=self.global_index);},
            CallReferenceNeed::Read=>{self.reads.insert(element.unwrap().0);},
            CallReferenceNeed::Write=>{self.writes.insert(element.unwrap().0);},
            CallReferenceNeed::Cross=>{self.cross_access.insert(element.unwrap().0);},
            CallReferenceNeed::Reference=>{let Place::Reference{address,words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let zero=self.constant(Scalar::U32,0).regs[0];let(source,word)=address.source_at(words[0],zero);result=Some(CallReference{storage:CallStorage::Indexed(source),offset:None,word});},
            CallReferenceNeed::Local=>{let(Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage))=place else{unsafe{std::hint::unreachable_unchecked()}};result=Some(CallReference{storage:CallStorage::Local{base:storage.start,len:storage.len()},offset:None,word:first_reference_word(place,&self.locals)});},
            CallReferenceNeed::Element=>{let Place::Element{words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let(binding,index)=element.unwrap();result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::Data{binding,index}),offset:None,word:words[0]});},
            CallReferenceNeed::Application=>{let Place::ApplicationElement{handle,identity,index,stride,words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let components=place.ty().components.iter().zip(words).map(|(&(scalar,_),&word)|(scalar,word));let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,expose_writes,None,components,span)?;result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))}),offset:None,word:words[0]});},
            CallReferenceNeed::View=>{let Place::ViewElement{address,index,stride,words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::View{address:*address,index:*index,stride:*stride}),offset:None,word:words[0]});},
            CallReferenceNeed::IndexedReference=>{let Place::Indexed{base,offset,words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let scalar=place.ty().components[0].0;let(source,offset,word)=self.array_memory(base,*offset,words[0],scalar,expose_writes,span)?;result=Some(CallReference{storage:CallStorage::Indexed(source),offset:Some(offset),word});},
            CallReferenceNeed::IndexedLocal=>{let Place::Indexed{base,offset,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let(Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage))=&**base else{unsafe{std::hint::unreachable_unchecked()}};result=Some(CallReference{storage:CallStorage::Local{base:storage.start,len:storage.len()},offset:Some(*offset),word:first_reference_word(place,&self.locals)});},
            CallReferenceNeed::IndexedElement=>{let Place::Indexed{offset,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let(binding,index)=element.unwrap();result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::Data{binding,index}),offset:Some(*offset),word:first_reference_word(place,&self.locals)});},
            CallReferenceNeed::IndexedApplication=>{let Place::Indexed{base,offset,words,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let Place::ApplicationElement{handle,identity,index,stride,..}=&**base else{unsafe{std::hint::unreachable_unchecked()}};let components=place.ty().components.iter().zip(words).map(|(&(scalar,_),&word)|(scalar,word));let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,expose_writes,Some(*offset),components,span)?;result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))}),offset:Some(*offset),word:first_reference_word(place,&self.locals)});},
            CallReferenceNeed::IndexedView=>{let Place::Indexed{base,offset,..}=place else{unsafe{std::hint::unreachable_unchecked()}};let Place::ViewElement{address,index,stride,..}=&**base else{unsafe{std::hint::unreachable_unchecked()}};result=Some(CallReference{storage:CallStorage::Indexed(IndexedSource::View{address:*address,index:*index,stride:*stride}),offset:Some(*offset),word:first_reference_word(place,&self.locals)});},
            CallReferenceNeed::BadStorage=>return self.fail(span,"B264","外部引用需要 CPU 普通值的实际存储"),
            CallReferenceNeed::DataAccess=>self.data_access(place,access,None),
            CallReferenceNeed::Complete=>return Ok(result.take().unwrap()),
        }}
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
        let d = &instance.definition;
        let signature = unsafe { self.world.call(crate::function_types::named_signature(self.world, instance)).await }?;
        let name = d.function.external().unwrap();
        let target = CallTarget::External(ExternalName {library: name.library().into(), symbol: name.symbol().into()});
        unsafe { self.world.call(self.c_call(
            target, &signature.parameters, args, signature.result, signature.never,
            Some(instance.into()), span, generated_origin,
        )).await }
    }

    pub(super) async fn function_value_call(&mut self, callee_span: Span, args: &Expressions, span: Span,
        lower: impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<Value>>) -> Result<Option<returns::Returned>> {
        unsafe { self.world.call(self.function_value_arguments(callee_span,span,lower,async |builder,parameters|
            unsafe { builder.world.call(builder.call_arguments(cooperative::CallParameters::CFunction(parameters,span),args,span)).await })).await }
    }
    pub(super) async fn function_value_arguments(&mut self,callee_span:Span,span:Span,
        lower:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>,
        prepare:impl std::ops::AsyncFnOnce(&mut Self,&[CallParameter])->Result<Option<Vec<Argument>>>)->Result<Option<returns::Returned>> {
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
        let Some(arguments) = prepare(self,&parameters).await? else { return Ok(None); };
        let scope = self.scope.clone();
        let contract = if function.contracts.is_empty() { None } else {
            Some(outputs::Contract::signature(scope.source(), scope.module(),
                scope.imports(), parameters.len(), &function.contracts, span))
        };
        unsafe { self.world.call(self.c_call(CallTarget::Indirect(value.regs[0]), &parameters, arguments, result, never, contract, span, None)).await }
            .map(|value| value.map(returns::Returned::value))
    }
    #[inline(never)]
    async fn c_call(&mut self, target: CallTarget, parameters: &[CallParameter], args: Vec<Argument>, result: Option<Ty>,
        never: bool, definition: Option<outputs::Contract<'_>>, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Option<Value>> {
        fn kind(argument:&Argument)->CArgumentKind {match argument {Argument::Scoped{..}=>CArgumentKind::Scoped,Argument::Value(_)=>CArgumentKind::Value,Argument::Reference(_)=>CArgumentKind::Reference,_=>CArgumentKind::Other}}
        let mut cursor=CCallCursor {index:0,phase:0};
        let mut facts=CCallFacts {cpu:u32::from(self.program.target==Target::Cpu),never:u32::from(never),contract:u32::from(definition.is_some()),capture_names:0,reference:0,access:Access::Value,kind:CArgumentKind::Other,captured:0,declared:0};
        let count=parameters.len().min(args.len()) as u64;let mut source=args.into_iter();
        let mut arguments=Vec::new();let mut written=Vec::new();let mut held=Vec::new();let mut output_inputs=BTreeMap::new();
        let mut memory=Vec::new();let mut capture_names=BTreeSet::new();let mut argument=None;let mut captured=None;
        let mut place=None;let mut address=None;let mut values=Vec::new();let mut memory_use=None;
        let mut target=Some(target);let mut generated_origin=generated_origin;
        loop {let index=cursor.index as usize;let step=unsafe{c_call_step(cursor,facts,count,0,0)};cursor=step.cursor;match step.need {
            CCallNeed::BadTarget=>return self.fail(span,"B265","C ABI调用只能在CPU位置执行"),
            CCallNeed::Inputs=>{let(opaque,names)=definition.map_or_else(||(Some(Access::InOut),BTreeSet::new()),|contract|contract.c_call_inputs(!never));capture_names=names;facts.capture_names=u32::from(!capture_names.is_empty());memory.push((view::Borrow::source(view::Source::External),opaque));},
            CCallNeed::Inspect=>{argument=source.next();facts.kind=kind(argument.as_ref().unwrap());facts.reference=u32::from(matches!(parameters[index].kind,CallParameterKind::Reference{..}));facts.access=match &parameters[index].kind{CallParameterKind::Value(_)=>Access::Value,CallParameterKind::Reference{access,..}=>*access};facts.captured=0;captured=None;},
            CCallNeed::Scoped=>{let Argument::Scoped{value,owner}=argument.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};held.push(owner);argument=Some(Argument::resource(value));facts.kind=kind(argument.as_ref().unwrap());},
            CCallNeed::Materialize=>{let Argument::Value(value)=argument.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};argument=Some(Argument::Reference(self.materialize_array(value,None)));facts.kind=CArgumentKind::Reference;},
            CCallNeed::Value=>{let(CallParameterKind::Value(ty),Argument::Value(value))=(&parameters[index].kind,argument.as_ref().unwrap())else{unsafe{std::hint::unreachable_unchecked()}};self.require_same(ty,&value.ty,span)?;memory[0].0.union(&self.address_contents(&value.ty,&value.proof));},
            CCallNeed::CaptureName=>{let name=definition.unwrap().parameter_name(index);facts.captured=u32::from(capture_names.contains(&name));captured=Some(name);},
            CCallNeed::CapturedValue|CCallNeed::PlainValue=>{let Argument::Value(value)=argument.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let registers=if matches!(step.need,CCallNeed::CapturedValue){let registers=value.regs.clone();output_inputs.insert(captured.take().unwrap(),value);registers}else{value.regs};let CallParameterKind::Value(ty)=&parameters[index].kind else{unsafe{std::hint::unreachable_unchecked()}};arguments.push(CallArgument::Value{#[cfg(feature="compiler")]origin:None,dataset_borrow:None,ty:ty.clone(),values:registers});},
            CCallNeed::Reference=>{let Argument::Reference(owner)=argument.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let CallParameterKind::Reference{ty,..}=&parameters[index].kind else{unsafe{std::hint::unreachable_unchecked()}};if step.reading!=0{memory[0].0.union(&self.address_contents(ty,&owner.proof));}place=Some(owner.place);self.require_same(ty,place.as_ref().unwrap().ty(),span)?;let origin=unsafe{self.world.call(self.position_origin(place.as_ref().unwrap(),span)).await}?;memory.push((view::Borrow::source(view::Source::Position(Arc::new(origin))),Some(facts.access)));},
            CCallNeed::Address=>address=Some(unsafe{self.world.call(self.external_reference(place.as_ref().unwrap(),facts.access,facts.access.writes(),span)).await}?),
            CCallNeed::WrittenReference|CCallNeed::PlainReference=>{let CallParameterKind::Reference{ty,access}=&parameters[index].kind else{unsafe{std::hint::unreachable_unchecked()}};if matches!(step.need,CCallNeed::WrittenReference){written.push((index,place.take().unwrap()));}else{place=None;}arguments.push(CallArgument::Reference{#[cfg(feature="compiler")]origin:None,ty:ty.clone(),access:*access,address:address.take().unwrap()});},
            CCallNeed::Invalid=>return self.fail(span,"B263","C ABI实参只接收普通值及实际位置"),
            CCallNeed::Prove=>unsafe{self.world.call(contracts::prove_c_inputs(self,definition.unwrap(),&output_inputs,span)).await}?,
            CCallNeed::Registers=>values=result.iter().flat_map(|ty|ty.components.iter().map(|(scalar,_)|*scalar)).map(|scalar|self.reg(scalar)).collect(),
            CCallNeed::Memory=>memory_use=Some(unsafe{self.world.call(self.call_address_memory(std::mem::take(&mut memory),span)).await}?),
            CCallNeed::Emit=>{let kind=InstKind::Call(Call{failure:None,error:None,target:target.take().unwrap(),arguments:std::mem::take(&mut arguments),memory:Vec::new(),memory_use:memory_use.take(),result:result.clone(),values:values.clone()});let first=self.code.len();if let Some(origin)=generated_origin.take(){self.code.push(Inst::generated(kind,origin));}else{self.emit(kind);}if let Some(declaration)=definition.and_then(outputs::Contract::declaration_origin){let declaration=Arc::new(declaration);for instruction in &mut self.code[first..]{instruction.reference_declaration(declaration.clone());}}},
            CCallNeed::Stop=>{self.flow=Flow::STOP;return Ok(None);},
            CCallNeed::Returned=>break,
            _=>unsafe{std::hint::unreachable_unchecked()},
        }}
        let returned=result.map(|ty|Value{proof:bounds::Proof::default().with_external_borrows(&ty),ty,regs:values,literal:false,uniform:false,constant:None});
        let written_count=written.len() as u64;let held_count=held.len() as u64;let mut written=written.into_iter();let mut held=held.into_iter();
        let mut output=None;let mut guards=Vec::new();
        loop {let step=unsafe{c_call_step(cursor,facts,count,written_count,held_count)};cursor=step.cursor;match step.need {
            CCallNeed::WrittenInspect=>output=written.next(),
            CCallNeed::Guards=>{let(index,place)=output.as_ref().unwrap();let d=definition.unwrap();guards=unsafe{self.world.call(outputs::guards(self,d,*index,place.ty(),&output_inputs,returned.as_ref())).await}?;facts.declared=u32::from(outputs::declared(d,*index));},
            CCallNeed::Forget=>self.forget_output_initialization(&output.as_ref().unwrap().1,span)?,
            CCallNeed::AllGuards=>guards=vec![owned_presence::Presence::All;output.as_ref().unwrap().1.ty().components.len()],
            CCallNeed::Assign=>{let place=&output.as_ref().unwrap().1;unsafe{self.world.call(self.assign_owned_proof(place,bounds::Proof::default().with_external_borrows(place.ty()),false)).await}?;},
            CCallNeed::Initialize=>{let(_,place)=output.take().unwrap();self.initialize_output(&place,&guards);},
            CCallNeed::Finish=>{let name=held.next().unwrap();unsafe{self.world.call(self.finish_scoped_argument(&name,span)).await}?;},
            CCallNeed::Complete=>return Ok(returned),
            _=>unsafe{std::hint::unreachable_unchecked()},
        }}
    }

}

// Read the real Place mapping. Only its owning consumers collect these words.
enum ReferenceWords<'a> {
    Physical(&'a [u32], u32),
    Local(&'a [usize], &'a std::ops::Range<usize>, &'a BTreeMap<usize, Ty>),
}

impl<'a> ReferenceWords<'a> {
    fn iter(self) -> impl ExactSizeIterator<Item = u32> + 'a {
        let count = match &self {
            Self::Physical(words, _) => words.len(),
            Self::Local(slots, _, _) => slots.len(),
        };
        (0..count).map(move |index| match &self {
            Self::Physical(words, first) => *first + words[index],
            Self::Local(slots, storage, locals) => locals[&storage.end].components[slots[index] - storage.start].1,
        })
    }
}

fn reference_word_source<'a>(place: &'a Place, locals: &'a BTreeMap<usize, Ty>) -> Option<ReferenceWords<'a>> {
    Some(match place {
        Place::Reference { words, .. }
        | Place::Element { words, .. }
        | Place::ApplicationElement { words, .. }
        | Place::ViewElement { words, .. } => ReferenceWords::Physical(words, 0),
        Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
            ReferenceWords::Local(slots, storage, locals)
        }
        Place::Indexed { base, words, .. } => {
            let first = match &**base {
                Place::Local(_, slots, storage) | Place::ReadOnlyLocal(_, slots, storage) => {
                    locals[&storage.end].components[slots[0] - storage.start].1
                }
                _ => 0,
            };
            ReferenceWords::Physical(words, first)
        }
        _ => return None,
    })
}

fn first_reference_word(place: &Place, locals: &BTreeMap<usize, Ty>) -> u32 {
    reference_word_source(place, locals).unwrap().iter().next().unwrap()
}

pub(super) fn reference_words(place: &Place, locals: &BTreeMap<usize, Ty>) -> Option<Vec<u32>> {
    Some(reference_word_source(place, locals)?.iter().collect())
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
        let ty = place.ty();
        match function_inputs::reference_layout(ty, false).form {
            function_inputs::ReferenceForm::Empty => return Ok(Vec::new()),
            function_inputs::ReferenceForm::Whole => {
                let reference = self.call_reference(place, access, expose_writes, span)?;
                return Ok(vec![(ty.clone(), reference)]);
            }
            function_inputs::ReferenceForm::Split => {}
        }
        if let Place::Reference { address: ReferenceAddress::Components(_), .. } = place {
            return ty.components.iter().enumerate().map(|(component, &(scalar, _))| {
                let ty = Ty::scalar(scalar);
                let part = place.clone().project(ty.clone(), &[component]);
                Ok((ty, self.call_reference(&part, access, expose_writes, span)?))
            }).collect();
        }
        let Some(words) = reference_words(place, &self.locals) else {
            return self.fail(span, "B264", "原生借用需要CPU普通值的实际位置");
        };
        let base = self.call_reference(place, access, expose_writes, span)?;
        Ok(ty.components.iter().zip(words).map(|(&(scalar, _), word)| {
            let mut address = base.clone();
            address.word = word;
            (Ty::scalar(scalar), address)
        }).collect())
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
        let Some(words) = reference_word_source(place, &self.locals) else {
            return self.fail(span, "B264", "C ABI借用需要CPU普通值的实际位置");
        };
        let mut words = words.iter();
        let first = words.next().unwrap();
        Ok(first % ty.align == 0
            && std::iter::once(first).chain(words)
                .zip(&ty.components)
                .all(|(word, (_, offset))| word.checked_sub(first) == Some(*offset)))
    }
}

#[repr(C)]#[derive(Clone,Copy)]struct ReadInputCursor {phase:u32}
#[repr(C)]#[derive(Clone,Copy,Default)]struct ReadInputFacts {present:u32,bound:u32,place:u32,temporary:u32,owns:u32,value:u32,storage:u32,writable:u32,kind:u32,owner:u32,application:u32}
#[repr(u32)]enum ReadInputNeed {Reference,BorrowedData,Root,IsPlace,Temporary,Type,Binding,Materialize,Place,Storage,Writable,Initialize,Borrow,Aggregate,Application,OwnedReference,Scoped,Resource,Empty}
#[repr(C)]struct ReadInputStep {cursor:ReadInputCursor,need:ReadInputNeed}
unsafe extern "C" {
 #[link_name="bootstrap.call_arguments::read_input_root"]fn read_input_root(source:crate::syntax::wire::SignatureNode)->crate::syntax::wire::Optional<crate::syntax::wire::Text>;
 #[link_name="bootstrap.call_arguments::read_input_step"]fn read_input_step(cursor:ReadInputCursor,facts:ReadInputFacts)->ReadInputStep;
}

#[repr(u32)]#[derive(Clone,Copy)]enum ReferenceKind {Other,Reference,Local,Element,Application,View,Indexed}
#[repr(C)]#[derive(Clone,Copy)]struct CallReferenceFacts {atomic:u32,access:Access,kind:ReferenceKind,base:ReferenceKind,cross:u32}
#[repr(u32)]enum CallReferenceNeed {Kind,BadAtomic,Base,ElementFacts,Read,Write,Cross,Reference,Local,Element,Application,View,IndexedReference,IndexedLocal,IndexedElement,IndexedApplication,IndexedView,BadStorage,DataAccess,Complete}
#[repr(C)]struct CallReferenceStep {cursor:ReadInputCursor,need:CallReferenceNeed}
unsafe extern "C" {#[link_name="bootstrap.call_arguments::call_reference_step"]fn call_reference_step(cursor:ReadInputCursor,facts:CallReferenceFacts)->CallReferenceStep;}

#[repr(C)]#[derive(Clone,Copy)]struct CCallCursor {index:u64,phase:u32}
#[repr(u32)]#[derive(Clone,Copy)]enum CArgumentKind {Other,Value,Reference,Scoped}
#[repr(C)]#[derive(Clone,Copy)]struct CCallFacts {cpu:u32,never:u32,contract:u32,capture_names:u32,reference:u32,access:Access,kind:CArgumentKind,captured:u32,declared:u32}
#[repr(u32)]enum CCallNeed {BadTarget,Inputs,Inspect,Scoped,Materialize,Value,CaptureName,CapturedValue,PlainValue,Reference,Address,WrittenReference,PlainReference,Invalid,Prove,Registers,Memory,Emit,Stop,Returned,WrittenInspect,Guards,Forget,AllGuards,Assign,Initialize,Finish,Complete}
#[repr(C)]struct CCallStep {cursor:CCallCursor,need:CCallNeed,reading:u32}
unsafe extern "C" {#[link_name="bootstrap.call_arguments::c_call_step"]fn c_call_step(cursor:CCallCursor,facts:CCallFacts,arguments:u64,written:u64,held:u64)->CCallStep;}
