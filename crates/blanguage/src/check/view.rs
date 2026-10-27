//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
// 循环引用只在原Proof中连接入口和实际回边；公共合同只编码闭合来源。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LoopInput { pub level: usize, pub index: usize }
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Borrow {
    pub sources: BTreeSet<Source>,
    pub inputs: BTreeSet<LoopInput>,
}
impl Borrow {
    pub fn from_sources(sources:BTreeSet<Source>)->Self {Self{sources,..Self::default()}}
    pub fn empty(&self)->bool {self.sources.is_empty()&&self.inputs.is_empty()}
    pub fn union(&mut self,other:&Self) {self.sources.extend(&other.sources);self.inputs.extend(&other.inputs);}
    pub fn joined(mut self,other:Self)->Self {self.union(&other);self}
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut result=Self{sources:BTreeSet::new(),inputs:self.inputs.clone()};
        for source in &self.sources {result.union(&source.at_call(bindings));} result
    }
    pub fn resolve(&self,level:usize,values:&[Self])->Self {
        let mut result=Self::from_sources(self.sources.clone());
        for input in &self.inputs {
            if input.level==level {result.union(&values[input.index]);}
            else {result.inputs.insert(*input);}
        }
        result
    }
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
pub(super) fn resolve_binding(binding:&mut Binding,level:usize,values:&[Borrow]) {
    if let Some(proof)=binding.view_proof_mut(){proof.resolve_view_loop(level,values);}
    match binding {
        Binding::ApplicationData(data)=>data.handle.proof.resolve_view_loop(level,values),
        Binding::OwnedValue(value)=>{
            fn resolve(value:&mut owned_values::OwnedValue,level:usize,values:&[Borrow]) {
                value.value.proof.resolve_view_loop(level,values);
                for item in value.payloads.iter_mut().flatten() {match item.as_mut() {
                    returns::Returned::Dataset(data)=>data.handle.proof.resolve_view_loop(level,values),
                    returns::Returned::OwnedValue(value)=>resolve(value,level,values),
                    returns::Returned::Value(value)=>value.proof.resolve_view_loop(level,values),_=>{},
                }}
                value.sync_resource_borrows();
            }
            resolve(value,level,values);
        },
        Binding::ViewElement{view,..}=>view.proof.resolve_view_loop(level,values),_=>{},
    }
}
pub(super) enum Requirement {
    Unborrowed {resource:Borrow,borrow:Borrow,span:Span},
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,span:Span},
}
impl Requirement {
    fn borrow(&self)->&Borrow {match self {Self::Unborrowed{borrow,..}|Self::Return{borrow,..}=>borrow}}
    pub fn resolve(&mut self,level:usize,values:&[Borrow]) {
        let borrow=match self {Self::Unborrowed{borrow,..}|Self::Return{borrow,..}=>borrow};
        *borrow=borrow.resolve(level,values);
        if let Self::Unborrowed{resource,..}=self {*resource=resource.resolve(level,values);}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Data(ValueIdentity),
    Parameter(ValueIdentity),
    Unresolved,
}
impl Source {
    pub(super) fn bind_outputs(self, bindings: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        match self {
            Self::Data(id) => Self::Data(bindings.get(&id).copied().unwrap_or(id)),
            Self::Parameter(id) => Self::Parameter(bindings.get(&id).copied().unwrap_or(id)),
            Self::Unresolved => self,
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
            Self::Unresolved=>Borrow::from_sources(BTreeSet::from([self])),
        }
    }
}
crate::module::codec::variants!(Source{0=>Data(id),1=>Parameter(id),2=>Unresolved});
impl Binding {
    pub(super) fn view_parts(&self)->Option<(&Ty,&bounds::Proof)> {
        match self {
            Self::Value(v) if v.ty.contains_views()=>Some((&v.ty,&v.proof)),
            Self::Owned(v) if v.place.ty().contains_views()=>Some((v.place.ty(),&v.proof)),
            _=>None,
        }
    }
    pub(super) fn view_proof_mut(&mut self)->Option<&mut bounds::Proof> {
        match self {Self::Value(v) if v.ty.contains_views()=>Some(&mut v.proof),Self::Owned(v) if v.place.ty().contains_views()=>Some(&mut v.proof),_=>None}
    }
    pub(super) fn view_borrow(&self)->Borrow {
        if let Self::ViewElement{view,..}=self {return view.proof.borrowed_view();}
        self.view_parts().map(|(_,proof)|proof.borrowed_view()).unwrap_or_default()
    }

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
        if borrow.sources.contains(&Source::Unresolved) {return self.fail(match &requirement{Requirement::Unborrowed{span,..}|Requirement::Return{span,..}=>*span},"B271","view的实际借用来源尚未形成合同");}
        if let Requirement::Unborrowed{resource,borrow,..}=&requirement {
            if resource.sources.is_empty() && borrow.sources.is_empty() && !resource.inputs.is_empty() && !borrow.inputs.is_empty() && resource.inputs.iter().all(|a|borrow.inputs.iter().all(|b| {
                a.level==b.level && a.index!=b.index && self.loop_contexts.get(a.level).is_some_and(|context|context.view_inputs[a.index].independent() && context.view_inputs[b.index].independent())
            })) {return Ok(());}
        }
        match &requirement {
            Requirement::Unborrowed{resource,borrow,span} if borrow.sources.iter().any(|source|matches!(source,Source::Data(_)) && resource.sources.contains(source)) => return self.fail(*span,"B227","dataset仍被实际view消费者借用，不能写入、移动、传输或释放"),
            Requirement::Return{borrow,allowed,span} if borrow.sources.iter().any(|s|matches!(s,Source::Data(id) if !allowed.contains(id))) => return self.fail(*span,"B227","返回view不能借用当前函数将释放的dataset"),
            _=>{},
        }
        if !borrow.inputs.is_empty() || matches!(&requirement,Requirement::Unborrowed{resource,..} if !resource.inputs.is_empty()) {self.loop_contexts.last_mut().unwrap().view_requirements.push(requirement);}
        Ok(())
    }
    pub(super) fn require_view_available(&self, value: &Value, span: Span) -> Result<()> {
        for source in value.proof.borrowed_views() {
            match source {
                Source::Data(id) => self.application_handle_available(id, span)?,
                Source::Unresolved => {
                    return self.fail(span, "B271", "view返回的实际借用来源尚未形成合同");
                }
                Source::Parameter(_) => {}
            }
        }
        Ok(())
    }
    pub(super) fn require_view_return(&mut self, value: &Value, span: Span) -> Result<()> {
        self.require_view_available(value, span)?;
        self.require_view_return_proof(&value.proof, span)
    }
    pub(super) fn require_view_return_proof(&mut self,proof:&bounds::Proof,span:Span)->Result<()> {
        let allowed=self.env.values().filter_map(|binding|match binding {
            Binding::ApplicationData(data) if data.ownership==application::Ownership::Borrowed=>Some(data.handle.identity()),_=>None,
        }).collect();
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
