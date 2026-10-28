//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
// 循环引用只在原Proof中连接入口和实际回边；公共合同只编码闭合来源。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LoopInput { pub level: usize, pub index: usize }
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Borrow {
    pub sources: BTreeSet<Source>,
    pub inputs: BTreeSet<LoopInput>,
    pub calls: BTreeSet<view_contracts::CallResult>,
}
impl Borrow {
    pub fn from_sources(sources:BTreeSet<Source>)->Self {Self{sources,..Self::default()}}
    pub fn empty(&self)->bool {self.sources.is_empty()&&self.inputs.is_empty()&&self.calls.is_empty()}
    pub fn union(&mut self,other:&Self) {self.sources.extend(&other.sources);self.inputs.extend(&other.inputs);self.calls.extend(&other.calls);}
    pub fn joined(mut self,other:Self)->Self {self.union(&other);self}
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut result=Self{sources:BTreeSet::new(),inputs:self.inputs.clone(),calls:self.calls.clone()};
        for source in &self.sources {result.union(&source.at_call(bindings));} result
    }
    pub fn pending(&self)->bool {!self.inputs.is_empty()||!self.calls.is_empty()}
}
impl crate::module::codec::Encode for Borrow {
    fn encode(&self,w:&mut crate::module::codec::Writer) {crate::module::codec::Encode::encode(&self.sources,w);}
}
impl crate::module::codec::Decode for Borrow {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self> {
        Ok(Self::from_sources(crate::module::codec::Decode::decode(r)?))
    }
}
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
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,span:Span},
}
impl Requirement {
    pub fn borrow(&self)->&Borrow {match self {Self::Unborrowed{borrow,..}|Self::Return{borrow,..}=>borrow}}
    pub fn pending(&self)->bool {self.borrow().pending()||matches!(self,Self::Unborrowed{resource,..}if resource.pending())}
    pub fn mapped(&self,map:impl Fn(&Borrow)->Borrow)->Self {
        match self {
            Self::Unborrowed{resource,borrow,span}=>Self::Unborrowed{resource:map(resource),borrow:map(borrow),span:*span},
            Self::Return{borrow,allowed,span}=>Self::Return{borrow:map(borrow),allowed:allowed.clone(),span:*span},
        }
    }
    pub fn check(&self,source:&crate::Source)->Result<()> {
        if self.borrow().sources.contains(&Source::Unresolved){return source.fail(match self{Self::Unborrowed{span,..}|Self::Return{span,..}=>*span},"B271","view的实际借用来源尚未形成合同");}
        match self {
            Self::Unborrowed{resource,borrow,span} if borrow.sources.iter().any(|source|matches!(source,Source::Data(_))&&resource.sources.contains(source))=>source.fail(*span,"B227","dataset仍被实际view消费者借用，不能写入、移动、传输或释放"),
            Self::Return{borrow,allowed,span} if borrow.sources.iter().any(|source|matches!(source,Source::Data(id)if !allowed.contains(id)))=>source.fail(*span,"B227","返回view不能借用当前函数将释放的dataset"),
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
    pub(super) fn data_value_borrow(&self,data:&application::Data)->Borrow {
        self.mapped_data_borrow(data.handle.identity()).unwrap_or_else(|| {
            let mut borrow=data.handle.proof.view_borrow(0);
            if borrow.empty(){borrow.sources.insert(Source::Data(data.handle.identity()));}borrow
        })
    }
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
        if let Requirement::Unborrowed{resource,borrow,..}=&requirement {
            if resource.calls.is_empty() && borrow.calls.is_empty() && resource.sources.is_empty() && borrow.sources.is_empty() && !resource.inputs.is_empty() && !borrow.inputs.is_empty() && resource.inputs.iter().all(|a|borrow.inputs.iter().all(|b| {
                a.level==b.level && a.index!=b.index && self.loop_contexts.iter().find(|context|context.view_scope==Some(a.level)).is_some_and(|context|context.view_inputs[a.index].independent() && context.view_inputs[b.index].independent())
            })) {return Ok(());}
        }
        requirement.check(&self.definition.source)?;
        if requirement.pending(){self.view_model.uses.push(view_contracts::Use{requirement,source:self.definition.source.clone()});}
        Ok(())
    }
    pub(super) fn require_view_available(&self, value: &Value, span: Span) -> Result<()> {
        for source in value.proof.typed_view_borrow(&value.ty).sources {
            match source {
                Source::Data(id) => self.application_handle_available(id, span)?,
                Source::Unresolved => {
                    return self.fail(span, "B271", "view返回的实际借用来源尚未形成合同");
                }
                Source::Parameter(_) | Source::Inactive => {}
            }
        }
        Ok(())
    }
    pub(super) fn require_view_return(&mut self, value: &Value, span: Span) -> Result<()> {
        self.require_view_available(value, span)?;
        let mut proof=value.proof.borrow_state();proof.retain_view_borrows(&value.ty);
        self.require_view_return_proof(&proof, span)
    }
    pub(super) fn require_view_return_proof(&mut self,proof:&bounds::Proof,span:Span)->Result<()> {
        let mut allowed=self.env.values().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Borrowed).flat_map(|data|std::iter::once(data.handle.identity()).chain(data.handle.proof.borrowed_views().into_iter().filter_map(|source|match source {Source::Data(id)=>Some(id),_=>None}))).collect::<BTreeSet<_>>();
        for param in &self.definition.function.params {if matches!(param.access,Access::Write|Access::InOut) && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for data in value.resources(){if data.ownership!=application::Ownership::Moved{allowed.insert(data.handle.identity());allowed.extend(data.handle.proof.borrowed_views().into_iter().filter_map(|source|match source{Source::Data(id)=>Some(id),_=>None}));}}}}
        for(index,param)in self.definition.function.params.iter().enumerate(){if param.access==Access::Read && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for resource in 0..resource_paths(&value.value.ty).len(){allowed.insert(ValueIdentity::ResourceParameter(index,resource));}}}
        self.check_view_requirement(Requirement::Return{borrow:proof.borrowed_view(),allowed,span})
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
        let words = ty.components.iter().map(|(_, offset)| offset / 4).collect();
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
            let place = self.materialize_array(view);
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
