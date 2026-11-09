//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
// 循环引用只在原Proof中连接入口和实际回边；公共合同只编码闭合来源。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LoopInput { pub level: usize, pub index: usize }
// 同一来源边携带实际成立条件；不复制资源或建立运行时拥有状态。
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Edges<K:Ord>(BTreeMap<K,bounds::Condition>);
impl<K:Ord> Default for Edges<K>{fn default()->Self{Self(BTreeMap::new())}}
impl<K:Ord+Clone> Edges<K>{
    fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut result=Self::default();
        for(key,when)in &self.0{result.insert_when(key.clone(),when.substitute_values(values).assume(&bounds::Clause::default(),true));}
        result
    }

    pub fn insert(&mut self,key:K){self.insert_when(key,bounds::Clause::default());}
    pub fn insert_when(&mut self,key:K,when:bounds::Clause){
        // 不可能边仍表明来源已知；空图表示尚无来源关系，两者不能混为一谈。
        // 新来源边直接持有原条件图；同一来源实际合流时才规整。
        self.0.entry(key).and_modify(|old|*old=bounds::Condition::guard(bounds::Clause::either(&old.assume(&bounds::Clause::default(),true),&when).canonical())).or_insert_with(||bounds::Condition::guard(when));
    }
    pub fn union(&mut self,other:&Self){for(key,when)in &other.0{self.insert_when(key.clone(),when.assume(&bounds::Clause::default(),true));}}
    pub fn remove(&mut self,key:&K){self.0.remove(key);}
    pub fn contains(&self,key:&K)->bool{self.0.get(key).is_some_and(bounds::Condition::active)}
    pub fn is_empty(&self)->bool{self.0.is_empty()}
    pub fn iter(&self)->std::collections::btree_map::Keys<'_,K,bounds::Condition>{self.0.keys()}
    pub fn entries(&self)->impl Iterator<Item=(&K,bounds::Clause)>{self.0.iter().map(|(key,when)|(key,when.assume(&bounds::Clause::default(),true)))}
    pub fn into_entries(self)->impl Iterator<Item=(K,bounds::Clause)>{self.0.into_iter().map(|(key,when)|(key,when.assume(&bounds::Clause::default(),true)))}
    pub fn when(&self,key:&K)->bounds::Clause{self.0.get(key).map(|when|when.assume(&bounds::Clause::default(),true)).unwrap_or_else(bounds::Clause::impossible)}
    fn restricted(&self,path:&bounds::Clause)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(key.clone(),when.assume(path,true));}result}
    fn guards_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(key.clone(),when.guard_at_call(bindings).assume(&bounds::Clause::default(),true));}result}
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{let mut result=Self::default();for(key,when)in &self.0{result.insert_when(key.clone(),when.output_bound(bindings).assume(&bounds::Clause::default(),true));}result}
}
impl<K:Ord+Clone,const N:usize> From<[K;N]> for Edges<K>{fn from(values:[K;N])->Self{values.into_iter().collect()}}
impl<K:Ord+Clone> FromIterator<K> for Edges<K>{fn from_iter<T:IntoIterator<Item=K>>(values:T)->Self{let mut result=Self::default();for value in values{result.insert(value);}result}}
impl<'a,K:Ord> IntoIterator for &'a Edges<K>{type Item=&'a K;type IntoIter=std::collections::btree_map::Keys<'a,K,bounds::Condition>;fn into_iter(self)->Self::IntoIter{self.0.keys()}}
impl<K:Ord> IntoIterator for Edges<K>{type Item=K;type IntoIter=std::collections::btree_map::IntoKeys<K,bounds::Condition>;fn into_iter(self)->Self::IntoIter{self.0.into_keys()}}
impl<K:Ord+Clone+crate::module::codec::Encode> crate::module::codec::Encode for Edges<K>{
    fn encode(&self,w:&mut crate::module::codec::Writer){crate::module::codec::Encode::encode(&self.0,w);}
}
impl<K:Ord+Clone+crate::module::codec::Decode> crate::module::codec::Decode for Edges<K>{
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{let values=<BTreeMap<K,bounds::Condition> as crate::module::codec::Decode>::decode(r)?;let mut result=Self::default();for(key,when)in values{result.insert_when(key,when.assume(&bounds::Clause::default(),true));}Ok(result)}
}
#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Borrow{
    pub sources:Edges<Source>,
    pub inputs:Edges<LoopInput>,
    pub calls:Edges<view_contracts::CallResult>,
}
impl Borrow{
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut result=Self{inputs:self.inputs.substitute_values(values),calls:self.calls.substitute_values(values),..Self::default()};
        for(source,when)in self.sources.entries(){
            let next=match source {
                Source::Position(origin)=>origin.substitute_values(values).normalize(),
                Source::Element{parent,fields,index}=>Self::element(parent.substitute_values(values),fields.clone(),Source::index_substitute(*index,values)),
                Source::Projection{parent,fields}=>Self::projection(parent.substitute_values(values),fields.clone()),
                Source::Parameter(id,indices)=>{
                    let next=Source::Parameter(*id,indices.iter().map(|index|Source::index_substitute(*index,values)).collect());
                    if values.contains_key(id){next.at_call(values)}else{Self::source(next)}
                },
                Source::Data(id)if values.contains_key(id)=>source.clone().at_call(values),
                source=>Self::source(source.clone()),
            };
            result.union(&next.restricted(&when.substitute_values(values)));
        }
        result
    }

    pub fn from_sources(sources:impl IntoIterator<Item=Source>)->Self{Self{sources:sources.into_iter().collect(),..Self::default()}}
    pub fn source(source:Source)->Self{Self::from_sources([source])}
    pub fn element(parent:Self,fields:Vec<usize>,index:Option<bounds::Operand>)->Self {
        let mut result=Self::default();
        for(source,when)in parent.sources.entries(){
            let value=match source {
                Source::Projection{parent,fields:prefix}=>Self::element((**parent).clone(),prefix.iter().chain(&fields).copied().collect(),index),
                _=>Self::source(Source::Element{parent:Box::new(Self::source(source.clone())),fields:fields.clone(),index}),
            };
            result.union(&value.restricted(&when));
        }
        if !parent.inputs.is_empty()||!parent.calls.is_empty(){
            result.sources.insert(Source::Element{parent:Box::new(Self{inputs:parent.inputs,calls:parent.calls,..Self::default()}),fields,index});
        }
        result
    }
    // This projects a common input family; existing caller identities are not rebound.
    pub(super) fn select_elements(&self,indices:&[Option<bounds::Operand>])->Self {
        if indices.is_empty(){return self.clone();}
        let values=indices.iter().enumerate().map(|(at,index)|{
            let id=ValueIdentity::ElementIndex(indices.len()-1-at);
            let proof=match index{Some(index)=>bounds::Proof::element_operand(*index,(0,u32::MAX)),None=>bounds::Proof::default()};
            (id,proof)
        }).collect::<BTreeMap<_,_>>();
        let mut out=Self{inputs:self.inputs.substitute_values(&values),calls:self.calls.substitute_values(&values),..Self::default()};
        for(source,when)in self.sources.entries(){
            let value=match source{
                Source::Element{parent,fields,index}=>Self::element(parent.select_elements(indices),fields.clone(),Source::index_substitute(*index,&values)),
                Source::Parameter(id,selected)=>{
                    let selected=if matches!(id,ValueIdentity::DatasetContent(..))&&selected.is_empty(){indices.to_vec()}
                        else{selected.iter().map(|index|Source::index_substitute(*index,&values)).collect()};
                    Self::source(Source::Parameter(*id,selected))
                },
                Source::Projection{parent,fields}=>Self::projection(parent.select_elements(indices),fields.clone()),
                Source::Position(origin)=>origin.map_borrows(|borrow|borrow.select_elements(indices)).substitute_values(&values).normalize(),
                source=>Self::source(source.clone()),
            };
            out.union(&value.restricted(&when.substitute_values(&values)));
        }
        out
    }
    pub(super) fn overlaps_source(&self,source:&Source,when:&bounds::Clause)->bool {
        self.sources.entries().any(|(other,active)|{
            let path=bounds::Clause::both(when,&active);
            path.possible()&&source.overlaps(other,&path)
        })
    }
    pub fn projection(parent:Self,fields:Vec<usize>)->Self {
        if fields.is_empty(){return parent;}
        let mut result=Self::default();
        for(source,when)in parent.sources.entries(){
            let projected=match source {
                Source::Projection{parent,fields:prefix}=>Source::Projection{parent:parent.clone(),fields:prefix.iter().chain(&fields).copied().collect()},
                _=>Source::Projection{parent:Box::new(Self::source(source.clone())),fields:fields.clone()},
            };
            result.sources.insert_when(projected,when);
        }
        if !parent.inputs.is_empty()||!parent.calls.is_empty(){
            result.sources.insert(Source::Projection{parent:Box::new(Self{inputs:parent.inputs,calls:parent.calls,..Self::default()}),fields});
        }
        result
    }
    pub fn empty(&self)->bool{self.sources.is_empty()&&self.inputs.is_empty()&&self.calls.is_empty()}
    pub fn union(&mut self,other:&Self){self.sources.union(&other.sources);self.inputs.union(&other.inputs);self.calls.union(&other.calls);}
    pub fn joined(mut self,other:Self)->Self{self.union(&other);self}
    pub fn pending(&self)->bool{!self.inputs.is_empty()||!self.calls.is_empty()||self.sources.iter().any(|source|match source{Source::Position(origin)=>origin.pending(),Source::Element{parent,..}|Source::Projection{parent,..}=>parent.pending(),_=>false})}
    pub fn active(&self)->bool{self.sources.entries().any(|(_,when)|when.possible())||self.inputs.entries().any(|(_,when)|when.possible())||self.calls.entries().any(|(_,when)|when.possible())}
    pub fn restricted(&self,path:&bounds::Clause)->Self{Self{sources:self.sources.restricted(path),inputs:self.inputs.restricted(path),calls:self.calls.restricted(path)}}
    pub fn selected(choice:&bounds::Condition,a:&Self,b:&Self)->Self{
        // 完整借用相同，选支不改变来源；不生产随后无人需要的路径组合。
        if a==b{return a.clone();}
        a.restricted(&choice.assume(&bounds::Clause::default(),true)).joined(b.restricted(&choice.assume(&bounds::Clause::default(),false)))
    }
    pub fn guards_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{Self{sources:self.sources.guards_at_call(bindings),inputs:self.inputs.guards_at_call(bindings),calls:self.calls.guards_at_call(bindings)}}
    pub fn map_sources(&self,map:impl Fn(Source)->Self)->Self{let mut result=Self{inputs:self.inputs.clone(),calls:self.calls.clone(),..Self::default()};for(source,when)in self.sources.entries(){result.union(&map(source.clone()).restricted(&when));}result}
    pub fn unresolved(&self)->bool {self.sources.entries().any(|(source,when)|when.possible()&&match source {
        Source::Unresolved=>true,
        Source::Element{parent,..}|Source::Projection{parent,..}=>parent.restricted(&when).unresolved(),
        Source::Position(origin)=>matches!(&origin.root,address::Root::Resource{borrow,..}|address::Root::Derived(borrow)|address::Root::Dataset{borrow,..}if borrow.restricted(&when).unresolved()),
        _=>false,
    })}
    pub fn has_local(&self)->bool {self.sources.iter().any(|source|match source{Source::Position(origin)=>origin.has_local(),Source::Element{parent,..}|Source::Projection{parent,..}=>parent.has_local(),_=>false})}
    pub fn resources(&self)->Self {
        self.map_sources(|source|match source {
            Source::Position(origin)=>origin.resources(),
            Source::Element{parent,fields,index}=>Self::element(parent.resources(),fields,index),
            Source::Projection{parent,..}=>parent.resources(),
            source=>Self::source(source),
        })
    }
    pub fn owners(&self)->Self {
        self.map_sources(|source|match source{
            Source::Position(origin)=>origin.owners(),
            Source::Element{parent,fields,index}=>Self::element(parent.resources(),fields,index).joined(parent.owners()),
            Source::Projection{parent,..}=>parent.owners(),
            source=>Self::source(source),
        })
    }
    pub fn aliases(&self)->Vec<MemoryAlias> {
        self.sources.entries().filter(|(_,when)|when.possible()).flat_map(|(source,_)|match source {
            Source::Position(origin)=>origin.aliases(), _=>Vec::new(),
        }).collect::<BTreeSet<_>>().into_iter().collect()
    }
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{self.guards_at_call(bindings).map_sources(|source|source.at_call(bindings))}
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{
        let mut result=Self{inputs:self.inputs.output_bound(bindings),calls:self.calls.output_bound(bindings),..Self::default()};
        for(source,when)in self.sources.entries(){result.sources.insert_when(source.clone().bind_outputs(bindings),when.output_bound(bindings));}result
    }
}
impl crate::module::codec::Encode for Borrow{fn encode(&self,w:&mut crate::module::codec::Writer){
    crate::module::codec::Encode::encode(&self.sources.0.len(),w);
    for(source,when)in &self.sources.0{
        crate::module::codec::Encode::encode(source,w);
        // Inactive只表示此槽已知没有view，不授予任何资源或位置权限。
        if matches!(source,Source::Inactive){crate::module::codec::Encode::encode(&bounds::Condition::constant(true),w);}
        else{crate::module::codec::Encode::encode(when,w);}
    }
}}
impl crate::module::codec::Decode for Borrow{fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{Ok(Self{sources:crate::module::codec::Decode::decode(r)?,..Self::default()})}}
pub(super) fn resource_aliases(data:&application::Data)->Borrow {
    let mut value=Borrow::default();
    for id in data.root.proof.origins.first().into_iter().flatten(){
        let source=match id {
            ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)=>continue,
            ValueIdentity::DatasetContent(..)=>Source::Parameter(*id,Vec::new()),
            id=>Source::Data(*id),
        };
        value.sources.insert(source);
    }
    value
}
pub(super) fn resource_borrow(data:&application::Data)->Borrow{
    let mut value=data.root.proof.view_borrow(0);if value.empty()||matches!(data.root.identity(),ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)){value.sources.insert(Source::Data(data.root.identity()));}value
}
pub(super) fn resource_proof(data:&application::Data)->bounds::Proof{data.root.proof.borrow_state().with_view_borrow(0,resource_borrow(data))}

pub(super) enum InputSlot {
    Value {name:String,contents:Option<Vec<usize>>,component:usize,identity:Option<(ValueIdentity,Scalar)>},
    Data {name:String,path:Vec<usize>,independent:bool},
}
impl InputSlot {
    pub fn independent(&self)->bool {matches!(self,Self::Data{independent:true,..})}
    pub fn value_proof<'a>(&self,env:&'a BTreeMap<String,Binding>)->Option<&'a bounds::Proof>{
        match self {
            Self::Value{name,contents:Some(path),..}=>data_at(env.get(name)?,path)?.elements.known()?.view_value().map(|value|&value.proof),
            Self::Value{name,contents:None,..}=>match env.get(name)? {
                Binding::Value(value)=>Some(&value.proof),
                Binding::Owned(value)|Binding::ViewElement{element:value,..}=>Some(&value.proof),
                Binding::OwnedValue(value)=>Some(&value.value.proof),_=>None,
            },
            Self::Data{..}=>None,
        }
    }
    pub fn value_proof_mut<'a>(&self,env:&'a mut BTreeMap<String,Binding>)->Option<&'a mut bounds::Proof>{
        match self {
            Self::Value{name,contents:Some(path),..}=>data_at_mut(env.get_mut(name)?,path)?.elements.known_mut()?.view_value_mut().map(|value|&mut value.proof),
            Self::Value{name,contents:None,..}=>match env.get_mut(name)? {
                Binding::Value(value)=>Some(&mut value.proof),
                Binding::Owned(value)|Binding::ViewElement{element:value,..}=>Some(&mut value.proof),
                Binding::OwnedValue(value)=>Some(&mut value.value.proof),_=>None,
            },
            Self::Data{..}=>None,
        }
    }
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
    Unborrowed {resource:Borrow,borrow:Borrow,ownership:bool,span:Span},
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,owned:Borrow,span:Span},
    Address {borrow:Borrow,layout:address::Layout,written:Option<bounds::Proof>,writable:bool,initialized:bool,span:Span},
    Memory {target:address::MemoryTarget,borrow:Borrow,access:Option<Access>,written:Option<bounds::Proof>,snapshots:Option<Arc<[view_contracts::ValueSnapshot]>>,span:Span},
    Dataset {borrow:Borrow,index:Option<bounds::Operand>,access:Access,atomic:bool,values:Option<Arc<[view_contracts::ElementRead]>>,span:Span},
    Initialized {borrow:Borrow,read:address::InitializationRead,code:&'static str,span:Span},
    Nonzero {value:Box<Value>,path:bounds::Clause,span:Span},
}
impl Requirement {
    pub fn borrow(&self)->Option<&Borrow> {match self {Self::Unborrowed{borrow,..}|Self::Return{borrow,..}|Self::Address{borrow,..}|Self::Memory{borrow,..}|Self::Dataset{borrow,..}|Self::Initialized{borrow,..}=>Some(borrow),Self::Nonzero{..}=>None}}
    pub fn pending(&self)->bool {self.borrow().is_some_and(Borrow::pending)||matches!(self,Self::Nonzero{..})||matches!(self,Self::Unborrowed{resource,..}if resource.pending())||matches!(self,Self::Return{owned,..}if owned.pending())}
    pub fn under_path(&self,path:&bounds::Clause)->Self{match self{
        Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:borrow.restricted(path),allowed:allowed.clone(),owned:owned.clone(),span:*span},
        Self::Nonzero{..}=>self.clone(),
        Self::Unborrowed{..}|Self::Address{..}|Self::Memory{..}|Self::Dataset{..}|Self::Initialized{..}=>self.mapped(|borrow|borrow.restricted(path)),
    }}
    pub fn mapped(&self,map:impl Fn(&Borrow)->Borrow)->Self {
        match self {
            Self::Unborrowed{resource,borrow,ownership,span}=>Self::Unborrowed{resource:map(resource),borrow:map(borrow),ownership:*ownership,span:*span},
            Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:map(borrow),allowed:allowed.clone(),owned:map(owned),span:*span},
            Self::Address{borrow,layout,written,writable,initialized,span}=>Self::Address{borrow:map(borrow),layout:layout.clone(),written:written.clone(),writable:*writable,initialized:*initialized,span:*span},
            Self::Memory{target,borrow,access,written,snapshots,span}=>Self::Memory{target:*target,borrow:map(borrow),access:*access,written:written.clone(),snapshots:snapshots.clone(),span:*span},
            Self::Dataset{borrow,index,access,atomic,values,span}=>Self::Dataset{borrow:map(borrow),index:*index,access:*access,atomic:*atomic,values:values.clone(),span:*span},
            Self::Initialized{borrow,read,code,span}=>Self::Initialized{borrow:map(borrow),read:read.clone(),code,span:*span},
            Self::Nonzero{..}=>self.clone(),
        }
    }
    pub fn check(&self,source:&crate::Source)->Result<()> {
        if let Self::Nonzero{value,path,span}=self {
            return if bounds::integer_divisors(value,path).is_some(){Ok(())}else{source.fail(*span,"B152",bounds::INTEGER_DIVISOR_MESSAGE)};
        }
        if self.borrow().is_some_and(Borrow::unresolved)||matches!(self,Self::Unborrowed{resource,borrow,..}if !borrow.empty()&&resource.unresolved()){return source.fail(match self{Self::Unborrowed{span,..}|Self::Return{span,..}|Self::Address{span,..}|Self::Memory{span,..}|Self::Dataset{span,..}|Self::Initialized{span,..}|Self::Nonzero{span,..}=>*span},"B271","view的实际借用来源尚未形成合同");}
        if let Self::Address{borrow,layout,writable,initialized,span,..}=self {
            for(origin_source,when)in borrow.sources.entries() {
                if !when.possible(){continue;}
                if let Source::Position(origin)=origin_source {
                    if origin.layout!=*layout||!origin.layout_valid() {return source.fail(*span,"B263","地址解释的布局与原自有位置不同");}
                    if *writable&&!origin.writable {return source.fail(*span,"B227","自有地址不能升级原只读位置的写权限");}
                    if *initialized&&!origin.symbolic_input()&&!origin.initialized.iter().all(|ready|*ready) {return source.fail(*span,"B142","地址读取或inout需要原位置已经初始化");}
                }
            }
        }
        if let Self::Initialized{read,code,span,..}=self {
            if !read.complete(){return source.fail(*span,code,"读取或返回前没有完整初始化原位置的所需分量");}
        }
        match self {
            Self::Return{borrow,span,..} if borrow.has_local()=>source.fail(*span,"B227","地址不能返回或输出当前函数的局部存储"),
            Self::Unborrowed{resource,borrow,ownership,span} if (if *ownership{borrow.owners()}else{borrow.resources()}).sources.entries().any(|(source,when)|matches!(source,Source::Data(_)|Source::Element{..})&&resource.resources().overlaps_source(source,&when))=>source.fail(*span,"B227","dataset仍被实际view消费者借用，不能写入、移动、传输或释放"),
            Self::Return{borrow,allowed,owned,span} if !owned.pending()&&borrow.owners().sources.entries().any(|(source,when)|matches!(source,Source::Data(id)if !allowed.contains(id)&&when.overlaps(&bounds::Condition::guard(owned.owners().sources.when(source)).assume(&bounds::Clause::default(),false))))=>source.fail(*span,"B227","返回view不能借用当前函数将释放的dataset"),
            _=>Ok(()),
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Data(ValueIdentity),
    Element { parent:Box<Borrow>, fields:Vec<usize>, index:Option<bounds::Operand> },
    Projection { parent:Box<Borrow>, fields:Vec<usize> },
    Parameter(ValueIdentity,Vec<Option<bounds::Operand>>),
    Unresolved,
    // 实际变体分支在此槽没有view；区别于尚未知的来源。
    Inactive,
    // Explicit CPU address producers with no Blink data owner.
    External,
    Position(Arc<address::Origin>),
}
impl Source {
    fn index_substitute(index:Option<bounds::Operand>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Operand>{
        match index{
            Some(index@bounds::Operand::Value(id))if values.contains_key(&id)=>index.at_call(values),
            other=>other,
        }
    }
    fn index_at_call(index:Option<bounds::Operand>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Operand>{
        match index{
            Some(index@bounds::Operand::Value(id@ValueIdentity::ElementIndex(_)))if !values.contains_key(&id)=>Some(index),
            index=>index.and_then(|index|index.at_call(values)),
        }
    }
    fn overlaps(&self,other:&Self,path:&bounds::Clause)->bool{
        match(self,other){
            (Self::Element{parent,fields,index},Self::Element{parent:other,fields:other_fields,index:other_index})=>{
                if fields!=other_fields{return false;}
                if let(Some(a),Some(b))=(index,other_index){if !bounds::Condition::index_equal(*a,*b).assume(path,true).possible(){return false;}}
                parent.sources.entries().any(|(source,when)|other.overlaps_source(source,&bounds::Clause::both(path,&when)))
                    ||parent.inputs.entries().any(|(input,when)|bounds::Clause::both(path,&when).overlaps(&other.inputs.when(input)))
                    ||parent.calls.entries().any(|(call,when)|bounds::Clause::both(path,&when).overlaps(&other.calls.when(call)))
            },
            // Different input views can still borrow the same external resource.
            (Self::Parameter(a,_),Self::Parameter(b,_))=>a==b,
            _=>self==other,
        }
    }
    pub(super) fn bind_outputs(self, bindings: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        match self {
            Self::Data(id) => Self::Data(bindings.get(&id).copied().unwrap_or(id)),
            Self::Parameter(id,indices) => Self::Parameter(bindings.get(&id).copied().unwrap_or(id),indices.into_iter().map(|index|index.and_then(|index|index.map(|id|Some(bindings.get(&id).copied().unwrap_or(id))))).collect()),
            Self::Position(origin) => Self::Position(Arc::new(origin.bind_outputs(bindings))),
            Self::Element{parent,fields,index}=>Self::Element{parent:Box::new(parent.output_bound(bindings)),fields,index:index.and_then(|index|index.map(|id|Some(bindings.get(&id).copied().unwrap_or(id))))},
            Self::Projection{parent,fields}=>Self::Projection{parent:Box::new(parent.output_bound(bindings)),fields},
            Self::Unresolved | Self::Inactive | Self::External => self,
        }
    }
    pub(super) fn at_call(self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow {
        match self {
            Self::Parameter(id,indices)=>bindings.get(&id).map(|p|{
                let indices=indices.into_iter().map(|index|Self::index_at_call(index,bindings)).collect::<Vec<_>>();
                p.view_borrow(0).select_elements(&indices)
            }).unwrap_or_else(||Borrow::from_sources(BTreeSet::from([Self::Unresolved]))),
            Self::Data(id)=>{
                let Some(proof)=bindings.get(&id) else {return Borrow::from_sources(BTreeSet::from([Self::Unresolved]));};
                let value=proof.view_borrow(0);
                if !value.empty(){value}else{Borrow::from_sources(BTreeSet::from([proof.known_identity(0).map(Self::Data).unwrap_or(Self::Unresolved)]))}
            }
            Self::Position(origin)=>origin.at_call(bindings),
            Self::Element{parent,fields,index}=>Borrow::element(parent.at_call(bindings),fields,Self::index_at_call(index,bindings)),
            Self::Projection{parent,fields}=>Borrow::projection(parent.at_call(bindings),fields),
            Self::Unresolved|Self::Inactive|Self::External=>Borrow::from_sources(BTreeSet::from([self])),
        }
    }
}
crate::module::codec::variants!(Source{0=>Data(id),1=>Parameter(id,indices),2=>Unresolved,3=>Inactive,4=>External,5=>Position(origin),6=>Element{parent,fields,index},7=>Projection{parent,fields}});
impl Binding {
    pub(super) fn view_parts(&self)->Option<(&Ty,&bounds::Proof)> {
        match self {
            Self::Value(v) if v.ty.contains_borrows()=>Some((&v.ty,&v.proof)),
            Self::Owned(v) | Self::ViewElement { element: v, .. } if v.place.ty().contains_borrows()=>Some((v.place.ty(),&v.proof)),
            Self::OwnedValue(v) if v.value.ty.contains_borrows()=>Some((&v.value.ty,&v.value.proof)),
            _=>None,
        }
    }
    pub(super) fn view_proof_mut(&mut self)->Option<&mut bounds::Proof> {
        match self {Self::Value(v) if v.ty.contains_borrows()=>Some(&mut v.proof),Self::Owned(v) | Self::ViewElement { element: v, .. } if v.place.ty().contains_borrows()=>Some(&mut v.proof),Self::OwnedValue(v) if v.value.ty.contains_borrows()=>Some(&mut v.value.proof),_=>None}
    }
    pub(super) fn view_borrow(&self,builder:&Builder<'_>)->Result<Borrow> {
        if let Self::DataReference(data)=self{return Ok(resource_borrow(&data.data));}
        if let Self::OwnedValue(value)=self && value.ownership==application::Ownership::Borrowed {
            let mut borrow=value.value.proof.typed_view_borrow(&value.value.ty);
            for data in value.resources(){borrow.union(&data.root.proof.view_borrow(0));}return Ok(borrow);
        }
        if let Self::ViewElement{view,..}=self {return Ok(view.proof.borrowed_view());}
        let mut borrow=self.view_parts().map(|(ty,proof)|proof.typed_view_borrow(ty)).unwrap_or_default();
        borrow.union(&user_owned::cleanup_borrow(builder,self,&BTreeSet::new())?);Ok(borrow)
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
                if data.root.identity()==*identity {value.union(&data.root.proof.view_borrow(0));}
            }}
            if value.empty(){value.sources.insert(Source::Data(*identity));}result.union(&value);
        }
        result
    }
    pub(super) fn data_value_borrow(&self,data:&application::Data)->Borrow {resource_borrow(data)}
    pub(super) fn application_view_borrowed(&self, identity: ValueIdentity) -> Result<bool> {
        for (name,binding) in &self.env {
            if self.live_view_borrow(name,binding)?.owners().sources.contains(&Source::Data(identity)) { return Ok(true); }
        }
        Ok(false)
    }
    pub(super) fn require_view_unborrowed(&mut self,resource:&Borrow,span:Span)->Result<()> {
        let mut borrow=Borrow::default();
        for (name,binding) in &self.env {borrow.union(&self.live_view_borrow(name,binding)?);}
        self.check_view_requirement(Requirement::Unborrowed{resource:resource.clone(),borrow,ownership:false,span})
    }
    pub(super) fn check_view_requirement(&mut self,requirement:Requirement)->Result<()> {
        if requirement.pending()||matches!(&requirement,Requirement::Initialized{..}|Requirement::Memory{..}) {
            self.view_model.uses.push(view_contracts::Use{requirement,source:self.definition.source.clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect()});
            return Ok(());
        }
        if requirement.borrow().is_some_and(Borrow::empty){return Ok(());}
        requirement.under_path(&self.path).check(&self.definition.source)?;
        if matches!(&requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Memory{..}|Requirement::Dataset{..}){self.view_model.uses.push(view_contracts::Use{requirement,source:self.definition.source.clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect()});}
        Ok(())
    }
    pub(super) fn require_view_available(&self, value: &Value, resources: &[&application::Data], span: Span) -> Result<()> {
        fn selected(source:&Source)->bool{match source{
            Source::Data(_)=>true,
            Source::Parameter(_,indices)=>indices.iter().all(Option::is_some),
            Source::Element{parent,index:Some(_),..}|Source::Projection{parent,..}=>parent.calls.is_empty()&&parent.inputs.is_empty()&&!parent.sources.is_empty()&&parent.sources.iter().all(selected),
            _=>false,
        }}
        fn available(builder:&Builder<'_>,borrow:Borrow,resources:&[&application::Data],span:Span)->Result<()>{
            for(source,when)in borrow.sources.entries(){
                if !when.possible(){continue;}
                if matches!(source,Source::Element{..}|Source::Projection{..})&&selected(source){
                    let held=resources.iter().copied().chain(builder.env.values().flat_map(Binding::application_resources)).any(|data|{
                        let present=match &data.ownership{
                            application::Ownership::Conditional(present)=>builder.condition_truth(&present.condition(),Some(&bounds::Condition::guard(when.clone())))==Some(true),
                            ownership=>ownership.available(),
                        };
                        let actual=resource_borrow(data).restricted(&builder.path);
                        present&&!when.overlaps(&bounds::Condition::guard(actual.sources.when(source)).assume(&bounds::Clause::default(),false))
                    });
                    if held{continue;}
                }
                match source{
                    Source::Data(id)=>builder.application_handle_available(*id,span,resources,&when)?,
                    Source::Element{parent,..}|Source::Projection{parent,..}=>available(builder,parent.restricted(&when),resources,span)?,
                    Source::Position(origin)=>available(builder,origin.owners().restricted(&when),resources,span)?,
                    Source::Unresolved=>return builder.fail(span,"B271","view返回的实际借用来源尚未形成合同"),
                    Source::Parameter(..)|Source::Inactive|Source::External=>{},
                }
            }
            Ok(())
        }
        available(self,value.proof.typed_view_borrow(&value.ty).restricted(&self.path),resources,span)
    }
    pub(super) fn require_view_return_proof(&mut self,proof:&bounds::Proof,owned:Borrow,span:Span)->Result<()> {
        let mut allowed=self.env.values().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Borrowed).flat_map(|data|std::iter::once(data.root.identity()).chain(data.root.proof.borrowed_views().into_iter().filter_map(|source|match source {Source::Data(id)=>Some(id),_=>None}))).collect::<BTreeSet<_>>();
        for param in &self.definition.function.params {if matches!(param.access,Access::Write|Access::InOut) && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for data in value.resources(){if data.ownership!=application::Ownership::Moved{allowed.insert(data.root.identity());allowed.extend(data.root.proof.borrowed_views().into_iter().filter_map(|source|match source{Source::Data(id)=>Some(id),_=>None}));}}}}
        for(index,param)in self.definition.function.params.iter().enumerate(){if param.access==Access::Read && let Some(Binding::OwnedValue(value))=self.env.get(&param.name){for resource in 0..resource_paths(&value.value.ty).len(){allowed.insert(ValueIdentity::ResourceParameter(index,resource));}}}
        self.check_view_requirement(Requirement::Return{borrow:proof.borrowed_view(),allowed,owned,span})
    }
    pub(super) fn create_application_view(
        &mut self,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        if self.application.is_none() && self.program.target != Target::Cpu
            || explicit.len() != 1 || !matches!(args.len(), 1 | 3)
        {
            return self.fail(span, "B271", "CPU应用以view<T>(dataset或view[,start,end])借用现有存储");
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
        let mut value = if let Some(data) = self.application_reference(&args[0])? {
        if !data.permits(Access::Read) {
            return self.fail(span, "B214", "原子dataset不能转换为普通只读view");
        }
        self.require_same(ty.view.as_deref().unwrap(), data.element_layout().1, span)?;
        self.require_application_reference_initialized(&data, span)?;
        let(stride,_)=self.application_reference_layout(&data);
        let stride_parts=self.application_stride_parts(&stride);
        self.application_cpu(&data, span)?;
        let address = self.application_projection_address(&data,span)?;
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
        let stride=Value{ty:Ty::vector(Scalar::U32,2),regs:stride_parts.to_vec(),proof:bounds::Proof::default(),literal:false,uniform:stride.uniform,constant:None};
        proof.append(&stride);
        proof = proof.with_view_borrow(0, self.data_value_borrow(&data.data));
        Value {
            proof,
            literal: false,
            ty,
            regs: vec![address[0], address[1], data.length.regs[0],stride_parts[0],stride_parts[1]],
            uniform: false,
            constant: None,
        }
        } else {
            let Some(value) = self.expr(&args[0])? else { return Ok(None); };
            self.require_same(&ty, &value.ty, args[0].span)?;
            value
        };
        if args.len() == 3 {
            let held = self.hold_binding(Binding::Value(value.clone()));
            let evaluated = args[1..].iter().map(|arg| self.expr(arg)).collect::<Result<Option<Vec<_>>>>();
            self.env.remove(&held);
            let Some(mut limits) = evaluated? else { return Ok(None); };
            let end = limits.pop().unwrap(); let start = limits.pop().unwrap();
            for limit in [&start, &end] { self.require_same(&Ty::scalar(Scalar::U32), &limit.ty, span)?; }
            let length = Self::view_length(&value);
            for (a, b) in [(&start, &end), (&end, &length)] {
                if !(a.proof.below_length(&a.component_origins(0), &bounds::Proof::capture(b), 0, false) || bounds::Condition::comparison(Comparison::LessEqual, a, b).truth_in(&self.path) == Some(true)) {
                    return self.fail(span, "B137", "view区间需要证明start<=end<=原长度");
                }
            }
            let borrow = value.proof.view_borrow(0);
            let length = self.binary("-", end, start.clone(), span,None)?;
            let parts=[value.regs[3],value.regs[4]];
            let stride_value=Value{ty:Ty::vector(Scalar::U32,2),regs:parts.to_vec(),proof:value.projected_proof(&[3,4]),literal:false,uniform:value.uniform,constant:None};
            let low=self.reg(Scalar::U64);self.emit(InstKind::Cast{dst:low,src:parts[0]});
            let high=self.reg(Scalar::U64);self.emit(InstKind::Cast{dst:high,src:parts[1]});
            let width=self.constant(Scalar::U64,32).regs[0];
            let shifted=self.reg(Scalar::U64);self.emit(InstKind::Binary{dst:shifted,op:BinaryOp::ShiftLeft,a:high,b:width});
            let stride=self.reg(Scalar::U64);self.emit(InstKind::Binary{dst:stride,op:BinaryOp::Or,a:low,b:shifted});
            let zero = self.constant(Scalar::U32, 0).regs[0];
            let address = self.offset_application_address([value.regs[0], value.regs[1]], start.regs[0], stride, zero);
            let pointer = Value { proof: bounds::Proof::default(), ty: Ty::vector(Scalar::U32, 2),
                regs: address.to_vec(), literal: false, uniform: false, constant: None };
            value.proof = bounds::Proof::default();
            value.proof.append(&pointer); value.proof.append(&length); value.proof.append(&stride_value);
            value.proof = value.proof.with_view_borrow(0, borrow);
            value.regs = vec![address[0], address[1], length.regs[0],parts[0],parts[1]];
        }
        Ok(Some(value))
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
        let stride = ElementStride::Value([view.regs[3],view.regs[4]]);
        let words = ty.components.iter().map(|(_, offset)| *offset).collect();
        Place::ViewElement {
            ty,
            source: view.view_identity(),
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
            if place.ty().dataset.is_some() {
                let data = self.application_place_reference(place, base.span)?;
                return self.application_index(data, index);
            }
            if place.ty().indexed_element().is_some() {
                return self.array_place(place, index);
            }
            self.read_place(&place, base.span, None)?
        } else {
            let returned = self.evaluate_returned(base, None)?;
            if !self.flow.continues() {
                return Ok(None);
            }
            match returned {
                Some(returns::Returned::Dataset(data)) => {
                    self.hold_binding(Binding::ApplicationData(data.clone()));
                    return self.application_index(application::DataReference::direct(data), index);
                }
                Some(returns::Returned::OwnedValue(value)) if value.value.ty.array.is_some() => {
                    let owner = self.owned_value_binding(value, true);
                    let place = owner.place().unwrap();
                    self.hold_binding(Binding::OwnedValue(owner));
                    return self.array_place(place, index);
                }
                returned => {
                    let Some(value) = self.returned_expression(returned, base.span)? else {
                        return Ok(None);
                    };
                    value
                }
            }
        };
        if view.ty.array.is_some() {
            let place = self.materialize_array(view,None).place;
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
        let length=Self::view_length(&view);
        if !self.index_in(
            &value,
            IndexDomain::View(view.view_identity()),
            length.minimum(),
            Some(&length.proof),
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
