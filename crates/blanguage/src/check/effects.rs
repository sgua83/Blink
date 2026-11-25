//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;
use crate::syntax::wire::{ExprRef, Optional, SignatureNode, Slice, StmtRef, StorageView, Text, TypeRef};
use std::ops::ControlFlow;
mod collection;
mod loop_projection;
mod sources;
use sources::{Local, Scope};

type ComponentProjection = (Ty, Vec<Vec<usize>>);

// One original future-write query: resource identity can precede emitted storage.
pub(super) struct DataProjection<'a> {
    data: Option<&'a application::Data>,
    pub(super) resource: view::Borrow,
    pub(super) base: Ty,
    access: Access,
    pub(super) index: Option<bounds::Operand>,
    pub(super) value: ComponentProjection,
}


enum Write {
    Move { place: Expr, when: bounds::Condition },
    Position {
        place: Expr,
        call: Option<Expr>,
        contents: bool,
        iterator: Option<(String, Expr)>,
        when: Option<bounds::Condition>,
        local_value: bool,
    },
    External { call:Expr, instance:Option<crate::generics::Instance>, arguments:BTreeMap<ValueIdentity,bounds::Proof>, syntax:Expressions },
    Memory {
        call: Expr,
        borrow: view::Borrow,
        effect: std::cell::OnceCell<Option<address::MemoryEffect>>,
    },
    Generator(Expr),
}

// The existing write consumer borrows its captures through one original call.
trait WriteSink {
    fn accept<'a>(&'a mut self, world: &'a World, value: Write) -> crate::frontend::Call<'a, Result<()>>;
}
impl<F: std::ops::AsyncFnMut(Write) -> Result<()>> WriteSink for F {
    fn accept<'a>(&'a mut self, world: &'a World, value: Write) -> crate::frontend::Call<'a, Result<()>> {
        unsafe { world.call(self(value)) }
    }
}

impl Write {
    fn memory(call: Expr, borrow: view::Borrow) -> Self {
        Self::Memory { call, borrow, effect: std::cell::OnceCell::new() }
    }

    fn memory_effect(&self) -> Option<&address::MemoryEffect> {
        let Self::Memory { borrow, effect, .. } = self else { return None; };
        effect.get_or_init(|| Builder::address_write_effect(borrow)).as_ref()
    }

    fn restrict_move(&mut self,condition:&bounds::Condition) {
        if let Self::Move{when,..}=self{*when=when.clone().logical(condition.clone(),false);}
    }
}

pub(super) enum StorageWrite {
    Frame { root:Reg, range:Option<std::ops::Range<u128>> },
    Descendants { root:Reg },
}
// Immutable output of the existing loop write preanalysis, not memory state.
#[derive(Default)]
pub(super) struct LoopWrites {
    pub locals: BTreeSet<usize>,
    pub storage: Vec<StorageWrite>,
    pub unknown_storage: bool,
    pub external: bool,
    pub body_bounds: Vec<(String, (u32, u32))>,
}

impl Builder<'_> {
    // 只投影可能改写的现有所有者，不求值下标或再次生成表达式代码。
    async fn written_projection(&self,expr:&Expr,name:&str,owner:&Place,iterator:Option<(&str,&Expr)>,writes:&[Write])->Result<Option<ComponentProjection>> {
        unsafe { self.world.call(self.component_projection(expr,name,owner.ty(),Some(owner),iterator,&|index|self.constant_index(index,writes))).await }
    }
    async fn component_projection(
        &self,
        expr: &Expr,
        name: &str,
        root_ty: &Ty,
        owner: Option<&Place>,
        iterator: Option<(&str, &Expr)>,
        known_index: &impl Fn(&Expr)->Result<Option<u32>>,
    ) -> Result<Option<ComponentProjection>> {
        match expr.kind() {
            ExprKind::Name(root) => {
                if let Some((iterator, source)) = iterator
                    && root == iterator
                {
                    return Ok(owner.and_then(|owner|self.application_write_projection(source,owner)));
                }
                if root == name {
                    let ty = root_ty.clone();
                    let components = (0..ty.components.len()).map(|i| vec![i]).collect();
                    return Ok(Some((ty, components)));
                }
                if let Some(Binding::Owned(source)) = self.env.get(root)
                    && let Place::ApplicationElement { identity: source_id, words: source_words, index_range: source_range, .. } = &source.place
                    && let Some(owner) = owner
                    && let Place::ApplicationElement { identity, words, index_range, .. } = owner.storage()
                    && source_id == identity
                {
                    let overlaps = source_range.0 <= index_range.1 && index_range.0 <= source_range.1;
                    let components = source_words.iter().map(|word| {
                        if !overlaps { Vec::new() }
                        else if matches!(owner, Place::ApplicationElement { .. }) {
                            words.iter().enumerate().filter_map(|(i, target)| (target == word).then_some(i)).collect()
                        } else if words.contains(word) { (0..owner.ty().components.len()).collect() }
                        else { Vec::new() }
                    }).collect();
                    return Ok(Some((source.place.ty().clone(), components)));
                }
                Ok(None)
            }
            ExprKind::Member(base, field) => {
                let Some((ty, components)) =
                    unsafe { self.world.call(self.component_projection(&base,name,root_ty,owner,iterator,known_index)).await }?
                else {
                    return Ok(None);
                };
                let (ty, selected) = member_type(&ty, field, self.scope.source(), expr.span())?;
                Ok(Some((
                    ty,
                    selected.iter().map(|i| components[*i].clone()).collect(),
                )))
            }
            ExprKind::Index(base, index) => {
                if let Some((ty, mut components)) = owner.and_then(|owner|self.application_write_projection(&base,owner)) {
                    if let (Some(index), Some(Place::ApplicationElement { index_range, .. })) =
                        (known_index(&index)?, owner.map(Place::storage))
                        && (index < index_range.0 || index > index_range.1)
                    {
                        for selected in &mut components {
                            selected.clear();
                        }
                    }
                    return Ok(Some((ty, components)));
                }
                if let ExprKind::Name(root) = base.kind()
                    && let Some(Binding::Data(view)) = self.env.get(root)
                {
                    if let Some(Place::Element { binding, words, .. }) = owner
                        && *binding == view.binding
                    {
                        let (base, ty) = self.dataset_layout(view);
                        let ty = ty.clone();
                        let components = ty
                            .components
                            .iter()
                            .map(|(_, offset)| {
                                words
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, word)| (*word == base + offset).then_some(i))
                                    .collect()
                            })
                            .collect();
                        return Ok(Some((ty, components)));
                    }
                    return Ok(None);
                }
                let Some((ty, components)) =
                    unsafe { self.world.call(self.component_projection(&base,name,root_ty,owner,iterator,known_index)).await }?
                else {
                    return Ok(None);
                };
                let Some(element) = &ty.array else {
                    return Ok(None);
                };
                let constant = known_index(&index)?;
                let width = element.components.len();
                let elements = loop_projection::array_elements(ty.values[0], constant);
                let selected = (0..width)
                    .map(|component| {
                        elements
                            .clone()
                            .flat_map(|i| components[i * width + component].iter().copied())
                            .collect()
                    })
                    .collect();
                Ok(Some(((**element).clone(), selected)))
            }
            ExprKind::Specialize(name,explicit,args) if name=="unsafe_position"=>{
                let Some(owner)=owner else{return Ok(None);};
                if args.len()!=1{return Ok(None);}
                let explicit=unsafe { self.world.call(self.scope.bindings().arguments(self.world,&explicit,
                    (self.scope.source(),self.scope.module(),self.scope.imports()),expr.span())).await }?;
                let [GenericArgument::Type(syntax)]=explicit.as_slice() else{return Ok(None);};
                let ty=unsafe { self.world.call(self.world.ty(&syntax,self.scope.source(),expr.span(),self.scope.module(),self.scope.imports())).await }?;
                // Syntax has not read the address; actual Memory and loop producers resolve its writes.
                let components=vec![(0..owner.ty().components.len()).collect();ty.components.len()];
                Ok(Some((ty,components)))
            }
            _ => Ok(None),
        }
    }
    fn constant_index(&self, index: &Expr, writes: &[Write]) -> Result<Option<u32>> {
        Ok(match index.kind() {
            ExprKind::StaticU32(value) => Some(value),
            ExprKind::Number(text) => {
                let (ty, value) = number(self.scope.source(), index.span(), text)?;
                (ty == Ty::scalar(Scalar::U32)).then_some(value as u32)
            }
            ExprKind::Name(name)
                if !writes
                    .iter()
                    .any(|write| match write {
                        Write::External{..}|Write::Generator(_)=>true,
                        Write::Memory{..}=>self.env.get(name).and_then(Binding::stored_place).is_some_and(|owner|!self.memory_effect_components(write.memory_effect(), &owner, false).is_empty()),
                        Write::Position{contents:false,place,..}=>assigned_root(&place).as_deref()==Some(name)||address_write(place),
                        _=>false,
                    }) =>
            {
                match self.env.get(name) {
                    Some(Binding::Value(value)) if value.ty == Ty::scalar(Scalar::U32) => {
                        value.constant.as_ref().map(|bits| bits[0] as u32)
                    }
                    Some(Binding::Owned(owner)) if *owner.place.ty() == Ty::scalar(Scalar::U32) => {
                        owner
                            .proof
                            .known_value(owner.place.ty())
                            .map(|bits| bits[0] as u32)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }
    fn written_application_data(&self, expr: &Expr) -> Option<(&application::Data,u32,&Ty)> {
        if let ExprKind::Name(name) = expr.kind() {
            match self.env.get(name) {
                Some(Binding::ApplicationData(data))=>return Some((data,0,&data.ty)),
                Some(Binding::DataReference(data))=>{let(offset,ty)=data.element_layout();return Some((&data.data,offset,ty));},
                _=>{},
            }
        }
        if let ExprKind::Member(base,field)=expr.kind() && let Some((data,offset,ty))=self.written_application_data(&base) {
            let(word,ty)=ty.field(field)?;return Some((data,offset+word,ty));
        }
        let projection = self.owned_projection(expr)?;
        let Binding::OwnedValue(owner) = self.env.get(&projection.root)? else {
            return None;
        };
        let mut owner = owner;
        for index in projection.path {
            match owner.payloads[index].as_deref()? {
                returns::Returned::Dataset(data) => return Some((data,0,&data.ty)),
                returns::Returned::OwnedValue(child) => owner = child,
                _ => return None,
            }
        }
        None
    }
    fn application_write_projection(
        &self,
        expr: &Expr,
        owner: &Place,
    ) -> Option<ComponentProjection> {
        let (data,offset,ty) = self.written_application_data(expr)?;
        let Place::ApplicationElement {
            identity, words, ..
        } = owner.storage()
        else {
            return None;
        };
        if *identity != data.root.identity() {
            return None;
        }
        let components = ty
            .components
            .iter()
            .map(|(_, at)| {
                if matches!(owner, Place::ApplicationElement { .. }) {
                    words
                        .iter()
                        .enumerate()
                        .filter_map(|(i, word)| (*word == offset+*at).then_some(i))
                        .collect()
                } else if words.contains(&(offset+*at)) {
                    (0..owner.ty().components.len()).collect()
                } else {
                    Vec::new()
                }
            })
            .collect();
        Some((ty.clone(), components))
    }
    async fn loop_write_projection<'a,'e>(
        &'a self,expr:&Expr,known_index:&impl std::ops::AsyncFn(&Expr,bool)->Result<(Option<bounds::Operand>,Option<bounds::Condition>)>,
    )->Result<Option<(DataProjection<'a>,&'a application::Data,Option<bounds::Condition>)>> {
        match expr.kind() {
            ExprKind::Name(name)=>{
                let Some(Binding::Owned(owner))=self.env.get(name)else{return Ok(None);};
                let Place::ApplicationElement{identity,words,index_value,..}=&owner.place else{return Ok(None);};
                let Some(data)=self.env.values().flat_map(Binding::application_resources)
                    .find(|data|data.root.identity()==*identity)else{return Ok(None);};
                let components=words.iter().map(|word|data.ty.components.iter().enumerate()
                    .filter_map(|(i,(_,offset))|(word==offset).then_some(i)).collect()).collect();
                let index=Some(*index_value);
                Ok(Some((DataProjection{data:Some(data),resource:view::resource_borrow(data),base:data.ty.clone(),access:data.access,index,value:(owner.place.ty().clone(),components)},data,None)))
            },
            ExprKind::Index(base,index)=>{
                if let Some((data,offset,ty))=self.written_application_data(&base) {
                    let components=ty.components.iter().map(|part|vec![data.ty.components.iter().position(|base|loop_projection::part_matches(*base,*part,offset)).unwrap()]).collect();
                    let(index,when)=known_index(&index,true).await?;
                    return Ok(Some((DataProjection{data:Some(data),resource:view::resource_borrow(data),base:data.ty.clone(),access:data.access,index,value:(ty.clone(),components)},data,when)));
                }
                let Some((mut projection,input,mut when))=unsafe { self.world.call(self.loop_write_projection(&base,known_index)).await }? else{return Ok(None);};
                let (ty,components)=projection.value;
                if let Some(element)=&ty.dataset {
                    let mut resource=view::Borrow::default();
                    for fields in view::resource_paths(&projection.base) {
                        let (mut offset,mut field)=(0,&projection.base);
                        for at in &fields{let(word,next)=field.aggregate_part(*at);offset+=word;field=next;}
                        if field.components.iter().zip(&components).all(|(part,parts)|parts.iter().any(|i|loop_projection::part_matches(projection.base.components[*i],*part,offset))) {
                            resource.union(&view::Borrow::element(projection.resource.clone(),fields,projection.index));
                        }
                    }
                    let element=unsafe { self.world.call(self.world.ty(element,self.scope.source(),expr.span(),self.scope.module(),self.scope.imports())).await }?;
                    let components=(0..element.components.len()).map(|i|vec![i]).collect();
                    let(index,selection)=known_index(&index,true).await?;
                    when=when.map(|when|when.scope_element_indices(1));
                    if let Some(selection)=selection{when=Some(match when{Some(previous)=>previous.logical(selection,false),None=>selection});}
                    projection.data=None;projection.resource=resource;projection.base=element.clone();projection.index=index;projection.value=(element,components);
                } else {
                    let Some(element)=&ty.array else{return Ok(None);};
                    let constant=match known_index(&index,false).await?.0{Some(bounds::Operand::Constant(Scalar::U32,value))=>Some(value as u32),_=>None};let width=element.components.len();
                    let elements=loop_projection::array_elements(ty.values[0],constant);
                    let selected=(0..width).map(|component|elements.clone().flat_map(|i|components[i*width+component].iter().copied()).collect()).collect();
                    projection.value=((**element).clone(),selected);
                }
                Ok(Some((projection,input,when)))
            },
            ExprKind::Member(base,field)=>{
                let Some((mut projection,input,when))=unsafe { self.world.call(self.loop_write_projection(&base,known_index)).await }? else{return Ok(None);};
                let(ty,selected)=member_type(&projection.value.0,field,self.scope.source(),expr.span())?;
                projection.value=(ty,selected.iter().map(|i|projection.value.1[*i].clone()).collect());
                Ok(Some((projection,input,when)))
            },
            _=>Ok(None),
        }
    }
    async fn normalize_writes(&mut self,writes:Vec<Write>)->Result<Vec<Write>>{
        let mut normalized=Vec::new();
        for write in writes {
            let write=match write {
                Write::Position{place,call:None,contents:false,when,..}if address_write(&place)=>{
                    let scope=Scope::current(self);
                    let mut borrow=match unsafe { self.world.call(scope.input_snapshot(&place,scope.bindings.len(),Access::Write,None,None)).await }? {
                        Some((_,_,borrow))=>borrow,
                        None=>view::Borrow::source(view::Source::Unresolved),
                    };
                    if let Some(when)=when {borrow=borrow.restricted(&when.assume(&self.path,true));}
                    Write::memory(place,borrow)
                },
                Write::Generator(factory)=>{
                    if let Some(instance)=unsafe { self.world.call(Scope::current(self).factory_instance(&factory)).await }? {
                        let function=&instance.definition.function;
                        if !function.c_abi()&&function.kind()==FunctionKind::Value&&function.target()==Target::Cpu
                            &&function.params().len()==1&&function.params().at(0).access()==Access::Value {
                            unsafe { self.world.call(self.native_declaration(&instance)).await }?;
                            let definition=&instance.definition;
                            let result_name=unsafe { self.world.call(instance.bindings.type_name(self.world,&definition.function.result(),
                                (&definition.source,&definition.module,&definition.imports),definition.function.span())).await }?;
                            let result=unsafe { self.world.call(self.world.ty(&result_name,&definition.source,definition.function.span(),&definition.module,&definition.imports)).await }?;
                            if !result.owns_resources()&&self.functions.writes_only_references(&instance.name){continue;}
                        }
                    }
                    Write::Generator(factory)
                },
                // Domain effects stay possible writes; its execution interface is not an ordinary call contract.
                Write::External{call,instance:Some(instance),arguments,syntax}
                    if instance.definition.execution!=crate::execution::Requirement::Domain
                        || instance.definition.function.target()==Target::Cpu&&!unsafe { self.world.call(super::kernel_interface(self.world,&instance)).await }?=>{
                    unsafe { self.world.call(self.native_declaration(&instance)).await }?;
                    if self.functions.read_only(&instance.name) {
                        normalized.retain(|write|!matches!(write,Write::Position{call:Some(origin),..} if origin.same_node(&call)));
                    }
                    let local=normalized.iter().filter_map(|write|match write {
                        Write::Position{call:Some(origin),place,local_value:true,..}if origin.same_node(&call)=>Some(place.clone()),
                        _=>None,
                    }).collect::<Vec<_>>();
                    // Direct lexical storage stays a syntax position; other memory uses the closed contract.
                    let mut owned=Vec::new();
                    for(parameter,arg)in instance.parameters(self.world).zip(&syntax){
                        let parameter=unsafe { self.world.call(parameter).await }?;
                        // Texture descriptors are borrowed resources, not ordinary owning values.
                        if !parameter.ty.is_texture_resource() && unsafe { self.world.call(cooperative::parameter_type(self.world,&parameter,(&instance.definition.source,&instance.definition.module,&instance.definition.imports,instance.definition.function.kind()),self.program.target)).await }?.owns_resources(){owned.push(arg);}
                    }
                    normalized.retain(|write|!matches!(write,Write::Position{call:Some(origin),place,..}
                        if origin.same_node(&call)&&!owned.iter().chain(local.iter()).any(|arg|arg.same_node(place))));
                    match self.native_call_memory(&instance.name,&arguments,|parameter|syntax.get(parameter).is_some_and(|arg|local.iter().any(|place|place.same_node(&arg)))) {
                        Some(memory)=>{
                            normalized.extend(memory.into_iter().filter_map(|(borrow,_,_)|{
                                let borrow=borrow.restricted(&self.path);
                                borrow.active().then_some(Write::memory(call.clone(),borrow))
                            }));
                            continue;
                        },
                        None=>Write::memory(call,view::Borrow::source(view::Source::Unresolved)),
                    }
                },
                write=>write,
            };
            normalized.push(write);
        }
        Ok(normalized)
    }
    pub(super) async fn loop_writes(
        &mut self,
        condition: Option<&Expr>,
        body: &Block,
        borrowed_data: Option<ValueIdentity>,
        initial: bool,
    ) -> Result<LoopWrites> {
        let mut writes = Vec::new();
        unsafe { self.world.call(statement_writes(self, body, &mut async |place: Write| {writes.push(place);Ok(())})).await }?;
        if let Some(condition)=condition {
            unsafe { self.world.call(expression_writes(self, condition, &mut async |place: Write| {writes.push(place);Ok(())})).await }?;
        }
        let writes=unsafe { self.world.call(self.normalize_writes(writes)).await }?;
        let mut effect=unsafe { self.world.call(self.project_loop_writes(&writes)).await }?;
        let body_bounds=if !initial && let Some(condition)=condition {
            unsafe { self.world.call(self.prepare_loop_scalar_headers(condition,body,&writes,&effect)).await }?
        }else{Vec::new()};
        let writes=if !effect.external && self.env.values().any(|binding|matches!(binding,Binding::Owned(owner) if *owner.place.ty()==Ty::scalar(Scalar::Bool) || condition.is_some() && *owner.place.ty()==Ty::scalar(Scalar::U32))) {
            let mut selected=Vec::new();
            {
                let mut scope=Scope::current(self);scope.writes=Some(&effect);
                unsafe { self.world.call(scope.writes_statements(body,body,&mut async |write: Write|{selected.push(write);Ok(())})).await }?;
                if let Some(condition)=condition {unsafe { self.world.call(scope.writes_expression(condition,&mut async |write: Write|{selected.push(write);Ok(())},None,Access::Value)).await }?;}
            }
            let writes=unsafe { self.world.call(self.normalize_writes(selected)).await }?;
            effect=unsafe { self.world.call(self.project_loop_writes(&writes)).await }?;
            writes
        }else{writes};
        effect.body_bounds=body_bounds;
        if !initial {
            let cursor=if condition.is_none()&&!bounds::seeks(body){self.loop_contexts.last().and_then(|context|context.iterator.as_ref()).map(|(name,_)|name.clone())}else{None};
            unsafe { self.world.call(self.invalidate_writes(&writes,borrowed_data,&effect,condition,cursor.as_deref())).await }?;
        }
        Ok(effect)
    }
}

fn address_write(expr:&Expr)->bool {
    match expr.kind() {
        ExprKind::Specialize(name,_,_)=>name=="unsafe_position",
        ExprKind::Member(base,_)|ExprKind::Index(base,_)=>address_write(&base),
        _=>false,
    }
}
fn assigned_root(expr: &Expr) -> Option<String> {
    match expr.kind() {
        ExprKind::Name(name) => Some(name.to_owned()),
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(&base),
        _ => None,
    }
}
impl<'b, 'w> Scope<'b, 'w> {

    async fn input_snapshot(&self,expr:&Expr,limit:usize,access:Access,element:Option<(&DataProjection<'_>,&bounds::Clause)>,ordinal:Option<&str>)->Result<Option<(Ty,bounds::Proof,view::Borrow)>> {
        if let ExprKind::Name(name) = expr.kind()
            && let Some(value) = self.lexical.bindings().values.get(name).copied()
        {
            return Ok(Some((Ty::scalar(Scalar::U32), bounds::Proof::unsigned(value, value), view::Borrow::default())));
        }
        let builder=self.builder.unwrap();
        if let ExprKind::Name(name)=expr.kind() && ordinal==Some(name) {
            return Ok(Some((Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),view::Borrow::default())));
        }
        if let ExprKind::Name(name)=expr.kind()
            &&let Some(i)=self.bindings[..limit].iter().rposition(|(bound,_)|*bound==name)
            &&let Local::Initialized(None,value,_)=&self.bindings[i].1
            &&!builder.mutable_names.contains(name) {return unsafe { self.world.call(self.input_snapshot(&value,i,access,element,ordinal)).await };}
        match expr.kind() {
            ExprKind::Member(base,field)if field=="length"=>{
                if let Some((ty,proof,_))=unsafe { self.world.call(self.input_snapshot(&base,limit,Access::Value,element,ordinal)).await }? && ty.dataset.is_some() {
                    return Ok(Some((Ty::scalar(Scalar::U32),proof.project(&[1],ty.components.len()),view::Borrow::default())));
                }
            },
            ExprKind::Call(name,args)if name=="address_of"=>{
                let (1,Some(place))=(args.len(),args.first())else{return Ok(None);};
                let Some((_,_,borrow))=unsafe { self.world.call(self.input_snapshot(&place,limit,Access::Read,element,ordinal)).await }?else{return Ok(None);};
                if !borrow.active(){return Ok(None);}
                return Ok(Some((Ty::builtin("address").unwrap(),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,borrow);component_proof},view::Borrow::default())));
            },
            ExprKind::Specialize(name,explicit,args)if name=="unsafe_position"=>{
                let (1,Some(address))=(args.len(),args.first())else{return Ok(None);};
                let explicit=unsafe { self.world.call(self.lexical.bindings().arguments(self.world,&explicit,
                    (self.lexical.source(),self.lexical.module(),self.lexical.imports()),expr.span())).await }?;
                let [GenericArgument::Type(syntax)]=explicit.as_slice()else{return Ok(None);};
                let Some((ty,proof,_))=unsafe { self.world.call(self.input_snapshot(&address,limit,Access::Value,element,ordinal)).await }?else{return Ok(None);};
                if !ty.is_address(){return Ok(None);}
                let ty=unsafe { builder.world.call(builder.world.ty(&syntax,builder.scope.source(),expr.span(),builder.scope.module(),builder.scope.imports())).await }?;
                let layout=address::Layout::of(&ty);
                let origin=address::Origin{root:address::Root::Derived(proof.view_borrow(0)),base:layout.clone(),layout,words:ty.components.iter().map(|(_,word)|*word).collect(),positions:Arc::from([]),writable:true,initialized:vec![false;ty.components.len()].into()};
                return Ok(Some((ty.clone(),bounds::Proof::default().with_external_borrows(&ty),origin.normalize())));
            },
            ExprKind::Member(base,field)if address_write(&base)=>{
                let Some((ty,proof,borrow))=unsafe { self.world.call(self.input_snapshot(&base,limit,access,element,ordinal)).await }?else{return Ok(None);};
                let(selected,parts)=member_type(&ty,field,builder.scope.source(),expr.span())?;
                let origin=address::Origin{root:address::Root::Derived(borrow),base:address::Layout::of(&ty),layout:address::Layout::of(&selected),words:parts.iter().map(|part|ty.components[*part].1).collect(),positions:Arc::from([]),writable:true,initialized:vec![false;selected.components.len()].into()};
                return Ok(Some((selected,proof.project(&parts,ty.components.len()),origin.normalize())));
            },
            ExprKind::Index(base,index)if address_write(&base)=>{
                let Some((ty,_,borrow))=unsafe { self.world.call(self.input_snapshot(&base,limit,access,element,ordinal)).await }?else{return Ok(None);};
                let Some(selected)=ty.array.as_deref()else{return Ok(None);};
                let known=unsafe { self.world.call(self.input_snapshot(&index,limit,Access::Value,element,ordinal)).await }?.and_then(|(ty,proof,_)|(ty==Ty::scalar(Scalar::U32)).then(||proof.known_value(&ty)).flatten()).map(|bits|bits[0]);
                let mut locations=view::Borrow::default();
                for at in (0..ty.values[0]).filter(|at|known.is_none_or(|known|known==u64::from(*at))) {
                    let(offset,_)=ty.aggregate_part(at as usize);
                    let origin=address::Origin{root:address::Root::Derived(borrow.clone()),base:address::Layout::of(&ty),layout:address::Layout::of(selected),words:selected.components.iter().map(|(_,word)|offset+word).collect(),positions:Arc::from([]),writable:true,initialized:vec![false;selected.components.len()].into()};locations.union(&origin.normalize());
                }
                return Ok(Some((selected.clone(),bounds::Proof::default().with_external_borrows(selected),locations)));
            },
            ExprKind::StaticU32(value)=>return Ok(Some((Ty::scalar(Scalar::U32),bounds::Proof::unsigned(value,value),view::Borrow::default()))),
            ExprKind::Number(number)=>{
                let(ty,value)=super::number(self.lexical.source(),expr.span(),number)?;
                return Ok((ty==Ty::scalar(Scalar::U32)).then(||(ty,bounds::Proof::unsigned(value as u32,value as u32),view::Borrow::default())));
            },
            ExprKind::Binary(op,a,b)if matches!(op,"&&"|"||")=>{
                let condition=|value:Option<(Ty,bounds::Proof,view::Borrow)>|value.and_then(|(ty,proof,_)|(ty==Ty::scalar(Scalar::Bool)).then(||proof.component_condition(0).cloned()).flatten()).unwrap_or_default();
                let left=condition(unsafe { self.world.call(self.input_snapshot(&a,limit,Access::Value,element,ordinal)).await }?);
                let right=condition(unsafe { self.world.call(self.input_snapshot(&b,limit,Access::Value,element,ordinal)).await }?);
                let condition=left.logical(right,op=="||");
                return Ok((!condition.unconstrained()).then(||(Ty::scalar(Scalar::Bool),bounds::Proof::condition(condition),view::Borrow::default())));
            },
            ExprKind::Binary(op,a,b)if matches!(op,"=="|"!="|"<"|"<="|">"|">=")=>{
                let(Some((left,a,_)),Some((right,b,_)))=(unsafe { self.world.call(self.input_snapshot(&a,limit,Access::Value,element,ordinal)).await }?,unsafe { self.world.call(self.input_snapshot(&b,limit,Access::Value,element,ordinal)).await }?)else{return Ok(None);};
                if left!=Ty::scalar(Scalar::U32)||right!=left{return Ok(None);}
                let operand=|proof:&bounds::Proof|proof.known_value(&left).map(|bits|bounds::Operand::Constant(Scalar::U32,bits[0])).or_else(||proof.known_identity(0).map(bounds::Operand::Value));
                let(left,right)=(operand(&a),operand(&b));
                let condition=match(op,left,right){
                    ("<",left,right)=>bounds::Condition::index_below((left,&a),(right,&b),true),
                    ("<=",left,right)=>bounds::Condition::index_below((left,&a),(right,&b),false),
                    (">",left,right)=>bounds::Condition::index_below((right,&b),(left,&a),true),
                    (">=",left,right)=>bounds::Condition::index_below((right,&b),(left,&a),false),
                    ("!=",Some(left),Some(right))=>bounds::Condition::index_equal(left,right).negate(),
                    ("==",Some(left),Some(right))=>bounds::Condition::index_equal(left,right),
                    _=>bounds::Condition::default(),
                };
                return Ok(Some((Ty::scalar(Scalar::Bool),bounds::Proof::condition(condition),view::Borrow::default())));
            },
            _=>{},
        }
        let Some(name)=assigned_root(&expr)else{
            if let ExprKind::Bool(value)=expr.kind(){return Ok(Some((Ty::scalar(Scalar::Bool),bounds::Proof::condition(bounds::Condition::constant(value)),view::Borrow::default())));}
            return Ok(None);
        };
        if let Some(index)=self.bindings[..limit].iter().rposition(|(bound,_)|*bound==name) {
            let Local::NamedElement(source)=&self.bindings[index].1 else{return Ok(None);};
            if self.bindings[..index].iter().any(|(bound,_)|bound==source){return Ok(None);}
            let Some((data,offset,element))=(match builder.env.get(source) {
                Some(Binding::ApplicationData(data))=>Some((data,0,&data.ty)),
                Some(Binding::DataReference(data))=>{let(offset,ty)=data.element_layout();Some((&data.data,offset,ty))},
                _=>None,
            })else{return Ok(None);};
            let Some((ty,parts))=unsafe { builder.world.call(builder.component_projection(expr,&name,element,None,None,&|_|Ok(None))).await }?else{return Ok(None);};
            let mut borrow=view::Borrow::default();
            for choice in 0..parts.first().map_or(0,Vec::len) {
                let mut origin=application::dataset_origin(data,offset,element,false);
                origin.layout=address::Layout::of(&ty);
                origin.words=parts.iter().map(|part|offset+element.components[part[choice]].1).collect();
                origin.initialized=vec![false;ty.components.len()].into();
                borrow.union(&origin.normalize());
            }
            return Ok(Some((ty.clone(),bounds::Proof::default().with_external_borrows(&ty),borrow)));
        }
        let Some(binding)=builder.env.get(&name)else{return Ok(None);};
        let known_index=|index:&Expr| {
            if let ExprKind::Name(name)=index.kind()
                &&(self.bindings[..limit].iter().any(|(bound,_)|*bound==name)||!matches!(builder.env.get(name),Some(Binding::Value(_)))) {return Ok(None);}
            if let Some(index)=builder.constant_index(index,&[])?{return Ok(Some(bounds::Operand::Constant(Scalar::U32,Bits::from(index))));}
            Ok(match index.kind(){ExprKind::Name(name)=>match builder.env.get(name){Some(Binding::Value(value))if value.ty==Ty::scalar(Scalar::U32)=>Some(bounds::Operand::of(value,0)),_=>None},_=>None})
        };
        if matches!(binding,Binding::ApplicationData(_)|Binding::DataReference(_)|Binding::OwnedValue(_))
            && let Some((projection,input,_))=unsafe { builder.world.call(builder.loop_write_projection(expr,&async |index,_|known_index(index).map(|index|(index,None)))).await }? {
            let proof=match(element,self.writes){
                (Some((target,path)),Some(writes))=>unsafe { builder.world.call(builder.application_element_snapshot(&projection,input,target,(writes,path))).await }?,
                _=>bounds::Proof::default().with_external_borrows(&projection.value.0),
            };
            let(ty,parts)=projection.value;
            let mut borrow=view::Borrow::default();
            for choice in 0..parts.first().map_or(0,Vec::len) {
                let origin=address::Origin {
                    root:address::Root::Resource{borrow:projection.resource.clone(),index:projection.index},base:address::Layout::of(&projection.base),layout:address::Layout::of(&ty),
                    words:parts.iter().map(|part|projection.base.components[part[choice]].1).collect(),positions:Arc::from([]),
                    writable:projection.access.writes(),initialized:vec![false;ty.components.len()].into(),
                };
                borrow.union(&origin.normalize());
            }
            return Ok(Some((ty,proof,borrow)));
        }
        let (ty,source,owner)=match binding {
            Binding::Value(value)=>(value.ty.clone(),bounds::Proof::capture(value),None),
            Binding::Owned(owner)=>(owner.place.ty().clone(),owner.proof.clone(),Some(owner.place.clone())),
            Binding::OwnedValue(owner)=>(owner.value.ty.clone(),bounds::Proof::capture(&owner.value),owner.place()),
            _=>return Ok(None),
        };
        let kept=match (binding,self.writes,owner.as_ref().map(Place::storage)) {
            (Binding::Value(_),_,_)=>(0..ty.components.len()).collect::<Vec<_>>(),
            (_,Some(writes),Some(Place::Local(_,slots,..)))if !writes.external=>slots.iter().enumerate()
                .filter_map(|(component,slot)|(!writes.locals.contains(slot)).then_some(component)).collect(),
            _=>Vec::new(),
        };
        let mut proof=bounds::Proof::default().with_external_borrows(&ty);
        proof.assign(&kept,ty.components.len(),source.project(&kept,ty.components.len()));
        let Some((selected,parts))=unsafe { builder.world.call(builder.component_projection(expr,&name,&ty,owner.as_ref(),None,&|index|Ok(match known_index(index)?{Some(bounds::Operand::Constant(Scalar::U32,value))=>Some(value as u32),_=>None}))).await }?else{return Ok(None);};
        let mut value=None::<bounds::Proof>;let mut borrow=view::Borrow::default();
        for choice in 0..parts.first().map_or(0,Vec::len) {
            let indices=parts.iter().map(|part|part[choice]).collect::<Vec<_>>();
            let projection=proof.project(&indices,ty.components.len());
            if let Some(value)=&mut value{value.join_condition(&projection,selected.components.len(),None);}else{value=Some(projection);}
            if access!=Access::Value && let Some(place)=&owner{borrow.union(&unsafe { builder.world.call(builder.position_origin(&place.clone().project(selected.clone(),&indices),expr.span())).await }?.normalize());}
        }
        Ok(Some((selected,value.unwrap_or_default(),borrow)))
    }
    async fn call_inputs(&self,instance:&crate::generics::Instance,args:&Expressions)->Result<BTreeMap<ValueIdentity,bounds::Proof>> {
        let definition=&instance.definition;let builder=self.builder.unwrap();let mut inputs=BTreeMap::new();let mut offset=application::pointer_type().components.len();
        for(index,(parameter,arg))in instance.parameters(self.world).zip(args).enumerate() {
            let parameter=unsafe { self.world.call(parameter).await }?;
            if parameter.ty.is_texture_resource() {
                // The native call already passes one texture descriptor address.
                offset += 1;
                continue;
            }
            let ty=unsafe { self.world.call(cooperative::parameter_type(self.world,&parameter,(&definition.source,&definition.module,&definition.imports,definition.function.kind()),builder.program.target)).await }?;
            if parameter.ty.dataset {
                let descriptor=Builder::borrowed_dataset_parameter_type(parameter.access,definition.execution);
                if let Some(name)=assigned_root(&arg) &&!self.bindings.iter().any(|(bound,_)|*bound==name) {
                    if let Some((data,field,ty))=builder.written_application_data(&arg) {
                        let origin=application::dataset_origin(data,field,ty,false);
                        inputs.insert(ValueIdentity::Register(offset),{let mut component_proof=data.root.proof.clone();component_proof.set_view_borrow(0,origin.normalize());component_proof});
                        inputs.insert(ValueIdentity::Register(offset+1),bounds::Proof::capture(&data.length));
                    }
                }
                offset+=descriptor.components.len();continue;
            }
            let(proof,storage)=match unsafe { self.world.call(self.input_snapshot(&arg,self.bindings.len(),parameter.access,None,None)).await }?{Some((_,proof,storage))=>(proof,storage),None=>(bounds::Proof::default().with_external_borrows(&ty),view::Borrow::source(view::Source::Unresolved))};
            if matches!(parameter.access,Access::Read|Access::Write|Access::InOut|Access::Atomic) {
                let storage=if storage.empty(){view::Borrow::source(view::Source::Unresolved)}else{storage};
                inputs.insert(ValueIdentity::ParameterStorage(index),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,storage);component_proof});
                for component in 0..ty.components.len(){inputs.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                offset += function_inputs::reference_layout(&ty, false).count * 2;
            }else{
                for component in 0..ty.components.len(){inputs.insert(ValueIdentity::Register(offset+component),proof.project(&[component],ty.components.len()));}
                offset+=ty.components.len();
            }
        }
        Ok(inputs)
    }

    async fn call_instance(&self,call:&Expr)->Result<Option<crate::generics::Instance>> {
        let(name,explicit,args)=match call.kind() {
            ExprKind::Call(name,args)=>{
                if self.lexical.bindings().type_call(name).is_some()||unsafe { self.world.call(self.named(name,call.span())).await }?.is_some(){return Ok(None);}
                (name,None,args)
            },
            ExprKind::Specialize(name,types,args)=>(name,Some(types),args),
            _=>return Ok(None),
        };
        let Some(key)=self.world.resolve(name,self.lexical.module(),self.lexical.imports(),true).unique()else{return Ok(None);};
        let function=&self.world.functions[&key].function;
        if function.c_abi()||function.kind()!=FunctionKind::Value||function.target()==Target::Gpu{return Ok(None);}
        let actuals = unsafe { self.world.call(self.call_actuals(&args, self.bindings.len())).await }?;
        let explicit=match explicit.as_ref() {
            Some(arguments)=>Some(unsafe { self.world.call(self.lexical.bindings().arguments(self.world,arguments,
                (self.lexical.source(),self.lexical.module(),self.lexical.imports()),call.span())).await }?),
            None=>None,
        };
        unsafe { self.world.call(generics::specialize(self.world,&key,&actuals,explicit.as_deref(),
            (self.lexical.source(),self.lexical.module(),self.lexical.imports()),call.span())).await }.map(Some)
    }
    async fn factory_instance(&self,factory:&Expr)->Result<Option<crate::generics::Instance>> {
        let(name,explicit)=match factory.kind() {
            ExprKind::Name(name)=>(name,None),
            ExprKind::FunctionRef(name,types)=>(name,Some(types)),
            _=>return Ok(None),
        };
        let Some(key)=self.world.resolve(name,self.lexical.module(),self.lexical.imports(),true).unique()else{return Ok(None);};
        let actual=generics::Actual{ty:Some(std::borrow::Cow::Owned(Ty::scalar(Scalar::U32))),count:None};
        let explicit=match explicit.as_ref() {
            Some(arguments)=>Some(unsafe { self.world.call(self.lexical.bindings().arguments(self.world,arguments,
                (self.lexical.source(),self.lexical.module(),self.lexical.imports()),factory.span())).await }?),
            None=>None,
        };
        unsafe { self.world.call(generics::specialize(self.world,&key,&[actual],explicit.as_deref(),
            (self.lexical.source(),self.lexical.module(),self.lexical.imports()),factory.span())).await }.map(Some)
    }

}
async fn expression_writes(builder: &Builder<'_>, expr: &Expr, writes: &mut dyn WriteSink) -> Result<()> {
    unsafe { builder.world.call(Scope::current(builder).writes_expression(expr,writes,None,Access::Value)).await }.map(|_| ())
}
async fn statement_writes(builder: &Builder<'_>, code: &Block, writes: &mut dyn WriteSink) -> Result<()> {
    unsafe { builder.world.call(Scope::current(builder).writes_statements(code, code, writes)).await }
}
pub(super) async fn value_writes(builder: &Builder<'_>, body: &Block) -> Result<Vec<Expr>> {
    let mut values = Vec::new();
    unsafe { builder.world.call(statement_writes(builder, body, &mut async |write: Write| {
        if let Write::Position{contents:false,iterator:None,place,..}=write {values.push(place);}
        Ok(())
    })).await }?;
    Ok(values)
}
pub(super) async fn expression_assignments(builder: &Builder<'_>, expr: &Expr, names: &mut BTreeSet<String>) -> Result<()> {
    unsafe { builder.world.call(expression_writes(builder, expr, &mut async |write: Write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(&place) {names.insert(root);}
        Ok(())
    })).await }
}
pub(super) async fn assigned_names(world: &World, lexical: &LexicalScope, code: &Block, names: &mut BTreeSet<String>) -> Result<()> {
    unsafe { world.call(Scope::declared(world, lexical).writes_statements(code, code, &mut async |write: Write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(&place) {names.insert(root);}
        Ok(())
    })).await }
}
pub(super) async fn assigned_parallel(world:&World,lexical:&LexicalScope,name:&str,domain:&ParallelDomain,body:&Block,names:&mut BTreeSet<String>)->Result<()> {
    let mut scope=Scope::declared(world,lexical);
    let mut assigned=async |write: Write|{
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(&place){names.insert(root);}
        Ok(())
    };
    if let Some(count)=domain.count(){unsafe { world.call(scope.writes_expression(&count,&mut assigned,None,Access::Value)).await }?;}
    if let Some(selection)=domain.selection(){unsafe { world.call(scope.writes_expression(&selection,&mut assigned,None,Access::Value)).await }?;}
    scope.bindings.push((name.into(),Local::NamedElement(domain.dataset().into())));
    unsafe { world.call(scope.writes_statements(body,body,&mut assigned)).await }
}

fn contains_expression(root: &Expr, selected: &Expr) -> bool {
    unsafe { numeric_contains_expression(root.signature_node(), selected.signature_node()) }
}

fn contains_statement(statement: &Stmt, selected: &Expr) -> bool {
    unsafe { numeric_contains_statement(statement.signature_node(), selected.signature_node()) }
}
pub(super) fn modified_owners(
    env: &BTreeMap<String, Binding>,
    names: &BTreeSet<String>,
) -> BTreeSet<String> {
    let data: BTreeSet<_> = names
        .iter()
        .filter_map(|name| {
            if let Some(Binding::Data(view)) = env.get(name) {
                Some(view.binding)
            } else {
                None
            }
        })
        .collect();
    let application: BTreeSet<_> = names
        .iter()
        .flat_map(|name| match env.get(name) {
            Some(Binding::ApplicationData(data)) => vec![data],
            Some(Binding::OwnedValue(owner)) => owner.resources(),
            _ => Vec::new(),
        })
        .map(|data| data.root.identity())
        .collect();
    env.iter()
        .filter_map(|(name, binding)| {
            let owner = binding.stored_place()?;
            let through_data =
                matches!(owner,Place::Element{binding,..} if data.contains(&binding));
            let through_application = matches!(owner.storage(), Place::ApplicationElement { identity, .. } if application.contains(identity));
            (names.contains(name) || through_data || through_application).then(|| name.clone())
        })
        .collect()
}


fn has_numeric_loop_control(body: &Block, seek: bool) -> bool {
    unsafe { numeric_loop_control(body.source_view().storage(), body.source_slice(), u32::from(seek)) }
}

enum LoopPlace { Stored(Place, usize), Local, Length, Element(DataProjection<'static>, usize) }

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum NumericKind { Missing, Unknown, Ordered, Assign, Reflect, Shift }

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct NumericWide { low: u64, high: u64 }

impl NumericWide {
    fn signed(self) -> i128 { ((self.high as i128) << 64) | self.low as i128 }
    fn from_signed(value: i128) -> Self { Self { low: value as u64, high: (value >> 64) as u64 } }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct NumericTransfer {
    kind: NumericKind, flags: u32, low: NumericWide, high: NumericWide, least: NumericWide, most: NumericWide,
}

impl NumericTransfer {
    fn kind(kind: NumericKind, flags: u32) -> Self {
        Self { kind, flags, low: NumericWide::default(), high: NumericWide::default(),
            least: NumericWide::default(), most: NumericWide::default() }
    }
}

#[allow(dead_code)]
#[repr(u32)]
enum NumericProofAction { None, Left, Right, Join, Retain, Shift, Reflect }

#[repr(C)]
struct NumericCombination { effect: NumericTransfer, proof: NumericProofAction }

#[derive(Default)]
#[repr(C)]
struct NumericRange { lower: u32, upper: u32 }

impl From<&bounds::Proof> for NumericRange {
    fn from(proof: &bounds::Proof) -> Self {
        Self { lower: proof.unsigned_minimum(0), upper: proof.unsigned_maximum(0) }
    }
}

#[repr(C)]
struct NumericIterations { present: u32, count: u64 }

#[repr(C)]
struct NumericRanges { present: u32, body: NumericRange, header: NumericRange }

#[repr(C)]
struct NumericRetention { flags: u32, proof: NumericProofAction }

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(u32)]
enum NumericTruth { Absent, Conditional, False, True }

#[derive(Default)]
#[repr(C)]
struct NumericPathCursor { index: u64, arm: u64, phase: u32 }

#[allow(dead_code)]
#[repr(u32)]
enum NumericPathAction {
    Next, Stop, Expression, Target, Assignment, Declare, Let, Condition,
    Yes, No, Arm, Reenter, Nested, NestedPoint, ObserveLoop, Finish,
}

#[repr(C)]
struct NumericPathStep {
    cursor: NumericPathCursor,
    action: NumericPathAction,
    statement: StmtRef,
    value: ExprRef,
    body: Slice,
    binding: Optional<Text>,
    variant: Text,
    annotation: Optional<TypeRef>,
    local: u32,
}

#[allow(dead_code)]
#[repr(u32)]
enum NumericRhsAction { Unknown, Preserve, Relative, Shift, Subtract, Reflect, Value, Resize, Length }

#[repr(C)]
struct NumericRhsStep { phase: u32, action: NumericRhsAction, value: ExprRef }

unsafe extern "C" {
    #[link_name = "bootstrap.source_writes::numeric_shift"]
    fn numeric_shift(amount: u32, subtract: u32) -> NumericTransfer;
    #[link_name = "bootstrap.source_writes::numeric_identity"]
    fn numeric_identity(effect: NumericTransfer) -> bool;
    #[link_name = "bootstrap.source_writes::numeric_zero_delta"]
    fn numeric_zero_delta(effect: NumericTransfer) -> bool;
    #[link_name = "bootstrap.source_writes::numeric_then"]
    fn numeric_then(left: NumericTransfer, right: NumericTransfer) -> NumericCombination;
    #[link_name = "bootstrap.source_writes::numeric_join"]
    fn numeric_join(left: NumericTransfer, right: NumericTransfer) -> NumericCombination;
    #[link_name = "bootstrap.source_writes::numeric_nested_order"]
    fn numeric_nested_order(effect: NumericTransfer, guard: NumericRange, assigned: NumericRange) -> NumericTransfer;
    #[link_name = "bootstrap.source_writes::numeric_iterations"]
    fn numeric_iterations(effect: NumericTransfer, input: NumericRange, guard: NumericRange) -> NumericIterations;
    #[link_name = "bootstrap.source_writes::numeric_ranges"]
    fn numeric_ranges(effect: NumericTransfer, input: NumericRange, iterations: NumericIterations) -> NumericRanges;
    #[link_name = "bootstrap.source_writes::numeric_retention"]
    fn numeric_retention(effect: NumericTransfer, input: NumericRange, guard: NumericRange, bounded: u32) -> NumericRetention;
    #[link_name = "bootstrap.source_writes::numeric_contains_expression"]
    fn numeric_contains_expression(root: SignatureNode, selected: SignatureNode) -> bool;
    #[link_name = "bootstrap.source_writes::numeric_contains_statement"]
    fn numeric_contains_statement(root: SignatureNode, selected: SignatureNode) -> bool;
    #[link_name = "bootstrap.source_writes::numeric_loop_control"]
    fn numeric_loop_control(storage: StorageView, body: Slice, seek: u32) -> bool;
    #[link_name = "bootstrap.source_writes::numeric_path_step"]
    fn numeric_path_step(storage: StorageView, block: Slice, cursor: NumericPathCursor, truth: NumericTruth,
        reaches: u32, point: Optional<SignatureNode>) -> NumericPathStep;
    #[link_name = "bootstrap.source_writes::numeric_rhs_step"]
    fn numeric_rhs_step(source: SignatureNode, phase: u32, accepted: u32, length: u32, same_name: u32) -> NumericRhsStep;
}


#[derive(Clone)]
enum NumericEffect {
    Unknown,
    Ordered { lower: bool, upper: bool },
    Assign(bounds::Proof),
    Reflect(u32),
    Shift { low: i128, high: i128, least: i128, most: i128 },
}

impl NumericEffect {
    fn transfer(&self) -> NumericTransfer {
        match *self {
            Self::Unknown => NumericTransfer::kind(NumericKind::Unknown, 0),
            Self::Ordered { lower, upper } =>
                NumericTransfer::kind(NumericKind::Ordered, u32::from(lower) | (u32::from(upper) << 1)),
            Self::Assign(_) => NumericTransfer::kind(NumericKind::Assign, 0),
            Self::Reflect(amount) => NumericTransfer::kind(NumericKind::Reflect, amount),
            Self::Shift { low, high, least, most } => NumericTransfer {
                kind: NumericKind::Shift, flags: 0,
                low: NumericWide::from_signed(low), high: NumericWide::from_signed(high),
                least: NumericWide::from_signed(least), most: NumericWide::from_signed(most),
            },
        }
    }

    fn from_transfer(transfer: NumericTransfer, proof: Option<bounds::Proof>) -> Option<Self> {
        Some(match transfer.kind {
            NumericKind::Missing => return None,
            NumericKind::Unknown => Self::Unknown,
            NumericKind::Ordered => Self::Ordered { lower: transfer.flags & 1 != 0, upper: transfer.flags & 2 != 0 },
            NumericKind::Assign => Self::Assign(proof.unwrap()),
            NumericKind::Reflect => Self::Reflect(transfer.flags),
            NumericKind::Shift => Self::Shift { low: transfer.low.signed(), high: transfer.high.signed(),
                least: transfer.least.signed(), most: transfer.most.signed() },
        })
    }

    fn into_proof(self) -> Option<bounds::Proof> {
        match self { Self::Assign(proof) => Some(proof), _ => None }
    }

    fn shift(amount: u32, subtract: bool) -> Self {
        Self::from_transfer(unsafe { numeric_shift(amount, u32::from(subtract)) }, None).unwrap()
    }

    fn preserve() -> Self { Self::shift(0, false) }
    fn is_identity(&self) -> bool { unsafe { numeric_identity(self.transfer()) } }
    fn zero_delta(&self) -> bool { unsafe { numeric_zero_delta(self.transfer()) } }

    fn combine(left: Option<Self>, right: Option<Self>, combined: NumericCombination) -> Option<Self> {
        let proof = match combined.proof {
            NumericProofAction::None => None,
            NumericProofAction::Left => left.and_then(Self::into_proof),
            NumericProofAction::Right => right.and_then(Self::into_proof),
            NumericProofAction::Join => {
                let mut proof = left.unwrap().into_proof().unwrap();
                proof.join(&right.unwrap().into_proof().unwrap(), 1, None);
                Some(proof)
            }
            NumericProofAction::Retain => {
                let mut proof = bounds::Proof::default();
                let flags = right.unwrap().transfer().flags;
                proof.retain_loop_bounds(&left.unwrap().into_proof().unwrap(), 0, flags & 1 != 0, flags & 2 != 0);
                Some(proof)
            }
            NumericProofAction::Shift => {
                let shift = right.unwrap().transfer();
                Some(left.unwrap().into_proof().unwrap().unsigned_shifted(shift.low.signed(), shift.high.signed()))
            }
            NumericProofAction::Reflect => unreachable!(),
        };
        Self::from_transfer(combined.effect, proof)
    }

    fn then(self, other: Self) -> Self {
        let combined = unsafe { numeric_then(self.transfer(), other.transfer()) };
        Self::combine(Some(self), Some(other), combined).unwrap()
    }

    fn nested_order(effect: Option<&Self>, guard: &bounds::Proof) -> Self {
        let transfer = transfer_path(effect);
        let assigned = match effect { Some(Self::Assign(proof)) => proof.into(), _ => NumericRange::default() };
        Self::from_transfer(unsafe { numeric_nested_order(transfer, guard.into(), assigned) }, None).unwrap()
    }

    fn iteration_bound(&self, input: &bounds::Proof, guard: &bounds::Proof) -> Option<u64> {
        let result = unsafe { numeric_iterations(self.transfer(), input.into(), guard.into()) };
        (result.present != 0).then_some(result.count)
    }

    fn bounded_ranges(&self, input: &bounds::Proof, iterations: u64) -> Option<((u32, u32), (u32, u32))> {
        let result = unsafe { numeric_ranges(self.transfer(), input.into(), NumericIterations { present: 1, count: iterations }) };
        (result.present != 0).then_some(((result.body.lower, result.body.upper), (result.header.lower, result.header.upper)))
    }

    fn retained(&self, proof: &mut bounds::Proof, guard: &bounds::Proof, bounded: bool) -> (bool, bool) {
        let transfer = self.transfer();
        let result = unsafe { numeric_retention(transfer, (&*proof).into(), guard.into(), u32::from(bounded)) };
        match result.proof {
            NumericProofAction::Join => proof.join(match self {
                Self::Assign(assigned) => assigned,
                _ => unreachable!(),
            }, 1, None),
            NumericProofAction::Reflect => proof.join(&proof.unsigned_reflected(transfer.flags), 1, None),
            _ => {}
        }
        (result.flags & 1 != 0, result.flags & 2 != 0)
    }
}

fn transfer_path(effect: Option<&NumericEffect>) -> NumericTransfer {
    effect.map_or_else(|| NumericTransfer::kind(NumericKind::Missing, 0), NumericEffect::transfer)
}

fn join_numeric_paths(left: Option<NumericEffect>, right: Option<NumericEffect>) -> Option<NumericEffect> {
    let joined = unsafe { numeric_join(transfer_path(left.as_ref()), transfer_path(right.as_ref())) };
    NumericEffect::combine(left, right, joined)
}
impl Builder<'_> {
    fn loop_index_operand(&self,index:&Expr,writes:&[Write])->Result<Option<bounds::Operand>> {
        Ok(match index.kind() {
            ExprKind::Name(name)=>match self.env.get(name){
                Some(Binding::Value(value))if value.ty==Ty::scalar(Scalar::U32)=>Some(bounds::Operand::of(value,0)),
                _=>self.constant_index(index,writes)?.map(|value|bounds::Operand::Constant(Scalar::U32,Bits::from(value))),
            },
            ExprKind::StaticU32(_)|ExprKind::Number(_)=>self.constant_index(index,writes)?.map(|value|bounds::Operand::Constant(Scalar::U32,Bits::from(value))),
            _=>None,
        })
    }
    async fn loop_element_proof(&mut self,target:&DataProjection<'_>,component:usize,environment:&BTreeMap<String,Binding>,path:&bounds::Clause)->Result<bounds::Proof> {
        let mut result:Option<bounds::Proof>=None;
        for binding in environment.values(){for original in binding.application_resources(){
            for(scope,complete,parents)in self.application_element_scopes(&[],&target.resource,original,(path,environment)){
                let mut data=original.clone();let mut available=true;
                for fields in scope {
                    let model=unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?;
                    let Some(next)=application::model_data(Some(&model),&fields)else{available=false;break;};data=next.clone();
                }
                let proof=if available&&complete&&data.ty==target.base {
                    let model=unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?;
                    let value=self.returned_storage(&model);let mut value=self.value_component(&value,component);
                    let selected=parents.scope_element_indices(1).logical(target.index.map(|index|bounds::Condition::element_index(0,index)).unwrap_or_else(||bounds::Condition::constant(true)),false);
                    let active=selected.assume(path,true);
                    value.proof=bounds::Proof::returning(&value,environment,&active);
                    if bounds::Condition::nonzero_value(&value).truth_in(&bounds::Clause::default())==Some(true){value.proof.mark_nonzero(0);}
                    value.proof=bounds::scoped_proof(&value.ty,value.proof,&active,environment);
                    bounds::Proof::capture(&value)
                }else{bounds::Proof::default()};
                if let Some(previous)=&mut result{previous.join(&proof,1,None);}else{result=Some(proof);}
            }
        }}
        Ok(result.unwrap_or_default())
    }
    async fn retain_loop_element(&mut self,target:&DataProjection<'_>,component:usize,when:&bounds::Condition,environment:&mut BTreeMap<String,Binding>,context:(&bounds::Clause,&BTreeMap<String,Binding>),update:&impl Fn(&Value)->bounds::Proof)->Result<()> {
        for name in environment.keys().cloned().collect::<Vec<_>>(){
            let mut binding=environment[&name].clone();
            for at in 0..application::binding_element_count(&binding){
                let Some(root)=application::binding_element_data_mut(&mut binding,at)else{continue;};
                for(scope,complete,parents)in self.application_element_scopes(&[],&target.resource,root,context){
                    if !complete{continue;}
                    let selected=parents.scope_element_indices(1).logical(target.index.map(|index|bounds::Condition::element_index(0,index)).unwrap_or_else(||bounds::Condition::constant(true)),false).logical(when.clone(),false);
                    let mut data=Some(&mut *root);
                    for fields in scope{data=match data{Some(data)=>{unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;application::model_data_mut(data.elements.known_mut(),&fields)},None=>None};}
                    let Some(data)=data else{continue;};
                    if data.ty!=target.base{continue;}
                    unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;
                    if let Some(value)=data.elements.known_mut().and_then(returns::Returned::view_value_mut){
                        let previous=self.value_component(value,component);
                        let mut next=update(&previous);next.join_condition(&bounds::Proof::capture(&previous),1,Some(&selected));value.proof.assign(&[component],value.regs.len(),next);
                    }
                }
            }
            environment.insert(name,binding);
        }
        Ok(())
    }
    async fn numeric_write(&self, write: &Write, name: &str, target: &LoopPlace, writes: &[Write]) -> Result<bool> {
        if matches!(write,Write::Move{..}){return Ok(false);}
        if matches!(target,LoopPlace::Local) {
            return Ok(match write {
                Write::Position{place,..}=>assigned_root(&place).is_none_or(|root|root==name)||address_write(place),
                _=>true,
            });
        }
        if let LoopPlace::Element(target,component)=target {
            fn overlaps(a:&view::Borrow,b:&view::Borrow,path:&bounds::Clause)->bool {
                a.unresolved()||b.unresolved()
                    ||a.sources.entries().any(|(source,when)|b.overlaps_source(source,&bounds::Clause::both(path,&when)))
                    ||a.inputs.iter().any(|input|{
                        let when = bounds::Clause::both(&a.inputs.when(input), &b.inputs.when(input));
                        when == bounds::Clause::default() || when.overlaps(path)
                    })
                    ||a.calls.entries().any(|(call,when)|bounds::Clause::both(path,&when).overlaps(&b.calls.when(call)))
            }
            fn ancestors(resource:&view::Borrow,write:&DataProjection<'_>,path:&bounds::Clause)->bool {
                resource.sources.entries().any(|(source,when)|{
                    let active=bounds::Clause::both(path,&when);
                    match source {
                        view::Source::Element{parent,fields,index}=>{
                            if overlaps(parent,&write.resource,&active)&&index.zip(write.index).is_none_or(|(a,b)|a.overlaps(bounds::Selector::Direct(b),&active)){
                                let(mut offset,mut ty)=(0,&write.base);
                                for field in fields {let(word,next)=ty.aggregate_part(*field);offset+=word;ty=next;}
                                if write.value.1.iter().flatten().any(|part|{let(scalar,word)=write.base.components[*part];word<offset+ty.size&&offset<word+scalar.size()}){return true;}
                            }
                            ancestors(parent,write,&active)
                        },
                        view::Source::Projection{parent,..}=>overlaps(parent,&write.resource,&active)||ancestors(parent,write,&active),
                        _=>false,
                    }
                })
            }
            if let Write::Memory{borrow,..}=write {
                if !borrow.inputs.is_empty()||!borrow.calls.is_empty()||borrow.unresolved()||target.resource.unresolved(){return Ok(true);}
                let(scalar,word)=target.base.components[*component];
                for(source,when)in borrow.sources.entries(){
                    let active=bounds::Clause::both(&self.path,&when);if !active.possible(){continue;}
                    let view::Source::Position(origin)=source else{if matches!(source,view::Source::Inactive){continue;}return Ok(true);};
                    if !origin.writable{continue;}
                    match &origin.root {
                        address::Root::Resource{borrow,index}|address::Root::Dataset{borrow,index}
                            if origin.base==address::Layout::of(&target.base)
                                &&borrow.restricted(&active)==target.resource.restricted(&active)=>{
                            if target.index.zip(*index).is_some_and(|(a,b)|!bounds::Condition::index_equal(a,b).assume(&active,true).possible()){continue;}
                            if origin.layout.parts.iter().zip(origin.words.iter()).any(|((written,_),at)|word<*at+written.size()&&*at<word+scalar.size()){return Ok(true);}
                        },
                        address::Root::Local{..}=>{
                            let Some(positions)=origin.memory_positions(&active)else{return Ok(true);};
                            let mut matched=false;
                            for binding in self.env.values() {
                                let conflict = view::visit_data_paths(binding, |path, data, _| {
                                    if self.application_element_scopes(&[], &target.resource, data, (&active, &self.env)).is_empty() {
                                        return ControlFlow::Continue(());
                                    }
                                    if data.ownership != application::Ownership::Owned {
                                        return ControlFlow::Break(());
                                    }
                                    let descriptor = match binding {
                                        Binding::ApplicationData(data) => data.place.clone(),
                                        _ => binding.stored_place(),
                                    };
                                    if let Some(mut descriptor) = descriptor {
                                        for &field in path {
                                            if descriptor.ty().has_variants() {
                                                return ControlFlow::Break(());
                                            }
                                            let (offset, ty) = descriptor.ty().aggregate_part(field);
                                            let components = ty.components.iter().map(|(_, at)| {
                                                descriptor.ty().components.iter().position(|(_, word)| *word == offset + at).unwrap()
                                            }).collect::<Vec<_>>();
                                            let projected = ty.clone();
                                            descriptor = descriptor.project(projected, &components);
                                        }
                                        if !self.address_write_components(&positions, &descriptor).is_empty() {
                                            return ControlFlow::Break(());
                                        }
                                    }
                                    // Register descriptors have no local slots for this address to write.
                                    matched = true;
                                    ControlFlow::Continue(())
                                });
                                if conflict.is_break() { return Ok(true); }
                            }
                            if !matched{return Ok(true);}
                        },
                        _=>return Ok(true),
                    }
                }
                return Ok(false);
            }
            let Write::Position{place,contents,iterator,local_value,..}=write else{return Ok(true);};
            if *local_value{return Ok(false);}
            if address_write(place)||iterator.is_some(){return Ok(true);}
            if *contents{return Ok(self.written_application_data(place).is_none_or(|(data,..)|overlaps(&target.resource.owners(),&view::resource_borrow(data),&self.path)));}
            if let Some((written,_,_))=unsafe { self.world.call(self.loop_write_projection(place,&async |index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))).await }? {
                if ancestors(&target.resource,&written,&self.path){return Ok(true);}
                if !overlaps(&target.resource,&written.resource,&self.path){return Ok(false);}
                if target.index.zip(written.index).is_some_and(|(a,b)|!bounds::Condition::index_equal(a,b).assume(&self.path,true).possible()){return Ok(false);}
                if target.base!=written.base{return Ok(true);}
                let(scalar,word)=target.base.components[*component];
                return Ok(written.value.1.iter().flatten().any(|part|{let(other,at)=written.base.components[*part];word<at+other.size()&&at<word+scalar.size()}));
            }
            let Some(root)=assigned_root(&place)else{return Ok(true);};let Some(binding)=self.env.get(&root)else{return Ok(true);};let Some(owner)=binding.stored_place()else{return Ok(true);};
            let Some((_,parts))=unsafe { self.world.call(self.written_projection(place,&root,&owner,None,writes)).await }?else{return Ok(true);};
            for data in binding.application_resources(){
                if !self.application_element_scopes(&[],&target.resource,data,(&self.path,&self.env)).is_empty()
                    && let Some(descriptor)=&data.place {
                    let changed=self.address_write_components(&self.positions(descriptor),&owner);
                    if parts.iter().flatten().any(|component|changed.contains(component)){return Ok(true);}
                }
            }
            let(scalar,word)=target.base.components[*component];let origin=address::Origin{root:address::Root::Resource{borrow:target.resource.clone(),index:target.index},layout:address::Layout{size:scalar.size(),align:scalar.size(),parts:vec![(scalar,0)].into()},base:address::Layout::of(&target.base),words:vec![word].into(),positions:Arc::from([]),writable:true,initialized:vec![false].into()};
            let borrow=view::Borrow::source(view::Source::Position(Arc::new(origin)));let changed=self.memory_effect_components(Self::address_write_effect(&borrow).as_ref(), &owner, false);
            return Ok(parts.iter().flatten().any(|component|changed.contains(component)));
        }
        let LoopPlace::Stored(owner,component)=target else {
            return Ok(matches!(write,Write::Position{contents:false,place,..}if matches!(place.kind(),ExprKind::Name(bound)if bound==name)));
        };
        let component=*component;
        if matches!(write,Write::Memory{..}){return Ok(self.memory_effect_components(write.memory_effect(), owner, false).contains(&component));}
        let Write::Position{place,contents,iterator,..}=write else{return Ok(true);};
        let projection = if *contents { self.application_write_projection(place, owner) }
            else { unsafe { self.world.call(self.written_projection(place, name, owner, iterator.as_ref().map(|(name,index)|(name.as_str(),index)), writes)).await }? };
        Ok(projection.is_some_and(|(_,parts)|parts.iter().any(|part|part.contains(&component))))
    }
    async fn numeric_expression_effect(&self, expr: &Expr, name: &str, target: &LoopPlace, writes: &[Write]) -> Result<NumericEffect> {
        for write in writes.iter().filter(|write|match write {
            Write::External{call,..}|Write::Memory{call,..}|Write::Generator(call)=>contains_expression(expr,call),
            Write::Position{call,..}=>call.as_ref().is_some_and(|call|contains_expression(expr,call)),
            Write::Move{..}=>false,
        }) {
            if unsafe { self.world.call(self.numeric_write(write,name,target,writes)).await }? { return Ok(NumericEffect::Unknown); }
        }
        Ok(NumericEffect::preserve())
    }

    async fn numeric_assigned_value(&self, rhs: &Expr, writes: &[Write], absolute: bool) -> Result<NumericEffect> {
        let value = if let Some(value) = self.constant_index(rhs, writes)? {
            bounds::Proof::unsigned(value, value)
        } else if absolute && let ExprKind::Name(source) = rhs.kind() {
            match self.env.get(source) {
                Some(Binding::Value(value)) if value.ty == Ty::scalar(Scalar::U32) => bounds::Proof::capture(value),
                Some(Binding::Owned(value)) if value.place.ty() == &Ty::scalar(Scalar::U32) => {
                    let place = LoopPlace::Stored(value.place.clone(), 0);
                    for write in writes {
                        if unsafe { self.world.call(self.numeric_write(write, source, &place, writes)).await }? {
                            return Ok(NumericEffect::Unknown);
                        }
                    }
                    value.proof.clone()
                }
                _ => return Ok(NumericEffect::Unknown),
            }
        } else {
            return Ok(NumericEffect::Unknown);
        };
        Ok(NumericEffect::Assign(value))
    }

    async fn numeric_assignment_effect(
        &self, statement: &Stmt, name: &str, target: &LoopPlace, writes: &[Write], absolute: bool,
        scope: &Scope<'_, '_>,
    ) -> Result<NumericEffect> {
        let StmtKind::Assign(lhs, _, rhs) = statement.kind() else { unreachable!() };
        if matches!(target, LoopPlace::Element(..))
            && let Some(local) = unsafe { self.world.call(scope.local_value_write(&lhs)).await }? {
            return Ok(if local { NumericEffect::preserve() } else { NumericEffect::Unknown });
        }
        let length = matches!(target, LoopPlace::Length);
        let projection = match target {
            LoopPlace::Length => {
                if !matches!(lhs.kind(), ExprKind::Name(bound) if bound == name)
                    || scope.bindings.iter().any(|(bound, _)| bound == name) {
                    return Ok(NumericEffect::preserve());
                }
                None
            }
            LoopPlace::Local => {
                if address_write(&lhs) { return Ok(NumericEffect::Unknown); }
                if assigned_root(&lhs).as_deref() != Some(name) { return Ok(NumericEffect::preserve()); }
                if !matches!(lhs.kind(), ExprKind::Name(bound) if bound == name)
                    || unsafe { self.world.call(scope.named(name, lhs.span())).await }? != Some(Ty::scalar(Scalar::U32)) {
                    return Ok(NumericEffect::Unknown);
                }
                Some((Ty::scalar(Scalar::U32), vec![vec![0]], 0))
            }
            LoopPlace::Stored(owner, component) => {
                let Some((ty, parts)) = unsafe { self.world.call(self.written_projection(&lhs, name, owner, None, writes)).await }?
                    else { return Ok(NumericEffect::preserve()); };
                Some((ty, parts, *component))
            }
            LoopPlace::Element(expected, component) => {
                let written = Write::Position { place: lhs.clone(), call: None, contents: false,
                    iterator: None, when: None, local_value: false };
                if !unsafe { self.world.call(self.numeric_write(&written, name, target, writes)).await }? {
                    return Ok(NumericEffect::preserve());
                }
                let Some((actual, _, _)) = unsafe { self.world.call(self.loop_write_projection(&lhs,
                    &async |index, _| self.loop_index_operand(index, writes).map(|index| (index, None)))).await }?
                    else { return Ok(NumericEffect::Unknown); };
                if expected.base != actual.base || expected.resource.restricted(&self.path) != actual.resource.restricted(&self.path)
                    || !expected.index.zip(actual.index).is_some_and(|(a, b)|
                        bounds::Condition::index_equal(a, b).truth_in(&self.path) == Some(true)) {
                    return Ok(NumericEffect::Unknown);
                }
                Some((actual.value.0, actual.value.1, *component))
            }
        };
        if let Some((_, parts, component)) = &projection {
            if !parts.iter().any(|part| part.contains(component)) { return Ok(NumericEffect::preserve()); }
            // A possible location is not necessarily the position selected by this write.
            if address_write(&lhs) || parts.iter().any(|part| part.contains(component) && part.len() != 1) {
                return Ok(NumericEffect::Unknown);
            }
        }
        let source = statement.signature_node();
        let same_name = u32::from(matches!(rhs.kind(), ExprKind::Name(source) if source == name));
        let mut phase = 0;
        let mut accepted = false;
        loop {
            let step = unsafe { numeric_rhs_step(source, phase, u32::from(accepted), u32::from(length), same_name) };
            phase = step.phase;
            let value: Expr = statement.child(step.value.offset);
            match step.action {
                NumericRhsAction::Unknown => return Ok(NumericEffect::Unknown),
                NumericRhsAction::Preserve => return Ok(NumericEffect::preserve()),
                NumericRhsAction::Relative => {
                    let (ty, parts, _) = projection.as_ref().unwrap();
                    accepted = match target {
                        LoopPlace::Local => matches!(value.kind(), ExprKind::Name(bound) if bound == name),
                        LoopPlace::Stored(owner, _) => unsafe {
                            self.world.call(self.written_projection(&value, name, owner, None, writes)).await
                        }?.is_some_and(|(source, selected)| source == *ty && selected == *parts),
                        LoopPlace::Element(expected, _) => unsafe {
                            self.world.call(self.loop_write_projection(&value,
                                &async |index, _| self.loop_index_operand(index, writes).map(|index| (index, None)))).await
                        }?.is_some_and(|(actual, _, _)| actual.base == expected.base
                            && actual.resource.restricted(&self.path) == expected.resource.restricted(&self.path)
                            && actual.index.zip(expected.index).is_some_and(|(a, b)|
                                bounds::Condition::index_equal(a, b).truth_in(&self.path) == Some(true))
                            && actual.value.0 == *ty && actual.value.1 == *parts),
                        LoopPlace::Length => unreachable!(),
                    };
                }
                NumericRhsAction::Shift | NumericRhsAction::Subtract | NumericRhsAction::Reflect => {
                    let Some(amount) = self.constant_index(&value, writes)? else { return Ok(NumericEffect::Unknown); };
                    return Ok(match step.action {
                        NumericRhsAction::Reflect => NumericEffect::Reflect(amount),
                        NumericRhsAction::Subtract => NumericEffect::shift(amount, true),
                        _ => NumericEffect::shift(amount, false),
                    });
                }
                NumericRhsAction::Resize => {
                    accepted = unsafe { self.world.call(scope.named("resize", value.span())).await }?.is_none()
                        && scope.world.resolve("resize", scope.lexical.module(), scope.lexical.imports(), true).is_missing();
                }
                NumericRhsAction::Length => {
                    if matches!(value.kind(), ExprKind::Name(source) if scope.bindings.iter().any(|(bound, _)| bound == source)) {
                        return Ok(NumericEffect::Unknown);
                    }
                    return unsafe { self.world.call(self.numeric_assigned_value(&value, writes, absolute)).await };
                }
                NumericRhsAction::Value => {
                    return unsafe { self.world.call(self.numeric_assigned_value(&value, writes, absolute)).await };
                }
            }
        }
    }

    async fn numeric_loop_paths(
        &self, body: &Block, mut next: Option<NumericEffect>, continues: &mut Option<NumericEffect>, name: &str,
        target: &LoopPlace, writes: &[Write], absolute: bool, scope: &mut Scope<'_, '_>,
        path: &bounds::Clause, point: Option<&Expr>, observed: &mut Option<NumericEffect>,
    ) -> Result<Option<NumericEffect>> {
        let outer = scope.bindings.len();
        let view = body.source_view();
        let storage = view.storage();
        let selected = Optional { present: u32::from(point.is_some()),
            value: point.map_or(SignatureNode { storage, offset: 0 }, Expr::signature_node) };
        let mut cursor = NumericPathCursor::default();
        let mut truth = NumericTruth::Absent;
        let mut condition = bounds::Condition::default();
        let mut branches = None;
        let mut nested = None;
        loop {
            let step = unsafe { numeric_path_step(storage, body.source_slice(), cursor, truth,
                u32::from(next.is_some()), selected) };
            cursor = step.cursor;
            let statement = || -> Stmt { view.node(step.statement.offset) };
            let child = || view.node(step.value.offset);
            let block = || view.list(step.body);
            let text = |value| view.text(value).to_owned();
            match step.action {
                NumericPathAction::Next => {
                    if let Some(joined) = branches.take() {
                        next = joined;
                        condition = bounds::Condition::default();
                    }
                    if let Some(nested) = nested.take() { next = next.map(|next| next.then(nested)); }
                    truth = NumericTruth::Absent;
                }
                NumericPathAction::Finish => break,
                NumericPathAction::Stop => {
                    next = None;
                    break;
                }
                NumericPathAction::ObserveLoop => {
                    *observed = join_numeric_paths(observed.take(), next.take());
                    break;
                }
                NumericPathAction::Expression | NumericPathAction::Target => {
                    let expr = child();
                    let effect = unsafe { self.world.call(self.numeric_expression_effect(&expr, name, target, writes)).await }?;
                    if matches!(step.action, NumericPathAction::Target) {
                        next = next.map(|next| next.then(effect));
                        if point.is_some_and(|point|expr.same_node(point)) {
                            *observed = join_numeric_paths(observed.take(), next.clone());
                        }
                    } else {
                        if step.local == 1 && effect.zero_delta()
                            && point.is_some_and(|point|contains_expression(&expr, point)) {
                            *observed = join_numeric_paths(observed.take(), next.clone());
                        }
                        next = next.map(|next| next.then(effect));
                        if step.local == 2 { branches = Some(None); }
                    }
                }
                NumericPathAction::Assignment => {
                    let effect = unsafe { self.world.call(self.numeric_assignment_effect(&statement(), name, target, writes, absolute, scope)).await }?;
                    next = next.map(|next| next.then(effect));
                }
                NumericPathAction::Declare => scope.bindings.push((text(step.binding.value),
                    Local::Typed(view.node(step.annotation.value.offset)))),
                NumericPathAction::Let => scope.bindings.push((text(step.binding.value), Local::Initialized(
                    (step.annotation.present != 0).then(|| view.node(step.annotation.value.offset)),
                    child(), body.slice(cursor.index as usize + 1..)))),
                NumericPathAction::Condition => {
                    condition = unsafe { self.world.call(scope.writes_expression(&child(),
                        &mut async |_: Write| Ok(()), match target {
                            LoopPlace::Element(target, _) => Some((target, path)), _ => None,
                        }, Access::Value)).await }?.unwrap_or_default();
                    truth = match condition.truth_in(path) {
                        Some(false) => NumericTruth::False,
                        Some(true) => NumericTruth::True,
                        None => NumericTruth::Conditional,
                    };
                    branches = Some(None);
                }
                NumericPathAction::Yes | NumericPathAction::No | NumericPathAction::Arm => {
                    if step.binding.present != 0 {
                        scope.bindings.push((text(step.binding.value), Local::Payload(child(), text(step.variant))));
                    }
                    let branch_path;
                    let path = match step.action {
                        NumericPathAction::Yes | NumericPathAction::No => {
                            branch_path = condition.assume(path, matches!(step.action, NumericPathAction::Yes));
                            &branch_path
                        }
                        _ => path,
                    };
                    let branch = unsafe { self.world.call(self.numeric_loop_paths(&block(), next.clone(),
                        continues, name, target, writes, absolute, scope, path, point, observed)).await }?;
                    if step.binding.present != 0 { scope.bindings.pop(); }
                    branches = Some(join_numeric_paths(branches.take().unwrap(), branch));
                }
                NumericPathAction::Reenter => {
                    *continues = join_numeric_paths(continues.take(), next.take());
                    break;
                }
                NumericPathAction::Nested => {
                    let statement = statement();
                    let mut changed = false;
                    for write in writes.iter().filter(|write| contains_statement(&statement, match write {
                        Write::External { call, .. } | Write::Memory { call, .. } | Write::Generator(call) => call,
                        Write::Position { call, place, .. } => call.as_ref().unwrap_or(place),
                        Write::Move { place, .. } => place,
                    })) {
                        if unsafe { self.world.call(self.numeric_write(write, name, target, writes)).await }? {
                            changed = true;
                            break;
                        }
                    }
                    if !changed {
                        nested = Some(NumericEffect::preserve());
                    } else if step.local != 0 {
                        let condition = child();
                        let body = block();
                        nested = Some(NumericEffect::Unknown);
                        if unsafe { self.world.call(scope.named(name, condition.span())).await }? == Some(Ty::scalar(Scalar::U32))
                            && !has_numeric_loop_control(&body, false) && !has_numeric_loop_control(&body, true)
                            && unsafe { self.world.call(self.numeric_expression_effect(&condition, name, target, writes)).await }?.is_identity() {
                            let guard = unsafe { self.world.call(scope.input_snapshot(&condition, scope.bindings.len(),
                                Access::Value, None, Some(name))).await }?
                                .and_then(|(_, proof, _)| proof.component_condition(0).cloned())
                                .map(|condition| bounds::scoped_proof(&Ty::scalar(Scalar::U32),
                                    bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),
                                    &condition.assume(&bounds::Clause::default(), true), &BTreeMap::new())).unwrap_or_default();
                            let mut nested_scope = Scope { bindings: scope.bindings.clone(), writes: scope.writes, ..Scope::current(self) };
                            let mut reentries = None;
                            let effect = unsafe { self.world.call(self.numeric_loop_paths(&body, Some(NumericEffect::preserve()),
                                &mut reentries, name, target, writes, absolute, &mut nested_scope, path, None,
                                &mut None)).await }?;
                            let joined = join_numeric_paths(effect, reentries);
                            nested = Some(NumericEffect::nested_order(joined.as_ref(), &guard));
                        }
                    } else {
                        nested = Some(NumericEffect::Unknown);
                    }
                }
                NumericPathAction::NestedPoint => {
                    if step.binding.present != 0 {
                        let local = match step.local {
                            1 => Local::Range,
                            2 => Local::Element(child()),
                            3 => Local::NamedElement(text(step.variant)),
                            _ => unreachable!(),
                        };
                        scope.bindings.push((text(step.binding.value), local));
                    }
                    unsafe { self.world.call(self.numeric_loop_paths(&block(), next.clone().map(|next| next.then(nested.clone().unwrap())),
                        &mut None, name, target, writes, absolute, scope, path, point, observed)).await }?;
                    if step.binding.present != 0 { scope.bindings.pop(); }
                }
            }
        }
        scope.bindings.truncate(outer);
        Ok(next)
    }
    async fn prepare_loop_scalar_headers(&mut self,condition:&Expr,body:&Block,writes:&[Write],effect:&LoopWrites)->Result<Vec<(String,(u32,u32))>> {
        let candidates=self.env.iter().filter_map(|(name,binding)|match binding {
            Binding::Owned(owner)if owner.place.ty()==&Ty::scalar(Scalar::U32)
                &&matches!(owner.place.storage(),Place::Local(..)|Place::Reference{..})=>Some((name.clone(),owner.place.clone(),owner.proof.clone())),
            _=>None,
        }).collect::<Vec<_>>();
        let mut retained=Vec::new();
        for(name,owner,input)in candidates {
            let target=LoopPlace::Stored(owner,0);let mut changed=false;
            for write in writes{if unsafe { self.world.call(self.numeric_write(write,&name,&target,writes)).await }?{changed=true;break;}}
            if !changed{continue;}
            let mut scope=Scope::current(self);scope.writes=Some(effect);
            let pure=unsafe { self.world.call(self.numeric_expression_effect(condition,&name,&target,writes)).await }?.is_identity();
            let guard=if pure {unsafe { self.world.call(scope.input_snapshot(condition,0,Access::Value,None,Some(&name))).await }?
                .and_then(|(_,proof,_)|proof.component_condition(0).cloned())
                .map(|condition|bounds::scoped_proof(&Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),&condition.assume(&bounds::Clause::default(),true),&BTreeMap::new())).unwrap_or_default()}else{bounds::Proof::default()};
            let mut continues=None;
            let next=if pure{unsafe { self.world.call(self.numeric_loop_paths(body,Some(NumericEffect::preserve()),&mut continues,&name,&target,writes,!has_numeric_loop_control(body,true),&mut scope,&bounds::Clause::default(),None,&mut None)).await }?}else{Some(NumericEffect::Unknown)};
            retained.push((name,input,guard,join_numeric_paths(next,continues)));
        }
        let iterations=retained.iter().filter_map(|(_,input,guard,effect)|effect.as_ref().and_then(|effect|effect.iteration_bound(input,guard))).min();
        let mut body_bounds=Vec::new();
        for(name,mut input,guard,transfer)in retained {
            let bounded=iterations.and_then(|iterations|transfer.as_ref()?.bounded_ranges(&input,iterations));
            let identity=transfer.as_ref().is_none_or(NumericEffect::is_identity);
            let(lower,upper)=transfer.as_ref().map_or((true,true),|transfer|transfer.retained(&mut input,&guard,bounded.is_some()));
            if let Some(Binding::Owned(owner))=self.env.get_mut(&name) {
            owner.uniform=false;
            if !identity {
                owner.proof.forget_value(&[0],1);
                let context=self.loop_contexts.last_mut().unwrap();
                if context.view_scope.is_some()&&context.entry.env.contains_key(&name)
                    &&let Place::Local(_,slots,..)=owner.place.storage()
                    &&!effect.locals.contains(&slots[0]) {
                    let id=context.value_input(&name,None,0,Scalar::U32);
                    owner.proof=std::mem::take(&mut owner.proof).with_component_identity(0,id);
                }
                owner.proof.retain_loop_bounds(&input,0,lower,upper);
            }
            if let Some((body,header))=bounded {
                owner.proof.retain_loop_bounds(&bounds::Proof::unsigned(header.0,header.1),0,true,true);
                body_bounds.push((name,body));
            }
            }
        }
        Ok(body_bounds)
    }
    pub(super) async fn retain_loop_bounds(&mut self, condition:Option<&Expr>, body:&Block, initial:&BTreeMap<String,Binding>, header:&mut BTreeMap<String,Binding>)->Result<bool> {
        let mut writes=Vec::new();unsafe { self.world.call(statement_writes(self,body,&mut async |write: Write|{writes.push(write);Ok(())})).await }?;if let Some(condition)=condition {unsafe { self.world.call(expression_writes(self,condition,&mut async |write: Write|{writes.push(write);Ok(())})).await }?;}
        let places=writes.iter().filter_map(|write|match write{Write::Position{place,contents:false,iterator:None,..}=>Some(place.clone()),_=>None}).collect::<Vec<_>>();
        let writes=unsafe { self.world.call(self.normalize_writes(writes)).await }?;
        let mut elements=Vec::<DataProjection<'static>>::new();
        for place in places{
            if let Some((projection,_,_))=unsafe { self.world.call(self.loop_write_projection(&place,&async |index,_|self.loop_index_operand(index,&writes).map(|index|(index,None)))).await }?{
                elements.push(DataProjection{data:None,resource:projection.resource,base:projection.base,access:projection.access,index:projection.index,value:projection.value});
            }
        }
        let context=self.loop_contexts.last().unwrap();let entry_path=context.entry.path.clone();
        let empty=context.control_source.test.as_ref().unwrap().at_call(&context.control_source.bindings(&context.entry)).negate();
        let mut candidates=Vec::new();
        for(name,binding)in initial {
            let length;let(owner,proof)=match binding {
                Binding::Owned(owner)=>{
                    if condition.is_some()&&owner.place.ty()==&Ty::scalar(Scalar::U32)
                        &&matches!(owner.place.storage(),Place::Local(..)|Place::Reference{..}) {continue;}
                    (Some(owner.place.clone()),&owner.proof)
                },
                Binding::OwnedValue(owner)=>{let Some(place)=owner.place()else{continue};(Some(place),&owner.value.proof)},
                Binding::ApplicationData(data)=>{length=bounds::Proof::capture(&data.length);(None,&length)},_=>continue,
            };
            let components=match &owner{Some(owner)if matches!(owner.storage(),Place::Local(..)|Place::Reference{..})=>owner.ty().components.as_slice(),Some(_)=>continue,None=>&[(Scalar::U32,0)]};
            let guard=match self.env.get(name){Some(Binding::Owned(value))=>&value.proof,Some(Binding::OwnedValue(value))=>&value.value.proof,Some(Binding::ApplicationData(data))=>&data.length.proof,_=>continue};
            for(component,(scalar,_))in components.iter().enumerate(){if *scalar==Scalar::U32{
                let target=match &owner{Some(owner)=>LoopPlace::Stored(owner.clone(),component),None=>LoopPlace::Length};
                candidates.push((name.clone(),target,proof.project(&[component],components.len()),Some(guard.project(&[component],components.len()))));
            }}
        }
        for projection in elements {
            for component in projection.value.1.iter().flatten().copied().collect::<BTreeSet<_>>(){
                if candidates.iter().any(|(_,target,_,_)|matches!(target,LoopPlace::Element(other,at)if *at==component&&other.base==projection.base&&other.resource==projection.resource&&other.index==projection.index)){continue;}
                let input=unsafe { self.world.call(self.loop_element_proof(&projection,component,initial,&entry_path)).await }?;
                let guard=if projection.base.components[component].0==Scalar::U32{Some(unsafe { self.world.call(self.loop_element_proof(&projection,component,header,&self.path.clone())).await }?)}else{None};
                let selected=DataProjection{data:None,resource:projection.resource.clone(),base:projection.base.clone(),access:projection.access,index:projection.index,value:projection.value.clone()};
                candidates.push((String::new(),LoopPlace::Element(selected,component),input,guard));
            }
        }
        let mut retained=Vec::new();
        for(name,target,mut input,guard)in candidates {
            let mut written=false;for write in &writes{if unsafe { self.world.call(self.numeric_write(write,&name,&target,&writes)).await }?{written=true;break;}}
            if !written{continue;}
            if let Some(condition)=condition&&matches!(unsafe { self.world.call(self.numeric_expression_effect(condition,&name,&target,&writes)).await }?,NumericEffect::Unknown){continue;}
            if let LoopPlace::Element(element,component)=&target {
                unsafe { self.world.call(self.retain_loop_element(element,*component,&empty,header,(&entry_path,initial),&|_|input.clone())).await }?;
            }
            let Some(guard)=guard else{continue;};
            if let Some(identity)=input.known_identity(0){let mut domains=input.domains(0).into_owned();domains.insert(IndexDomain::Upper(identity,0));input=input.with_domains(domains);}
            let mut scope=Scope::current(self);scope.writes=Some(&self.loop_contexts.last().unwrap().writes);
            let mut continues=None;let next=unsafe { self.world.call(self.numeric_loop_paths(body,Some(NumericEffect::preserve()),&mut continues,&name,&target,&writes,!has_numeric_loop_control(body,true),&mut scope,&bounds::Clause::default(),None,&mut None)).await }?;
            retained.push((name,target,input,guard,join_numeric_paths(next,continues)));
        }
        let iterations=retained.iter().filter_map(|(_,_,input,guard,effect)|effect.as_ref().and_then(|effect|effect.iteration_bound(input,guard))).min();
        for(name,target,mut proof,guard,effect)in retained {
            let bounded=iterations.and_then(|iterations|effect.as_ref()?.bounded_ranges(&proof,iterations));
            let(lower,upper)=effect.as_ref().map_or((true,true),|effect|effect.retained(&mut proof,&guard,bounded.is_some()));
            if !lower&&!upper&&bounded.is_none(){continue;}
            if let LoopPlace::Element(target,component)=target {
                if target.index.is_none(){continue;}
                let retain=|previous:&Value,range:Option<(u32,u32)>|{let mut value=previous.clone();value.proof.retain_loop_bounds(&proof,0,lower,upper);if let Some((low,high))=range{value.proof.retain_loop_bounds(&bounds::Proof::unsigned(low,high),0,true,true);}bounds::Proof::capture(&value)};
                let mut environment=self.env.clone();unsafe { self.world.call(self.retain_loop_element(&target,component,&bounds::Condition::constant(true),&mut environment,(&entry_path,initial),&|value|retain(value,bounded.map(|ranges|ranges.0)))).await }?;self.env=environment;
                unsafe { self.world.call(self.retain_loop_element(&target,component,&bounds::Condition::constant(true),header,(&entry_path,initial),&|value|retain(value,bounded.map(|ranges|ranges.1)))).await }?;
            }else{for(environment,range)in [(&mut self.env,bounded.map(|ranges|ranges.0)),(&mut *header,bounded.map(|ranges|ranges.1))]{
                let Some(binding)=environment.get_mut(&name)else{continue;};
                let current=match &target{
                    LoopPlace::Stored(_,component)=>binding.proof_storage().map(|(proof,_,_)|(proof,*component)),
                    LoopPlace::Length=>if let Binding::ApplicationData(data)=binding{Some((&mut data.length.proof,0))}else{None},
                    LoopPlace::Element(..)|LoopPlace::Local=>None,
                };
                if let Some((current,component))=current{
                    current.retain_loop_bounds(&proof,component,lower,upper);
                    if let Some((low,high))=range{current.retain_loop_bounds(&bounds::Proof::unsigned(low,high),component,true,true);}
                }
            }}
        }
        unsafe { self.world.call(self.retain_loop_ownership(&writes,condition,body,header)).await }?;
        Ok(iterations!=Some(0))
    }
    async fn retain_loop_ownership(&mut self,writes:&[Write],condition:Option<&Expr>,body:&Block,header:&mut BTreeMap<String,Binding>)->Result<()> {
        use owned_presence::Presence;use ownership::ApplicationIndex;use returns::Returned;
        fn index(proof:&bounds::Proof)->Option<ApplicationIndex>{
            if let Some(bits)=proof.known_value(&Ty::scalar(Scalar::U32)){return Some(ApplicationIndex::Constant(bits[0] as u32));}
            match proof.known_identity(0){Some(ValueIdentity::Register(reg))=>Some(ApplicationIndex::Value(ValueIdentity::Register(reg),reg)),_=>None}
        }
        // The mask follows actual already-evaluated inputs. It introduces no new runtime value.
        let context=self.loop_contexts.last().unwrap();let initial=context.control_source.bindings(&context.entry);let mut changes=Vec::new();
        for effect in writes{
            let Write::Move{place,..}=effect else{continue;};
            let point=std::cell::Cell::new(None::<Expr>);
            let Some((projection,_,_))=unsafe { self.world.call(self.loop_write_projection(place,&async |at,dataset|{
                if dataset{point.set(Some(at.clone()));}
                let operand=match at.kind(){ExprKind::Name(name)=>match self.env.get(name){Some(Binding::Owned(value))if value.place.ty()==&Ty::scalar(Scalar::U32)=>value.proof.known_identity(0).map(bounds::Operand::Value),_=>self.loop_index_operand(at,writes)?},_=>self.loop_index_operand(at,writes)?};
                Ok((operand,None))
            })).await }?else{continue;};
            let(Some(data),Some(bounds::Operand::Value(id)))=(projection.data,projection.index)else{continue;};
            let Some(begin)=initial.get(&id).and_then(index)else{continue;};
            let Some(view_contracts::ControlInput::Value{name,contents:None,component})=context.control_source.inputs.get(&id)else{continue;};
            let cursor=if let Some(cursor)=context.control_source.cursor.as_ref().filter(|cursor|cursor.identity()==id){ApplicationIndex::Value(id,cursor.regs[0])}
                else{
                    let Some(Binding::Owned(value))=self.env.get(name)else{continue;};
                    let Place::Local(_,slots,..)=value.place.storage()else{continue;};
                    let Some(reg)=self.code.iter().rev().find_map(|instruction|match instruction.kind{
                        InstKind::LoadLocal(reg,slot)if slot==slots[*component]=>Some(reg),_=>None,
                    })else{continue;};
                    ApplicationIndex::Value(id,reg)
                };
            let target=if context.control_source.cursor.is_some(){LoopPlace::Local}else{match self.env.get(name){Some(Binding::Owned(value))=>LoopPlace::Stored(value.place.clone(),*component),_=>continue}};
            if let Some(condition)=condition{
                if !unsafe { self.world.call(self.numeric_expression_effect(condition,name,&target,writes)).await }?.zero_delta(){continue;}
            }
            let point=point.take();let point=point.as_ref().unwrap_or(place);
            let mut scope=Scope::current(self);scope.writes=Some(&context.writes);let mut observed=None;
            let point_writes=if context.control_source.cursor.is_some(){&[][..]}else{writes};
            unsafe { self.world.call(self.numeric_loop_paths(body,Some(NumericEffect::preserve()),&mut None,name,&target,point_writes,false,&mut scope,&bounds::Clause::default(),Some(point),&mut observed)).await }?;
            if !observed.as_ref().is_some_and(NumericEffect::is_identity){continue;}
            if data.place.as_ref().is_some_and(|place|matches!(place.storage(),Place::Local(_,slots,..)if slots.iter().any(|slot|context.writes.locals.contains(slot)))){continue;}
            let mut components=projection.value.1.iter().flatten().copied().collect::<BTreeSet<_>>();
            for write in writes{if let Write::Position{place,..}=write{
                if let Some((written,_,_))=unsafe { self.world.call(self.loop_write_projection(place,&async |at,_|self.loop_index_operand(at,writes).map(|at|(at,None)))).await }?
                    &&written.data.is_some_and(|other|other.root.identity()==data.root.identity())&&written.base==projection.base {
                    for component in written.value.1.iter().flatten(){components.remove(component);}
                }
            }}
            if components.is_empty(){continue;}
            let mut selected = None;
            for (name, binding) in &self.env {
                let duplicate = view::visit_data_paths(binding, |path, owner, _| {
                    if owner.root.identity() == data.root.identity()
                        && self.resolve_ownership(&owner.ownership, None) == application::Ownership::Owned
                    {
                        if selected.is_some() { return ControlFlow::Break(()); }
                        selected = Some((name, path.to_vec()));
                    }
                    ControlFlow::Continue(())
                });
                if duplicate.is_break() {
                    selected = None;
                    break;
                }
            }
            let Some((name, path)) = selected else { continue; };
            let ordinal=ApplicationIndex::Value(ValueIdentity::ElementIndex(0),0);
            let remaining=Presence::indices(Comparison::Less,ordinal,begin).or(Presence::indices(Comparison::GreaterEqual,ordinal,cursor));
            changes.push((name.clone(), path, components, remaining));
        }
        fn qualify(model:&mut Returned,when:&bounds::Condition){
            let path=when.assume(&bounds::Clause::default(),true);
            match model{
                Returned::Dataset(data)=>{
                    data.root.proof.set_view_borrow(0,view::resource_borrow(data).restricted(&path));
                    if let Some(child)=data.elements.known_mut(){qualify(child,&when.scope_element_indices(1));}
                },
                Returned::OwnedValue(value)=>{
                    for component in 0..value.value.ty.components.len(){let borrow=value.value.proof.view_borrow(component).restricted(&path);value.value.proof.set_view_borrow(component,borrow);}
                    for child in value.payloads.iter_mut().flatten(){qualify(child,when);}
                },_=>{},
            }
        }
        fn restrict(model:&mut Returned,ty:&Ty,components:&BTreeSet<usize>,when:&Presence){
            if (0..ty.components.len()).all(|component|components.contains(&component)){
                model.restrict_owned(when);qualify(model,&when.condition());return;
            }
            let Returned::OwnedValue(value)=model else{return;};if ty.has_owned_presence(){return;}
            for(field,child)in value.payloads.iter_mut().enumerate(){let Some(child)=child.as_deref_mut()else{continue;};let(offset,child_ty)=ty.aggregate_part(field);
                let selected=child_ty.components.iter().enumerate().filter_map(|(part,(scalar,word))|ty.components.iter().position(|item|item==&(*scalar,offset+word)).filter(|at|components.contains(at)).map(|_|part)).collect::<BTreeSet<_>>();
                if !selected.is_empty(){restrict(child,child_ty,&selected,when);}
            }
        }
        for(name,path,components,remaining)in changes{
            let apply=async |builder:&mut Self,binding:&mut Binding|->Result<()>{
                let data=view::data_at_mut(binding,&path).unwrap();let mut model=*unsafe { builder.world.call(builder.element_model(&data.ty,&data.elements,(&data.residency,&[]),builder.scope.span())).await }?;
                restrict(&mut model,&data.ty,&components,&remaining);data.elements=application::ElementFacts::Known(Box::new(model));Ok(())
            };
            let mut current=self.env.remove(&name).unwrap();apply(self,&mut current).await?;self.env.insert(name.clone(),current);
            let mut incoming=self.loop_contexts.last_mut().unwrap().entry.env.remove(&name).unwrap();apply(self,&mut incoming).await?;self.loop_contexts.last_mut().unwrap().entry.env.insert(name.clone(),incoming);
            let mut exit=header.remove(&name).unwrap();apply(self,&mut exit).await?;header.insert(name,exit);
        }
        Ok(())
    }
}
