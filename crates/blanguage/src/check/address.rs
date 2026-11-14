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
    Resource { borrow: view::Borrow, index: Option<bounds::Operand> },
    Buffer(usize),
    Derived(view::Borrow),
    External(Identity),
    Dataset { borrow: view::Borrow, index: Option<bounds::Operand> },
}
crate::module::codec::variants!(Root { 0=>Local{base,len}, 1=>Parameter(index), 2=>Resource{borrow,index}, 3=>Buffer(binding), 4=>Derived(borrow), 5=>External(identity), 6=>Dataset{borrow,index} });
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
// A queried byte interval in one original scalar position; no stored memory state.
#[derive(Clone)]
pub(super) struct MemoryRegion { pub position:Position, pub word:u32, pub offset:u32, pub size:u32 }
// One original loop input and its actual projections name the same logical storage.
// Its byte layout does not depend on closing the source/lifetime relation.
fn same_loop_resource(a:&view::Borrow,b:&view::Borrow)->bool {
    if !a.calls.is_empty()||!b.calls.is_empty()||!a.reads.is_empty()||!b.reads.is_empty(){return false;}
    if !a.sources.is_empty()||!b.sources.is_empty(){
        if !a.inputs.is_empty()||!b.inputs.is_empty(){return false;}
        let mut a=a.sources.iter();let mut b=b.sources.iter();
        return match(a.next(),b.next(),a.next(),b.next()){
            (Some(view::Source::Element{parent,fields,index}),Some(view::Source::Element{parent:other,fields:other_fields,index:other_index}),None,None)
                if fields==other_fields&&index==other_index=>same_loop_resource(parent,other),
            (Some(view::Source::Projection{parent,fields}),Some(view::Source::Projection{parent:other,fields:other_fields}),None,None)
                if fields==other_fields=>same_loop_resource(parent,other),
            _=>false,
        };
    }
    let mut a=a.inputs.iter();let mut b=b.inputs.iter();
    matches!((a.next(),b.next(),a.next(),b.next()),(Some(a),Some(b),None,None)if a==b)
}
impl MemoryRegion {
    pub(super) fn written_bytes(&self,position:&Position,scalar:Scalar)->u8 {
        let offset=match (&self.position,position) {
            (Position::Borrowed(other,base,word),Position::Borrowed(borrow,layout,at)) if base==layout=>{
                if !same_loop_resource(other,borrow)&&!borrow.sources.entries().any(|(source,when)|other.overlaps_source(source,&when)){return 0;}
                i64::from(*word)+i64::from(self.offset)-i64::from(*at)
            },
            (written,position) if written==position=>i64::from(self.offset),
            (written,position) if owned_presence::Presence::selected(written,position)!=owned_presence::Presence::None=>return ((1u16<<scalar.size())-1) as u8,
            _=>return 0,
        };
        let begin=offset.max(0);let end=(offset+i64::from(self.size)).min(i64::from(scalar.size()));
        if begin>=end{return 0;}
        (((1u16<<(end-begin))-1)<<begin) as u8
    }
}
impl Origin {
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let mut origin=self.clone();
        origin.root=match &self.root {
            Root::Parameter(parameter)if values.contains_key(&ValueIdentity::ParameterStorage(*parameter))=>{origin.positions=Arc::from([]);Root::Derived(values[&ValueIdentity::ParameterStorage(*parameter)].view_borrow(0))},
            Root::External(identity)=>Root::External(identity.substitute_values(values)),
            Root::Derived(borrow)=>Root::Derived(borrow.substitute_values(values)),
            Root::Resource{borrow,index}=>Root::Resource{borrow:borrow.substitute_values(values),index:*index},
            root=>root.clone(),
        };
        if let Root::Resource{borrow,index:Some(index)}|Root::Dataset{borrow,index:Some(index)}=&mut origin.root
            &&let bounds::Operand::Value(id)=index && values.contains_key(id) {
            match index.at_call(values){Some(bound)=>*index=bound,None=>*borrow=view::Borrow::source(view::Source::Unresolved)}
        }
        origin
    }

    pub fn symbolic_input(&self)->bool {
        match &self.root {
            Root::Derived(borrow)|Root::Dataset{borrow,..}=>borrow.sources.iter().any(|source|match source {
                view::Source::Parameter(ValueIdentity::Register(_)|ValueIdentity::ParameterContent(..)|ValueIdentity::DatasetContent(..),_)
                |view::Source::Contents{..}=>true,
                view::Source::Position(origin)=>origin.symbolic_input(),
                view::Source::Relation(graph,root)=>graph.symbolic(*root),
                _=>false,
            }),
            _=>false,
        }
    }
    pub fn layout_valid(&self)->bool {
        match &self.root {
            Root::Derived(borrow)|Root::Dataset{borrow,..}=>borrow.sources.iter().all(|source|match source {
                view::Source::Position(origin)=>origin.layout==self.base&&origin.layout_valid(),
                view::Source::External|view::Source::Parameter(..)|view::Source::Contents{..}|view::Source::Inactive=>true,
                view::Source::Relation(graph,root)=>graph.symbolic(*root),
                _=>false,
            }),
            _=>true,
        }
    }
    pub fn position_at(&self,word:u32)->Option<Position> {
        let component=self.layout.parts.iter().position(|(_,at)|*at==word)?;
        if !self.positions.is_empty(){return Some(self.positions[component].clone());}
        if let Root::Dataset{borrow,index}=&self.root {
            return dataset_positions(borrow,*index,&self.base,&[self.words[component]]).map(|positions|positions[0].clone());
        }
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
                view::Source::Inactive => view::Borrow::source(view::Source::Inactive),
                view::Source::External => {
                    let mut value=self.clone();value.root=Root::External(Identity::Value(Vec::new()));value.base=value.layout.clone();
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
        if !borrow.inputs.is_empty()||!borrow.calls.is_empty()||!borrow.reads.is_empty() {
            let mut value=self.clone();
            value.root=Root::Derived(view::Borrow {inputs:borrow.inputs.clone(),calls:borrow.calls.clone(),reads:borrow.reads.clone(),..view::Borrow::default()});
            result.sources.insert(view::Source::Position(Arc::new(value)));
        }
        result
    }
    pub fn resources(&self) -> view::Borrow {
        match &self.root {
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} | Root::External(Identity::Storage(borrow)) => borrow.resources(),
            _ => view::Borrow::default(),
        }
    }
    pub fn owners(&self) -> view::Borrow {
        match &self.root {
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} => borrow.owners(),
            _ => view::Borrow::default(),
        }
    }
    pub fn has_local(&self) -> bool {
        match &self.root {
            Root::Local { .. } | Root::Buffer(_) => true,
            Root::Derived(borrow)|Root::Dataset{borrow,..} => borrow.has_local(),
            _ => false,
        }
    }
    pub fn pending(&self) -> bool {
        matches!(&self.root, Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} if borrow.pending())
    }
    pub fn aliases(&self) -> Vec<MemoryAlias> {
        match &self.root {
            Root::Local { base, len } => vec![MemoryAlias::Local { base: *base, len: *len }],
            Root::Buffer(binding) => vec![MemoryAlias::Data(*binding)],
            Root::Derived(borrow)|Root::Dataset{borrow,..} => borrow.aliases(),
            Root::External(Identity::Storage(borrow))=>borrow.aliases(),
            _ => Vec::new(),
        }
    }

    pub fn memory_positions(&self,path:&bounds::Clause)->Option<Vec<Position>> {
        self.memory_regions(path).map(|regions|regions.into_iter().map(|region|region.position).collect())
    }
    pub(super) fn memory_regions(&self,path:&bounds::Clause)->Option<Vec<MemoryRegion>> {
        let borrow=match &self.root {
            Root::External(Identity::Storage(borrow))|Root::Derived(borrow)=>borrow,
            Root::Resource{borrow,..}|Root::Dataset{borrow,..}=>return Some(self.layout.parts.iter().zip(self.words.iter()).map(|((scalar,word),at)|MemoryRegion{position:Position::Borrowed(Arc::new(borrow.clone()),self.base.clone(),*at),word:*word,offset:0,size:scalar.size()}).collect()),
            Root::External(_)=>return None,
            _=>return Some(self.layout.parts.iter().zip(self.positions.iter()).map(|((scalar,word),position)|MemoryRegion{position:position.clone(),word:*word,offset:0,size:scalar.size()}).collect()),
        };
        if borrow.empty()||borrow.pending(){return None;}
        let mut regions=Vec::new();
        for(source,when)in borrow.sources.entries(){
            let when=bounds::Clause::both(path, &when);if !when.possible(){continue;}
            let view::Source::Position(parent)=source else{return None;};
            let parent_regions=parent.memory_regions(&when)?;
            for((scalar,word),at)in self.layout.parts.iter().zip(self.words.iter()) {
                let end=u64::from(*at)+u64::from(scalar.size());if end>u64::from(parent.layout.size){return None;}
                for region in &parent_regions {
                    let begin=(*at).max(region.word);let end=(end as u32).min(region.word+region.size);
                    if begin<end{regions.push(MemoryRegion{position:region.position.clone(),word:*word+begin-*at,offset:region.offset+begin-region.word,size:end-begin});}
                }
            }
        }
        Some(regions)
    }
    pub fn map_borrows(&self, map: impl FnOnce(&view::Borrow)->view::Borrow) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} | Root::External(Identity::Storage(borrow)) => *borrow=map(borrow),
            _=>{}
        }
        value
    }
    pub fn bind_outputs(&self, bindings: &BTreeMap<ValueIdentity,ValueIdentity>) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} => *borrow=borrow.output_bound(bindings),
            Root::External(identity)=>*identity=identity.output_bound(bindings),
            _ => {}
        }
        if let Root::Resource{index:Some(index),..}|Root::Dataset{index:Some(index),..}=&mut value.root {
            if let bounds::Operand::Value(id)=index {if let Some(bound)=bindings.get(id){*id=*bound;}}
        }
        value
    }
    pub fn bound_at_call(&self, bindings: &BTreeMap<ValueIdentity,bounds::Proof>) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Parameter(parameter) => {
                let borrow=bindings.get(&ValueIdentity::ParameterStorage(*parameter))
                    .map(|proof|proof.view_borrow(0))
                    .unwrap_or_else(||view::Borrow::source(view::Source::Unresolved));
                value.root=Root::Derived(borrow);
            }
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} => *borrow=borrow.at_call(bindings),
            Root::External(identity)=>*identity=identity.at_call(bindings),
            _ => return value,
        }
        value.positions=Arc::from([]);
        if let Root::Resource{borrow,index:Some(index)}|Root::Dataset{borrow,index:Some(index)}=&mut value.root {
            match index.at_call(bindings) {Some(bound)=>*index=bound,None=>*borrow=view::Borrow::source(view::Source::Unresolved)}
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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Identity {
    Constant(Bits),
    Value(Vec<ValueIdentity>),
    Storage(view::Borrow),
}

impl Identity {
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        match self {
            Self::Constant(_)=>self.clone(),
            Self::Storage(borrow)=>Self::Storage(borrow.substitute_values(values)),
            Self::Value(ids)=>{
                for id in ids {if let Some(value)=values.get(id).and_then(|proof|proof.addresses.first()).cloned().flatten(){return value;}}
                Self::Value(ids.iter().flat_map(|id|match values.get(id){Some(proof)=>proof.origins.first().cloned().unwrap_or_default(),None=>vec![*id]}).collect())
            },
        }
    }

    pub(super) fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        match self {Self::Value(ids)=>Self::Value(ids.iter().map(|id|bindings.get(id).copied().unwrap_or(*id)).collect()),Self::Storage(borrow)=>Self::Storage(borrow.output_bound(bindings)),constant=>constant.clone()}
    }
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        match self {
            Self::Value(ids)=>{
                for id in ids {if let Some(address)=bindings.get(id).and_then(|proof|proof.addresses.first()).cloned().flatten(){return address;}}
                Self::Value(ids.iter().filter_map(|id|bindings.get(id).and_then(|proof|proof.known_identity(0))).collect())
            },
            Self::Storage(borrow)=>Self::Storage(borrow.map_sources(|source|match source {
                view::Source::Position(origin) if origin.has_local()=>view::Borrow::source(view::Source::External),
                source=>view::Borrow::source(source),
            }).at_call(bindings)),
            constant=>constant.clone(),
        }
    }
    pub(super) fn selected(choice:Option<&bounds::Condition>,a:Option<&Self>,b:Option<&Self>)->Option<Self> {
        if a==b{return a.cloned();}
        let borrow=|value:Option<&Self>|match value {Some(Self::Storage(borrow))=>borrow.clone(),_=>view::Borrow::source(view::Source::External)};
        if !matches!(a,Some(Self::Storage(_)))&&!matches!(b,Some(Self::Storage(_))){return None;}
        Some(Self::Storage(match choice{Some(choice)=>view::Borrow::selected(choice,&borrow(a),&borrow(b)),None=>borrow(a).joined(borrow(b))}))
    }
}
impl crate::module::codec::Encode for Identity {
    fn encode(&self,w:&mut crate::module::codec::Writer){match self {
        Self::Constant(bits)=>{0u8.encode(w);bits.encode(w);},
        Self::Value(ids)=>{1u8.encode(w);ids.encode(w);},
        Self::Storage(borrow)=>{2u8.encode(w);borrow.map_sources(|source|match source {view::Source::Position(origin)if origin.has_local()=>view::Borrow::source(view::Source::External),source=>view::Borrow::source(source)}).encode(w);},
    }}
}
impl crate::module::codec::Decode for Identity {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{Ok(match u8::decode(r)?{0=>Self::Constant(Bits::decode(r)?),1=>Self::Value(Vec::decode(r)?),2=>Self::Storage(view::Borrow::decode(r)?),_=>return Err("模块地址数值关系无效".into())})}
}


#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum MemoryTarget { Value(Reg), Call(usize) }
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum Capture { Read, Write }
// External仅表示没有静态来源；不能把它当作与本函数实际存储不相交。
#[derive(Default)]
pub(super) struct MemoryEffect {external:Option<Access>,known:Vec<(Arc<Origin>,bounds::Clause,Access,Option<bounds::Proof>)>}
pub(super) fn external_memory(borrow: &view::Borrow) -> bool {
    borrow.sources.entries().any(|(source, when)| {
        when.possible() && source_has_external_memory(source, &when)
    })
}

fn source_has_external_memory(source: &view::Source, when: &bounds::Clause) -> bool {
    match source {
        view::Source::External => true,
        view::Source::Position(origin) => position_has_external_memory(origin, when),
        _ => false,
    }
}

fn position_has_external_memory(origin: &Origin, when: &bounds::Clause) -> bool {
    match &origin.root {
        Root::External(_) => origin.memory_positions(when).is_none(),
        Root::Derived(borrow) => external_memory(borrow),
        _ => false,
    }
}

fn merged_external_access(left: Option<Access>, right: Option<Access>) -> Option<Access> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => Some(Access::InOut),
        (left, right) => left.or(right),
    }
}
impl MemoryEffect {
    pub fn of(borrow: &view::Borrow, access: Access, written: Option<&bounds::Proof>) -> Self {
        Self::from_sources(borrow.sources.entries(), access, written)
    }
    pub(super) fn from_sources<'a>(
        sources: impl Iterator<Item = (&'a view::Source, bounds::Clause)>,
        access: Access,
        written: Option<&bounds::Proof>,
    ) -> Self {
        let mut effect = Self::default();
        for (source, when) in sources.filter(|(_, when)| when.possible()) {
            if effect.external.is_none() && source_has_external_memory(source, &when) {
                effect.external = Some(access);
            }
            if let view::Source::Position(origin) = source {
                let access = if origin.writable { access } else { Access::Read };
                effect.known.push((origin.clone(), when, access, written.cloned()));
            }
        }
        effect
    }
    pub fn union(&mut self,other:Self) {
        self.known.extend(other.known);
        self.external = merged_external_access(self.external, other.external);
    }
    fn same_element(origin:&Origin,when:&bounds::Clause,position:&Position,index:Option<bounds::Operand>)->Option<bool> {
        let (Some(index),Position::Borrowed(borrow,layout,_))=(index,position)else{return None;};
        let(Root::Resource{borrow:source,index:Some(written)}|Root::Dataset{borrow:source,index:Some(written)})=&origin.root else{return None;};
        if origin.base!=*layout||!same_loop_resource(source,borrow){return None;}
        bounds::Condition::index_equal(*written,index).truth_in(when)
    }
    // The loop owner uses this query only for a forward cursor's preceding iterations.
    pub(super) fn outside_element_bytes(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        if self.external.is_some_and(|access|access.writes()){return ((1u16<<scalar.size())-1)as u8;}
        self.known.iter().filter(|(_,_,access,_)|access.writes()).fold(0,|bytes,(origin,when,_,_)|{
            bytes|if Self::same_element(origin,when,position,index)==Some(true){0}else{Self::origin_written_bytes(origin,when,position,scalar,index)}
        })
    }
    fn origin_written_bytes(origin:&Origin,when:&bounds::Clause,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        if Self::same_element(origin,when,position,index)==Some(false){return 0;}
        let all=((1u16<<scalar.size())-1) as u8;
        let known=||origin.memory_regions(when).map(|regions|regions.iter().fold(0,|bytes,region|bytes|region.written_bytes(position,scalar))).unwrap_or(all);
        match position{
            Position::Initialized=>0,
            Position::Local(_)=>known(),
            Position::Element(binding,_)=>if origin.aliases().iter().any(|alias|matches!(alias,MemoryAlias::Data(actual)if actual==binding)){all}else{0},
            Position::Borrowed(borrow,_,_) if matches!(&origin.root,
                Root::Resource{borrow:source,..}|Root::Dataset{borrow:source,..}if same_loop_resource(source,borrow))
                ||!borrow.pending()&&!borrow.unresolved()&&!external_memory(borrow)&&!borrow.sources.iter().any(|source|matches!(source,view::Source::Parameter(..)))=>known(),
            Position::Both(a,b)|Position::Conditional(_,a,b)=>Self::origin_written_bytes(origin,when,a,scalar,index)|Self::origin_written_bytes(origin,when,b,scalar,index),
            Position::Indexed(positions,_)=>positions.iter().fold(0,|bytes,position|bytes|Self::origin_written_bytes(origin,when,position,scalar,index)),
            _=>all,
        }
    }
    pub(super) fn access_conflicts(held:&view::Borrow,accessed:&view::Borrow,access:Access,path:&bounds::Clause)->bool {
        accessed.sources.entries().any(|(source,when)| {
            let view::Source::Position(origin)=source else {return false;};
            let when=bounds::Clause::both(path,&when);
            if !when.possible(){return false;}
            let index=match &origin.root {
                Root::Resource{index,..}|Root::Dataset{index,..}=>*index,
                _=>None,
            };
            let held_sources = held.sources.entries()
                .map(|(source, condition)| (source, bounds::Clause::both(&condition, &when)));
            let effect = Self::from_sources(held_sources, Access::Write, None);
            let owners=origin.owners();
            // A whole dataset loan also keeps the original owned element resources readable.
            let descendant=effect.known.iter().any(|(held,active,held_access,_)| {
                let Root::Dataset{borrow,index:None}=&held.root else {return false;};
                if !access.writes() && !held_access.writes() {return false;}
                let held=borrow.resources();
                owners.sources.entries().any(|(owner,path)|
                    held.overlaps_source(owner,&bounds::Clause::both(active,&path)))
            });
            descendant || origin.memory_regions(&when).is_some_and(|regions|regions.iter().any(|region| {
                let bytes=u16::from(effect.accessed_bytes(&region.position,Scalar::U64,index,|held|access.writes()||held.writes()));
                (bytes>>region.offset)&((1u16<<region.size)-1)!=0
            }))
        })
    }
    // The content producer has already selected this position on a feasible write path.
    pub(super) fn position_may_write(origin: &Origin, when: &bounds::Clause, position: &Position) -> bool {
        position_has_external_memory(origin, when)
            || origin.writable && Self::origin_written_bytes(origin, when, position, Scalar::U64, None) != 0
    }

    pub(super) fn written_bytes(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        self.accessed_bytes(position,scalar,index,Access::writes)
    }
    fn accessed_bytes(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>,select:impl Fn(Access)->bool)->u8 {
        if self.external.is_some_and(&select){return ((1u16<<scalar.size())-1) as u8;}
        self.known.iter().filter(|(_,_,access,_)|select(*access)).fold(0,|bytes,(origin,when,_,_)|bytes|Self::origin_written_bytes(origin,when,position,scalar,index))
    }
    pub(super) fn has_external_write(&self) -> bool {
        self.external.is_some_and(Access::writes)
    }

    // A possible write joins its value with the old value; it is never a must-write guarantee.
    pub(super) fn written_value(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>,previous:&bounds::Proof)->bounds::Proof {
        if self.has_external_write() {
            return bounds::Proof::default();
        }
        let mut writes = self.known.iter().filter(|(_, _, access, _)| access.writes())
            .filter_map(|(origin, when, _, written)| {
                let bytes = Self::origin_written_bytes(origin, when, position, scalar, index);
                (bytes != 0).then_some((origin, when, written, bytes))
            });
        let Some(first) = writes.next() else {
            return previous.clone();
        };
        let previous = previous.numeric_facts().at_call(&BTreeMap::new());
        let mut result = previous.clone();
        let all = ((1u16 << scalar.size()) - 1) as u8;
        for (origin, when, written, bytes) in std::iter::once(first).chain(writes) {
            let incoming=written.as_ref().and_then(|proof|{
                let regions=origin.memory_regions(when)?;let mut incoming=None;
                for region in regions.iter().filter(|region|region.written_bytes(position,scalar)!=0){
                    let component=origin.layout.parts.iter().position(|part|*part==(scalar,region.word))?;
                    if region.offset!=0||region.size!=scalar.size()||region.written_bytes(position,scalar)!=all{return None;}
                    let proof=proof.project(&[component],origin.layout.parts.len()).numeric_facts().at_call(&BTreeMap::new());
                    if let Some(incoming)=&mut incoming{bounds::Proof::join(incoming,&proof,1,None);}else{incoming=Some(proof);}
                }
                incoming
            });
            let incoming=match incoming{Some(incoming)=>incoming,None=>previous.written_range(0,scalar,bytes)};
            result.join(&incoming,1,None);
        }
        result
    }
}

/// The IR only consumes the possible aliases and their access modes.
#[derive(Default)]
pub(super) struct MemoryAccesses {
    known: Vec<MemoryAccess>,
    external: Option<Access>,
}
impl MemoryAccesses {
    pub(super) fn record(&mut self, effect: MemoryEffect) {
        self.known.extend(effect.known.into_iter().flat_map(|(origin, _, access, _)| {
            origin.aliases().into_iter().map(move |alias| MemoryAccess { alias, access })
        }));
        self.external = merged_external_access(self.external, effect.external);
    }

    fn local_effects(&self, locals: &BTreeMap<usize, Ty>) -> Vec<MemoryAccess> {
        let mut effects = self.known.clone();
        if let Some(access) = self.external {
            effects.extend(locals.iter().map(|(end, ty)| MemoryAccess {
                alias: MemoryAlias::Local { base: *end - ty.components.len(), len: ty.components.len() },
                access,
            }));
        }
        effects
    }
}
pub(super) fn apply_memory(code:&mut [Inst],memory:&BTreeMap<MemoryTarget,MemoryAccesses>,locals:&BTreeMap<usize,Ty>) {
    fn source(source:&mut IndexedSource,memory:&BTreeMap<MemoryTarget,MemoryAccesses>,locals:&BTreeMap<usize,Ty>) {
        if let IndexedSource::Pointer {address,aliases,..}=source {
            *aliases=memory.get(&MemoryTarget::Value(address[0])).into_iter().flat_map(|effect|effect.local_effects(locals)).map(|effect|effect.alias).collect::<BTreeSet<_>>().into_iter().collect();
        }
    }
    for instruction in code {
        match &mut (instruction).kind {
            InstKind::Call(call)=>{
                call.memory=call.memory_use.and_then(|usage|memory.get(&MemoryTarget::Call(usage))).map(|effect|effect.local_effects(locals)).unwrap_or_default();
                for argument in &mut call.arguments {
                    match argument {
                        CallArgument::Value{..}=>{}
                        CallArgument::Reference{address:CallReference{storage:CallStorage::Indexed(value),..},..}=>source(value,memory,locals),
                        _=>{}
                    }
                }
            }
            InstKind::LoadIndexed{source:value,..}|InstKind::StoreIndexed{source:value,..}=>source(value,memory,locals),
            InstKind::PositionAddress{address:CallReference{storage:CallStorage::Indexed(value),..},..}=>source(value,memory,locals),
            InstKind::Atomic(value)=>source(&mut value.target,memory,locals),
            InstKind::Scope(body)|InstKind::Cleanup{body,..}=>apply_memory(body,memory,locals),
            InstKind::If{yes,no,..}=>{apply_memory(yes,memory,locals);apply_memory(no,memory,locals);}
            InstKind::Loop{condition,body,..}=>{apply_memory(condition,memory,locals);apply_memory(body,memory,locals);}
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
                view::Source::Parameter(ValueIdentity::Register(_)|ValueIdentity::ParameterContent(..),_)=>true,
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

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum InitializationSource {
    Position(Origin),
    Task(view::Borrow),
}
crate::module::codec::variants!(InitializationSource {0=>Position(origin),1=>Task(borrow)});

impl InitializationSource {
    pub(super) fn pending(&self)->bool {
        match self {Self::Position(origin)=>origin.pending(),Self::Task(borrow)=>borrow.pending()}
    }
    pub(super) fn borrow(&self)->view::Borrow {
        match self {
            Self::Position(origin)=>view::Borrow::source(view::Source::Position(Arc::new(origin.clone()))),
            Self::Task(borrow)=>borrow.clone(),
        }
    }
    pub(super) fn map_borrows(&self,map:impl Fn(&view::Borrow)->view::Borrow)->Self {
        match self {
            Self::Position(origin)=>Self::Position(origin.map_borrows(map)),
            Self::Task(borrow)=>Self::Task(map(borrow)),
        }
    }
    fn bind_outputs(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        match self {
            Self::Position(origin)=>Self::Position(origin.bind_outputs(bindings)),
            Self::Task(borrow)=>Self::Task(borrow.output_bound(bindings)),
        }
    }
}

/// A normal-return guarantee names actual initialized locations.
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Initialization { pub source:InitializationSource,pub when:bounds::Condition }
crate::module::codec::record!(Initialization {source,when});

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
    pub(super) fn into_positions(self,path:&bounds::Clause)->Vec<Position> {
        let Self::Position(fact)=self else { return vec![Position::Initialization(Arc::new(self))]; };
        merge_initializations([fact]).into_iter().flat_map(|fact| {
            if !fact.source.pending() && fact.when.truth_in(path)==Some(true) {
                fact.positions(path)
            } else {
                vec![Position::Initialization(Arc::new(Self::Position(fact)))]
            }
        }).collect()
    }
    pub fn pending(&self)->bool {match self {
        Self::Call{..}=>true,
        Self::Position(fact)=>fact.source.pending(),
        Self::Iteration{iteration,..}=>iteration.states.iter().any(|state|initialization_effects(&state.initialized,&bounds::Clause::default()).iter().any(Self::pending)),
    }}
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        match self {
            Self::Position(fact)=>Self::Position(Initialization{source:fact.source.bind_outputs(bindings),when:fact.when.output_bound(bindings)}),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.output_bound(bindings)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.output_bound(bindings)},
        }
    }
    fn under(&self,condition:bounds::Condition)->Self {
        match self {
            Self::Position(fact)=>Self::Position(Initialization{source:fact.source.clone(),when:fact.when.clone().logical(condition,false)}),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.clone().logical(condition,false)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.clone().logical(condition,false)},
        }
    }
}
impl Initialization {
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let source=match &self.source {
            InitializationSource::Position(origin)=>InitializationSource::Position(origin.substitute_values(values)),
            InitializationSource::Task(borrow)=>InitializationSource::Task(borrow.substitute_values(values)),
        };
        Self{source,when:self.when.substitute_values(values)}
    }

    pub fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        let source=match &self.source {
            InitializationSource::Position(origin)=>InitializationSource::Position(origin.bound_at_call(bindings)),
            InitializationSource::Task(borrow)=>InitializationSource::Task(borrow.at_call(bindings)),
        };
        Self{source,when:self.when.guaranteed_at_call(bindings)}
    }
    pub fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self {
        Self{source:self.source.bind_outputs(bindings),when:self.when.guaranteed_at_call(&bindings.iter().map(|(source,target)|(*source,bounds::Proof::default().with_identity(*target))).collect())}
    }
    pub(super) fn position_parts(&self,path:&bounds::Clause)->Option<Vec<(Self,Option<Position>)>> {
        let InitializationSource::Position(origin)=&self.source else {return None;};
        Some(origin.layout.parts.iter().enumerate().map(|(component,(scalar,_))| {
            let size=scalar.size();
            let source=InitializationSource::Position(Origin{
                layout:Layout{size,align:size,parts:Arc::from([(*scalar,0)])},
                words:Arc::from([origin.words[component]]),
                positions:origin.positions.get(component).cloned().into_iter().collect(),
                initialized:Arc::from([origin.initialized[component]]),
                ..origin.clone()
            });
            let part=Self{source,when:self.when.clone()};
            let position=part.positions(path).into_iter().next();
            (part,position)
        }).collect())
    }

    pub fn positions(&self,path:&bounds::Clause)->Vec<Position> {
        fn positions(origin:&Origin,path:&bounds::Clause)->Vec<Position> {
            match &origin.root {
                Root::Dataset{..}=>borrowed_positions(&origin.normalize(),&origin.layout,&origin.layout.parts.iter().map(|(_,word)|*word).collect::<Vec<_>>()),
                Root::Derived(borrow)=>{
                    let borrow=borrow_in_path(borrow,path);
                    borrowed_positions(&borrow,&origin.base,&origin.words)
                }
                _=>origin.layout.parts.iter().filter_map(|(_,word)|origin.position_at(*word)).collect(),
            }
        }
        match &self.source {
            InitializationSource::Position(origin)=>positions(origin,path),
            InitializationSource::Task(borrow)=>borrow.sources.entries().flat_map(|(source,when)| {
                if let view::Source::Position(origin)=source
                    && bounds::Condition::guard(when).truth_in(path)==Some(true)
                {
                    positions(origin,path)
                } else {Vec::new()}
            }).collect(),
        }
    }
}
pub(super) fn merge_initializations(values:impl IntoIterator<Item=Initialization>)->Vec<Initialization> {
    let mut locations:BTreeMap<InitializationSource,bounds::Condition>=BTreeMap::new();
    let mut insert=|source,condition:bounds::Condition| {
        if condition.active() {
            locations.entry(source).and_modify(|when|*when=when.clone().logical(condition.clone(),true)).or_insert(condition);
        }
    };
    for fact in values {
        if let InitializationSource::Task(mut borrow)=fact.source {
            for (source,path) in std::mem::take(&mut borrow.sources).into_entries() {
                if let view::Source::Position(origin)=source {
                    insert(InitializationSource::Position(origin.as_ref().clone()),fact.when.clone().logical(bounds::Condition::guard(path),false));
                } else {
                    borrow.sources.insert_when(source,path);
                }
            }
            if borrow.active(){insert(InitializationSource::Task(borrow),fact.when);}
        } else {
            insert(fact.source,fact.when);
        }
    }
    locations.into_iter().map(|(source,when)|Initialization{source,when}).collect()
}
pub(super) fn borrow_in_path(borrow:&view::Borrow,path:&bounds::Clause)->view::Borrow {
    fn edges<K:Ord+Clone>(values:&view::Edges<K>,path:&bounds::Clause)->view::Edges<K>{
        let mut result=view::Edges::default();
        for(key,when)in values.entries() {
            if !bounds::Clause::both(path, &when).possible() {
                continue;
            }
            let condition = bounds::Condition::guard(when.clone());
            let when = if condition.assume(path, false).possible() {
                when
            } else {
                bounds::Clause::default()
            };
            result.insert_when(key.clone(),when);
        }
        result
    }
    view::Borrow{sources:edges(&borrow.sources,path),inputs:edges(&borrow.inputs,path),calls:edges(&borrow.calls,path),reads:edges(&borrow.reads,path)}
}
pub(super) fn initialization_effects(initialized:&BTreeSet<Position>,path:&bounds::Clause)->Vec<InitializationEffect> {
    fn visit(position:&Position,when:bounds::Condition,result:&mut Vec<InitializationEffect>) {
        match position {
            Position::Borrowed(borrow,layout,word)=>{
                let(scalar,_)=layout.parts.iter().find(|(_,offset)|offset==word).unwrap();
                let borrow=borrow_in_path(borrow,&when.assume(&bounds::Clause::default(),true));
                let part=Layout{size:scalar.size(),align:scalar.size(),parts:Arc::from([(*scalar,0)])};
                let origin=Origin{root:Root::Derived(borrow),layout:part,base:layout.clone(),words:Arc::from([*word]),positions:Arc::from([]),writable:true,initialized:Arc::from([true])};
                result.push(InitializationEffect::Position(Initialization{source:InitializationSource::Position(origin),when}));
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

fn dataset_positions(borrow:&view::Borrow,index:Option<bounds::Operand>,layout:&Layout,words:&[u32])->Option<Vec<Position>> {
    fn select(position:Position,index:bounds::Operand)->Option<Position> {
        if matches!(position,Position::Initialized|Position::Element(..)|Position::ApplicationElement(..)){return Some(position);}
        let selected=match index {
            bounds::Operand::Constant(Scalar::U32,value)=>ownership::ApplicationIndex::Constant(value as u32),
            bounds::Operand::Value(id@ValueIdentity::Register(reg))=>ownership::ApplicationIndex::Value(id,reg),
            _=>return None,
        };
        Some(match position {
            Position::Parameter(scope,word)=>Position::ApplicationElement(scope,selected,word as u32),
            Position::ApplicationInput(input,_,word)=>Position::ApplicationInput(input,Some(selected),word),
            Position::Both(a,b)=>Position::both(select(a.as_ref().clone(),index)?,select(b.as_ref().clone(),index)?),
            Position::Conditional(when,a,b)=>Position::selected(when.as_ref().clone(),select(a.as_ref().clone(),index)?,select(b.as_ref().clone(),index)?),
            Position::Indexed(choices,at)=>Position::Indexed(choices.iter().map(|position|select(position.clone(),index)).collect::<Option<_>>()?,at),
            _=>return None,
        })
    }
    let mut result=None;
    for(source,when)in borrow.sources.entries() {
        if !when.possible(){continue;}
        let view::Source::Position(parent)=source else{return None;};
        if parent.layout!=*layout{return None;}
        let positions=words.iter().map(|word|{
            let position=parent.position_at(*word)?;
            match index{Some(index)=>select(position,index),None=>Some(position)}
        }).collect::<Option<Vec<_>>>()?;
        if result.as_ref().is_some_and(|known|*known!=positions){return None;}
        result=Some(positions);
    }
    if !borrow.inputs.is_empty()||!borrow.calls.is_empty()||!borrow.reads.is_empty(){return None;}
    result
}
pub(super) fn borrowed_positions(borrow:&view::Borrow,layout:&Layout,words:&[u32])->Vec<Position> {
    let unresolved = |word| Position::Borrowed(Arc::new(borrow.clone()), layout.clone(), word);
    let mut positions = vec![None; words.len()];
    // Only fields without an unresolved position need the next feasible source.
    let mut remaining = words.len();
    for (source, when) in borrow.sources.entries() {
        if remaining == 0 {
            break;
        }
        if !when.possible() {
            continue;
        }
        let view::Source::Position(origin) = source else {
            return words.iter().map(|&word| unresolved(word)).collect();
        };
        for (&word, known) in words.iter().zip(&mut positions) {
            let Some(slot) = known else {
                *known = Some(origin.position_at(word));
                remaining -= usize::from(known.as_ref().is_some_and(Option::is_none));
                continue;
            };
            if slot.is_some() {
                let position = origin.position_at(word);
                if *slot != position {
                    *slot = None;
                    remaining -= 1;
                }
            }
        }
    }
    words.iter().zip(positions).map(|(&word, known)| {
        known.flatten().unwrap_or_else(|| unresolved(word))
    }).collect()
}

impl ReferenceAddress {
    pub(super) fn source_at(&self,word:u32,index:Reg)->(IndexedSource,u32) {
        let (address,word)=self.component(word);
        let source=match self {
            Self::Borrowed{borrow,..}=>IndexedSource::Pointer{address,index,stride:0,aliases:borrow.aliases()},
            Self::External{..}=>IndexedSource::Pointer{address,index,stride:0,aliases:Vec::new()},
            _=>IndexedSource::View{address,index,stride:ElementStride::Constant(0)},
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
fn resource_metadata(ty:&Ty,resources:Vec<Vec<usize>>)->Vec<std::ops::Range<u32>> {
    resources.into_iter().map(|fields|{
        let(mut offset,mut ty)=(0,ty);
        for field in fields{let(at,next)=if ty.has_variants(){(ty.variant_payload_offset(),ty.variants[field].ty.as_ref())}else{ty.aggregate_part(field)};offset+=at;ty=next;}
        offset..offset+ty.size
    }).collect()
}
impl Builder<'_> {
    pub(super) fn address_write_positions(&self, borrow: &view::Borrow) -> Option<Vec<Position>> {
        let mut positions = Vec::new();
        Self::visit_address_writes(borrow, |_, _, regions| {
            positions.extend(regions.into_iter().map(|region| region.position));
        })?;
        Some(positions)
    }

    fn visit_address_writes(
        borrow: &view::Borrow,
        mut visit: impl FnMut(&Arc<Origin>, bounds::Clause, Vec<MemoryRegion>),
    ) -> Option<()> {
        if !borrow.inputs.is_empty() || !borrow.calls.is_empty() || !borrow.reads.is_empty() {
            return None;
        }
        for (source, when) in borrow.sources.entries().filter(|(_, when)| when.possible()) {
            match source {
                view::Source::Position(origin) => {
                    if view::Borrow::source_unresolved(source, || Some(when.clone())) {
                        return None;
                    }
                    let regions = if origin.writable { origin.memory_regions(&when)? } else { Vec::new() };
                    visit(origin, when, regions);
                }
                view::Source::Inactive => {}
                _ => return None,
            }
        }
        Some(())
    }

    pub(super) fn resolved_address_write(&self, borrow: &view::Borrow) -> bool {
        Self::visit_address_writes(borrow, |_, _, _| {}).is_some()
    }

    pub(super) fn address_write_effect(&self, borrow: &view::Borrow) -> Option<MemoryEffect> {
        let mut effect = MemoryEffect::default();
        Self::visit_address_writes(borrow, |origin, when, _| {
            let external = effect.external.is_none() && match &origin.root {
                // Writable external positions already resolved their regions above.
                Root::External(_) if origin.writable => false,
                _ => position_has_external_memory(origin, &when),
            };
            if external {
                effect.external = Some(Access::Write);
            }
            if origin.writable {
                effect.known.push((origin.clone(), when, Access::Write, None));
            }
        })?;
        Some(effect)
    }
    // Numeric reads name physical storage, independently of initialization markers.
    pub(super) fn value_memory_location(&self,place:&Place)->(Vec<Position>,Option<bounds::Operand>) {
        if let Place::ApplicationElement{resource,index_value,..}=place.storage() {
            fn resource_position(position:Position,resource:&Arc<view::Borrow>)->Position {
                match position {
                    Position::Borrowed(_,layout,word)=>Position::Borrowed(resource.clone(),layout,word),
                    Position::Both(a,b)=>Position::both(resource_position(a.as_ref().clone(),resource),resource_position(b.as_ref().clone(),resource)),
                    Position::Conditional(when,a,b)=>Position::selected(when.as_ref().clone(),resource_position(a.as_ref().clone(),resource),resource_position(b.as_ref().clone(),resource)),
                    Position::Indexed(positions,index)=>Position::Indexed(positions.iter().cloned().map(|position|resource_position(position,resource)).collect(),index),
                    position=>position,
                }
            }
            let resource=Arc::new(resource.clone());
            (place.data_positions().into_iter().map(|position|resource_position(position,&resource)).collect(),Some(*index_value))
        }else{(self.positions(place),None)}
    }
    pub(super) fn memory_write_components(
        &self, borrow: &view::Borrow, place: &Place, outside_element: bool,
    ) -> Vec<usize> {
        let Some(effect) = self.address_write_effect(borrow) else {
            return (0..place.ty().components.len()).collect();
        };
        let (positions, index) = self.value_memory_location(place);
        let written = if outside_element { MemoryEffect::outside_element_bytes } else { MemoryEffect::written_bytes };
        positions.iter().zip(&place.ty().components).enumerate().filter_map(|(component, (position, (scalar, _)))| {
            (written(&effect, position, *scalar, index) != 0).then_some(component)
        }).collect()
    }
    pub(super) fn address_write_components(&self,positions:&[Position],place:&Place)->Vec<usize> {
        self.positions(place).iter().enumerate().filter_map(|(i,position)|positions.iter().any(|written|
            owned_presence::Presence::selected(written,position)!=owned_presence::Presence::None).then_some(i)).collect()
    }
    pub(super) fn update_address_elements(&mut self,borrow:&view::Borrow,effect:&MemoryEffect)->Result<()> {
        let data=borrow.sources.entries().filter(|(_,when)|when.possible()).filter_map(|(source,when)|{
            let view::Source::Position(origin)=source else{return None;};
            if !origin.writable{return None;}
            match &origin.root {
                Root::Resource{borrow:resource,index}|Root::Dataset{borrow:resource,index}=>Some((resource.restricted(&when),origin.words.to_vec(),index.map(|index|{
                    let range=match index{bounds::Operand::Constant(_,value)=>(value as u32,value as u32),_ =>(0,u32::MAX)};(index,range)
                }))),
                Root::External(Identity::Storage(resource))=>Some((resource.resources().restricted(&when),resource.sources.iter().filter_map(|source|match source{view::Source::Position(parent)=>Some(parent.words.iter().copied()),_=>None}).flatten().collect(),None)),
                _=>None,
            }
        }).collect::<Vec<_>>();
        for(resource,words,index)in data {self.update_application_elements(&[],&resource,&words,application::ElementUpdate::Bytes{effect,index})?;}
        Ok(())
    }
    pub(super) fn write_address_values(&mut self,borrow:&view::Borrow,memory:&[(view::Borrow,Option<Access>,Option<bounds::Proof>)])->Result<()> {
        let borrow=self.current_address_borrow(borrow);let mut effect=MemoryEffect::default();
        for(source,access,written)in memory{if let Some(access)=access.filter(|access|access.writes()){
            let source=self.current_address_borrow(source);effect.union(MemoryEffect::of(&source,access,written.as_ref()));
        }}
        if !self.resolved_address_write(&borrow) {
        let data=self.env.values().flat_map(Binding::application_resources)
            .map(|data|application::DataReference::direct(data.clone())).collect::<Vec<_>>();
        for data in data {self.update_application_contents(&data,&data.words(),application::ElementUpdate::Unknown)?;}
        for binding in self.env.values_mut() {
            let metadata=match &*binding{Binding::OwnedValue(owner)=>resource_metadata(&owner.value.ty,view::resource_paths(&owner.value.ty).into_iter().filter(|fields|view::owned_data_at(owner,fields).is_some()).collect()),_=>Vec::new()};
            if let Some((proof,uniform,owner))=binding.proof_storage() {
                let components=owner.ty().components.iter().enumerate().filter_map(|(i,(_,word))|(!metadata.iter().any(|range|range.contains(word))).then_some(i)).collect::<Vec<_>>();
                proof.forget_value(&components,owner.ty().components.len());
                *proof=std::mem::take(proof).with_external_borrows(owner.ty());
                *uniform=false;
                if let Binding::OwnedValue(owner)=binding {owner.value.constant=None;}
            }
        }
        }else{
        self.update_address_elements(&borrow,&effect)?;
        let selected=self.env.iter().filter_map(|(name,binding)|{
            let place=binding.stored_place()?;
            let(positions,index)=self.value_memory_location(&place);
            let components=positions.iter().zip(place.ty().components.iter()).enumerate().filter_map(|(component,(position,(scalar,_)))|{
                let bytes=effect.written_bytes(position,*scalar,index);(bytes!=0).then_some((component,position.clone(),index))
            }).collect::<Vec<_>>();
            (!components.is_empty()).then(||(name.clone(),components))
        }).collect::<BTreeMap<_,_>>();
        for(name,written)in selected {
            let components=written.iter().map(|(component,_,_)|*component).collect::<Vec<_>>();
            match self.env.get_mut(&name).unwrap(){
                Binding::Owned(owner)=>{
                    let ty=owner.place.ty();let mut ranges=bounds::Proof::default();
                    for(component,position,index)in &written{ranges.assign(&[*component],ty.components.len(),effect.written_value(position,ty.components[*component].0,*index,&owner.proof.project(&[*component],ty.components.len())));}
                    owner.uniform=false;owner.proof.forget_value(&components,ty.components.len());owner.proof.constrain_common_ranges(&ranges,ty.components.len());
                },
                Binding::OwnedValue(owner)=>{
                    let ty=&owner.value.ty;let mut ranges=bounds::Proof::default();
                    for(component,position,index)in &written{ranges.assign(&[*component],ty.components.len(),effect.written_value(position,ty.components[*component].0,*index,&owner.value.proof.project(&[*component],ty.components.len())));}
                    owner.invalidate_components(&components,None);owner.value.proof.constrain_common_ranges(&ranges,owner.value.ty.components.len());
                },
                _=>{},
            }

        }
        }
        Ok(())
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
            Place::Reference{address:ReferenceAddress::External{identity,..},..}=>{words=external::reference_words(place,&self.locals).unwrap();Root::External(identity.clone())},
            Place::Reference{scope:Some(parameter),..}=>{
                let ty=cooperative::parameter_type(self.world,&self.scope.parameters().nth(*parameter).unwrap().bound(self.world)?,(&self.scope.function().unwrap().definition.source,&self.scope.function().unwrap().definition.module,&self.scope.function().unwrap().definition.imports,self.scope.function().unwrap().definition.function.kind()),self.program.target)?;
                base=Layout::of(&ty);words=external::reference_words(place,&self.locals).unwrap();Root::Parameter(*parameter)
            }
            Place::ApplicationElement{resource,element_layout,words:actual,index_value,..}=>{
                base=element_layout.clone();words=actual.clone();
                Root::Resource{borrow:resource.clone(),index:Some(*index_value)}
            },
            Place::Element{binding,..}=>Root::Buffer(*binding),
            Place::ViewElement{source,..}=>{
                let mut borrow=view::Borrow::default();
                for binding in self.env.values() {
                    if let Some((ty,proof))=binding.view_parts()
                        && (0..3).all(|i|proof.known_identity(i)==Some(source[i])) {borrow.union(&proof.typed_view_borrow(ty));}
                }
                if borrow.empty(){borrow=view::Borrow::source(view::Source::Unresolved);}
                Root::Resource{borrow,index:None}
            }
            _=>return self.fail(span,"B264","address_of需要CPU普通值的实际连续位置"),
        };
        let positions=self.positions(place);
        let initialized=if positions.len()==layout.parts.len(){
            positions.iter().map(|position|self.position_initialized_in(position,&self.initialized,&self.env,None)).collect::<Vec<_>>()
        }else{vec![self.is_initialized(place);layout.parts.len()]};
        Ok(Origin{root,layout,base,words:words.into(),positions:positions.into(),writable,initialized:initialized.into()})
    }
    pub(super) fn position_borrows(&self,place:&Place,span:Span)->Result<view::Borrow> {
        let origin=self.position_origin(place,span)?;
        let positions=place.data_positions();
        if !matches!(place,Place::Indexed{..}) || !matches!(origin.root,Root::Resource{..}) || positions.is_empty() {
            return Ok(view::Borrow::source(view::Source::Position(Arc::new(origin))));
        }
        // The index selects one row of the existing per-component position choices.
        fn collect(origin:&Origin,positions:&[Position],when:&bounds::Clause,out:&mut view::Borrow) {
            if let Position::Indexed(choices,index)=&positions[0] {
                for choice in 0..choices.len() {
                    let when=bounds::Clause::both(when,&bounds::Clause::selection(*index,choice));
                    if !when.possible(){continue;}
                    let selected=positions.iter().map(|position| {
                        let Position::Indexed(choices,_)=position else{unreachable!()};
                        choices[choice].clone()
                    }).collect::<Vec<_>>();
                    collect(origin,&selected,&when,out);
                }
            } else {
                let mut origin=origin.clone();
                origin.words=positions.iter().map(|position| {
                    let Position::Borrowed(_,_,word)=position else{unreachable!()};
                    *word
                }).collect();
                out.sources.insert_when(view::Source::Position(Arc::new(origin)),when.clone());
            }
        }
        let mut borrow=view::Borrow::default();
        collect(&origin,&positions,&self.path,&mut borrow);
        Ok(borrow)
    }
    pub(super) fn reference_memory(&self,place:&Place,span:Span)->Result<Option<(MemoryTarget,view::Borrow)>> {
        if let Place::Reference{address:ReferenceAddress::Borrowed{address,..}|ReferenceAddress::External{address,..},..}=place{
            return Ok(Some((MemoryTarget::Value(address[0]),self.position_origin(place,span)?.normalize())));
        }
        Ok(None)
    }
    pub(super) fn current_address_borrow(&self,borrow:&view::Borrow)->view::Borrow {
        borrow.map_sources(|source|match source {
            view::Source::Position(origin)=>{
                let mut origin=origin.map_borrows(|borrow|self.current_address_borrow(borrow));
                if !origin.positions.is_empty(){
                    origin.initialized=origin.positions.iter().map(|position|self.position_initialized_in(position,&self.initialized,&self.env,None)).collect();
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
    pub(super) fn capture_address_values(&mut self,memory:usize,capture_kind:Capture)->Vec<view_contracts::ValueSnapshot> {
        let owners=self.env.iter().filter_map(|(name,binding)|binding.stored_place().map(|place|{let(positions,index)=self.value_memory_location(&place);(name.clone(),positions,index)})).collect::<Vec<_>>();
        let mut snapshots=Vec::new();
        let mut capture=|proof:&mut bounds::Proof,uniform:&mut bool,ty:&Ty,mut positions:Vec<(usize,Position)>,index:Option<bounds::Operand>,resources:Vec<Vec<usize>>|{
            let metadata=resource_metadata(ty,resources);
            positions.retain(|(component,_)|!metadata.iter().any(|range|range.contains(&ty.components[*component].1)));
            let fields=ty.borrow_fields().into_iter().filter(|(_,field)|field.is_address()).map(|(at,_)|ty.components.iter().position(|(_,word)|*word==at).unwrap()).collect::<BTreeSet<_>>();
            if capture_kind==Capture::Read{positions.retain(|(component,_)|fields.contains(component));}
            let width=ty.components.len();let previous=proof.clone();
            if capture_kind==Capture::Write{proof.forget_value(&positions.iter().map(|(component,_)|*component).collect::<Vec<_>>(),width);}
            for(component,position)in positions {
                let snapshot=snapshots.len();let identity=ValueIdentity::MemoryValue(memory,snapshot);
                let initial=Arc::new(previous.project(&[component],width));
                snapshots.push(view_contracts::ValueSnapshot{position:position.clone(),index,scalar:ty.components[component].0,proof:initial.clone()});
                if capture_kind==Capture::Write{
                    *proof=std::mem::take(proof).with_component_identity(component,identity);
                    if fields.contains(&component){
                        *proof=std::mem::take(proof).with_view_borrow(component,view::Borrow{reads:view::Edges::from([view_contracts::ContentRead::Snapshot{memory,component:snapshot,position,initial}]),..view::Borrow::default()});
                    }
                }
            }
            if capture_kind==Capture::Write{*uniform=false;}
        };
        for(name,positions,index)in owners {
            let binding=self.env.get_mut(&name).unwrap();
            let resources=match &*binding{Binding::OwnedValue(owner)=>view::resource_paths(&owner.value.ty).into_iter().filter(|fields|view::owned_data_at(owner,fields).is_some()).collect(),_=>Vec::new()};
            let Some((proof,uniform,owner))=binding.proof_storage()else{continue;};
            capture(proof,uniform,owner.ty(),positions.into_iter().enumerate().collect(),index,resources);
            if capture_kind==Capture::Write && let Binding::OwnedValue(owner)=binding{owner.value.constant=None;}
        }
        for binding in self.env.values_mut(){application::each_binding_data(binding,&mut|_,_,data|{
            let source=view::resource_borrow(data);let fields=view::resource_paths(&data.ty);
            let paths=std::iter::once(None).chain(data.elements.known().map(view_contracts::resource_output_paths).into_iter().flatten().map(Some)).collect::<Vec<_>>();
            for path in paths {
                let(borrow,selected)=match path {
                    None=>(source.clone(),&mut *data),
                    Some(resource)=>{
                        let(root,path)=&resource;
                        let mut borrow=view::Borrow::element(source.clone(),fields[*root].clone(),None);
                        for fields in path{borrow=view::Borrow::element(borrow,fields.clone(),None);}
                        let Some(selected)=data.elements.known_mut().and_then(|model|view_contracts::resource_output_mut(model,&resource))else{continue;};(borrow,selected)
                    },
                };
                if selected.ownership==application::Ownership::Moved{continue;}
                selected.prefixes.clear();
                let layout=Layout::of(&selected.ty);let borrow=Arc::new(borrow);
                let resources=view::resource_paths(&selected.ty).into_iter().filter(|fields|application::model_data(selected.elements.known(),fields).is_some()).collect();
                if let Some(value)=selected.elements.known_mut().and_then(returns::Returned::view_value_mut){
                    let positions=value.ty.components.iter().enumerate().map(|(component,(_,word))|(component,Position::Borrowed(borrow.clone(),layout.clone(),*word))).collect();
                    value.proof=bounds::Proof::capture(value);capture(&mut value.proof,&mut value.uniform,&value.ty,positions,None,resources);if capture_kind==Capture::Write{value.constant=None;value.proof=bounds::Proof::capture(value);}
                }
            }
        });}
        snapshots
    }
    pub(super) fn call_address_memory(&mut self,memory:impl IntoIterator<Item=(view::Borrow,Option<Access>)>,span:Span)->Result<usize> {
        let usage=self.view_model.uses.len();let mut written=view::Borrow::default();
        for(borrow,access)in memory{
            if access.is_some_and(|access|access.writes()){written.union(&borrow);}
            self.check_view_requirement(view::Requirement::Memory{target:MemoryTarget::Call(usage),borrow,access,written:None,snapshots:None,span,address_fields:Arc::from([])})?;
        }
        if written.pending()&&!self.resolved_address_write(&written){
            let snapshots=self.capture_address_values(usage,address::Capture::Write);
            if let view::Requirement::Memory{snapshots:target,..}=&mut self.view_model.uses[usage].requirement{*target=Some(snapshots.into());}
        }else{let memory=[(written,Some(Access::Write),None)];self.write_address_values(&memory[0].0,&memory)?;}
        Ok(usage)
    }
    pub(super) fn position_address(&mut self,args:&Expressions,span:Span)->Result<Option<Value>> {
        if self.program.target!=Target::Cpu{return self.fail(span,"B265","address_of只取得本进程CPU位置");}
        if args.len()!=1{return self.fail(span,"B153","address_of需要一个实际位置");}
        let arg=&args.at(0);
        fn root_name(expr:&Expr)->Option<String>{match expr.kind(){ExprKind::Name(name)=>Some(name.into()),ExprKind::Member(base,_)|ExprKind::Index(base,_)=>root_name(&base),_=>None}}
        if let Some(name)=root_name(arg)
            && let Some(Binding::Value(value))=self.env.get(&name).cloned() {
            let mut owner=self.materialize_array(value,None);owner.place=owner.place.read_only();self.env.insert(name.clone(),Binding::Owned(owner));
        }
        let Some(place)=self.place(arg)?else{return Ok(None);};
        if place.ty().owns_resources()||place.ty().contains_views()||place.ty().components.is_empty(){
            return self.fail(span,"B263","address_of需要非空普通值位置，资源通过原拥有者或借用接口交接");
        }
        let origin=self.position_origin(&place,span)?;
        let access=if origin.writable{Access::InOut}else{Access::Read};
        let address=self.external_reference(&place,access,access.writes(),span)?;
        let dst=self.reg(Scalar::U64);
        let ty=Ty::builtin("address").unwrap();

        let mut numeric=origin.clone();
        if let Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage)=place.storage() {
            let ty=self.locals[&storage.end].clone();let whole=Place::Local(ty,(storage.clone()).collect(),storage.clone());
            numeric=self.position_origin(&whole,span)?;
            let first=address.word;let selected=numeric.layout.parts.iter().enumerate().filter_map(|(i,(_,at))|(*at>=first).then_some(i)).collect::<Vec<_>>();
            numeric.layout=Layout{size:numeric.layout.size-first,align:place.ty().align,parts:selected.iter().map(|&i|{let(scalar,at)=numeric.layout.parts[i];(scalar,at-first)}).collect()};
            numeric.positions=selected.iter().map(|&i|numeric.positions[i].clone()).collect();numeric.initialized=selected.iter().map(|&i|numeric.initialized[i]).collect();
            numeric.base=numeric.layout.clone();numeric.words=numeric.layout.parts.iter().map(|(_,at)|*at).collect();
        }
        let identity=match place.storage(){Place::Reference{address:ReferenceAddress::External{identity,..}|ReferenceAddress::Borrowed{identity,..},..}=>identity.clone(),_=>Identity::Storage(view::Borrow::source(view::Source::Position(Arc::new(numeric))))};
        let mut proof=bounds::Proof::default();proof.addresses.push(Some(identity));
        self.emit(InstKind::PositionAddress{dst,address});
        let borrow=view::Borrow::source(view::Source::Position(Arc::new(origin)));
        Ok(Some(Value{ty,regs:vec![dst],proof:proof.with_view_borrow(0,borrow),literal:false,uniform:false,constant:None}))
    }

    pub(super) fn external_position(&mut self,explicit:&[GenericArgument],args:&Expressions,span:Span)->Result<Option<Place>> {
        self.external_position_value(explicit,args.len(),span,args.first().map(|arg|arg.span()).unwrap_or(span),
            |builder|builder.expr(&args.at(0)))
    }
    pub(super) fn external_position_value(&mut self,explicit:&[GenericArgument],count:usize,span:Span,argument_span:Span,
        argument:impl FnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Place>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B265", "unsafe_position只解释本进程CPU地址");
        }
        let ([GenericArgument::Type(name)], 1) = (explicit, count) else {
            return self.fail(span, "B153", "unsafe_position<T>需要一个明确普通类型和一个address实参");
        };
        let ty = self.world.ty(name, self.scope.source(), span, self.scope.module(), self.scope.imports())?;
        if ty.owns_resources() {
            return self.fail(span, "B224", "外部地址位置不能创建Blink拥有资源");
        }
        if ty.contains_views() || ty.components.is_empty() {
            return self.fail(span, "B263", "外部地址位置需要非空普通值布局，不能解释Blink资源引用");
        }
        let Some(mut value) = argument(self)? else { return Ok(None); };
        self.require_same(&Ty::builtin("address").unwrap(), &value.ty, argument_span)?;
        let borrow=borrow_in_path(&self.current_address_borrow(&value.proof.view_borrow(0)),&self.path);
        self.check_view_requirement(view::Requirement::Address{borrow:borrow.clone(),layout:Layout::of(&ty),written:None,writable:false,initialized:false,span})?;
        let external=!borrow.pending()&&!borrow.sources.is_empty()&&borrow.sources.iter().all(|source|matches!(source,view::Source::External));
        let identity = value.proof.addresses.first().cloned().flatten().unwrap_or_else(||match bounds::Proof::capture(&value).known_value(&value.ty) {
            Some(bits) => Identity::Constant(bits[0]),
            None => Identity::Value(value.component_origins(0)),
        });
        value.ty = Ty::scalar(Scalar::U64);
        let low = self.cast(value.clone(), Scalar::U32).regs[0];
        let shift = self.constant(Scalar::U64, 32);
        let high = self.binary(">>", value, shift, span,None)?;
        let high = self.cast(high, Scalar::U32).regs[0];
        let place=Place::Reference {
            words: ty.components.iter().map(|(_, offset)| *offset).collect(),
            ty:ty.clone(),
            address: if external{ReferenceAddress::External{address:[low,high],identity}}
                else{ReferenceAddress::Borrowed{address:[low,high],identity,borrow:borrow.clone(),layout:Layout::of(&ty)}},
            scope: None,
            access: Access::InOut,
        };
        Ok(Some(place))
    }
}
