//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
// 循环引用只在原Proof中连接入口和实际回边；公共合同只编码闭合来源。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LoopInput { pub level: usize, pub index: usize }
// 来源边只持有该来源实际成立的条件。不存在的边和已知不成立的边仍可区分。
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Edges<K:Ord>(BTreeMap<K,bounds::Clause>);
impl<K:Ord> Default for Edges<K>{fn default()->Self{Self(BTreeMap::new())}}
impl<K:Ord+Clone> Edges<K>{
    fn substitute_values(&self, values: &BTreeMap<ValueIdentity,bounds::Proof>) -> Self {
        Self(self.0.iter().map(|(key, when)| (key.clone(), when.substitute_values(values))).collect())
    }

    pub fn insert(&mut self, key: K) { self.insert_when(key, bounds::Clause::default()); }

    pub fn insert_when(&mut self, key: K, when: bounds::Clause) {
        use std::collections::btree_map::Entry;
        match self.0.entry(key) {
            Entry::Vacant(entry) => { entry.insert(when); }
            Entry::Occupied(mut entry) => {
                let joined = bounds::Clause::either(entry.get(), &when).canonical();
                entry.insert(joined);
            }
        }
    }

    pub fn union(&mut self, other: &Self) {
        use std::collections::btree_map::Entry;
        for (key, when) in &other.0 {
            match self.0.entry(key.clone()) {
                Entry::Vacant(entry) => { entry.insert(when.clone()); }
                Entry::Occupied(mut entry) => {
                    let joined = bounds::Clause::either(entry.get(), when);
                    entry.insert(joined);
                }
            }
        }
    }

    pub(in crate::check) fn insert_normalized(&mut self, key: K, when: bounds::Clause, normal: &mut bounds::Normalizer) {
        use std::collections::btree_map::Entry;
        match self.0.entry(key) {
            Entry::Vacant(entry) => { entry.insert(normal.clause(&when)); }
            Entry::Occupied(mut entry) => {
                let joined = normal.clause(&bounds::Clause::either(entry.get(), &when));
                entry.insert(joined);
            }
        }
    }

    pub(super) fn same_sources(&self, other: &Self) -> bool {
        self.0.keys().eq(other.0.keys())
    }

    pub(super) fn equivalent_paths(&self, other: &Self) -> bool {
        self.0.values().zip(other.0.values())
            .all(|(left, right)| left.equivalent(right))
    }

    pub fn remove(&mut self, key: &K) -> Option<bounds::Clause> { self.0.remove(key) }
    pub fn contains(&self, key: &K) -> bool { self.0.get(key).is_some_and(bounds::Clause::possible) }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
    pub fn iter(&self) -> std::collections::btree_map::Keys<'_, K, bounds::Clause> { self.0.keys() }
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = (&K, bounds::Clause)> {
        self.0.iter().map(|(key, when)| (key, when.clone()))
    }
    pub fn into_entries(self) -> impl Iterator<Item = (K, bounds::Clause)> { self.0.into_iter() }
    pub fn when(&self, key: &K) -> bounds::Clause {
        self.0.get(key).cloned().unwrap_or_else(bounds::Clause::impossible)
    }

    fn when_absent(&self, key: &K) -> bounds::Clause {
        bounds::Condition::guard(self.when(key)).assume(&bounds::Clause::default(), false)
    }

    fn restrict(&mut self, path: &bounds::Clause) {
        for when in self.0.values_mut() {
            *when = bounds::Clause::both(when, path);
        }
    }

    pub(super) fn restricted(&self, path: &bounds::Clause) -> Self {
        let mut value = self.clone();
        value.restrict(path);
        value
    }

    fn guards_at_call(&self, bindings: &BTreeMap<ValueIdentity,bounds::Proof>) -> Self {
        Self(self.0.iter().map(|(key, when)| {
            let when = when.project_guard(bindings);
            (key.clone(), when)
        }).collect())
    }

    fn output_bound(&self, bindings: &BTreeMap<ValueIdentity,ValueIdentity>) -> Self {
        Self(self.0.iter().map(|(key, when)| (key.clone(), when.output_bound(bindings))).collect())
    }
}
impl<K:Ord+Clone,const N:usize> From<[K;N]> for Edges<K>{fn from(values:[K;N])->Self{values.into_iter().collect()}}
impl<K:Ord+Clone> FromIterator<K> for Edges<K>{fn from_iter<T:IntoIterator<Item=K>>(values:T)->Self{let mut result=Self::default();for value in values{result.insert(value);}result}}
impl<'a,K:Ord> IntoIterator for &'a Edges<K>{type Item=&'a K;type IntoIter=std::collections::btree_map::Keys<'a,K,bounds::Clause>;fn into_iter(self)->Self::IntoIter{self.0.keys()}}
impl<K:Ord> IntoIterator for Edges<K>{type Item=K;type IntoIter=std::collections::btree_map::IntoKeys<K,bounds::Clause>;fn into_iter(self)->Self::IntoIter{self.0.into_keys()}}
impl<K:Ord+Clone+crate::module::codec::Decode> crate::module::codec::Decode for Edges<K>{
    fn decode(r: &mut crate::module::codec::Reader<'_>) -> crate::module::codec::Result<Self> {
        let values = <BTreeMap<K, bounds::Condition> as crate::module::codec::Decode>::decode(r)?;
        Ok(Self(values.into_iter().map(|(key, condition)| {
            (key, condition.assume(&bounds::Clause::default(), true))
        }).collect()))
    }
}
#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Borrow{
    pub sources:Edges<Source>,
    pub inputs:Edges<LoopInput>,
    pub calls:Edges<view_contracts::CallResult>,
    pub reads:Edges<view_contracts::ContentRead>,
}
impl Borrow {
    pub(super) fn normalize_contract(&mut self,normal:&mut bounds::Normalizer) {
        let sources=std::mem::take(&mut self.sources.0);
        for(mut source,when)in sources {
            match &mut source {
                Source::Position(origin)|Source::Contents{origin,..}=>normal.origin(Arc::make_mut(origin)),
                Source::Element{parent,..}|Source::Projection{parent,..}=>parent.normalize_contract(normal),
                Source::Relation(graph,_)=>Arc::make_mut(graph).normalize(normal),
                Source::Task{captures,outputs,initialized,guards,completions}=>{
                    captures.normalize_contract(normal);
                    initialized.normalize_contract(normal);
                    for output in outputs { output.normalize_contract(normal); }
                    for guard in guards { guard.target.normalize_contract(normal); }
                    for completion in completions { completion.task.normalize_contract(normal); }
                },
                Source::TaskOutput{task,..}=>task.normalize_contract(normal),
                _=>{},
            }
            self.sources.insert_normalized(source,when,normal);
        }
    }
}
#[derive(Clone,PartialEq,Eq)]
pub(super) struct TaskInputCompletion {
    pub offset:u32,
    pub result:Ty,
    pub layout:Ty,
    pub task:Borrow,
    pub parent:Option<(usize,usize)>,
}
crate::module::codec::record!(TaskInputCompletion {offset,result,layout,task,parent});
impl Ord for TaskInputCompletion {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        use crate::module::codec::Encode;
        self.offset.cmp(&other.offset)
            .then_with(||self.result.bytes().cmp(&other.result.bytes()))
            .then_with(||self.layout.bytes().cmp(&other.layout.bytes()))
            .then_with(||self.task.cmp(&other.task))
            .then_with(||self.parent.cmp(&other.parent))
    }
}
impl PartialOrd for TaskInputCompletion {
    fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}
}
impl TaskInputCompletion {
    pub(super) fn map_task(&self,map:impl FnOnce(&Borrow)->Borrow)->Self {
        Self{offset:self.offset,result:self.result.clone(),layout:self.layout.clone(),task:map(&self.task),parent:self.parent}
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TaskProjection { Returned(usize,Option<ValueIdentity>), Writes, Places, Initialized(Vec<Option<ValueIdentity>>) }
crate::module::codec::variants!(TaskProjection {0=>Returned(component,source),1=>Writes,2=>Places,3=>Initialized(returned)});

impl TaskProjection {
    pub(super) fn map_result(&self, map:impl Fn(ValueIdentity)->Option<ValueIdentity>)->Self {
        match self {
            Self::Returned(component,source)=>Self::Returned(*component,source.and_then(&map)),
            Self::Initialized(returned)=>Self::Initialized(returned.iter().map(|id|id.and_then(&map)).collect()),
            projection=>projection.clone(),
        }
    }
}

impl Borrow{
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut result=Self{inputs:self.inputs.substitute_values(values),calls:self.calls.substitute_values(values),reads:self.reads.substitute_values(values),..Self::default()};
        for(source,when)in self.sources.entries(){
            let next=match source {
                Source::Position(origin)=>origin.substitute_values(values).normalize(),
                Source::Relation(graph,root)=>Self::source(Source::Relation(Arc::new(graph.substitute_values(values)),*root)),
                Source::Contents{origin,word}=>Self::source(Source::Contents{origin:Arc::new(origin.substitute_values(values)),word:*word}),
                Source::Element{parent,fields,index}=>Self::element(parent.substitute_values(values),fields.clone(),Source::index_substitute(*index,values)),
                Source::Projection{parent,fields}=>Self::projection(parent.substitute_values(values),fields.clone()),
                Source::Task{captures,outputs,initialized,guards,completions}=>Self::source(Source::Task{
                    captures:Box::new(captures.substitute_values(values)),
                    initialized:Box::new(initialized.substitute_values(values)),
                    outputs:outputs.iter().map(|output|output.substitute_values(values)).collect(),
                    completions:completions.iter().map(|completion|completion.map_task(|task|task.substitute_values(values))).collect(),
                    guards:guards.iter().cloned().map(|mut guard| {
                        guard.target=guard.target.substitute_values(values);
                        guard.map_arguments(|proof|proof.substitute_values(values));
                        guard.map_inputs(|word|match word {
                            bounds::Operand::Value(id) if values.contains_key(&id)=>word.at_call(values),
                            _=>Some(word),
                        });
                        guard
                    }).collect(),
                }),
                Source::TaskOutput{task,projection}=>task.substitute_values(values).task_projection(projection.map_result(|id|match values.get(&id){Some(proof)=>proof.known_identity(0),None=>Some(id)})),
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
        if !parent.inputs.is_empty()||!parent.calls.is_empty()||!parent.reads.is_empty(){
            result.sources.insert(Source::Element{parent:Box::new(Self{inputs:parent.inputs,calls:parent.calls,reads:parent.reads,..Self::default()}),fields,index});
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
        let mut out=Self{inputs:self.inputs.substitute_values(&values),calls:self.calls.substitute_values(&values),reads:self.reads.substitute_values(&values),..Self::default()};
        for(source,when)in self.sources.entries(){
            let value=match source{
                Source::Element{parent,fields,index}=>Self::element(parent.select_elements(indices),fields.clone(),Source::index_substitute(*index,&values)),
                Source::Relation(graph,root)=>graph.select(*root,indices),
                Source::Parameter(id,selected)=>{
                    let selected=if matches!(id,ValueIdentity::DatasetContent(..))&&selected.is_empty(){indices.to_vec()}
                        else{selected.iter().map(|index|Source::index_substitute(*index,&values)).collect()};
                    Self::source(Source::Parameter(*id,selected))
                },
                Source::Projection{parent,fields}=>Self::projection(parent.select_elements(indices),fields.clone()),
                Source::Task{captures,outputs,initialized,guards,completions}=>Self::source(Source::Task{
                    captures:Box::new(captures.select_elements(indices)),
                    initialized:Box::new(initialized.select_elements(indices)),
                    outputs:outputs.iter().map(|output|output.select_elements(indices)).collect(),
                    completions:completions.iter().map(|completion|completion.map_task(|task|task.select_elements(indices))).collect(),
                    guards:guards.iter().cloned().map(|mut guard| {
                        guard.target=guard.target.select_elements(indices);
                        guard
                    }).collect(),
                }),
                Source::TaskOutput{task,projection}=>task.select_elements(indices).task_projection(projection.clone()),
                Source::Position(origin)=>origin.map_borrows(|borrow|borrow.select_elements(indices)).substitute_values(&values).normalize(),
                Source::Contents{origin,word}=>Self::source(Source::Contents{origin:Arc::new(origin.map_borrows(|borrow|borrow.select_elements(indices)).substitute_values(&values)),word:*word}),
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
        if !parent.inputs.is_empty()||!parent.calls.is_empty()||!parent.reads.is_empty(){
            result.sources.insert(Source::Projection{parent:Box::new(Self{inputs:parent.inputs,calls:parent.calls,reads:parent.reads,..Self::default()}),fields});
        }
        result
    }
    pub(super) fn task_projection(&self,projection:TaskProjection)->Self {
        let mut result=Self::default();
        for (source,when) in self.sources.entries() {
            let value=match (source,&projection) {
                (Source::Task{outputs,..},TaskProjection::Returned(component,None)) => outputs[*component].clone(),
                (Source::Task{initialized,..},TaskProjection::Initialized(returned)) => initialized.initialization_guards(|id|match id {
                    ValueIdentity::TaskResult(component)=>returned[component].map(identity_proof),
                    _=>Some(identity_proof(id)),
                }),
                (Source::Inactive,TaskProjection::Initialized(_)) => Self::default(),
                (Source::Task{captures,..},TaskProjection::Writes|TaskProjection::Places) => captures.task_projection(projection.clone()),
                (Source::Position(origin),TaskProjection::Writes) if origin.writable => Self::source(source.clone()),
                (Source::Position(_),TaskProjection::Places) => Self::source(source.clone()),
                // Memory effects use all possible returned owners. The completion marker
                // selects the concrete owner only when the caller receives the result.
                (Source::TaskOutput{task,projection:TaskProjection::Returned(component,Some(_))},
                    TaskProjection::Writes|TaskProjection::Places) => {
                    task.task_projection(TaskProjection::Returned(*component,None)).task_projection(projection.clone())
                },
                (Source::TaskOutput{task,projection:held},TaskProjection::Writes|TaskProjection::Places)
                    if matches!(held,TaskProjection::Writes|TaskProjection::Places) =>
                {
                    task.task_projection(if *held==TaskProjection::Writes { TaskProjection::Writes } else { projection.clone() })
                },
                (Source::Inactive|Source::Data(_)|Source::Position(_)|Source::External,TaskProjection::Writes|TaskProjection::Places) => Self::default(),
                (Source::Inactive|Source::Unresolved,_) => Self::source(source.clone()),
                _ => Self::source(Source::TaskOutput{task:Box::new(Self::source(source.clone())),projection:projection.clone()}),
            };
            result.union(&value.restricted(&when));
        }
        if !self.inputs.is_empty()||!self.calls.is_empty()||!self.reads.is_empty(){
            result.sources.insert(Source::TaskOutput{
                task:Box::new(Self{inputs:self.inputs.clone(),calls:self.calls.clone(),reads:self.reads.clone(),..Self::default()}),
                projection,
            });
        }
        result
    }
    pub(super) fn content_inputs(&self)->bool{self.sources.iter().any(|source|match source{
        Source::Contents{..}=>true,
        Source::Relation(graph,root)=>graph.content_inputs(*root),
        Source::Position(origin)=>origin.resources().content_inputs(),
        Source::Element{parent,..}|Source::Projection{parent,..}=>parent.content_inputs(),
        Source::Task{captures,outputs,initialized,..}=>captures.content_inputs()||initialized.content_inputs()||outputs.iter().any(Self::content_inputs),
        Source::TaskOutput{task,..}=>task.content_inputs(),
        _=>false,
    })}
    pub fn empty(&self)->bool{self.sources.is_empty()&&self.inputs.is_empty()&&self.calls.is_empty()&&self.reads.is_empty()}
    pub fn union(&mut self,other:&Self){self.sources.union(&other.sources);self.inputs.union(&other.inputs);self.calls.union(&other.calls);self.reads.union(&other.reads);}
    pub fn joined(mut self,other:Self)->Self{self.union(&other);self}
    pub fn pending(&self)->bool{!self.inputs.is_empty()||!self.calls.is_empty()||!self.reads.is_empty()||self.sources.iter().any(|source|match source{Source::Position(origin)|Source::Contents{origin,..}=>origin.pending(),Source::Element{parent,..}|Source::Projection{parent,..}=>parent.pending(),Source::Relation(graph,root)=>graph.pending(*root),Source::Task{captures,outputs,initialized,..}=>captures.pending()||initialized.pending()||outputs.iter().any(Self::pending),Source::TaskOutput{task,projection}=>!matches!(projection,TaskProjection::Returned(..))||task.pending(),_=>false})}
    pub fn active(&self)->bool{self.sources.entries().any(|(_,when)|when.possible())||self.inputs.entries().any(|(_,when)|when.possible())||self.calls.entries().any(|(_,when)|when.possible())||self.reads.entries().any(|(_,when)|when.possible())}
    fn restricted_by(mut self, condition: &bounds::Clause) -> Self {
        self.sources.restrict(condition);
        self.inputs.restrict(condition);
        self.calls.restrict(condition);
        self.reads.restrict(condition);
        self
    }

    pub fn restricted(&self, path: &bounds::Clause) -> Self {
        if self.empty() {
            return Self::default();
        }
        self.clone().restricted_by(path)
    }
    pub fn selected(choice:&bounds::Condition,a:&Self,b:&Self)->Self{
        // 完整借用相同，选支不改变来源；不生产随后无人需要的路径组合。
        if a==b{return a.clone();}
        a.restricted(&choice.assume(&bounds::Clause::default(),true)).joined(b.restricted(&choice.assume(&bounds::Clause::default(),false)))
    }
    fn initialization_guards(&self, lookup:impl Fn(ValueIdentity)->Option<bounds::Proof>)->Self {
        let mut result=self.clone();
        for when in result.sources.0.values_mut() {
            let condition = bounds::Condition::guard(when.clone());
            let bindings=bounds::Proof::condition(condition.clone()).scalar_identities().into_iter()
                .filter_map(|id|lookup(id).map(|proof|(id,proof))).collect();
            *when=condition.guaranteed_at_call(&bindings).assume(&bounds::Clause::default(), true);
        }
        result
    }

    pub(super) fn initialization_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        self.initialization_guards(|id|match id {
            ValueIdentity::TaskResult(_)=>Some(identity_proof(id)),
            _=>bindings.get(&id).cloned(),
        })
    }

    pub fn guards_at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{Self{sources:self.sources.guards_at_call(bindings),inputs:self.inputs.guards_at_call(bindings),calls:self.calls.guards_at_call(bindings),reads:self.reads.guards_at_call(bindings)}}
    pub(super) fn bind_guards(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        Self{sources:self.sources.output_bound(bindings),inputs:self.inputs.output_bound(bindings),
            calls:self.calls.output_bound(bindings),reads:self.reads.output_bound(bindings)}
    }
    pub fn map_sources(&self, map: impl Fn(Source) -> Self) -> Self {
        let mut result = Self {
            inputs: self.inputs.clone(), calls: self.calls.clone(),
            reads: self.reads.clone(), ..Self::default()
        };
        for (source, condition) in &self.sources.0 {
            result.union(&map(source.clone()).restricted_by(condition));
        }
        result
    }
    pub fn unresolved(&self)->bool {
        self.unresolved_in(&mut || Some(bounds::Clause::default()))
    }

    fn unresolved_in(
        &self,
        reachable_path: &mut dyn FnMut() -> Option<bounds::Clause>,
    ) -> bool {
        self.sources.entries().any(|(source, when)| {
            Self::source_unresolved(source, || {
                let path = reachable_path()?;
                let when = bounds::Clause::both(&path, &when);
                when.possible().then_some(when)
            })
        })
    }

    pub(super) fn source_unresolved(
        source: &Source,
        reachable_path: impl FnOnce() -> Option<bounds::Clause>,
    ) -> bool {
        let path = std::cell::LazyCell::new(reachable_path);
        let mut reachable = || (*path).clone();
        match source {
            Source::Unresolved => reachable().is_some(),
            Source::Relation(graph, root) =>
                reachable().is_some() && graph.unresolved(*root),
            Source::Element { parent, .. } | Source::Projection { parent, .. } =>
                parent.unresolved_in(&mut reachable),
            Source::Task { captures, outputs, initialized, .. } =>
                captures.unresolved_in(&mut reachable)
                    || initialized.unresolved_in(&mut reachable)
                    || outputs.iter().any(|output| output.unresolved_in(&mut reachable)),
            Source::TaskOutput { task, .. } => task.unresolved_in(&mut reachable),
            Source::Contents { origin, .. } =>
                reachable().is_some() && (origin.has_local() || origin.resources().unresolved()),
            Source::Position(origin) => match &origin.root {
                address::Root::Resource { borrow, .. } | address::Root::Derived(borrow)
                | address::Root::Dataset { borrow, .. } => borrow.unresolved_in(&mut reachable),
                _ => false,
            },
            _ => false,
        }
    }

    pub fn has_local(&self)->bool {
        fn query(value:&Borrow,path:&bounds::Clause)->bool {
            value.sources.entries().any(|(source,when)|{
                let when=bounds::Clause::both(path,&when);
                when.possible()&&match source {
                    Source::Relation(graph,root)=>graph.has_local(*root),
                    Source::Position(origin)=>match &origin.root {
                        address::Root::Derived(parent)|address::Root::Dataset{borrow:parent,..}=>query(parent,&when),
                        _=>origin.has_local(),
                    },
                    Source::Element{parent,..}|Source::Projection{parent,..}=>query(parent,&when),
                    Source::Task{captures,outputs,initialized,..}=>query(captures,&when)||query(initialized,&when)||outputs.iter().any(|output|query(output,&when)),
                    Source::TaskOutput{task,..}=>query(task,&when),
                    _=>false,
                }
            })
        }
        query(self,&bounds::Clause::default())
    }
    pub fn view_resources(&self)->Self {
        self.map_sources(|source|match source {
            Source::Task{captures,..}=>captures.map_sources(|source|match source {
                Source::Position(_)=>Self::default(),
                source=>Self::source(source).view_resources(),
            }),
            Source::TaskOutput{..}=>Self::source(source),
            source=>Self::source(source).resources(),
        })
    }
    pub fn resources(&self)->Self {
        self.map_sources(|source|match source {
            Source::Position(origin)=>origin.resources(),
            Source::Relation(graph,root)=>graph.resources(root),
            Source::Element{parent,fields,index}=>Self::element(parent.resources(),fields,index),
            Source::Projection{parent,..}=>parent.resources(),
            Source::Task{captures,..}=>captures.resources(),
            Source::TaskOutput{task,..}=>task.resources(),
            source=>Self::source(source),
        })
    }
    pub fn owners(&self)->Self {
        self.map_sources(|source|match source{
            Source::Position(origin)=>origin.owners(),
            Source::Relation(graph,root)=>graph.owners(root),
            Source::Element{parent,fields,index}=>Self::element(parent.resources(),fields,index).joined(parent.owners()),
            Source::Projection{parent,..}=>parent.owners(),
            Source::Task{captures,..}=>captures.owners(),
            Source::TaskOutput{task,..}=>task.owners(),
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
        let mut result=Self{inputs:self.inputs.output_bound(bindings),calls:self.calls.output_bound(bindings),reads:self.reads.output_bound(bindings),..Self::default()};
        for(source,when)in self.sources.entries(){result.sources.insert_when(source.clone().bind_outputs(bindings),when.output_bound(bindings));}result
    }
}
impl crate::module::codec::Encode for Borrow{fn encode(&self,w:&mut crate::module::codec::Writer){
    crate::module::codec::Encode::encode(&self.sources.0.len(),w);
    for(source,when)in &self.sources.0{
        crate::module::codec::Encode::encode(source,w);
        // Inactive只表示此槽已知没有view，不授予任何资源或位置权限。
        if matches!(source,Source::Inactive){crate::module::codec::Encode::encode(&bounds::Condition::constant(true),w);}
        else{crate::module::codec::Encode::encode(&bounds::Condition::guard(when.clone()),w);}
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Exclusion {
    Resource,
    Ownership,
    Position(Access),
    View,
}
crate::module::codec::variants!(Exclusion {0=>Resource,1=>Ownership,2=>Position(access),3=>View});

#[derive(Clone)]
pub(super) enum Requirement {
    Unborrowed {resource:Borrow,borrow:Borrow,kind:Exclusion,span:Span},
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,owned:Borrow,span:Span},
    Address {borrow:Borrow,layout:address::Layout,written:Option<bounds::Proof>,writable:bool,initialized:bool,span:Span},
    Memory {target:address::MemoryTarget,borrow:Borrow,access:Option<Access>,written:Option<bounds::Proof>,snapshots:Option<Arc<[view_contracts::ValueSnapshot]>>,address_fields:Arc<[(u32,usize)]>,span:Span},
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
            Self::Unborrowed{resource,borrow,kind,span}=>Self::Unborrowed{resource:map(resource),borrow:map(borrow),kind:*kind,span:*span},
            Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:map(borrow),allowed:allowed.clone(),owned:map(owned),span:*span},
            Self::Address{borrow,layout,written,writable,initialized,span}=>Self::Address{borrow:map(borrow),layout:layout.clone(),written:written.clone(),writable:*writable,initialized:*initialized,span:*span},
            Self::Memory{target,borrow,access,written,snapshots,address_fields,span}=>Self::Memory{target:*target,borrow:map(borrow),access:*access,written:written.clone(),snapshots:snapshots.clone(),address_fields:address_fields.clone(),span:*span},
            Self::Dataset{borrow,index,access,atomic,values,span}=>Self::Dataset{borrow:map(borrow),index:*index,access:*access,atomic:*atomic,values:values.clone(),span:*span},
            Self::Initialized{borrow,read,code,span}=>Self::Initialized{borrow:map(borrow),read:read.clone(),code,span:*span},
            Self::Nonzero{..}=>self.clone(),
        }
    }
    pub fn check(&self, source: &crate::Source, call_path: Option<&bounds::Clause>) -> Result<()> {
        let applies = || call_path.is_none_or(bounds::Clause::possible);

        if let Self::Nonzero{value,path,span}=self {
            return if bounds::integer_divisors(value,path).is_some(){Ok(())}else{source.fail(*span,"B152",bounds::INTEGER_DIVISOR_MESSAGE)};
        }
        if let Self::Address{borrow,layout,writable,initialized,span,..}=self {
            for (origin_source, when) in borrow.sources.entries() {
                let Source::Position(origin) = origin_source else { continue; };
                let violation = if origin.layout != *layout || !origin.layout_valid() {
                    ("B263", "地址解释的布局与原自有位置不同")
                } else if *writable && !origin.writable {
                    ("B227", "自有地址不能升级原只读位置的写权限")
                } else if *initialized && !origin.symbolic_input() && !origin.initialized.iter().all(|ready| *ready) {
                    ("B142", "地址读取或inout需要原位置已经初始化")
                } else {
                    continue;
                };
                if when.possible() {
                    return if applies() { source.fail(*span, violation.0, violation.1) } else { Ok(()) };
                }
            }
        }
        if (self.borrow().is_some_and(Borrow::unresolved)
            || matches!(self, Self::Unborrowed { resource, borrow, .. } if !borrow.empty() && resource.unresolved())) && applies()
        {
            return source.fail(match self{Self::Unborrowed{span,..}|Self::Return{span,..}|Self::Address{span,..}|Self::Memory{span,..}|Self::Dataset{span,..}|Self::Initialized{span,..}|Self::Nonzero{span,..}=>*span},"B271","view的实际借用来源尚未形成合同");
        }
        if let Self::Initialized{read,code,span,..}=self {
            if !read.complete(){return source.fail(*span,code,"读取或返回前没有完整初始化原位置的所需分量");}
        }
        match self {
            Self::Return{borrow,span,..} if borrow.has_local()=>source.fail(*span,"B227","地址不能返回或输出当前函数的局部存储"),
            Self::Unborrowed{resource,borrow,kind,span} => {
                let held=match kind {
                    Exclusion::Position(access) => return if address::MemoryEffect::access_conflicts(borrow,resource,*access,&bounds::Clause::default()) {
                        source.fail(*span,"B227","访问与运行中任务持有的位置权限冲突")
                    } else { Ok(()) },
                    Exclusion::Ownership => borrow.owners(),
                    Exclusion::Resource => borrow.resources(),
                    Exclusion::View => borrow.view_resources(),
                };
                if held.sources.entries().any(|(source,when)|
                    matches!(source,Source::Data(_)|Source::Element{..}) && resource.resources().overlaps_source(source,&when))
                {
                    source.fail(*span,"B227","dataset仍被实际view消费者借用，不能写入、移动、传输或释放")
                } else { Ok(()) }
            },
            Self::Return{borrow,allowed,owned,span} if !owned.pending()&&borrow.owners().sources.entries().any(|(source,when)|matches!(source,Source::Data(id)if !allowed.contains(id)&&when.overlaps(&owned.owners().sources.when_absent(source))))=>source.fail(*span,"B227","返回view不能借用当前函数将释放的dataset"),
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
    // The address value held by one input storage component, not its location owner.
    Contents { origin:Arc<address::Origin>, word:u32 },
    Relation(Arc<view_contracts::relation::Graph>,usize),
    Task { captures:Box<Borrow>, outputs:Vec<Borrow>, initialized:Box<Borrow>, guards:Vec<outputs::TaskGuard>, completions:Vec<TaskInputCompletion> },
    TaskOutput { task:Box<Borrow>, projection:TaskProjection },
}
fn identity_proof(id:ValueIdentity)->bounds::Proof {
    bounds::Proof::default().with_identity(id)
}

impl Source {
    pub(in crate::check) fn index_substitute(index:Option<bounds::Operand>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Operand>{
        match index{
            Some(index@bounds::Operand::Value(id))if values.contains_key(&id)=>index.at_call(values),
            other=>other,
        }
    }
    pub(in crate::check) fn index_at_call(index:Option<bounds::Operand>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Operand>{
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
            Self::Relation(graph,root)=>Self::Relation(Arc::new(graph.output_bound(bindings)),root),
            Self::Contents{origin,word}=>Self::Contents{origin:Arc::new(origin.bind_outputs(bindings)),word},
            Self::Element{parent,fields,index}=>Self::Element{parent:Box::new(parent.output_bound(bindings)),fields,index:index.and_then(|index|index.map(|id|Some(bindings.get(&id).copied().unwrap_or(id))))},
            Self::Projection{parent,fields}=>Self::Projection{parent:Box::new(parent.output_bound(bindings)),fields},
            Self::Task{captures,outputs,initialized,guards,completions}=>Self::Task{
                captures:Box::new(captures.output_bound(bindings)),
                initialized:Box::new(initialized.output_bound(bindings)),
                outputs:outputs.into_iter().map(|output|output.output_bound(bindings)).collect(),
                completions:completions.iter().map(|completion|completion.map_task(|task|task.output_bound(bindings))).collect(),
                guards:guards.into_iter().map(|mut guard| {
                    guard.target=guard.target.output_bound(bindings);
                    let actual=bindings.iter().map(|(from,to)|(*from,bounds::Proof::default().with_identity(*to))).collect();
                    guard.map_arguments(|proof|proof.at_call(&actual));
                    guard.map_inputs(|word|word.map(|id|bindings.get(&id).copied()));
                    guard
                }).collect(),
            },
            Self::TaskOutput{task,projection}=>Self::TaskOutput{task:Box::new(task.output_bound(bindings)),projection:projection.map_result(|id|Some(bindings.get(&id).copied().unwrap_or(id)))},
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
            Self::Relation(graph,root)=>Borrow::source(Self::Relation(Arc::new(graph.at_call(bindings)),root)),
            Self::Contents{origin,word}=>Borrow::source(Self::Contents{origin:Arc::new(origin.bound_at_call(bindings)),word}),
            Self::Element{parent,fields,index}=>Borrow::element(parent.at_call(bindings),fields,Self::index_at_call(index,bindings)),
            Self::Projection{parent,fields}=>Borrow::projection(parent.at_call(bindings),fields),
            Self::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Self::Task{
                captures:Box::new(captures.at_call(bindings)),
                initialized:Box::new(initialized.initialization_at_call(bindings).map_sources(|source|source.at_call(bindings))),
                outputs:outputs.into_iter().map(|output|output.at_call(bindings)).collect(),
                completions:completions.iter().map(|completion|completion.map_task(|task|task.at_call(bindings))).collect(),
                guards:guards.into_iter().map(|mut guard| {
                    guard.target=guard.target.at_call(bindings);
                    guard.map_arguments(|proof|proof.at_call(bindings));
                    guard.map_inputs(|word|word.at_call(bindings));
                    guard
                }).collect(),
            }),
            Self::TaskOutput{task,projection}=>task.at_call(bindings).task_projection(projection.map_result(|id|match id {
                ValueIdentity::TaskWorkCompletion(..)=>Some(id),
                _=>bindings.get(&id)?.known_identity(0),
            })),
            Self::Unresolved|Self::Inactive|Self::External=>Borrow::from_sources(BTreeSet::from([self])),
        }
    }
}
impl crate::module::codec::Encode for Source {
    fn encode(&self,w:&mut crate::module::codec::Writer){
        match self {
            Self::Data(id)=>{0u32.encode(w);id.encode(w);},
            Self::Parameter(id,indices)=>{1u32.encode(w);id.encode(w);indices.encode(w);},
            Self::Unresolved=>2u32.encode(w),Self::Inactive=>3u32.encode(w),Self::External=>4u32.encode(w),
            Self::Position(origin)=>{5u32.encode(w);origin.encode(w);},
            Self::Element{parent,fields,index}=>{6u32.encode(w);parent.encode(w);fields.encode(w);index.encode(w);},
            Self::Projection{parent,fields}=>{7u32.encode(w);parent.encode(w);fields.encode(w);},
            Self::Contents{origin,word}=>{8u32.encode(w);origin.encode(w);word.encode(w);},
            Self::Relation(graph,root)=>{9u32.encode(w);graph.selected(*root).encode(w);},
            Self::Task{captures,outputs,initialized,guards,completions}=>{
                10u32.encode(w);captures.encode(w);outputs.encode(w);initialized.encode(w);guards.encode(w);completions.encode(w);
            },
            Self::TaskOutput{task,projection}=>{11u32.encode(w);task.encode(w);projection.encode(w);},
        }
    }
}
impl crate::module::codec::Decode for Source {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{
        use crate::module::codec::Decode;
        Ok(match u32::decode(r)? {
            0=>Self::Data(Decode::decode(r)?),1=>Self::Parameter(Decode::decode(r)?,Decode::decode(r)?),
            2=>Self::Unresolved,3=>Self::Inactive,4=>Self::External,5=>Self::Position(Decode::decode(r)?),
            6=>Self::Element{parent:Decode::decode(r)?,fields:Decode::decode(r)?,index:Decode::decode(r)?},
            7=>Self::Projection{parent:Decode::decode(r)?,fields:Decode::decode(r)?},
            8=>Self::Contents{origin:Decode::decode(r)?,word:Decode::decode(r)?},
            9=>Self::Relation(Arc::new(Decode::decode(r)?),0),
            10=>Self::Task{captures:Decode::decode(r)?,outputs:Decode::decode(r)?,initialized:Decode::decode(r)?,
                guards:Decode::decode(r)?,completions:Decode::decode(r)?},
            11=>Self::TaskOutput{task:Decode::decode(r)?,projection:Decode::decode(r)?},
            _=>return Err("模块枚举编码错误：Source".into()),
        })
    }
}
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
        if !ty.owns_resources(){return;}
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
        self.check_view_requirement(Requirement::Unborrowed{resource:resource.clone(),borrow,kind:Exclusion::Resource,span})
    }
    pub(super) fn task_places(&self)->Borrow {
        let mut writes=Borrow::default();
        for binding in self.env.values() {
            if let Binding::OwnedValue(value)=binding {
                value.cleanup_views(&BTreeSet::new(),&mut writes,&|_,field,borrow| {
                    if field.task_result().is_some() { borrow.task_projection(TaskProjection::Places) }
                    else { Borrow::default() }
                });
            }
        }
        writes
    }
    pub(super) fn require_place_access(&mut self,place:&Place,access:Access,span:Span)->Result<()> {
        if self.application_entry() && !matches!(place.storage(),Place::AtomicParameter(_)|Place::TileElement{..}) {
            let borrow=self.task_places();
            if !borrow.empty() {
                let resource=self.position_borrows(place,span)?.map_sources(|source|match source {
                    Source::Position(origin)=>origin.normalize(),
                    source=>Borrow::source(source),
                });
                self.check_view_requirement(Requirement::Unborrowed{resource,borrow,kind:Exclusion::Position(access),span})?;
            }
        }
        if access.writes() && let Place::ApplicationElement{resource,..}=place.storage() {
            let borrow=self.view_uses.all(self,&[])?;
            self.check_view_requirement(Requirement::Unborrowed{
                resource:resource.clone(),borrow,kind:Exclusion::View,span,
            })?;
        }
        Ok(())
    }
    pub(super) fn check_view_requirement(&mut self,requirement:Requirement)->Result<()> {
        if requirement.pending()||matches!(&requirement,Requirement::Initialized{..}|Requirement::Memory{..}) {
            self.view_model.uses.push(view_contracts::Use{requirement,source:self.scope.source().clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect()});
            return Ok(());
        }
        if requirement.borrow().is_some_and(Borrow::empty){return Ok(());}
        requirement.under_path(&self.path).check(self.scope.source(), None)?;
        if matches!(&requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Memory{..}|Requirement::Dataset{..}){self.view_model.uses.push(view_contracts::Use{requirement,source:self.scope.source().clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect()});}
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
                        present&&!when.overlaps(&actual.sources.when_absent(source))
                    });
                    if held{continue;}
                }
                match source{
                    Source::Data(id)=>builder.application_handle_available(*id,span,resources,&when)?,
                    Source::Element{parent,..}|Source::Projection{parent,..}=>available(builder,parent.restricted(&when),resources,span)?,
                    Source::Position(origin)=>available(builder,origin.owners().restricted(&when),resources,span)?,
                    Source::Task{captures,..}=>available(builder,captures.restricted(&when),resources,span)?,
                    Source::TaskOutput{task,..}=>available(builder,task.owners().restricted(&when),resources,span)?,
                    Source::Unresolved=>return builder.fail(span,"B271","view返回的实际借用来源尚未形成合同"),
                    Source::Parameter(..)|Source::Contents{..}|Source::Relation(..)|Source::Inactive|Source::External=>{},
                }
            }
            Ok(())
        }
        available(self,value.proof.typed_view_borrow(&value.ty).restricted(&self.path),resources,span)
    }
    pub(super) fn require_view_return_proof(&mut self,proof:&bounds::Proof,owned:Borrow,span:Span)->Result<()> {
        self.require_return_borrow(proof.borrowed_view(),owned,span)
    }
    pub(super) fn require_return_borrow(&mut self,borrow:Borrow,owned:Borrow,span:Span)->Result<()> {
        let mut allowed=self.env.values().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Borrowed).flat_map(|data|std::iter::once(data.root.identity()).chain(data.root.proof.borrowed_views().into_iter().filter_map(|source|match source {Source::Data(id)=>Some(id),_=>None}))).collect::<BTreeSet<_>>();
        for param in self.scope.parameters() {if matches!(param.access(),Access::Write|Access::InOut) && let Some(Binding::OwnedValue(value))=self.env.get(param.name()){for data in value.resources(){if data.ownership!=application::Ownership::Moved{allowed.insert(data.root.identity());allowed.extend(data.root.proof.borrowed_views().into_iter().filter_map(|source|match source{Source::Data(id)=>Some(id),_=>None}));}}}}
        for(index,param)in self.scope.parameters().enumerate(){if param.access()==Access::Read && let Some(Binding::OwnedValue(value))=self.env.get(param.name()){for resource in 0..resource_paths(&value.value.ty).len(){allowed.insert(ValueIdentity::ResourceParameter(index,resource));}}}
        self.check_view_requirement(Requirement::Return{borrow,allowed,owned,span})
    }
    pub(super) fn create_application_view(
        &mut self,
        explicit: &[GenericArgument],
        args: &Expressions,
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
            self.scope.source(),
            span,
            self.scope.module(),
            self.scope.imports(),
        )?;
        let mut value = if let Some(data) = self.application_reference(&args.at(0))? {
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
            let Some(value) = self.expr(&args.at(0))? else { return Ok(None); };
            self.require_same(&ty, &value.ty, args.at(0).span())?;
            value
        };
        if args.len() == 3 {
            let held = self.hold_binding(Binding::Value(value.clone()));
            let evaluated = args.slice(1..).iter().map(|arg| self.expr(&arg)).collect::<Result<Option<Vec<_>>>>();
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
                let data = self.application_place_reference(place, base.span())?;
                return self.application_index(data, index);
            }
            if place.ty().indexed_element().is_some() {
                return self.array_place(place, index);
            }
            self.read_place(&place, base.span(), None)?
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
                    let Some(value) = self.returned_expression(returned, base.span())? else {
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
        self.view_value_at(view,index.span(),|builder|builder.expr(index))
    }
    pub(super) fn view_value_at(&mut self,view:Value,span:Span,
        read_index:impl FnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Place>> {
        if view.ty.view.is_none() {
            return self.fail(span, "B136", "索引需要 dataset、tile 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = read_index(self);
        self.env.remove(&held);
        let Some(value) = evaluated? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, span)?;
        let length=Self::view_length(&view);
        if !self.index_in(
            &value,
            IndexDomain::View(view.view_identity()),
            length.minimum(),
            Some(&length.proof),
        ) {
            return self.fail(
                span,
                "B137",
                "view 索引有效性无法静态证明；使用该引用值的长度条件或有界游标",
            );
        }
        Ok(Some(Self::view_element(&view, value.regs[0])))
    }
}
