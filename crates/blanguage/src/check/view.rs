//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
use std::ops::ControlFlow;
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
        for (key, when) in &other.0 {
            self.union_when(key.clone(), when);
        }
    }

    pub(in crate::check) fn union_restricted(&mut self, other: &Self, path: &bounds::Clause) {
        for (key, when) in &other.0 {
            self.union_when(key.clone(), &bounds::Clause::both(when, path));
        }
    }

    pub(in crate::check) fn union_when(&mut self, key: K, when: &bounds::Clause) {
        use std::collections::btree_map::Entry;
        match self.0.entry(key) {
            Entry::Vacant(entry) => { entry.insert(when.clone()); }
            Entry::Occupied(mut entry) => {
                let joined = bounds::Clause::either(entry.get(), when);
                entry.insert(joined);
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
    pub(super) fn retain_paths(&mut self, possible: impl Fn(&bounds::Clause) -> bool) {
        self.0.retain(|_, when| possible(when));
    }
    pub fn contains(&self, key: &K) -> bool { self.0.get(key).is_some_and(bounds::Clause::possible) }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
    pub fn iter(&self) -> std::collections::btree_map::Keys<'_, K, bounds::Clause> { self.0.keys() }
    pub(super) fn paths(&self) -> impl Iterator<Item = &bounds::Clause> { self.0.values() }
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

    fn with_paths(&self, paths: &mut impl Iterator<Item = bounds::Clause>) -> Self {
        Self(self.0.keys().cloned().zip(paths).collect())
    }

    fn output_bound(&self, bindings: &bounds::OutputBindings) -> Self {
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
#[repr(C)]
struct LoopBorrowEdge {
    level: usize,
    input: usize,
    present: u32,
}

#[repr(C)]
struct LoopBorrowHost {
    inspect: unsafe extern "C" fn(*mut std::ffi::c_void) -> LoopBorrowEdge,
    source: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> *const Borrow,
    replace: unsafe extern "C" fn(*mut std::ffi::c_void, *const Borrow),
    retain: unsafe extern "C" fn(*mut std::ffi::c_void),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::project_loop_borrow"]
    fn source_project_loop_borrow(context: *mut std::ffi::c_void, scope: usize, host: LoopBorrowHost);
}

impl Borrow {
    pub(super) fn project_loop_inputs<'a>(
        mut self, scope: usize, source: impl Fn(usize) -> Option<&'a Borrow>,
    ) -> Self {
        use std::{ffi::c_void, iter::Peekable, ptr};

        struct Projection<I: Iterator, F> {
            inputs: Peekable<I>,
            result: Borrow,
            source: F,
        }
        type Input = (LoopInput, bounds::Clause);

        fn project<'a, I: Iterator<Item = Input>, F: Fn(usize) -> Option<&'a Borrow>>(
            inputs: I, result: Borrow, scope: usize, source: F,
        ) -> Borrow {
            unsafe extern "C" fn inspect<I: Iterator<Item = Input>, F>(raw: *mut c_void) -> LoopBorrowEdge {
                let state = unsafe { &mut *raw.cast::<Projection<I, F>>() };
                match state.inputs.peek() {
                    Some((input, _)) => LoopBorrowEdge { level: input.level, input: input.index, present: 1 },
                    None => LoopBorrowEdge { level: 0, input: 0, present: 0 },
                }
            }
            unsafe extern "C" fn source_at<'a, I: Iterator<Item = Input>, F: Fn(usize) -> Option<&'a Borrow>>(
                raw: *mut c_void, index: usize,
            ) -> *const Borrow {
                let state = unsafe { &*raw.cast::<Projection<I, F>>() };
                (state.source)(index).map_or(ptr::null(), ptr::from_ref)
            }
            unsafe extern "C" fn replace<I: Iterator<Item = Input>, F>(raw: *mut c_void, source: *const Borrow) {
                let state = unsafe { &mut *raw.cast::<Projection<I, F>>() };
                // A replace or retain consumes exactly the edge exposed by inspect.
                let (_, when) = unsafe { state.inputs.next().unwrap_unchecked() };
                state.result.union_restricted(unsafe { &*source }, &when);
            }
            unsafe extern "C" fn retain<I: Iterator<Item = Input>, F>(raw: *mut c_void) {
                let state = unsafe { &mut *raw.cast::<Projection<I, F>>() };
                let (input, when) = unsafe { state.inputs.next().unwrap_unchecked() };
                state.result.inputs.insert_when(input, when);
            }

            let mut state = Projection { inputs: inputs.peekable(), result, source };
            unsafe {
                source_project_loop_borrow(ptr::from_mut(&mut state).cast(), scope, LoopBorrowHost {
                    inspect: inspect::<I, F>, source: source_at::<I, F>,
                    replace: replace::<I, F>, retain: retain::<I, F>,
                });
            }
            state.result
        }

        let inputs = std::mem::take(&mut self.inputs).into_entries();
        project(inputs, self, scope, source)
    }

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
                Source::Element{parent,fields,index}=>Self::selected_element(parent.substitute_values(values),fields.clone(),Source::index_substitute(*index,values)),
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
            result.union_restricted(&next,&when.substitute_values(values));
        }
        result
    }

    pub fn from_sources(sources:impl IntoIterator<Item=Source>)->Self{Self{sources:sources.into_iter().collect(),..Self::default()}}
    pub fn source(source:Source)->Self{Self::from_sources([source])}
    pub fn element(parent:Self,fields:Vec<usize>,index:Option<bounds::Operand>)->Self {
        Self::selected_element(parent, fields, index.map(bounds::Selector::Direct))
    }

    pub(super) fn selected_element(parent:Self,fields:Vec<usize>,index:Option<bounds::Selector>)->Self {
        let mut result=Self::default();
        for (source, when) in parent.sources.into_entries() {
            let value = match source {
                Source::Projection { parent, fields: mut prefix } => {
                    prefix.extend_from_slice(&fields);
                    Self::selected_element(*parent, prefix, index)
                }
                source => Self::source(Source::Element {
                    parent: Box::new(Self::source(source)), fields: fields.clone(), index,
                }),
            };
            result.union_restricted(&value,&when);
        }
        if !parent.inputs.is_empty()||!parent.calls.is_empty()||!parent.reads.is_empty(){
            result.sources.insert(Source::Element{parent:Box::new(Self{inputs:parent.inputs,calls:parent.calls,reads:parent.reads,..Self::default()}),fields,index});
        }
        result
    }
    // This projects a common input family; existing caller identities are not rebound.
    pub(super) fn select_elements(&self,indices:&[Option<bounds::Selector>])->Self {
        if indices.is_empty(){return self.clone();}
        let values=indices.iter().enumerate().map(|(at,index)|{
            let id=ValueIdentity::ElementIndex(indices.len()-1-at);
            let proof=index.and_then(bounds::Selector::operand).map(|index|bounds::Proof::element_operand(index,(0,u32::MAX))).unwrap_or_default();
            (id,proof)
        }).collect::<BTreeMap<_,_>>();
        let select = |index: Option<bounds::Selector>| {
            if let Some(bounds::Selector::Direct(bounds::Operand::Value(ValueIdentity::ElementIndex(depth)))) = index
                && depth < indices.len()
            {
                return indices[indices.len() - 1 - depth];
            }
            Source::index_substitute(index, &values)
        };
        let mut out=Self{inputs:self.inputs.substitute_values(&values),calls:self.calls.substitute_values(&values),reads:self.reads.substitute_values(&values),..Self::default()};
        for(source,when)in self.sources.entries(){
            let value=match source{
                Source::Element{parent,fields,index}=>Self::selected_element(parent.select_elements(indices),fields.clone(),select(*index)),
                Source::Relation(graph,root)=>graph.select(*root,indices),
                Source::Parameter(id,selected)=>{
                    let selected=if matches!(id,ValueIdentity::DatasetContent(..))&&selected.is_empty(){indices.to_vec()}
                        else{selected.iter().copied().map(select).collect()};
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
            out.union_restricted(&value,&when.substitute_values(&values));
        }
        out
    }
    pub(super) fn overlaps_source(&self,source:&Source,when:&bounds::Clause)->bool {
        self.sources.entries().any(|(other,active)|{
            source.overlaps(other,||bounds::Clause::both(when,&active))
        })
    }
    pub fn projection(parent:Self,fields:Vec<usize>)->Self {
        if fields.is_empty(){return parent;}
        let mut result=Self::default();
        for (source, when) in parent.sources.into_entries() {
            let projected = match source {
                Source::Projection { parent, fields: mut prefix } => {
                    prefix.extend_from_slice(&fields);
                    Source::Projection { parent, fields: prefix }
                }
                source => Source::Projection {
                    parent: Box::new(Self::source(source)), fields: fields.clone(),
                },
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
            result.union_restricted(&value,&when);
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
    pub(super) fn union_restricted(&mut self, other: &Self, path: &bounds::Clause) {
        self.sources.union_restricted(&other.sources, path);
        self.inputs.union_restricted(&other.inputs, path);
        self.calls.union_restricted(&other.calls, path);
        self.reads.union_restricted(&other.reads, path);
    }

    pub fn joined(mut self,other:Self)->Self{self.union(&other);self}
    pub(super) fn reads_returned_at(&self, memory: usize) -> bool {
        self.reads.0.contains_key(&view_contracts::ContentRead::Returned { memory })
    }

    pub fn pending(&self) -> bool {
        !self.inputs.is_empty() || !self.calls.is_empty() || !self.reads.is_empty()
            || self.sources.iter().any(Source::pending)
    }

    pub(in crate::check) fn pending_in_path(&self, path: &bounds::Clause) -> bool {
        fn present<K: Ord>(edges: &Edges<K>, path: &bounds::Clause) -> bool {
            edges.0.values().any(|when| bounds::Clause::both(path, when).possible())
        }
        present(&self.inputs, path) || present(&self.calls, path) || present(&self.reads, path)
            || self.sources.0.iter().any(|(source, when)| {
                bounds::Clause::both(path, when).possible() && source.pending()
            })
    }
    pub fn active(&self)->bool{self.sources.entries().any(|(_,when)|when.possible())||self.inputs.entries().any(|(_,when)|when.possible())||self.calls.entries().any(|(_,when)|when.possible())||self.reads.entries().any(|(_,when)|when.possible())}
    pub(in crate::check) fn restricted_by(mut self, condition: &bounds::Clause) -> Self {
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
    pub fn selected(self,choice:&bounds::Condition,b:&Self)->Self{
        // Equal complete borrows need no conditional path construction.
        if &self==b{return self;}
        let mut selected = self.restricted_by(&choice.assume(&bounds::Clause::default(), true));
        selected.union_restricted(b, &choice.assume(&bounds::Clause::default(), false));
        selected
    }
    fn initialization_guards(&self, lookup:impl Fn(ValueIdentity)->Option<bounds::Proof>)->Self {
        let mut result=self.clone();
        for when in result.sources.0.values_mut() {
            let condition = bounds::Condition::guard(when.clone());
            let bindings=condition.scalar_identities().into_iter()
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

    pub fn guards_at_call(mut self, bindings: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        let paths = self.guard_paths_mut().collect::<Vec<_>>();
        let projected = bounds::Clause::project_guard_paths(paths.iter().map(|path| &**path), bindings);
        for (path, result) in paths.into_iter().zip(projected) {
            *path = result;
        }
        self
    }

    fn guard_paths_mut(&mut self) -> impl Iterator<Item = &mut bounds::Clause> {
        self.sources.0.values_mut().chain(self.inputs.0.values_mut())
            .chain(self.calls.0.values_mut()).chain(self.reads.0.values_mut())
    }

    pub(super) fn project_guards(mut self, guards: &bounds::PublicGuards<'_>) -> Self {
        for condition in self.guard_paths_mut() {
            *condition = guards.project(condition);
        }
        self
    }

    pub(super) fn map_sources_at_call(
        &self, bindings: &BTreeMap<ValueIdentity, bounds::Proof>, map: impl Fn(Source, &bounds::Clause) -> Self,
    ) -> Self {
        let roots = self.inputs.paths().chain(self.calls.paths())
            .chain(self.reads.paths()).chain(self.sources.paths());
        let mut projected = bounds::Clause::project_guard_paths(roots, bindings).into_iter();
        let mut result = Self {
            inputs: self.inputs.with_paths(&mut projected),
            calls: self.calls.with_paths(&mut projected),
            reads: self.reads.with_paths(&mut projected),
            ..Self::default()
        };
        for ((source, original), path) in self.sources.0.iter().zip(projected) {
            result.union_restricted(&map(source.clone(), original), &path);
        }
        result
    }
    pub(super) fn bind_guards(&self,bindings:&bounds::OutputBindings)->Self {
        Self{sources:self.sources.output_bound(bindings),inputs:self.inputs.output_bound(bindings),
            calls:self.calls.output_bound(bindings),reads:self.reads.output_bound(bindings)}
    }
    pub(in crate::check) fn retain_sources(&mut self, keep: impl Fn(&Source) -> bool) {
        self.sources.0.retain(|source, _| keep(source));
    }

    pub fn map_sources(&self, map: impl Fn(Source) -> Self) -> Self {
        self.map_source_paths(|source,_| map(source))
    }

    pub(super) fn map_source_paths(&self, map: impl Fn(Source, &bounds::Clause) -> Self) -> Self {
        let mut result = Self {
            inputs: self.inputs.clone(), calls: self.calls.clone(),
            reads: self.reads.clone(), ..Self::default()
        };
        for (source, condition) in &self.sources.0 {
            result.union_restricted(&map(source.clone(), condition), condition);
        }
        result
    }
    pub fn unresolved(&self) -> bool {
        let root = std::cell::LazyCell::new(bounds::Clause::default);
        self.unresolved_in(&|| Some(&*root))
    }

    pub(super) fn unresolved_under(&self, path: &bounds::Clause) -> bool {
        self.unresolved_in(&|| Some(path))
    }

    fn unresolved_in<'a>(
        &self,
        reachable_path: &dyn Fn() -> Option<&'a bounds::Clause>,
    ) -> bool {
        self.any_source_in(reachable_path, Self::source_unresolved)
    }

    fn any_source_in<'a>(
        &self,
        reachable_path: &dyn Fn() -> Option<&'a bounds::Clause>,
        query: impl for<'path> Fn(&Source, &dyn Fn() -> Option<&'path bounds::Clause>) -> bool,
    ) -> bool {
        self.sources.0.iter().any(|(source, when)| {
            let path = std::cell::LazyCell::new(|| {
                let path = reachable_path()?;
                let when = bounds::Clause::both(path, when);
                when.possible().then_some(when)
            });
            query(source, &|| path.as_ref())
        })
    }

    pub(super) fn source_unresolved<'a>(
        source: &Source,
        reachable: &dyn Fn() -> Option<&'a bounds::Clause>,
    ) -> bool {
        match source {
            Source::Unresolved => reachable().is_some(),
            Source::Relation(graph, root) =>
                graph.unresolved(*root) && reachable().is_some(),
            Source::Element { parent, .. } | Source::Projection { parent, .. } =>
                parent.unresolved_in(reachable),
            Source::Task { captures, outputs, initialized, .. } =>
                captures.unresolved_in(reachable)
                    || initialized.unresolved_in(reachable)
                    || outputs.iter().any(|output| output.unresolved_in(reachable)),
            Source::TaskOutput { task, .. } => task.unresolved_in(reachable),
            Source::Contents { origin, .. } =>
                (origin.has_local() || origin.resources().unresolved()) && reachable().is_some(),
            Source::Position(origin) => match &origin.root {
                address::Root::Resource { borrow, .. } | address::Root::Derived(borrow)
                | address::Root::Dataset { borrow, .. } => borrow.unresolved_in(reachable),
                _ => false,
            },
            _ => false,
        }
    }

    pub fn has_local(&self) -> bool {
        let root = std::cell::LazyCell::new(bounds::Clause::default);
        self.has_local_in(&|| Some(&*root))
    }

    fn has_local_in<'a>(
        &self,
        reachable_path: &dyn Fn() -> Option<&'a bounds::Clause>,
    ) -> bool {
        self.any_source_in(reachable_path, Self::source_has_local_in)
    }

    pub(in crate::check) fn source_has_local(source: &Source) -> bool {
        let root = std::cell::LazyCell::new(bounds::Clause::default);
        Self::source_has_local_in(source, &|| Some(&*root))
    }

    fn source_has_local_in<'a>(
        source: &Source,
        reachable: &dyn Fn() -> Option<&'a bounds::Clause>,
    ) -> bool {
        match source {
            Source::Relation(graph, root) => graph.has_local(*root) && reachable().is_some(),
            Source::Position(origin) => match &origin.root {
                address::Root::Derived(parent) | address::Root::Dataset { borrow: parent, .. } =>
                    parent.has_local_in(reachable),
                _ => origin.has_local() && reachable().is_some(),
            },
            Source::Element { parent, .. } | Source::Projection { parent, .. }
            | Source::TaskOutput { task: parent, .. } => parent.has_local_in(reachable),
            Source::Task { captures, outputs, initialized, .. } =>
                captures.has_local_in(reachable)
                    || initialized.has_local_in(reachable)
                    || outputs.iter().any(|output| output.has_local_in(reachable)),
            _ => false,
        }
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
            Source::Element{parent,fields,index}=>Self::selected_element(parent.resources(),fields,index),
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
            Source::Element{parent,fields,index}=>Self::selected_element(parent.resources(),fields,index).joined(parent.owners()),
            Source::Projection{parent,..}=>parent.owners(),
            Source::Task{captures,..}=>captures.owners(),
            Source::TaskOutput{task,..}=>task.owners(),
            source=>Self::source(source),
        })
    }
    pub(super) fn aliases(&self, needed: &dyn Fn(&MemoryAlias) -> bool) -> Vec<MemoryAlias> {
        let mut aliases = BTreeSet::new();
        for (source, when) in &self.sources.0 {
            if let Source::Position(origin) = source {
                let selected = origin.aliases_when(
                    &|alias| !aliases.contains(alias) && needed(alias),
                    || when.possible(),
                );
                aliases.extend(selected);
            }
        }
        aliases.into_iter().collect()
    }
    pub fn at_call(&self, bindings: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        self.map_sources_at_call(bindings, |source, _| source.at_call(bindings))
    }
    pub fn output_bound(&self,bindings:&bounds::OutputBindings)->Self{
        let mut result=Self{inputs:self.inputs.output_bound(bindings),calls:self.calls.output_bound(bindings),reads:self.reads.output_bound(bindings),..Self::default()};
        for(source,when)in self.sources.entries(){result.sources.insert_when(source.clone().bind_outputs(bindings),when.output_bound(bindings));}result
    }
}


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
fn implicit_resource_source(data: &application::Data) -> Option<Source> {
    let id = data.root.identity();
    (data.root.proof.view_borrow_ref(0).is_none_or(Borrow::empty)
        || matches!(id, ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..)))
        .then_some(Source::Data(id))
}

pub(super) fn resource_borrow_ref(data: &application::Data) -> std::borrow::Cow<'_, Borrow> {
    use std::borrow::Cow;
    match implicit_resource_source(data) {
        Some(source) => {
            let mut value = data.root.proof.view_borrow(0);
            value.sources.insert(source);
            Cow::Owned(value)
        }
        None => Cow::Borrowed(data.root.proof.view_borrow_ref(0).unwrap()),
    }
}

pub(super) fn resource_borrow(data: &application::Data) -> Borrow {
    resource_borrow_ref(data).into_owned()
}

pub(super) fn take_resource_borrow(data: &mut application::Data) -> Borrow {
    let source = implicit_resource_source(data);
    let mut value = data.root.proof.take_view_borrow(0);
    if let Some(source) = source { value.sources.insert(source); }
    value
}
pub(super) fn resource_proof(data:&application::Data)->bounds::Proof{{let mut component_proof=data.root.proof.borrow_state();component_proof.set_view_borrow(0,resource_borrow(data));component_proof}}

pub(super) enum InputSlot {
    Value {name:String,contents:Option<Vec<usize>>,component:usize,identity:Option<(ValueIdentity,Scalar)>},
    Data {name:String,path:Vec<usize>},
}
impl InputSlot {
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
pub(super) fn visit_data(binding: &Binding, mut visit: impl FnMut(&application::Data)) {
    let mut active = |data: &application::Data| {
        if data.ownership != application::Ownership::Moved {
            visit(data);
        }
    };
    match binding {
        Binding::ApplicationData(data) => active(data),
        Binding::OwnedValue(value) => value.visit_resources(&mut active),
        _ => {}
    }
}

pub(super) fn visit_data_mut(binding: &mut Binding, mut visit: impl FnMut(&mut application::Data)) {
    let mut active = |data: &mut application::Data| {
        if data.ownership != application::Ownership::Moved {
            visit(data);
        }
    };
    match binding {
        Binding::ApplicationData(data) => active(data),
        Binding::OwnedValue(value) => value.resources_mut(&mut active),
        _ => {}
    }
}

pub(super) fn visit_data_paths(
    binding: &Binding, mut visit: impl FnMut(&[usize], &application::Data, bool) -> ControlFlow<()>,
) -> ControlFlow<()> {
    match binding {
        Binding::ApplicationData(data) if data.ownership != application::Ownership::Moved =>
            visit(&[], data, data.ownership == application::Ownership::Owned),
        Binding::OwnedValue(value) => value.visit_live_paths(visit),
        _ => ControlFlow::Continue(()),
    }
}

pub(super) fn for_each_data_path(
    binding: &Binding, mut visit: impl FnMut(&[usize], &application::Data, bool),
) {
    let _ = visit_data_paths(binding, |path, data, independent| {
        visit(path, data, independent);
        ControlFlow::Continue(())
    });
}
pub(super) fn data_at<'a>(binding: &'a Binding, path: &[usize]) -> Option<&'a application::Data> {
    match binding {
        Binding::ApplicationData(data) if path.is_empty() && data.ownership != application::Ownership::Moved => Some(data),
        Binding::OwnedValue(value) => owned_values::live_data_at(value, path),
        _ => None,
    }
}

pub(super) fn data_at_mut<'a>(binding: &'a mut Binding, path: &[usize]) -> Option<&'a mut application::Data> {
    match binding {
        Binding::ApplicationData(data) if path.is_empty() => Some(data),
        Binding::OwnedValue(value) => owned_data_at_mut(value, path),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Exclusion {
    Resource,
    Ownership,
    Position(Access),
    View,
}


#[derive(Clone)]
pub(super) enum Requirement {
    Unborrowed {resource:Borrow,borrow:Borrow,kind:Exclusion,span:Span},
    Return {borrow:Borrow,allowed:BTreeSet<ValueIdentity>,owned:Borrow,span:Span},
    Address {borrow:Borrow,layout:address::Layout,written:Option<bounds::Proof>,writable:bool,initialized:bool,external_atomic:bool,span:Span},
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
    pub fn mapped(&self, mut map: impl FnMut(&Borrow) -> Borrow) -> Self {
        match self {
            Self::Unborrowed{resource,borrow,kind,span}=>Self::Unborrowed{resource:map(resource),borrow:map(borrow),kind:*kind,span:*span},
            Self::Return{borrow,allowed,owned,span}=>Self::Return{borrow:map(borrow),allowed:allowed.clone(),owned:map(owned),span:*span},
            Self::Address{borrow,layout,written,writable,initialized,external_atomic,span}=>Self::Address{borrow:map(borrow),layout:layout.clone(),written:written.clone(),writable:*writable,initialized:*initialized,external_atomic:*external_atomic,span:*span},
            Self::Memory{target,borrow,access,written,snapshots,address_fields,span}=>Self::Memory{target:*target,borrow:map(borrow),access:*access,written:written.clone(),snapshots:snapshots.clone(),address_fields:address_fields.clone(),span:*span},
            Self::Dataset{borrow,index,access,atomic,values,span}=>Self::Dataset{borrow:map(borrow),index:*index,access:*access,atomic:*atomic,values:values.clone(),span:*span},
            Self::Initialized{borrow,read,code,span}=>Self::Initialized{borrow:map(borrow),read:read.clone(),code,span:*span},
            Self::Nonzero{..}=>self.clone(),
        }
    }
    pub(super) fn unresolved_diagnostic<T>(source: &crate::Source, span: Span) -> Result<T> {
        source.fail(span, "B271", "view的实际借用来源尚未形成合同")
    }

    pub fn check(&self, source: &crate::Source, call_path: Option<&bounds::Clause>) -> Result<()> {
        let applies = || call_path.is_none_or(bounds::Clause::possible);

        if let Self::Nonzero{value,path,span}=self {
            return if bounds::integer_divisors(value,path).is_some(){Ok(())}else{source.fail(*span,"B152",bounds::INTEGER_DIVISOR_MESSAGE)};
        }
        if let Self::Address{borrow,layout,writable,initialized,external_atomic,span,..}=self {
            for (origin_source, when) in borrow.sources.entries() {
                let Source::Position(origin) = origin_source else { continue; };
                let violation = if origin.layout != *layout || !origin.layout_valid() {
                    ("B263", "地址解释的布局与原自有位置不同")
                } else if *external_atomic && origin.has_blink_storage(&when) {
                    ("B214", "外部原子位置不能改变已知 Blink 位置的访问权")
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
            return Self::unresolved_diagnostic(source, match self {
                Self::Unborrowed { span, .. } | Self::Return { span, .. } | Self::Address { span, .. }
                | Self::Memory { span, .. } | Self::Dataset { span, .. } | Self::Initialized { span, .. }
                | Self::Nonzero { span, .. } => *span,
            });
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
                let target = resource.resources();
                if held.sources.entries().any(|(source, when)|
                    matches!(source, Source::Data(_) | Source::Element { .. }) && target.overlaps_source(source, &when))
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
    Element { parent:Box<Borrow>, fields:Vec<usize>, index:Option<bounds::Selector> },
    Projection { parent:Box<Borrow>, fields:Vec<usize> },
    Parameter(ValueIdentity,Vec<Option<bounds::Selector>>),
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
    fn pending(&self) -> bool {
        match self {
            Self::Position(origin) | Self::Contents { origin, .. } => origin.pending(),
            Self::Element { parent, .. } | Self::Projection { parent, .. } => parent.pending(),
            Self::Relation(graph, root) => graph.pending(*root),
            Self::Task { captures, outputs, initialized, .. } => {
                captures.pending() || initialized.pending() || outputs.iter().any(Borrow::pending)
            }
            Self::TaskOutput { task, projection } => {
                !matches!(projection, TaskProjection::Returned(..)) || task.pending()
            }
            _ => false,
        }
    }

    pub(in crate::check) fn index_substitute(index:Option<bounds::Selector>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Selector>{
        index.and_then(|index| {
            if index.identity().is_some_and(|id| values.contains_key(&id)) { index.at_call(values) }
            else { Some(index) }
        })
    }
    pub(in crate::check) fn index_at_call(index:Option<bounds::Selector>,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<bounds::Selector>{
        index.and_then(|index| match index.identity() {
            Some(id @ ValueIdentity::ElementIndex(_)) if !values.contains_key(&id) => Some(index),
            _ => index.at_call(values),
        })
    }
    fn overlaps(&self,other:&Self,path:impl FnOnce()->bounds::Clause)->bool{
        match(self,other){
            (Self::Element{parent,fields,index},Self::Element{parent:other,fields:other_fields,index:other_index})=>{
                if fields!=other_fields{return false;}
                let path=path();
                let overlaps = match (index, other_index) {
                    (Some(a), Some(b)) => a.overlaps(*b, &path),
                    _ => path.possible(),
                };
                if !overlaps { return false; }
                parent.sources.entries().any(|(source,when)|other.overlaps_source(source,&bounds::Clause::both(&path,&when)))
                    ||parent.inputs.entries().any(|(input,when)|bounds::Clause::both(&path,&when).overlaps(&other.inputs.when(input)))
                    ||parent.calls.entries().any(|(call,when)|bounds::Clause::both(&path,&when).overlaps(&other.calls.when(call)))
            },
            // Different input views can still borrow the same external resource.
            (Self::Data(a),Self::Data(b)) | (Self::Parameter(a,_),Self::Parameter(b,_))=>a==b&&path().possible(),
            _=>std::mem::discriminant(self)==std::mem::discriminant(other)&&path().possible()&&self==other,
        }
    }
    pub(super) fn bind_outputs(self, bindings: &bounds::OutputBindings) -> Self {
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
            Self::Element{parent,fields,index}=>Borrow::selected_element(parent.at_call(bindings),fields,Self::index_at_call(index,bindings)),
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
    pub(super) async fn view_borrow(&self,builder:&Builder<'_>)->Result<Borrow> {
        if let Self::DataReference(data)=self{return Ok(resource_borrow(&data.data));}
        if let Self::OwnedValue(value)=self && value.ownership==application::Ownership::Borrowed {
            let mut borrow=value.value.proof.typed_view_borrow(&value.value.ty);
            for data in value.resources(){borrow.union(&data.root.proof.view_borrow(0));}return Ok(borrow);
        }
        if let Self::ViewElement{view,..}=self {return Ok(view.proof.borrowed_view());}
        let mut borrow=self.view_parts().map(|(ty,proof)|proof.typed_view_borrow(ty)).unwrap_or_default();
        unsafe { builder.world.call(user_owned::cleanup_borrow(builder,self,&mut borrow)).await }?;Ok(borrow)
    }

}
pub(super) use crate::ir::{
    dataset_count as resource_count, dataset_path as resource_path, dataset_paths as resource_paths,
};
pub(super) use owned_values::{owned_data_at, owned_data_at_mut};

impl Builder<'_> {
    fn mapped_data_borrow(&self,identity:ValueIdentity)->Option<Borrow> {
        self.loop_contexts.iter().rev().find_map(|context|context.view_resources.get(&identity).cloned())
    }
    pub(super) fn data_borrow(&self,identities:&[ValueIdentity])->Borrow {
        let mut result=Borrow::default();
        for identity in identities {
            if let Some(borrow)=self.mapped_data_borrow(*identity){result.union(&borrow);continue;}
            let mut value=Borrow::default();
            for binding in self.env.values() {
                visit_data(binding, |data| {
                    if data.root.identity() == *identity {
                        value.union(&data.root.proof.view_borrow(0));
                    }
                });
            }
            if value.empty(){value.sources.insert(Source::Data(*identity));}result.union(&value);
        }
        result
    }
    pub(super) fn data_value_borrow(&self,data:&application::Data)->Borrow {resource_borrow(data)}
    pub(super) async fn application_view_borrowed(&self, identity: ValueIdentity) -> Result<bool> {
        for (name,binding) in &self.env {
            if unsafe { self.world.call(self.live_view_borrow(name,binding)).await }?.owners().sources.contains(&Source::Data(identity)) { return Ok(true); }
        }
        Ok(false)
    }
    pub(super) async fn require_view_unborrowed(&mut self,resource:&Borrow,span:Span)->Result<()> {
        let mut borrow=Borrow::default();
        for (name,binding) in &self.env {borrow.union(&unsafe { self.world.call(self.live_view_borrow(name,binding)).await }?);}
        self.check_view_requirement(Requirement::Unborrowed{resource:resource.clone(),borrow,kind:Exclusion::Resource,span})
    }
    pub(super) fn task_places(&self)->Borrow {
        let mut writes=Borrow::default();
        for binding in self.env.values() {
            if let Binding::OwnedValue(value)=binding {
                value.cleanup_views(&mut writes,&|_,field,borrow| {
                    if field.task_result().is_some() { borrow.task_projection(TaskProjection::Places) }
                    else { Borrow::default() }
                });
            }
        }
        writes
    }
    pub(super) async fn require_place_access(&mut self,place:&Place,access:Access,span:Span)->Result<()> {
        if self.application_entry() && !matches!(place.storage(),Place::AtomicParameter(_)|Place::TileElement{..}) {
            let borrow=self.task_places();
            if !borrow.empty() {
                let resource=unsafe { self.world.call(self.position_borrows(place,span)).await }?.map_sources(|source|match source {
                    Source::Position(origin)=>origin.normalize(),
                    source=>Borrow::source(source),
                });
                self.check_view_requirement(Requirement::Unborrowed{resource,borrow,kind:Exclusion::Position(access),span})?;
            }
        }
        if access.writes() && let Place::ApplicationElement{resource,..}=place.storage() {
            let borrow=unsafe { self.world.call(self.view_uses.all(self,&[])).await }?;
            self.check_view_requirement(Requirement::Unborrowed{
                resource:resource.clone(),borrow,kind:Exclusion::View,span,
            })?;
        }
        Ok(())
    }
    pub(super) fn check_view_requirement(&mut self,requirement:Requirement)->Result<()> {
        if requirement.pending()||matches!(&requirement,Requirement::Initialized{..}|Requirement::Memory{..}) {
            self.view_model.uses.push(view_contracts::Use{requirement,source:self.scope.source().clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect(),opaque_write_possible:Default::default()});
            return Ok(());
        }
        if requirement.borrow().is_some_and(Borrow::empty){return Ok(());}
        requirement.under_path(&self.path).check(self.scope.source(), None)?;
        if matches!(&requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Memory{..}|Requirement::Dataset{..}){self.view_model.uses.push(view_contracts::Use{requirement,source:self.scope.source().clone(),path:self.path.clone(),active_loops:self.loop_contexts.iter().filter_map(|context|context.view_scope).collect(),opaque_write_possible:Default::default()});}
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
        for(index,param)in self.scope.parameters().enumerate(){if param.access()==Access::Read && let Some(Binding::OwnedValue(value))=self.env.get(param.name()){for resource in 0..resource_count(&value.value.ty){allowed.insert(ValueIdentity::ResourceParameter(index,resource));}}}
        self.check_view_requirement(Requirement::Return{borrow,allowed,owned,span})
    }
    pub(super) async fn create_application_view(
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
        let ty = unsafe { self.world.call(self.world.ty(
            &name,
            self.scope.source(),
            span,
            self.scope.module(),
            self.scope.imports(),
        )).await }?;
        enum Input {
            Dataset(application::DataReference),
            Value(Value),
        }
        let argument = args.at(0);
        let input = match unsafe { self.world.call(self.application_reference(&argument)).await }? {
            Some(data) => Input::Dataset(data),
            None => match unsafe { self.world.call(self.evaluate_returned(&argument, None)).await }? {
                Some(returns::Returned::Dataset(data)) => {
                    self.hold_binding(Binding::ApplicationData(data.clone()));
                    Input::Dataset(application::DataReference::direct(data))
                }
                returned => {
                    let Some(value) = self.returned_expression(returned, argument.span(), false)? else {
                        return Ok(None);
                    };
                    Input::Value(value)
                }
            },
        };
        let mut value = match input {
            Input::Dataset(data) => {
                if !data.permits(Access::Read) {
                    return self.fail(span, "B214", "原子dataset不能转换为普通只读view");
                }
                let element = ty.view.as_deref().unwrap();
                let actual = data.element_layout().1;
                if !element.same_layout(actual) {
                    return self.fail(span, "B124", format!("类型不匹配：需要 {}，实际为 {}", element.name, actual.name));
                }
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
                proof.set_view_borrow(0, self.data_value_borrow(&data.data));
                Value {
                    proof,
                    literal: false,
                    ty,
                    regs: vec![address[0], address[1], data.length.regs[0],stride_parts[0],stride_parts[1]],
                    uniform: false,
                    constant: None,
                }
            }
            Input::Value(value) => {
                self.require_same(&ty, &value.ty, argument.span())?;
                value
            }
        };
        if args.len() == 3 {
            let held = self.hold_binding(Binding::Value(value.clone()));
            let evaluated=async {
                let mut limits=Vec::new();
                for arg in args.slice(1..).iter() {
                    let Some(value)=unsafe { self.world.call(self.expr(&arg)).await }? else { return Ok(None); };
                    limits.push(value);
                }
                Ok(Some(limits))
            }.await;
            self.env.remove(&held);
            let Some(mut limits) = evaluated? else { return Ok(None); };
            let end = limits.pop().unwrap(); let start = limits.pop().unwrap();
            for limit in [&start, &end] { self.require_same(&Ty::scalar(Scalar::U32), &limit.ty, span)?; }
            let length = Self::view_length(&value);
            for (a, b) in [(&start, &end), (&end, &length)] {
                if !(bounds::Proof::below_length((&a.component_origins(0), &a.proof.domains(0)), (&b.component_origins(0), &b.proof.domains(0)), false) || bounds::Condition::comparison(Comparison::LessEqual, a, b).truth_in(&self.path) == Some(true)) {
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
            value.proof.set_view_borrow(0, borrow);
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
        let mut ty = view.ty.view.as_deref().unwrap().clone();
        ty.complete_value_layout();
        let stride = ElementStride::Value([view.regs[3],view.regs[4]]);
        let words = ty.components.iter().map(|(_, offset)| *offset).collect();
        let element_layout=address::Layout::of(&ty);
        Place::ViewElement {
            ty,
            source: view.view_identity(),
            resource: view.proof.typed_view_borrow(&view.ty),
            element_layout,
            address: [view.regs[0], view.regs[1]],
            index,
            stride,
            words,
        }
    }
    pub(super) async fn view_place(&mut self,base:&Expr,index:&Expr)->Result<Option<Place>> {
        let mut cursor=SourceValueCursor {phase:0};let mut facts=ViewPositionFacts {placed:u32::from(self.is_place(base)),present:0,dataset:0,indexed:0,continuing:0,kind:0,array:0};
        let mut place=None;let mut value=None;let mut returned=None;let mut data=None;
        loop {let step=unsafe {view_position_step(cursor,ViewPositionFacts {..facts})};cursor=step.cursor;match step.need {
            ViewPositionNeed::Parent=>{place=unsafe {self.world.call(self.place(base)).await}?;facts.present=u32::from(place.is_some());},
            ViewPositionNeed::DatasetType=>facts.dataset=u32::from(place.as_ref().unwrap().ty().dataset.is_some()),
            ViewPositionNeed::IndexedType=>facts.indexed=u32::from(place.as_ref().unwrap().ty().indexed_count().is_some()),
            ViewPositionNeed::Reference=>data=Some(unsafe {self.world.call(self.application_place_reference(place.take().unwrap(),base.span())).await}?),
            ViewPositionNeed::ApplicationIndex=>return unsafe {self.world.call(self.application_index(data.take().unwrap(),index)).await},
            ViewPositionNeed::Read=>{value=Some(unsafe {self.world.call(self.read_place(place.as_ref().unwrap(),base.span(),None)).await}?);facts.present=1;},
            ViewPositionNeed::Returned=>{returned=unsafe {self.world.call(self.evaluate_returned(base,None)).await}?;facts.continuing=u32::from(self.flow.continues());},
            ViewPositionNeed::Kind=>facts.kind=match &returned{Some(returns::Returned::Dataset(_))=>1,Some(returns::Returned::OwnedValue(_))=>2,_=>0},
            ViewPositionNeed::OwnedArray=>{let Some(returns::Returned::OwnedValue(owner))=&returned else{unsafe {std::hint::unreachable_unchecked()}};facts.array=u32::from(owner.value.ty.array.is_some());},
            ViewPositionNeed::HoldDataset=>{let Some(returns::Returned::Dataset(actual))=returned.take()else{unsafe {std::hint::unreachable_unchecked()}};self.hold_binding(Binding::ApplicationData(actual.clone()));data=Some(application::DataReference::direct(actual));},
            ViewPositionNeed::HoldOwned=>{let Some(returns::Returned::OwnedValue(actual))=returned.take()else{unsafe {std::hint::unreachable_unchecked()}};let owner=self.owned_value_binding(actual,true);place=owner.place();self.hold_binding(Binding::OwnedValue(owner));},
            ViewPositionNeed::Ordinary=>{value=self.returned_expression(returned.take(),base.span(),false)?;facts.present=u32::from(value.is_some());},
            ViewPositionNeed::ArrayType=>facts.array=u32::from(value.as_ref().unwrap().ty.array.is_some()),
            ViewPositionNeed::Materialize=>place=Some(self.materialize_array(value.take().unwrap(),None).place),
            ViewPositionNeed::ArrayPlace=>return unsafe {self.world.call(self.array_place(place.take().unwrap(),index)).await},
            ViewPositionNeed::View=>return unsafe {self.world.call(self.view_value_place(value.take().unwrap(),index)).await},
            ViewPositionNeed::Empty=>return Ok(None),
        }}
    }
    pub(super) async fn view_value_place(&mut self, view: Value, index: &Expr) -> Result<Option<Place>> {
        unsafe { self.world.call(self.view_value_at(view,index.span(),async |builder|unsafe { builder.world.call(builder.expr(index)).await })).await }
    }
    pub(super) async fn view_value_at(&mut self,view:Value,span:Span,
        read_index:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Place>> {
        if view.ty.view.is_none() {
            return self.fail(span, "B136", "索引需要 dataset、tile 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = read_index(self).await;
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

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(TaskInputCompletion);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        offset: Slot,
        result: Slot,
        layout: Slot,
        task: Slot,
        parent: Slot,
    }
    #[repr(C)]
    struct Writer {
        offset: WriteCallback,
        result: WriteCallback,
        layout: WriteCallback,
        task: WriteCallback,
        parent: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        offset: ReadCallback,
        result: ReadCallback,
        layout: ReadCallback,
        task: ReadCallback,
        parent: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_task_completion"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_task_completion"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for TaskInputCompletion {
        fn empty() -> Self {
            Self {
                offset: <u32 as Native>::empty(),
                result: <Ty as Native>::empty(),
                layout: <Ty as Native>::empty(),
                task: <Borrow as Native>::empty(),
                parent: <Option<(usize, usize)> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        offset: contracts::address(&self.offset).cast_mut(),
                        result: contracts::address(&self.result).cast_mut(),
                        layout: contracts::address(&self.layout).cast_mut(),
                        task: contracts::address(&self.task).cast_mut(),
                        parent: contracts::address(&self.parent).cast_mut(),
                    },
                    Writer {
                        offset: contracts::write_value::<u32>,
                        result: contracts::write_value::<Ty>,
                        layout: contracts::write_value::<Ty>,
                        task: contracts::write_value::<Borrow>,
                        parent: contracts::write_value::<Option<(usize, usize)>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        offset: contracts::slot(&mut output.offset),
                        result: contracts::slot(&mut output.result),
                        layout: contracts::slot(&mut output.layout),
                        task: contracts::slot(&mut output.task),
                        parent: contracts::slot(&mut output.parent),
                    },
                    Reader {
                        offset: contracts::read_value::<u32>,
                        result: contracts::read_value::<Ty>,
                        layout: contracts::read_value::<Ty>,
                        task: contracts::read_value::<Borrow>,
                        parent: contracts::read_value::<Option<(usize, usize)>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(TaskProjection);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        returned: ReadCallback,
        writes: ReadCallback,
        places: ReadCallback,
        initialized: ReadCallback,
    }
    unsafe extern "C" fn build_returned(context: Slot, destination: Slot) -> ReadStatus {
        let mut component = <usize as Native>::empty();
        let mut source = <Option<ValueIdentity> as Native>::empty();
        #[repr(C)]
        struct Fields {
            component: Slot,
            source: Slot,
        }
        #[repr(C)]
        struct Host {
            component: ReadCallback,
            source: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_task_projection_returned"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    component: contracts::slot(&mut component),
                    source: contracts::slot(&mut source),
                },
                Host {
                    component: contracts::read_value::<usize>,
                    source: contracts::read_value::<Option<ValueIdentity>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<TaskProjection>() = TaskProjection::Returned(component, source);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_writes(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<TaskProjection>() = TaskProjection::Writes;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_places(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<TaskProjection>() = TaskProjection::Places;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_initialized(context: Slot, destination: Slot) -> ReadStatus {
        let mut returned = <Vec<Option<ValueIdentity>> as Native>::empty();
        #[repr(C)]
        struct Fields {
            returned: Slot,
        }
        #[repr(C)]
        struct Host {
            returned: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_task_projection_initialized"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    returned: contracts::slot(&mut returned),
                },
                Host {
                    returned: contracts::read_value::<Vec<Option<ValueIdentity>>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<TaskProjection>() = TaskProjection::Initialized(returned);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_task_projection"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for TaskProjection {
        fn empty() -> Self {
            Self::Writes
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Returned(component, source) => {
                    #[repr(C)]
                    struct Fields {
                        component: Slot,
                        source: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        component: WriteCallback,
                        source: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_task_projection_returned"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                component: contracts::address(component).cast_mut(),
                                source: contracts::address(source).cast_mut(),
                            },
                            Host {
                                component: contracts::write_value::<usize>,
                                source: contracts::write_value::<Option<ValueIdentity>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Writes => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_task_projection_writes"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Places => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_task_projection_places"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Initialized(returned) => {
                    #[repr(C)]
                    struct Fields {
                        returned: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        returned: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_task_projection_initialized"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                returned: contracts::address(returned).cast_mut(),
                            },
                            Host {
                                returned: contracts::write_value::<Vec<Option<ValueIdentity>>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        returned: build_returned,
                        writes: build_writes,
                        places: build_places,
                        initialized: build_initialized,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Exclusion);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        resource: ReadCallback,
        ownership: ReadCallback,
        position: ReadCallback,
        view: ReadCallback,
    }
    unsafe extern "C" fn build_resource(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Exclusion>() = Exclusion::Resource;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_ownership(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Exclusion>() = Exclusion::Ownership;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut access = <Access as Native>::empty();
        #[repr(C)]
        struct Fields {
            access: Slot,
        }
        #[repr(C)]
        struct Host {
            access: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_exclusion_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    access: contracts::slot(&mut access),
                },
                Host {
                    access: contracts::read_value::<Access>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Exclusion>() = Exclusion::Position(access);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_view(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Exclusion>() = Exclusion::View;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_exclusion"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Exclusion {
        fn empty() -> Self {
            Self::Resource
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Resource => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_exclusion_resource"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Ownership => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_exclusion_ownership"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Position(access) => {
                    #[repr(C)]
                    struct Fields {
                        access: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        access: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_exclusion_position"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                access: contracts::address(access).cast_mut(),
                            },
                            Host {
                                access: contracts::write_value::<Access>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::View => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_exclusion_view"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        resource: build_resource,
                        ownership: build_ownership,
                        position: build_position,
                        view: build_view,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Source);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        data: ReadCallback,
        parameter: ReadCallback,
        unresolved: ReadCallback,
        inactive: ReadCallback,
        external: ReadCallback,
        position: ReadCallback,
        element: ReadCallback,
        projection: ReadCallback,
        contents: ReadCallback,
        relation: ReadCallback,
        task: ReadCallback,
        task_output: ReadCallback,
    }
    unsafe extern "C" fn build_data(context: Slot, destination: Slot) -> ReadStatus {
        let mut id = <ValueIdentity as Native>::empty();
        #[repr(C)]
        struct Fields {
            id: Slot,
        }
        #[repr(C)]
        struct Host {
            id: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_data"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    id: contracts::slot(&mut id),
                },
                Host {
                    id: contracts::read_value::<ValueIdentity>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Data(id);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_parameter(context: Slot, destination: Slot) -> ReadStatus {
        let mut id = <ValueIdentity as Native>::empty();
        let mut indices = <Vec<Option<bounds::Selector>> as Native>::empty();
        #[repr(C)]
        struct Fields {
            id: Slot,
            indices: Slot,
        }
        #[repr(C)]
        struct Host {
            id: ReadCallback,
            indices: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_parameter"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    id: contracts::slot(&mut id),
                    indices: contracts::slot(&mut indices),
                },
                Host {
                    id: contracts::read_value::<ValueIdentity>,
                    indices: contracts::read_value::<Vec<Option<bounds::Selector>>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Parameter(id, indices);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_unresolved(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Source>() = Source::Unresolved;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_inactive(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Source>() = Source::Inactive;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_external(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Source>() = Source::External;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <Arc<address::Origin> as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                },
                Host {
                    origin: contracts::read_value::<Arc<address::Origin>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Position(origin);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_element(context: Slot, destination: Slot) -> ReadStatus {
        let mut parent = <Box<Borrow> as Native>::empty();
        let mut fields = <Vec<usize> as Native>::empty();
        let mut index = <Option<bounds::Selector> as Native>::empty();
        #[repr(C)]
        struct Fields {
            parent: Slot,
            fields: Slot,
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            parent: ReadCallback,
            fields: ReadCallback,
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_element"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    parent: contracts::slot(&mut parent),
                    fields: contracts::slot(&mut fields),
                    index: contracts::slot(&mut index),
                },
                Host {
                    parent: contracts::read_value::<Box<Borrow>>,
                    fields: contracts::read_value::<Vec<usize>>,
                    index: contracts::read_value::<Option<bounds::Selector>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Element {
                parent,
                fields,
                index,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_projection(context: Slot, destination: Slot) -> ReadStatus {
        let mut parent = <Box<Borrow> as Native>::empty();
        let mut fields = <Vec<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            parent: Slot,
            fields: Slot,
        }
        #[repr(C)]
        struct Host {
            parent: ReadCallback,
            fields: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_projection"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    parent: contracts::slot(&mut parent),
                    fields: contracts::slot(&mut fields),
                },
                Host {
                    parent: contracts::read_value::<Box<Borrow>>,
                    fields: contracts::read_value::<Vec<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Projection { parent, fields };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_contents(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <Arc<address::Origin> as Native>::empty();
        let mut word = <u32 as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
            word: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
            word: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_contents"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                    word: contracts::slot(&mut word),
                },
                Host {
                    origin: contracts::read_value::<Arc<address::Origin>>,
                    word: contracts::read_value::<u32>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Contents { origin, word };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_relation(context: Slot, destination: Slot) -> ReadStatus {
        let mut graph = <view_contracts::relation::Graph as Native>::empty();
        #[repr(C)]
        struct Fields {
            graph: Slot,
        }
        #[repr(C)]
        struct Host {
            graph: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_relation"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    graph: contracts::slot(&mut graph),
                },
                Host {
                    graph: contracts::read_value::<view_contracts::relation::Graph>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Relation(Arc::new(graph), 0);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_task(context: Slot, destination: Slot) -> ReadStatus {
        let mut captures = <Box<Borrow> as Native>::empty();
        let mut outputs = <Vec<Borrow> as Native>::empty();
        let mut initialized = <Box<Borrow> as Native>::empty();
        let mut guards = <Vec<outputs::TaskGuard> as Native>::empty();
        let mut completions = <Vec<TaskInputCompletion> as Native>::empty();
        #[repr(C)]
        struct Fields {
            captures: Slot,
            outputs: Slot,
            initialized: Slot,
            guards: Slot,
            completions: Slot,
        }
        #[repr(C)]
        struct Host {
            captures: ReadCallback,
            outputs: ReadCallback,
            initialized: ReadCallback,
            guards: ReadCallback,
            completions: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_task"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    captures: contracts::slot(&mut captures),
                    outputs: contracts::slot(&mut outputs),
                    initialized: contracts::slot(&mut initialized),
                    guards: contracts::slot(&mut guards),
                    completions: contracts::slot(&mut completions),
                },
                Host {
                    captures: contracts::read_value::<Box<Borrow>>,
                    outputs: contracts::read_value::<Vec<Borrow>>,
                    initialized: contracts::read_value::<Box<Borrow>>,
                    guards: contracts::read_value::<Vec<outputs::TaskGuard>>,
                    completions: contracts::read_value::<Vec<TaskInputCompletion>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::Task {
                captures,
                outputs,
                initialized,
                guards,
                completions,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_task_output(context: Slot, destination: Slot) -> ReadStatus {
        let mut task = <Box<Borrow> as Native>::empty();
        let mut projection = <TaskProjection as Native>::empty();
        #[repr(C)]
        struct Fields {
            task: Slot,
            projection: Slot,
        }
        #[repr(C)]
        struct Host {
            task: ReadCallback,
            projection: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_borrow_source_task_output"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    task: contracts::slot(&mut task),
                    projection: contracts::slot(&mut projection),
                },
                Host {
                    task: contracts::read_value::<Box<Borrow>>,
                    projection: contracts::read_value::<TaskProjection>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Source>() = Source::TaskOutput { task, projection };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_borrow_source"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Source {
        fn empty() -> Self {
            Self::Unresolved
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Data(id) => {
                    #[repr(C)]
                    struct Fields {
                        id: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        id: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_data"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                id: contracts::address(id).cast_mut(),
                            },
                            Host {
                                id: contracts::write_value::<ValueIdentity>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Parameter(id, indices) => {
                    #[repr(C)]
                    struct Fields {
                        id: Slot,
                        indices: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        id: WriteCallback,
                        indices: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_parameter"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                id: contracts::address(id).cast_mut(),
                                indices: contracts::address(indices).cast_mut(),
                            },
                            Host {
                                id: contracts::write_value::<ValueIdentity>,
                                indices: contracts::write_value::<Vec<Option<bounds::Selector>>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Unresolved => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_unresolved"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Inactive => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_inactive"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::External => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_external"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Position(origin) => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_position"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<Arc<address::Origin>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Element {
                    parent,
                    fields,
                    index,
                } => {
                    #[repr(C)]
                    struct Fields {
                        parent: Slot,
                        fields: Slot,
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        parent: WriteCallback,
                        fields: WriteCallback,
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_element"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                parent: contracts::address(parent).cast_mut(),
                                fields: contracts::address(fields).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                parent: contracts::write_value::<Box<Borrow>>,
                                fields: contracts::write_value::<Vec<usize>>,
                                index: contracts::write_value::<Option<bounds::Selector>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Projection { parent, fields } => {
                    #[repr(C)]
                    struct Fields {
                        parent: Slot,
                        fields: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        parent: WriteCallback,
                        fields: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_projection"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                parent: contracts::address(parent).cast_mut(),
                                fields: contracts::address(fields).cast_mut(),
                            },
                            Host {
                                parent: contracts::write_value::<Box<Borrow>>,
                                fields: contracts::write_value::<Vec<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Contents { origin, word } => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                        word: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                        word: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_contents"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                                word: contracts::address(word).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<Arc<address::Origin>>,
                                word: contracts::write_value::<u32>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Relation(graph, root) => {
                    let selected = graph.selected(*root);
                    let graph = &selected;
                    #[repr(C)]
                    struct Fields {
                        graph: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        graph: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_relation"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                graph: contracts::address(graph).cast_mut(),
                            },
                            Host {
                                graph: contracts::write_value::<view_contracts::relation::Graph>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Task {
                    captures,
                    outputs,
                    initialized,
                    guards,
                    completions,
                } => {
                    #[repr(C)]
                    struct Fields {
                        captures: Slot,
                        outputs: Slot,
                        initialized: Slot,
                        guards: Slot,
                        completions: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        captures: WriteCallback,
                        outputs: WriteCallback,
                        initialized: WriteCallback,
                        guards: WriteCallback,
                        completions: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_task"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                captures: contracts::address(captures).cast_mut(),
                                outputs: contracts::address(outputs).cast_mut(),
                                initialized: contracts::address(initialized).cast_mut(),
                                guards: contracts::address(guards).cast_mut(),
                                completions: contracts::address(completions).cast_mut(),
                            },
                            Host {
                                captures: contracts::write_value::<Box<Borrow>>,
                                outputs: contracts::write_value::<Vec<Borrow>>,
                                initialized: contracts::write_value::<Box<Borrow>>,
                                guards: contracts::write_value::<Vec<outputs::TaskGuard>>,
                                completions: contracts::write_value::<Vec<TaskInputCompletion>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::TaskOutput { task, projection } => {
                    #[repr(C)]
                    struct Fields {
                        task: Slot,
                        projection: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        task: WriteCallback,
                        projection: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_borrow_source_task_output"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                task: contracts::address(task).cast_mut(),
                                projection: contracts::address(projection).cast_mut(),
                            },
                            Host {
                                task: contracts::write_value::<Box<Borrow>>,
                                projection: contracts::write_value::<TaskProjection>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        data: build_data,
                        parameter: build_parameter,
                        unresolved: build_unresolved,
                        inactive: build_inactive,
                        external: build_external,
                        position: build_position,
                        element: build_element,
                        projection: build_projection,
                        contents: build_contents,
                        relation: build_relation,
                        task: build_task,
                        task_output: build_task_output,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Borrow);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Address, Native, Slot};
    unsafe extern "C" fn entry(source: Address, output: Slot) {
        let (source, when) = unsafe { &*source.cast::<(&Source, &bounds::Clause)>() };
        let condition = if matches!(source, Source::Inactive) {
            bounds::Condition::constant(true)
        } else {
            bounds::Condition::guard((*when).clone())
        };
        unsafe {
            contracts::write_pair_with(
                contracts::address(*source),
                contracts::address(&condition),
                output,
                contracts::write_value::<Source>,
                contracts::write_value::<bounds::Condition>,
            )
        }
    }
    impl Native for Borrow {
        fn empty() -> Self {
            Self::default()
        }
        unsafe fn write(&self, output: Slot) {
            unsafe { contracts::write_map_with(&self.sources.0, output, entry) }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut values = BTreeMap::<Source, bounds::Condition>::new();
            let status = unsafe { Native::read(context, &mut values) };
            if status == ReadStatus::Success {
                output.sources = Edges(
                    values
                        .into_iter()
                        .map(|(source, when)| {
                            (source, when.assume(&bounds::Clause::default(), true))
                        })
                        .collect(),
                );
            }
            status
        }
    }
};
