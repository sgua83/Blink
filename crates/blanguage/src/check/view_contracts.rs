//! view借用与拥有资源沿原输入、循环和调用图交接；访问条件属于函数静态合同。
use super::*;
use view::{Borrow, LoopInput, Requirement, Source as Origin};
// Resource identity is the original root plus its typed element-field path.
// Model discovery order and the amount of known descendant facts do not name an output.
pub(super) type ResourcePath=(usize,Vec<Vec<usize>>);
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Output { Returned(usize), Written(usize,usize), ReturnedResource(ResourcePath), WrittenResource(usize,ResourcePath) }
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct CallResult { pub call:usize, pub output:Output }
pub(super) type Outputs=BTreeMap<Output,Borrow>;
#[derive(Clone)]
pub(super) enum ContentRead {
    Value {memory:usize,word:u32,borrow:Arc<Borrow>,snapshots:Arc<[ValueSnapshot]>},
    Snapshot {memory:usize,component:usize,position:Position,index:Option<bounds::Operand>,initial:Arc<bounds::Proof>},
    Call {memory:usize,origin:Arc<address::Origin>,word:u32,snapshots:Option<Arc<[ValueSnapshot]>>},
    Returned {memory:usize},
    Relation {memory:usize,graph:Arc<relation::Graph>,root:usize,snapshots:Option<Arc<[ValueSnapshot]>>},
}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
enum ContentKey<'a>{Value(usize,u32),Snapshot(usize,usize),Call(usize,&'a address::Origin,u32),Returned(usize),Relation(usize,&'a relation::Graph,usize)}
impl ContentRead {
    fn key(&self)->ContentKey<'_>{match self {
        Self::Value{memory,word,..}=>ContentKey::Value(*memory,*word),
        Self::Snapshot{memory,component,..}=>ContentKey::Snapshot(*memory,*component),
        Self::Call{memory,origin,word,..}=>ContentKey::Call(*memory,origin,*word),
        Self::Returned{memory}=>ContentKey::Returned(*memory),
        Self::Relation{memory,graph,root,..}=>ContentKey::Relation(*memory,graph,*root),
    }}
}
impl PartialEq for ContentRead{fn eq(&self,other:&Self)->bool{self.key()==other.key()}}
impl Eq for ContentRead{}
impl PartialOrd for ContentRead{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for ContentRead{fn cmp(&self,other:&Self)->std::cmp::Ordering{self.key().cmp(&other.key())}}
#[derive(Clone,PartialEq,Eq)]
pub(super) struct PositionAccess {pub layout:address::Layout,pub written:Option<bounds::Proof>,pub writable:bool,pub initialized:bool}
impl Ord for PositionAccess {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        use crate::module::codec::Encode;
        (&self.layout,self.writable,self.initialized).cmp(&(&other.layout,other.writable,other.initialized))
            .then_with(||self.written.bytes().cmp(&other.written.bytes()))
    }
}
impl PartialOrd for PositionAccess {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Access<Path = bounds::Condition> {
    Resource {resources:Borrow,borrows:Borrow,kind:view::Exclusion,when:Path},
    Position {borrows:Borrow,when:Path,position:PositionAccess},
    Memory {borrows:Borrow,when:Path,access:super::Access},
    Dataset {borrows:Borrow,when:Path,index:Option<bounds::Operand>,access:super::Access,atomic:bool},
    Initialized {borrows:Borrow,initialized:Vec<address::Initialization>,when:Path},
    Stored {borrows:Borrow,word:u32,value:Borrow,when:Path},
}
impl<Path> Access<Path> {
    fn replace_when<T>(self, when: T) -> (Access<T>, Path) {
        match self {
            Self::Resource { resources, borrows, kind, when: original } =>
                (Access::Resource { resources, borrows, kind, when }, original),
            Self::Position { borrows, position, when: original } =>
                (Access::Position { borrows, position, when }, original),
            Self::Memory { borrows, access, when: original } =>
                (Access::Memory { borrows, access, when }, original),
            Self::Dataset { borrows, index, access, atomic, when: original } =>
                (Access::Dataset { borrows, index, access, atomic, when }, original),
            Self::Initialized { borrows, initialized, when: original } =>
                (Access::Initialized { borrows, initialized, when }, original),
            Self::Stored { borrows, word, value, when: original } =>
                (Access::Stored { borrows, word, value, when }, original),
        }
    }

    fn accessed(&self)->Option<(&Borrow,&Path,super::Access)> {
        match self {
            Self::Position{borrows,when,position}=>{
                let access=if position.writable||position.written.is_some(){super::Access::Write}else{super::Access::Read};
                Some((borrows,when,access))
            },
            Self::Memory{borrows,when,access}|Self::Dataset{borrows,when,access,..}=>Some((borrows,when,*access)),
            Self::Stored{borrows,when,..}=>Some((borrows,when,super::Access::Write)),
            Self::Resource{..}|Self::Initialized{..}=>None,
        }
    }
}

impl Access {
    fn memory_effect(&self) -> Option<(&Borrow, &bounds::Condition, super::Access, Option<&bounds::Proof>)> {
        match self {
            Self::Memory { borrows, when, access } | Self::Dataset { borrows, when, access, .. } =>
                Some((borrows, when, *access, None)),
            Self::Position { borrows, when, position } => {
                let access = match (&position.written, position.initialized) {
                    (Some(_), true) => super::Access::InOut,
                    (Some(_), false) => super::Access::Write,
                    (None, true) => super::Access::Read,
                    (None, false) => return None,
                };
                Some((borrows, when, access, position.written.as_ref()))
            }
            _ => None,
        }
    }

    fn at_recursive_entry(
        mut self, inputs: &BTreeMap<ValueIdentity, bounds::Proof>, invoked: &bounds::Condition,
    ) -> Option<Self> {
        let (borrows, when) = match &mut self {
            Self::Position { borrows, when, position } => {
                position.written = position.written.as_ref().map(|proof| proof.at_call(inputs));
                (borrows, when)
            }
            Self::Initialized { borrows, when, initialized } => {
                *initialized = initialized.iter().map(|fact| fact.at_call(inputs)).collect();
                (borrows, when)
            }
            Self::Resource { resources, borrows, when, .. } => {
                *resources = resources.at_call(inputs);
                (borrows, when)
            }
            Self::Dataset { borrows, when, index, .. } => {
                *index = index.and_then(|value| value.at_call(inputs));
                (borrows, when)
            }
            Self::Memory { borrows, when, .. } => (borrows, when),
            Self::Stored { .. } => return None,
        };
        *borrows = borrows.at_call(inputs);
        *when = when.guard_at_call(inputs).logical(invoked.clone(), false);
        Some(self)
    }

    fn active_entry_access(&self) -> bool {
        match self {
            Self::Resource { resources, borrows, when, .. } =>
                resources.active() && borrows.active() && when.active(),
            Self::Position { borrows, when, .. } | Self::Memory { borrows, when, .. } |
            Self::Dataset { borrows, when, .. } | Self::Initialized { borrows, when, .. } =>
                borrows.active() && when.active(),
            Self::Stored { .. } => false,
        }
    }

    pub(super) fn when_mut(&mut self)->&mut bounds::Condition {match self {
        Self::Resource{when,..}|Self::Position{when,..}|Self::Memory{when,..}|Self::Dataset{when,..}|Self::Initialized{when,..}|Self::Stored{when,..}=>when,
    }}
    pub(super) fn content_context(&self)->bool{match self{
        Self::Resource{resources,borrows,..}=>resources.content_inputs()||borrows.content_inputs(),
        Self::Position{borrows,..}|Self::Memory{borrows,..}|Self::Dataset{borrows,..}|Self::Initialized{borrows,..}=>borrows.content_inputs(),
        Self::Stored{..}=>true,
    }}

    fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        match self {
            Self::Resource{resources,borrows,kind,when}=>Self::Resource{resources:resources.substitute_values(values),borrows:borrows.substitute_values(values),kind:*kind,when:when.substitute_values(values)},
            Self::Position{borrows,when,position}=>Self::Position{borrows:borrows.substitute_values(values),when:when.substitute_values(values),position:PositionAccess{written:position.written.as_ref().map(|proof|proof.substitute_values(values)),..position.clone()}},
            Self::Memory{borrows,when,access}=>Self::Memory{borrows:borrows.substitute_values(values),when:when.substitute_values(values),access:*access},
            Self::Dataset{borrows,when,index,access,atomic}=>{
                let index=index.and_then(|index|match index{bounds::Operand::Value(id)if values.contains_key(&id)=>index.at_call(values),index=>Some(index)});
                Self::Dataset{borrows:borrows.substitute_values(values),when:when.substitute_values(values),index,access:*access,atomic:*atomic}
            },
            Self::Initialized{borrows,initialized,when}=>Self::Initialized{borrows:borrows.substitute_values(values),initialized:initialized.iter().map(|value|value.substitute_values(values)).collect(),when:when.substitute_values(values)},
            Self::Stored{borrows,word,value,when}=>Self::Stored{borrows:borrows.substitute_values(values),word:*word,value:value.substitute_values(values),when:when.substitute_values(values)},
        }
    }
}

#[derive(Clone,Default,PartialEq,Eq)]
pub(super) struct Interface {pub outputs:Outputs,pub access:BTreeSet<Access>,pub initialization:Vec<address::Initialization>}
pub(super) struct ValueSnapshot { pub position:Position, pub index:Option<bounds::Operand>, pub scalar:Scalar, pub proof:Arc<bounds::Proof> }
// The original element model and the inputs selected by this read, after earlier Memory values close.
pub(super) struct ElementRead { pub scalar:Scalar, pub proof:bounds::Proof, pub bindings:Arc<BTreeMap<ValueIdentity,bounds::Proof>> }
pub(super) struct Call {
    pub(super) function:String,
    pub(super) memory:usize,
    deferred:bool,
    live_task_places:Borrow,
    pub(super) arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    pub(super) snapshots:Option<Arc<[ValueSnapshot]>>,
    initialization:Vec<address::InitializationEffect>,
    outputs:BTreeSet<Output>,
    pub(super) path:bounds::Clause,
    source:crate::Source,
    span:Span,
}
pub(super) struct Use {
    pub requirement: Requirement,
    pub source: crate::Source,
    pub path: bounds::Clause,
    pub active_loops: BTreeSet<usize>,
    pub(super) opaque_write_possible: std::sync::OnceLock<bool>,
}

#[derive(Clone)]
pub(super) enum ControlInput {Value{name:String,contents:Option<Vec<usize>>,component:usize},Length{name:String,path:Vec<usize>}}
impl ControlInput {
    fn project<'a>(&self, lookup: impl FnOnce(&str) -> Option<&'a Binding>) -> Option<bounds::Proof> {
        match self {
            Self::Value { name, contents: None, component } => lookup(name)
                .and_then(binding_value)
                .map(|(ty, proof)| proof.project(&[*component], ty.components.len())),
            Self::Value { name, contents: Some(path), component } => lookup(name)
                .and_then(|binding| view::data_at(binding, path))
                .and_then(|data| data.elements.known())
                .and_then(returns::Returned::view_value)
                .map(|value| bounds::Proof::capture_selected(value, &[*component], value.ty.components.len())),
            Self::Length { name, path } => lookup(name)
                .and_then(|binding| view::data_at(binding, path))
                .map(|data| bounds::Proof::capture(&data.length)),
        }
    }
}
#[derive(Clone,Default)]
pub(super) struct LoopControl {
    pub cursor:Option<Value>,
    pub test:Option<bounds::Condition>,
    pub inputs:BTreeMap<ValueIdentity,ControlInput>,
}
enum ControlValue<'a> {
    Value(&'a Value),
    Stored(&'a bounds::Proof),
}

impl ControlValue<'_> {
    fn known_identity(&self, component: usize) -> Option<ValueIdentity> {
        match self {
            Self::Value(value) => value.regs.get(component).map(|_| value.component_identity(component)),
            Self::Stored(proof) => proof.known_identity(component),
        }
    }

    fn project(&self, components: &[usize], width: usize) -> bounds::Proof {
        match self {
            Self::Value(value) => bounds::Proof::capture_selected(value, components, width),
            Self::Stored(proof) => proof.project(components, width),
        }
    }
}

fn binding_value(binding: &Binding) -> Option<(&Ty, ControlValue<'_>)> {
    match binding {
        Binding::Value(value) => Some((&value.ty, ControlValue::Value(value))),
        Binding::Owned(value) | Binding::ViewElement { element: value, .. } =>
            Some((value.place.ty(), ControlValue::Stored(&value.proof))),
        Binding::OwnedValue(value) => Some((&value.value.ty, ControlValue::Value(&value.value))),
        _ => None,
    }
}

impl LoopControl {
    pub fn capture(test:&Value,environment:&BTreeMap<String,Binding>,cursor:Option<Value>)->Self {
        let condition = bounds::Condition::of(test);
        let identities = condition.input_identities();
        let mut inputs = BTreeMap::new();
        for(name,binding)in environment {
            if let Some((ty,proof))=binding_value(binding) {
                for component in 0..ty.components.len() {
                    if let Some(id)=proof.known_identity(component) && identities.contains(&id) {
                        inputs.insert(id,ControlInput::Value{name:name.clone(),contents:None,component});
                    }
                }
            }
            view::for_each_data_path(binding, |path, data, _| {
                let id = data.length.identity();
                if identities.contains(&id) {
                    inputs.insert(id, ControlInput::Length { name: name.clone(), path: path.to_vec() });
                }
            });
        }
        Self{cursor,test:Some(condition),inputs}
    }
    pub(super) fn bindings(&self, producer: &flow::LoopState) -> BTreeMap<ValueIdentity, bounds::Proof> {
        self.bindings_from(producer.cursor.as_ref(), |name| producer.env.get(name))
    }

    pub(super) fn bindings_from<'a>(
        &self,
        next: Option<&Value>,
        lookup: impl Fn(&str) -> Option<&'a Binding>,
    ) -> BTreeMap<ValueIdentity, bounds::Proof> {
        let mut values = self.inputs.iter().filter_map(|(&id, input)| {
            input.project(&lookup).map(|proof| (id, proof))
        }).collect::<BTreeMap<_, _>>();
        if let (Some(cursor), Some(next)) = (&self.cursor, next) {
            values.insert(cursor.identity(), bounds::Proof::capture(next));
        }
        values
    }

    fn binding(&self, id: ValueIdentity, producer: &flow::LoopState) -> Option<bounds::Proof> {
        if let (Some(cursor), Some(next)) = (&self.cursor, &producer.cursor)
            && cursor.identity() == id
        {
            return Some(bounds::Proof::capture(next));
        }
        self.inputs.get(&id)?.project(|name| producer.env.get(name))
    }
}
pub(super) struct LoopResource {
    pub name:String,
    pub path:Vec<usize>,
}
pub(super) struct LoopDefinition {
    pub borrow:Borrow,
    pub resource:Option<LoopResource>,
    pub value:Option<(ValueIdentity,Scalar)>,
    // Every input refers to the same actual entry and continued producers.
    pub states:Arc<[flow::LoopState]>,
    pub control:Arc<LoopControl>,
}
pub(super) struct LoopHistory {
    pub registers: std::ops::Range<Reg>,
    pub uses: std::ops::Range<usize>,
    pub backedge: Option<bounds::Clause>,
}

#[derive(Default)]
pub(super) struct Function {
    next_scope:usize,
    pub(super) calls:Vec<Call>,
    pub loops:BTreeMap<LoopInput,LoopDefinition>,
    pub independent:BTreeSet<LoopInput>,
    pub outputs:Outputs,
    pub initialization:Vec<address::InitializationEffect>,
    pub normal_returns:Option<bounds::Condition>,
    pub resource_names:BTreeMap<Output,Borrow>,
    pub parameters:BTreeMap<ValueIdentity,ValueIdentity>,
    pub element_inputs:BTreeMap<residency::Input,(usize,usize)>,
    pub input_width:usize,
    pub uses:Vec<Use>,
    pub history: BTreeMap<usize, LoopHistory>,
}
pub(super) fn components(ty:&Ty)->BTreeSet<usize> {
    ty.borrow_fields().into_iter().map(|(offset,_)|ty.components.iter().position(|(_,at)|*at==offset).unwrap()).collect()
}
pub(super) fn resource_borrows(value:&returns::Returned)->Vec<Borrow>{
    match value{
        returns::Returned::Dataset(value)=>vec![view::resource_borrow(value)],
        returns::Returned::OwnedValue(value)=>view::resource_paths(&value.value.ty).into_iter().map(|path|view::owned_data_at(value,&path).map(view::resource_borrow).unwrap_or_else(||Borrow::from_sources(BTreeSet::from([Origin::Inactive])))).collect(),
        _=>Vec::new(),
    }
}
pub(super) fn resource_value_mut(value:&mut returns::Returned,index:usize)->Option<&mut application::Data>{match value{
    returns::Returned::Dataset(value)if index==0=>Some(value),
    returns::Returned::OwnedValue(value)=>{let path=view::resource_path(&value.value.ty, index)?;view::owned_data_at_mut(value,&path)},_=>None,
}}
fn visit_resource_outputs<'a, const BORROW: bool>(
    value: &'a returns::Returned,
    mut visit: impl FnMut(usize, &[Vec<usize>], Option<(&'a application::Data, &'a application::Data)>),
) {
    fn children<'a, const BORROW: bool>(
        root: (usize, &'a application::Data),
        parent: &'a application::Data,
        available: bool,
        path: &mut Vec<Vec<usize>>,
        visit: &mut impl FnMut(usize, &[Vec<usize>], Option<(&'a application::Data, &'a application::Data)>),
    ) {
        let child_available = std::cell::LazyCell::new(|| {
            available && (!BORROW || parent.length.maximum() != 0)
        });
        for fields in view::resource_paths(&parent.ty) {
            if let Some(child) = application::model_data(parent.elements.known(), &fields) {
                path.push(fields);
                visit(root.0, path, (*child_available).then_some((root.1, child)));
                children::<BORROW>(root, child, *child_available, path, visit);
                path.pop();
            }
        }
    }

    let single;
    let owned;
    let roots: &[Option<&application::Data>] = match value {
        returns::Returned::Dataset(data) => {
            single = [Some(data)];
            &single
        }
        returns::Returned::OwnedValue(value) => {
            owned = view::resource_paths(&value.value.ty).into_iter()
                .map(|path| view::owned_data_at(value, &path)).collect::<Vec<_>>();
            &owned
        }
        _ => return,
    };
    // Reserve the original slot identity even when that variant has no data.
    for (root, data) in roots.iter().copied().enumerate() {
        visit(root, &[], data.map(|data| (data, data)));
    }
    let mut path = Vec::new();
    for (root, data) in roots.iter().copied().enumerate() {
        if let Some(data) = data {
            children::<BORROW>((root, data), data, true, &mut path, &mut visit);
        }
    }
}

pub(super) fn resource_output_paths(value: &returns::Returned) -> Vec<ResourcePath> {
    let mut paths = Vec::new();
    visit_resource_outputs::<false>(value, |root, path, _| paths.push((root, path.to_vec())));
    paths
}

pub(super) fn root_resource(value:&returns::Returned,index:usize)->Option<&application::Data>{match value{
    returns::Returned::Dataset(data)if index==0=>Some(data),
    returns::Returned::OwnedValue(value)=>view::resource_path(&value.value.ty, index).and_then(|path|view::owned_data_at(value,&path)),_=>None,
}}

pub(super) fn resource_outputs(
    value: &returns::Returned,
    mut output: impl FnMut(ResourcePath, Borrow),
) {
    visit_resource_outputs::<true>(value, |root, path, model| {
        let borrow = match model {
            Some((root_data, data)) => {
                let name = std::cell::LazyCell::new(|| resource_output_name(root_data, path));
                view::resource_borrow(data).map_sources(|source| match source {
                    Origin::Data(ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..)) =>
                        (*name).clone(),
                    source => Borrow::source(source),
                })
            }
            None => Borrow::source(Origin::Inactive),
        };
        output((root, path.to_vec()), borrow);
    });
}

pub(super) fn resource_output<'a>(value:&'a returns::Returned,resource:&ResourcePath)->Option<&'a application::Data>{
    let(root,path)=resource;let mut data=root_resource(value,*root)?;
    for fields in path{data=application::model_data(data.elements.known(),fields)?;}Some(data)
}
pub(super) fn resource_output_mut<'a>(value:&'a mut returns::Returned,resource:&ResourcePath)->Option<&'a mut application::Data>{
    let(root,path)=resource;let mut data=resource_value_mut(value,*root)?;
    for fields in path{data=application::model_data_mut(data.elements.known_mut(),fields)?;}Some(data)
}
fn resource_output_name(root: &application::Data, path: &[Vec<usize>]) -> Borrow {
    let mut name = view::resource_borrow(root);
    for (at, fields) in path.iter().enumerate() {
        let index = ValueIdentity::ElementIndex(path.len() - 1 - at);
        name = Borrow::element(name, fields.clone(), Some(bounds::Operand::Value(index)));
    }
    name
}
pub(super) fn set_resource_outputs(value:&mut returns::Returned,mut borrow:impl FnMut(ResourcePath)->Borrow){
    for resource in resource_output_paths(value){if let Some(data)=resource_output_mut(value,&resource){data.root.proof.set_view_borrow(0,borrow(resource));}}
}
fn disjoint(a:&Borrow,b:&Borrow,independent:&BTreeSet<LoopInput>,active:&BTreeSet<usize>)->bool {
    if !a.calls.is_empty()||!b.calls.is_empty(){return false;}
    if a.sources.is_empty()&&b.sources.is_empty()&&!a.inputs.is_empty()&&!b.inputs.is_empty()
        &&a.inputs.iter().all(|a|b.inputs.iter().all(|b|a.level==b.level&&a.index!=b.index&&independent.contains(a)&&independent.contains(b))){return true;}
    // 活动循环入口已将所有原环境资源和view改写为输入边；本轮裸Data来源属于本轮新资源。
    // 仅在该实际循环内区分当前新资源与入口拥有资源，不能用于循环完成后的消费者。
    let fresh=|input:&Borrow,current:&Borrow|input.sources.is_empty()&&!input.inputs.is_empty()
        &&input.inputs.iter().all(|input|independent.contains(input)&&active.contains(&input.level))
        &&current.inputs.is_empty()&&!current.sources.is_empty()&&current.sources.iter().all(|source|matches!(source,Origin::Data(_)|Origin::Inactive));
    fresh(a,b)||fresh(b,a)
}
// Resource access binds its element variables once for both location and guard.
// These names belong to one static access, not machine inputs or resource owners.
fn resource_index_values(borrow:&Borrow)->BTreeSet<ValueIdentity>{
    fn visit(borrow:&Borrow,ids:&mut BTreeSet<ValueIdentity>){
        for path in borrow.sources.entries().map(|(_,when)|when).chain(borrow.inputs.entries().map(|(_,when)|when)).chain(borrow.calls.entries().map(|(_,when)|when)){
            ids.extend(bounds::Clause::scalar_identities([&path]).into_iter().filter(|id|matches!(id,ValueIdentity::ElementIndex(_))));
        }
        for source in &borrow.sources{match source{
            Origin::Element{parent,index,..}=>{if let Some(id)=index.and_then(bounds::Selector::identity){ids.insert(id);}visit(parent,ids);},
            Origin::Parameter(_,indices)=>{for index in indices{if let Some(id)=index.and_then(bounds::Selector::identity){ids.insert(id);}}},
            Origin::Projection{parent,..}=>visit(parent,ids),
            Origin::Position(origin)=>visit(&origin.resources(),ids),
            _=>{},
        }}
    }
    let mut ids=BTreeSet::new();visit(borrow,&mut ids);ids
}
impl Function {
    pub fn pending_values(&self)->bool {self.loops.values().any(|definition|definition.value.is_some())||self.uses.iter().any(|usage|match &usage.requirement {
        Requirement::Memory{snapshots:Some(snapshots),..}=>!snapshots.is_empty(),
        Requirement::Dataset{values:Some(_),..}=>true,
        _=>false,
    })}
    fn value_context(&self,values:Option<&BTreeMap<ValueIdentity,bounds::Proof>>)->bounds::Clause {
        match values {
            Some(values)=>bounds::Clause::values(self.uses.iter().enumerate().flat_map(|(memory,usage)|{
                let snapshots=match &usage.requirement{Requirement::Memory{snapshots:Some(snapshots),..}=>snapshots.as_ref(),_=>&[]};
                let read=match &usage.requirement{Requirement::Dataset{values:Some(read),..}=>Some(read.as_ref()),_=>None};
                snapshots.iter().enumerate().map(move |(component,snapshot)|{
                    let id=ValueIdentity::MemoryValue(memory,component);(snapshot.scalar,id,&values[&id])
                }).chain(read.into_iter().map(move |read|{
                    let id=ValueIdentity::MemoryValue(memory,0);(read[0].scalar,id,&values[&id])
                }))
            }).chain(self.loops.values().filter_map(|definition|definition.value).map(|(id,scalar)|(scalar,id,&values[&id])))),
            None=>bounds::Clause::default(),
        }
    }
    pub fn initialization_when(&self,fact:&address::Initialization)->bounds::Condition {
        // Direct parameter outputs already express the validated normal-return contract.
        if matches!(&fact.source,address::InitializationSource::Position(origin) if matches!(origin.root,address::Root::Parameter(_))) {
            return fact.when.clone();
        }
        let normal=match &self.normal_returns {Some(normal)=>normal.clone(),None=>bounds::Condition::constant(false)};
        normal.negate().logical(fact.when.clone(),true)
    }
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,memory:usize,arguments:BTreeMap<ValueIdentity,bounds::Proof>,snapshots:Option<Arc<[ValueSnapshot]>>,initialization:Vec<address::InitializationEffect>,path:bounds::Clause,source:crate::Source,span:Span,deferred:bool,live_task_places:Borrow)->usize {
        let call=self.calls.len();self.calls.push(Call{function:function.into(),memory,deferred,live_task_places,arguments,snapshots,initialization,outputs:BTreeSet::new(),path,source,span});call
    }
    pub fn result(&mut self,call:usize,output:Output)->Borrow {
        self.calls[call].outputs.insert(output.clone());
        Borrow{calls:view::Edges::from([CallResult{call,output}]),..Borrow::default()}
    }
    pub fn capture_resources(&mut self,value:&returns::Returned,output:impl Fn(ResourcePath)->Output){
        resource_outputs(value, |resource, borrow| {
            self.outputs.insert(output(resource), borrow);
        });
    }
    pub fn capture(&mut self,proof:&bounds::Proof,ty:&Ty,output:impl Fn(usize)->Output) {
        for component in components(ty) {self.outputs.insert(output(component),proof.view_borrow(component));}
    }
    fn input(&self,id:ValueIdentity)->bool {match id{ValueIdentity::Register(n)=>n<self.input_width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)|ValueIdentity::ParameterStorage(..)|ValueIdentity::DatasetContent(..)=>true,_=>false}}
    fn forwards_recursive_output(&self, name: &str, output: &Output, definition: &Borrow) -> bool {
        matches!(output, Output::Returned(_)) && definition.calls.iter().any(|result| {
            self.calls[result.call].function == name && result.output == *output
        })
    }

    fn address_borrows(&self,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,borrow:Borrow)->Option<Borrow> {
        let selected=borrow.map_sources(|source|match source {
            Origin::Parameter(id,indices)=>self.parameters.get(&id).copied().filter(|id|self.input(*id))
                .map(|id|Borrow::source(Origin::Parameter(id,indices.into_iter().map(|index|contract_index(index,self,BorrowProjection::Input,None)).collect()))).unwrap_or_else(||Borrow::source(Origin::Unresolved)),
            Origin::Position(origin) if matches!(origin.root,address::Root::Parameter(_)|address::Root::Derived(_)|address::Root::External(_)|address::Root::Dataset{..}|address::Root::Resource{..}) =>
                {
                    let mut origin=(*origin).clone();
                    if let address::Root::Resource{index,..}|address::Root::Dataset{index,..}=&mut origin.root {
                        *index=index.filter(|index|index.map(|id|self.parameters.get(&id).copied().filter(|id|self.input(*id))).is_some());
                    }
                    if let address::Root::Resource{borrow,..}|address::Root::Dataset{borrow,..}|address::Root::External(address::Identity::Storage(borrow))=&mut origin.root {
                        *borrow=borrow.map_sources(|source|match source{
                            Origin::Data(id) if !self.parameters.get(&id).is_some_and(|id|self.input(*id))=>Borrow::default(),
                            Origin::Position(origin)if origin.has_local()=>Borrow::default(),source=>Borrow::source(source)
                        });
                        if !borrow.active(){return Borrow::default();}
                    }
                    canonical(Borrow::source(Origin::Position(Arc::new(origin))),self,&BTreeMap::new())
                },
            Origin::Unresolved|Origin::External|Origin::Contents{..}|Origin::Relation(..)
            |Origin::TaskOutput{projection:view::TaskProjection::Writes|view::TaskProjection::Places,..}=>canonical(Borrow::source(source),self,&BTreeMap::new()),
            _=>Borrow::default(),
        });
        if !selected.active(){return None;}
        Some(selected.guards_at_call(parameters))
    }
    fn initialization_access(&self,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,read:&address::InitializationRead,when:&bounds::Clause,project:&impl Fn(&bounds::Clause,&BTreeMap<ValueIdentity,bounds::Proof>)->bounds::Clause)->Option<Access>{
        if read.missing().is_empty(){return None;}
        let initialized:Vec<_>=address::initialization_effects(&read.initialized,&bounds::Clause::default()).into_iter().filter_map(|effect|{
            let address::InitializationEffect::Position(mut fact)=effect else{return None;};
            if fact.source.borrow().has_local(){return None;}
            fact.source=match fact.source {
                address::InitializationSource::Position(origin)=>{
                    let value=canonical(Borrow::source(Origin::Position(Arc::new(origin))),self,&BTreeMap::new());
                    if value.unresolved(){return None;}
                    let Origin::Position(origin)=value.sources.iter().next()? else{return None;};
                    address::InitializationSource::Position(origin.as_ref().clone())
                },
                address::InitializationSource::Task(borrow)=>{
                    let value=canonical(borrow,self,&BTreeMap::new());
                    if value.unresolved(){return None;}
                    address::InitializationSource::Task(value)
                },
            };
            fact.when=fact.when.guaranteed_at_call(parameters);
            Some(fact)
        }).collect();
        self.address_access(parameters,read.borrow(),|borrows,when|Access::Initialized{borrows,initialized:address::merge_initializations(initialized),when},when,project)
    }
    fn address_access(&self,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,borrow:Borrow,make:impl FnOnce(Borrow,bounds::Condition)->Access,when:&bounds::Clause,project:&impl Fn(&bounds::Clause,&BTreeMap<ValueIdentity,bounds::Proof>)->bounds::Clause)->Option<Access>{
        let borrows = self.address_borrows(parameters,borrow)?;
        let when = project(when,parameters);
        when.possible().then(|| make(borrows, bounds::Condition::guard(when)))
    }
    fn access(&self,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,resources:Borrow,borrows:Borrow,kind:view::Exclusion,when:&bounds::Clause,project:&impl Fn(&bounds::Clause,&BTreeMap<ValueIdentity,bounds::Proof>)->bounds::Clause)->Option<Access>{
        if matches!(kind,view::Exclusion::Position(_)) {
            let resources = self.address_borrows(parameters,resources)?;
            return self.address_access(parameters,borrows, |borrows, when| Access::Resource { resources, borrows, kind, when }, when,project);
        }
        let mut parameters=std::borrow::Cow::Borrowed(parameters);
        let mut indices=resource_index_values(&resources);indices.extend(resource_index_values(&borrows));
        indices.extend(bounds::Clause::scalar_identities([when]).into_iter().filter(|id|matches!(id,ValueIdentity::ElementIndex(_))));
        let mut next=0;
        for id in indices{
            if let Some(target)=self.parameters.get(&id).copied().filter(|target|self.input(*target)){
                parameters.to_mut().entry(id).or_insert_with(||bounds::Proof::default().with_identity(target));
            }else{
                parameters.to_mut().insert(id,bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(next)));next+=1;
            }
        }
        fn external(function:&Function,borrow:Borrow,parameters:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow{
            borrow.map_sources(|source|match source{
                Origin::Data(id)=>function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Borrow::source(Origin::Data(id))).unwrap_or_default(),
                Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Origin::Parameter(id,indices.into_iter().map(|index|index.and_then(|index|index.at_call(parameters))).collect())).unwrap_or(Origin::Unresolved)),
                Origin::Element{parent,fields,index}=>Borrow::selected_element(external(function,*parent,parameters),fields,index.and_then(|index|index.at_call(parameters))),
                Origin::Projection{parent,fields}=>Borrow::projection(external(function,*parent,parameters),fields),
                Origin::Position(origin)=>origin.resources(),
                Origin::Contents{..}|Origin::Relation(..)=>canonical(Borrow::source(source),function,&BTreeMap::new()),
                Origin::Task{captures,..}=>external(function,*captures,parameters),
                Origin::TaskOutput{task,projection}=>external(function,*task,parameters).task_projection(projection),
                Origin::Unresolved=>Borrow::source(source),Origin::Inactive|Origin::External=>Borrow::default(),
            }).guards_at_call(parameters)
        }
        let resources=external(self,resources.resources(),&parameters);
        let borrows=external(self,match kind {
            view::Exclusion::Ownership=>borrows.owners(),
            view::Exclusion::View=>borrows.view_resources(),
            _=>borrows.resources(),
        },&parameters);
        if !resources.active()||!borrows.active(){return None;}
        let when=bounds::Condition::guard(project(when,&parameters).canonical());
        when.active().then_some(Access::Resource{resources,borrows,kind,when})
    }
    fn dataset_access(&self,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,borrow:Borrow,index:Option<bounds::Operand>,access:super::Access,atomic:bool,when:&bounds::Clause,owns_input:&impl Fn(ValueIdentity)->bool,project:&impl Fn(&bounds::Clause,&BTreeMap<ValueIdentity,bounds::Proof>)->bounds::Clause)->Option<Access> {
        fn inputs(borrow:&Borrow,function:&Function,owns_input:&impl Fn(ValueIdentity)->bool)->Borrow {
            borrow.map_sources(|source|match source {
                Origin::Data(id) if !function.parameters.get(&id).is_some_and(|id|function.input(*id)&&!owns_input(*id))=>Borrow::default(),
                Origin::Element{parent,fields,index}=>Borrow::selected_element(inputs(&parent,function,owns_input),fields,index),
                Origin::Projection{parent,fields}=>Borrow::projection(inputs(&parent,function,owns_input),fields),
                Origin::Position(origin)=>{
                    let mut origin=origin.map_borrows(|borrow|inputs(borrow,function,owns_input));
                    if let address::Root::Resource{borrow,index:None}=origin.root {origin.root=address::Root::Derived(borrow);}
                    origin.normalize()
                },
                source=>Borrow::source(source),
            })
        }

        let index=index.and_then(|index|index.map(|id|self.parameters.get(&id).copied().filter(|id|self.input(*id))));
        self.address_access(parameters,inputs(&borrow,self,owns_input),|borrows,when|Access::Dataset{borrows,when,index,access,atomic},when,project)
    }
}
#[derive(Clone,Default,PartialEq,Eq)]
struct State<'a> {
    name: &'a str,
    loops: Option<Arc<relation::Graph>>,
    entries: Option<BTreeMap<ValueIdentity, Borrow>>,
    interface: Interface,
}
#[derive(Clone, Copy)]
struct LoopScope<'a> {
    name: &'a str,
    loops: Option<&'a Arc<relation::Graph>>,
}
impl State<'_> {
    fn loop_scope(&self) -> LoopScope<'_> {
        LoopScope { name: self.name, loops: self.loops.as_ref() }
    }
}
type StorageObserver<'a> = dyn Fn(&str, crate::compiler::BorrowSolveStorage) + Sync + 'a;

struct States<'a> {
    functions: BTreeMap<String, State<'a>>,
    observe: Option<&'a StorageObserver<'a>>,
}

impl<'a> std::ops::Deref for States<'a> {
    type Target = BTreeMap<String, State<'a>>;
    fn deref(&self) -> &Self::Target { &self.functions }
}

impl std::ops::DerefMut for States<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.functions }
}


// Loop sources are solved only for actual output, contract and memory consumers.
fn position_roots(position:&Position,roots:&mut Vec<Borrow>) {
    match position {
        Position::ApplicationInput(input,..)=>roots.push(Borrow{inputs:view::Edges::from([*input]),..Borrow::default()}),
        Position::Borrowed(borrow,..)=>roots.push(borrow.as_ref().clone()),
        Position::Initialization(effect)=>initialization_roots(effect,roots),
        Position::Both(a,b)|Position::Conditional(_,a,b)=>{position_roots(a,roots);position_roots(b,roots);}
        Position::Indexed(choices,_)=>for position in choices.iter(){position_roots(position,roots);},
        _=>{},
    }
}
fn initialization_roots(effect:&address::InitializationEffect,roots:&mut Vec<Borrow>) {
    match effect {
        address::InitializationEffect::Position(fact)=>roots.push(fact.source.borrow()),
        address::InitializationEffect::Call{..}=>{},
        address::InitializationEffect::Iteration{iteration,..}=>{
            for entry in iteration.states.iter(){for position in &entry.initialized{position_roots(position,roots);}}
            for(scope,_)in &iteration.outputs{position_roots(scope,roots);}
        }
    }
}
fn used_loops(functions:&BTreeMap<String,&Function>,closed:&BTreeMap<String,Interface>)->BTreeMap<String,BTreeSet<LoopInput>> {
    let mut contracts=closed.iter().filter_map(|(name,interface)|(!interface.access.is_empty()||!interface.initialization.is_empty()).then_some(name.clone())).collect::<BTreeSet<_>>();
    contracts.extend(functions.iter().filter_map(|(name,function)|function.uses.iter().any(|usage|matches!(usage.requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Dataset{..}|Requirement::Initialized{..})).then_some(name.clone())));
    loop {
        let needed=functions.iter().filter_map(|(name,function)|function.calls.iter().any(|call|contracts.contains(&call.function)).then_some(name.clone())).collect::<BTreeSet<_>>();
        let before=contracts.len();contracts.extend(needed);if before==contracts.len(){break;}
    }
    functions.iter().map(|(name,function)|{
        let mut roots=function.outputs.values().cloned().collect::<Vec<_>>();
        for effect in &function.initialization{initialization_roots(effect,&mut roots);}
        for usage in &function.uses {
            if let Some(borrow)=usage.requirement.borrow(){roots.push(borrow.clone());}
            match &usage.requirement {
                Requirement::Unborrowed{resource,..}=>roots.push(resource.clone()),
                Requirement::Return{owned,..}=>roots.push(owned.clone()),
                Requirement::Initialized{read,..}=>{
                    for position in &read.positions{position_roots(position,&mut roots);}
                    for position in &read.initialized{position_roots(position,&mut roots);}
                }
                Requirement::Memory{snapshots,written,..}=>{
                    if let Some(written)=written{roots.push(written.borrowed_view());}
                    if let Some(snapshots)=snapshots{for snapshot in snapshots.iter(){position_roots(&snapshot.position,&mut roots);roots.push(snapshot.proof.borrowed_view());}}
                },
                Requirement::Address{..}|Requirement::Dataset{..}|Requirement::Nonzero{..}=>{},
            }
        }
        for call in &function.calls {
            roots.push(call.live_task_places.clone());
            if contracts.contains(&call.function){roots.extend(call.arguments.values().map(bounds::Proof::borrowed_view));}
        }
        let mut loops=BTreeSet::new();let mut calls=BTreeSet::new();
        while let Some(borrow)=roots.pop() {
            for source in &borrow.sources {
                match source {
                    Origin::Element{parent,..}|Origin::Projection{parent,..}|Origin::TaskOutput{task:parent,..}=>roots.push(parent.as_ref().clone()),
                    Origin::Task{captures,outputs,initialized,..}=>{roots.push(captures.as_ref().clone());roots.push(initialized.as_ref().clone());roots.extend(outputs.iter().cloned());},
                    _=>{},
                }
                if let Origin::Contents{origin,..}=source{roots.push(origin.resources());}
                if let Origin::Relation(graph,root)=source{roots.extend(graph.seed_borrows(*root).into_iter().cloned());}
                if let Origin::Position(origin)=source {
                    match &origin.root {address::Root::Resource{borrow,..}|address::Root::Derived(borrow)=>roots.push(borrow.clone()),_=>{}}
                }
            }
            for input in &borrow.inputs{if loops.insert(*input){roots.push(function.loops[input].borrow.clone());}}
            for result in &borrow.calls{if calls.insert(result.call){roots.extend(function.calls[result.call].arguments.values().map(bounds::Proof::borrowed_view));}}
        }
        (name.clone(),loops)
    }).collect()
}


fn call_content_sources(borrow:&Borrow,call:&Call,arguments:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow {
    borrow.map_sources(|source|match source {
        Origin::Relation(graph,root)=>Borrow{reads:view::Edges::from([ContentRead::Relation{memory:call.memory,graph:Arc::new(graph.at_call(arguments)),root,snapshots:call.snapshots.clone()}]),..Borrow::default()},
        Origin::Contents{origin,word}=>{
            let origin=origin.map_borrows(|parent|call_content_sources(parent,call,arguments)).bound_at_call(arguments);
            Borrow{reads:view::Edges::from([ContentRead::Call{memory:call.memory,origin:Arc::new(origin),word,snapshots:call.snapshots.clone()}]),..Borrow::default()}
        },
        Origin::Position(origin)=>origin.map_borrows(|parent|call_content_sources(parent,call,arguments)).normalize(),
        Origin::Element{parent,fields,index}=>Borrow::selected_element(call_content_sources(&parent,call,arguments),fields,index),
        Origin::Projection{parent,fields}=>Borrow::projection(call_content_sources(&parent,call,arguments),fields),
        Origin::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Origin::Task{
            captures:Box::new(call_content_sources(&captures,call,arguments)),
            initialized:Box::new(call_content_sources(&initialized,call,arguments)),
            outputs:outputs.into_iter().map(|output|call_content_sources(&output,call,arguments)).collect(),
            completions:completions.iter().map(|completion|completion.map_task(|task|call_content_sources(task,call,arguments))).collect(),
            guards:guards.into_iter().map(|mut guard|{
                guard.target=call_content_sources(&guard.target,call,arguments);
                guard
            }).collect(),
        }),
        Origin::TaskOutput{task,projection}=>call_content_sources(&task,call,arguments).task_projection(projection),
        source=>Borrow::source(source),
    })
}
fn input_contents(origin:&address::Origin,word:u32)->Borrow {
    match &origin.root {
        address::Root::External(_)=>Borrow::source(Origin::External),
        address::Root::Local{..}|address::Root::Buffer(_)=>Borrow::source(Origin::Unresolved),
        _=>Borrow::source(Origin::Contents{origin:Arc::new(origin.clone()),word}),
    }
}
fn same_content_storage(a:&address::Root,b:&address::Root)->bool {
    match(a,b){
        (address::Root::Derived(a),address::Root::Derived(b))=>a.sources.iter().eq(b.sources.iter())
            &&a.inputs.iter().eq(b.inputs.iter())&&a.calls.iter().eq(b.calls.iter())&&a.reads.iter().eq(b.reads.iter()),
        _=>a==b,
    }
}
fn content_indices(a:&address::Root,b:&address::Root)->Option<(Option<bounds::Operand>,Option<bounds::Operand>)> {
    match(a,b) {
        (address::Root::Resource{borrow:a,index:at}|address::Root::Dataset{borrow:a,index:at},
         address::Root::Resource{borrow:b,index:bt}|address::Root::Dataset{borrow:b,index:bt})
            if a==b=>Some((*at,*bt)),
        _=>None,
    }
}
fn content_word(origin:&address::Origin,word:u32)->Option<(usize,u32)>{
    let component=origin.layout.parts.iter().position(|(scalar,at)|*at==word&&*scalar==Scalar::U64)?;
    Some((component,origin.words[component]))
}
fn content_position(origin:&address::Origin,word:u32)->Option<Position>{
    origin.position_at(word).filter(|position|!matches!(position,Position::Initialized))
}
#[derive(Default)]
struct ContentAtRead {
    writes: Borrow,
    earlier: Option<bounds::Clause>,
}

impl ContentAtRead {
    fn write(&mut self, incoming: &Borrow, when: &bounds::Condition, complete: bool) {
        let selected=when.assume(&bounds::Clause::default(),true);
        let visible = self.earlier.as_ref().map_or_else(
            || selected.clone(),
            |earlier| bounds::Clause::both(earlier, &selected),
        );
        self.writes.union_restricted(incoming, &visible);
        if complete {
            let unchanged = when.assume(&bounds::Clause::default(), false);
            self.earlier = Some(match self.earlier.take() {
                Some(earlier) => bounds::Clause::both(&earlier, &unchanged),
                None => unchanged,
            });
        }
    }

    fn write_with_history(&mut self, incoming: &Borrow, when: &bounds::Condition, complete: bool,
        names: &BTreeMap<ValueIdentity, ValueIdentity>,
    ) {
        if names.is_empty() {
            self.write(incoming, when, complete);
        } else {
            self.write(&incoming.output_bound(names), &when.output_bound(names), complete);
        }
    }

    fn with_initial(self, initial: Borrow) -> Borrow {
        let initial = match self.earlier {
            Some(earlier) => initial.restricted_by(&earlier),
            None => initial,
        };
        initial.joined(self.writes)
    }
}

fn content_write_overlap<'a>(
    query: Option<(&address::Origin, u32)>, position: Option<&Position>,
    target: &address::Origin, word: u32, active: impl FnOnce() -> Option<&'a bounds::Clause>,
) -> Option<bool> {
    let active=active()?;
    let complete = query.is_some_and(|(origin, at)| {
        let storage=match content_indices(&origin.root,&target.root) {
            Some((Some(a),Some(b)))=>bounds::Condition::index_equal(a,b).truth_in(active)==Some(true),
            Some(_)=>false,
            None=>same_content_storage(&origin.root,&target.root),
        };
        origin.base == target.base && storage
            && content_word(origin, at).zip(content_word(target, word))
                .is_some_and(|((_, read), (_, written))| read == written)
    }) || query.is_none_or(|(origin,_)|content_indices(&origin.root,&target.root).is_none())
        && position.zip(content_position(target, word).as_ref()).is_some_and(|(read, written)| read == written);

    if complete {
        return Some(true);
    }
    let position = position?;
    address::MemoryEffect::position_may_write(target, active, position).then_some(false)
}

fn content_write_paths(
    query: Option<(&address::Origin, u32)>, position: Option<&Position>,
    targets: &Borrow, word: u32, execution: &bounds::Condition,
) -> bounds::Condition {
    let mut hit = bounds::Clause::impossible();
    let mut miss = execution.assume(&bounds::Clause::default(), false);
    for (source, selected) in targets.sources.entries() {
        let mut active = execution.assume(&selected, true);
        if let (Some((read,_)),Origin::Position(target))=(query,source)
            && let Some((Some(a),Some(b)))=content_indices(&read.root,&target.root)
        {
            let same=bounds::Condition::index_equal(a,b);
            miss=bounds::Clause::either(&miss,&same.assume(&active,false));
            active=same.assume(&active,true);
        }
        if !active.possible() { continue; }
        let overlap = match source {
            Origin::Position(origin) => content_write_overlap(query, position, origin, word, || Some(&active)),
            Origin::External => Some(false),
            _ => None,
        };
        if overlap.is_some() {
            hit = bounds::Clause::either(&hit, &active);
        }
        if overlap != Some(true) {
            miss = bounds::Clause::either(&miss, &active);
        }
    }
    bounds::Condition::paths(hit, miss)
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LoopSources {
    Symbolic,
    Deferred,
    Resolved,
}

enum EvaluationInput {
    Content(ContentRead),
    Relation(Arc<relation::Graph>, usize),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum EvaluationIdentity<'a> {
    Content(&'a ContentRead),
    Relation(*const relation::Graph, usize),
}

impl EvaluationInput {
    fn identity(&self) -> EvaluationIdentity<'_> {
        match self {
            Self::Content(read) => EvaluationIdentity::Content(read),
            Self::Relation(graph, root) => EvaluationIdentity::Relation(Arc::as_ptr(graph), *root),
        }
    }
}
impl PartialEq for EvaluationInput {
    fn eq(&self, other: &Self) -> bool { self.identity() == other.identity() }
}
impl Eq for EvaluationInput {}
impl PartialOrd for EvaluationInput {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for EvaluationInput {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.identity().cmp(&other.identity()) }
}

type StoredLocations = BTreeSet<(Arc<address::Origin>, u32)>;

struct LoopBindings<'a> {
    outer: Option<&'a LoopBindings<'a>>,
    inputs: &'a BTreeMap<LoopInput, usize>,
    values: &'a BTreeMap<usize, Borrow>,
}

impl LoopBindings<'_> {
    fn get(&self, input: &LoopInput) -> Option<&Borrow> {
        let mut bindings = Some(self);
        while let Some(scope) = bindings {
            if let Some(value) = scope.inputs.get(input).and_then(|id| scope.values.get(id)) {
                return Some(value);
            }
            bindings = scope.outer;
        }
        None
    }
}

struct BorrowEvaluation<'a>{
    function:&'a Function,state:LoopScope<'a>,known:&'a States<'a>,expand_loops:bool,deferred_reads:bool,
    loop_bindings: Option<&'a LoopBindings<'a>>,
    values: BTreeMap<(LoopSources, EvaluationInput), Borrow>,
    stored: Option<Arc<StoredLocations>>,
    exit_writes: Option<&'a ReturnedWrites>,
    stored_paths: StoredPaths<'a>,
}
// A replay may visit the same physical write for many words or exits.
// The callee contracts and argument bindings stay fixed inside this evaluation,
// including the nested relation scopes that replay its writes.
type CallWritePaths = std::sync::Mutex<
    BTreeMap<(usize, usize), Arc<std::sync::OnceLock<bounds::Condition>>>,
>;

enum StoredPaths<'a> {
    Own(CallWritePaths),
    Borrowed(&'a CallWritePaths),
}

impl Default for StoredPaths<'_> {
    fn default() -> Self { Self::Own(Default::default()) }
}

impl StoredPaths<'_> {
    fn paths(&self) -> &CallWritePaths {
        match self {
            Self::Own(paths) => paths,
            Self::Borrowed(paths) => paths,
        }
    }

    fn project(&self, call_index: usize, index: usize, guard: &bounds::Condition,
        path: &bounds::Clause, call: &Call,
    ) -> bounds::Condition {
        let projected = self.paths().lock().unwrap().entry((call_index, index))
            .or_default().clone();
        projected.get_or_init(|| {
            bounds::Condition::execution(path).logical(guard.at_call(&call.arguments), false)
        }).clone()
    }
}

impl Function {
    fn repeated_read(&self, memory: usize) -> bool {
        self.visit_carried_writes(memory, |_, _, _| true)
    }
}

impl BorrowEvaluation<'_>{
    fn content_event<'call>(&mut self, query: Option<(&address::Origin, u32)>, position: Option<&Position>,
        memory: usize, call: impl FnOnce() -> Option<(usize, &'call Call)>,
        mut write: impl FnMut(&Borrow, &bounds::Condition, bool)) {
        let(function,known)=(self.function,self.known);
        let usage=&function.uses[memory];
        let Requirement::Memory{borrow,access,written,address_fields,..}=&usage.requirement else{return;};
        if let Some((call_index, call)) = call() {
            for (index, access) in known[&call.function].interface.access.iter().enumerate().rev() {
                let Access::Stored { borrows, word, value, when } = access else { continue; };
                let target = self.evaluate(&call_content_sources(borrows, call, &call.arguments).at_call(&call.arguments));
                let execution = self.stored_paths.project(call_index, index, when, &usage.path, call);
                let path = content_write_paths(query, position, &target, *word, &execution);
                if path.active() {
                    let stored = self.evaluate(&call_content_sources(value, call, &call.arguments).at_call(&call.arguments));
                    write(&stored, &path, true);
                }
            }
            return;
        }
        if !access.is_some_and(super::Access::writes){return;}
        // This opaque write already contributes the same possible external
        // contents for every queried word, without replacing earlier contents.
        if written.is_none()
            && borrow.sources.when(&Origin::External) == bounds::Clause::default()
        {
            let when = &usage.path;
            // Every queried word consumes the same immutable write path.
            if *usage.opaque_write_possible.get_or_init(|| when.possible()) {
                write(&Borrow::source(Origin::External), &bounds::Condition::execution(when), false);
            }
            return;
        }
        let targets=self.evaluate(borrow);
        if let Some(written) = written {
            let execution = bounds::Condition::execution(&usage.path);
            for &(word, component) in address_fields.iter().rev() {
                let path = content_write_paths(query, position, &targets, word, &execution);
                if path.active() {
                    let stored = self.evaluate(&written.view_borrow(component));
                    write(&stored, &path, true);
                }
            }
            return;
        }
        for (source, selected) in targets.sources.entries().rev() {
            let when = std::cell::LazyCell::new(|| bounds::Clause::both(&usage.path, &selected));
            let active = std::cell::LazyCell::new(|| when.possible().then_some(&*when));
            match source {
                Origin::Position(_) => {
                    if let Some(position) = position && let Some(path) = *active {
                        let effect = address::MemoryEffect::of(
                            &Borrow::source(source.clone()).restricted(path), super::Access::Write, None,
                        );
                        if effect.written_bytes(position, Scalar::U64, None) != 0 {
                            write(&Borrow::source(Origin::External), &bounds::Condition::execution(path), false);
                        }
                    }
                },
                Origin::External => {
                    if let Some(path) = *active {
                        write(&Borrow::source(Origin::External), &bounds::Condition::execution(path), false);
                    }
                },
                _ => {},
            }
        }
    }

    fn content_before(&mut self,origin:&address::Origin,word:u32,memory:usize,snapshots:Option<&[ValueSnapshot]>)->Borrow{

        let position=content_position(origin,word);
        if let Some(snapshots)=snapshots{
            let index = match &origin.root {
                address::Root::Resource { index, .. } | address::Root::Dataset { index, .. } => *index,
                _ => None,
            };
            let captured_position = match &origin.root {
                address::Root::Resource { .. } | address::Root::Dataset { .. } => origin
                    .memory_regions(&self.function.uses[memory].path)
                    .and_then(|regions| regions.into_iter().find(|region| region.word == word
                        && region.offset == 0 && region.size == Scalar::U64.size()))
                    .map(|region| region.position),
                _ => position.clone(),
            };
            let mut captured=None;
            for snapshot in snapshots.iter(){
                if snapshot.scalar!=Scalar::U64 {continue;}
                let snapshot_position=match &snapshot.position {
                    Position::Borrowed(borrow,layout,word)=>Position::Borrowed(
                        Arc::new(self.evaluate(borrow)),layout.clone(),*word,
                    ),
                    position=>position.clone(),
                };
                if captured_position.as_ref()==Some(&snapshot_position){
                    let mut bindings = BTreeMap::new();
                    match (snapshot.index, index) {
                        (Some(bounds::Operand::Value(id @ ValueIdentity::ElementIndex(_))), Some(index)) => {
                            bindings.insert(id, bounds::Proof::element_operand(index, (0, u32::MAX)));
                        }
                        (Some(bounds::Operand::Value(id @ ValueIdentity::ElementIndex(_))), None) => {
                            bindings.insert(id,bounds::Proof::default());
                        }
                        (Some(selected), Some(index)) if bounds::Condition::index_equal(selected, index)
                            .truth_in(&self.function.uses[memory].path) != Some(true) => continue,
                        (Some(_), None)=>continue,
                        _ => {}
                    }
                    let value=self.evaluate(&snapshot.proof.view_borrow(0)).substitute_values(&bindings);
                    if let Some(old)=&mut captured{Borrow::union(old,&value);}else{captured=Some(value);}
                }
            }
            if let Some(value)=captured{return value;}
        }
        let mut result=ContentAtRead::default();
        let function = self.function;
        function.visit_content_history(memory, |at, call| {
            let names = std::cell::LazyCell::new(|| function.history_names(at, memory, None));
            self.content_event(Some((origin, word)), position.as_ref(), at, || call,
                |borrow, when, complete| result.write_with_history(borrow, when, complete, &names));
        });
        result.with_initial(input_contents(origin,word))
    }

    fn stored_locations<'p>(
        &mut self,
        parameters: impl Fn() -> &'p BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> Arc<StoredLocations> {
        if let Some(stored) = &self.stored {
            return Arc::clone(stored);
        }
        let (function, known) = (self.function, self.known);
        let exposed = exposed_resources(function, |output| self.evaluate(&function.outputs[output]));
        let mut result = BTreeSet::new();
        function.visit_stored_locations(|path, site| match site {
            relation::StoredLocation::Call(call_index, call) => {
                for (index, access) in known[&call.function].interface.access.iter().enumerate() {
                    let Access::Stored { borrows, word, when, .. } = access else { continue; };
                    let target = self.evaluate(&call_content_sources(borrows, call, &call.arguments).at_call(&call.arguments));
                    let write_path = std::cell::LazyCell::new(|| self.stored_paths.project(call_index, index, when, path, call)
                        .assume(&bounds::Clause::default(), true));
                    for (source, when) in target.sources.entries() {
                        if let Origin::Position(origin) = source
                            && bounds::Clause::both(&when, &write_path).possible()
                        {
                            result.insert((origin.clone(), *word));
                        }
                    }
                }
            }
            relation::StoredLocation::Direct(borrow, fields) => {
                let target = self.evaluate(borrow);
                for (source, when) in target.sources.entries() {
                    if let Origin::Position(origin) = source
                        && bounds::Clause::both(&when, path).possible()
                    {
                        for &(word, _) in fields {
                            result.insert((origin.clone(), word));
                        }
                    }
                }
            }
        });
        result.retain(|(origin, _)| stored_target(origin, function, &exposed, &parameters).active());
        let result = Arc::new(result);
        self.stored = Some(Arc::clone(&result));
        result
    }

    fn content_at_exit(&mut self, origin: &address::Origin, word: u32, memory: usize) -> Borrow {
        self.content_before(origin, word, memory, None).restricted_by(&self.function.uses[memory].path)
    }

    fn content_read(&mut self,read:&ContentRead,sources:LoopSources)->Borrow{
        let key = (sources, EvaluationInput::Content(read.clone()));
        if let Some(value)=self.values.get(&key){return value.clone();}
        let function=self.function;
        let value=match read {
            ContentRead::Relation{memory,graph,root,snapshots}=>{
                let (graph,root)=graph.bind_incoming(*root,*memory,snapshots.clone(),function,self.state,self.known);
                graph.solve(root,self).restricted_by(&function.uses[*memory].path)
            },
            ContentRead::Returned{memory}=>{
                let mut value=Borrow::default();
                let parameters = std::cell::LazyCell::new(|| public_parameters(function));
                if let Some(history) = self.exit_writes.filter(|_| matches!(sources, LoopSources::Resolved)
                    && self.loop_bindings.is_none()) {
                    for location in history {
                        let mut result = ContentAtRead::default();
                        let first = location.events.partition_point(|event| event.at >= *memory);
                        for event in &location.events[first..] {
                            let names = function.history_names(event.at, *memory, None);
                            for write in &event.writes {
                                result.write_with_history(&write.borrow, &write.when, write.complete, &names);
                            }
                        }
                        let contents = result.with_initial(input_contents(&location.origin, location.word))
                            .restricted_by(&function.uses[*memory].path);
                        value.union(&contents);
                    }
                } else {
                    for (origin, word) in self.stored_locations(|| &parameters).iter() {
                        value.union(&self.content_at_exit(origin, *word, *memory));
                    }
                }
                value
            },
            ContentRead::Value { memory, .. } | ContentRead::Call { memory, .. }
                if function.repeated_read(*memory) =>
            {
                let (graph, root) = relation::Graph::loop_content(read, function, self.state, self.known);
                graph.solve(root, self)
            }
            ContentRead::Value{memory,word,borrow,snapshots}=>{
                let targets=self.evaluate(borrow);
                let mut value=Borrow::default();
                for(source,when)in targets.sources.entries(){
                    if let Origin::Position(origin)=source{
                        value.union_restricted(&self.content_before(origin,*word,*memory,Some(snapshots)), &when);
                    }
                }
                value.restricted_by(&function.uses[*memory].path)
            },
            ContentRead::Snapshot{memory,position,index,initial,..}=>{
                let value=self.evaluate_sources(&initial.view_borrow(0),sources);
                let mut result=ContentAtRead::default();
                let query=match (position,index) {
                    (Position::Borrowed(borrow,base,word),Some(index))=>Some(address::Origin{
                        root:address::Root::Resource{borrow:borrow.as_ref().clone(),index:Some(*index)},
                        layout:address::Layout::of(&Ty::scalar(Scalar::U64)),base:base.clone(),
                        words:Arc::from([*word]),positions:Arc::from([]),writable:false,initialized:Arc::from([true]),
                    }),
                    _=>None,
                };
                let function = self.function;
                self.content_event(query.as_ref().map(|origin| (origin, 0)), Some(position), *memory,
                    || function.call_at_memory(*memory, true),
                    |borrow, when, complete| result.write(borrow, when, complete));
                result.with_initial(value).restricted_by(&function.uses[*memory].path)
            },
            ContentRead::Call{memory,origin,word,snapshots}=>{
                let targets=self.evaluate(&Borrow::source(Origin::Position(origin.clone())));
                let mut value=Borrow::default();
                for(source,when)in targets.sources.entries(){
                    if let Origin::Position(origin)=source{
                        value.union_restricted(&self.content_before(origin,*word,*memory,snapshots.as_deref()), &when);
                    }
                }
                value.restricted_by(&function.uses[*memory].path)
            },
        };
        self.values.insert(key,value.clone());
        value
    }

    fn relation(&mut self, graph: &Arc<relation::Graph>, root: usize, sources: LoopSources) -> Borrow {
        // The original graph stays live with the key. A different evaluation
        // scope owns its own results and resolves its own loop bindings.
        let key = (sources, EvaluationInput::Relation(graph.clone(), root));
        if let Some(value) = self.values.get(&key) {
            return value.clone();
        }
        let value = if graph.pending(root) {
            let (graph, root) = graph.bind(root, self.function, self.state, self.known);
            graph.solve(root, self)
        } else {
            graph.solve(root, self)
        };
        self.values.insert(key, value.clone());
        value
    }

    fn evaluate(&mut self,borrow:&Borrow)->Borrow{
        let sources=if self.deferred_reads{LoopSources::Deferred}else if self.expand_loops{LoopSources::Resolved}else{LoopSources::Symbolic};
        self.evaluate_sources(borrow,sources)
    }

    fn evaluate_sources(&mut self,borrow:&Borrow,sources:LoopSources)->Borrow{
        let(function,state,known)=(self.function,self.state,self.known);
        let mut value=Borrow{..Borrow::default()};
        for(source,when)in borrow.sources.entries() {
            let resolved=match source {
                Origin::Relation(graph,root)=>{
                    if matches!(sources,LoopSources::Deferred){Borrow::source(source.clone())}else{
                        self.relation(graph, *root, sources)
                    }
                },
                Origin::Element{parent,fields,index}=>Borrow::selected_element(self.evaluate(parent),fields.clone(),*index),
                Origin::Projection{parent,fields}=>Borrow::projection(self.evaluate(parent),fields.clone()),
                Origin::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Origin::Task{
                    captures:Box::new(self.evaluate(captures)),
                    initialized:Box::new(self.evaluate(initialized)),
                    outputs:outputs.iter().map(|output|self.evaluate(output)).collect(),
                    completions:completions.iter().map(|completion|completion.map_task(|task|self.evaluate(task))).collect(),
                    guards:guards.iter().cloned().map(|mut guard|{
                        guard.target=self.evaluate(&guard.target);
                        guard
                    }).collect(),
                }),
                Origin::TaskOutput{task,projection}=>self.evaluate(task).task_projection(projection.clone()),
                Origin::Position(origin)=>origin.map_borrows(|root|self.evaluate(root)).normalize(),
                Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(origin.map_borrows(|root|self.evaluate(root))),word:*word}),
                source=>Borrow::source(source.clone()),
            };
            value.union_restricted(&resolved, &when);
        }
        for(input,when)in borrow.inputs.entries(){if matches!(sources,LoopSources::Resolved){
            let bound = self.loop_bindings.and_then(|scope| scope.get(input));
            match (bound, state.loops) {
                (Some(bound), _) => value.union_restricted(bound, &when),
                (None, Some(graph)) =>
                    value.union_restricted(&self.relation(graph, graph.input_root(*input), sources), &when),
                (None, None) => {}
            }
        }else{value.inputs.insert_when(*input,when.clone());}}
        for(read,when)in borrow.reads.entries(){if matches!(sources,LoopSources::Deferred){value.reads.insert_when(read.clone(),when);}else{value.union_restricted(&self.content_read(read,sources), &when);}}
        for(result,when)in borrow.calls.entries(){
            let call=&function.calls[result.call];
            let Some(output)=known[&call.function].interface.outputs.get(&result.output)else {value.sources.insert_when(Origin::Unresolved,when.clone());continue;};
            let actual=call_content_sources(output,call,&call.arguments).at_call(&call.arguments);
            value.union_restricted(&self.evaluate(&actual), &when);
        }value
    }
}

// Select storage that survives this function; private stack slots and unreturned
// owned resources have no caller-side stored output.
fn stored_target<'p>(
    origin: &address::Origin, function: &Function, exposed: &BTreeMap<Origin, Borrow>,
    parameters: impl FnOnce() -> &'p BTreeMap<ValueIdentity, bounds::Proof>,
) -> Borrow {
    fn escaping(borrow:&Borrow,function:&Function,exposed:&BTreeMap<Origin,Borrow>)->Borrow{
        borrow.map_sources(|source|match source{
            Origin::Data(id) if !function.parameters.get(&id).is_some_and(|id|function.input(*id))&&!exposed.contains_key(&Origin::Data(id))=>Borrow::default(),
            Origin::Position(origin) if origin.has_local()=>Borrow::default(),
            Origin::Element{parent,fields,index}=>Borrow::selected_element(escaping(&parent,function,exposed),fields,index),
            Origin::Projection{parent,fields}=>Borrow::projection(escaping(&parent,function,exposed),fields),
            source=>Borrow::source(source),
        })
    }
    if origin.has_local(){return Borrow::default();}
    let origin=origin.map_borrows(|borrow|escaping(borrow,function,exposed));
    if matches!(&origin.root,address::Root::Resource{borrow,..}|address::Root::Dataset{borrow,..}|address::Root::Derived(borrow)|address::Root::External(address::Identity::Storage(borrow)) if !borrow.active()){return Borrow::default();}
    canonical_relation(Borrow::source(Origin::Position(Arc::new(origin))), function, None, exposed, parameters(), BorrowProjection::Input)
}
fn evaluate(borrow:&Borrow,function:&Function,state:&State,known:&States,expand_loops:bool)->Borrow {
    BorrowEvaluation{function,state:state.loop_scope(),known,expand_loops,deferred_reads:false,loop_bindings:None,values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()}.evaluate(borrow)
}

#[derive(Clone, Copy)]
enum BorrowProjection {
    Input,
    Result,
    Initialization,
}

fn contract_index(
    index: Option<bounds::Selector>, function: &Function, projection: BorrowProjection,
    source: Option<&bounds::Clause>,
) -> Option<bounds::Selector> {
    let name = |id| match id {
        ValueIdentity::ElementIndex(_) => Some(id),
        _ => function.parameters.get(&id).copied().filter(|id| {
            function.input(*id)
                || matches!(projection, BorrowProjection::Result) && matches!(id, ValueIdentity::Register(_))
        }),
    };
    let index = index?;
    if let Some(public) = index.map(name) { return Some(public); }
    if !matches!(projection, BorrowProjection::Result) { return None; }
    let exits = function.normal_returns.as_ref()?;
    let source = source?;
    match index {
        bounds::Selector::Direct(bounds::Operand::Value(id)) => exits.return_selector(id, source),
        bounds::Selector::Affine { .. } | bounds::Selector::Prefix(_) => exits.return_prefix(index, source),
        bounds::Selector::Direct(bounds::Operand::Constant(..)) => Some(index),
    }?.map(name)
}
fn public_parameters(function: &Function) -> BTreeMap<ValueIdentity, bounds::Proof> {
    function.parameters.iter()
        .map(|(&local, &formal)| (local, bounds::Proof::default().with_identity(formal)))
        .collect()
}

fn canonical(borrow: Borrow, function: &Function, exposed: &BTreeMap<Origin, Borrow>) -> Borrow {
    canonical_relation(borrow, function, None, exposed, &public_parameters(function), BorrowProjection::Input)
}
fn canonical_relation(
    borrow: Borrow, function: &Function, resource: Option<&Borrow>, exposed: &BTreeMap<Origin, Borrow>,
    parameters: &BTreeMap<ValueIdentity, bounds::Proof>, projection: BorrowProjection,
) -> Borrow {
    canonical_with_guards(borrow, function, resource, exposed, &bounds::PublicGuards::new(parameters), projection)
}

fn canonical_address(
    origin: &address::Origin, function: &Function, exposed: &BTreeMap<Origin, Borrow>,
    guards: &bounds::PublicGuards<'_>,
) -> address::Origin {
    let mut value = origin.map_borrows(|borrow| {
        canonical_with_guards(borrow.clone(), function, None, exposed, guards, BorrowProjection::Input)
    });
    if let address::Root::External(address::Identity::Value(ids)) = &mut value.root {
        *ids = ids.iter().filter_map(|id| function.parameters.get(id).copied().filter(|id| function.input(*id))).collect();
    }
    if let address::Root::Resource { borrow, index: Some(index) }
        | address::Root::Dataset { borrow, index: Some(index) } = &mut value.root
    {
        match index.map(|id| function.parameters.get(&id).copied().filter(|id| function.input(*id))) {
            Some(bound) => *index = bound,
            None => *borrow = Borrow::source(Origin::Unresolved),
        }
    }
    if !matches!(value.root, address::Root::Local { .. }) {
        value.positions = Arc::from([]);
    }
    value
}

fn canonical_with_guards(
    mut borrow: Borrow, function: &Function, resource: Option<&Borrow>, exposed: &BTreeMap<Origin, Borrow>,
    guards: &bounds::PublicGuards<'_>, projection: BorrowProjection,
) -> Borrow {
    let parameters = guards.bindings;
    let selection = match projection {
        BorrowProjection::Result => BorrowProjection::Result,
        _ => BorrowProjection::Input,
    };
    // Impossible source edges carry known absence, not a live private owner.
    // Keep that knowledge in the existing Inactive representation at the ABI.
    let known=!borrow.sources.is_empty();
    borrow.sources.retain_paths(|path| guards.possible(path));
    if known&&borrow.sources.is_empty(){borrow.sources.insert(Origin::Inactive);}
    let renamed=|source, when: &bounds::Clause|{
        if matches!(&source,Origin::Element{..}|Origin::Projection{..}) && let Some(outputs)=exposed.get(&source){
            return match resource{Some(resource)=>resource.clone(),None=>outputs.clone()};
        }
        match source {
        Origin::Data(id)=>{
            let input=function.parameters.get(&id).copied().filter(|id|function.input(*id));
            if let Some(input)=input{Borrow::source(Origin::Data(input))}
            else if let Some(resource)=resource{resource.clone()}
            else if let Some(outputs)=exposed.get(&Origin::Data(id)){outputs.clone()}
            else{Borrow::source(Origin::Unresolved)}
        }
        Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().map(|id|Origin::Parameter(id,indices.into_iter().map(|index|contract_index(index,function,selection,Some(when))).collect())).unwrap_or(Origin::Unresolved)),
        Origin::Element{parent,fields,index}=>Borrow::selected_element(canonical_with_guards(*parent,function,None,exposed,guards,selection),fields,contract_index(index,function,selection,Some(when))),
        Origin::Projection{parent,fields}=>Borrow::projection(canonical_with_guards(*parent,function,None,exposed,guards,selection),fields),
        Origin::Task{captures,outputs,initialized,guards:task_guards,completions}=>Borrow::source(Origin::Task{
            captures:Box::new(canonical_with_guards(*captures,function,None,exposed,guards,BorrowProjection::Input)),
            initialized:Box::new(canonical_with_guards(*initialized,function,None,exposed,guards,BorrowProjection::Initialization)),
            outputs:outputs.into_iter().map(|output|canonical_with_guards(output,function,None,exposed,guards,BorrowProjection::Input)).collect(),
            completions:completions.iter().map(|completion|completion.map_task(|task|canonical_with_guards(task.clone(),function,None,exposed,guards,BorrowProjection::Input))).collect(),
            guards:task_guards.into_iter().map(|mut guard|{
                guard.target=canonical_with_guards(guard.target,function,None,exposed,guards,BorrowProjection::Input);
                guard.map_arguments(|proof|proof.at_call(parameters));
                guard.map_inputs(|word|word.map(|id|function.parameters.get(&id).copied()));
                guard
            }).collect(),
        }),
        Origin::TaskOutput{task,projection}=>canonical_with_guards(*task,function,None,exposed,guards,BorrowProjection::Input).task_projection(projection.map_result(|id|function.parameters.get(&id).copied())),
        Origin::Position(origin)=>Borrow::source(Origin::Position(Arc::new(canonical_address(&origin,function,exposed,guards)))),
        Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(canonical_address(&origin,function,exposed,guards)),word}),
        Origin::Relation(graph,root)=>Borrow::source(Origin::Relation(Arc::new(graph.canonical(function,exposed,guards)),root)),
        Origin::Unresolved|Origin::Inactive|Origin::External=>Borrow::source(source),
    }};
    if matches!(projection, BorrowProjection::Initialization) {
        return borrow.initialization_at_call(parameters).map_source_paths(renamed);
    }
    match resource {
        // 拥有输出的来源改名不附加条件，先投影正式输入关系。
        Some(_) => borrow.map_sources_at_call(parameters, renamed),
        // 借用输出需先合成实际拥有者条件，保留来源相关性。
        None=>borrow.map_source_paths(renamed).project_guards(guards),
    }
}


fn resolved_position(position:&Position,path:&bounds::Clause,map:&impl Fn(&Borrow)->Borrow,application:&impl Fn(LoopInput,Option<ownership::ApplicationIndex>,u32)->Position)->Position {
    match position {
        Position::ApplicationInput(input,index,word)=>application(*input,*index,*word),
        Position::Borrowed(borrow,layout,word)=>{
            let borrow=address::borrow_in_path(&map(borrow),path);
            let next=address::borrowed_positions(&borrow,layout,&[*word]).remove(0);
            if next==*position{next}else{resolved_position(&next,path,map,application)}
        }
        Position::Both(a,b)=>Position::both(resolved_position(a,path,map,application),resolved_position(b,path,map,application)),
        Position::Conditional(when,a,b)=>match when.condition().truth_in(path) {
            Some(true)=>resolved_position(a,path,map,application),Some(false)=>resolved_position(b,path,map,application),
            None=>Position::selected(when.as_ref().clone(),resolved_position(a,path,map,application),resolved_position(b,path,map,application)),
        },
        Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|resolved_position(choice,path,map,application)).collect(),*index),
        _=>position.clone(),
    }
}
fn bind_application_position(position:&Position,input:LoopInput,scope:&Position,bindings:&BTreeMap<ValueIdentity,bounds::Proof>,path:&bounds::Clause)->Position {
    match position {
        Position::ApplicationInput(at,index,word)if *at==input=>scope.application_word(*index,*word),
        Position::Both(a,b)=>Position::both(bind_application_position(a,input,scope,bindings,path),bind_application_position(b,input,scope,bindings,path)),
        Position::Conditional(when,a,b)=>match when.condition().at_call(bindings).truth_in(path) {
            Some(true)=>bind_application_position(a,input,scope,bindings,path),
            Some(false)=>bind_application_position(b,input,scope,bindings,path),
            None=>Position::selected(when.as_ref().clone(),bind_application_position(a,input,scope,bindings,path),bind_application_position(b,input,scope,bindings,path)),
        },
        Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|bind_application_position(choice,input,scope,bindings,path)).collect(),*index),
        _=>position.clone(),
    }
}
fn resolved_application_position(input:LoopInput,index:Option<ownership::ApplicationIndex>,word:u32,initialized:&BTreeSet<Position>,path:&bounds::Clause,function:&Function,state:&State,known:&States,visiting:&mut BTreeSet<Position>)->Position {
    let position=Position::ApplicationInput(input,index,word);
    if position.initialized_by(initialized){return Position::Initialized;}
    // An unchanged cycle supplies no counterexample; every actual entry and
    // continued producer is still checked below in the same source graph.
    if !visiting.insert(position.clone()){return Position::Initialized;}
    let definition=&function.loops[&input];let resource=definition.resource.as_ref().unwrap();
    let mut result=Position::Initialized;
    let mut invariant=function.parameters.keys().map(|id|(*id,bounds::Proof::default().with_identity(*id))).collect::<BTreeMap<_,_>>();
    for(name,binding)in &definition.states[0].env {
        if let Some((ty,proof))=binding_value(binding) {
            for component in 0..ty.components.len() {
                if let Some(id)=proof.known_identity(component) && definition.states.iter().skip(1).all(|producer|
                    producer.env.get(name).and_then(binding_value).is_some_and(|(_,proof)|proof.known_identity(component)==Some(id))) {
                    invariant.insert(id,bounds::Proof::default().with_identity(id));
                }
            }
        }
    }
    for producer in definition.states.iter() {
        let mut bindings=invariant.clone();bindings.extend(definition.control.bindings(producer));
        let read_path=path.project_guard(&bindings);
        let source_path=bounds::Clause::both(&producer.path,&read_path);
        if !source_path.possible(){continue;}
        let Some(data)=producer.env.get(&resource.name).and_then(|binding|view::data_at(binding,&resource.path))else{continue;};
        let Some(scope)=&data.initialization else{continue;};
        let mut facts=resolved_initialization(&producer.initialized,&source_path,function,state,known);
        facts.extend(initialized.iter().map(|fact|bind_application_position(fact,input,scope,&bindings,&source_path)));
        let wanted=scope.application_word(index,word);
        let ready=wanted.initialized_by(&facts) || index.is_none() && data.length.minimum()==data.length.maximum()
            && scope.application_prefix_initialized(data.length.minimum(),word,&facts);
        let value=if ready{Position::Initialized}else{
            let visit=std::cell::RefCell::new(&mut *visiting);
            resolved_position(&wanted,&source_path,&|borrow|evaluate(borrow,function,state,known,true),
                &|next,at,word|resolved_application_position(next,at,word,&facts,&source_path,function,state,known,&mut visit.borrow_mut()))
        };
        result=Position::both(result,value);
    }
    visiting.remove(&position);result
}

struct InitializedEffects {
    facts:Vec<address::Initialization>,
    iterations:Vec<(BTreeSet<Position>,bounds::Condition)>,
}
impl InitializedEffects {
    fn positions(&self,path:&bounds::Clause)->BTreeSet<Position> {
        let mut result=self.facts.iter().filter(|fact|fact.when.truth_in(path)==Some(true)).flat_map(|fact|fact.positions(path)).collect::<BTreeSet<_>>();
        for(positions,when)in &self.iterations {if when.truth_in(path)==Some(true){result.extend(positions.iter().cloned());}}
        result
    }
}
fn iteration_positions(iteration:&address::Iteration,function:&Function,state:&State,known:&States)->BTreeSet<Position> {
    let states=iteration.states.iter().map(|entry|resolved_initialization(&entry.initialized,&entry.path.assume(&bounds::Clause::default(),true),function,state,known)).collect::<Vec<_>>();
    flow::iteration_positions(iteration,&states)
}
fn initialization_effects(effects:&[address::InitializationEffect],function:&Function,state:&State,known:&States)->InitializedEffects {
    let mut facts=Vec::new();
    let mut iterations=Vec::new();
    for effect in effects {
        match effect {
            address::InitializationEffect::Position(fact)=>facts.push(fact.clone()),
            address::InitializationEffect::Call{call,when}=>{
                let call=&function.calls[*call];
                for fact in &known[&call.function].interface.initialization {
                    let mut fact=fact.at_call(&call.arguments);
                    fact.when=fact.when.logical(when.clone(),false);
                    facts.push(fact);
                }
            }
            address::InitializationEffect::Iteration{iteration,when}=>{
                let positions=iteration_positions(iteration,function,state,known);
                for effect in address::initialization_effects(&positions,&bounds::Clause::default()) {
                    if let address::InitializationEffect::Position(mut fact)=effect {
                        fact.when=fact.when.logical(when.clone(),false);facts.push(fact);
                    }
                }
                iterations.push((positions,when.clone()));
            }
        }
    }
    for fact in &mut facts {fact.source=fact.source.map_borrows(|borrow|evaluate(borrow,function,state,known,true));}
    InitializedEffects{facts:address::merge_initializations(facts),iterations}
}
fn resolved_initialization(initialized:&BTreeSet<Position>,path:&bounds::Clause,function:&Function,state:&State,known:&States)->BTreeSet<Position> {
    let map=|borrow:&Borrow|evaluate(borrow,function,state,known,true);
    let effects=address::initialization_effects(initialized,&bounds::Clause::default());
    let effects=initialization_effects(&effects,function,state,known);
    let mut result=initialized.iter().filter(|position|!matches!(position,Position::Initialization(_)))
        .map(|position|resolved_position(position,path,&map,&|input,index,word|Position::ApplicationInput(input,index,word))).collect::<BTreeSet<_>>();
    result.extend(effects.positions(path));result
}
fn resolved_read(read:&address::InitializationRead,path:&bounds::Clause,function:&Function,state:&State,known:&States)->address::InitializationRead {
    let initialized=resolved_initialization(&read.initialized,path,function,state,known);
    let positions=read.positions.iter().map(|position|{
        let visiting=std::cell::RefCell::new(BTreeSet::new());
        resolved_position(position,path,&|borrow|evaluate(borrow,function,state,known,true),
            &|input,index,word|resolved_application_position(input,index,word,&initialized,path,function,state,known,&mut visiting.borrow_mut()))
    }).collect();
    address::InitializationRead{positions,initialized}
}

fn refreshed_at_call<'a>(borrow:Borrow,path:&bounds::Clause,initialized:&impl Fn()->&'a BTreeSet<Position>)->Borrow {
    borrow.map_sources(|source|match source {
        Origin::Position(origin) if origin.initialized.iter().any(|ready| !ready) => {
            let fact=address::Initialization{source:address::InitializationSource::Position(origin.as_ref().clone()),when:bounds::Condition::constant(true)};
            let positions=fact.positions(path);
            let mut origin=origin.as_ref().clone();
            origin.initialized=origin.initialized.iter().enumerate().map(|(i,ready)|*ready||positions.get(i).is_some_and(|position|position.initialized_by(initialized()))).collect();
            Borrow::source(Origin::Position(Arc::new(origin)))
        }
        source=>Borrow::source(source),
    })
}

fn borrow_at_call(borrow: &Borrow, call: &Call, arguments: &BTreeMap<ValueIdentity, bounds::Proof>) -> Borrow {
    call_content_sources(borrow, call, arguments).at_call(arguments)
}

fn access_at_call<'a>(access:&Access,call:&Call,path:&bounds::Clause,evaluation:&mut BorrowEvaluation<'_>,initialized:&impl Fn()->&'a BTreeSet<Position>)->Access<bounds::Clause>{
    let mut apply=|access:&Access,arguments:&BTreeMap<ValueIdentity,bounds::Proof>|{
    let(Access::Resource{borrows,when,..}|Access::Position{borrows,when,..}|Access::Memory{borrows,when,..}|Access::Dataset{borrows,when,..}|Access::Initialized{borrows,when,..}|Access::Stored{borrows,when,..})=access;
    let borrows=borrow_at_call(borrows, call, arguments);
    let when=bounds::Clause::both(path, &when.projected_case(true, arguments));
    let borrows=evaluation.evaluate(&borrows);
    let borrows=if matches!(access,Access::Initialized{..}){borrows}else{refreshed_at_call(borrows,path,initialized)};
    match access {
        Access::Resource{resources,kind,..}=>Access::Resource{resources:evaluation.evaluate(&borrow_at_call(resources,call,arguments)),borrows,kind:*kind,when},
        Access::Position{position,..}=>Access::Position{borrows,when,position:position.clone()},
        Access::Memory{access,..}=>Access::Memory{borrows,when,access:*access},
        // The value belongs to content reads. Access consumers use only the write location.
        Access::Stored{word,value,..}=>Access::Stored{borrows,when,word:*word,value:value.clone()},
        Access::Initialized{initialized,..}=>Access::Initialized{borrows,when,initialized:initialized.iter().map(|fact|fact.at_call(arguments)).collect()},
        Access::Dataset{index,access,atomic,..}=>Access::Dataset{borrows,when,index:index.and_then(|index|index.at_call(arguments)),access:*access,atomic:*atomic},
    }
    };
    match access{
        Access::Resource{resources,borrows,when,..}=>{
            let mut indices=resource_index_values(resources);indices.extend(resource_index_values(borrows));
            indices.extend(when.scalar_identities());
            let mut caller=bounds::Clause::scalar_identities([path]);
            for value in call.arguments.values(){caller.extend(value.scalar_identities());caller.extend(resource_index_values(&value.borrowed_view()));}
            let mut next=caller.into_iter().filter_map(|id|if let ValueIdentity::ElementIndex(index)=id{Some(index+1)}else{None}).max().unwrap_or(0);
            let mut names=BTreeMap::new();let mut arguments=call.arguments.clone();
            for id in indices{if matches!(id,ValueIdentity::ElementIndex(_)){
                let target=ValueIdentity::ElementIndex(next);next+=1;
                names.insert(id,bounds::Proof::default().with_identity(target));arguments.insert(target,bounds::Proof::default().with_identity(target));
            }}
            apply(&access.substitute_values(&names),&arguments)
        },
        _=>apply(access,&call.arguments),
    }
}

fn output_initialization_read(borrows:&Borrow,initialized:&[address::Initialization],path:&bounds::Clause)->address::InitializationRead {
    let mut positions=Vec::new();
    for(source,when)in borrows.sources.entries(){
        let Origin::Position(origin)=source else {continue;};
        if !bounds::Clause::both(path, &when).possible(){continue;}
        positions.extend(address::borrowed_positions(&Borrow::source(source.clone()),&origin.layout,&origin.layout.parts.iter().map(|(_,word)|*word).collect::<Vec<_>>()));
    }
    let initialized=initialized.iter().filter(|fact|fact.when.truth_in(path)==Some(true)).flat_map(|fact|fact.positions(path)).collect();
    address::InitializationRead{positions,initialized}
}
fn normalized_condition(when:bounds::Condition)->bounds::Condition {
    bounds::Condition::paths(when.assume(&bounds::Clause::default(),true).canonical(),when.assume(&bounds::Clause::default(),false).canonical())
}
fn exposed_resources<S: std::borrow::Borrow<Borrow>>(
    function: &Function,
    mut physical: impl FnMut(&Output) -> S,
) -> BTreeMap<Origin, Borrow> {
    fn local(source:&Origin,function:&Function)->bool{match source{
        Origin::Data(id)=>matches!(id,ValueIdentity::Register(_))&&!function.parameters.get(id).is_some_and(|id|function.input(*id)),
        Origin::Element{parent,index:Some(_),..}|Origin::Projection{parent,..}=>parent.calls.is_empty()&&parent.inputs.is_empty()&&parent.active()&&parent.sources.iter().all(|source|local(source,function)),
        _=>false,
    }}
    let mut exposed:BTreeMap<Origin,Borrow>=BTreeMap::new();
    for (output, name) in &function.resource_names {
        let physical = physical(output);
        for (source, when) in physical.borrow().sources.entries() {
            let named = match source {
                Origin::Data(id) => !matches!(id, ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..)),
                source => local(source, function),
            };
            if named {
                exposed.entry(source.clone()).or_default().union(&name.restricted(&when));
            }
        }
    }
    exposed
}
struct InitializationExitPaths {
    parameters: BTreeMap<ValueIdentity, bounds::Proof>,
    exits: Vec<(bounds::Clause, bounds::Condition)>,
}

impl InitializationExitPaths {
    fn new(function: &Function) -> Self {
        let parameters = public_parameters(function);
        let exits = function.initialization.iter().filter_map(|effect| {
            let when = match effect {
                address::InitializationEffect::Position(fact) => &fact.when,
                address::InitializationEffect::Call { when, .. } | address::InitializationEffect::Iteration { when, .. } => when,
            };
            let path = when.projected_case(true, &parameters);
            path.possible().then(|| (path.clone(), bounds::Condition::guard(path)))
        }).collect();
        Self { parameters, exits }
    }
}

fn initializations(function:&Function,state:&State,known:&States,exit_paths:Option<&InitializationExitPaths>)->Vec<address::Initialization> {
    let exposed=exposed_resources(function, |output| evaluate(&function.outputs[output],function,state,known,true));
    let discovery_parameters = std::cell::LazyCell::new(|| public_parameters(function));
    let parameters = || match exit_paths {
        Some(paths) => &paths.parameters,
        None => &*discovery_parameters,
    };
    // Reading a fixed input before this function's first write already requires those bytes on entry.
    // Re-exporting the same bytes as newly initialized adds no fact for any legitimate caller.
    let mut required=Vec::new();
    for(memory,usage)in function.uses.iter().enumerate(){
        if function.call_at_memory(memory, false).is_some()||matches!(&usage.requirement,Requirement::Memory{access:Some(access),..}if access.writes()){break;}
        if let Requirement::Address{borrow,initialized:true,..}=&usage.requirement && !borrow.pending(){
            for(source,when)in borrow.sources.entries(){
                let value=canonical_relation(Borrow::source(source.clone()), function, None, &exposed, parameters(), BorrowProjection::Input);
                for selected in value.sources.iter(){if let Origin::Position(origin)=selected{required.push((origin.clone(),bounds::Condition::guard(bounds::Clause::both(&usage.path,&when))));}}
            }
        }
    }
    let facts=initialization_effects(&function.initialization,function,state,known).facts;
    let facts=address::merge_initializations(facts.into_iter().filter_map(|mut fact|{
        fact.source=match fact.source {
            address::InitializationSource::Position(mut origin)=>{
                // A guarantee names one actual selected location; possible owners are never separate writes.
                loop {
                    origin=origin.map_borrows(|borrow|address::borrow_in_path(borrow,&fact.when.assume(&bounds::Clause::default(),true)));
                    let value=origin.normalize();
                    if !value.inputs.is_empty()||!value.calls.is_empty()||value.sources.iter().count()!=1{break;}
                    let(source,when)=value.sources.entries().next().unwrap();
                    let Origin::Position(selected)=source else{break;};
                    if bounds::Condition::guard(when).truth_in(&bounds::Clause::default())!=Some(true)||**selected==origin{break;}
                    origin=selected.as_ref().clone();
                }
                if origin.has_local(){return None;}
                origin=origin.map_borrows(|borrow|canonical_relation(borrow.clone(), function, None, &exposed, parameters(), BorrowProjection::Input));
                if let address::Root::Resource{index:Some(index),..}|address::Root::Dataset{index:Some(index),..}=&mut origin.root {
                    *index=index.map(|id|function.parameters.get(&id).copied().filter(|id|function.input(*id)))?;
                }
                let path=fact.when.assume(&bounds::Clause::default(),true);
                if origin.layout.parts.iter().zip(origin.words.iter()).all(|((scalar,_),word)|required.iter().any(|(read,when)|{
                    when.truth_in(&path)==Some(true)&&read.base==origin.base&&same_content_storage(&read.root,&origin.root)
                        &&read.layout.parts.iter().zip(read.words.iter()).any(|((read_scalar,_),read_word)|read_scalar==scalar&&read_word==word)
                })){return None;}
                origin.positions=Arc::from([]);
                address::InitializationSource::Position(origin)
            },
            address::InitializationSource::Task(borrow)=>{
                if borrow.has_local(){return None;}
                address::InitializationSource::Task(canonical_relation(borrow, function, None, &exposed, parameters(), BorrowProjection::Input))
            },
        };
        if exit_paths.is_none(){fact.when=bounds::Condition::constant(true);}
        Some(fact)
    }));
    let Some(exit_paths) = exit_paths else { return facts; };
    facts.into_iter().filter_map(|mut fact|{
        // A normal exit supplies the guarantee when its path excludes failure.
        let projected_failure = function.initialization_when(&fact)
            .projected_case(false, &exit_paths.parameters);
        let failure = bounds::Clause::both(&bounds::Clause::default(), &projected_failure).canonical();
        // Original normal exits form a finite induction domain; recursive scalar substitutions
        // cannot manufacture an unbounded chain of successively expanded path formulas.
        let mut when = bounds::Condition::constant(false);
        if !failure.possible() {
            when = bounds::Condition::constant(true);
        } else {
            for (path, exit) in &exit_paths.exits {
                if !bounds::Clause::both(path, &failure).possible() {
                    when = when.logical(exit.clone(), true);
                }
            }
        }
        fact.when=normalized_condition(when);
        fact.when.active().then_some(fact)
    }).collect()
}
fn converge_initializations(
    worker: &crate::compiler::Worker, functions: &BTreeMap<String, &Function>, known: &mut States,
) -> Result<()> {
    // First grow written locations, then retain only guarantees on normal return.
    for candidates in [true, false] {
        // Original exit paths depend on the declaration and parameter identities.
        let exit_paths: BTreeMap<_, _> = if candidates {
            BTreeMap::new()
        } else {
            functions.iter().filter(|(name, _)| !known[*name].interface.initialization.is_empty())
                .map(|(name, function)| (name, InitializationExitPaths::new(function))).collect()
        };
        loop {
            let tasks = functions.iter()
                .filter(|(name, _)| candidates || !known[*name].interface.initialization.is_empty())
                .collect::<Vec<_>>();
            let changed = crate::parallel::try_map(worker, tasks, |_| (), |_, (name, function)| {
                let state = &known[name];
                let old = &state.interface.initialization;
                let paths = exit_paths.get(name);
                let facts = initializations(function, state, known, paths);
                let facts = if paths.is_some() {
                    let guarantees: BTreeMap<_, _> = facts.into_iter()
                        .map(|fact| (fact.source, fact.when)).collect();
                    old.iter().filter_map(|old| {
                        let when = normalized_condition(old.when.clone().logical(guarantees.get(&old.source)?.clone(), false));
                        when.active().then(|| address::Initialization { source: old.source.clone(), when })
                    }).collect()
                } else {
                    facts
                };
                Ok((facts != state.interface.initialization).then_some((name, facts)))
            }).map_err(|error| borrow_worker_error(functions, error))?;
            let changed = changed.into_iter().flatten().collect::<Vec<_>>();
            if changed.is_empty() {
                break;
            }
            for (name, facts) in changed {
                known.get_mut(name).unwrap().interface.initialization = facts;
            }
        }
    }
    Ok(())
}
fn converge_access(
    worker: &crate::compiler::Worker, functions: &BTreeMap<String, &Function>, known: &mut States,
    owns_input: &(impl Fn(&str, ValueIdentity) -> bool + Sync),
) -> Result<()> {
    fn recursive_groups<'a>(functions: &'a BTreeMap<String, &Function>) -> BTreeMap<&'a str, Arc<[&'a str]>> {
        #[repr(C)]
        struct CallRange { first: usize, end: usize }
        #[repr(C)]
        struct CallGraph { rows: *const CallRange, callees: *const usize, count: usize }
        #[repr(C)]
        #[derive(Clone, Default)]
        struct CallNode { index: usize, low: usize }
        #[repr(C)]
        #[derive(Clone, Default)]
        struct CallVisit { node: usize, next: usize, self_call: u64 }
        #[repr(C)]
        struct CallWork { states: *mut CallNode, visits: *mut CallVisit, active: *mut usize }
        unsafe extern "C" {
            #[link_name = "bootstrap.relation_order::recursive_call_groups"]
            fn resolve_groups(graph: CallGraph, work: CallWork, groups: *mut usize);
        }

        let names = functions.keys().map(String::as_str).collect::<Vec<_>>();
        let mut callees = Vec::new();
        let rows = functions.values().map(|function| {
            let first = callees.len();
            callees.extend(function.calls.iter().filter_map(|call| {
                names.binary_search(&call.function.as_str()).ok()
            }));
            CallRange { first, end: callees.len() }
        }).collect::<Vec<_>>();
        let count = names.len();
        let mut states = vec![CallNode::default(); count];
        let mut visits = vec![CallVisit::default(); count];
        let mut active = vec![0; count];
        let mut groups = vec![0; count];
        unsafe {
            resolve_groups(
                CallGraph { rows: rows.as_ptr(), callees: callees.as_ptr(), count },
                CallWork { states: states.as_mut_ptr(), visits: visits.as_mut_ptr(), active: active.as_mut_ptr() },
                groups.as_mut_ptr(),
            );
        }

        let mut members = BTreeMap::<usize, Vec<&str>>::new();
        for (name, group) in names.into_iter().zip(groups) {
            if group != 0 {
                members.entry(group).or_default().push(name);
            }
        }
        let mut recursive = BTreeMap::new();
        for members in members.into_values() {
            let members: Arc<[_]> = Arc::from(members);
            for &name in members.iter() {
                recursive.insert(name, Arc::clone(&members));
            }
        }
        recursive
    }
    let groups = recursive_groups(functions);
    fn insert(accesses: &mut BTreeMap<Access<()>, bounds::Condition>, access: Access) {
        let (access, when) = access.replace_when(());
        accesses.entry(access).and_modify(|old|{if *old!=when {
            *old=old.clone().logical(when.clone(),true);
        }}).or_insert(when);
    }

    // Borrow sources, stored writes and initialization are fixed before this stage.
    // Only the access requirements propagated through calls change between rounds.
    let direct = crate::parallel::try_map(worker, functions, |_| (), |_, (name, function)| {
        let state = &known[name];
        let parameters = residency::input_parameters(&function.parameters, function.input_width);
        let mut access=BTreeMap::new();
        let mut evaluation = BorrowEvaluation {
            function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
            loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None, stored_paths: Default::default(),
        };
        for value in state.interface.access.iter().filter(|access|matches!(access,Access::Stored{..})) {insert(&mut access,value.clone());}
        for usage in &function.uses {
            if let Requirement::Initialized{read,code,..}=&usage.requirement {
                let read=resolved_read(read,&usage.path,function,state,known);
                if *code=="B229" {
                    if let Some(value)=function.initialization_access(&parameters,&read,&usage.path,&bounds::Clause::project_guard){insert(&mut access,value);}
                    continue;
                }
                for(source,when)in read.borrow().sources.entries() {
                    if let Origin::Position(origin)=source {
                        if let Some(value)=function.address_access(&parameters,Borrow::source(source.clone()),|borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:origin.layout.clone(),written:None,writable:false,initialized:true}},
                            &bounds::Clause::both(&usage.path,&when),&bounds::Clause::project_guard){insert(&mut access,value);}
                    }
                }
                continue;
            }
            let value=match &usage.requirement {
                Requirement::Unborrowed{resource,borrow,kind,..}=>function.access(
                    &parameters,evaluation.evaluate(resource),evaluation.evaluate(borrow),
                    *kind,&usage.path,&bounds::Clause::project_guard),
                Requirement::Address{borrow,layout,written,writable,initialized,..}=>function.address_access(
                    &parameters,evaluation.evaluate(borrow),
                    |borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:layout.clone(),written:written.as_ref().map(|proof|proof.at_call(&parameters)),writable:*writable,initialized:*initialized}},
                    &usage.path,&bounds::Clause::project_guard),
                Requirement::Memory{target:address::MemoryTarget::Call(memory),borrow,access:Some(access),..} if function.call_at_memory(*memory, true).is_none()=>
                    function.address_access(&parameters,evaluation.evaluate(borrow),|borrows,when|Access::Memory{borrows,when,access:*access},&usage.path,&bounds::Clause::project_guard),
                Requirement::Dataset{borrow,index,access,atomic,..}=>function.dataset_access(&parameters,evaluation.evaluate(borrow),*index,*access,*atomic,&usage.path,&|id|owns_input(name,id),&bounds::Clause::project_guard),
                _=>None,
            };
            if let Some(value)=value{insert(&mut access,value);}
        }
        Ok((name.as_str(), (parameters, access)))
    }).map_err(|error| borrow_worker_error(functions, error))?
        .into_iter().collect::<BTreeMap<_, _>>();

    // Sources, stored writes and initialization are fixed; a call's projection
    // changes only with its callee's access contract.
    struct CallProjection<'a> {
        condition: bounds::GuardProjection<'a>,
        // Slots follow the callee's current ordered requirements.
        access: Vec<Option<Vec<Access>>>,
    }

    fn matching_requirements(old: &BTreeSet<Access>, new: &BTreeSet<Access>) -> Vec<Option<usize>> {
        use std::cmp::Ordering;
        let mut old = old.iter().enumerate().peekable();
        new.iter().map(|requirement| {
            while let Some((_, prior)) = old.peek() {
                match (*prior).cmp(requirement) {
                    Ordering::Less => { old.next(); }
                    Ordering::Equal => {
                        let (index, prior) = old.next().unwrap();
                        return (prior == requirement).then_some(index);
                    }
                    Ordering::Greater => break,
                }
            }
            None
        }).collect()
    }

    let components = groups.iter().filter_map(|(&entry, members)| {
        if members.len() < 2 { return None; }
        let inputs = relation::Graph::component_inputs(entry, members, functions, known, |member| &direct[member].0);
        let (parameters, _) = &direct[entry];
        let invoked = functions[entry].calls.iter().filter(|call| members.contains(&call.function.as_str()))
            .map(|call| bounds::Condition::execution(&call.path).guard_at_call(parameters))
            .reduce(|a,b|a.logical(b,true)).unwrap();
        Some((entry, (members, invoked, inputs)))
    }).collect::<BTreeMap<_,_>>();

    let mut call_accesses = functions.keys().map(|name| {
        (name.as_str(), BTreeMap::<usize, CallProjection<'_>>::new())
    }).collect::<BTreeMap<_, _>>();
    let mut ready = call_accesses.keys().copied().collect::<BTreeSet<_>>();
    while !ready.is_empty() {
        let peers = ready.iter().filter_map(|name| groups.get(name)).filter(|group| {
            group.iter().any(|name| components.contains_key(name))
        }).flat_map(|group| group.iter().copied()).collect::<Vec<_>>();
        ready.extend(peers);
        let tasks: Vec<_> = call_accesses.iter_mut().filter(|(name, _)| ready.contains(*name)).collect();
        let bases = crate::parallel::try_map(worker, tasks, Clone::clone, |worker, (&name, projections)| {
            // This function reads one immutable round of call contracts.
            let _required_scope = bounds::required_scope();
            let (name, function) = functions.get_key_value(name).unwrap();
            let state=&known[name];
            let (parameters, local_access)=&direct[name.as_str()];
            let mut access=local_access.clone();
            let component = components.get(name.as_str());
            let evaluation = || BorrowEvaluation {
                function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
                loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None, stored_paths: Default::default(),
            };
            let internal = |call: &Call| {
                component.is_some_and(|(members, _, _)| members.contains(&call.function.as_str()))
            };
            for (index, call) in function.calls.iter().enumerate() {
                if call.function == *name && state.entries.is_some() && component.is_none() {
                    continue;
                }
                if internal(call) && !call.live_task_places.active() {
                    continue;
                }
                let requirements = &known[&call.function].interface.access;
                projections.entry(index).or_insert_with(|| CallProjection {
                    condition: bounds::GuardProjection::new(&call.path, parameters),
                    access: vec![None; requirements.len()],
                });
            }
            let pending = projections.iter_mut()
                .filter_map(|(&index, projection)| projection.access.iter().any(Option::is_none).then_some((index, projection)))
                .collect::<Vec<_>>();
            let project_call = |evaluation: &mut BorrowEvaluation<'_>, (index, projection): (usize, &mut CallProjection<'_>)| {
                let call = &function.calls[index];
                let requirements = &known[&call.function].interface.access;
                let condition = &projection.condition;
                let project = |path:&bounds::Clause,bindings:&BTreeMap<ValueIdentity,bounds::Proof>| condition.project(path,bindings);
                let projected = &mut projection.access;
                let live_task_places=evaluation.evaluate(&call.live_task_places);
                let task_access=live_task_places.active();
                let recursive = internal(call);
                let initialized = std::cell::LazyCell::new(|| initialization_effects(&call.initialization, function, state, known).positions(&call.path));
                for (requirement, previous) in requirements.iter().zip(&mut *projected) {
                    if previous.is_some() {
                        continue;
                    }
                    let projected = previous.insert(Vec::new());
                    if recursive && (!task_access || requirement.accessed().is_none()) {
                        continue;
                    }
                    if !task_access && (matches!(requirement,Access::Stored{..})
                        || matches!(requirement,Access::Position{position,..} if !position.writable && !position.initialized && (call.deferred || position.written.is_none()))) {
                        continue;
                    }
                    let value=access_at_call(requirement,call,&call.path,evaluation,&|| &*initialized);
                    if task_access
                        && let Some((accessed,when,mode))=value.accessed()
                        && let Some(exclusion)=function.access(parameters,accessed.clone(),live_task_places.clone(),view::Exclusion::Position(mode),when,&project)
                    {
                        projected.push(exclusion);
                    }
                    // The entry graph owns ordinary access transfer within this component.
                    // This call contributes only the exclusion of its own live tasks.
                    if recursive {
                        continue;
                    }
                    let value=match value {
                        Access::Position{mut position,borrows,when}=>{
                            if call.deferred { position.written=None; }
                            if position.written.is_none()&&!position.initialized&&!position.writable{continue;}
                            position.written=position.written.as_ref()
                                .map(|proof|proof.at_call(&call.arguments).at_call(parameters));
                            function.address_access(parameters,borrows,|borrows,when|Access::Position{borrows,when,position},&when,&project)
                        },
                        Access::Stored{..}=>continue,
                        Access::Memory{borrows,when,access}=>function.address_access(parameters,borrows,|borrows,when|Access::Memory{borrows,when,access},&when,&project),
                        Access::Initialized{borrows,initialized,when}=>function.initialization_access(
                            parameters,&output_initialization_read(&borrows,&initialized,&when),&when,&project),
                        Access::Resource{resources,borrows,kind,when}=>function.access(parameters,resources,borrows,kind,&when,&project),
                        Access::Dataset{borrows,index,access,atomic,when}=>function.dataset_access(parameters,borrows,index,access,atomic,&when,&|id|owns_input(name,id),&project),
                    };
                    if let Some(value)=value{projected.push(value);}
                }
            };
            crate::parallel::try_map(
                worker, pending,
                |_| (bounds::required_scope(), evaluation()),
                |(_, evaluation), call| {
                    project_call(evaluation, call);
                    Ok::<(), crate::ast::Diagnostic>(())
                },
            ).map_err(|error| borrow_worker_error(functions, error))?;
            for projection in projections.values() {
                for value in projection.access.iter().flatten().flatten() {
                    insert(&mut access, value.clone());
                }
            }
            if let Some(entries)=&state.entries && component.is_none(){
                let invoked=function.calls.iter().filter(|call|call.function==*name).map(|call|bounds::Condition::execution(&call.path).guard_at_call(parameters)).reduce(|a,b|a.logical(b,true)).unwrap();
                let inputs=entries.iter().map(|(input,value)|(*input,{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,value.clone());component_proof})).collect::<BTreeMap<_,_>>();
                let direct = access.iter()
                    .map(|(value, when)| value.clone().replace_when(when.clone()).0)
                    .collect::<Vec<_>>();
                for value in direct.into_iter().filter_map(|value| value.at_recursive_entry(&inputs, &invoked)) {
                    insert(&mut access, value);
                }
            }
            Ok((name, access))
        }).map_err(|error| borrow_worker_error(functions, error))?;
        let changed = bases.into_iter().filter_map(|(name, mut access)| {
            if let Some((_, invoked, inputs)) = components.get(name.as_str()) {
                for (member, bindings) in inputs {
                    let local = direct[member].1.iter()
                        .map(|(value, when)| value.clone().replace_when(when.clone()).0);
                    let projected = call_accesses[member].values().flat_map(|projection| {
                        projection.access.iter().flatten().flatten().cloned()
                    });
                    for value in local.chain(projected).filter_map(|value| value.at_recursive_entry(bindings, invoked)) {
                        if value.active_entry_access() { insert(&mut access, value); }
                    }
                }
            }
            let normalize = groups.contains_key(name.as_str());
            let next = access.into_iter().map(|(value, when)| {
                let when = if normalize { normalized_condition(when) } else { when };
                value.replace_when(when).0
            }).collect::<BTreeSet<_>>();
            (next != known[name].interface.access).then_some((name, next))
        }).collect::<Vec<_>>();
        if changed.is_empty() {
            break;
        }
        // Match each changed callee once while both contracts are available.
        // Move projections of unchanged requirements to their new slots.
        let matching = changed.iter().map(|(name, next)| {
            matching_requirements(&known[*name].interface.access, next)
        }).collect::<Vec<_>>();
        ready.clear();
        for (&caller, projections) in &mut call_accesses {
            for (&index, projection) in projections {
                let callee = functions[caller].calls[index].function.as_str();
                if let Ok(change) = changed.binary_search_by_key(&callee, |(name, _)| name.as_str()) {
                    let mut prior = std::mem::take(&mut projection.access);
                    projection.access = matching[change].iter().map(|previous| {
                        previous.and_then(|index| prior[index].take())
                    }).collect();
                    ready.insert(caller);
                }
            }
        }
        for (name, access) in changed {
            known.get_mut(name).unwrap().interface.access = access;
        }
    }
    Ok(())
}

fn stored_outputs<'p>(
    worker: &crate::compiler::Worker,
    evaluation: &mut BorrowEvaluation<'_>, exposed: &BTreeMap<Origin, Borrow>,
    parameters: impl Fn() -> &'p BTreeMap<ValueIdentity, bounds::Proof>,
) -> std::result::Result<BTreeSet<Access>, crate::parallel::Error<crate::ast::Diagnostic>> {
    let BorrowEvaluation { function, state, known, .. } = *evaluation;
    let exits: Vec<_> = function.uses.iter().filter_map(|usage| match &usage.requirement {
        Requirement::Return { borrow, .. } => Some(&borrow.reads),
        _ => None,
    }).flat_map(|reads| reads.iter()).filter_map(|read| match read {
        ContentRead::Returned { memory } => Some(*memory),
        _ => None,
    }).collect();
    if exits.is_empty() {
        return Ok(BTreeSet::new());
    }

    let locations = evaluation.stored_locations(&parameters);
    let mut stored = BTreeMap::<(Borrow, u32), Vec<_>>::new();
    for (origin, word) in locations.iter() {
        let target = stored_target(origin, function, exposed, &parameters);
        stored.entry((target, *word)).or_default().push(Arc::clone(origin));
    }
    if stored.is_empty() { return Ok(BTreeSet::new()); }

    let bindings = parameters();
    let store = |evaluation: &mut BorrowEvaluation<'_>, guards: &bounds::PublicGuards<'_>, ((borrows, word), origins): ((Borrow, u32), Vec<Arc<address::Origin>>)| {
        let mut value = Borrow::default();
        for &memory in &exits {
            for origin in &origins {
                let incoming = evaluation.content_at_exit(origin, word, memory);
                let incoming = canonical_with_guards(incoming, function, None, exposed, guards, BorrowProjection::Input);
                value.union(&incoming);
            }
        }
        Access::Stored { borrows, word, value, when: bounds::Condition::constant(true) }
    };
    if stored.len() == 1 {
        let guards = bounds::PublicGuards::stored(bindings);
        return Ok(stored.into_iter().map(|entry| store(evaluation, &guards, entry)).collect());
    }

    // Each location owns its result and the original exit/write order.
    // The write guards have the same bindings in every worker and already have
    // an owner in this pass. Mutable reads and loop values stay worker-local.
    let paths = evaluation.stored_paths.paths();
    let results = crate::parallel::try_map(worker, stored, |_| {
        (BorrowEvaluation {
            function, state, known, expand_loops: true, deferred_reads: false,
            loop_bindings: None, values: BTreeMap::new(),
            stored: Some(locations.clone()), exit_writes: None, stored_paths: StoredPaths::Borrowed(paths),
        }, bounds::PublicGuards::stored(bindings))
    }, |(evaluation, guards), entry| Ok(store(evaluation, guards, entry)))?;
    Ok(results.into_iter().collect())
}

fn borrow_worker_error(
    functions: &BTreeMap<String, &Function>, error: crate::parallel::Error<crate::ast::Diagnostic>,
) -> crate::ast::Diagnostic {
    match error {
        crate::parallel::Error::Work(error) => error,
        crate::parallel::Error::Resource(error) => {
            let path = functions.values().find_map(|function| function.uses.first()
                .map(|usage| usage.source.path.as_path()))
                .unwrap_or_else(|| std::path::Path::new("<borrow-solver>"));
            crate::compiler::diagnostic(path, error)
        }
    }
}

struct ConvergenceUpdate<'a> {
    name: &'a String,
    loops: Option<Arc<relation::Graph>>,
    entries: Option<BTreeMap<ValueIdentity, Borrow>>,
    outputs: Outputs,
    stored: BTreeSet<Access>,
    interface_changed: bool,
}

fn converge(
    worker: &crate::compiler::Worker,
    functions: &BTreeMap<String, &Function>,
    known: &mut States,
    loop_inputs: &BTreeMap<String, BTreeSet<LoopInput>>,
    unknown: &BTreeSet<(String, Output)>,
) -> Result<()> {
    let mut ready: BTreeSet<_> = functions.keys().collect();
    while !ready.is_empty() {
        let changed = crate::parallel::try_map(worker, ready, Clone::clone, |worker, name| {
            let function = functions[name];
            let state = &known[name];
            let loops = relation::Graph::loops(&loop_inputs[name], function, state.name, known);
            let scope = LoopScope { name: state.name, loops: loops.as_ref() };
            let recursive = relation::Graph::recursive_inputs(name, function, scope, known);
            let entry_bindings = recursive.as_ref().and_then(|inputs| inputs.forwarding.as_ref());
            let mut evaluation=BorrowEvaluation{function,state:scope,known,expand_loops:true,deferred_reads:false,loop_bindings:None,values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()};
            let physical=function.outputs.iter().map(|(output,definition)|{
                let basis=match entry_bindings{
                    Some(bindings)if function.forwards_recursive_output(name,output,definition)=>{
                        let mut basis=definition.clone();let calls=basis.calls.iter().filter(|result|function.calls[result.call].function==*name&&result.output==*output).cloned().collect::<Vec<_>>();
                        for call in calls{basis.calls.remove(&call);}
                        std::borrow::Cow::Owned(basis.substitute_values(bindings))
                    },
                    _=>std::borrow::Cow::Borrowed(definition),
                };
                let value=evaluation.evaluate(&basis);
                let value=if function.resource_names.contains_key(output){value.resources()}else{value};
                (output.clone(),value)
            }).collect::<Outputs>();
            // 每个真实资源输出命名自己的交付存储；view保留所有可能的实际输出所有者。
            let exposed=exposed_resources(function, |output| &physical[output]);
            let parameters = std::cell::LazyCell::new(|| public_parameters(function));
            let outputs = physical.into_iter().map(|(output, definition)| {
                let projection = match output {
                    Output::Returned(_) | Output::ReturnedResource(_) => BorrowProjection::Result,
                    _ => BorrowProjection::Input,
                };
                let mut value = canonical_relation(definition, function, function.resource_names.get(&output), &exposed, &parameters, projection);
                if unknown.contains(&(name.clone(),output.clone())){value.sources.insert(Origin::Unresolved);}
                (output, value)
            }).collect::<Outputs>();
            let access = stored_outputs(worker, &mut evaluation, &exposed, || &parameters)
                .map_err(|error| borrow_worker_error(functions, error))?;
            // 来源条件按语义收敛；等价路径的重新组合不是新的调用合同。
            let interface_changed = access != state.interface.access || outputs.iter().any(|(output, value)| {
                let old = &state.interface.outputs[output];
                value.inputs != old.inputs || value.calls != old.calls || value.reads != old.reads
                    || !value.sources.same_sources(&old.sources)
                    || !value.sources.equivalent_paths(&old.sources)
            });
            let entries = recursive.map(|inputs| inputs.entries);
            Ok((interface_changed || loops != state.loops || entries != state.entries)
                .then_some(ConvergenceUpdate { name, loops, entries, outputs, stored: access, interface_changed }))

        }).map_err(|error| borrow_worker_error(functions, error))?;
        let changed = changed.into_iter().flatten().collect::<Vec<_>>();
        let interfaces: BTreeSet<_> = changed.iter().filter_map(|update| {
            update.interface_changed.then_some(update.name.as_str())
        }).collect();
        // The originating function already consumed its current graph and entries.
        // A new interface reaches the callers that read it in the preceding round.
        ready = BTreeSet::new();
        if !interfaces.is_empty() {
            ready.extend(functions.iter().filter_map(|(name, function)| {
                function.calls.iter().any(|call| interfaces.contains(call.function.as_str())).then_some(name)
            }));
        }
        for update in changed {
            let state = known.get_mut(update.name).unwrap();
            state.loops = update.loops;
            state.entries = update.entries;
            state.interface.outputs = update.outputs;
            state.interface.access = update.stored;
        }
    }
    Ok(())
}

struct LoopConditions {
    carried: Vec<LoopInput>,
    terms: Vec<bounds::Condition>,
}
impl LoopConditions {
    fn include(&mut self, source: &bounds::Proof, value: &bounds::Proof) {
        if value.component_condition(0).is_some()
            && let Some(input) = source.carried_loop_condition()
        {
            self.carried.push(input);
        } else {
            self.terms.push(value.condition_for_join(0));
        }
    }

    fn collect(
        &mut self,
        id: ValueIdentity,
        definition: &LoopDefinition,
        source: &bounds::Proof,
        mut proof: bounds::Proof,
        values: &BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> (bounds::Proof, bool) {
        let mut explicit = proof.component_condition(0).is_some();
        if explicit {
            self.include(source, &proof);
        }
        if values.contains_key(&id) {
            for state in &definition.states[1..] {
                let source = definition.control.binding(id, state).unwrap();
                let incoming = source.substitute_values(values);
                let conditioned = incoming.component_condition(0).is_some();
                if explicit || conditioned {
                    if !explicit {
                        self.terms.push(proof.condition_for_join(0));
                    }
                    self.include(&source, &incoming);
                    explicit = true;
                }
                proof.join_fields(&incoming, 1, None);
            }
        }
        (proof, explicit)
    }
}

fn solve_loop_value(
    function: &Function,
    input: LoopInput,
    id: ValueIdentity,
    definition: &LoopDefinition,
    values: &BTreeMap<ValueIdentity, bounds::Proof>,
) -> bounds::Proof {
    let source = definition.control.binding(id, &definition.states[0]).unwrap();
    let proof = source.substitute_values(values);
    if definition.states.len() == 1 || !values.contains_key(&id) {
        return proof;
    }

    let mut conditions = LoopConditions { carried: Vec::new(), terms: Vec::new() };
    let (mut proof, _) = conditions.collect(id, definition, &source, proof, values);
    // Close the original carry edges before building the two disjunctions.
    let mut visited = BTreeSet::from([input]);
    while let Some(input) = conditions.carried.pop() {
        if !visited.insert(input) {
            continue;
        }
        let id = ValueIdentity::LoopValue(input.level, input.index);
        let definition = &function.loops[&input];
        let source = definition.control.binding(id, &definition.states[0]).unwrap();
        let value = source.substitute_values(values);
        let (value, explicit) = conditions.collect(id, definition, &source, value, values);
        if !explicit {
            conditions.terms.push(value.condition_for_join(0));
        }
    }
    // A memory snapshot can carry the same loop value back with an extra
    // condition. Close that logical union here instead of unfolding its syntax
    // again on every value round.
    let condition = conditions.terms.into_iter().reduce(bounds::Condition::common)
        .map(|condition| bounds::Normalizer::new().condition(&condition));
    proof.set_loop_condition(condition);
    proof
}

type MemoryEffects = BTreeMap<address::MemoryTarget, address::MemoryAccesses>;
type ScalarValues = BTreeMap<ValueIdentity, bounds::Proof>;

pub(super) struct SolvedFunction {
    pub interface: Interface,
    pub memory: MemoryEffects,
    pub values: Option<ScalarValues>,
}

pub(super) fn solve_views(
    worker: &crate::compiler::Worker,
    functions: &BTreeMap<String, &Function>, closed: BTreeMap<String, Interface>,
    owns_input: impl Fn(&str, ValueIdentity) -> bool + Sync, observe: Option<&StorageObserver<'_>>,
) -> Result<Vec<(String, SolvedFunction)>> {
    let Solved { mut states, memory, mut values } = solve(worker, functions, closed, owns_input, observe)?;
    Ok(memory.into_iter().map(|(name, memory)| {
        let mut interface = states.remove(&name).unwrap().interface;
        let mut values = values.remove(&name);
        if let Some(values) = values.as_mut() {
            let parameters = &functions[&name].parameters;
            for proof in values.values_mut() {
                *proof = proof.output_bound(parameters);
            }
            for borrow in interface.outputs.values_mut() {
                *borrow = borrow.substitute_values(values);
            }
            interface.access = interface.access.iter().map(|access| access.substitute_values(values)).collect();
            for initialization in &mut interface.initialization {
                *initialization = initialization.substitute_values(values);
            }
        }
        (name, SolvedFunction { interface, memory, values })
    }).collect())
}

pub(super) fn solve_range_memory(
    worker: &crate::compiler::Worker,
    functions: &BTreeMap<String, &Function>, closed: BTreeMap<String, Interface>,
) -> Result<BTreeMap<String, MemoryEffects>> {
    Ok(solve(worker, functions, closed, |_, _| false, None)?.memory)
}

struct Solved<'a> {
    states: BTreeMap<String, State<'a>>,
    memory: BTreeMap<String, MemoryEffects>,
    values: BTreeMap<String, ScalarValues>,
}

enum MemoryConsumers<'a> {
    Snapshot,
    Ir(&'a mut address::MemoryAccesses),
    SnapshotAndIr(&'a mut address::MemoryAccesses),
}

impl MemoryConsumers<'_> {
    fn from_sources<'a, 'b>(
        &mut self,
        sources: impl Iterator<Item = (&'a view::Source, bounds::Clause)>,
        path: impl FnOnce() -> bounds::Clause,
        access: super::Access,
        written: impl FnOnce() -> Option<&'b bounds::Proof>,
    ) -> address::MemoryEffect {
        if !matches!(self, Self::Ir(_)) && access.writes() {
            let effect = address::MemoryEffect::from_sources(sources, path, access, written);
            if let Self::SnapshotAndIr(aliases) = self { aliases.record(&effect); }
            effect
        } else {
            if let Self::Ir(aliases) | Self::SnapshotAndIr(aliases) = self {
                aliases.extend_sources(sources, path, access);
            }
            address::MemoryEffect::default()
        }
    }
}

fn call_memory_effect(
    evaluation: &mut BorrowEvaluation<'_>,
    call: &Call,
    resolved: &ScalarValues,
    consumer: &mut MemoryConsumers<'_>,
) -> address::MemoryEffect {
    let mut effect = address::MemoryEffect::default();
    let accesses = &evaluation.known[&call.function].interface.access;
    if matches!(consumer, MemoryConsumers::Snapshot) {
        // A possible opaque write makes every numeric snapshot unknown. Consume
        // its declaration before producing positions that would be discarded.
        for (borrows, when, access, _) in accesses.iter().filter_map(Access::memory_effect) {
            if !access.writes() { continue; }
            let Some((_, selected)) = borrows.sources.entries().find(|(source, _)| matches!(source, Origin::External)) else { continue; };
            let borrow = Borrow::source(Origin::External).restricted(&selected);
            let borrow = borrow_at_call(&borrow, call, &call.arguments);
            let borrow = evaluation.evaluate(&borrow).substitute_values(resolved);
            let values = address::MemoryEffect::from_sources(borrow.sources.entries(), || {
                bounds::Clause::both(&call.path, &when.projected_case(true, &call.arguments))
                    .substitute_values(resolved)
            }, access, || None);
            if values.has_external_write() { return values; }
        }
    }
    for (borrows, when, access, written) in accesses.iter().filter_map(Access::memory_effect) {
        if matches!(consumer, MemoryConsumers::Snapshot) && !access.writes() {
            continue;
        }
        let mut remaining;
        let borrows = if matches!(consumer, MemoryConsumers::Snapshot)
            && borrows.sources.iter().any(|source| matches!(source, Origin::External))
        {
            // The explicit source was infeasible. Other sources and deferred
            // inputs can still resolve to an external write and must be read.
            remaining = borrows.clone();
            remaining.sources.remove(&Origin::External);
            &remaining
        } else { borrows };
        let borrows = borrow_at_call(borrows, call, &call.arguments);
        let borrows = evaluation.evaluate(&borrows);
        let borrow = borrows.substitute_values(resolved);
        let path = || {
            bounds::Clause::both(&call.path, &when.projected_case(true, &call.arguments))
                .substitute_values(resolved)
        };
        let written = std::cell::LazyCell::new(|| {
            written.map(|proof| proof.at_call(&call.arguments).substitute_values(resolved))
        });
        let values = consumer.from_sources(
            borrow.sources.entries(), path, access, || written.as_ref(),
        );
        effect.union(values);
        if matches!(consumer, MemoryConsumers::Snapshot) && effect.has_external_write() {
            break;
        }
    }
    effect
}

fn memory_use_effect(
    evaluation: &mut BorrowEvaluation<'_>, usages: &[Use], resolved: &ScalarValues,
    consumer: &mut MemoryConsumers<'_>, first_path: impl Fn() -> bounds::Clause,
) -> address::MemoryEffect {
    let mut effect = address::MemoryEffect::default();
    for (index, usage) in usages.iter().enumerate() {
        if let Requirement::Memory { borrow, access: Some(access), written, .. } = &usage.requirement {
            if matches!(consumer, MemoryConsumers::Snapshot) && !access.writes() { continue; }
            let borrow = evaluation.evaluate(borrow).substitute_values(resolved);
            let path = || if index == 0 { first_path() } else { usage.path.substitute_values(resolved) };
            let written = std::cell::LazyCell::new(|| {
                written.as_ref().map(|proof| proof.substitute_values(resolved))
            });
            effect.union(consumer.from_sources(
                borrow.sources.entries(), path, *access, || written.as_ref(),
            ));
        }
    }
    effect
}

fn solve<'a>(
    worker: &crate::compiler::Worker,
    functions: &'a BTreeMap<String, &Function>, closed: BTreeMap<String, Interface>,
    owns_input: impl Fn(&str, ValueIdentity) -> bool + Sync, observe: Option<&'a StorageObserver<'a>>,
) -> Result<Solved<'a>> {
    let loop_inputs = used_loops(functions, &closed);
    let mut known = States {
        functions: closed.into_iter().map(|(name,interface)|(name,State{interface,..State::default()})).collect(),
        observe,
    };
    for(name,function)in functions{known.insert(name.clone(),State{
        name,
        loops: None,
        entries:None,
        interface:Interface{outputs:function.outputs.keys().map(|output|(output.clone(),Borrow::default())).collect(),access:BTreeSet::new(),initialization:Vec::new()},
    });}
    converge(worker, functions, &mut known, &loop_inputs, &BTreeSet::new())?;
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output|known[name].interface.outputs[output].empty().then_some((name.clone(),output.clone())))).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(worker, functions, &mut known, &loop_inputs, &unknown)?;}
    converge_initializations(worker, functions, &mut known)?;
    converge_access(worker, functions, &mut known, &owns_input)?;
    let results = crate::parallel::try_map(worker, functions, Clone::clone,
        |worker, (name, function)| finish_function(worker, name, function, &known),
    ).map_err(|error| borrow_worker_error(functions, error))?;
    let mut memory = BTreeMap::new();
    let mut scalar_values = BTreeMap::new();
    for (name, (effects, values)) in functions.keys().zip(results) {
        memory.insert(name.clone(), effects);
        if let Some(values) = values { scalar_values.insert(name.clone(), values); }
    }
    Ok(Solved { states: known.functions, memory, values: scalar_values })
}

struct ExitWrite {
    borrow: Borrow,
    when: bounds::Condition,
    complete: bool,
}

struct ExitEvent {
    at: usize,
    writes: Vec<ExitWrite>,
}

struct ExitContents {
    origin: Arc<address::Origin>,
    word: u32,
    events: Vec<ExitEvent>,
}

type ReturnedWrites = Vec<ExitContents>;

fn prepare_returned_writes(
    worker: &crate::compiler::Worker, function: &Function, state: &State, known: &States<'_>,
    paths: &CallWritePaths,
) -> Result<Option<ReturnedWrites>> {
    let mut latest = None;
    let mut exits = 0;
    for usage in &function.uses {
        let Requirement::Return { borrow, .. } = &usage.requirement else { continue; };
        for read in borrow.reads.iter() {
            if let ContentRead::Returned { memory } = read {
                latest = Some(latest.map_or(*memory, |at: usize| at.max(*memory)));
                exits += 1;
            }
        }
    }
    let Some(latest) = latest.filter(|_| exits > 1) else { return Ok(None); };
    let evaluation = || BorrowEvaluation {
        function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
        loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None,
        stored_paths: StoredPaths::Borrowed(paths),
    };
    let mut source = evaluation();
    let parameters = std::cell::LazyCell::new(|| public_parameters(function));
    let locations = source.stored_locations(|| &parameters);
    let writes = crate::parallel::try_map(worker, locations.iter(), |_| evaluation(),
        |evaluation, (origin, word)| {
            let position = content_position(origin, *word);
            let mut events = Vec::new();
            function.visit_content_history(latest, |at, call| {
                let mut writes = Vec::new();
                evaluation.content_event(Some((origin, *word)), position.as_ref(), at, || call,
                    |borrow, when, complete| writes.push(ExitWrite { borrow: borrow.clone(), when: when.clone(), complete }));
                if !writes.is_empty() {
                    events.push(ExitEvent { at, writes });
                }
            });
            Ok::<_, crate::ast::Diagnostic>(ExitContents { origin: origin.clone(), word: *word, events })
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => crate::compiler::diagnostic(&function.uses[latest].source.path, error),
        })?;
    Ok(Some(writes))
}

fn finish_function(
    worker: &crate::compiler::Worker, name: &str, function: &Function, known: &States<'_>,
) -> Result<(MemoryEffects, Option<ScalarValues>)> {
    let _required_scope = bounds::required_scope();
    let state = &known[name];
    let mut evaluation = BorrowEvaluation {
        function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
        loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None, stored_paths: Default::default(),
    };
    let mut effects = MemoryEffects::new();
    let mut resolved = BTreeMap::new();
    let has_feedback = function.loops.values().any(|definition| definition.value.is_some());
    loop {
        // Each value identity is written once in the original use/backedge order.
        let mut values_changed = false;
        for (memory, call, usages) in memory_events(function) {
            let usage = &usages[0];
            if let Requirement::Dataset{values:Some(read),..}=&usage.requirement {
                let mut proof=read[0].proof.substitute_values(&resolved).at_call(&read[0].bindings);
                for value in &read[1..] {
                    let value=value.proof.substitute_values(&resolved).at_call(&value.bindings);
                    bounds::Proof::join(&mut proof,&value,1,None);
                }
                let identity=ValueIdentity::MemoryValue(memory,0);
                if has_feedback {values_changed=values_changed||resolved.get(&identity)!=Some(&proof);}
                resolved.insert(identity,proof);
                continue;
            }
            let Requirement::Memory{target,snapshots,..}=&usage.requirement else{continue;};
            let snapshots = snapshots.as_ref().filter(|values| !values.is_empty());
            let mut consumer = match (has_feedback, snapshots.is_some()) {
                (true, false) => continue,
                (true, true) => MemoryConsumers::Snapshot,
                (false, true) => MemoryConsumers::SnapshotAndIr(effects.entry(*target).or_default()),
                (false, false) => MemoryConsumers::Ir(effects.entry(*target).or_default()),
            };
            let (effect, scope) = {
                let path = std::cell::LazyCell::new(|| usage.path.substitute_values(&resolved));
                let effect = if let Some(call) = call {
                    call_memory_effect(&mut evaluation, call, &resolved, &mut consumer)
                } else {
                    memory_use_effect(&mut evaluation, usages, &resolved, &mut consumer, || (*path).clone())
                };
                let scope = (snapshots.is_some() && !effect.has_external_write())
                    .then(|| bounds::proof_scope(&path, &BTreeMap::new()));
                (effect, scope)
            };
            if let Some(snapshots) = snapshots {
                for (component, snapshot) in snapshots.iter().enumerate() {
                    let proof = if let Some(scope) = &scope {
                        let position = match &snapshot.position {
                            Position::Borrowed(borrow, layout, word) => Position::Borrowed(
                                Arc::new(evaluation.evaluate(borrow).substitute_values(&resolved)),
                                layout.clone(),
                                *word,
                            ),
                            position => position.clone(),
                        };
                        let index = snapshot.index.and_then(|index| match index {
                            bounds::Operand::Value(id) if resolved.contains_key(&id) => index.at_call(&resolved),
                            index => Some(index),
                        });
                        let proof = snapshot.proof.substitute_values(&resolved);
                        effect.written_value(&position, snapshot.scalar, index,
                            &scope(&Ty::scalar(snapshot.scalar), proof))
                    } else {
                        bounds::Proof::default()
                    };
                    let identity = ValueIdentity::MemoryValue(memory, component);
                    let proof = proof.with_identity(identity);
                    if has_feedback {values_changed=values_changed||resolved.get(&identity)!=Some(&proof);}
                    resolved.insert(identity, proof);
                }
            }
        }
        for (input, definition) in &function.loops {
            let Some((id, _)) = definition.value else { continue; };
            let proof = solve_loop_value(function, *input, id, definition, &resolved);
            values_changed=values_changed||resolved.get(&id)!=Some(&proof);
            resolved.insert(id, proof);
        }
        if !has_feedback||!values_changed{break;}
    }
    if has_feedback {
        // Every snapshot identity is stable and written only by its own event.
        // Both calls and direct uses hand IR aliases to their one final receiver.
        for (_, call, usages) in memory_events(function) {
            let Requirement::Memory { target, .. } = &usages[0].requirement else { continue; };
            let mut consumer = MemoryConsumers::Ir(effects.entry(*target).or_default());
            if let Some(call) = call {
                call_memory_effect(&mut evaluation, call, &resolved, &mut consumer);
            } else {
                memory_use_effect(&mut evaluation, usages, &resolved, &mut consumer,
                    || usages[0].path.substitute_values(&resolved));
            }
        }
    }
    let resolved = function.pending_values().then_some(resolved);
    let context = function.value_context(resolved.as_ref());
    let paths = CallWritePaths::default();
    let symbolic_paths = CallWritePaths::default();
    let history = prepare_returned_writes(worker, function, state, known, &paths)?;
    crate::parallel::try_map(
        worker,
        &function.uses,
        |_| {
            let evaluation = BorrowEvaluation {
                function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
                loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: history.as_ref(),
                stored_paths: StoredPaths::Borrowed(&paths),
            };
            let symbolic = BorrowEvaluation {
                function, state: state.loop_scope(), known, expand_loops: false, deferred_reads: false,
                loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None,
                stored_paths: StoredPaths::Borrowed(&symbolic_paths),
            };
            (bounds::required_scope(), evaluation, symbolic)
        },
        |(_, evaluation, symbolic), usage| {
            let path=bounds::Clause::both(&usage.path,&context);
            if let Requirement::Nonzero{value,path,span}=&usage.requirement {
                let values = resolved.as_ref().unwrap();
                let mut value=(**value).clone();
                let path=bounds::Clause::both(path,&context);
                value.proof=bounds::scoped_proof(&value.ty,value.proof.substitute_values(values),&path,&BTreeMap::new());
                value.constant=value.constant.or_else(||value.proof.known_value(&value.ty));
                Requirement::Nonzero{value:Box::new(value),path,span:*span}.check(&usage.source, None)?;
                return Ok(());
            }
            if let Requirement::Initialized{read,code,span,..}=&usage.requirement {
                let read=resolved_read(read,&path,function,state,known);
                Requirement::Initialized{borrow:read.borrow(),read,code,span:*span}.check(&usage.source, None)?;
                return Ok(());
            }
            // The memory and dataset records have already supplied the effects.
            // Here the only consumer is the unresolved-source diagnostic.
            if let Requirement::Memory { borrow, span, .. } | Requirement::Dataset { borrow, span, .. } = &usage.requirement {
                if evaluation.evaluate(borrow).unresolved_under(&path) {
                    return Requirement::unresolved_diagnostic(&usage.source, *span);
                }
                return Ok(());
            }
            if let Requirement::Unborrowed { resource, borrow, .. } = &usage.requirement {
                let resource = symbolic.evaluate(resource);
                let borrow = symbolic.evaluate(borrow);
                if disjoint(&resource, &borrow, &function.independent, &usage.active_loops) {
                    return Ok(());
                }
            }
            usage.requirement.mapped(|borrow| evaluation.evaluate(borrow))
                .under_path(&path).check(&usage.source, None)
        },
    ).map_err(|error| match error {
        crate::parallel::Error::Work(error) => error,
        crate::parallel::Error::Resource(error) =>
            crate::compiler::diagnostic(&function.uses[0].source.path, error),
    })?;
    let mut evaluation = BorrowEvaluation {
        function, state: state.loop_scope(), known, expand_loops: true, deferred_reads: false,
        loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None,
        stored_paths: StoredPaths::Borrowed(&paths),
    };
    for call in &function.calls{
        let path=bounds::Clause::both(&call.path,&context);
        let live_task_places=evaluation.evaluate(&call.live_task_places);
        let initialized = std::cell::LazyCell::new(|| initialization_effects(&call.initialization, function, state, known).positions(&path));
        for requirement in &known[&call.function].interface.access{
            if live_task_places.empty() && matches!(requirement,Access::Memory{..}|Access::Dataset{..}|Access::Stored{..}) {
                continue;
            }
            let value=access_at_call(requirement,call,&path,&mut evaluation,&|| &*initialized);
            let(Access::Resource{when,..}|Access::Position{when,..}|Access::Memory{when,..}|Access::Dataset{when,..}|Access::Initialized{when,..}|Access::Stored{when,..})=&value;
            let has_live_task_places = !live_task_places.empty();
            // An ordinary address call only needs reachability when it has a diagnostic.
            let path = when.clone();
            let diagnostic_path = match &value {
                Access::Position { .. } if !has_live_task_places => Some(&path),
                _ if !when.possible() => continue,
                _ => None,
            };
            if has_live_task_places && let Some((accessed,_,mode))=value.accessed() {
                Requirement::Unborrowed{
                    resource:accessed.restricted(&path),borrow:live_task_places.restricted(&path),
                    kind:view::Exclusion::Position(mode),span:call.span,
                }.check(&call.source, None)?;
            }
            let requirement=match value {
                Access::Position{borrows,position,..}=>Requirement::Address{borrow:borrows.restricted(&path),layout:position.layout,written:None,writable:position.writable,initialized:position.initialized,span:call.span},
                Access::Resource{resources,borrows,kind,..}=>Requirement::Unborrowed{resource:resources.restricted(&path),borrow:borrows.restricted(&path),kind,span:call.span},
                Access::Memory{..}|Access::Dataset{..}|Access::Stored{..}=>continue,
                Access::Initialized{borrows,initialized,..}=>{
                    let read=output_initialization_read(&borrows,&initialized,&path);
                    Requirement::Initialized{borrow:borrows,read,code:"B229",span:call.span}
                },
            };
            requirement.check(&call.source, diagnostic_path)?;
        }
        for output in &call.outputs{
            let borrow=known[&call.function].interface.outputs.get(output);
            if matches!(output,Output::Returned(_)|Output::Written(..))&&borrow.is_none_or(|borrow|borrow.empty()||borrow.sources.contains(&Origin::Unresolved)){
                return call.source.fail(call.span,"B271","view返回或输出没有可闭合的实际借用来源");
            }
        }
    }
    Ok((effects, resolved))
}

impl Builder<'_> {
    pub(super) fn dataset_effect(&mut self,place:&Place,access:super::Access,atomic:bool,span:Span)->Result<Vec<(usize,usize)>> {
        let mut reads=Vec::new();
        let Place::ApplicationElement{sources,element_layout,index_value,..}=place.storage() else {return Ok(reads)};
        let borrow=self.data_borrow(sources);
        for (component,((scalar,_),position)) in place.ty().components.iter().zip(place.data_positions()).enumerate() {
            let mut pending=vec![(position,bounds::Clause::default())];
            while let Some((position,path))=pending.pop() {
                match position {
                    Position::Borrowed(_,_,word)=>{
                        let origin=address::Origin{
                            root:address::Root::Derived(borrow.clone()),layout:address::Layout::of(&Ty::scalar(*scalar)),
                            base:element_layout.clone(),words:Arc::from([word]),positions:Arc::from([]),
                            writable:access.writes(),initialized:Arc::from([access.reads()]),
                        };
                        let usage=self.view_model.uses.len();
                        self.check_view_requirement(Requirement::Dataset{borrow:Borrow::source(Origin::Position(Arc::new(origin))).restricted(&path),index:Some(*index_value),access,atomic,values:None,span})?;
                        if access==super::Access::Read&&!atomic {reads.push((component,usage));}
                    }
                    Position::Indexed(choices,index)=>pending.extend(choices.iter().enumerate().map(|(i,position)|(position.clone(),bounds::Clause::both(&path,&bounds::Clause::selection(index,i))))),
                    _=>{},
                }
            }
        }
        Ok(reads)
    }

    pub(super) fn native_dataset_access(&mut self,access:&BTreeSet<Access>,arguments:&BTreeMap<ValueIdentity,bounds::Proof>,span:Span)->Result<Vec<MemoryAccess>> {
        let mut memory=Vec::new();
        for access in access {
            if let Access::Initialized{borrows,initialized,when}=access {
                let path=bounds::Clause::both(&self.path, &when.projected_case(true, arguments));
                if path.possible(){
                    let borrows=borrows.at_call(arguments);
                    let initialized=initialized.iter().map(|fact|fact.at_call(arguments)).collect::<Vec<_>>();
                    let read=output_initialization_read(&borrows,&initialized,&path);
                    Requirement::Initialized{borrow:borrows,read,code:"B229",span}.check(self.scope.source(), None)?;
                }
                continue;
            }
            let Access::Dataset{borrows,when,index,access,atomic}=access else {continue};
            let path=bounds::Clause::both(&self.path, &when.projected_case(true, arguments));
            if !path.possible(){continue;}
            let cross=!matches!(index.and_then(|index|index.at_call(arguments)),Some(bounds::Operand::Value(ValueIdentity::Register(index))) if index==self.global_index);
            let borrow=self.current_address_borrow(&borrows.at_call(arguments)).restricted(&path);
            for (source,when) in borrow.sources.entries() {
                if !when.possible(){continue;}
                let Origin::Position(origin)=source else {continue};
                memory.extend(origin.aliases().into_iter().map(|alias|MemoryAccess{alias,access:if *atomic{super::Access::Atomic}else{*access}}));
                for ((scalar,_),position) in origin.layout.parts.iter().zip(origin.positions.iter()) {
                    let mut pending=vec![position];
                    while let Some(position)=pending.pop() {
                        match position {
                            Position::Element(binding,word)=>{
                                if access.writes()&&!atomic {
                                    if cross || !matches!(self.view_access[binding],super::Access::Write|super::Access::InOut) {
                                        return self.fail(span,"B144",format!("没有 {} 当前元素的独占修改权",self.program.buffers[*binding].name));
                                    }
                                    self.element_domains.insert(*binding);
                                }
                                if access.reads(){
                                    self.reads.insert(*binding);
                                    if !atomic{self.memory_regions.entry(*binding).or_default().0.insert((cross,*word,*word+scalar.size()));}
                                }
                                if access.writes(){
                                    self.writes.insert(*binding);
                                    self.memory_regions.entry(*binding).or_default().1.insert((*word,*word+scalar.size()),None);
                                }
                                if cross||*atomic{self.cross_access.insert(*binding);}
                            }
                            Position::Indexed(choices,_)=>pending.extend(choices.iter()),
                            _=>{},
                        }
                    }
                }
            }
        }
        Ok(memory)
    }
}

pub(super) mod relation;
use relation::memory_events;

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(PositionAccess);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        layout: Slot,
        written: Slot,
        writable: Slot,
        initialized: Slot,
    }
    #[repr(C)]
    struct Writer {
        layout: WriteCallback,
        written: WriteCallback,
        writable: WriteCallback,
        initialized: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        layout: ReadCallback,
        written: ReadCallback,
        writable: ReadCallback,
        initialized: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_position_access"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_position_access"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for PositionAccess {
        fn empty() -> Self {
            Self {
                layout: <address::Layout as Native>::empty(),
                written: <Option<bounds::Proof> as Native>::empty(),
                writable: <bool as Native>::empty(),
                initialized: <bool as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        layout: contracts::address(&self.layout).cast_mut(),
                        written: contracts::address(&self.written).cast_mut(),
                        writable: contracts::address(&self.writable).cast_mut(),
                        initialized: contracts::address(&self.initialized).cast_mut(),
                    },
                    Writer {
                        layout: contracts::write_value::<address::Layout>,
                        written: contracts::write_value::<Option<bounds::Proof>>,
                        writable: contracts::write_value::<bool>,
                        initialized: contracts::write_value::<bool>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        layout: contracts::slot(&mut output.layout),
                        written: contracts::slot(&mut output.written),
                        writable: contracts::slot(&mut output.writable),
                        initialized: contracts::slot(&mut output.initialized),
                    },
                    Reader {
                        layout: contracts::read_value::<address::Layout>,
                        written: contracts::read_value::<Option<bounds::Proof>>,
                        writable: contracts::read_value::<bool>,
                        initialized: contracts::read_value::<bool>,
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

crate::codec::contracts::codec!(Access);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        resource: ReadCallback,
        position: ReadCallback,
        memory: ReadCallback,
        dataset: ReadCallback,
        initialized: ReadCallback,
        stored: ReadCallback,
    }
    unsafe extern "C" fn build_resource(context: Slot, destination: Slot) -> ReadStatus {
        let mut resources = <Borrow as Native>::empty();
        let mut borrows = <Borrow as Native>::empty();
        let mut kind = <view::Exclusion as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        #[repr(C)]
        struct Fields {
            resources: Slot,
            borrows: Slot,
            kind: Slot,
            when: Slot,
        }
        #[repr(C)]
        struct Host {
            resources: ReadCallback,
            borrows: ReadCallback,
            kind: ReadCallback,
            when: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_resource"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    resources: contracts::slot(&mut resources),
                    borrows: contracts::slot(&mut borrows),
                    kind: contracts::slot(&mut kind),
                    when: contracts::slot(&mut when),
                },
                Host {
                    resources: contracts::read_value::<Borrow>,
                    borrows: contracts::read_value::<Borrow>,
                    kind: contracts::read_value::<view::Exclusion>,
                    when: contracts::read_value::<bounds::Condition>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Resource {
                resources,
                borrows,
                kind,
                when,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrows = <Borrow as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        let mut position = <PositionAccess as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrows: Slot,
            when: Slot,
            position: Slot,
        }
        #[repr(C)]
        struct Host {
            borrows: ReadCallback,
            when: ReadCallback,
            position: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrows: contracts::slot(&mut borrows),
                    when: contracts::slot(&mut when),
                    position: contracts::slot(&mut position),
                },
                Host {
                    borrows: contracts::read_value::<Borrow>,
                    when: contracts::read_value::<bounds::Condition>,
                    position: contracts::read_value::<PositionAccess>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Position {
                borrows,
                when,
                position,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_memory(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrows = <Borrow as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        let mut access = <crate::ast::Access as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrows: Slot,
            when: Slot,
            access: Slot,
        }
        #[repr(C)]
        struct Host {
            borrows: ReadCallback,
            when: ReadCallback,
            access: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_memory"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrows: contracts::slot(&mut borrows),
                    when: contracts::slot(&mut when),
                    access: contracts::slot(&mut access),
                },
                Host {
                    borrows: contracts::read_value::<Borrow>,
                    when: contracts::read_value::<bounds::Condition>,
                    access: contracts::read_value::<crate::ast::Access>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Memory {
                borrows,
                when,
                access,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_dataset(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrows = <Borrow as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        let mut index = <Option<bounds::Operand> as Native>::empty();
        let mut access = <crate::ast::Access as Native>::empty();
        let mut atomic = <bool as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrows: Slot,
            when: Slot,
            index: Slot,
            access: Slot,
            atomic: Slot,
        }
        #[repr(C)]
        struct Host {
            borrows: ReadCallback,
            when: ReadCallback,
            index: ReadCallback,
            access: ReadCallback,
            atomic: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_dataset"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrows: contracts::slot(&mut borrows),
                    when: contracts::slot(&mut when),
                    index: contracts::slot(&mut index),
                    access: contracts::slot(&mut access),
                    atomic: contracts::slot(&mut atomic),
                },
                Host {
                    borrows: contracts::read_value::<Borrow>,
                    when: contracts::read_value::<bounds::Condition>,
                    index: contracts::read_value::<Option<bounds::Operand>>,
                    access: contracts::read_value::<crate::ast::Access>,
                    atomic: contracts::read_value::<bool>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Dataset {
                borrows,
                when,
                index,
                access,
                atomic,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_initialized(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrows = <Borrow as Native>::empty();
        let mut initialized = <Vec<address::Initialization> as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrows: Slot,
            initialized: Slot,
            when: Slot,
        }
        #[repr(C)]
        struct Host {
            borrows: ReadCallback,
            initialized: ReadCallback,
            when: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_initialized"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrows: contracts::slot(&mut borrows),
                    initialized: contracts::slot(&mut initialized),
                    when: contracts::slot(&mut when),
                },
                Host {
                    borrows: contracts::read_value::<Borrow>,
                    initialized: contracts::read_value::<Vec<address::Initialization>>,
                    when: contracts::read_value::<bounds::Condition>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Initialized {
                borrows,
                initialized,
                when,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_stored(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrows = <Borrow as Native>::empty();
        let mut word = <u32 as Native>::empty();
        let mut value = <Borrow as Native>::empty();
        let mut when = <bounds::Condition as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrows: Slot,
            word: Slot,
            value: Slot,
            when: Slot,
        }
        #[repr(C)]
        struct Host {
            borrows: ReadCallback,
            word: ReadCallback,
            value: ReadCallback,
            when: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_view_access_stored"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrows: contracts::slot(&mut borrows),
                    word: contracts::slot(&mut word),
                    value: contracts::slot(&mut value),
                    when: contracts::slot(&mut when),
                },
                Host {
                    borrows: contracts::read_value::<Borrow>,
                    word: contracts::read_value::<u32>,
                    value: contracts::read_value::<Borrow>,
                    when: contracts::read_value::<bounds::Condition>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Access>() = Access::Stored {
                borrows,
                word,
                value,
                when,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_view_access"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Access {
        fn empty() -> Self {
            Self::Memory {
                borrows: Borrow::default(),
                when: bounds::Condition::constant(false),
                access: crate::ast::Access::Read,
            }
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Resource {
                    resources,
                    borrows,
                    kind,
                    when,
                } => {
                    #[repr(C)]
                    struct Fields {
                        resources: Slot,
                        borrows: Slot,
                        kind: Slot,
                        when: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        resources: WriteCallback,
                        borrows: WriteCallback,
                        kind: WriteCallback,
                        when: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_resource"]
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
                                resources: contracts::address(resources).cast_mut(),
                                borrows: contracts::address(borrows).cast_mut(),
                                kind: contracts::address(kind).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                            },
                            Host {
                                resources: contracts::write_value::<Borrow>,
                                borrows: contracts::write_value::<Borrow>,
                                kind: contracts::write_value::<view::Exclusion>,
                                when: contracts::write_value::<bounds::Condition>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Position {
                    borrows,
                    when,
                    position,
                } => {
                    #[repr(C)]
                    struct Fields {
                        borrows: Slot,
                        when: Slot,
                        position: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrows: WriteCallback,
                        when: WriteCallback,
                        position: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_position"]
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
                                borrows: contracts::address(borrows).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                                position: contracts::address(position).cast_mut(),
                            },
                            Host {
                                borrows: contracts::write_value::<Borrow>,
                                when: contracts::write_value::<bounds::Condition>,
                                position: contracts::write_value::<PositionAccess>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Memory {
                    borrows,
                    when,
                    access,
                } => {
                    #[repr(C)]
                    struct Fields {
                        borrows: Slot,
                        when: Slot,
                        access: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrows: WriteCallback,
                        when: WriteCallback,
                        access: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_memory"]
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
                                borrows: contracts::address(borrows).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                                access: contracts::address(access).cast_mut(),
                            },
                            Host {
                                borrows: contracts::write_value::<Borrow>,
                                when: contracts::write_value::<bounds::Condition>,
                                access: contracts::write_value::<crate::ast::Access>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Dataset {
                    borrows,
                    when,
                    index,
                    access,
                    atomic,
                } => {
                    #[repr(C)]
                    struct Fields {
                        borrows: Slot,
                        when: Slot,
                        index: Slot,
                        access: Slot,
                        atomic: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrows: WriteCallback,
                        when: WriteCallback,
                        index: WriteCallback,
                        access: WriteCallback,
                        atomic: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_dataset"]
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
                                borrows: contracts::address(borrows).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                                access: contracts::address(access).cast_mut(),
                                atomic: contracts::address(atomic).cast_mut(),
                            },
                            Host {
                                borrows: contracts::write_value::<Borrow>,
                                when: contracts::write_value::<bounds::Condition>,
                                index: contracts::write_value::<Option<bounds::Operand>>,
                                access: contracts::write_value::<crate::ast::Access>,
                                atomic: contracts::write_value::<bool>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Initialized {
                    borrows,
                    initialized,
                    when,
                } => {
                    #[repr(C)]
                    struct Fields {
                        borrows: Slot,
                        initialized: Slot,
                        when: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrows: WriteCallback,
                        initialized: WriteCallback,
                        when: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_initialized"]
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
                                borrows: contracts::address(borrows).cast_mut(),
                                initialized: contracts::address(initialized).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                            },
                            Host {
                                borrows: contracts::write_value::<Borrow>,
                                initialized: contracts::write_value::<Vec<address::Initialization>>,
                                when: contracts::write_value::<bounds::Condition>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Stored {
                    borrows,
                    word,
                    value,
                    when,
                } => {
                    #[repr(C)]
                    struct Fields {
                        borrows: Slot,
                        word: Slot,
                        value: Slot,
                        when: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrows: WriteCallback,
                        word: WriteCallback,
                        value: WriteCallback,
                        when: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_view_access_stored"]
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
                                borrows: contracts::address(borrows).cast_mut(),
                                word: contracts::address(word).cast_mut(),
                                value: contracts::address(value).cast_mut(),
                                when: contracts::address(when).cast_mut(),
                            },
                            Host {
                                borrows: contracts::write_value::<Borrow>,
                                word: contracts::write_value::<u32>,
                                value: contracts::write_value::<Borrow>,
                                when: contracts::write_value::<bounds::Condition>,
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
                        resource: build_resource,
                        position: build_position,
                        memory: build_memory,
                        dataset: build_dataset,
                        initialized: build_initialized,
                        stored: build_stored,
                    },
                )
            }
        }
    }
};
