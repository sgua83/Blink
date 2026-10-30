//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
// 循环引用只在原Proof中连接入口和实际回边；公共合同只编码闭合来源。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LoopInput { pub level: usize, pub index: usize }
// 同一来源边携带实际成立条件；不复制资源或建立运行时拥有状态。
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Edges<K:Ord>(BTreeMap<K,bounds::Condition>);
impl<K:Ord> Default for Edges<K>{fn default()->Self{Self(BTreeMap::new())}}
impl<K:Ord+Copy> Edges<K>{
    pub fn insert(&mut self,key:K){self.insert_when(key,bounds::Clause::default());}
    pub fn insert_when(&mut self,key:K,when:bounds::Clause){
        // 不可能边仍表明来源已知；空图表示尚无来源关系，两者不能混为一谈。
        // 新来源边直接持有原条件图；同一来源实际合流时才规整。
        self.0.entry(key).and_modify(|old|*old=bounds::Condition::guard(bounds::Clause::either(&old.assume(&bounds::Clause::default(),true),&when).canonical())).or_insert_with(||bounds::Condition::guard(when));
    }
    pub fn union(&mut self,other:&Self){for(key,when)in &other.0{self.insert_when(*key,when.assume(&bounds::Clause::default(),true));}}
    pub fn contains(&self,key:&K)->bool{self.0.get(key).is_some_and(bounds::Condition::active)}
    pub fn is_empty(&self)->bool{self.0.is_empty()}
    pub fn iter(&self)->std::collections::btree_map::Keys<'_,K,bounds::Condition>{self.0.keys()}
    pub fn entries(&self)->impl Iterator<Item=(&K,bounds::Clause)>{self.0.iter().map(|(key,when)|(key,when.assume(&bounds::Clause::default(),true)))}
    pub fn into_entries(self)->impl Iterator<Item=(K,bounds::Clause)>{self.0.into_iter().map(|(key,when)|(key,when.assume(&bounds::Clause::default(),true)))}
    pub fn when(&self,key:&K)->bounds::Clause{self.0.get(key).map(|when|when.assume(&bounds::Clause::default(),true)).unwrap_or_else(bounds::Clause::impossible)}
    fn restricted(&self,path:&bounds::Clause)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(*key,when.assume(path,true));}result}
    fn guards_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(*key,when.guard_at_call(bindings).assume(&bounds::Clause::default(),true));}result}
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(*key,when.output_bound(bindings).assume(&bounds::Clause::default(),true));}result}
}
impl<K:Ord+Copy,const N:usize> From<[K;N]> for Edges<K>{fn from(values:[K;N])->Self{values.into_iter().collect()}}
impl<K:Ord+Copy> FromIterator<K> for Edges<K>{fn from_iter<T:IntoIterator<Item=K>>(values:T)->Self{let mut result=Self::default();for value in values{result.insert(value);}result}}
impl<'a,K:Ord> IntoIterator for &'a Edges<K>{type Item=&'a K;type IntoIter=std::collections::btree_map::Keys<'a,K,bounds::Condition>;fn into_iter(self)->Self::IntoIter{self.0.keys()}}
impl<K:Ord> IntoIterator for Edges<K>{type Item=K;type IntoIter=std::collections::btree_map::IntoKeys<K,bounds::Condition>;fn into_iter(self)->Self::IntoIter{self.0.into_keys()}}
impl<K:Ord+Copy+crate::module::codec::Encode> crate::module::codec::Encode for Edges<K>{
    fn encode(&self,w:&mut crate::module::codec::Writer){crate::module::codec::Encode::encode(&self.0,w);}
}
impl<K:Ord+Copy+crate::module::codec::Decode> crate::module::codec::Decode for Edges<K>{
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{let values=<BTreeMap<K,bounds::Condition> as crate::module::codec::Decode>::decode(r)?;let mut result=Self::default();for(key,when)in values{result.insert_when(key,when.assume(&bounds::Clause::default(),true));}Ok(result)}
}
#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Borrow{
    pub sources:Edges<Source>,
    pub inputs:Edges<LoopInput>,
    pub calls:Edges<view_contracts::CallResult>,
}
impl Borrow{
    pub fn from_sources(sources:impl IntoIterator<Item=Source>)->Self{Self{sources:sources.into_iter().collect(),..Self::default()}}
    pub fn source(source:Source)->Self{Self::from_sources([source])}
    pub fn empty(&self)->bool{self.sources.is_empty()&&self.inputs.is_empty()&&self.calls.is_empty()}
    pub fn union(&mut self,other:&Self){self.sources.union(&other.sources);self.inputs.union(&other.inputs);self.calls.union(&other.calls);}
    pub fn joined(mut self,other:Self)->Self{self.union(&other);self}
    pub fn pending(&self)->bool{!self.inputs.is_empty()||!self.calls.is_empty()}
    pub fn active(&self)->bool{self.sources.entries().any(|(_,when)|when.possible())||self.inputs.entries().any(|(_,when)|when.possible())||self.calls.entries().any(|(_,when)|when.possible())}
    pub fn restricted(&self,path:&bounds::Clause)->Self{Self{sources:self.sources.restricted(path),inputs:self.inputs.restricted(path),calls:self.calls.restricted(path)}}
    pub fn selected(choice:&bounds::Condition,a:&Self,b:&Self)->Self{
        // 完整借用相同，选支不改变来源；不生产随后无人需要的路径组合。
        if a==b{return a.clone();}
        a.restricted(&choice.assume(&bounds::Clause::default(),true)).joined(b.restricted(&choice.assume(&bounds::Clause::default(),false)))
    }
    pub fn guards_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{Self{sources:self.sources.guards_at_call(bindings),inputs:self.inputs.guards_at_call(bindings),calls:self.calls.guards_at_call(bindings)}}
    pub fn map_sources(&self,map:impl Fn(Source)->Self)->Self{let mut result=Self{inputs:self.inputs.clone(),calls:self.calls.clone(),..Self::default()};for(source,when)in self.sources.entries(){result.union(&map(*source).restricted(&when));}result}
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{self.guards_at_call(bindings).map_sources(|source|source.at_call(bindings))}
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{
        let mut result=Self{inputs:self.inputs.output_bound(bindings),calls:self.calls.output_bound(bindings),..Self::default()};
        for(source,when)in self.sources.entries(){result.sources.insert_when(source.bind_outputs(bindings),when.output_bound(bindings));}result
    }
}
impl crate::module::codec::Encode for Borrow{fn encode(&self,w:&mut crate::module::codec::Writer){crate::module::codec::Encode::encode(&self.sources,w);}}
impl crate::module::codec::Decode for Borrow{fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{Ok(Self{sources:crate::module::codec::Decode::decode(r)?,..Self::default()})}}
pub(super) fn resource_borrow(data:&application::Data)->Borrow{
    let mut value=data.handle.proof.view_borrow(0);if value.empty(){value.sources.insert(Source::Data(data.handle.identity()));}value
}
pub(super) fn resource_proof(data:&application::Data)->bounds::Proof{data.handle.proof.clone().with_view_borrow(0,resource_borrow(data))}

pub(super) enum InputSlot {
    Value {name:String,component:usize},
    Data {name:String,path:Vec<usize>,independent:bool},
}
impl InputSlot {
    pub fn independent(&self)->bool {matches!(self,Self::Data{independent:true,..})}
}
pub(super) fn data_paths(binding:&Binding)->Vec<(Vec<usize>,&application::Data,bool)> {
    fn visit<'a>(value:&'a owned_values::OwnedValue,path:&mut Vec<usize>,independent:bool,result:&mut Vec<(Vec<usize>,&'a application::Data,bool)>) {
        if value.ownership==application::Ownership::Moved{return;}
        let independent=independent && !value.value.ty.has_variants();
        for (index,payload) in value.payloads.iter().enumerate() {
            let Some(payload)=payload.as_deref() else{continue};path.push(index);
            match payload {
                returns::Returned::Dataset(data) if data.ownership!=application::Ownership::Moved=>result.push((path.clone(),data,independent && data.ownership==application::Ownership::Owned)),
                returns::Returned::OwnedValue(child)=>visit(child,path,independent,result),_=>{},
            }
            path.pop();
        }
    }
    let mut result=Vec::new();
    match binding {
        Binding::ApplicationData(data) if data.ownership!=application::Ownership::Moved=>result.push((Vec::new(),data,data.ownership==application::Ownership::Owned)),
        Binding::OwnedValue(value)=>visit(value,&mut Vec::new(),true,&mut result),_=>{},
    }
    result
}
pub(super) fn data_at<'a>(binding:&'a Binding,path:&[usize])->Option<&'a application::Data> {
    data_paths(binding).into_iter().find_map(|(at,data,_)|(at==path).then_some(data))
}
pub(super) fn data_at_mut<'a>(binding:&'a mut Binding,path:&[usize])->Option<&'a mut application::Data> {
    match binding {
        Binding::ApplicationData(data) if path.is_empty()=>Some(data),
        Binding::OwnedValue(value)=>{
            let (first,rest)=path.split_first()?;
            let mut item=value.payloads.get_mut(*first)?.as_deref_mut()?;
            for index in rest {let returns::Returned::OwnedValue(value)=item else{return None};item=value.payloads.get_mut(*index)?.as_deref_mut()?;}
            if let returns::Returned::Dataset(data)=item {Some(data)}else{None}
        }
        _=>None,
    }
}
#[derive(Clone)]
pub(super) enum Requirement {
    Unborrowed {resource:Borrow,borrow:Borrow,span:Span},
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,owned:Borrow,span:Span},
}
impl Requirement {
    pub fn borrow(&self)->&Borrow {match self {Self::Unborrowed{borrow,..}|Self::Return{borrow,..}=>borrow}}
    pub fn pending(&self)->bool {self.borrow().pending()||matches!(self,Self::Unborrowed{resource,..}if resource.pending())||matches!(self,Self::Return{owned,..}if owned.pending())}
    pub fn under_path(&self,path:&bounds::Clause)->Self{match self{
        Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:borrow.restricted(path),allowed:allowed.clone(),owned:owned.clone(),span:*span},
        Self::Unborrowed{..}=>self.mapped(|borrow|borrow.restricted(path)),
    }}
    pub fn mapped(&self,map:impl Fn(&Borrow)->Borrow)->Self {
        match self {
            Self::Unborrowed{resource,borrow,span}=>Self::Unborrowed{resource:map(resource),borrow:map(borrow),span:*span},
            Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:map(borrow),allowed:allowed.clone(),owned:map(owned),span:*span},
        }
    }
    pub fn check(&self,source:&crate::Source)->Result<()> {
        if self.borrow().sources.contains(&Source::Unresolved)||matches!(self,Self::Unborrowed{resource,borrow,..}if !borrow.empty()&&resource.sources.contains(&Source::Unresolved)){return source.fail(match self{Self::Unborrowed{span,..}|Self::Return{span,..}=>*span},"B271","view的实际借用来源尚未形成合同");}
        match self {
            Self::Unborrowed{resource,borrow,span} if borrow.sources.entries().any(|(source,when)|matches!(source,Source::Data(_))&&when.overlaps(&resource.sources.when(source)))=>source.fail(*span,"B227","dataset仍被实际view消费者借用，不能写入、移动、传输或释放"),
            Self::Return{borrow,allowed,owned,span} if !owned.pending()&&borrow.sources.entries().any(|(source,when)|matches!(source,Source::Data(id)if !allowed.contains(id)&&when.overlaps(&bounds::Condition::guard(owned.sources.when(source)).assume(&bounds::Clause::default(),false))))=>source.fail(*span,"B227","返回view不能借用当前函数将释放的dataset"),
            _=>Ok(()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Data(ValueIdentity),
    Parameter(ValueIdentity),
    Unresolved,
    // 实际变体分支在此槽没有view；区别于尚未知的来源。
    Inactive,
}
impl Source {
    pub(super) fn bind_outputs(self, bindings: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        match self {
            Self::Data(id) => Self::Data(bindings.get(&id).copied().unwrap_or(id)),
            Self::Parameter(id) => Self::Parameter(bindings.get(&id).copied().unwrap_or(id)),
            Self::Unresolved | Self::Inactive => self,
        }
    }
    pub(super) fn at_call(self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow {
        match self {
            Self::Parameter(id)=>bindings.get(&id).map(|p|p.view_borrow(0)).unwrap_or_else(||Borrow::from_sources(BTreeSet::from([Self::Unresolved]))),
            Self::Data(id)=>{
                let Some(proof)=bindings.get(&id) else {return Borrow::from_sources(BTreeSet::from([Self::Unresolved]));};
                let value=proof.view_borrow(0);
                if !value.empty(){value}else{Borrow::from_sources(BTreeSet::from([proof.known_identity(0).map(Self::Data).unwrap_or(Self::Unresolved)]))}
            }
            Self::Unresolved|Self::Inactive=>Borrow::from_sources(BTreeSet::from([self])),
        }
    }
}
crate::module::codec::variants!(Source{0=>Data(id),1=>Parameter(id),2=>Unresolved,3=>Inactive});
impl Binding {
    pub(super) fn view_parts(&self)->Option<(&Ty,&bounds::Proof)> {
        match self {
            Self::Value(v) if v.ty.contains_views()=>Some((&v.ty,&v.proof)),
            Self::Owned(v) if v.place.ty().contains_views()=>Some((v.place.ty(),&v.proof)),
            Self::OwnedValue(v) if v.value.ty.contains_views()=>Some((&v.value.ty,&v.value.proof)),
            _=>None,
        }
    }
    pub(super) fn view_proof_mut(&mut self)->Option<&mut bounds::Proof> {
        match self {Self::Value(v) if v.ty.contains_views()=>Some(&mut v.proof),Self::Owned(v) if v.place.ty().contains_views()=>Some(&mut v.proof),Self::OwnedValue(v) if v.value.ty.contains_views()=>Some(&mut v.value.proof),_=>None}
    }
    pub(super) fn view_borrow(&self)->Borrow {
        if let Self::OwnedValue(value)=self && value.ownership==application::Ownership::Borrowed {
            let mut borrow=value.value.proof.typed_view_borrow(&value.value.ty);
            for data in value.resources(){borrow.union(&data.handle.proof.view_borrow(0));}return borrow;
        }
        if let Self::ViewElement{view,..}=self {return view.proof.borrowed_view();}
        self.view_parts().map(|(ty,proof)|proof.typed_view_borrow(ty)).unwrap_or_default()
    }

}
pub(super) fn resource_paths(ty:&Ty)->Vec<Vec<usize>> {
    fn visit(ty:&Ty,path:&mut Vec<usize>,result:&mut Vec<Vec<usize>>) {
        if ty.dataset.is_some(){result.push(path.clone());return;}
        if ty.has_variants(){for(index,variant)in ty.variants.iter().enumerate(){path.push(index);visit(&variant.ty,path,result);path.pop();}}
        else{for index in 0..ty.aggregate_len(){let(_,field)=ty.aggregate_part(index);path.push(index);visit(field,path,result);path.pop();}}
    }
    let mut result=Vec::new();visit(ty,&mut Vec::new(),&mut result);result
}
pub(super) fn owned_data_at<'a>(value:&'a owned_values::OwnedValue,path:&[usize])->Option<&'a application::Data> {
    let(first,rest)=path.split_first()?;
    match value.payloads.get(*first)?.as_deref()? {returns::Returned::Dataset(data)if rest.is_empty()=>Some(data),returns::Returned::OwnedValue(child)=>owned_data_at(child,rest),_=>None}
}
pub(super) fn owned_data_at_mut<'a>(value:&'a mut owned_values::OwnedValue,path:&[usize])->Option<&'a mut application::Data>{
    let(first,rest)=path.split_first()?;
    match value.payloads.get_mut(*first)?.as_deref_mut()?{returns::Returned::Dataset(data)if rest.is_empty()=>Some(data),returns::Returned::OwnedValue(child)=>owned_data_at_mut(child,rest),_=>None}
}
impl Builder<'_> {
    fn mapped_data_borrow(&self,identity:ValueIdentity)->Option<Borrow> {
        self.loop_contexts.iter().rev().find_map(|context|context.view_resources.get(&identity).cloned())
    }
    pub(super) fn data_borrow(&self,identities:&[ValueIdentity])->Borrow {
        let mut result=Borrow::default();
        for identity in identities {
            if let Some(borrow)=self.mapped_data_borrow(*identity){result.union(&borrow);continue;}
            let mut value=Borrow::default();
            for binding in self.env.values() {for (_,data,_) in data_paths(binding) {
                if data.handle.identity()==*identity {value.union(&data.handle.proof.view_borrow(0));}
            }}
            if value.empty(){value.sources.insert(Source::Data(*identity));}result.union(&value);
        }
        result
    }
    pub(super) fn data_value_borrow(&self,data:&application::Data)->Borrow {resource_borrow(data)}
    pub(super) fn application_view_borrowed(&self, identity: ValueIdentity) -> bool {
        self.env
            .iter()
            .any(|(name,b)| self.live_view_sources(name,b).contains(&Source::Data(identity)))
    }
    pub(super) fn require_view_unborrowed(&mut self,sources:&[ValueIdentity],span:Span)->Result<()> {
        let mut borrow=Borrow::default();
        for (name,binding) in &self.env {borrow.union(&self.live_view_borrow(name,binding));}
        for identity in sources {self.check_view_requirement(Requirement::Unborrowed{resource:self.data_borrow(&[*identity]),borrow:borrow.clone(),span})?;}
        Ok(())
    }
    pub(super) fn check_view_requirement(&mut self,requirement:Requirement)->Result<()> {
        let borrow=requirement.borrow();
        if borrow.empty(){return Ok(());}
        requirement.under_path(&self.path).check(&self.definition.source)?;
        if requirement.pending()||matches!(&requirement,Requirement::Unborrowed{..}){self.view_model.uses.push(view_contracts::Use{requirement,source:self.definition.source.clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect()});}
        Ok(())
    }
    pub(super) fn require_view_available(&self, value: &Value, span: Span) -> Result<()> {
        let borrow=value.proof.typed_view_borrow(&value.ty).restricted(&self.path);
        for(source,when)in borrow.sources.entries(){
            if !when.possible(){continue;}
            match *source {
                Source::Data(id) => self.application_handle_available(id, span)?,
                Source::Unresolved => {
                    return self.fail(span, "B271", "view返回的实际借用来源尚未形成合同");
                }
                Source::Parameter(_) | Source::Inactive => {}
            }
        }
        Ok(())
    }
    pub(super) fn require_view_return_proof(&mut self,proof:&bounds::Proof,owned:Borrow,span:Span)->Result<()> {
        let mut allowed=self.env.values().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Borrowed).flat_map(|data|std::iter::once(data.handle.identity()).chain(data.handle.proof.borrowed_views().into_iter().filter_map(|source|match source {Source::Data(id)=>Some(id),_=>None}))).collect::<BTreeSet<_>>();
        for param in &self.definition.function.params {if matches!(param.access,Access::Write|Access::InOut) && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for data in value.resources(){if data.ownership!=application::Ownership::Moved{allowed.insert(data.handle.identity());allowed.extend(data.handle.proof.borrowed_views().into_iter().filter_map(|source|match source{Source::Data(id)=>Some(id),_=>None}));}}}}
        for(index,param)in self.definition.function.params.iter().enumerate(){if param.access==Access::Read && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for resource in 0..resource_paths(&value.value.ty).len(){allowed.insert(ValueIdentity::ResourceParameter(index,resource));}}}
        self.check_view_requirement(Requirement::Return{borrow:proof.borrowed_view(),allowed,owned,span})
    }
    pub(super) fn create_application_view(
        &mut self,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        if self.application.is_none() || explicit.len() != 1 || args.len() != 1 {
            return self.fail(span, "B271", "CPU应用以view<T>(dataset)借用现有存储");
        }
        let GenericArgument::Type(element) = &explicit[0] else {
            return self.fail(span, "B271", "view<T>需要元素类型");
        };
        let mut name = element.clone();
        name.view = true;
        let ty = self.world.ty(
            &name,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        let Some(data) = self.application_reference(&args[0])? else {
            return self.fail(span, "B271", "view借用需要具有实际所有者的dataset位置");
        };
        self.require_same(ty.view.as_deref().unwrap(), &data.ty, span)?;
        self.require_application_initialized(&data, span)?;
        self.application_cpu(&data, span)?;
        let address =
            self.application_address(data.handle.regs[0], data.handle.identity(), false, span)?;
        let mut proof = bounds::Proof::default();
        let pointer = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::U32, 2),
            regs: address.to_vec(),
            uniform: false,
            constant: None,
        };
        proof.append(&pointer);
        proof.append(&data.length);
        proof = proof.with_view_borrow(0, self.data_borrow(&data.sources));
        Ok(Some(Value {
            proof,
            literal: false,
            ty,
            regs: vec![address[0], address[1], data.length.regs[0]],
            uniform: false,
            constant: None,
        }))
    }
}
impl Builder<'_> {
    pub(super) fn view_length(view: &Value) -> Value {
        Value {
            proof: view.projected_proof(&[2]).with_domains(Domains::from([
                IndexDomain::ViewLength(view.view_identity()),
            ])),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![view.regs[2]],
            uniform: view.uniform,
            constant: None,
        }
    }
    pub(super) fn view_element(view: &Value, index: Reg) -> Place {
        let ty = view.ty.view.as_deref().unwrap().clone();
        let stride = ir::align_up(ty.size, ty.align);
        let words = ty.components.iter().map(|(_, offset)| *offset).collect();
        Place::ViewElement {
            ty,
            address: [view.regs[0], view.regs[1]],
            index,
            stride,
            words,
        }
    }
    pub(super) fn view_place(&mut self, base: &Expr, index: &Expr) -> Result<Option<Place>> {
        let view = if self.is_place(base) {
            let Some(place) = self.place(base)? else {
                return Ok(None);
            };
            if place.ty().array.is_some() {
                return self.array_place(place, index);
            }
            self.read_place(&place, base.span)?
        } else {
            let Some(value) = self.expr(base)? else {
                return Ok(None);
            };
            value
        };
        if view.ty.array.is_some() {
            let place = self.materialize_array(view).place;
            return self.array_place(place, index);
        }
        self.view_value_place(view, index)
    }
    pub(super) fn view_value_place(&mut self, view: Value, index: &Expr) -> Result<Option<Place>> {
        if view.ty.view.is_none() {
            return self.fail(index.span, "B136", "索引需要 dataset、tile 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = self.expr(index);
        self.env.remove(&held);
        let Some(value) = evaluated? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        if !self.index_in(
            &value,
            IndexDomain::View(view.view_identity()),
            Self::view_length(&view).minimum(),
        ) {
            return self.fail(
                index.span,
                "B137",
                "view 索引有效性无法静态证明；使用该引用值的长度条件或有界游标",
            );
        }
        Ok(Some(Self::view_element(&view, value.regs[0])))
    }
}
