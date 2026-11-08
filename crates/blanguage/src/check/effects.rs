//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;
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


enum Write<'e> {
    Position {
        place: &'e Expr,
        call: Option<&'e Expr>,
        contents: bool,
        iterator: Option<(&'e str, &'e Expr)>,
        when: Option<bounds::Condition>,
        local_value: bool,
    },
    External { call:&'e Expr, instance:Option<crate::generics::Instance>, arguments:BTreeMap<ValueIdentity,bounds::Proof>, syntax:&'e[Expr] },
    Memory { call:&'e Expr, borrow:view::Borrow },
    Generator(&'e Expr),
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
                Write::Memory{borrow,..}if self.address_write_positions(borrow).is_some()=>{self.write_address_values(&[(borrow.clone(),Some(Access::Write),None)])?;continue;},
                Write::External{..}|Write::Generator(_)|Write::Memory{..}=>{
                    let contents=self.env.values().flat_map(Binding::application_resources).map(|data|(
                        data.root.identity(),view::resource_borrow(data),data.ty.components.iter().map(|(_,word)|*word).collect::<Vec<_>>()
                    )).collect::<Vec<_>>();
                    for(source,resource,words)in contents{self.update_application_elements(&[source],&resource,&words,application::ElementUpdate::Unknown)?;}
                    continue;
                },
                Write::Position{place,contents:true,when,..}=>self.written_application_data(place).map(|(data,offset,ty)|(
                    vec![data.root.identity()],view::resource_borrow(data),ty.components.iter().map(|(_,word)|offset+word).collect::<Vec<_>>(),None,when.as_ref()
                )),
                Write::Position{place,contents:false,when,..}=>self.loop_write_projection(place,&|index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))?.map(|(projection,_,_)|(
                    projection.data.map(|data|data.root.identity()).into_iter().collect::<Vec<_>>(),projection.resource,
                    projection.value.1.into_iter().flatten().map(|i|projection.base.components[i].1).collect::<Vec<_>>(),
                    projection.index.map(|index|{let range=match index{bounds::Operand::Constant(_,value)=>(value as u32,value as u32),_ =>(0,u32::MAX)};(index,range)}),when.as_ref()
                )),
            };
            if let Some((sources,resource,words,index,when))=changed{self.update_application_elements(&sources,&resource,&words,application::ElementUpdate::Element{proof:&bounds::Proof::default(),index,when})?;}
        }
        let mut resources = Vec::new();
        for (name,binding) in &self.env {
            let Binding::ApplicationData(data) = binding else { continue; };
            if borrowed_data == Some(data.root.identity()) { continue; }
            if let Some(id) = data.root.proof.known_identity(0) && self.application_borrowed(id)? { continue; }
            if writes.iter().any(|write| matches!(write,
                Write::Position{contents:false,place,..} if matches!(&place.kind, ExprKind::Name(target) if target == name))) {
                resources.push(name.clone());
            }
        }
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.root.proof.forget_value(&[0],data.root.ty.components.len());
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
        &'a self,expr:&Expr,known_index:&impl Fn(&Expr,bool)->Result<(Option<bounds::Operand>,Option<bounds::Condition>)>,
    )->Result<Option<(DataProjection<'a>,&'a application::Data,Option<bounds::Condition>)>> {
        match &expr.kind {
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
                if let Some((data,offset,ty))=self.written_application_data(base) {
                    let components=ty.components.iter().map(|(scalar,word)|vec![data.ty.components.iter().position(|part|part==&(*scalar,offset+word)).unwrap()]).collect();
                    let(index,when)=known_index(index,true)?;
                    return Ok(Some((DataProjection{data:Some(data),resource:view::resource_borrow(data),base:data.ty.clone(),access:data.access,index,value:(ty.clone(),components)},data,when)));
                }
                let Some((mut projection,input,mut when))=self.loop_write_projection(base,known_index)? else{return Ok(None);};
                let (ty,components)=projection.value;
                if let Some(element)=&ty.dataset {
                    let mut resource=view::Borrow::default();
                    for fields in view::resource_paths(&projection.base) {
                        let (mut offset,mut field)=(0,&projection.base);
                        for at in &fields{let(word,next)=field.aggregate_part(*at);offset+=word;field=next;}
                        if field.components.iter().zip(&components).all(|((scalar,word),parts)|parts.iter().any(|i|projection.base.components[*i]==(*scalar,offset+word))) {
                            resource.union(&view::Borrow::element(projection.resource.clone(),fields,projection.index));
                        }
                    }
                    let element=self.world.ty(element,&self.definition.source,expr.span,&self.definition.module,&self.definition.imports)?;
                    let components=(0..element.components.len()).map(|i|vec![i]).collect();
                    let(index,selection)=known_index(index,true)?;
                    when=when.map(|when|when.scope_element_indices(1));
                    if let Some(selection)=selection{when=Some(match when{Some(previous)=>previous.logical(selection,false),None=>selection});}
                    projection.data=None;projection.resource=resource;projection.base=element.clone();projection.index=index;projection.value=(element,components);
                } else {
                    let Some(element)=&ty.array else{return Ok(None);};
                    let constant=match known_index(index,false)?.0{Some(bounds::Operand::Constant(Scalar::U32,value))=>Some(value as u32),_=>None};let width=element.components.len();
                    let elements=(0..ty.values[0] as usize).filter(|i|constant.is_none_or(|value|*i==value as usize)).collect::<Vec<_>>();
                    let selected=(0..width).map(|component|elements.iter().flat_map(|i|components[*i*width+component].iter().copied()).collect()).collect();
                    projection.value=((**element).clone(),selected);
                }
                Ok(Some((projection,input,when)))
            },
            ExprKind::Member(base,field)=>{
                let Some((mut projection,input,when))=self.loop_write_projection(base,known_index)? else{return Ok(None);};
                let(ty,selected)=member_type(&projection.value.0,field,&self.definition.source,expr.span)?;
                projection.value=(ty,selected.iter().map(|i|projection.value.1[*i].clone()).collect());
                Ok(Some((projection,input,when)))
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
                    let borrow=match self.native_call_memory(&instance.name,&arguments){Some(memory)=>memory.into_iter().fold(view::Borrow::default(),|mut result,(borrow,_,_)|{result.union(&borrow);result}),None=>view::Borrow::source(view::Source::Unresolved)};
                    if !borrow.active(){continue;}
                    Write::Memory{call,borrow}
                },
                write=>write,
            };
            normalized.push(write);
        }
        Ok(normalized)
    }
    fn project_loop_storage(&self,input:&application::Data,resource:&view::Borrow,index:Option<bounds::Operand>,parts:&[(Scalar,u32)])->Result<Option<Vec<StorageWrite>>> {
        let Some(locations)=self.application_resource_storage(input,resource)?else{return Ok(None);};
        let mut writes=Vec::new();
        for(location,ty,layout)in locations {
            match location {
                StorageWrite::Descendants{root}=>writes.push(StorageWrite::Descendants{root}),
                StorageWrite::Frame{root,..}=>{
                    if let(Some(bounds::Operand::Constant(Scalar::U32,index)),application::DataLayout::Declared)=(index,&layout){
                        let base=u128::from(index as u32)*u128::from(ir::align_up(ty.size,ty.align));
                        for(scalar,word)in parts{let start=base+u128::from(*word);writes.push(StorageWrite::Frame{root,range:Some(start..start+u128::from(scalar.size()))});}
                    }else{writes.push(StorageWrite::Frame{root,range:None});}
                },
            }
        }
        Ok(Some(writes))
    }
    fn project_loop_writes(&self,writes:&[Write<'_>])->Result<LoopWrites>{
        let mut effect=LoopWrites::default();
        for write in writes {
            match write {
                Write::External{..}|Write::Generator(_)=>{effect.unknown_storage=true;effect.external=true;},
                Write::Memory{borrow,..}=>{
                    let Some(positions)=self.address_write_positions(borrow)else{effect.unknown_storage=true;effect.external=true;continue;};
                    for(source,_)in borrow.sources.entries().filter(|(_,when)|when.possible()) {
                        let view::Source::Position(origin)=source else{continue;};
                        if !origin.writable{continue;}
                        match &origin.root {
                            address::Root::Resource{borrow,index}|address::Root::Dataset{borrow,index}=>{
                                let parts=origin.layout.parts.iter().zip(origin.words.iter()).map(|((scalar,_),word)|(*scalar,*word)).collect::<Vec<_>>();
                                let mut located=false;
                                for input in self.env.values().flat_map(Binding::application_resources){
                                    if let Some(storage)=self.project_loop_storage(input,borrow,*index,&parts)?{
                                        located=true;effect.storage.extend(storage);
                                    }
                                }
                                if !located{effect.unknown_storage=true;}
                            },
                            _=>if origin.resources().active(){effect.unknown_storage=true;},
                        }
                    }
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
                            effect.storage.push(StorageWrite::Frame{root:data.root.regs[0],range:None});
                        }else{effect.unknown_storage=true;}
                        continue;
                    }
                    if let Some((projection,input,_))=self.loop_write_projection(place,&|index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))? {
                        let parts=projection.value.1.iter().flatten().map(|component|projection.base.components[*component]).collect::<Vec<_>>();
                        match self.project_loop_storage(input,&projection.resource,projection.index,&parts)?{
                            Some(storage)=>effect.storage.extend(storage),
                            None=>effect.unknown_storage=true,
                        }
                        continue;
                    }
                    let Some(name)=assigned_root(place) else {effect.unknown_storage=true;continue;};
                    let Some(binding)=self.env.get(name) else {
                        if !matches!(place.kind,ExprKind::Name(_)){effect.unknown_storage=true;}
                        continue;
                    };
                    let owner=match binding {Binding::ApplicationData(data)=>data.place.clone(),_=>binding.stored_place()};
                    if let Some(owner)=owner
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
        statement_writes(self, body, &mut |place| {writes.push(place);Ok(())})?;
        if let Some(condition)=condition {
            expression_writes(self, condition, &mut |place| {writes.push(place);Ok(())})?;
        }
        let writes=self.normalize_writes(writes)?;
        let mut effect=self.project_loop_writes(&writes)?;
        let writes=if !effect.external && self.env.values().any(|binding|matches!(binding,Binding::Owned(owner) if *owner.place.ty()==Ty::scalar(Scalar::Bool))) {
            let mut selected=Vec::new();
            {
                let mut scope=Scope::current(self);scope.writes=Some(&effect);
                scope.writes_statements(body,&mut|write|{selected.push(write);Ok(())})?;
                if let Some(condition)=condition {scope.writes_expression(condition,&mut|write|{selected.push(write);Ok(())},None)?;}
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

    fn input_snapshot(&self,expr:&Expr,limit:usize,access:Access,element:Option<(&DataProjection<'_>,&bounds::Clause)>,ordinal:Option<&str>)->Result<Option<(Ty,bounds::Proof,view::Borrow)>> {
        let builder=self.builder.unwrap();
        if let ExprKind::Name(name)=&expr.kind && ordinal==Some(name.as_str()) {
            return Ok(Some((Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),view::Borrow::default())));
        }
        if let ExprKind::Name(name)=&expr.kind
            &&let Some(i)=self.bindings[..limit].iter().rposition(|(bound,_)|*bound==name)
            &&let Local::Initialized(None,value,_)=self.bindings[i].1
            &&!builder.mutable_names.contains(name) {return self.input_snapshot(value,i,access,element,ordinal);}
        match &expr.kind {
            ExprKind::Member(base,field)if field=="length"=>{
                if let Some((ty,proof,_))=self.input_snapshot(base,limit,Access::Value,element,ordinal)? && ty.dataset.is_some() {
                    return Ok(Some((Ty::scalar(Scalar::U32),proof.project(&[1],ty.components.len()),view::Borrow::default())));
                }
            },
            ExprKind::StaticU32(value)=>return Ok(Some((Ty::scalar(Scalar::U32),bounds::Proof::unsigned(*value,*value),view::Borrow::default()))),
            ExprKind::Number(number)=>{
                let(ty,value)=super::number(&self.definition.source,expr.span,number)?;
                return Ok((ty==Ty::scalar(Scalar::U32)).then(||(ty,bounds::Proof::unsigned(value as u32,value as u32),view::Borrow::default())));
            },
            ExprKind::Binary(op,a,b)if matches!(op.as_str(),"&&"|"||")=>{
                let condition=|value:Option<(Ty,bounds::Proof,view::Borrow)>|value.and_then(|(ty,proof,_)|(ty==Ty::scalar(Scalar::Bool)).then(||proof.component_condition(0).cloned()).flatten()).unwrap_or_default();
                let left=condition(self.input_snapshot(a,limit,Access::Value,element,ordinal)?);
                let right=condition(self.input_snapshot(b,limit,Access::Value,element,ordinal)?);
                let condition=left.logical(right,op=="||");
                return Ok((!condition.unconstrained()).then(||(Ty::scalar(Scalar::Bool),bounds::Proof::condition(condition),view::Borrow::default())));
            },
            ExprKind::Binary(op,a,b)if matches!(op.as_str(),"=="|"!="|"<"|"<="|">"|">=")=>{
                let(Some((left,a,_)),Some((right,b,_)))=(self.input_snapshot(a,limit,Access::Value,element,ordinal)?,self.input_snapshot(b,limit,Access::Value,element,ordinal)?)else{return Ok(None);};
                if left!=Ty::scalar(Scalar::U32)||right!=left{return Ok(None);}
                let operand=|proof:&bounds::Proof|proof.known_value(&left).map(|bits|bounds::Operand::Constant(Scalar::U32,bits[0])).or_else(||proof.known_identity(0).map(bounds::Operand::Value));
                let(Some(left),Some(right))=(operand(&a),operand(&b))else{return Ok(None);};
                let condition=match op.as_str(){
                    "<"=>bounds::Condition::index_below((left,&a),(right,&b),true),
                    "<="=>bounds::Condition::index_below((left,&a),(right,&b),false),
                    ">"=>bounds::Condition::index_below((right,&b),(left,&a),true),
                    ">="=>bounds::Condition::index_below((right,&b),(left,&a),false),
                    "!="=>bounds::Condition::index_equal(left,right).negate(),
                    _=>bounds::Condition::index_equal(left,right),
                };
                return Ok(Some((Ty::scalar(Scalar::Bool),bounds::Proof::condition(condition),view::Borrow::default())));
            },
            _=>{},
        }
        let Some(name)=assigned_root(expr)else{
            if let ExprKind::Bool(value)=expr.kind{return Ok(Some((Ty::scalar(Scalar::Bool),bounds::Proof::condition(bounds::Condition::constant(value)),view::Borrow::default())));}
            return Ok(None);
        };
        if self.bindings[..limit].iter().any(|(bound,_)|*bound==name){return Ok(None);}
        let Some(binding)=builder.env.get(name)else{return Ok(None);};
        let known_index=|index:&Expr| {
            if let ExprKind::Name(name)=&index.kind
                &&(self.bindings[..limit].iter().any(|(bound,_)|*bound==name)||!matches!(builder.env.get(name),Some(Binding::Value(_)))) {return Ok(None);}
            if let Some(index)=builder.constant_index(index,&[])?{return Ok(Some(bounds::Operand::Constant(Scalar::U32,Bits::from(index))));}
            Ok(match &index.kind{ExprKind::Name(name)=>match builder.env.get(name){Some(Binding::Value(value))if value.ty==Ty::scalar(Scalar::U32)=>Some(bounds::Operand::of(value,0)),_=>None},_=>None})
        };
        if matches!(binding,Binding::ApplicationData(_)|Binding::DataReference(_)|Binding::OwnedValue(_))
            && let Some((projection,input,_))=builder.loop_write_projection(expr,&|index,_|known_index(index).map(|index|(index,None)))? {
            let proof=match(element,self.writes){
                (Some((target,path)),Some(writes))=>builder.application_element_snapshot(&projection,input,target,(writes,path))?,
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
        let Some((selected,parts))=builder.component_projection(expr,name,&ty,owner.as_ref(),None,&|index|Ok(match known_index(index)?{Some(bounds::Operand::Constant(Scalar::U32,value))=>Some(value as u32),_=>None}))?else{return Ok(None);};
        let mut value=None::<bounds::Proof>;let mut borrow=view::Borrow::default();
        for choice in 0..parts.first().map_or(0,Vec::len) {
            let indices=parts.iter().map(|part|part[choice]).collect::<Vec<_>>();
            let projection=proof.project(&indices,ty.components.len());
            if let Some(value)=&mut value{value.join_condition(&projection,selected.components.len(),None);}else{value=Some(projection);}
            if access!=Access::Value && let Some(place)=&owner{borrow.union(&builder.position_origin(&place.clone().project(selected.clone(),&indices),expr.span)?.normalize());}
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
            let(proof,storage)=match self.input_snapshot(arg,self.bindings.len(),parameter.access,None,None)?{Some((_,proof,storage))=>(proof,storage),None=>(bounds::Proof::default().with_external_borrows(&ty),view::Borrow::source(view::Source::Unresolved))};
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
    fn function_writes(&self, ty: Ty, args: &'e [Expr], call: &'e Expr, writes: &mut dyn FnMut(Write<'e>)->Result<()>) -> Result<()> {
        let Some(function) = &ty.function else {
            return self.definition.source.fail(call.span, "B263", "间接调用目标需要cfn原生C签名值");
        };
        if outputs::external_memory(function.contracts.iter()).is_some_and(|access|access.writes()){
            writes(Write::External{call,instance:None,arguments:BTreeMap::new(),syntax:args})?;
        }
        for ((access, _), arg) in function.parameters.iter().zip(args) {
            if matches!(access, Access::Write | Access::InOut) {
                writes(Write::Position{ place: arg, call: Some(call), contents: false, iterator: None,when:None,local_value:false})?;
            }
        }
        Ok(())
    }
    fn writes_expression(&self, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)->Result<()>, element:Option<(&DataProjection<'_>,&bounds::Clause)>) -> Result<Option<bounds::Condition>> {
        let mut truth = None;
        match &expr.kind {
            ExprKind::Bool(value) => truth = Some(bounds::Condition::constant(*value)),
            ExprKind::Name(name) if self.builder.is_some_and(|builder|!builder.mutable_names.contains(name)) || self.writes.is_some() => {
                if let Some((ty,proof,_))=self.input_snapshot(expr,self.bindings.len(),Access::Value,element,None)?
                    && ty==Ty::scalar(Scalar::Bool) {
                    truth=proof.component_condition(0).cloned().or_else(||proof.known_value(&ty).map(|bits|bounds::Condition::constant(bits[0]!=0)));
                }
            }
            ExprKind::Invoke(callee, args) => {
                let ty = self.expression(callee)?.ok_or_else(|| Diagnostic {
                    source: self.definition.source.clone(), span: callee.span, code: "B263",
                    message: "不能从原声明与词法来源取得间接调用签名".into(),
                })?;
                self.function_writes(ty, args, expr, writes)?;
                self.writes_expression(callee,writes,element)?;
                for arg in args { self.writes_expression(arg,writes,element)?; }
            }
            ExprKind::Call(name, args) | ExprKind::Cooperate(name, _, args) | ExprKind::Specialize(name, _, args) => {
                if self.builder.is_none() && name == "address_of" && let [place] = args.as_slice() {
                    writes(Write::Position{ place, call: Some(expr), contents: false, iterator: None,when:None,local_value:false})?;
                }
                if name=="take_last" && self.named(name,expr.span)?.is_none() && let [_,place]=args.as_slice() {
                    writes(Write::Position{place,call:Some(expr),contents:false,iterator:None,when:None,local_value:false})?;
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
                                if !read_only {writes(Write::Position{ place: arg, call: Some(expr), contents, iterator: None,when:None,local_value:false})?;}
                            }
                        }
                        if !covered {let arguments=match &instance{Some(instance)=>self.call_inputs(&instance.definition,args)?,None=>BTreeMap::new()};writes(Write::External{call:expr,instance,arguments,syntax:args})?;}
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
                            Some((_,Local::Initialized(None,value,_)))if self.builder.is_some_and(|builder|!builder.mutable_names.contains(name))=>zero(value)?,
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
                        if !read_only {writes(Write::Generator(factory))?;}
                    }
                }
                for arg in args { self.writes_expression(arg,writes,element)?; }
            }
            ExprKind::Construct(_, args) => for arg in args { self.writes_expression(arg,writes,element)?; },
            ExprKind::Unary(op,base)if op=="!"=>truth=self.writes_expression(base,writes,element)?.map(bounds::Condition::negate),
            ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => { self.writes_expression(base,writes,element)?; }
            ExprKind::Index(a, b) => { self.writes_expression(a,writes,element)?; self.writes_expression(b,writes,element)?; }
            ExprKind::Binary(op, a, b) => {
                let left = self.writes_expression(a,writes,element)?;
                let skipped = matches!((op.as_str(), left.as_ref().and_then(bounds::Condition::boolean_constant)), ("&&", Some(false)) | ("||", Some(true)));
                let right = if self.builder.is_none() || !skipped { self.writes_expression(b,writes,element)? } else { None };
                truth = match op.as_str() {
                    "&&"|"||"=>{
                        let condition=left.unwrap_or_default().logical(right.unwrap_or_default(),op=="||");
                        (!condition.unconstrained()).then_some(condition)
                    },
                    "=="|"!="|"<"|"<="|">"|">=" if self.builder.is_some()=>self.input_snapshot(expr,self.bindings.len(),Access::Value,element,None)?.and_then(|(ty,proof,_)|(ty==Ty::scalar(Scalar::Bool)).then(||proof.component_condition(0).cloned()).flatten()),
                    _=>None,
                };
            }
            ExprKind::Select(a, b, c) => {
                let selected=self.writes_expression(a,writes,element)?.as_ref().and_then(bounds::Condition::boolean_constant);
                if self.builder.is_none() || selected!=Some(false){self.writes_expression(b,writes,element)?;}
                if self.builder.is_none() || selected!=Some(true){self.writes_expression(c,writes,element)?;}
            }
            _ => {}
        }
        Ok(truth)
    }
    fn writes_statements(&mut self, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)->Result<()>) -> Result<()> {
        let outer = self.bindings.len();
        for (statement_at,statement) in code.iter().enumerate() {
            match &statement.kind {
                StmtKind::Declare(ty, name) => self.bindings.push((name, Local::Typed(ty))),
                StmtKind::Let(ty, name, expr) => {
                    self.writes_expression(expr,writes,None)?;
                    self.bindings.push((name, Local::Initialized(ty.as_ref(),expr,&code[statement_at+1..])));
                }
                StmtKind::Assign(lhs, _, rhs) => {
                    writes(Write::Position{ place: lhs, call: None, contents: false, iterator: None,when:None,local_value:self.builder.is_some()&&self.local_value_write(lhs)?==Some(true)})?;
                    self.writes_expression(lhs,writes,None)?;
                    self.writes_expression(rhs,writes,None)?;
                }
                StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Seek(_, expr) => { self.writes_expression(expr,writes,None)?; }
                StmtKind::If(expr, yes, no) => {
                    let condition=self.writes_expression(expr,writes,None)?;
                    let known=condition.as_ref().and_then(bounds::Condition::boolean_constant);
                    for(truth,body)in [(true,yes),(false,no)]{
                        if known==Some(!truth){continue;}
                        if let Some(condition)=condition.as_ref().filter(|_|known.is_none()){
                            let condition=if truth{condition.clone()}else{condition.clone().negate()};
                            self.writes_statements(body,&mut|mut write|{
                                if let Write::Position{call:None,when,..}=&mut write{
                                    *when=Some(match when.take(){Some(previous)=>previous.logical(condition.clone(),false),None=>condition.clone()});
                                }
                                writes(write)
                            })?;
                        }else{self.writes_statements(body,writes)?;}
                    }
                }
                StmtKind::Match(value, arms) => {
                    self.writes_expression(value,writes,None)?;
                    for arm in arms {
                        if let Some(name) = &arm.binding { self.bindings.push((name, Local::Payload(value, &arm.variant))); }
                        self.writes_statements(&arm.body, writes)?;
                        if arm.binding.is_some() { self.bindings.pop(); }
                    }
                }
                StmtKind::While(expr, body) => {
                    let mut future=Vec::new();
                    self.writes_expression(expr,&mut|write|{future.push(write);Ok(())},None)?;
                    self.writes_statements(body,&mut|write|{future.push(write);Ok(())})?;
                    if let Some(builder)=self.builder {
                        let scope=Scope{bindings:self.bindings.clone(),writes:self.writes,..Scope::current(builder)};
                        let mut retained=Vec::new();
                        for(binding_at,(name,local))in scope.bindings.iter().enumerate(){
                            let Local::Initialized(_,initial,after)=local else{continue;};
                            if scope.named(name,expr.span)?!=Some(Ty::scalar(Scalar::U32)){continue;}
                            let Some((ty,input,_))=scope.input_snapshot(initial,binding_at,Access::Value,None,None)?else{continue;};
                            if ty!=Ty::scalar(Scalar::U32){continue;}
                            let target=builder.env.get(*name).and_then(Binding::stored_place).map_or(LoopPlace::Local,|place|LoopPlace::Stored(place,0));
                            let mut prefix_scope=Scope{bindings:scope.bindings[..=binding_at].to_vec(),writes:scope.writes,..Scope::current(builder)};
                            let mut prefix=None;
                            builder.numeric_loop_paths(*after,Some(NumericEffect::shift(0)),&mut None,name,&target,&future,false,&mut prefix_scope,&bounds::Clause::default(),Some(expr),&mut prefix)?;
                            if !matches!(prefix,Some(NumericEffect::Shift{low:0,high:0,..})){continue;}
                            let before=builder.numeric_expression_effect(expr,name,&target,&future)?;
                            if !matches!(before,NumericEffect::Shift{low:0,high:0,..}){continue;}
                            let guard=scope.input_snapshot(expr,scope.bindings.len(),Access::Value,None,Some(name))?
                                .and_then(|(_,proof,_)|proof.component_condition(0).cloned())
                                .map(|condition|bounds::scoped_proof(&Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),&condition.assume(&bounds::Clause::default(),true),&BTreeMap::new())).unwrap_or_default();
                            let mut numeric_scope=Scope{bindings:scope.bindings.clone(),writes:scope.writes,..Scope::current(builder)};
                            let mut continues=None;
                            let next=builder.numeric_loop_paths(body,Some(before),&mut continues,name,&target,&future,false,&mut numeric_scope,&bounds::Clause::default(),None,&mut None)?;
                            retained.push((*name,input,guard,join_numeric_paths(next,continues)));
                        }
                        let iterations=retained.iter().filter_map(|(_,input,guard,effect)|match effect{None=>Some(1),Some(effect)=>effect.iteration_bound(input,guard)}).min();
                        for at in 0..future.len(){
                            let selection=if let Write::Position{place,call:None,..}=&future[at]{
                                builder.loop_write_projection(place,&|index,dataset|{
                                    let operand=builder.loop_index_operand(index,&future)?;
                                    let ExprKind::Name(name)=&index.kind else{return Ok((operand,None));};
                                    if !dataset||scope.named(name,index.span)?!=Some(Ty::scalar(Scalar::U32)){return Ok((operand,None));}
                                    let mut condition=scope.input_snapshot(expr,scope.bindings.len(),Access::Value,None,Some(name))?.and_then(|(_,proof,_)|proof.component_condition(0).cloned());
                                    if iterations==Some(0){return Ok((operand,Some(bounds::Condition::constant(false))));}
                                    let range=iterations.and_then(|iterations|retained.iter().find(|(bound,_,_,_)|*bound==name.as_str()).and_then(|(_,input,_,effect)|match effect {
                                        None=>Some((input.unsigned_minimum(0),input.unsigned_maximum(0))),
                                        Some(effect)=>effect.bounded_ranges(input,iterations).map(|range|range.0),
                                    }));
                                    if let Some((low,high))=range{
                                        let ordinal=bounds::Operand::Value(ValueIdentity::ElementIndex(0));
                                        let proof=bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0));
                                        for(value,lower)in [(low,true),(high,false)]{
                                            let bound=bounds::Proof::unsigned(value,value);let operand=bounds::Operand::Constant(Scalar::U32,Bits::from(value));
                                            let limit=if lower{bounds::Condition::index_below((operand,&bound),(ordinal,&proof),false)}else{bounds::Condition::index_below((ordinal,&proof),(operand,&bound),false)};
                                            condition=Some(match condition{Some(previous)=>previous.logical(limit,false),None=>limit});
                                        }
                                    }
                                    let Some(condition)=condition else{return Ok((operand,None));};
                                    let target=builder.env.get(name).and_then(Binding::stored_place).map_or(LoopPlace::Local,|place|LoopPlace::Stored(place,0));
                                    let mut numeric_scope=Scope{bindings:scope.bindings.clone(),writes:scope.writes,..Scope::current(builder)};
                                    let mut observed=None;let before=builder.numeric_expression_effect(expr,name,&target,&future)?;
                                    builder.numeric_loop_paths(body,Some(before),&mut None,name,&target,&future,false,&mut numeric_scope,&bounds::Clause::default(),Some(place),&mut observed)?;
                                    let condition=match observed {
                                        None=>Some(bounds::Condition::constant(false)),
                                        Some(NumericEffect::Shift{low:0,high:0,..})=>Some(condition),
                                        _=>None,
                                    };
                                    Ok((operand,condition))
                                })?.and_then(|(_,_,selection)|selection)
                            }else{None};
                            if let(Some(selection),Write::Position{when,..})=(selection,&mut future[at]){
                                *when=Some(match when.take(){Some(previous)=>previous.logical(selection,false),None=>selection});
                            }
                        }
                    }
                    for write in future{writes(write)?;}
                }
                StmtKind::For(name, start, end, body) => {
                    self.writes_expression(start,writes,None)?;
                    if let Some(end) = end { self.writes_expression(end,writes,None)?; }
                    let mut range=None::<bounds::Condition>;
                    if self.builder.is_some() && let Some(end)=end {
                        let ordinal=bounds::Operand::Value(ValueIdentity::ElementIndex(0));
                        let proof=bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0));
                        for(expr,lower)in [(start,true),(end,false)] {
                            if lower&&bounds::seeks(body){continue;}
                            let Some((ty,bound,_))=self.input_snapshot(expr,self.bindings.len(),Access::Value,None,None)?else{continue;};
                            if ty!=Ty::scalar(Scalar::U32){continue;}
                            let operand=bound.known_value(&ty).map(|bits|bounds::Operand::Constant(Scalar::U32,bits[0])).or_else(||bound.known_identity(0).map(bounds::Operand::Value));
                            let Some(operand)=operand else{continue;};
                            let condition=if lower{bounds::Condition::index_below((operand,&bound),(ordinal,&proof),false)}
                                else{bounds::Condition::index_below((ordinal,&proof),(operand,&bound),true)};
                            range=Some(match range{Some(previous)=>previous.logical(condition,false),None=>condition});
                        }
                    }
                    let builder=self.builder;
                    self.bindings.push((name, if end.is_some() { Local::Range } else { Local::Element(start) }));
                    self.writes_statements(body, &mut |mut write| {
                        if let Write::Position{place,call:None,when,..}=&mut write
                            && let (Some(builder),Some(range))=(builder,range.as_ref()) {
                            if let Some((_,_,Some(selection)))=builder.loop_write_projection(place,&|index,dataset|Ok((None,
                                (dataset&&matches!(&index.kind,ExprKind::Name(bound)if bound==name)).then(||range.clone()))))? {
                                *when=Some(match when.take(){Some(previous)=>previous.logical(selection,false),None=>selection});
                            }
                        }
                        if let Write::Position{place,iterator,..}=&mut write
                            && end.is_none() && assigned_root(place)==Some(name) {*iterator=Some((name,start));}
                        writes(write)
                    })?;
                    self.bindings.pop();
                }
                StmtKind::Parallel(name, domain, body) => {
                    if let Some(count) = &domain.count { self.writes_expression(count,writes,None)?; }
                    if let Some(selection) = &domain.selection { self.writes_expression(selection,writes,None)?; }
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
fn expression_writes<'e>(builder: &Builder<'_>, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)->Result<()>) -> Result<()> {
    Scope::current(builder).writes_expression(expr,writes,None).map(|_| ())
}
fn statement_writes<'e>(builder: &Builder<'_>, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)->Result<()>) -> Result<()> {
    Scope::current(builder).writes_statements(code, writes)
}
pub(super) fn value_writes<'e>(builder: &Builder<'_>, body: &'e [Stmt]) -> Result<Vec<&'e Expr>> {
    let mut values = Vec::new();
    statement_writes(builder, body, &mut |write| {
        if let Write::Position{contents:false,iterator:None,place,..}=write {values.push(place);}
        Ok(())
    })?;
    Ok(values)
}
pub(super) fn expression_assignments(builder: &Builder<'_>, expr: &Expr, names: &mut BTreeSet<String>) -> Result<()> {
    expression_writes(builder, expr, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
        Ok(())
    })
}
pub(super) fn assigned_names(world: &World, definition: &Definition, code: &[Stmt], names: &mut BTreeSet<String>) -> Result<()> {
    Scope::declared(world, definition).writes_statements(code, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
        Ok(())
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
enum LoopPlace { Stored(Place, usize), Local, Length, Element(DataProjection<'static>, usize) }

#[derive(Clone)]
enum NumericEffect {
    Unknown,
    Assign(bounds::Proof),
    Reflect(u32),
    Shift { low: i128, high: i128, least: i128, most: i128 },
}
impl NumericEffect {

    fn iteration_bound(&self,input:&bounds::Proof,guard:&bounds::Proof)->Option<i128> {
        let Self::Shift{low,high,least,most}=self else{return None;};
        let(first,last)=(i128::from(input.unsigned_minimum(0)),i128::from(input.unsigned_maximum(0)));
        let(lower,upper)=(i128::from(guard.unsigned_minimum(0)),i128::from(guard.unsigned_maximum(0)));
        if last<lower||first>upper{return Some(0);}
        if lower.checked_add(*least)?<0||upper.checked_add(*most)?>i128::from(u32::MAX){return None;}
        if *high<0{Some((last-lower)/high.checked_neg()?+1)}
        else if *low>0{Some((upper-first)/low+1)}else{None}
    }
    fn bounded_ranges(&self,input:&bounds::Proof,iterations:i128)->Option<((u32,u32),(u32,u32))> {
        let Self::Shift{low,high,least,most}=self else{return None;};
        let(first,last)=(i128::from(input.unsigned_minimum(0)),i128::from(input.unsigned_maximum(0)));
        // A zero-iteration body has no reachable values; entry is also its sound overapproximation.
        let before=if iterations==0{0}else{iterations-1};
        let body_low=first.checked_add(before.checked_mul(*low)?.min(0))?;
        let body_high=last.checked_add(before.checked_mul(*high)?.max(0))?;
        if body_low.checked_add(*least)?<0||body_high.checked_add(*most)?>i128::from(u32::MAX){return None;}
        let header_low=first.checked_add(iterations.checked_mul(*low)?.min(0))?;
        let header_high=last.checked_add(iterations.checked_mul(*high)?.max(0))?;
        Some(((body_low as u32,body_high as u32),(header_low as u32,header_high as u32)))
    }

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
    fn loop_index_operand(&self,index:&Expr,writes:&[Write<'_>])->Result<Option<bounds::Operand>> {
        Ok(match &index.kind {
            ExprKind::Name(name)=>match self.env.get(name){
                Some(Binding::Value(value))if value.ty==Ty::scalar(Scalar::U32)=>Some(bounds::Operand::of(value,0)),
                _=>self.constant_index(index,writes)?.map(|value|bounds::Operand::Constant(Scalar::U32,Bits::from(value))),
            },
            ExprKind::StaticU32(_)|ExprKind::Number(_)=>self.constant_index(index,writes)?.map(|value|bounds::Operand::Constant(Scalar::U32,Bits::from(value))),
            _=>None,
        })
    }
    fn loop_element_proof(&mut self,target:&DataProjection<'_>,component:usize,environment:&BTreeMap<String,Binding>,path:&bounds::Clause)->Result<bounds::Proof> {
        let mut result:Option<bounds::Proof>=None;
        for binding in environment.values(){for original in binding.application_resources(){
            for(scope,complete,parents)in self.application_element_scopes(&[],&target.resource,original,(path,environment)){
                let mut data=original.clone();let mut available=true;
                for fields in scope {
                    let model=self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.definition.function.span)?;
                    let Some(next)=application::model_data(Some(&model),&fields)else{available=false;break;};data=next.clone();
                }
                let proof=if available&&complete&&data.ty==target.base {
                    let model=self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.definition.function.span)?;
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
    fn retain_loop_element(&mut self,target:&DataProjection<'_>,component:usize,when:&bounds::Condition,environment:&mut BTreeMap<String,Binding>,context:(&bounds::Clause,&BTreeMap<String,Binding>),update:&impl Fn(&Value)->bounds::Proof)->Result<()> {
        for name in environment.keys().cloned().collect::<Vec<_>>(){
            let mut binding=environment[&name].clone();
            for at in 0..application::binding_element_count(&binding){
                let Some(root)=application::binding_element_data_mut(&mut binding,at)else{continue;};
                for(scope,complete,parents)in self.application_element_scopes(&[],&target.resource,root,context){
                    if !complete{continue;}
                    let selected=parents.scope_element_indices(1).logical(target.index.map(|index|bounds::Condition::element_index(0,index)).unwrap_or_else(||bounds::Condition::constant(true)),false).logical(when.clone(),false);
                    let mut data=Some(&mut *root);
                    for fields in scope{data=match data{Some(data)=>{self.expand_current_element_facts(data)?;application::model_data_mut(data.elements.known_mut(),&fields)},None=>None};}
                    let Some(data)=data else{continue;};
                    if data.ty!=target.base{continue;}
                    self.expand_current_element_facts(data)?;
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
    fn numeric_write(&self, write: &Write<'_>, name: &str, target: &LoopPlace, writes: &[Write<'_>]) -> Result<bool> {
        if matches!(target,LoopPlace::Local) {
            return Ok(match write {
                Write::Position{place,..}=>assigned_root(place).is_none_or(|root|root==name)||address_write(place),
                _=>true,
            });
        }
        if let LoopPlace::Element(target,component)=target {
            fn overlaps(a:&view::Borrow,b:&view::Borrow,path:&bounds::Clause)->bool {
                a.unresolved()||b.unresolved()
                    ||a.sources.entries().any(|(source,when)|b.overlaps_source(source,&bounds::Clause::both(path,&when)))
                    ||a.inputs.entries().any(|(input,when)|bounds::Clause::both(path,&when).overlaps(&b.inputs.when(input)))
                    ||a.calls.entries().any(|(call,when)|bounds::Clause::both(path,&when).overlaps(&b.calls.when(call)))
            }
            fn ancestors(resource:&view::Borrow,write:&DataProjection<'_>,path:&bounds::Clause)->bool {
                resource.sources.entries().any(|(source,when)|{
                    let active=bounds::Clause::both(path,&when);
                    match source {
                        view::Source::Element{parent,fields,index}=>{
                            if overlaps(parent,&write.resource,&active)&&index.zip(write.index).is_none_or(|(a,b)|bounds::Condition::index_equal(a,b).assume(&active,true).possible()){
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
                            for binding in self.env.values(){for(path,data,_)in view::data_paths(binding){
                                if self.application_element_scopes(&[],&target.resource,data,(&active,&self.env)).is_empty(){continue;}
                                if data.ownership!=application::Ownership::Owned{return Ok(true);}
                                let descriptor=match binding{Binding::ApplicationData(data)=>data.place.clone(),_=>binding.stored_place()};
                                if let Some(mut descriptor)=descriptor{
                                    for field in path{
                                        if descriptor.ty().has_variants(){return Ok(true);}
                                        let(offset,ty)=descriptor.ty().aggregate_part(field);
                                        let components=ty.components.iter().map(|(_,at)|descriptor.ty().components.iter().position(|(_,word)|*word==offset+at).unwrap()).collect::<Vec<_>>();
                                        let ty=ty.clone();descriptor=descriptor.project(ty,&components);
                                    }
                                    if !self.address_write_components(&positions,&descriptor).is_empty(){return Ok(true);}
                                }
                                // Register descriptors have no local slots for this address to write.
                                matched=true;
                            }}
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
            if let Some((written,_,_))=self.loop_write_projection(place,&|index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))? {
                if ancestors(&target.resource,&written,&self.path){return Ok(true);}
                if !overlaps(&target.resource,&written.resource,&self.path){return Ok(false);}
                if target.index.zip(written.index).is_some_and(|(a,b)|!bounds::Condition::index_equal(a,b).assume(&self.path,true).possible()){return Ok(false);}
                if target.base!=written.base{return Ok(true);}
                let(scalar,word)=target.base.components[*component];
                return Ok(written.value.1.iter().flatten().any(|part|{let(other,at)=written.base.components[*part];word<at+other.size()&&at<word+scalar.size()}));
            }
            let Some(root)=assigned_root(place)else{return Ok(true);};let Some(binding)=self.env.get(root)else{return Ok(true);};let Some(owner)=binding.stored_place()else{return Ok(true);};
            let Some((_,parts))=self.written_projection(place,root,&owner,None,writes)?else{return Ok(true);};
            for data in binding.application_resources(){
                if !self.application_element_scopes(&[],&target.resource,data,(&self.path,&self.env)).is_empty()
                    && let Some(descriptor)=&data.place {
                    let changed=self.address_write_components(&self.positions(descriptor),&owner);
                    if parts.iter().flatten().any(|component|changed.contains(component)){return Ok(true);}
                }
            }
            let(scalar,word)=target.base.components[*component];let origin=address::Origin{root:address::Root::Resource{borrow:target.resource.clone(),index:target.index},layout:address::Layout{size:scalar.size(),align:scalar.size(),parts:vec![(scalar,0)].into()},base:address::Layout::of(&target.base),words:vec![word].into(),positions:Arc::from([]),writable:true,initialized:vec![false].into()};
            let borrow=view::Borrow::source(view::Source::Position(Arc::new(origin)));let changed=self.loop_memory_components(&borrow,&owner);
            return Ok(parts.iter().flatten().any(|component|changed.contains(component)));
        }
        let LoopPlace::Stored(owner,component)=target else {
            return Ok(matches!(write,Write::Position{contents:false,place,..}if matches!(&place.kind,ExprKind::Name(bound)if bound==name)));
        };
        let component=*component;
        if let Write::Memory{borrow,..}=write{return Ok(self.loop_memory_components(borrow,owner).contains(&component));}
        let Write::Position{place,contents,iterator,..}=write else{return Ok(true);};
        let projection = if *contents { self.application_write_projection(place, owner) }
            else { self.written_projection(place, name, owner, *iterator, writes)? };
        Ok(projection.is_some_and(|(_,parts)|parts.iter().any(|part|part.contains(&component))))
    }
    fn numeric_expression_effect(&self, expr: &Expr, name: &str, target: &LoopPlace, writes: &[Write<'_>]) -> Result<NumericEffect> {
        for write in writes.iter().filter(|write|match write {
            Write::External{call,..}|Write::Memory{call,..}|Write::Generator(call)=>contains_expression(expr,call),
            Write::Position{call,..}=>call.is_some_and(|call|contains_expression(expr,call)),
        }) {
            if self.numeric_write(write,name,target,writes)? { return Ok(NumericEffect::Unknown); }
        }
        Ok(NumericEffect::shift(0))
    }
    fn numeric_assigned_value(&self,rhs:&Expr,writes:&[Write<'_>],absolute:bool)->Result<NumericEffect> {
        let value=if let Some(value)=self.constant_index(rhs,writes)? {bounds::Proof::unsigned(value,value)}
        else if absolute && let ExprKind::Name(source)=&rhs.kind {
            match self.env.get(source) {
                Some(Binding::Value(value)) if value.ty==Ty::scalar(Scalar::U32)=>bounds::Proof::capture(value),
                Some(Binding::Owned(value)) if value.place.ty()==&Ty::scalar(Scalar::U32)=>{
                    for write in writes {if self.numeric_write(write,source,&LoopPlace::Stored(value.place.clone(),0),writes)? {return Ok(NumericEffect::Unknown);}}
                    value.proof.clone()
                },
                _=>return Ok(NumericEffect::Unknown),
            }
        } else {return Ok(NumericEffect::Unknown);};
        Ok(NumericEffect::Assign(value))
    }
    fn numeric_assignment_effect(&self, lhs: &Expr, op: &str, rhs: &Expr, name: &str, target: &LoopPlace, writes: &[Write<'_>], absolute: bool, scope:&Scope<'_, '_, '_>) -> Result<NumericEffect> {
        if matches!(target,LoopPlace::Element(..))&&let Some(local)=scope.local_value_write(lhs)? {
            return Ok(if local{NumericEffect::shift(0)}else{NumericEffect::Unknown});
        }
        if matches!(target,LoopPlace::Length) {
            if !matches!(&lhs.kind,ExprKind::Name(bound)if bound==name) || scope.bindings.iter().any(|(bound,_)|*bound==name) {return Ok(NumericEffect::shift(0));}
            if op!="=" {return Ok(NumericEffect::Unknown);}
            let length=match &rhs.kind {
                ExprKind::Call(function,args) if function=="resize" && args.len()==3 && scope.named(function,rhs.span)?.is_none()=>&args[1],
                ExprKind::Specialize(function,_,args) if function=="dataset" && !args.is_empty()=>&args[0],
                ExprKind::Name(source) if source==name=>return Ok(NumericEffect::shift(0)),
                _=>return Ok(NumericEffect::Unknown),
            };
            if matches!(&length.kind,ExprKind::Name(source)if scope.bindings.iter().any(|(bound,_)|bound==source)) {return Ok(NumericEffect::Unknown);}
            return self.numeric_assigned_value(length,writes,absolute);
        };
        let (ty,parts,component)=match target {
            LoopPlace::Local=>{
                if address_write(lhs){return Ok(NumericEffect::Unknown);}
                if assigned_root(lhs)!=Some(name){return Ok(NumericEffect::shift(0));}
                if !matches!(&lhs.kind,ExprKind::Name(bound)if bound==name)||scope.named(name,lhs.span)?!=Some(Ty::scalar(Scalar::U32)){return Ok(NumericEffect::Unknown);}
                (Ty::scalar(Scalar::U32),vec![vec![0]],0)
            },
            LoopPlace::Stored(owner,component)=>{
                let Some((ty,parts))=self.written_projection(lhs,name,owner,None,writes)?else{return Ok(NumericEffect::shift(0));};(ty,parts,*component)
            },
            LoopPlace::Element(expected,component)=>{
                let written=Write::Position{place:lhs,call:None,contents:false,iterator:None,when:None,local_value:false};
                if !self.numeric_write(&written,name,target,writes)?{return Ok(NumericEffect::shift(0));}
                let Some((actual,_,_))=self.loop_write_projection(lhs,&|index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))?else{return Ok(NumericEffect::Unknown);};
                if expected.base!=actual.base||expected.resource.restricted(&self.path)!=actual.resource.restricted(&self.path)||!expected.index.zip(actual.index).is_some_and(|(a,b)|bounds::Condition::index_equal(a,b).truth_in(&self.path)==Some(true)){return Ok(NumericEffect::Unknown);}
                (actual.value.0,actual.value.1,*component)
            },
            LoopPlace::Length=>return Ok(NumericEffect::Unknown),
        };
        if !parts.iter().any(|part|part.contains(&component)) { return Ok(NumericEffect::shift(0)); }
        // 未知地址可能写此分量；单个候选分量不表示该次写必然选中了它。
        if address_write(lhs){return Ok(NumericEffect::Unknown);}
        // 动态候选位置不被当作同一实际分量。
        if parts.iter().any(|part|part.contains(&component)&&part.len()!=1) { return Ok(NumericEffect::Unknown); }
        let relative=|base:&Expr|->Result<bool>{Ok(match target{
            LoopPlace::Local=>matches!(&base.kind,ExprKind::Name(bound)if bound==name),
            LoopPlace::Stored(owner,_)=>self.written_projection(base,name,owner,None,writes)?.is_some_and(|(source,selected)|source==ty&&selected==parts),
            LoopPlace::Element(expected,_)=>self.loop_write_projection(base,&|index,_|self.loop_index_operand(index,writes).map(|index|(index,None)))?.is_some_and(|(actual,_,_)|actual.base==expected.base&&actual.resource.restricted(&self.path)==expected.resource.restricted(&self.path)&&actual.index.zip(expected.index).is_some_and(|(a,b)|bounds::Condition::index_equal(a,b).truth_in(&self.path)==Some(true))&&actual.value.0==ty&&actual.value.1==parts),
            LoopPlace::Length=>false,
        })};
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
                    return self.numeric_assigned_value(rhs,writes,absolute);
                },
            },
            _ => return Ok(NumericEffect::Unknown),
        };
        Ok(amount.map(|amount|NumericEffect::shift(i128::from(amount)*sign)).unwrap_or(NumericEffect::Unknown))
    }
    fn numeric_loop_paths<'e>(&self, body:&'e [Stmt], mut next:Option<NumericEffect>, continues:&mut Option<NumericEffect>, name:&str, target:&LoopPlace, writes:&[Write<'_>], absolute:bool, scope:&mut Scope<'_, '_, 'e>, path:&bounds::Clause,point:Option<&Expr>,observed:&mut Option<NumericEffect>)->Result<Option<NumericEffect>> {
        let outer = scope.bindings.len();
        for (statement_at,statement) in body.iter().enumerate() {
            let Some(before)=next.take() else { break; };
            if let(Some(point),StmtKind::While(condition,_))=(point,&statement.kind) && std::ptr::eq(point,condition) {
                *observed=join_numeric_paths(observed.take(),Some(before));
                break;
            }
            next=match &statement.kind {
                StmtKind::Assign(lhs,op,rhs)=>{
                    let before=before.then(self.numeric_expression_effect(lhs,name,target,writes)?);
                    if point.is_some_and(|point|std::ptr::eq(lhs,point)){*observed=join_numeric_paths(observed.take(),Some(before.clone()));}
                    Some(before.then(self.numeric_expression_effect(rhs,name,target,writes)?)
                        .then(self.numeric_assignment_effect(lhs,op,rhs,name,target,writes,absolute,scope)?))
                },
                StmtKind::Let(ty,local,expr)=>{
                    let next=before.then(self.numeric_expression_effect(expr,name,target,writes)?);
                    scope.bindings.push((local,Local::Initialized(ty.as_ref(),expr,&body[statement_at+1..])));
                    Some(next)
                },
                StmtKind::Declare(ty,local)=>{scope.bindings.push((local,Local::Typed(ty)));Some(before)},
                StmtKind::Expr(expr)=>Some(before.then(self.numeric_expression_effect(expr,name,target,writes)?)),
                StmtKind::If(condition,yes,no)=>{
                    let before=before.then(self.numeric_expression_effect(condition,name,target,writes)?);
                    let condition=scope.writes_expression(condition,&mut |_|Ok(()),match target{LoopPlace::Element(target,_)=>Some((target,path)),_=>None})?.unwrap_or_default();
                    let truth=condition.truth_in(path);
                    let yes=if truth==Some(false){None}else{self.numeric_loop_paths(yes,Some(before.clone()),continues,name,target,writes,absolute,scope,&condition.assume(path,true),point,observed)?};
                    let no=if truth==Some(true){None}else{self.numeric_loop_paths(no,Some(before),continues,name,target,writes,absolute,scope,&condition.assume(path,false),point,observed)?};
                    join_numeric_paths(yes,no)
                },
                StmtKind::Match(value,arms)=>{
                    let before=before.then(self.numeric_expression_effect(value,name,target,writes)?);let mut next=None;
                    for arm in arms {
                        if let Some(local)=&arm.binding{scope.bindings.push((local,Local::Payload(value,&arm.variant)));}
                        next=join_numeric_paths(next,self.numeric_loop_paths(&arm.body,Some(before.clone()),continues,name,target,writes,absolute,scope,path,point,observed)?);
                        if arm.binding.is_some(){scope.bindings.pop();}
                    }next
                },
                StmtKind::Seek(_,expr)=>{
                    let before=before.then(self.numeric_expression_effect(expr,name,target,writes)?);
                    *continues=join_numeric_paths(continues.take(),Some(before));None
                },
                StmtKind::Continue=>{*continues=join_numeric_paths(continues.take(),Some(before));None},
                StmtKind::Break|StmtKind::Return(_)=>None,
                StmtKind::While(_,body)|StmtKind::For(_,_,_,body)|StmtKind::Parallel(_,_,body)=>{
                    let mut effect=NumericEffect::shift(0);
                    for write in writes.iter().filter(|write|contains_statement(statement,match write {
                        Write::External{call,..}|Write::Memory{call,..}|Write::Generator(call)=>call,
                        Write::Position{call,place,..}=>call.unwrap_or(place),
                    })) {
                        if self.numeric_write(write,name,target,writes)? {effect=NumericEffect::Unknown;break;}
                    }
                    if point.is_some_and(|point|contains_statement(statement,point)) {
                        if let StmtKind::For(local,start,end,_)=&statement.kind{scope.bindings.push((local,if end.is_some(){Local::Range}else{Local::Element(start)}));}
                        if let StmtKind::Parallel(local,domain,_)=&statement.kind{scope.bindings.push((local,Local::NamedElement(&domain.dataset)));}
                        self.numeric_loop_paths(body,Some(before.clone().then(effect.clone())),&mut None,name,target,writes,absolute,scope,path,point,observed)?;
                        if !matches!(statement.kind,StmtKind::While(..)){scope.bindings.pop();}
                    }
                    Some(before.then(effect))
                },
                _=>Some(before),
            };
        }
        scope.bindings.truncate(outer);
        Ok(next)
    }
    pub(super) fn retain_loop_bounds(&mut self, condition:Option<&Expr>, body:&[Stmt], initial:&BTreeMap<String,Binding>, header:&mut BTreeMap<String,Binding>)->Result<bool> {
        let mut writes=Vec::new();statement_writes(self,body,&mut|write|{writes.push(write);Ok(())})?;if let Some(condition)=condition {expression_writes(self,condition,&mut|write|{writes.push(write);Ok(())})?;}
        let places=writes.iter().filter_map(|write|match write{Write::Position{place,contents:false,iterator:None,..}=>Some(*place),_=>None}).collect::<Vec<_>>();
        let writes=self.normalize_writes(writes)?;
        let mut elements=Vec::<DataProjection<'static>>::new();
        for place in places{
            if let Some((projection,_,_))=self.loop_write_projection(place,&|index,_|self.loop_index_operand(index,&writes).map(|index|(index,None)))?{
                elements.push(DataProjection{data:None,resource:projection.resource,base:projection.base,access:projection.access,index:projection.index,value:projection.value});
            }
        }
        let context=self.loop_contexts.last().unwrap();let entry_path=context.entry.path.clone();
        let empty=context.control_source.test.as_ref().unwrap().at_call(&context.control_source.bindings(&context.entry)).negate();
        let mut candidates=Vec::new();
        for(name,binding)in initial {
            let length;let(owner,proof)=match binding {
                Binding::Owned(owner)=>(Some(owner.place.clone()),&owner.proof),
                Binding::OwnedValue(owner)=>{let Some(place)=owner.place()else{continue};(Some(place),&owner.value.proof)},
                Binding::ApplicationData(data)=>{length=bounds::Proof::capture(&data.length);(None,&length)},_=>continue,
            };
            let components=match &owner{Some(owner)if matches!(owner.storage(),Place::Local(..)|Place::Reference{..})=>owner.ty().components.as_slice(),Some(_)=>continue,None=>&[(Scalar::U32,0)]};
            let guard=match self.env.get(name){Some(Binding::Owned(value))=>&value.proof,Some(Binding::OwnedValue(value))=>&value.value.proof,Some(Binding::ApplicationData(data))=>&data.length.proof,_=>continue};
            for(component,(scalar,_))in components.iter().enumerate(){if *scalar==Scalar::U32{
                let target=match &owner{Some(owner)=>LoopPlace::Stored(owner.clone(),component),None=>LoopPlace::Length};
                candidates.push((name.clone(),target,proof.project(&[component],components.len()),guard.project(&[component],components.len())));
            }}
        }
        for projection in elements {
            for component in projection.value.1.iter().flatten().copied().collect::<BTreeSet<_>>(){
                if candidates.iter().any(|(_,target,_,_)|matches!(target,LoopPlace::Element(other,at)if *at==component&&other.base==projection.base&&other.resource==projection.resource&&other.index==projection.index)){continue;}
                let input=self.loop_element_proof(&projection,component,initial,&entry_path)?;
                let guard=self.loop_element_proof(&projection,component,header,&self.path.clone())?;
                let selected=DataProjection{data:None,resource:projection.resource.clone(),base:projection.base.clone(),access:projection.access,index:projection.index,value:projection.value.clone()};
                candidates.push((String::new(),LoopPlace::Element(selected,component),input,guard));
            }
        }
        let mut retained=Vec::new();
        for(name,target,mut input,guard)in candidates {
            let mut written=false;for write in &writes{if self.numeric_write(write,&name,&target,&writes)?{written=true;break;}}
            if !written{continue;}
            if let Some(condition)=condition&&matches!(self.numeric_expression_effect(condition,&name,&target,&writes)?,NumericEffect::Unknown){continue;}
            if let LoopPlace::Element(element,component)=&target {
                self.retain_loop_element(element,*component,&empty,header,(&entry_path,initial),&|_|input.clone())?;
                if element.base.components[*component].0!=Scalar::U32{continue;}
            }
            if let Some(identity)=input.known_identity(0){let mut domains=input.domains(0);domains.insert(IndexDomain::Upper(identity,0));input=input.with_domains(domains);}
            let mut scope=Scope::current(self);scope.writes=Some(&self.loop_contexts.last().unwrap().writes);
            let mut continues=None;let next=self.numeric_loop_paths(body,Some(NumericEffect::shift(0)),&mut continues,&name,&target,&writes,!bounds::seeks(body),&mut scope,&bounds::Clause::default(),None,&mut None)?;
            retained.push((name,target,input,guard,join_numeric_paths(next,continues)));
        }
        let iterations=retained.iter().filter_map(|(_,_,input,guard,effect)|effect.as_ref().and_then(|effect|effect.iteration_bound(input,guard))).min();
        for(name,target,mut proof,guard,effect)in retained {
            let bounded=iterations.and_then(|iterations|effect.as_ref()?.bounded_ranges(&proof,iterations));
            let(lower,upper)=match effect {
                None=>(true,true),
                Some(NumericEffect::Shift{low,high,least,most})=>{
                    let(lower,upper)=(low>=0,high<=0);
                    let low=guard.unsigned_minimum(0).max(if lower{proof.unsigned_minimum(0)}else{0});
                    let high=guard.unsigned_maximum(0).min(if upper{proof.unsigned_maximum(0)}else{u32::MAX});
                    if bounded.is_some()||i128::from(low)+least>=0&&i128::from(high)+most<=i128::from(u32::MAX){(lower,upper)}else{(false,false)}
                },
                Some(NumericEffect::Reflect(amount))=>{let reflected=proof.unsigned_reflected(amount);proof.join(&reflected,1,None);(true,true)},
                Some(NumericEffect::Assign(repeated))=>{proof.join(&repeated,1,None);(true,true)},
                Some(NumericEffect::Unknown)=>continue,
            };
            if !lower&&!upper&&bounded.is_none(){continue;}
            if let LoopPlace::Element(target,component)=target {
                if target.index.is_none(){continue;}
                let retain=|previous:&Value,range:Option<(u32,u32)>|{let mut value=previous.clone();value.proof.retain_loop_bounds(&proof,0,lower,upper);if let Some((low,high))=range{value.proof.retain_loop_bounds(&bounds::Proof::unsigned(low,high),0,true,true);}bounds::Proof::capture(&value)};
                let mut environment=self.env.clone();self.retain_loop_element(&target,component,&bounds::Condition::constant(true),&mut environment,(&entry_path,initial),&|value|retain(value,bounded.map(|ranges|ranges.0)))?;self.env=environment;
                self.retain_loop_element(&target,component,&bounds::Condition::constant(true),header,(&entry_path,initial),&|value|retain(value,bounded.map(|ranges|ranges.1)))?;
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
        Ok(iterations!=Some(0))
    }
}
