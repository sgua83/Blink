//! CPU位置取址和解释共用原存储、权限、初始化及借用来源。
use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Layout {
    pub size: u32,
    pub align: u32,
    pub parts: Arc<[(Scalar, u32)]>,
}
impl Layout {
    pub fn of(ty: &Ty) -> Self { Self { size: ty.size, align: ty.align, parts: ty.components.clone().into() } }
}
impl crate::module::codec::Encode for Layout {
    fn encode(&self, w: &mut crate::module::codec::Writer) {
        self.size.encode(w); self.align.encode(w); self.parts.as_ref().encode(w);
    }
}
impl crate::module::codec::Decode for Layout {
    fn decode(r: &mut crate::module::codec::Reader<'_>) -> crate::module::codec::Result<Self> {
        Ok(Self { size: u32::decode(r)?, align: u32::decode(r)?, parts: Vec::<(Scalar,u32)>::decode(r)?.into() })
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Root {
    Local { base: usize, len: usize },
    Parameter(usize),
    Resource(view::Borrow),
    Buffer(usize),
    Derived(view::Borrow),
    External,
}
crate::module::codec::variants!(Root { 0=>Local{base,len}, 1=>Parameter(index), 2=>Resource(borrow), 3=>Buffer(binding), 4=>Derived(borrow), 5=>External });
/// A projection of original storage; positions stay in the checking function.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Origin {
    pub root: Root,
    pub layout: Layout,
    pub base: Layout,
    pub words: Arc<[u32]>,
    pub positions: Arc<[Position]>,
    pub writable: bool,
    pub initialized: Arc<[bool]>,
}
impl Origin {
    pub fn symbolic_input(&self)->bool {
        match &self.root {
            Root::Derived(borrow)=>borrow.sources.iter().any(|source|match source {
                view::Source::Parameter(ValueIdentity::Register(_)|ValueIdentity::ParameterContent(..)|ValueIdentity::DatasetContent(..))=>true,
                view::Source::Position(origin)=>origin.symbolic_input(),
                _=>false,
            }),
            _=>false,
        }
    }
    pub fn layout_valid(&self)->bool {
        match &self.root {
            Root::Derived(borrow)=>borrow.sources.iter().all(|source|match source {
                view::Source::Position(origin)=>origin.layout==self.base&&origin.layout_valid(),
                view::Source::External|view::Source::Parameter(_)|view::Source::Inactive=>true,
                _=>false,
            }),
            _=>true,
        }
    }
    pub fn position_at(&self,word:u32)->Option<Position> {
        let component=self.layout.parts.iter().position(|(_,at)|*at==word)?;
        if !self.positions.is_empty(){return Some(self.positions[component].clone());}
        if self.initialized[component]{return Some(Position::Initialized);}
        if let Root::Derived(borrow)=&self.root {
            let parent_word=self.words[component];
            let mut known=None;
            for(source,when)in borrow.sources.entries() {
                if !when.possible(){continue;}
                let view::Source::Position(parent)=source else{return None};
                if parent.layout!=self.base{return None;}
                let position=parent.position_at(parent_word)?;
                if known.as_ref().is_some_and(|old|*old!=position){return None;}
                known=Some(position);
            }
            return known;
        }
        None
    }
    pub fn project(&self, selected:&Self) -> Self {
        let indices=selected.words.iter().map(|word|self.layout.parts.iter().position(|(_,at)|at==word).unwrap()).collect::<Vec<_>>();
        Self { root:self.root.clone(),layout:selected.layout.clone(),base:self.base.clone(),
            words:indices.iter().map(|&i|self.words[i]).collect(),
            positions:if self.positions.is_empty(){Arc::from([])}else{indices.iter().map(|&i|self.positions[i].clone()).collect()},
            writable:self.writable&&selected.writable,initialized:indices.iter().enumerate().map(|(component,&i)|self.initialized[i]||selected.initialized[component]).collect() }
    }
    pub fn normalize(&self) -> view::Borrow {
        let Root::Derived(borrow)=&self.root else {
            return view::Borrow::source(view::Source::Position(Arc::new(self.clone())));
        };
        let mut result=view::Borrow::default();
        for(source,when)in borrow.sources.entries() {
            let value=match source {
                view::Source::Position(parent) if parent.layout==self.base =>
                    view::Borrow::source(view::Source::Position(Arc::new(parent.project(self)))),
                view::Source::External => {
                    let mut value=self.clone();value.root=Root::External;value.base=value.layout.clone();
                    value.words=value.layout.parts.iter().map(|(_,at)|*at).collect();value.initialized=vec![true;value.layout.parts.len()].into();
                    view::Borrow::source(view::Source::Position(Arc::new(value)))
                }
                source => {
                    let mut value=self.clone();value.root=Root::Derived(view::Borrow::source(source.clone()));
                    view::Borrow::source(view::Source::Position(Arc::new(value)))
                }
            };
            result.union(&value.restricted(&when));
        }
        if !borrow.inputs.is_empty()||!borrow.calls.is_empty() {
            let mut value=self.clone();
            value.root=Root::Derived(view::Borrow {inputs:borrow.inputs.clone(),calls:borrow.calls.clone(),..view::Borrow::default()});
            result.sources.insert(view::Source::Position(Arc::new(value)));
        }
        result
    }
    pub fn resources(&self) -> view::Borrow {
        match &self.root {
            Root::Resource(borrow) | Root::Derived(borrow) => borrow.resources(),
            _ => view::Borrow::default(),
        }
    }
    pub fn has_local(&self) -> bool {
        match &self.root {
            Root::Local { .. } | Root::Buffer(_) => true,
            Root::Derived(borrow) => borrow.has_local(),
            _ => false,
        }
    }
    pub fn pending(&self) -> bool {
        matches!(&self.root, Root::Resource(borrow) | Root::Derived(borrow) if borrow.pending())
    }
    pub fn aliases(&self) -> Vec<MemoryAlias> {
        match &self.root {
            Root::Local { base, len } => vec![MemoryAlias::Local { base: *base, len: *len }],
            Root::Buffer(binding) => vec![MemoryAlias::Data(*binding)],
            Root::Derived(borrow) => borrow.aliases(),
            _ => Vec::new(),
        }
    }
    pub fn map_borrows(&self, map: impl Fn(&view::Borrow)->view::Borrow) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Resource(borrow) | Root::Derived(borrow) => *borrow=map(borrow),
            _=>{}
        }
        value
    }
    pub fn bind_outputs(&self, bindings: &BTreeMap<ValueIdentity,ValueIdentity>) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Resource(borrow) | Root::Derived(borrow) => *borrow=borrow.output_bound(bindings),
            _ => {}
        }
        value
    }
    pub fn bound_at_call(&self, bindings: &BTreeMap<ValueIdentity,bounds::Proof>) -> Self {
        let mut value=self.clone();
        value.positions=Arc::from([]);
        match &mut value.root {
            Root::Parameter(parameter) => {
                let borrow=bindings.get(&ValueIdentity::ParameterStorage(*parameter))
                    .map(|proof|proof.view_borrow(0))
                    .unwrap_or_else(||view::Borrow::source(view::Source::Unresolved));
                value.root=Root::Derived(borrow);
            }
            Root::Resource(borrow) | Root::Derived(borrow) => *borrow=borrow.at_call(bindings),
            _ => {}
        }
        value
    }
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->view::Borrow {
        view::Borrow::source(view::Source::Position(Arc::new(self.bound_at_call(bindings))))
    }
}
impl crate::module::codec::Encode for Origin {
    fn encode(&self,w:&mut crate::module::codec::Writer) {
        self.root.encode(w); self.layout.encode(w); self.base.encode(w); self.words.as_ref().encode(w);
        self.writable.encode(w); self.initialized.as_ref().encode(w);
    }
}
impl crate::module::codec::Decode for Origin {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self> {
        let root=Root::decode(r)?;
        if matches!(root,Root::Local { .. }|Root::Buffer(_)) {
            return Err("模块公开地址合同不能包含被调函数的私有存储".into());
        }
        Ok(Self {root,layout:Layout::decode(r)?,base:Layout::decode(r)?,words:Vec::<u32>::decode(r)?.into(),
            positions:Arc::from([]),writable:bool::decode(r)?,initialized:Vec::<bool>::decode(r)?.into()})
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Identity {
    Constant(Bits),
    Value(Vec<ValueIdentity>),
}

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum MemoryTarget { Value(Reg), Call(usize) }
// External仅表示没有静态来源；不能把它当作与本函数实际存储不相交。
pub(super) enum MemoryEffect { Known(Vec<MemoryAccess>), External(Access) }
pub(super) fn external_memory(borrow:&view::Borrow)->bool {
    borrow.sources.entries().any(|(source,when)|when.possible()&&match source {
        view::Source::External=>true,
        view::Source::Position(origin)=>match &origin.root {
            Root::External=>true,
            Root::Derived(borrow)=>external_memory(borrow),
            _=>false,
        },
        _=>false,
    })
}
impl MemoryEffect {
    pub fn of(borrow:&view::Borrow,access:Access)->Self {
        if external_memory(borrow){return Self::External(access);}
        Self::Known(borrow.sources.entries().filter(|(_,when)|when.possible()).flat_map(|(source,_)|match source {
            view::Source::Position(origin)=>origin.aliases().into_iter().map(|alias|MemoryAccess{alias,access:if origin.writable{access}else{Access::Read}}).collect::<Vec<_>>(),
            _=>Vec::new(),
        }).collect())
    }
    pub fn union(&mut self,other:Self) {
        match (&mut *self,other) {
            (Self::Known(a),Self::Known(b))=>a.extend(b),
            (Self::External(access),Self::Known(effects))=>if effects.iter().any(|effect|effect.access!=*access){*access=Access::InOut;},
            (target,Self::External(mut access))=>{
                if match target {Self::Known(effects)=>effects.iter().any(|effect|effect.access!=access),Self::External(old)=>*old!=access}{access=Access::InOut;}
                *target=Self::External(access);
            },
        }
    }
    fn local_effects(&self,locals:&BTreeMap<usize,Ty>)->Vec<MemoryAccess> {
        match self {
            Self::Known(effects)=>effects.clone(),
            Self::External(access)=>locals.iter().map(|(end,ty)|MemoryAccess{alias:MemoryAlias::Local{base:*end-ty.components.len(),len:ty.components.len()},access:*access}).collect(),
        }
    }
}
pub(super) fn apply_memory(code:&mut [Inst],memory:&BTreeMap<MemoryTarget,MemoryEffect>,locals:&BTreeMap<usize,Ty>) {
    fn source(source:&mut IndexedSource,memory:&BTreeMap<MemoryTarget,MemoryEffect>,locals:&BTreeMap<usize,Ty>) {
        if let IndexedSource::Pointer {address,aliases,..}=source {
            *aliases=memory.get(&MemoryTarget::Value(address[0])).into_iter().flat_map(|effect|effect.local_effects(locals)).map(|effect|effect.alias).collect::<BTreeSet<_>>().into_iter().collect();
        }
    }
    for instruction in code {
        match instruction {
            Inst::Call(call)=>{
                call.memory=call.memory_use.take().and_then(|usage|memory.get(&MemoryTarget::Call(usage))).map(|effect|effect.local_effects(locals)).unwrap_or_default();
                for argument in &mut call.arguments {
                    match argument {
                        CallArgument::Value{..}=>{}
                        CallArgument::Reference{address:CallReference{storage:CallStorage::Indexed(value),..},..}=>source(value,memory,locals),
                        _=>{}
                    }
                }
            }
            Inst::LoadIndexed{source:value,..}|Inst::StoreIndexed{source:value,..}=>source(value,memory,locals),
            Inst::PositionAddress{address:CallReference{storage:CallStorage::Indexed(value),..},..}=>source(value,memory,locals),
            Inst::Atomic(value)=>source(&mut value.target,memory,locals),
            Inst::Scope(body)=>apply_memory(body,memory,locals),
            Inst::If{yes,no,..}=>{apply_memory(yes,memory,locals);apply_memory(no,memory,locals);}
            Inst::Loop{condition,body,..}=>{apply_memory(condition,memory,locals);apply_memory(body,memory,locals);}
            _=>{}
        }
    }
}

/// Reads retain the facts at their own use, before later calls or writes.
#[derive(Clone)]
pub(super) struct InitializationRead {
    pub positions:Vec<Position>,
    pub initialized:BTreeSet<Position>,
}
impl InitializationRead {
    pub fn missing(&self)->Vec<&Position> {
        self.positions.iter().filter(|position|!position.initialized_by(&self.initialized)).collect()
    }
    pub fn complete(&self)->bool {
        fn input(position:&Position)->bool {match position {
            Position::Borrowed(borrow,..)=>!borrow.empty()&&borrow.sources.iter().all(|source|match source {
                view::Source::Parameter(ValueIdentity::Register(_)|ValueIdentity::ParameterContent(..))=>true,
                view::Source::Position(origin)=>origin.symbolic_input()&&!origin.has_local(),
                _=>false,
            })&&borrow.inputs.is_empty()&&borrow.calls.is_empty(),
            Position::Both(a,b)|Position::Conditional(_,a,b)=>input(a)&&input(b),
            Position::Indexed(choices,_)=>choices.iter().all(input),
            Position::Initialized=>true,
            _=>false,
        }}
        self.missing().into_iter().all(input)
    }
    pub fn borrow(&self)->view::Borrow {
        fn visit(position:&Position,result:&mut view::Borrow) {match position {
            Position::ApplicationInput(input,..)=>result.inputs.insert(*input),
            Position::Borrowed(borrow,layout,word)=>{
                let(scalar,_)=layout.parts.iter().find(|(_,at)|at==word).unwrap();
                let part=Layout{size:scalar.size(),align:scalar.size(),parts:Arc::from([(*scalar,0)])};
                let origin=Origin{root:Root::Derived(borrow.as_ref().clone()),layout:part,base:layout.clone(),words:Arc::from([*word]),positions:Arc::from([]),writable:false,initialized:Arc::from([false])};
                result.union(&origin.normalize());
            }
            Position::Both(a,b)|Position::Conditional(_,a,b)=>{visit(a,result);visit(b,result);}
            Position::Indexed(choices,_)=>for choice in choices.iter(){visit(choice,result);},
            _=>{},
        }}
        let mut result=view::Borrow::default();
        for position in self.missing(){visit(position,&mut result);}
        result
    }
}

/// A normal-return guarantee names one actual location and retains both outcomes.
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Initialization { pub origin:Origin,pub when:bounds::Condition }
crate::module::codec::record!(Initialization {origin,when});

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct InitializationState {
    pub initialized:BTreeSet<Position>,
    pub path:bounds::Condition,
}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Iteration {
    pub states:Arc<[InitializationState]>,
    pub index:ownership::ApplicationIndex,
    pub range:Option<(u32,u32)>,
    pub nonempty:bool,
    pub outputs:Vec<(Position,Vec<u32>)>,
}

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum InitializationEffect {
    Position(Initialization),
    Call{call:usize,when:bounds::Condition},
    Iteration{iteration:Arc<Iteration>,when:bounds::Condition},
}
impl InitializationEffect {
    pub fn pending(&self)->bool {match self {
        Self::Call{..}=>true,
        Self::Position(fact)=>fact.origin.pending(),
        Self::Iteration{iteration,..}=>iteration.states.iter().any(|state|initialization_effects(&state.initialized,&bounds::Clause::default()).iter().any(Self::pending)),
    }}
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        match self {
            Self::Position(fact)=>Self::Position(Initialization{origin:fact.origin.bind_outputs(bindings),when:fact.when.output_bound(bindings)}),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.output_bound(bindings)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.output_bound(bindings)},
        }
    }
    fn under(&self,condition:bounds::Condition)->Self {
        match self {
            Self::Position(fact)=>Self::Position(Initialization{origin:fact.origin.clone(),when:fact.when.clone().logical(condition,false)}),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.clone().logical(condition,false)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.clone().logical(condition,false)},
        }
    }
}
impl Initialization {
    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        Self{origin:self.origin.bound_at_call(bindings),when:self.when.guaranteed_at_call(bindings)}
    }
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        Self{origin:self.origin.bind_outputs(bindings),when:self.when.guaranteed_at_call(&bindings.iter().map(|(source,target)|(*source,bounds::Proof::default().with_identity(*target))).collect())}
    }
    pub fn positions(&self,path:&bounds::Clause)->Vec<Position> {
        match &self.origin.root {
            Root::Derived(borrow)=>{
                let borrow=borrow_in_path(borrow,path);
                borrowed_positions(&borrow,&self.origin.base,&self.origin.words)
            }
            _=>self.origin.layout.parts.iter().filter_map(|(_,word)|self.origin.position_at(*word)).collect(),
        }
    }
}
pub(super) fn merge_initializations(values:impl IntoIterator<Item=Initialization>)->Vec<Initialization> {
    let mut locations:BTreeMap<Origin,bounds::Condition>=BTreeMap::new();
    for fact in values {
        if !fact.when.active(){continue;}
        locations.entry(fact.origin).and_modify(|when|*when=when.clone().logical(fact.when.clone(),true)).or_insert(fact.when);
    }
    locations.into_iter().map(|(origin,when)|Initialization{origin,when}).collect()
}
pub(super) fn borrow_in_path(borrow:&view::Borrow,path:&bounds::Clause)->view::Borrow {
    fn edges<K:Ord+Clone>(values:&view::Edges<K>,path:&bounds::Clause)->view::Edges<K>{
        let mut result=view::Edges::default();
        for(key,when)in values.entries() {
            let condition=bounds::Condition::guard(when.clone());
            if !condition.assume(path,true).possible(){continue;}
            let when=if condition.truth_in(path)==Some(true){bounds::Clause::default()}else{when};
            result.insert_when(key.clone(),when);
        }
        result
    }
    view::Borrow{sources:edges(&borrow.sources,path),inputs:edges(&borrow.inputs,path),calls:edges(&borrow.calls,path)}
}
pub(super) fn initialization_effects(initialized:&BTreeSet<Position>,path:&bounds::Clause)->Vec<InitializationEffect> {
    fn visit(position:&Position,when:bounds::Condition,result:&mut Vec<InitializationEffect>) {
        match position {
            Position::Borrowed(borrow,layout,word)=>{
                let(scalar,_)=layout.parts.iter().find(|(_,offset)|offset==word).unwrap();
                let borrow=borrow_in_path(borrow,&when.assume(&bounds::Clause::default(),true));
                let part=Layout{size:scalar.size(),align:scalar.size(),parts:Arc::from([(*scalar,0)])};
                let origin=Origin{root:Root::Derived(borrow),layout:part,base:layout.clone(),words:Arc::from([*word]),positions:Arc::from([]),writable:true,initialized:Arc::from([true])};
                result.push(InitializationEffect::Position(Initialization{origin,when}));
            }
            Position::Initialization(effect)=>result.push(effect.under(when)),
            Position::Conditional(choice,a,b)=>{
                visit(a,when.clone().logical(choice.condition(),false),result);
                visit(b,when.logical(choice.condition().negate(),false),result);
            }
            Position::Both(a,b)=>{visit(a,when.clone(),result);visit(b,when,result);}
            Position::Indexed(choices,index)=>for(i,choice)in choices.iter().enumerate(){
                visit(choice,when.clone().logical(bounds::Condition::selected(*index,i),false),result);
            },
            _=>{},
        }
    }
    let mut result=Vec::new();
    let condition=bounds::Condition::execution(path);
    for position in initialized {visit(position,condition.clone(),&mut result);}
    result
}
pub(super) fn initialized_by_effects(position:&Position,initialized:&BTreeSet<Position>,path:&bounds::Clause)->bool {
    let effects=initialization_effects(initialized,&bounds::Clause::default());
    let facts=merge_initializations(effects.into_iter().filter_map(|effect|match effect{
        InitializationEffect::Position(fact)=>Some(fact),InitializationEffect::Call{..}|InitializationEffect::Iteration{..}=>None,
    }));
    let known=facts.iter().filter(|fact|fact.when.truth_in(path)==Some(true)).flat_map(|fact|fact.positions(path)).collect();
    position.initialized_by(&known)
}

pub(super) fn borrowed_positions(borrow:&view::Borrow,layout:&Layout,words:&[u32])->Vec<Position> {
    words.iter().map(|word| {
            let mut known=None;
            for(source,when)in borrow.sources.entries() {
                if !when.possible(){continue;}
                let view::Source::Position(origin)=source else{
                    return Position::Borrowed(Arc::new(borrow.clone()),layout.clone(),*word);
                };
                let Some(position)=origin.position_at(*word)else{return Position::Borrowed(Arc::new(borrow.clone()),layout.clone(),*word)};
                if known.as_ref().is_some_and(|old|*old!=position){return Position::Borrowed(Arc::new(borrow.clone()),layout.clone(),*word);}
                known=Some(position);
            }
            known.unwrap_or_else(||Position::Borrowed(Arc::new(borrow.clone()),layout.clone(),*word))
    }).collect()
}

impl ReferenceAddress {
    pub(super) fn source_at(&self,word:u32,index:Reg)->(IndexedSource,u32) {
        let (address,word)=self.component(word);
        let source=match self {
            Self::Borrowed{borrow,..}=>IndexedSource::Pointer{address,index,stride:0,aliases:borrow.aliases()},
            Self::External{..}=>IndexedSource::Pointer{address,index,stride:0,aliases:Vec::new()},
            _=>IndexedSource::View{address,index,stride:0},
        };
        (source,word)
    }
    pub(super) fn positions(&self,words:&[u32])->Option<Vec<Position>> {
        let Self::Borrowed{borrow,layout,..}=self else{return None;};
        Some(borrowed_positions(borrow,layout,words))
    }
}
impl PartialEq for ReferenceAddress {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Contiguous(a), Self::Contiguous(b)) => a == b,
            (Self::Components(a), Self::Components(b)) => a == b,
            (Self::External { identity: a, .. }, Self::External { identity: b, .. })
            | (Self::Borrowed { identity:a,.. },Self::Borrowed { identity:b,.. })=>a==b,
            _ => false,
        }
    }
}
impl ReferenceAddress {
    pub(super) fn overlaps(&self, ty: &Ty, words: &[u32], other: &Self, other_ty: &Ty, other_words: &[u32]) -> bool {
        let (base, other_base) = match (self, other) {
            (Self::External { identity: Identity::Constant(a), .. },
             Self::External { identity: Identity::Constant(b), .. }) => (u128::from(*a), u128::from(*b)),
            (Self::External { identity: Identity::Value(a), .. },
             Self::External { identity: Identity::Value(b), .. })
                if a.iter().any(|identity| b.contains(identity)) => (0, 0),
            _ if self == other => (0, 0),
            _ => return false,
        };
        ty.components.iter().zip(words).any(|((scalar, _), word)| {
            let begin = base + u128::from(*word);
            other_ty.components.iter().zip(other_words).any(|((other_scalar, _), other_word)| {
                let other_begin = other_base + u128::from(*other_word);
                begin < other_begin + u128::from(other_scalar.size())
                    && other_begin < begin + u128::from(scalar.size())
            })
        })
    }
}
impl Builder<'_> {
    pub(super) fn external_write_values(&mut self) {
        for binding in self.env.values_mut() {
            if let Some((proof,uniform,owner))=binding.proof_storage() {
                proof.forget_value(&(0..owner.ty().components.len()).collect::<Vec<_>>(),owner.ty().components.len());
                *proof=std::mem::take(proof).with_external_borrows(owner.ty());
                *uniform=false;
                if let Binding::OwnedValue(owner)=binding {owner.value.constant=None;}
            }
        }
    }
    pub(super) fn position_origin(&self,place:&Place,span:Span)->Result<Origin> {
        let layout=Layout::of(place.ty());
        let mut base=layout.clone();
        let mut words=layout.parts.iter().map(|(_,word)|*word).collect::<Vec<_>>();
        let writable=match place.storage() {
            Place::Local(..)=>true,
            Place::Reference{access,..}|Place::ApplicationElement{access,..}=>matches!(access,Access::Write|Access::InOut),
            Place::Element{binding,writable,..}=>*writable&&matches!(self.view_access[binding],Access::Write|Access::InOut),
            _=>false,
        };
        let root=match place.storage() {
            Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage)=>Root::Local{base:storage.start,len:storage.len()},
            Place::Reference{address:ReferenceAddress::Borrowed{borrow,layout:parent,..},..}=>{
                base=parent.clone();words=external::reference_words(place,&self.locals).unwrap();Root::Derived(borrow.clone())
            }
            Place::Reference{address:ReferenceAddress::External{..},..}=>Root::External,
            Place::Reference{scope:Some(parameter),..}=>{
                let ty=cooperative::parameter_type(self.world,&self.definition.function.params[*parameter],&self.definition)?;
                base=Layout::of(&ty);words=external::reference_words(place,&self.locals).unwrap();Root::Parameter(*parameter)
            }
            Place::ApplicationElement{sources,..}=>Root::Resource(self.data_borrow(sources)),
            Place::Element{binding,..}=>Root::Buffer(*binding),
            Place::ViewElement{source,..}=>{
                let mut borrow=view::Borrow::default();
                for binding in self.env.values() {
                    if let Some((ty,proof))=binding.view_parts()
                        && (0..3).all(|i|proof.known_identity(i)==Some(source[i])) {borrow.union(&proof.typed_view_borrow(ty));}
                }
                if borrow.empty(){borrow=view::Borrow::source(view::Source::Unresolved);}
                Root::Resource(borrow)
            }
            _=>return self.fail(span,"B264","address_of需要CPU普通值的实际连续位置"),
        };
        let positions=self.positions(place);
        let initialized=if positions.len()==layout.parts.len(){
            positions.iter().map(|position|self.position_initialized_in(position,&self.initialized,&self.env)).collect::<Vec<_>>()
        }else{vec![self.is_initialized(place);layout.parts.len()]};
        Ok(Origin{root,layout,base,words:words.into(),positions:positions.into(),writable,initialized:initialized.into()})
    }
    pub(super) fn reference_position_borrow(&self,place:&Place,span:Span)->Result<Option<view::Borrow>> {
        if matches!(place,Place::Reference{address:ReferenceAddress::Borrowed{..}|ReferenceAddress::External{..},..}){
            return Ok(Some(self.position_origin(place,span)?.normalize()));
        }
        Ok(None)
    }
    pub(super) fn current_address_borrow(&self,borrow:&view::Borrow)->view::Borrow {
        borrow.map_sources(|source|match source {
            view::Source::Position(origin)=>{
                let mut origin=origin.map_borrows(|borrow|self.current_address_borrow(borrow));
                if !origin.positions.is_empty(){
                    origin.initialized=origin.positions.iter().map(|position|self.position_initialized_in(position,&self.initialized,&self.env)).collect();
                }
                origin.normalize()
            }
            source=>view::Borrow::source(source),
        })
    }
    pub(super) fn current_address_proof(&self,ty:&Ty,proof:&bounds::Proof)->bounds::Proof {
        let mut result=proof.clone();
        for(offset,field)in ty.borrow_fields() {
            if field.is_address() {
                let component=ty.components.iter().position(|(_,word)|*word==offset).unwrap();
                result=result.with_view_borrow(component,self.current_address_borrow(&proof.view_borrow(component)));
            }
        }
        result
    }
    pub(super) fn address_contents(&self,ty:&Ty,proof:&bounds::Proof)->view::Borrow {
        let mut borrow=view::Borrow::default();
        for(offset,field)in ty.borrow_fields() {
            if !field.is_address(){continue;}
            let component=ty.components.iter().position(|(_,word)|*word==offset).unwrap();
            borrow.union(&self.current_address_borrow(&proof.view_borrow(component)));
        }
        borrow
    }
    pub(super) fn call_address_memory(&mut self,borrow:view::Borrow,span:Span)->Result<usize> {
        if external_memory(&borrow) {self.external_write_values();} else if !borrow.empty() {
        let positions=borrow.sources.entries().filter(|(_,when)|when.possible()).flat_map(|(source,_)|match source {
            view::Source::Position(origin)if origin.writable=>origin.positions.iter().cloned().collect::<Vec<_>>(),
            _=>Vec::new(),
        }).collect::<Vec<_>>();
        let selected=self.env.iter().filter_map(|(name,binding)|{
            let place=binding.stored_place()?;
            let owner=self.positions(&place);
            let components=owner.iter().enumerate().filter_map(|(i,position)|positions.iter().any(|written|
                owned_presence::Presence::selected(written,position)!=owned_presence::Presence::None).then_some(i)).collect::<Vec<_>>();
            (!components.is_empty()).then(||(name.clone(),components))
        }).collect::<BTreeMap<_,_>>();
        for(name,components)in selected {
            match self.env.get_mut(&name).unwrap(){
                Binding::Owned(owner)=>{owner.uniform=false;owner.proof.forget_value(&components,owner.place.ty().components.len());}
                Binding::OwnedValue(owner)=>owner.invalidate_components(&components,None),
                _=>{},
            }
        }
        }
        let usage=self.view_model.uses.len();
        self.check_view_requirement(view::Requirement::Memory{target:MemoryTarget::Call(usage),borrow,span})?;
        Ok(usage)
    }
    pub(super) fn position_address(&mut self,args:&[Expr],span:Span)->Result<Option<Value>> {
        if self.program.target!=Target::Cpu{return self.fail(span,"B265","address_of只取得本进程CPU位置");}
        let [arg]=args else{return self.fail(span,"B153","address_of需要一个实际位置");};
        fn root_name(expr:&Expr)->Option<&String>{match &expr.kind{ExprKind::Name(name)=>Some(name),ExprKind::Member(base,_)|ExprKind::Index(base,_)=>root_name(base),_=>None}}
        if let Some(name)=root_name(arg)
            && let Some(Binding::Value(value))=self.env.get(name).cloned() {
            let owner=self.materialize_array(value);self.env.insert(name.clone(),Binding::Owned(owner));
        }
        let Some(place)=self.place(arg)?else{return Ok(None);};
        if place.ty().owns_resources()||place.ty().contains_views()||place.ty().components.is_empty(){
            return self.fail(span,"B263","address_of需要非空普通值位置，资源通过原拥有者或借用接口交接");
        }
        let origin=self.position_origin(&place,span)?;
        let access=if origin.writable{Access::InOut}else{Access::Read};
        let address=self.external_reference(&place,access,span)?;
        let dst=self.reg(Scalar::U64);
        self.code.push(Inst::PositionAddress{dst,address});
        let ty=Ty::builtin("address").unwrap();
        let borrow=view::Borrow::source(view::Source::Position(Arc::new(origin)));
        Ok(Some(Value{ty,regs:vec![dst],proof:bounds::Proof::default().with_view_borrow(0,borrow),literal:false,uniform:false,constant:None}))
    }

    pub(super) fn external_position(&mut self, explicit: &[GenericArgument], args: &[Expr], span: Span) -> Result<Option<Place>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B265", "unsafe_position只解释本进程CPU地址");
        }
        let ([GenericArgument::Type(name)], [arg]) = (explicit, args) else {
            return self.fail(span, "B153", "unsafe_position<T>需要一个明确普通类型和一个address实参");
        };
        let ty = self.world.ty(name, &self.definition.source, span, &self.definition.module, &self.definition.imports)?;
        if ty.owns_resources() {
            return self.fail(span, "B224", "外部地址位置不能创建Blink拥有资源");
        }
        if ty.contains_views() || ty.components.is_empty() {
            return self.fail(span, "B263", "外部地址位置需要非空普通值布局，不能解释Blink资源引用");
        }
        let Some(mut value) = self.expr(arg)? else { return Ok(None); };
        self.require_same(&Ty::builtin("address").unwrap(), &value.ty, arg.span)?;
        let borrow=borrow_in_path(&self.current_address_borrow(&value.proof.view_borrow(0)),&self.path);
        self.check_view_requirement(view::Requirement::Address{borrow:borrow.clone(),layout:Layout::of(&ty),writes:false,initialized:false,span})?;
        let external=!borrow.pending()&&!borrow.sources.is_empty()&&borrow.sources.iter().all(|source|matches!(source,view::Source::External));
        let identity = match bounds::Proof::capture(&value).known_value(&value.ty) {
            Some(bits) => Identity::Constant(bits[0]),
            None => Identity::Value(value.component_origins(0)),
        };
        value.ty = Ty::scalar(Scalar::U64);
        let low = self.cast(value.clone(), Scalar::U32).regs[0];
        let shift = self.constant(Scalar::U64, 32);
        let high = self.binary(">>", value, shift, span)?;
        let high = self.cast(high, Scalar::U32).regs[0];
        self.check_view_requirement(view::Requirement::Memory{target:MemoryTarget::Value(low),borrow:borrow.clone(),span})?;
        Ok(Some(Place::Reference {
            words: ty.components.iter().map(|(_, offset)| *offset).collect(),
            ty:ty.clone(),
            address: if external{ReferenceAddress::External{address:[low,high],identity}}
                else{ReferenceAddress::Borrowed{address:[low,high],identity,borrow:borrow.clone(),layout:Layout::of(&ty)}},
            scope: None,
            access: Access::InOut,
        }))
    }
}
