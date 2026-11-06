//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;
mod sources;
use sources::{Local, Scope};

type ComponentProjection = (Ty, Vec<Vec<usize>>);

enum Write<'e> {
    Position {
        place: &'e Expr,
        call: Option<&'e Expr>,
        contents: bool,
        iterator: Option<(&'e str, &'e Expr)>,
    },
    External { call:&'e Expr, instance:Option<crate::generics::Instance>, arguments:BTreeMap<ValueIdentity,bounds::Proof>, syntax:&'e[Expr] },
    Memory { call:&'e Expr, borrow:view::Borrow },
    Generator(&'e Expr),
}

// Immutable output of the existing loop write preanalysis, not memory state.
#[derive(Default)]
pub(super) struct LoopWrites {
    pub locals: BTreeSet<usize>,
    pub storage: Vec<(Reg,Option<std::ops::Range<u128>>)>,
    pub unknown_storage: bool,
    pub external: bool,
}

impl Builder<'_> {
    // 只投影可能改写的现有所有者，不求值下标或再次生成表达式代码。
    fn written_projection(&self,expr:&Expr,name:&str,owner:&Place,iterator:Option<(&str,&Expr)>,writes:&[Write<'_>])->Result<Option<ComponentProjection>> {
        self.component_projection(expr,name,owner.ty(),Some(owner),iterator,&|index|self.constant_index(index,writes))
    }
    fn component_projection(
        &self,
        expr: &Expr,
        name: &str,
        root_ty: &Ty,
        owner: Option<&Place>,
        iterator: Option<(&str, &Expr)>,
        known_index: &impl Fn(&Expr)->Result<Option<u32>>,
    ) -> Result<Option<ComponentProjection>> {
        match &expr.kind {
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
                    self.component_projection(base,name,root_ty,owner,iterator,known_index)?
                else {
                    return Ok(None);
                };
                let (ty, selected) = member_type(&ty, field, &self.definition.source, expr.span)?;
                Ok(Some((
                    ty,
                    selected.iter().map(|i| components[*i].clone()).collect(),
                )))
            }
            ExprKind::Index(base, index) => {
                if let Some((ty, mut components)) = owner.and_then(|owner|self.application_write_projection(base,owner)) {
                    if let (Some(index), Some(Place::ApplicationElement { index_range, .. })) =
                        (known_index(index)?, owner.map(Place::storage))
                        && (index < index_range.0 || index > index_range.1)
                    {
                        for selected in &mut components {
                            selected.clear();
                        }
                    }
                    return Ok(Some((ty, components)));
                }
                if let ExprKind::Name(root) = &base.kind
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
                    self.component_projection(base,name,root_ty,owner,iterator,known_index)?
                else {
                    return Ok(None);
                };
                let Some(element) = &ty.array else {
                    return Ok(None);
                };
                let constant = known_index(index)?;
                let width = element.components.len();
                let elements: Vec<_> = (0..ty.values[0] as usize)
                    .filter(|i| constant.is_none_or(|value| *i == value as usize))
                    .collect();
                let selected = (0..width)
                    .map(|component| {
                        elements
                            .iter()
                            .flat_map(|i| components[*i * width + component].iter().copied())
                            .collect()
                    })
                    .collect();
                Ok(Some(((**element).clone(), selected)))
            }
            ExprKind::Specialize(name,explicit,args) if name=="unsafe_position"=>{
                let Some(owner)=owner else{return Ok(None);};
                let ([GenericArgument::Type(syntax)],[_])=(explicit.as_slice(),args.as_slice()) else{return Ok(None);};
                let ty=self.world.ty(syntax,&self.definition.source,expr.span,&self.definition.module,&self.definition.imports)?;
                // Syntax has not read the address; actual Memory and loop producers resolve its writes.
                let components=vec![(0..owner.ty().components.len()).collect();ty.components.len()];
                Ok(Some((ty,components)))
            }
            _ => Ok(None),
        }
    }
    fn loop_memory_components(&self,borrow:&view::Borrow,owner:&Place)->Vec<usize> {
        match self.address_write_positions(borrow){Some(positions)=>self.address_write_components(&positions,owner),None=>(0..owner.ty().components.len()).collect()}
    }
    fn constant_index(&self, index: &Expr, writes: &[Write<'_>]) -> Result<Option<u32>> {
        Ok(match &index.kind {
            ExprKind::StaticU32(value) => Some(*value),
            ExprKind::Number(text) => {
                let (ty, value) = number(&self.definition.source, index.span, text)?;
                (ty == Ty::scalar(Scalar::U32)).then_some(value as u32)
            }
            ExprKind::Name(name)
                if !writes
                    .iter()
                    .any(|write| match write {
                        Write::External{..}|Write::Generator(_)=>true,
                        Write::Memory{borrow,..}=>self.env.get(name).and_then(Binding::stored_place).is_some_and(|owner|!self.loop_memory_components(borrow,&owner).is_empty()),
                        Write::Position{contents:false,place,..}=>assigned_root(place)==Some(name)||address_write(place),
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
        if let ExprKind::Name(name) = &expr.kind {
            match self.env.get(name) {
                Some(Binding::ApplicationData(data))=>return Some((data,0,&data.ty)),
                Some(Binding::DataReference(data))=>{let(offset,ty)=data.element_layout();return Some((&data.data,offset,ty));},
                _=>{},
            }
        }
        if let ExprKind::Member(base,field)=&expr.kind && let Some((data,offset,ty))=self.written_application_data(base) {
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
    fn invalidate_writes(
        &mut self,
        writes: &[Write<'_>],
        borrowed_data: Option<ValueIdentity>,
        effect: &LoopWrites,
    ) -> Result<()> {
        for write in writes {
            let changed=match write {
                Write::Memory{borrow,..}if self.address_write_positions(borrow).is_some()=>{self.write_address_values(borrow);continue;},
                Write::External{..}|Write::Generator(_)|Write::Memory{..}=>{
                    let contents=self.env.values().flat_map(Binding::application_resources).map(|data|(
                        data.root.identity(),view::resource_borrow(data),data.ty.components.iter().map(|(_,word)|*word).collect::<Vec<_>>()
                    )).collect::<Vec<_>>();
                    for(source,resource,words)in contents{self.update_application_elements(&[source],&resource,&words,application::ElementUpdate::Unknown);}
                    continue;
                },
                Write::Position{place,contents:true,..}=>self.written_application_data(place).map(|(data,offset,ty)|(
                    data.root.identity(),view::resource_borrow(data),ty.components.iter().map(|(_,word)|offset+word).collect::<Vec<_>>()
                )),
                Write::Position{place,contents:false,..}=>self.loop_write_projection(place,&|index|self.constant_index(index,writes))?.map(|(data,_,(_,components))|(
                    data.root.identity(),view::resource_borrow(data),components.into_iter().flatten().map(|i|data.ty.components[i].1).collect::<Vec<_>>()
                )),
            };
            if let Some((source,resource,words))=changed{self.update_application_elements(&[source],&resource,&words,application::ElementUpdate::Unknown);}
        }
        let resources: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| {
                let Binding::ApplicationData(data) = binding else {
                    return None;
                };
                (borrowed_data != Some(data.root.identity())
                    && !data
                        .root
                        .proof
                        .known_identity(0)
                        .is_some_and(|id| self.application_borrowed(id))
                    && writes.iter().any(|write| matches!(write,
                        Write::Position{contents:false,place,..} if matches!(&place.kind, ExprKind::Name(target) if target == name))))
                .then(|| name.clone())
            })
            .collect();
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.root.proof = bounds::Proof::default();
                data.length.proof = data
                    .count
                    .map(|count| bounds::Proof::unsigned(count, count))
                    .unwrap_or_default();
                data.length.constant = None;
                data.length.uniform = false;
            }
        }
        let mut modified: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (name, binding) in &self.env {
            if let Some(owner) = binding.stored_place() {
                for write in writes {
                    let components = match write {
                        Write::External{..}|Write::Generator(_)=>vec![(0..owner.ty().components.len()).collect()],
                        Write::Memory{borrow,..}=>vec![self.loop_memory_components(borrow,&owner)],
                        Write::Position{place,contents,iterator,..}=>{
                            let projection=if *contents {self.application_write_projection(place,&owner)}
                                else {self.written_projection(place,name,&owner,*iterator,writes)?};
                            projection.map(|(_,components)|components).unwrap_or_default()
                        },
                    };
                    for component in components.into_iter().flatten() {
                        modified.entry(name.clone()).or_default().insert(component);
                    }
                }
            }
        }
        let raw=writes.iter().any(|write|match write {
            Write::External{..}|Write::Generator(_)|Write::Memory{..}=>true,
            Write::Position{contents:false,place,..}=>address_write(place),
            _=>false,
        });
        for (name, binding) in &mut self.env {
            if let Some(components) = modified.get(name) {
                let components: Vec<_> = components.iter().copied().collect();
                if raw && let Some((proof,_,storage))=binding.proof_storage() {
                    for(offset,field)in storage.ty().borrow_fields() {
                        if field.is_address() {
                            let component=storage.ty().components.iter().position(|(_,at)|*at==offset).unwrap();
                            if components.contains(&component){
                                let borrow=proof.view_borrow(component).joined(view::Borrow::source(view::Source::External));
                                *proof=std::mem::take(proof).with_view_borrow(component,borrow);
                            }
                        }
                    }
                }
                match binding {
                    Binding::Owned(owner) => {
                        owner.uniform = false;
                        owner.proof.forget_value(&components,owner.place.ty().components.len());
                        // Direct writes retain their numeric analysis; indirect changes use this loop's original inputs.
                        let context=self.loop_contexts.last_mut().unwrap();
                        if context.view_scope.is_some() && context.entry.env.contains_key(name)
                            && let Place::Local(_,slots,..)=owner.place.storage() {
                            for component in components.into_iter().filter(|component|!effect.locals.contains(&slots[*component])) {
                                let scalar=owner.place.ty().components[component].0;
                                let id=ValueIdentity::Register(self.registers.len());self.registers.push(scalar);
                                let input=view::InputSlot::Value{name:name.clone(),contents:None,component,identity:Some((id,scalar))};
                                if let Some(slot)=context.view_inputs.iter_mut().find(|slot|matches!(slot,view::InputSlot::Value{name:target,contents:None,component:part,..}if target==name&&*part==component)) {*slot=input;}
                                else{context.view_inputs.push(input);}
                                owner.proof=std::mem::take(&mut owner.proof).with_component_identity(component,id);
                            }
                        }
                    }
                    Binding::OwnedValue(owner) => {
                        owner.invalidate_components(&components, borrowed_data)
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    fn loop_write_projection<'a>(
        &'a self,expr:&Expr,known_index:&impl Fn(&Expr)->Result<Option<u32>>,
    )->Result<Option<(&'a application::Data,Option<u32>,ComponentProjection)>> {
        match &expr.kind {
            ExprKind::Name(name)=>{
                let Some(Binding::Owned(owner))=self.env.get(name)else{return Ok(None);};
                let Place::ApplicationElement{identity,words,index_range,..}=&owner.place else{return Ok(None);};
                let Some(data)=self.env.values().flat_map(Binding::application_resources)
                    .find(|data|data.root.identity()==*identity)else{return Ok(None);};
                let components=words.iter().map(|word|data.ty.components.iter().enumerate()
                    .filter_map(|(i,(_,offset))|(word==offset).then_some(i)).collect()).collect();
                let index=(index_range.0==index_range.1).then_some(index_range.0);
                Ok(Some((data,index,(owner.place.ty().clone(),components))))
            },
            ExprKind::Index(base,index)=>{
                if let Some((data,offset,ty))=self.written_application_data(base) {
                    let components=ty.components.iter().map(|(scalar,word)|vec![data.ty.components.iter().position(|part|part==&(*scalar,offset+word)).unwrap()]).collect();
                    return Ok(Some((data,known_index(index)?,(ty.clone(),components))));
                }
                let Some((data,at,(ty,components)))=self.loop_write_projection(base,known_index)? else{return Ok(None);};
                let Some(element)=&ty.array else{return Ok(None);};
                let constant=known_index(index)?;let width=element.components.len();
                let elements=(0..ty.values[0] as usize).filter(|i|constant.is_none_or(|value|*i==value as usize)).collect::<Vec<_>>();
                let selected=(0..width).map(|component|elements.iter().flat_map(|i|components[*i*width+component].iter().copied()).collect()).collect();
                Ok(Some((data,at,((**element).clone(),selected))))
            },
            ExprKind::Member(base,field)=>{
                let Some((data,at,(ty,components)))=self.loop_write_projection(base,known_index)? else{return Ok(None);};
                let(ty,selected)=member_type(&ty,field,&self.definition.source,expr.span)?;
                Ok(Some((data,at,(ty,selected.iter().map(|i|components[*i].clone()).collect()))))
            },
            _=>Ok(None),
        }
    }
    fn normalize_writes<'e>(&mut self,writes:Vec<Write<'e>>)->Result<Vec<Write<'e>>>{
        let mut normalized=Vec::new();
        for write in writes {
            let write=match write {
                Write::Generator(factory)=>{
                    if let Some(instance)=Scope::current(self).factory_instance(factory)? {
                        let function=&instance.definition.function;
                        if !function.c_abi&&function.kind==FunctionKind::Value&&function.target==Target::Cpu
                            &&function.params.len()==1&&function.params[0].access==Access::Value {
                            self.native_declaration(&instance)?;
                            let definition=&instance.definition;
                            let result=self.world.ty(&definition.function.result,&definition.source,definition.function.span,&definition.module,&definition.imports)?;
                            if !result.owns_resources()&&self.functions.writes_only_references(&instance.name){continue;}
                        }
                    }
                    Write::External{call:factory,instance:None,arguments:BTreeMap::new(),syntax:&[]}
                },
                Write::External{call,instance:Some(instance),arguments,syntax}=>{
                    self.native_declaration(&instance)?;
                    if self.functions.read_only(&instance.name) {
                        normalized.retain(|write|!matches!(write,Write::Position{call:Some(origin),..} if std::ptr::eq(*origin,call)));
                    }
                    // Closed memory effects own writes; owned arguments still consume their places.
                    let mut owned=Vec::new();
                    for(parameter,arg)in instance.definition.function.params.iter().zip(syntax){
                        if cooperative::parameter_type(self.world,parameter,&instance.definition)?.owns_resources(){owned.push(arg);}
                    }
                    normalized.retain(|write|!matches!(write,Write::Position{call:Some(origin),place,..}
                        if std::ptr::eq(*origin,call)&&!owned.iter().any(|arg|std::ptr::eq(*arg,*place))));
                    let borrow=match self.native_call_memory(&instance.name,&arguments){Some(borrow)=>borrow,None=>view::Borrow::source(view::Source::Unresolved)};
                    if !borrow.active(){continue;}
                    Write::Memory{call,borrow}
                },
                write=>write,
            };
            normalized.push(write);
        }
        Ok(normalized)
    }
    fn project_loop_writes(&self,writes:&[Write<'_>])->Result<LoopWrites>{
        let mut effect=LoopWrites::default();
        for write in writes {
            match write {
                Write::External{..}|Write::Generator(_)=>{effect.unknown_storage=true;effect.external=true;},
                Write::Memory{borrow,..}=>{
                    let Some(positions)=self.address_write_positions(borrow)else{effect.unknown_storage=true;effect.external=true;continue;};
                    if borrow.resources().active(){effect.unknown_storage=true;}
                    for binding in self.env.values(){
                        let Some(owner)=binding.stored_place()else{continue;};
                        let components=self.address_write_components(&positions,&owner);
                        if components.is_empty(){continue;}
                        match owner.storage(){
                            Place::Local(_,slots,..)=>effect.locals.extend(components.into_iter().map(|component|slots[component])),
                            Place::Reference{..}=>{effect.unknown_storage=true;effect.external=true;},
                            _=>effect.unknown_storage=true,
                        }
                    }
                },
                Write::Position{place,contents,iterator,..}=>{
                    if address_write(place) {effect.unknown_storage=true;effect.external=true;continue;}
                    if iterator.is_some() {effect.unknown_storage=true;continue;}
                    if *contents {
                        if let Some((data,_,ty))=self.written_application_data(place) && !ty.owns_resources() {
                            effect.storage.push((data.root.regs[0],None));
                        }else{effect.unknown_storage=true;}
                        continue;
                    }
                    if let Some((data,index,(_,components)))=self.loop_write_projection(place,&|index|self.constant_index(index,writes))? {
                        let root=data.root.regs[0];
                        if let (Some(index),application::DataLayout::Declared)=(index,&data.layout) {
                            let base=u128::from(index)*u128::from(ir::align_up(data.ty.size,data.ty.align));
                            for component in components.into_iter().flatten() {
                                let(scalar,word)=data.ty.components[component];let start=base+u128::from(word);
                                effect.storage.push((root,Some(start..start+u128::from(scalar.size()))));
                            }
                        }else{effect.storage.push((root,None));}
                        continue;
                    }
                    let Some(name)=assigned_root(place) else {effect.unknown_storage=true;continue;};
                    let Some(binding)=self.env.get(name) else {
                        if !matches!(place.kind,ExprKind::Name(_)){effect.unknown_storage=true;}
                        continue;
                    };
                    if let Some(owner)=binding.stored_place()
                        && let Place::Local(_,slots,..)=&owner
                        && let Some((_,components))=self.written_projection(place,name,&owner,None,writes)? {
                        effect.locals.extend(components.into_iter().flatten().map(|component|slots[component]));
                    }else{
                        effect.unknown_storage=true;
                        if matches!(binding.stored_place(),Some(Place::Reference{..})){effect.external=true;}
                    }
                },
            }
        }
        Ok(effect)
    }
    pub(super) fn loop_writes(
        &mut self,
        condition: Option<&Expr>,
        body: &[Stmt],
        borrowed_data: Option<ValueIdentity>,
        initial: bool,
    ) -> Result<LoopWrites> {
        let mut writes = Vec::new();
        statement_writes(self, body, &mut |place| writes.push(place))?;
        if let Some(condition)=condition {
            expression_writes(self, condition, &mut |place| writes.push(place))?;
        }
        let writes=self.normalize_writes(writes)?;
        let mut effect=self.project_loop_writes(&writes)?;
        let writes=if !effect.external && self.env.values().any(|binding|matches!(binding,Binding::Owned(owner) if *owner.place.ty()==Ty::scalar(Scalar::Bool))) {
            let mut selected=Vec::new();
            {
                let mut scope=Scope::current(self);scope.writes=Some(&effect);
                scope.writes_statements(body,&mut|write|selected.push(write))?;
                if let Some(condition)=condition {scope.writes_expression(condition,&mut|write|selected.push(write))?;}
            }
            let writes=self.normalize_writes(selected)?;
            effect=self.project_loop_writes(&writes)?;
            writes
        }else{writes};
        if !initial {self.invalidate_writes(&writes,borrowed_data,&effect)?;}
        Ok(effect)
    }
}

fn address_write(expr:&Expr)->bool {
    match &expr.kind {
        ExprKind::Specialize(name,_,_)=>name=="unsafe_position",
        ExprKind::Member(base,_)|ExprKind::Index(base,_)=>address_write(base),
        _=>false,
    }
}
fn assigned_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name),
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(base),
        _ => None,
    }
}
impl<'b, 'w, 'e> Scope<'b, 'w, 'e> {

    fn input_snapshot(&self,expr:&Expr,limit:usize)->Result<Option<(Ty,bounds::Proof,view::Borrow)>> {
        let builder=self.builder.unwrap();
        if let ExprKind::Name(name)=&expr.kind
            &&let Some(i)=self.bindings[..limit].iter().rposition(|(bound,_)|*bound==name)
            &&let Local::Inferred(value)=self.bindings[i].1
            &&!builder.mutable_names.contains(name) {return self.input_snapshot(value,i);}
        let Some(name)=assigned_root(expr)else{
            if let ExprKind::Bool(value)=expr.kind{return Ok(Some((Ty::scalar(Scalar::Bool),bounds::Proof::condition(bounds::Condition::constant(value)),view::Borrow::default())));}
            return Ok(None);
        };
        if self.bindings[..limit].iter().any(|(bound,_)|*bound==name){return Ok(None);}
        let Some(binding)=builder.env.get(name)else{return Ok(None);};
        let known_index=|index:&Expr| {
            if let ExprKind::Name(name)=&index.kind
                &&(self.bindings[..limit].iter().any(|(bound,_)|*bound==name)||!matches!(builder.env.get(name),Some(Binding::Value(_)))) {return Ok(None);}
            builder.constant_index(index,&[])
        };
        if matches!(binding,Binding::ApplicationData(_)|Binding::DataReference(_)|Binding::OwnedValue(_))
            && let Some((data,_,(ty,parts)))=builder.loop_write_projection(expr,&known_index)? {
            let mut borrow=view::Borrow::default();
            for choice in 0..parts.first().map_or(0,Vec::len) {
                let origin=address::Origin {
                    root:address::Root::Resource(view::resource_borrow(data)),base:address::Layout::of(&data.ty),layout:address::Layout::of(&ty),
                    words:parts.iter().map(|part|data.ty.components[part[choice]].1).collect(),positions:Arc::from([]),
                    writable:matches!(data.access,Access::Write|Access::InOut),initialized:vec![false;ty.components.len()].into(),
                };
                borrow.union(&origin.normalize());
            }
            let proof=bounds::Proof::default().with_external_borrows(&ty);
            return Ok(Some((ty,proof,borrow)));
        }
        let (ty,proof,owner)=match binding {
            Binding::Value(value)=>(value.ty.clone(),bounds::Proof::capture(value),None),
            Binding::Owned(owner)=>{
                let mut proof=bounds::Proof::default().with_external_borrows(owner.place.ty());
                if let Some(writes)=self.writes && !writes.external
                    && *owner.place.ty()==Ty::scalar(Scalar::Bool)
                    && let Place::Local(_,slots,..)=owner.place.storage()
                    && slots.iter().all(|slot|!writes.locals.contains(slot)) {proof=owner.proof.clone();}
                (owner.place.ty().clone(),proof,Some(owner.place.clone()))
            },
            Binding::OwnedValue(owner)=>(owner.value.ty.clone(),bounds::Proof::default().with_external_borrows(&owner.value.ty),owner.place()),
            _=>return Ok(None),
        };
        let Some((selected,parts))=builder.component_projection(expr,name,&ty,owner.as_ref(),None,&known_index)?else{return Ok(None);};
        let mut value=None::<bounds::Proof>;let mut borrow=view::Borrow::default();
        for choice in 0..parts.first().map_or(0,Vec::len) {
            let indices=parts.iter().map(|part|part[choice]).collect::<Vec<_>>();
            let projection=proof.project(&indices,ty.components.len());
            if let Some(value)=&mut value{value.join_condition(&projection,selected.components.len(),None);}else{value=Some(projection);}
            if let Some(place)=&owner{borrow.union(&builder.position_origin(&place.clone().project(selected.clone(),&indices),expr.span)?.normalize());}
        }
        Ok(Some((selected,value.unwrap_or_default(),borrow)))
    }
    fn call_inputs(&self,definition:&Definition,args:&[Expr])->Result<BTreeMap<ValueIdentity,bounds::Proof>> {
        let builder=self.builder.unwrap();let mut inputs=BTreeMap::new();let mut offset=application::pointer_type().components.len();
        for(index,(parameter,arg))in definition.function.params.iter().zip(args).enumerate() {
            let ty=cooperative::parameter_type(self.world,parameter,definition)?;
            if parameter.ty.dataset {
                let descriptor=Builder::borrowed_dataset_parameter_type(parameter.access,definition.execution);
                if let Some(name)=assigned_root(arg) &&!self.bindings.iter().any(|(bound,_)|*bound==name) {
                    if let Some((data,field,ty))=builder.written_application_data(arg) {
                        let origin=application::dataset_origin(data,field,ty,false);
                        inputs.insert(ValueIdentity::Register(offset),data.root.proof.clone().with_view_borrow(0,origin.normalize()));
                        inputs.insert(ValueIdentity::Register(offset+1),bounds::Proof::capture(&data.length));
                    }
                }
                offset+=descriptor.components.len();continue;
            }
            let(proof,storage)=match self.input_snapshot(arg,self.bindings.len())?{Some((_,proof,storage))=>(proof,storage),None=>(bounds::Proof::default().with_external_borrows(&ty),view::Borrow::source(view::Source::Unresolved))};
            if matches!(parameter.access,Access::Read|Access::Write|Access::InOut|Access::Atomic) {
                let storage=if storage.empty(){view::Borrow::source(view::Source::Unresolved)}else{storage};
                inputs.insert(ValueIdentity::ParameterStorage(index),bounds::Proof::default().with_view_borrow(0,storage));
                for component in 0..ty.components.len(){inputs.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                offset+=native_function::reference_parameters(&ty).len()*application::pointer_type().components.len();
            }else{
                for component in 0..ty.components.len(){inputs.insert(ValueIdentity::Register(offset+component),proof.project(&[component],ty.components.len()));}
                offset+=ty.components.len();
            }
        }
        Ok(inputs)
    }

    fn call_instance(&self,call:&Expr)->Result<Option<crate::generics::Instance>> {
        let(name,explicit,args)=match &call.kind {
            ExprKind::Call(name,args)=>{
                if self.named(name,call.span)?.is_some(){return Ok(None);}
                (name.as_str(),None,args)
            },
            ExprKind::Specialize(name,types,args)=>(name.as_str(),Some(types.as_slice()),args),
            _=>return Ok(None),
        };
        let Some(key)=self.world.resolve(name,&self.definition.module,&self.definition.imports,true)else{return Ok(None);};
        let function=&self.world.functions[&key].function;
        if function.c_abi||function.kind!=FunctionKind::Value||function.target!=Target::Cpu{return Ok(None);}
        let types=args.iter().map(|arg|self.expression(arg)).collect::<Result<Vec<_>>>()?;
        let actuals=types.iter().map(|ty|generics::Actual{
            ty:ty.as_ref().map(std::borrow::Cow::Borrowed),
            count:ty.as_ref().and_then(|ty|ty.dataset.as_ref().and(ty.values.first().copied())),
        }).collect::<Vec<_>>();
        generics::specialize(self.world,&key,&actuals,explicit,
            (&self.definition.source,&self.definition.module,&self.definition.imports),call.span).map(Some)
    }
    fn factory_instance(&self,factory:&Expr)->Result<Option<crate::generics::Instance>> {
        let(name,explicit)=match &factory.kind {
            ExprKind::Name(name)=>(name.as_str(),None),
            ExprKind::FunctionRef(name,types)=>(name.as_str(),Some(types.as_slice())),
            _=>return Ok(None),
        };
        let Some(key)=self.world.resolve(name,&self.definition.module,&self.definition.imports,true)else{return Ok(None);};
        let actual=generics::Actual{ty:Some(std::borrow::Cow::Owned(Ty::scalar(Scalar::U32))),count:None};
        generics::specialize(self.world,&key,&[actual],explicit,
            (&self.definition.source,&self.definition.module,&self.definition.imports),factory.span).map(Some)
    }
    fn function_writes(&self, ty: Ty, args: &'e [Expr], call: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
        let Some(function) = &ty.function else {
            return self.definition.source.fail(call.span, "B263", "间接调用目标需要cfn原生C签名值");
        };
        if outputs::external_memory(function.contracts.iter()).is_some_and(|access|access.writes()){
            writes(Write::External{call,instance:None,arguments:BTreeMap::new(),syntax:args});
        }
        for ((access, _), arg) in function.parameters.iter().zip(args) {
            if matches!(access, Access::Write | Access::InOut) {
                writes(Write::Position { place: arg, call: Some(call), contents: false, iterator: None });
            }
        }
        Ok(())
    }
    fn writes_expression(&self, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<Option<bool>> {
        let mut truth = None;
        match &expr.kind {
            ExprKind::Bool(value) => truth = Some(*value),
            ExprKind::Name(name) if self.builder.is_some_and(|builder|!builder.mutable_names.contains(name)) || self.writes.is_some() => {
                if let Some((ty,proof,_))=self.input_snapshot(expr,self.bindings.len())?
                    && ty==Ty::scalar(Scalar::Bool) {
                    truth=proof.known_value(&ty).map(|bits|bits[0]!=0);
                }
            }
            ExprKind::Invoke(callee, args) => {
                let ty = self.expression(callee)?.ok_or_else(|| Diagnostic {
                    source: self.definition.source.clone(), span: callee.span, code: "B263",
                    message: "不能从原声明与词法来源取得间接调用签名".into(),
                })?;
                self.function_writes(ty, args, expr, writes)?;
                self.writes_expression(callee, writes)?;
                for arg in args { self.writes_expression(arg, writes)?; }
            }
            ExprKind::Call(name, args) | ExprKind::Cooperate(name, _, args) | ExprKind::Specialize(name, _, args) => {
                if self.builder.is_none() && name == "address_of" && let [place] = args.as_slice() {
                    writes(Write::Position { place, call: Some(expr), contents: false, iterator: None });
                }
                if name=="take_last" && self.named(name,expr.span)?.is_none() && let [_,place]=args.as_slice() {
                    writes(Write::Position{place,call:Some(expr),contents:false,iterator:None});
                }
                let local = if matches!(expr.kind, ExprKind::Call(..)) { self.named(name, expr.span)? } else { None };
                if let Some(ty) = local { self.function_writes(ty, args, expr, writes)?; }
                else {
                    let record = matches!(expr.kind, ExprKind::Specialize(..))
                        && self.world.resolve(name, &self.definition.module, &self.definition.imports, false).is_some();
                    if !record && let Some(key) = self.world.resolve(name, &self.definition.module, &self.definition.imports, true) {
                        let instance=if self.builder.is_some(){self.call_instance(expr)?}else{None};
                        let covered=self.world.functions[&key].function.external.is_some()
                            && !outputs::Contract::from(self.world.functions[&key].as_ref()).external_memory().is_some_and(|access|access.writes());
                        let read_only=instance.as_ref().is_some_and(|instance|self.builder.is_some_and(|builder|builder.functions.read_only(&instance.name)));
                        for (param, arg) in self.world.functions[&key].function.params.iter().zip(args) {
                            if matches!(param.access, Access::InOut | Access::Write) && !param.ty.tile {
                                let contents=param.ty.dataset || self.expression(arg)?.is_some_and(|ty|ty.dataset.is_some());
                                if !read_only {writes(Write::Position { place: arg, call: Some(expr), contents, iterator: None });}
                            }
                        }
                        if !covered {let arguments=match &instance{Some(instance)=>self.call_inputs(&instance.definition,args)?,None=>BTreeMap::new()};writes(Write::External{call:expr,instance,arguments,syntax:args});}
                    }
                }
                if name=="dataset" && matches!(&expr.kind,ExprKind::Specialize(..))
                    && let [length,factory]=args.as_slice() {
                    let zero=|value:&Expr|->Result<bool>{Ok(match &value.kind {
                        ExprKind::StaticU32(count)=>*count==0,
                        ExprKind::Number(text)=>number(&self.definition.source,value.span,text)?.1==0,
                        _=>false,
                    })};
                    let empty=if zero(length)? {true}
                    else if let ExprKind::Name(name)=&length.kind {
                        match self.bindings.iter().rev().find(|(local,_)|*local==name.as_str()) {
                            Some((_,Local::Inferred(value)))if self.builder.is_some_and(|builder|!builder.mutable_names.contains(name))=>zero(value)?,
                            Some(_)=>false,
                            None=>self.builder.is_some_and(|builder|matches!(builder.env.get(name),Some(Binding::Value(value))
                                if value.ty==Ty::scalar(Scalar::U32)&&value.constant.as_ref().is_some_and(|bits|bits.first()==Some(&0)))),
                        }
                    }else{false};
                    if !empty {
                        let read_only=if let Some(builder)=self.builder
                            && let Some(instance)=self.factory_instance(factory)? {
                            let definition=&instance.definition;
                            let result=self.world.ty(&definition.function.result,&definition.source,definition.function.span,&definition.module,&definition.imports)?;
                            !result.owns_resources()&&builder.functions.writes_only_references(&instance.name)
                        }else{false};
                        if !read_only {writes(Write::Generator(factory));}
                    }
                }
                for arg in args { self.writes_expression(arg, writes)?; }
            }
            ExprKind::Construct(_, args) => for arg in args { self.writes_expression(arg, writes)?; },
            ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => { self.writes_expression(base, writes)?; }
            ExprKind::Index(a, b) => { self.writes_expression(a, writes)?; self.writes_expression(b, writes)?; }
            ExprKind::Binary(op, a, b) => {
                let left = self.writes_expression(a, writes)?;
                let skipped = matches!((op.as_str(), left), ("&&", Some(false)) | ("||", Some(true)));
                let right = if self.builder.is_none() || !skipped { self.writes_expression(b, writes)? } else { None };
                truth = match (op.as_str(), left, right) {
                    ("&&", Some(false), _) | ("&&", _, Some(false)) => Some(false),
                    ("||", Some(true), _) | ("||", _, Some(true)) => Some(true),
                    ("&&", Some(true), right) | ("||", Some(false), right) => right,
                    _ => None,
                };
            }
            ExprKind::Select(a, b, c) => {
                let selected=self.writes_expression(a,writes)?;
                if self.builder.is_none() || selected!=Some(false){self.writes_expression(b,writes)?;}
                if self.builder.is_none() || selected!=Some(true){self.writes_expression(c,writes)?;}
            }
            _ => {}
        }
        Ok(truth)
    }
    fn writes_statements(&mut self, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
        let outer = self.bindings.len();
        for statement in code {
            match &statement.kind {
                StmtKind::Declare(ty, name) => self.bindings.push((name, Local::Typed(ty))),
                StmtKind::Let(ty, name, expr) => {
                    self.writes_expression(expr, writes)?;
                    self.bindings.push((name, ty.as_ref().map_or(Local::Inferred(expr), Local::Typed)));
                }
                StmtKind::Assign(lhs, _, rhs) => {
                    writes(Write::Position { place: lhs, call: None, contents: false, iterator: None });
                    self.writes_expression(lhs, writes)?;
                    self.writes_expression(rhs, writes)?;
                }
                StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Seek(_, expr) => { self.writes_expression(expr, writes)?; }
                StmtKind::If(expr, yes, no) => {
                    let truth = self.writes_expression(expr, writes)?;
                    if truth != Some(false) { self.writes_statements(yes, writes)?; }
                    if truth != Some(true) { self.writes_statements(no, writes)?; }
                }
                StmtKind::Match(value, arms) => {
                    self.writes_expression(value, writes)?;
                    for arm in arms {
                        if let Some(name) = &arm.binding { self.bindings.push((name, Local::Payload(value, &arm.variant))); }
                        self.writes_statements(&arm.body, writes)?;
                        if arm.binding.is_some() { self.bindings.pop(); }
                    }
                }
                StmtKind::While(expr, body) => { self.writes_expression(expr, writes)?; self.writes_statements(body, writes)?; }
                StmtKind::For(name, start, end, body) => {
                    self.writes_expression(start, writes)?;
                    if let Some(end) = end { self.writes_expression(end, writes)?; }
                    self.bindings.push((name, if end.is_some() { Local::Range } else { Local::Element(start) }));
                    self.writes_statements(body, &mut |mut write| {
                        if let Write::Position{place,iterator,..}=&mut write
                            && end.is_none() && assigned_root(place)==Some(name) {*iterator=Some((name,start));}
                        writes(write);
                    })?;
                    self.bindings.pop();
                }
                StmtKind::Parallel(name, domain, body) => {
                    if let Some(count) = &domain.count { self.writes_expression(count, writes)?; }
                    if let Some(selection) = &domain.selection { self.writes_expression(selection, writes)?; }
                    self.bindings.push((name, Local::NamedElement(&domain.dataset)));
                    self.writes_statements(body, writes)?; self.bindings.pop();
                }
                _ => {}
            }
        }
        self.bindings.truncate(outer);
        Ok(())
    }
}
fn expression_writes<'e>(builder: &Builder<'_>, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
    Scope::current(builder).writes_expression(expr, writes).map(|_| ())
}
fn statement_writes<'e>(builder: &Builder<'_>, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
    Scope::current(builder).writes_statements(code, writes)
}
pub(super) fn value_writes<'e>(builder: &Builder<'_>, body: &'e [Stmt]) -> Result<Vec<&'e Expr>> {
    let mut values = Vec::new();
    statement_writes(builder, body, &mut |write| {
        if let Write::Position{contents:false,iterator:None,place,..}=write {values.push(place);}
    })?;
    Ok(values)
}
pub(super) fn expression_assignments(builder: &Builder<'_>, expr: &Expr, names: &mut BTreeSet<String>) -> Result<()> {
    expression_writes(builder, expr, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
    })
}
pub(super) fn assigned_names(world: &World, definition: &Definition, code: &[Stmt], names: &mut BTreeSet<String>) -> Result<()> {
    Scope::declared(world, definition).writes_statements(code, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
    })
}
fn contains_expression(root: &Expr, selected: &Expr) -> bool {
    if std::ptr::eq(root, selected) { return true; }
    match &root.kind {
        ExprKind::Invoke(callee, args) => contains_expression(callee, selected) || args.iter().any(|arg| contains_expression(arg, selected)),
        ExprKind::Call(_, args) | ExprKind::Specialize(_, _, args) | ExprKind::Cooperate(_, _, args) | ExprKind::Construct(_, args) =>
            args.iter().any(|arg| contains_expression(arg, selected)),
        ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => contains_expression(base, selected),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => contains_expression(a, selected) || contains_expression(b, selected),
        ExprKind::Select(a, b, c) => contains_expression(a, selected) || contains_expression(b, selected) || contains_expression(c, selected),
        _ => false,
    }
}
fn contains_statement(statement: &Stmt, selected: &Expr) -> bool {
    let expression = |expr: &Expr| contains_expression(expr, selected);
    let block = |body: &[Stmt]| body.iter().any(|statement| contains_statement(statement, selected));
    match &statement.kind {
        StmtKind::Let(_, _, expr) | StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Seek(_, expr) => expression(expr),
        StmtKind::Assign(a, _, b) => expression(a) || expression(b),
        StmtKind::If(expr, yes, no) => expression(expr) || block(yes) || block(no),
        StmtKind::Match(expr, arms) => expression(expr) || arms.iter().any(|arm| block(&arm.body)),
        StmtKind::While(expr, body) => expression(expr) || block(body),
        StmtKind::For(_, start, end, body) => expression(start) || end.as_ref().is_some_and(expression) || block(body),
        StmtKind::Parallel(_, domain, body) => domain.count.as_ref().is_some_and(expression) || domain.selection.as_ref().is_some_and(expression) || block(body),
        _ => false,
    }
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


// 同一位置写效果：位移、独立赋值，以及由同一c-x和恒等组成的有限反向关系。
#[derive(Clone)]
enum NumericEffect {
    Unknown,
    Assign(bounds::Proof),
    Reflect(u32),
    Shift { low: i128, high: i128, least: i128, most: i128 },
}
impl NumericEffect {
    fn shift(amount: i128) -> Self { Self::Shift { low: amount, high: amount, least: amount.min(0), most: amount.max(0) } }
    fn then(self, next: Self) -> Self {
        match (&self,&next) {
            (_,Self::Assign(_))=>return next,
            (Self::Assign(_)|Self::Reflect(_),Self::Shift{low:0,high:0,least:0,most:0})=>return self,
            (Self::Shift{low:0,high:0,least:0,most:0},Self::Reflect(_))=>return next,
            (Self::Reflect(a),Self::Reflect(b)) if a==b=>return self,
            _=>{},
        }
        let (Self::Shift {low:a,high:b,least:c,most:d}, Self::Shift {low:e,high:f,least:g,most:h}) = (self,next) else { return Self::Unknown; };
        let (Some(low),Some(high),Some(least),Some(most)) = (a.checked_add(e),b.checked_add(f),a.checked_add(g),b.checked_add(h)) else { return Self::Unknown; };
        Self::Shift {low,high,least:c.min(least),most:d.max(most)}
    }
    fn join(self, other: Self) -> Self {
        match (self,other) {
            (Self::Assign(mut a),Self::Assign(b))=>{a.join(&b,1,None);Self::Assign(a)},
            (Self::Reflect(a),Self::Reflect(b)) if a==b=>Self::Reflect(a),
            (a@Self::Reflect(_),Self::Shift{low:0,high:0,least:0,most:0})
            |(Self::Shift{low:0,high:0,least:0,most:0},a@Self::Reflect(_))=>a,
            (a@Self::Assign(_),Self::Shift{low:0,high:0,least:0,most:0})
            |(Self::Shift{low:0,high:0,least:0,most:0},a@Self::Assign(_))=>a,
            (Self::Shift{low:a,high:b,least:c,most:d},Self::Shift{low:e,high:f,least:g,most:h})=>
                Self::Shift{low:a.min(e),high:b.max(f),least:c.min(g),most:d.max(h)},
            _=>Self::Unknown,
        }
    }
}
fn join_numeric_paths(a: Option<NumericEffect>, b: Option<NumericEffect>) -> Option<NumericEffect> {
    match (a,b) { (Some(a),Some(b))=>Some(a.join(b)), (a,None)|(None,a)=>a }
}
impl Builder<'_> {
    fn numeric_write(&self, write: &Write<'_>, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<bool> {
        if let Write::Memory{borrow,..}=write{return Ok(self.loop_memory_components(borrow,owner).contains(&component));}
        let Write::Position{place,contents,iterator,..}=write else{return Ok(true);};
        let projection = if *contents { self.application_write_projection(place, owner) }
            else { self.written_projection(place, name, owner, *iterator, writes)? };
        Ok(projection.is_some_and(|(_,parts)|parts.iter().any(|part|part.contains(&component))))
    }
    fn numeric_expression_effect(&self, expr: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<NumericEffect> {
        for write in writes.iter().filter(|write|match write {
            Write::External{call,..}|Write::Memory{call,..}|Write::Generator(call)=>contains_expression(expr,call),
            Write::Position{call,..}=>call.is_some_and(|call|contains_expression(expr,call)),
        }) {
            if self.numeric_write(write,name,owner,component,writes)? { return Ok(NumericEffect::Unknown); }
        }
        Ok(NumericEffect::shift(0))
    }
    fn numeric_assignment_effect(&self, lhs: &Expr, op: &str, rhs: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>], absolute: bool) -> Result<NumericEffect> {
        let Some((ty,parts))=self.written_projection(lhs,name,owner,None,writes)? else { return Ok(NumericEffect::shift(0)); };
        if !parts.iter().any(|part|part.contains(&component)) { return Ok(NumericEffect::shift(0)); }
        // 未知地址可能写此分量；单个候选分量不表示该次写必然选中了它。
        if address_write(lhs){return Ok(NumericEffect::Unknown);}
        // 动态候选位置不被当作同一实际分量。
        if parts.iter().any(|part|part.contains(&component)&&part.len()!=1) { return Ok(NumericEffect::Unknown); }
        let relative=|base:&Expr|->Result<bool>{Ok(self.written_projection(base,name,owner,None,writes)?.is_some_and(|(source,selected)|source==ty&&selected==parts))};
        let (sign,amount)=match op {
            "+=" => (1,self.constant_index(rhs,writes)?),
            "-=" => (-1,self.constant_index(rhs,writes)?),
            "=" => match &rhs.kind {
                ExprKind::Binary(operation,base,amount) if matches!(operation.as_str(),"+"|"-")&&relative(base)? => (if operation=="+"{1}else{-1},self.constant_index(amount,writes)?),
                ExprKind::Binary(operation,amount,base) if operation=="+"&&relative(base)? => (1,self.constant_index(amount,writes)?),
                ExprKind::Binary(operation,amount,base) if operation=="-"&&relative(base)? =>
                    return Ok(self.constant_index(amount,writes)?.map(NumericEffect::Reflect).unwrap_or(NumericEffect::Unknown)),
                _ if relative(rhs)? => (1,Some(0)),
                _ => {
                    let value=if let Some(value)=self.constant_index(rhs,writes)? {bounds::Proof::unsigned(value,value)}
                    else if absolute && let ExprKind::Name(source)=&rhs.kind {
                        match self.env.get(source) {
                            Some(Binding::Value(value)) if value.ty==Ty::scalar(Scalar::U32)=>bounds::Proof::capture(value),
                            Some(Binding::Owned(value)) if value.place.ty()==&Ty::scalar(Scalar::U32)=>{
                                for write in writes {if self.numeric_write(write,source,&value.place,0,writes)? {return Ok(NumericEffect::Unknown);}}
                                value.proof.clone()
                            },
                            _=>return Ok(NumericEffect::Unknown),
                        }
                    } else {return Ok(NumericEffect::Unknown);};
                    return Ok(NumericEffect::Assign(value));
                },
            },
            _ => return Ok(NumericEffect::Unknown),
        };
        Ok(amount.map(|amount|NumericEffect::shift(i128::from(amount)*sign)).unwrap_or(NumericEffect::Unknown))
    }
    fn numeric_loop_paths<'e>(&self, body:&'e [Stmt], mut next:Option<NumericEffect>, continues:&mut Option<NumericEffect>, name:&str, owner:&Place, component:usize, writes:&[Write<'_>], absolute:bool, scope:&mut Scope<'_, '_, 'e>)->Result<Option<NumericEffect>> {
        let outer = scope.bindings.len();
        for statement in body {
            let Some(before)=next.take() else { break; };
            next=match &statement.kind {
                StmtKind::Assign(lhs,op,rhs)=>Some(before
                    .then(self.numeric_expression_effect(lhs,name,owner,component,writes)?)
                    .then(self.numeric_expression_effect(rhs,name,owner,component,writes)?)
                    .then(self.numeric_assignment_effect(lhs,op,rhs,name,owner,component,writes,absolute)?)),
                StmtKind::Let(ty,local,expr)=>{
                    let next=before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?);
                    scope.bindings.push((local,ty.as_ref().map_or(Local::Inferred(expr),Local::Typed)));
                    Some(next)
                },
                StmtKind::Declare(ty,local)=>{scope.bindings.push((local,Local::Typed(ty)));Some(before)},
                StmtKind::Expr(expr)=>Some(before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?)),
                StmtKind::If(condition,yes,no)=>{
                    let before=before.then(self.numeric_expression_effect(condition,name,owner,component,writes)?);
                    let truth=scope.writes_expression(condition,&mut |_|{})?;
                    let yes=if truth==Some(false){None}else{self.numeric_loop_paths(yes,Some(before.clone()),continues,name,owner,component,writes,absolute,scope)?};
                    let no=if truth==Some(true){None}else{self.numeric_loop_paths(no,Some(before),continues,name,owner,component,writes,absolute,scope)?};
                    join_numeric_paths(yes,no)
                },
                StmtKind::Match(value,arms)=>{
                    let before=before.then(self.numeric_expression_effect(value,name,owner,component,writes)?);let mut next=None;
                    for arm in arms {
                        if let Some(local)=&arm.binding{scope.bindings.push((local,Local::Payload(value,&arm.variant)));}
                        next=join_numeric_paths(next,self.numeric_loop_paths(&arm.body,Some(before.clone()),continues,name,owner,component,writes,absolute,scope)?);
                        if arm.binding.is_some(){scope.bindings.pop();}
                    }next
                },
                StmtKind::Seek(_,expr)=>{
                    let before=before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?);
                    *continues=join_numeric_paths(continues.take(),Some(before));None
                },
                StmtKind::Continue=>{*continues=join_numeric_paths(continues.take(),Some(before));None},
                StmtKind::Break|StmtKind::Return(_)=>None,
                StmtKind::While(..)|StmtKind::For(..)|StmtKind::Parallel(..)=>{
                    let mut effect=NumericEffect::shift(0);
                    for write in writes.iter().filter(|write|contains_statement(statement,match write {
                        Write::External{call,..}|Write::Memory{call,..}|Write::Generator(call)=>call,
                        Write::Position{call,place,..}=>call.unwrap_or(place),
                    })) {
                        if self.numeric_write(write,name,owner,component,writes)? {effect=NumericEffect::Unknown;break;}
                    }
                    Some(before.then(effect))
                },
                _=>Some(before),
            };
        }
        scope.bindings.truncate(outer);
        Ok(next)
    }
    pub(super) fn retain_loop_bounds(&mut self, condition:Option<&Expr>, body:&[Stmt], initial:&BTreeMap<String,Binding>, header:&mut BTreeMap<String,Binding>)->Result<()> {
        let mut writes=Vec::new();statement_writes(self,body,&mut|write|writes.push(write))?;if let Some(condition)=condition {expression_writes(self,condition,&mut|write|writes.push(write))?;}
        let writes=self.normalize_writes(writes)?;
        let mut retained=Vec::new();
        for (name,binding) in initial {
            let (owner,proof)=match binding { Binding::Owned(owner)=>(owner.place.clone(),&owner.proof),Binding::OwnedValue(owner)=>{let Some(place)=owner.place()else{continue};(place,&owner.value.proof)},_=>continue };
            // 这里只解释有确定位置身份的本地／独占参数；数据候选别名仍由原写效果撤回。
            if !matches!(owner.storage(),Place::Local(..)|Place::Reference{..}) {continue;}
            for (component,(scalar,_)) in owner.ty().components.iter().enumerate() {
                if *scalar!=Scalar::U32 {continue;}
                let mut written=false;for write in &writes {if self.numeric_write(write,name,&owner,component,&writes)? {written=true;break;}}
                if !written {continue;}
                if let Some(condition)=condition && matches!(self.numeric_expression_effect(condition,name,&owner,component,&writes)?,NumericEffect::Unknown) {continue;}
                let mut input=proof.project(&[component],owner.ty().components.len());
                if let Some(identity)=input.known_identity(0) {
                    let mut domains=input.domains(0);domains.insert(IndexDomain::Upper(identity,0));input=input.with_domains(domains);
                }
                let mut continues=None;let next=self.numeric_loop_paths(body,Some(NumericEffect::shift(0)),&mut continues,name,&owner,component,&writes,!bounds::seeks(body),&mut Scope::current(self))?;
                let (lower,upper,least,most)=match join_numeric_paths(next,continues) {
                    None=>(true,true,0,0),
                    Some(NumericEffect::Shift{low,high,least,most})=>(low>=0,high<=0,least,most),
                    Some(NumericEffect::Reflect(amount))=>{
                        let reflected=input.unsigned_reflected(amount);
                        let mut invariant=input;invariant.join(&reflected,1,None);
                        retained.push((name.clone(),component,invariant,true,true));
                        continue;
                    },
                    Some(NumericEffect::Assign(repeated))=>{
                        let mut invariant=input;invariant.join(&repeated,1,None);
                        retained.push((name.clone(),component,invariant,true,true));
                        continue;
                    },
                    Some(NumericEffect::Unknown)=>continue,
                };
                if !lower&&!upper {continue;}
                let guard=match self.env.get(name) {Some(Binding::Owned(value))=>&value.proof,Some(Binding::OwnedValue(value))=>&value.value.proof,_=>continue};
                let low=guard.unsigned_minimum(component).max(if lower{proof.unsigned_minimum(component)}else{0});
                let high=guard.unsigned_maximum(component).min(if upper{proof.unsigned_maximum(component)}else{u32::MAX});
                if i128::from(low)+least<0||i128::from(high)+most>i128::from(u32::MAX) {continue;}
                retained.push((name.clone(),component,input,lower,upper));
            }
        }
        for (name,component,proof,lower,upper) in retained {
            for environment in [&mut self.env,&mut *header] {if let Some(binding)=environment.get_mut(&name)&&let Some((current,_,_))=binding.proof_storage(){current.retain_loop_bounds(&proof,component,lower,upper);}}
        }
        Ok(())
    }
}
