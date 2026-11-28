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
#[repr(C)]
struct RegionBytes { word: u32, offset: u32, size: u32 }

#[repr(u32)]
enum RegionSeedKind { Positions, Resource, External }

#[repr(C)]
struct RegionSeed { parts: usize, positions: usize, words: usize, kind: RegionSeedKind, materialize: u32 }

#[repr(C)]
struct RegionPart { word: u32, size: u32 }

#[repr(C)]
struct RegionSeedHost {
    context: *mut std::ffi::c_void,
    part: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> RegionPart,
    reserve: unsafe extern "C" fn(*mut std::ffi::c_void, usize),
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, usize, RegionBytes),
}

struct SeedRegions<'a, 'r> {
    origin: &'a Origin,
    resource: Option<&'a view::Borrow>,
    shared_resource: Option<Arc<view::Borrow>>,
    output: Option<&'r mut Vec<MemoryRegion>>,
}

impl SeedRegions<'_, '_> {
    unsafe extern "C" fn part(context: *const std::ffi::c_void, slot: usize) -> RegionPart {
        let input = unsafe { &*context.cast::<Self>() };
        let (scalar, word) = input.origin.layout.parts[slot];
        RegionPart { word, size: scalar.size() }
    }

    unsafe extern "C" fn reserve(context: *mut std::ffi::c_void, count: usize) {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.output.as_mut().unwrap().reserve(count);
    }

    unsafe extern "C" fn emit(context: *mut std::ffi::c_void, slot: usize, bytes: RegionBytes) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let position = if let Some(resource) = input.resource {
            let shared = input.shared_resource.get_or_insert_with(|| Arc::new(resource.clone()));
            Position::Borrowed(Arc::clone(shared), input.origin.base.clone(), input.origin.words[slot])
        } else {
            input.origin.positions[slot].clone()
        };
        input.output.as_mut().unwrap().push(MemoryRegion {
            position, word: bytes.word, offset: bytes.offset, size: bytes.size,
        });
    }

    fn collect(&mut self, kind: RegionSeedKind) -> Option<()> {
        let seed = RegionSeed {
            parts: self.origin.layout.parts.len(),
            positions: self.origin.positions.len(),
            words: self.origin.words.len(),
            kind,
            materialize: self.output.is_some().into(),
        };
        let host = RegionSeedHost { context: (self as *mut Self).cast(), part: Self::part,
            reserve: Self::reserve, emit: Self::emit };
        (unsafe { seed_memory_regions(seed, host) } != 0).then_some(())
    }
}

#[repr(C)]
struct RegionSpan {
    regions: *mut std::ffi::c_void,
    first: usize,
    end: usize,
}

#[repr(C)]
struct RegionHost {
    part: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> RegionBytes,
    region: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> RegionBytes,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, usize, RegionBytes),
    finish: unsafe extern "C" fn(RegionSpan),
}

#[repr(u32)]
enum AddressSourceKind { End, Position, Inactive, Other }

#[repr(u32)]
enum AddressSourceWalk { Write, DerivedRegion }

#[repr(C)]
struct AddressSourceEntry { kind: AddressSourceKind, possible: u32 }

#[repr(C)]
struct AddressSourceHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressSourceEntry,
    projection: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
}

struct AddressSourceInput<'a, 'p, I, F> {
    entries: I,
    path: Option<&'p bounds::Clause>,
    current: Option<(&'a view::Source, bounds::Clause)>,
    visit: F,
}

impl<'a, I, F> AddressSourceInput<'a, '_, I, F>
where
    I: Iterator<Item = (&'a view::Source, bounds::Clause)>,
    F: FnMut(&'a view::Source, &'a Arc<Origin>, bounds::Clause) -> Option<()>,
{
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressSourceEntry {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.current = input.entries.next().map(|(source, when)| {
            let when = match input.path {
                Some(path) => bounds::Clause::both(path, &when),
                None => when,
            };
            (source, when)
        });
        let Some((source, when)) = &input.current else {
            return AddressSourceEntry { kind: AddressSourceKind::End, possible: 0 };
        };
        let kind = match source {
            view::Source::Position(_) => AddressSourceKind::Position,
            view::Source::Inactive => AddressSourceKind::Inactive,
            _ => AddressSourceKind::Other,
        };
        AddressSourceEntry { kind, possible: when.possible().into() }
    }

    unsafe extern "C" fn projection(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (source, when) = input.current.take().unwrap();
        let view::Source::Position(origin) = source else { unreachable!() };
        (input.visit)(source, origin, when).is_some().into()
    }

    fn walk(&mut self, unavailable: bool, walk: AddressSourceWalk) -> Option<()> {
        let host = AddressSourceHost {
            context: (self as *mut Self).cast(),
            next: Self::next,
            projection: Self::projection,
        };
        (unsafe { b_walk_address_sources(unavailable.into(), walk, host) } != 0).then_some(())
    }
}

fn walk_address_sources<'a>(
    borrow: &'a view::Borrow,
    path: Option<&bounds::Clause>,
    visit: impl FnMut(&'a view::Source, &'a Arc<Origin>, bounds::Clause) -> Option<()>,
) -> Option<()> {
    let (walk, unavailable) = if path.is_some() {
        (AddressSourceWalk::DerivedRegion, borrow.empty() || borrow.pending())
    } else {
        (AddressSourceWalk::Write,
            !borrow.inputs.is_empty() || !borrow.calls.is_empty() || !borrow.reads.is_empty())
    };
    AddressSourceInput { entries: borrow.sources.entries(), path, current: None, visit }.walk(unavailable, walk)
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::walk_address_sources"]
    fn b_walk_address_sources(unavailable: u32, walk: AddressSourceWalk, host: AddressSourceHost) -> u32;
    #[link_name = "bootstrap.call_arguments::seed_memory_regions"]
    fn seed_memory_regions(seed: RegionSeed, host: RegionSeedHost) -> u32;
    #[link_name = "bootstrap.call_arguments::project_memory_regions"]
    fn project_memory_regions(origin: *const std::ffi::c_void, parts: usize, parent_size: u32,
        parent: RegionSpan, host: RegionHost) -> u32;
}

unsafe extern "C" fn projected_part(origin: *const std::ffi::c_void, at: usize) -> RegionBytes {
    let origin = unsafe { &*origin.cast::<Origin>() };
    let (scalar, word) = origin.layout.parts[at];
    RegionBytes { word, offset: origin.words[at], size: scalar.size() }
}

unsafe extern "C" fn parent_region(regions: *const std::ffi::c_void, at: usize) -> RegionBytes {
    let region = &unsafe { &*regions.cast::<Vec<MemoryRegion>>() }[at];
    RegionBytes { word: region.word, offset: region.offset, size: region.size }
}

unsafe extern "C" fn append_projected_region(regions: *mut std::ffi::c_void, at: usize, bytes: RegionBytes) {
    let regions = unsafe { &mut *regions.cast::<Vec<MemoryRegion>>() };
    let position = regions[at].position.clone();
    regions.push(MemoryRegion { position, word: bytes.word, offset: bytes.offset, size: bytes.size });
}

unsafe extern "C" fn finish_parent_regions(parent: RegionSpan) {
    let regions = unsafe { &mut *parent.regions.cast::<Vec<MemoryRegion>>() };
    drop(regions.drain(parent.first..parent.end));
}

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
    pub(super) fn has_blink_storage(&self, when: &bounds::Clause) -> bool {
        match &self.root {
            Root::External(_) => false,
            Root::Derived(borrow) => borrow.sources.entries().any(|(source, edge)| {
                matches!(source, view::Source::Position(parent)
                    if parent.has_blink_storage(&bounds::Clause::both(when, &edge)))
            }),
            _ => when.possible(),
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
        self.aliases_when(&|_| true, || true)
    }
    pub(super) fn aliases_when(
        &self, needed: &dyn Fn(&MemoryAlias) -> bool, possible: impl FnOnce() -> bool,
    ) -> Vec<MemoryAlias> {
        match &self.root {
            Root::Local { base, len } => {
                let alias = MemoryAlias::Local { base: *base, len: *len };
                if needed(&alias) && possible() { vec![alias] } else { Vec::new() }
            },
            Root::Buffer(binding) => {
                let alias = MemoryAlias::Data(*binding);
                if needed(&alias) && possible() { vec![alias] } else { Vec::new() }
            },
            Root::Derived(borrow)|Root::Dataset{borrow,..}|Root::External(Identity::Storage(borrow)) => {
                let aliases = borrow.aliases(needed);
                if aliases.is_empty() || possible() { aliases } else { Vec::new() }
            },
            _ => Vec::new(),
        }
    }

    pub fn memory_positions(&self,path:&bounds::Clause)->Option<Vec<Position>> {
        self.memory_regions(path).map(|regions|regions.into_iter().map(|region|region.position).collect())
    }
    pub(super) fn memory_regions(&self,path:&bounds::Clause)->Option<Vec<MemoryRegion>> {
        let mut regions = Vec::new();
        self.collect_memory_regions(path, Some(&mut regions))?;
        Some(regions)
    }
    fn collect_memory_regions(&self, path: &bounds::Clause, output: Option<&mut Vec<MemoryRegion>>) -> Option<()> {
        let (kind, resource) = match &self.root {
            Root::External(Identity::Storage(borrow)) | Root::Derived(borrow) => {
                return self.collect_parent_regions(borrow, path, output);
            }
            Root::Resource { borrow, .. } | Root::Dataset { borrow, .. } => (RegionSeedKind::Resource, Some(borrow)),
            Root::External(_) => (RegionSeedKind::External, None),
            _ => (RegionSeedKind::Positions, None),
        };
        SeedRegions { origin: self, resource, shared_resource: None, output }.collect(kind)
    }

    fn collect_parent_regions(&self, borrow: &view::Borrow, path: &bounds::Clause,
        mut output: Option<&mut Vec<MemoryRegion>>,
    ) -> Option<()> {
        walk_address_sources(borrow, Some(path), |_, parent, when| {
            let first = output.as_ref().map_or(0, |regions| regions.len());
            parent.collect_memory_regions(&when, output.as_deref_mut())?;
            let (regions, end) = output.as_deref_mut().map_or((std::ptr::null_mut(), first), |regions| {
                let end = regions.len();
                (std::ptr::from_mut(regions).cast(), end)
            });
            let span = RegionSpan { regions, first, end };
            let host = RegionHost {
                part: projected_part,
                region: parent_region,
                emit: append_projected_region,
                finish: finish_parent_regions,
            };
            let parts = self.layout.parts.len().min(self.words.len());
            // An omitted result passes an empty parent span. During materialization
            // the original span stays at the same indices until projection returns.
            (unsafe { project_memory_regions(std::ptr::from_ref(self).cast(), parts, parent.layout.size, span, host) } != 0)
                .then_some(())
        })
    }
    pub fn map_borrows(&self, map: impl FnOnce(&view::Borrow)->view::Borrow) -> Self {
        let mut value=self.clone();
        match &mut value.root {
            Root::Resource{borrow,..} | Root::Derived(borrow) | Root::Dataset{borrow,..} | Root::External(Identity::Storage(borrow)) => *borrow=map(borrow),
            _=>{}
        }
        value
    }
    pub fn bind_outputs(&self, bindings: &bounds::OutputBindings) -> Self {
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
                Self::Value(ids.iter().flat_map(|id|match values.get(id){Some(proof)=>proof.origins.first().map(|row| row.to_vec()).unwrap_or_default(),None=>vec![*id]}).collect())
            },
        }
    }

    pub(super) fn output_bound(&self,bindings:&bounds::OutputBindings)->Self {
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
        if !matches!(a,Some(Self::Storage(_)))&&!matches!(b,Some(Self::Storage(_))){return None;}
        let mut a=match a {Some(Self::Storage(borrow))=>borrow.clone(),_=>view::Borrow::source(view::Source::External)};
        let external;
        let b=match b {Some(Self::Storage(borrow))=>borrow,_=>{external=view::Borrow::source(view::Source::External);&external}};
        Some(Self::Storage(match choice{Some(choice)=>a.selected(choice,b),None=>{a.union(b);a}}))
    }
}




#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum MemoryTarget { Value(Reg), Call(usize) }
enum CapturePositions<'a> {
    Binding(&'a [Position]),
    Element { borrow: &'a Arc<view::Borrow>, layout: &'a Layout },
}

impl CapturePositions<'_> {
    fn len(&self, width: usize) -> usize {
        match self {
            Self::Binding(positions) => positions.len(),
            Self::Element { .. } => width,
        }
    }

    fn at(&self, ty: &Ty, component: usize) -> Position {
        match self {
            Self::Binding(positions) => positions[component].clone(),
            Self::Element { borrow, layout } =>
                Position::Borrowed(Arc::clone(borrow), (**layout).clone(), ty.components[component].1),
        }
    }
}

#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum Capture { Read, Write }
// External仅表示没有静态来源；不能把它当作与本函数实际存储不相交。
#[derive(Default)]
pub(super) struct MemoryEffect {external:Option<Access>,known:Vec<(Arc<Origin>,bounds::Clause,Access,Option<bounds::Proof>)>}
pub(super) fn external_memory(borrow: &view::Borrow) -> bool {
    borrow.sources.entries().any(|(source, when)| {
        source_has_external_memory(source, || when.possible().then_some(&when))
    })
}

fn source_has_external_memory<'a>(source: &view::Source, when: impl FnOnce() -> Option<&'a bounds::Clause>) -> bool {
    match source {
        view::Source::External => when().is_some(),
        view::Source::Position(origin) => position_has_external_memory(origin, when),
        _ => false,
    }
}

fn position_has_external_memory<'a>(origin: &Origin, when: impl FnOnce() -> Option<&'a bounds::Clause>) -> bool {
    match &origin.root {
        Root::External(_) => when().is_some_and(|when| origin.collect_memory_regions(when, None).is_none()),
        Root::Derived(borrow) => external_memory(borrow) && when().is_some(),
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
        Self::from_sources(borrow.sources.entries(), bounds::Clause::default, access, || written)
    }
    pub(super) fn from_sources<'a, 'b>(
        sources: impl Iterator<Item = (&'a view::Source, bounds::Clause)>,
        path: impl FnOnce() -> bounds::Clause,
        access: Access,
        written: impl FnOnce() -> Option<&'b bounds::Proof>,
    ) -> Self {
        let mut effect = Self::default();
        let path = std::cell::LazyCell::new(path);
        let written = std::cell::LazyCell::new(written);
        for (source, when) in sources {
            let origin = match source {
                view::Source::Position(origin) => Some(origin),
                view::Source::External if effect.external.is_none() => None,
                _ => continue,
            };
            let when = bounds::Clause::both(&when, &path);
            if !when.possible() {
                continue;
            }
            if effect.external.is_none() && source_has_external_memory(source, || Some(&when)) {
                effect.external = Some(access);
            }
            if let Some(origin) = origin {
                let access = if origin.writable { access } else { Access::Read };
                let value = if access.writes() { written.cloned() } else { None };
                effect.known.push((origin.clone(), when, access, value));
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
        let regions = std::cell::LazyCell::new(|| origin.memory_regions(when));
        Self::project_written_bytes(origin, when, position, scalar, index, &|| regions.as_deref())
    }

    fn project_written_bytes<'a>(
        origin: &Origin, when: &bounds::Clause, position: &Position, scalar: Scalar,
        index: Option<bounds::Operand>, regions: &impl Fn() -> Option<&'a [MemoryRegion]>,
    ) -> u8 {
        if Self::same_element(origin,when,position,index)==Some(false){return 0;}
        let all=((1u16<<scalar.size())-1) as u8;
        let known = || regions().map_or(all, |regions| regions.iter().fold(0, |bytes, region| {
            bytes | region.written_bytes(position, scalar)
        }));
        match position{
            Position::Initialized=>0,
            Position::Local(_)=>known(),
            Position::Element(binding, _) => {
                let aliases = origin.aliases_when(&|alias| matches!(alias, MemoryAlias::Data(actual) if actual == binding), || true);
                if aliases.is_empty() { 0 } else { all }
            },
            Position::Borrowed(borrow,_,_) if matches!(&origin.root,
                Root::Resource{borrow:source,..}|Root::Dataset{borrow:source,..}if same_loop_resource(source,borrow))
                ||!borrow.pending()&&!borrow.unresolved()&&!external_memory(borrow)&&!borrow.sources.iter().any(|source|matches!(source,view::Source::Parameter(..)))=>known(),
            Position::Both(a, b) | Position::Conditional(_, a, b) =>
                Self::project_written_bytes(origin, when, a, scalar, index, regions)
                | Self::project_written_bytes(origin, when, b, scalar, index, regions),
            Position::Indexed(positions, _) => positions.iter().fold(0, |bytes, position| {
                bytes | Self::project_written_bytes(origin, when, position, scalar, index, regions)
            }),
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
            let effect = Self::from_sources(held.sources.entries(), || when.clone(), Access::Write, || None);
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
        position_has_external_memory(origin, || Some(when))
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
                let regions = std::cell::LazyCell::new(move || origin.memory_regions(when));
                let bytes = Self::project_written_bytes(origin, when, position, scalar, index,
                    &|| regions.as_deref());
                (bytes != 0).then_some((origin, written, bytes, regions))
            });
        let Some(first) = writes.next() else {
            return previous.clone();
        };
        let previous = previous.numeric_at_call(&BTreeMap::new());
        let mut result = previous.clone();
        let all = ((1u16 << scalar.size()) - 1) as u8;
        for (origin, written, bytes, regions) in std::iter::once(first).chain(writes) {
            let incoming=written.as_ref().and_then(|proof|{
                let mut incoming=None;
                for region in regions.as_deref()? {
                    let region_bytes = region.written_bytes(position, scalar);
                    if region_bytes == 0 { continue; }
                    let component=origin.layout.parts.iter().position(|part|*part==(scalar,region.word))?;
                    if region.offset!=0||region.size!=scalar.size()||region_bytes!=all{return None;}
                    let proof=proof.project_numeric_at_call(&[component],origin.layout.parts.len(),&BTreeMap::new());
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
    fn contains(&self, alias: MemoryAlias, access: Access) -> bool {
        self.known.iter().any(|known| known.alias == alias && known.access == access)
    }
    fn needs_external(&self, access: Access) -> bool {
        merged_external_access(self.external, Some(access)) != self.external
    }
    pub(super) fn record(&mut self, effect: &MemoryEffect) {
        for (origin, _, access, _) in &effect.known {
            let aliases = origin.aliases_when(&|alias| !self.contains(*alias, *access), || true);
            self.known.extend(aliases.into_iter().map(|alias| MemoryAccess { alias, access: *access }));
        }
        self.external = merged_external_access(self.external, effect.external);
    }
    pub(super) fn extend_sources<'a>(
        &mut self,
        sources: impl Iterator<Item = (&'a view::Source, bounds::Clause)>,
        path: impl FnOnce() -> bounds::Clause,
        access: Access,
    ) {
        let path = std::cell::LazyCell::new(path);
        for (source, when) in sources {
            let origin = match source {
                view::Source::Position(origin) => Some(origin),
                view::Source::External if self.needs_external(access) => None,
                _ => continue,
            };
            let when = std::cell::LazyCell::new(|| {
                let when = bounds::Clause::both(&when, &path);
                when.possible().then_some(when)
            });
            if self.needs_external(access) && source_has_external_memory(source, || when.as_ref()) {
                self.external = merged_external_access(self.external, Some(access));
            }
            if let Some(origin) = origin {
                let access = if origin.writable { access } else { Access::Read };
                let aliases = origin.aliases_when(&|alias| !self.contains(*alias, access), || when.is_some());
                self.known.extend(aliases.into_iter()
                    .map(|alias| MemoryAccess { alias, access }));
            }
        }
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

/// A write loan maps its local positions to the original caller input at contract closure.
#[derive(Clone)]
pub(super) struct DatasetOutput {
    scope: usize,
    input: ValueIdentity,
    layout: Layout,
}

impl DatasetOutput {
    pub(super) fn new(data: &application::Data) -> Option<Self> {
        let Some(Position::Parameter(scope, _)) = &data.initialization else { return None; };
        Some(Self { scope: *scope, input: data.root.identity(), layout: Layout::of(&data.ty) })
    }

    fn field(&self, index: Option<bounds::Operand>, word: u32) -> Position {
        let root = Root::Dataset {
            borrow: view::Borrow::source(view::Source::Parameter(self.input, Vec::new())),
            index,
        };
        let origin = Origin {
            root,
            layout: self.layout.clone(),
            base: self.layout.clone(),
            words: self.layout.parts.iter().map(|(_, word)| *word).collect(),
            positions: Arc::from([]),
            writable: true,
            initialized: vec![false; self.layout.parts.len()].into(),
        };
        let borrow = view::Borrow::source(view::Source::Position(Arc::new(origin)));
        Position::Borrowed(Arc::new(borrow), self.layout.clone(), word)
    }

    pub(super) fn project(&self, position: Position) -> Position {
        match position {
            Position::Parameter(scope, word) if scope == self.scope => self.field(None, word as u32),
            Position::ApplicationElement(scope, index, word) if scope == self.scope => {
                let index = match index {
                    ownership::ApplicationIndex::Constant(index) => bounds::Operand::Constant(Scalar::U32, Bits::from(index)),
                    ownership::ApplicationIndex::Value(id, _) => bounds::Operand::Value(id),
                };
                self.field(Some(index), word)
            }
            Position::Both(a, b) => Position::both(self.project(Arc::unwrap_or_clone(a)), self.project(Arc::unwrap_or_clone(b))),
            Position::Conditional(when, a, b) => Position::selected(
                Arc::unwrap_or_clone(when), self.project(Arc::unwrap_or_clone(a)), self.project(Arc::unwrap_or_clone(b)),
            ),
            Position::Indexed(choices, index) => Position::Indexed(
                choices.iter().cloned().map(|position| self.project(position)).collect(), index,
            ),
            position => position,
        }
    }
}

/// Reads retain the facts at their own use, before later calls or writes.
#[derive(Clone)]
pub(super) struct InitializationRead {
    pub positions: Vec<Position>,
    pub initialized: BTreeSet<Position>,
    pub dataset_output: Option<DatasetOutput>,
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
    fn bind_outputs(&self,bindings:&bounds::OutputBindings)->Self {
        match self {
            Self::Position(origin)=>Self::Position(origin.bind_outputs(bindings)),
            Self::Task(borrow)=>Self::Task(borrow.output_bound(bindings)),
        }
    }
}

/// A normal-return guarantee names actual initialized locations.
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Initialization { pub source:InitializationSource,pub when:bounds::Condition }


#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct InitializationState {
    pub initialized: BTreeSet<Position>,
    pub path: bounds::Condition,
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
        Self::Iteration{iteration,..}=>iteration.states.iter().any(|state| has_pending_initialization(&state.initialized)),
    }}
    pub fn output_bound(&self,bindings:&bounds::OutputBindings)->Self {
        match self {
            Self::Position(fact)=>Self::Position(Initialization{source:fact.source.bind_outputs(bindings),when:fact.when.output_bound(bindings)}),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.output_bound(bindings)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.output_bound(bindings)},
        }
    }
    fn under(&self,condition:bounds::Condition)->Self {
        match self {
            Self::Position(fact)=>Self::Position(fact.under(condition)),
            Self::Call{call,when}=>Self::Call{call:*call,when:when.clone().logical(condition,false)},
            Self::Iteration{iteration,when}=>Self::Iteration{iteration:iteration.clone(),when:when.clone().logical(condition,false)},
        }
    }
}
impl Initialization {
    fn under(&self, condition: bounds::Condition) -> Self {
        Self { source: self.source.clone(), when: self.when.clone().logical(condition, false) }
    }

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
    pub fn output_bound(&self,bindings:&bounds::OutputBindings)->Self {
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
fn initialization_borrow(borrow: &view::Borrow, when: &bounds::Condition) -> view::Borrow {
    borrow_in_path(borrow, &when.assume(&bounds::Clause::default(), true))
}

fn borrowed_initialization(
    borrow: &view::Borrow, layout: &Layout, word: u32, when: &bounds::Condition,
) -> Initialization {
    let (scalar, _) = layout.parts.iter().find(|(_, offset)| *offset == word).unwrap();
    let part = Layout { size: scalar.size(), align: scalar.size(), parts: Arc::from([(*scalar, 0)]) };
    let origin = Origin {
        root: Root::Derived(initialization_borrow(borrow, when)),
        layout: part,
        base: layout.clone(),
        words: Arc::from([word]),
        positions: Arc::from([]),
        writable: true,
        initialized: Arc::from([true]),
    };
    Initialization { source: InitializationSource::Position(origin), when: when.clone() }
}

pub(super) fn initialization_effects(
    initialized: &BTreeSet<Position>, path: &bounds::Clause,
) -> Vec<InitializationEffect> {
    let mut result = Vec::new();
    Position::visit_initialization_effects(initialized, path, |position, when| {
        match position {
            Position::Borrowed(borrow, layout, word) => result.push(InitializationEffect::Position(
                borrowed_initialization(borrow, layout, *word, when),
            )),
            Position::Initialization(effect) => result.push(effect.under(when.clone())),
            _ => {}
        }
        false
    });
    result
}

pub(super) fn has_pending_initialization(initialized: &BTreeSet<Position>) -> bool {
    Position::visit_initialization_effects(initialized, &bounds::Clause::default(), |position, when| {
        match position {
            Position::Borrowed(borrow, ..) => {
                borrow.pending_in_path(&when.assume(&bounds::Clause::default(), true))
            }
            Position::Initialization(effect) => effect.pending(),
            _ => false,
        }
    })
}

pub(super) fn initialized_by_effects(
    position: &Position, initialized: &BTreeSet<Position>, path: &bounds::Clause,
) -> bool {
    let mut facts = Vec::new();
    Position::visit_initialization_effects(initialized, &bounds::Clause::default(), |position, when| {
        match position {
            Position::Borrowed(borrow, layout, word) => facts.push(borrowed_initialization(borrow, layout, *word, when)),
            Position::Initialization(effect) => {
                if let InitializationEffect::Position(fact) = effect.as_ref() {
                    facts.push(fact.under(when.clone()));
                }
            }
            _ => {}
        }
        false
    });
    let facts = merge_initializations(facts);
    let known = facts.iter().filter(|fact| fact.when.truth_in(path) == Some(true))
        .flat_map(|fact| fact.positions(path)).collect();
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
            Self::Borrowed{borrow,..}=>IndexedSource::Pointer{address,index,stride:0,aliases:borrow.aliases(&|_|true)},
            Self::External{..}=>IndexedSource::Pointer{address,index,stride:0,aliases:Vec::new()},
            _=>IndexedSource::View{address,index,stride:ElementStride::Constant(0)},
        };
        (source,word)
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
pub(super) fn resource_metadata(ty:&Ty,resources:Vec<Vec<usize>>)->Vec<std::ops::Range<u32>> {
    resources.into_iter().map(|fields|{
        let(mut offset,mut ty)=(0,ty);
        for field in fields{let(at,next)=if ty.has_variants(){(ty.variant_payload_offset(),ty.variants[field].ty.as_ref())}else{ty.aggregate_part(field)};offset+=at;ty=next;}
        offset..offset+ty.size
    }).collect()
}
impl Builder<'_> {
    pub(super) fn address_write_positions(&self, borrow: &view::Borrow) -> Option<Vec<Position>> {
        let mut positions = Vec::new();
        Self::visit_address_writes(borrow, |origin, when| {
            if origin.writable {
                positions.extend(origin.memory_regions(&when)?.into_iter().map(|region| region.position));
            }
            Some(())
        })?;
        Some(positions)
    }

    pub(super) fn visit_address_writes<'a>(
        borrow: &'a view::Borrow,
        mut visit: impl FnMut(&'a Arc<Origin>, bounds::Clause) -> Option<()>,
    ) -> Option<()> {
        walk_address_sources(borrow, None, |source, origin, when| {
            if view::Borrow::source_unresolved(source, &|| Some(&when)) {
                return None;
            }
            visit(origin, when)
        })
    }

    pub(super) fn resolved_address_write(&self, borrow: &view::Borrow) -> bool {
        Self::visit_address_writes(borrow, |origin, when| {
            if origin.writable { origin.collect_memory_regions(&when, None)?; }
            Some(())
        }).is_some()
    }

    pub(super) fn address_write_effect(borrow: &view::Borrow) -> Option<MemoryEffect> {
        let mut effect = MemoryEffect::default();
        Self::visit_address_writes(borrow, |origin, when| {
            if origin.writable { origin.collect_memory_regions(&when, None)?; }
            let external = effect.external.is_none() && match &origin.root {
                // Writable external positions already resolved their regions above.
                Root::External(_) if origin.writable => false,
                _ => position_has_external_memory(origin, || Some(&when)),
            };
            if external {
                effect.external = Some(Access::Write);
            }
            if origin.writable {
                effect.known.push((origin.clone(), when, Access::Write, None));
            }
            Some(())
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
    pub(super) fn memory_effect_components(
        &self, effect: Option<&MemoryEffect>, place: &Place, outside_element: bool,
    ) -> Vec<usize> {
        if place.immutable_local() {
            return Vec::new();
        }
        let Some(effect) = effect else {
            return (0..place.ty().components.len()).collect();
        };
        let (positions, index) = self.value_memory_location(place);
        let written = if outside_element { MemoryEffect::outside_element_bytes } else { MemoryEffect::written_bytes };
        positions.iter().zip(&place.ty().components).enumerate().filter_map(|(component, (position, (scalar, _)))| {
            (written(effect, position, *scalar, index) != 0).then_some(component)
        }).collect()
    }
    pub(super) fn address_write_components(&self,positions:&[Position],place:&Place)->Vec<usize> {
        self.positions(place).iter().enumerate().filter_map(|(i,position)|positions.iter().any(|written|
            owned_presence::Presence::selected(written,position)!=owned_presence::Presence::None).then_some(i)).collect()
    }
    pub(super) async fn update_address_elements(&mut self,borrow:&view::Borrow,effect:&MemoryEffect)->Result<()> {
        let data=borrow.sources.entries().filter_map(|(source,when)|{
            let view::Source::Position(origin)=source else{return None;};
            if !origin.writable{return None;}
            match &origin.root {
                Root::Resource{borrow:resource,index}|Root::Dataset{borrow:resource,index} if when.possible()=>Some((resource.restricted(&when),origin.words.to_vec(),index.map(|index|{
                    let range=match index{bounds::Operand::Constant(_,value)=>(value as u32,value as u32),_ =>(0,u32::MAX)};(index,range)
                }))),
                Root::External(Identity::Storage(resource)) if when.possible()=>Some((resource.resources().restricted(&when),resource.sources.iter().filter_map(|source|match source{view::Source::Position(parent)=>Some(parent.words.iter().copied()),_=>None}).flatten().collect(),None)),
                _=>None,
            }
        }).collect::<Vec<_>>();
        for(resource,words,index)in data {unsafe { self.world.call(self.update_application_elements(&[],&resource,&words,application::ElementUpdate::Bytes{effect,index})).await }?;}
        Ok(())
    }
    pub(super) async fn write_address_values(&mut self,borrow:&view::Borrow,memory:&[(view::Borrow,Option<Access>,Option<bounds::Proof>)])->Result<()> {
        let borrow=self.current_address_borrow(borrow);let mut effect=MemoryEffect::default();
        for(source,access,written)in memory{if let Some(access)=access.filter(|access|access.writes()){
            let source=self.current_address_borrow(source);effect.union(MemoryEffect::of(&source,access,written.as_ref()));
        }}
        if !self.resolved_address_write(&borrow) {
        let data=self.env.values().flat_map(Binding::application_resources)
            .map(|data|application::DataReference::direct(data.clone())).collect::<Vec<_>>();
        for data in data {unsafe { self.world.call(self.update_application_contents(&data,&data.words(),application::ElementUpdate::Unknown)).await }?;}
        for binding in self.env.values_mut() {
            let metadata=match &*binding{Binding::OwnedValue(owner)=>resource_metadata(&owner.value.ty,view::resource_paths(&owner.value.ty).into_iter().filter(|fields|view::owned_data_at(owner,fields).is_some()).collect()),_=>Vec::new()};
            if let Some((proof,uniform,owner))=binding.proof_storage() && !owner.immutable_local() {
                let components=owner.ty().components.iter().enumerate().filter_map(|(i,(_,word))|(!metadata.iter().any(|range|range.contains(word))).then_some(i)).collect::<Vec<_>>();
                proof.forget_value(&components,owner.ty().components.len());
                *proof=std::mem::take(proof).with_external_borrows(owner.ty());
                *uniform=false;
                if let Binding::OwnedValue(owner)=binding {owner.value.constant=None;}
            }
        }
        }else{
        unsafe { self.world.call(self.update_address_elements(&borrow,&effect)).await }?;
        let selected=self.env.iter().filter_map(|(name,binding)|{
            let place=binding.stored_place()?;
            if place.immutable_local() { return None; }
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
    pub(super) async fn position_origin(&self,place:&Place,span:Span)->Result<Origin> {
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
            Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage,_)=>Root::Local{base:storage.start,len:storage.len()},
            Place::Reference{address:ReferenceAddress::Borrowed{borrow,layout:parent,..},..}=>{
                base=parent.clone();words=external::reference_words(place,&self.locals).unwrap();Root::Derived(borrow.clone())
            }
            Place::Reference{address:ReferenceAddress::External{identity,..},..}=>{words=external::reference_words(place,&self.locals).unwrap();Root::External(identity.clone())},
            Place::Reference{scope:Some(parameter),..}=>{
                let ty=unsafe { self.world.call(cooperative::parameter_type(self.world,&unsafe { self.world.call(self.scope.parameters().nth(*parameter).unwrap().bound(self.world)).await }?,(&self.scope.function().unwrap().definition.source,&self.scope.function().unwrap().definition.module,&self.scope.function().unwrap().definition.imports,self.scope.function().unwrap().definition.function.kind()),self.program.target)).await }?;
                base=Layout::of(&ty);words=external::reference_words(place,&self.locals).unwrap();Root::Parameter(*parameter)
            }
            Place::ApplicationElement{resource,element_layout,words:actual,index_value,..}=>{
                base=element_layout.clone();words=actual.clone();
                Root::Resource{borrow:resource.clone(),index:Some(*index_value)}
            },
            Place::Element{binding,..}=>Root::Buffer(*binding),
            Place::ViewElement{resource,element_layout,words:actual,..}=>{
                base=element_layout.clone();words=actual.clone();
                Root::Resource{borrow:resource.clone(),index:None}
            }
            _=>return self.fail(span,"B264","address_of需要CPU普通值的实际连续位置"),
        };
        let positions=self.positions(place);
        let initialized=if positions.len()==layout.parts.len(){
            positions.iter().map(|position|self.position_initialized_in(position,&self.initialized,&self.env,None)).collect::<Vec<_>>()
        }else{vec![self.is_initialized(place);layout.parts.len()]};
        Ok(Origin{root,layout,base,words:words.into(),positions:positions.into(),writable,initialized:initialized.into()})
    }
    pub(super) async fn position_borrows(&self,place:&Place,span:Span)->Result<view::Borrow> {
        let origin=unsafe { self.world.call(self.position_origin(place,span)).await }?;
        if let Place::Indexed {base,offset,words,..}=place
            &&let Place::Reference {address:ReferenceAddress::Borrowed {..}|ReferenceAddress::External {..},words:base_words,..}=&**base {
            let mut result=view::Borrow::default();
            let index=ownership::ApplicationIndex::Value(ValueIdentity::Register(*offset),*offset);
            let (first_scalar,_)=place.ty().components[0];
            let parts=base_words.iter().zip(&base.ty().components).map(|(word,(scalar,_))|(u64::from(*word),*scalar)).collect::<BTreeSet<_>>();
            for ((scalar,_),at) in base.ty().components.iter().zip(base_words) {
                if *scalar!=first_scalar||*at<words[0]{continue;}
                let relative=*at-words[0];
                if !words.iter().zip(&place.ty().components).all(|(word,(scalar,_))|
                    parts.contains(&(u64::from(*word)+u64::from(relative),*scalar))) {continue;}
                let when=bounds::Clause::both(&self.path,&bounds::Clause::selection(index,relative as usize));
                if !when.possible(){continue;}
                let mut selected=origin.clone();
                selected.words=words.iter().map(|word|word+relative).collect();
                result.union(&selected.normalize().restricted(&when));
            }
            return Ok(result);
        }
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
                    let (Position::Borrowed(_,_,word)|Position::ViewElement(_,_,_,word))=position else{unreachable!()};
                    *word
                }).collect();
                out.sources.insert_when(view::Source::Position(Arc::new(origin)),when.clone());
            }
        }
        let mut borrow=view::Borrow::default();
        collect(&origin,&positions,&self.path,&mut borrow);
        Ok(borrow)
    }
    pub(super) async fn reference_memory(&self,place:&Place,span:Span)->Result<Option<(MemoryTarget,view::Borrow)>> {
        if let Place::Reference{address:ReferenceAddress::Borrowed{address,..}|ReferenceAddress::External{address,..},..}=place.storage(){
            let borrow=if matches!(place,Place::Indexed {..}) {
                unsafe {self.world.call(self.position_borrows(place,span)).await}?
            }else{unsafe {self.world.call(self.position_origin(place,span)).await}?.normalize()};
            return Ok(Some((MemoryTarget::Value(address[0]),borrow)));
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
                result.set_view_borrow(component,self.current_address_borrow(&proof.view_borrow(component)));
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
        let owners = self.env.iter().filter_map(|(name, binding)| {
            let place = binding.stored_place()?;
            if capture_kind == Capture::Write && place.immutable_local() {
                return None;
            }
            let (positions, index) = self.value_memory_location(&place);
            Some((name.clone(), positions, index))
        }).collect::<Vec<_>>();
        let writing = capture_kind == Capture::Write;
        let mut snapshots = Vec::new();
        let mut capture = |proof: &mut bounds::Proof, uniform: &mut bool, ty: &Ty,
            positions: CapturePositions<'_>, index: Option<bounds::Operand>, resources: Vec<Vec<usize>>| {
            let width = ty.components.len();
            let columns = ty.snapshot_columns(positions.len(width), writing, &resource_metadata(ty, resources));
            let previous = writing.then(|| proof.clone());
            if writing {
                let affected: Vec<_> = columns.iter().enumerate()
                    .filter_map(|(component, column)| column.captured().then_some(component)).collect();
                proof.forget_value(&affected, width);
            }
            for (component, column) in columns.into_iter().enumerate() {
                if !column.captured() { continue; }
                let snapshot = snapshots.len();
                let initial = Arc::new(previous.as_ref().unwrap_or(proof).project(&[component], width));
                let position = positions.at(ty, component);
                let read = (writing && column.address()).then(|| view_contracts::ContentRead::Snapshot {
                    memory, component: snapshot, position: position.clone(), index, initial: initial.clone(),
                });
                snapshots.push(view_contracts::ValueSnapshot { position, index, scalar: ty.components[component].0, proof: initial });
                if writing {
                    let identity = ValueIdentity::MemoryValue(memory, snapshot);
                    *proof = std::mem::take(proof).with_component_identity(component, identity);
                    if let Some(read) = read {
                        proof.set_view_borrow(component, view::Borrow { reads: view::Edges::from([read]), ..view::Borrow::default() });
                    }
                }
            }
            if writing { *uniform = false; }
        };
        for(name,positions,index)in owners {
            let binding=self.env.get_mut(&name).unwrap();
            let resources=match &*binding{Binding::OwnedValue(owner)=>view::resource_paths(&owner.value.ty).into_iter().filter(|fields|view::owned_data_at(owner,fields).is_some()).collect(),_=>Vec::new()};
            let Some((proof,uniform,owner))=binding.proof_storage()else{continue;};
            capture(proof,uniform,owner.ty(),CapturePositions::Binding(&positions), index, resources);
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
                    let positions = CapturePositions::Element { borrow: &borrow, layout: &layout };
                    value.proof = bounds::Proof::capture(value);
                    capture(&mut value.proof, &mut value.uniform, &value.ty, positions,
                        Some(bounds::Operand::Value(ValueIdentity::ElementIndex(0))), resources);
                    if writing { value.constant = None; value.proof = bounds::Proof::capture(value); }
                }
            }
        });}
        snapshots
    }
    pub(super) async fn call_address_memory(&mut self,memory:impl IntoIterator<Item=(view::Borrow,Option<Access>)>,span:Span)->Result<usize> {
        let usage=self.view_model.uses.len();let mut written=view::Borrow::default();
        for(borrow,access)in memory{
            if access.is_some_and(|access|access.writes()){written.union(&borrow);}
            self.check_view_requirement(view::Requirement::Memory{target:MemoryTarget::Call(usage),borrow,access,written:None,snapshots:None,span,address_fields:Arc::from([])})?;
        }
        if written.pending()&&!self.resolved_address_write(&written){
            let snapshots=self.capture_address_values(usage,address::Capture::Write);
            if let view::Requirement::Memory{snapshots:target,..}=&mut self.view_model.uses[usage].requirement{*target=Some(snapshots.into());}
        }else{let memory=[(written,Some(Access::Write),None)];unsafe { self.world.call(self.write_address_values(&memory[0].0,&memory)).await }?;}
        Ok(usage)
    }
    pub(super) async fn position_address(&mut self,args:&Expressions,span:Span)->Result<Option<Value>>{
        let mut cursor=AddressPhase{phase:0};let mut facts=TakeAddressFacts{cpu:u32::from(self.program.target==Target::Cpu),arguments:args.len()as u64,..TakeAddressFacts::default()};
        let mut place:Option<Place>=None;let mut origin:Option<Origin>=None;let mut address=None;let mut dst=None;let mut ty=None;let mut numeric=None;let mut identity=None;let mut proof=None;
        loop{let step=unsafe{take_address_step(cursor,TakeAddressFacts{..facts})};cursor=step.cursor;match step.need{
            TakeAddressNeed::BadTarget=>return self.fail(span,"B265","address_of只取得本进程CPU位置"),
            TakeAddressNeed::BadCount=>return self.fail(span,"B153","address_of需要一个实际位置"),
            TakeAddressNeed::Place=>{place=unsafe{self.world.call(self.place(&args.at(0))).await}?;facts.present=u32::from(place.is_some());},
            TakeAddressNeed::Empty=>return Ok(None),
            TakeAddressNeed::Layout=>{let ty=place.as_ref().unwrap().ty();facts.owns=u32::from(ty.owns_resources());facts.views=u32::from(ty.contains_views());facts.empty=u32::from(ty.components.is_empty());},
            TakeAddressNeed::BadLayout=>return self.fail(span,"B263","address_of需要非空普通值位置，资源通过原拥有者或借用接口交接"),
            TakeAddressNeed::Origin=>{origin=Some(unsafe{self.world.call(self.position_origin(place.as_ref().unwrap(),span)).await}?);facts.writable=u32::from(origin.as_ref().unwrap().writable);},
            TakeAddressNeed::Reference=>address=Some(unsafe{self.world.call(self.external_reference(place.as_ref().unwrap(),step.access,step.access.writes(),span)).await}?),
            TakeAddressNeed::Register=>{dst=Some(self.reg(Scalar::U64));ty=Some(Ty::builtin("address").unwrap());},
            TakeAddressNeed::Storage=>facts.storage=match place.as_ref().unwrap().storage(){Place::Local(..)|Place::ReadOnlyLocal(..)=>1,Place::Reference{address:ReferenceAddress::External{..}|ReferenceAddress::Borrowed{..},..}=>2,_=>0},
            TakeAddressNeed::LocalOrigin=>{let(Place::Local(_,_,storage)|Place::ReadOnlyLocal(_,_,storage,_))=place.as_ref().unwrap().storage()else{unsafe{std::hint::unreachable_unchecked()}};let ty=self.locals[&storage.end].clone();let whole=Place::Local(ty,storage.clone().collect(),storage.clone());numeric=Some(unsafe{self.world.call(self.position_origin(&whole,span)).await}?);},
            TakeAddressNeed::ReferenceIdentity=>{let Place::Reference{address:ReferenceAddress::External{identity:source,..}|ReferenceAddress::Borrowed{identity:source,..},..}=place.as_ref().unwrap().storage()else{unsafe{std::hint::unreachable_unchecked()}};identity=Some(source.clone());},
            TakeAddressNeed::CopyOrigin=>numeric=origin.clone(),
            TakeAddressNeed::Tail=>project_address_tail(numeric.as_mut().unwrap(),address.as_ref().unwrap().word,place.as_ref().unwrap().ty().align),
            TakeAddressNeed::StorageIdentity=>identity=Some(Identity::Storage(view::Borrow::source(view::Source::Position(Arc::new(numeric.take().unwrap()))))),
            TakeAddressNeed::Emit=>{let mut component=bounds::Proof::default();component.addresses.push(identity.take());proof=Some(component);self.emit(InstKind::PositionAddress{dst:dst.unwrap(),address:address.take().unwrap()});},
            TakeAddressNeed::Complete=>{let borrow=view::Borrow::source(view::Source::Position(Arc::new(origin.take().unwrap())));let mut proof=proof.take().unwrap();proof.set_view_borrow(0,borrow);return Ok(Some(Value{ty:ty.take().unwrap(),regs:vec![dst.unwrap()],proof,literal:false,uniform:false,constant:None}));},
        }}
    }

    pub(super) async fn external_position(&mut self,explicit:&[GenericArgument],args:&Expressions,span:Span)->Result<Option<Place>> {
        unsafe { self.world.call(self.external_position_value(explicit,args.len(),span,args.first().map(|arg|arg.span()).unwrap_or(span),
            async |builder|unsafe { builder.world.call(builder.expr(&args.at(0))).await })).await }
    }
    pub(super) async fn external_position_value(&mut self,explicit:&[GenericArgument],count:usize,span:Span,argument_span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Place>>{
        let mut external_atomic = false;
        let mut cursor=AddressPhase{phase:0};let mut facts=ExternalPositionFacts{cpu:u32::from(self.program.target==Target::Cpu),explicit_type:match explicit{[GenericArgument::Type(name)]=>if name.dataset{2}else{1},_=>0},arguments:count as u64,..ExternalPositionFacts::default()};
        let mut argument=Some(argument);let mut ty:Option<Ty>=None;let mut value=None;let mut borrow:Option<view::Borrow>=None;let mut identity=None;let mut bits=None;let mut low=None;let mut high=None;
        let external=loop{let step=unsafe{external_position_step(cursor,ExternalPositionFacts{..facts})};cursor=step.cursor;match step.need{
            ExternalPositionNeed::BadTarget=>return self.fail(span,"B265","unsafe_position只解释本进程CPU地址"),
            ExternalPositionNeed::BadArguments=>return self.fail(span,"B153","unsafe_position<T>需要一个明确普通类型和一个address实参"),
            ExternalPositionNeed::Type=>{let[GenericArgument::Type(name)]=explicit else{unsafe{std::hint::unreachable_unchecked()}};let name = if name.name == "atomic" && !name.canonical && !name.dataset && !name.view && !name.tile
                    && name.count.is_none() && name.contracts.is_none() {
                    let [GenericArgument::Type(scalar)] = name.arguments.as_slice() else {
                        return self.fail(span,"B215","外部原子位置需要 atomic<T> 中的一个明确标量类型");
                    };
                    external_atomic = true;
                    scalar
                } else {
                    name
                };
                let actual = unsafe { self.world.call(self.world.ty(name,self.scope.source(),span,self.scope.module(),self.scope.imports())).await }?;
                if external_atomic {
                    self.require_atomic_type(&actual, span)?;
                }
                facts.owns=u32::from(actual.owns_resources());facts.views=u32::from(actual.contains_views());facts.empty=u32::from(actual.components.is_empty());ty=Some(actual);},
            ExternalPositionNeed::Owned=>return self.fail(span,"B224","外部地址位置不能创建Blink拥有资源"),
            ExternalPositionNeed::BadLayout=>return self.fail(span,"B263","外部地址位置需要非空普通值布局，不能解释Blink资源引用"),
            ExternalPositionNeed::Argument=>{value=argument.take().unwrap()(self).await?;facts.present=u32::from(value.is_some());},
            ExternalPositionNeed::Empty=>return Ok(None),
            ExternalPositionNeed::RequireAddress=>self.require_same(&Ty::builtin("address").unwrap(),&value.as_ref().unwrap().ty,argument_span)?,
            ExternalPositionNeed::Borrow=>borrow=Some(borrow_in_path(&self.current_address_borrow(&value.as_ref().unwrap().proof.view_borrow(0)),&self.path)),
            ExternalPositionNeed::Requirement=>self.check_view_requirement(view::Requirement::Address{borrow:borrow.as_ref().unwrap().clone(),layout:Layout::of(ty.as_ref().unwrap()),written:None,writable:false,initialized:false,external_atomic,span})?,
            ExternalPositionNeed::BorrowKind=>{let source=borrow.as_ref().unwrap();facts.external=u32::from(data_sources_external(source.pending(),source.sources.iter()));},
            ExternalPositionNeed::Identity=>{identity=value.as_ref().unwrap().proof.addresses.first().cloned().flatten();facts.identity=u32::from(identity.is_some());},
            ExternalPositionNeed::KnownBits=>{let value=value.as_ref().unwrap();bits=bounds::Proof::capture(value).known_value(&value.ty);facts.constant=u32::from(bits.is_some());},
            ExternalPositionNeed::ConstantIdentity=>identity=Some(Identity::Constant(bits.take().unwrap()[0])),
            ExternalPositionNeed::DynamicIdentity=>identity=Some(Identity::Value(value.as_ref().unwrap().component_origins(0).to_vec())),
            ExternalPositionNeed::Reword=>value.as_mut().unwrap().ty=Ty::scalar(Scalar::U64),
            ExternalPositionNeed::Low=>low=Some(self.cast(value.as_ref().unwrap().clone(),Scalar::U32).regs[0]),
            ExternalPositionNeed::High=>{let shift=self.constant(Scalar::U64,32);let shifted=self.binary(">>",value.take().unwrap(),shift,span,None)?;high=Some(self.cast(shifted,Scalar::U32).regs[0]);},
            ExternalPositionNeed::ReadyExternal=>break true,
            ExternalPositionNeed::ReadyBorrowed=>break false,
        }};
        let ty=ty.take().unwrap();let words=ty.components.iter().map(|(_,offset)|*offset).collect();let identity=identity.take().unwrap();let address=[low.unwrap(),high.unwrap()];
        let address=if external{ReferenceAddress::External{address,identity}}else{ReferenceAddress::Borrowed{address,identity,borrow:borrow.take().unwrap(),layout:Layout::of(&ty)}};
        Ok(Some(Place::Reference{words,ty,address,scope:None,access:if external_atomic { Access::Atomic } else { Access::InOut }}))
    }
}


#[repr(C)]#[derive(Clone,Copy)]struct AddressPhase{phase:u32}
#[repr(C)]#[derive(Default)]struct TakeAddressFacts{cpu:u32,arguments:u64,present:u32,owns:u32,views:u32,empty:u32,writable:u32,storage:u32}
#[repr(u32)]enum TakeAddressNeed{BadTarget,BadCount,Place,Empty,Layout,BadLayout,Origin,Reference,Register,Storage,LocalOrigin,ReferenceIdentity,CopyOrigin,Tail,StorageIdentity,Emit,Complete}
#[repr(C)]struct TakeAddressStep{cursor:AddressPhase,need:TakeAddressNeed,access:Access}
#[repr(C)]struct AddressTailHost{offset:unsafe extern "C" fn(*mut std::ffi::c_void,u64)->u32,append:unsafe extern "C" fn(*mut std::ffi::c_void,u64,u32)}
#[repr(C)]#[derive(Default)]struct ExternalPositionFacts{cpu:u32,explicit_type:u32,arguments:u64,owns:u32,views:u32,empty:u32,present:u32,external:u32,identity:u32,constant:u32}
#[repr(u32)]enum ExternalPositionNeed{BadTarget,BadArguments,Type,Owned,BadLayout,Argument,Empty,RequireAddress,Borrow,Requirement,BorrowKind,Identity,KnownBits,ConstantIdentity,DynamicIdentity,Reword,Low,High,ReadyExternal,ReadyBorrowed}
#[repr(C)]struct ExternalPositionStep{cursor:AddressPhase,need:ExternalPositionNeed}
unsafe extern "C"{
 #[link_name="bootstrap.call_arguments::take_address_step"]fn take_address_step(cursor:AddressPhase,facts:TakeAddressFacts)->TakeAddressStep;
 #[link_name="bootstrap.call_arguments::address_tail"]fn address_tail(context:*mut std::ffi::c_void,count:u64,size:u32,first:u32,host:AddressTailHost)->u32;
 #[link_name="bootstrap.call_arguments::external_position_step"]fn external_position_step(cursor:AddressPhase,facts:ExternalPositionFacts)->ExternalPositionStep;
 #[link_name="bootstrap.call_arguments::data_external_sources"]fn b_data_external_sources(pending:u32,context:*mut std::ffi::c_void,next:unsafe extern "C" fn(*mut std::ffi::c_void)->u32)->u32;
}
struct AddressTail<'a>{source:&'a Origin,parts:Vec<(Scalar,u32)>,positions:Vec<Position>,initialized:Vec<bool>,words:Vec<u32>}
unsafe extern "C" fn tail_offset(context:*mut std::ffi::c_void,index:u64)->u32{unsafe{&*context.cast::<AddressTail<'_>>()}.source.layout.parts[index as usize].1}
unsafe extern "C" fn tail_append(context:*mut std::ffi::c_void,index:u64,offset:u32){let tail=unsafe{&mut *context.cast::<AddressTail<'_>>()};let i=index as usize;tail.parts.push((tail.source.layout.parts[i].0,offset));tail.positions.push(tail.source.positions[i].clone());tail.initialized.push(tail.source.initialized[i]);tail.words.push(offset);}
fn project_address_tail(source:&mut Origin,first:u32,align:u32){
 let mut tail=AddressTail{source,parts:Vec::new(),positions:Vec::new(),initialized:Vec::new(),words:Vec::new()};
 let size=unsafe{address_tail(std::ptr::from_mut(&mut tail).cast(),source.layout.parts.len()as u64,source.layout.size,first,AddressTailHost{offset:tail_offset,append:tail_append})};
 let AddressTail{parts,positions,initialized,words,..}=tail;source.layout=Layout{size,align,parts:parts.into()};source.positions=positions.into();source.initialized=initialized.into();source.words=words.into();source.base=source.layout.clone();
}
unsafe extern "C" fn data_source_next<'a,I:Iterator<Item=&'a view::Source>>(context:*mut std::ffi::c_void)->u32{match unsafe{&mut *context.cast::<I>()}.next(){None=>0,Some(view::Source::External)=>1,Some(_)=>2}}
fn data_sources_external<'a,I:Iterator<Item=&'a view::Source>>(pending:bool,mut sources:I)->bool{unsafe{b_data_external_sources(u32::from(pending),std::ptr::from_mut(&mut sources).cast(),data_source_next::<I>)!=0}}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Layout);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        size: Slot,
        align: Slot,
        parts: Slot,
    }
    #[repr(C)]
    struct Writer {
        size: WriteCallback,
        align: WriteCallback,
        parts: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        size: ReadCallback,
        align: ReadCallback,
        parts: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_address_layout"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_address_layout"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Layout {
        fn empty() -> Self {
            Self {
                size: <u32 as Native>::empty(),
                align: <u32 as Native>::empty(),
                parts: <Arc<[(Scalar, u32)]> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        size: contracts::address(&self.size).cast_mut(),
                        align: contracts::address(&self.align).cast_mut(),
                        parts: contracts::address(&self.parts).cast_mut(),
                    },
                    Writer {
                        size: contracts::write_value::<u32>,
                        align: contracts::write_value::<u32>,
                        parts: contracts::write_value::<Arc<[(Scalar, u32)]>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        size: contracts::slot(&mut output.size),
                        align: contracts::slot(&mut output.align),
                        parts: contracts::slot(&mut output.parts),
                    },
                    Reader {
                        size: contracts::read_value::<u32>,
                        align: contracts::read_value::<u32>,
                        parts: contracts::read_value::<Arc<[(Scalar, u32)]>>,
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

crate::codec::contracts::codec!(Root);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        local: ReadCallback,
        parameter: ReadCallback,
        resource: ReadCallback,
        buffer: ReadCallback,
        derived: ReadCallback,
        external: ReadCallback,
        dataset: ReadCallback,
    }
    unsafe extern "C" fn build_local(context: Slot, destination: Slot) -> ReadStatus {
        let mut base = <usize as Native>::empty();
        let mut len = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            base: Slot,
            len: Slot,
        }
        #[repr(C)]
        struct Host {
            base: ReadCallback,
            len: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_local"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    base: contracts::slot(&mut base),
                    len: contracts::slot(&mut len),
                },
                Host {
                    base: contracts::read_value::<usize>,
                    len: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Local { base, len };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_parameter(context: Slot, destination: Slot) -> ReadStatus {
        let mut index = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_parameter"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    index: contracts::slot(&mut index),
                },
                Host {
                    index: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Parameter(index);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_resource(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrow = <view::Borrow as Native>::empty();
        let mut index = <Option<bounds::Operand> as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrow: Slot,
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            borrow: ReadCallback,
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_resource"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrow: contracts::slot(&mut borrow),
                    index: contracts::slot(&mut index),
                },
                Host {
                    borrow: contracts::read_value::<view::Borrow>,
                    index: contracts::read_value::<Option<bounds::Operand>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Resource { borrow, index };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_buffer(context: Slot, destination: Slot) -> ReadStatus {
        let mut binding = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            binding: Slot,
        }
        #[repr(C)]
        struct Host {
            binding: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_buffer"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    binding: contracts::slot(&mut binding),
                },
                Host {
                    binding: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Buffer(binding);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_derived(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrow = <view::Borrow as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrow: Slot,
        }
        #[repr(C)]
        struct Host {
            borrow: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_derived"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrow: contracts::slot(&mut borrow),
                },
                Host {
                    borrow: contracts::read_value::<view::Borrow>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Derived(borrow);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_external(context: Slot, destination: Slot) -> ReadStatus {
        let mut identity = <Identity as Native>::empty();
        #[repr(C)]
        struct Fields {
            identity: Slot,
        }
        #[repr(C)]
        struct Host {
            identity: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_external"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    identity: contracts::slot(&mut identity),
                },
                Host {
                    identity: contracts::read_value::<Identity>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::External(identity);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_dataset(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrow = <view::Borrow as Native>::empty();
        let mut index = <Option<bounds::Operand> as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrow: Slot,
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            borrow: ReadCallback,
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_root_dataset"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrow: contracts::slot(&mut borrow),
                    index: contracts::slot(&mut index),
                },
                Host {
                    borrow: contracts::read_value::<view::Borrow>,
                    index: contracts::read_value::<Option<bounds::Operand>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Root>() = Root::Dataset { borrow, index };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_address_root"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Root {
        fn empty() -> Self {
            Self::Parameter(0)
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Local { base, len } => {
                    #[repr(C)]
                    struct Fields {
                        base: Slot,
                        len: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        base: WriteCallback,
                        len: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_local"]
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
                                base: contracts::address(base).cast_mut(),
                                len: contracts::address(len).cast_mut(),
                            },
                            Host {
                                base: contracts::write_value::<usize>,
                                len: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Parameter(index) => {
                    #[repr(C)]
                    struct Fields {
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_parameter"]
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
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                index: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Resource { borrow, index } => {
                    #[repr(C)]
                    struct Fields {
                        borrow: Slot,
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrow: WriteCallback,
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_resource"]
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
                                borrow: contracts::address(borrow).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                borrow: contracts::write_value::<view::Borrow>,
                                index: contracts::write_value::<Option<bounds::Operand>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Buffer(binding) => {
                    #[repr(C)]
                    struct Fields {
                        binding: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        binding: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_buffer"]
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
                                binding: contracts::address(binding).cast_mut(),
                            },
                            Host {
                                binding: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Derived(borrow) => {
                    #[repr(C)]
                    struct Fields {
                        borrow: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrow: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_derived"]
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
                                borrow: contracts::address(borrow).cast_mut(),
                            },
                            Host {
                                borrow: contracts::write_value::<view::Borrow>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::External(identity) => {
                    #[repr(C)]
                    struct Fields {
                        identity: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        identity: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_external"]
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
                                identity: contracts::address(identity).cast_mut(),
                            },
                            Host {
                                identity: contracts::write_value::<Identity>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Dataset { borrow, index } => {
                    #[repr(C)]
                    struct Fields {
                        borrow: Slot,
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrow: WriteCallback,
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_root_dataset"]
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
                                borrow: contracts::address(borrow).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                borrow: contracts::write_value::<view::Borrow>,
                                index: contracts::write_value::<Option<bounds::Operand>>,
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
                        local: build_local,
                        parameter: build_parameter,
                        resource: build_resource,
                        buffer: build_buffer,
                        derived: build_derived,
                        external: build_external,
                        dataset: build_dataset,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(InitializationSource);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        position: ReadCallback,
        task: ReadCallback,
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <Origin as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_initialization_source_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                },
                Host {
                    origin: contracts::read_value::<Origin>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<InitializationSource>() = InitializationSource::Position(origin);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_task(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrow = <view::Borrow as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrow: Slot,
        }
        #[repr(C)]
        struct Host {
            borrow: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_initialization_source_task"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrow: contracts::slot(&mut borrow),
                },
                Host {
                    borrow: contracts::read_value::<view::Borrow>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<InitializationSource>() = InitializationSource::Task(borrow);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_initialization_source"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for InitializationSource {
        fn empty() -> Self {
            Self::Task(view::Borrow::default())
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
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
                        #[link_name = "bootstrap.contract_codec::contract_write_initialization_source_position"]
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
                                origin: contracts::write_value::<Origin>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Task(borrow) => {
                    #[repr(C)]
                    struct Fields {
                        borrow: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrow: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_initialization_source_task"]
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
                                borrow: contracts::address(borrow).cast_mut(),
                            },
                            Host {
                                borrow: contracts::write_value::<view::Borrow>,
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
                        position: build_position,
                        task: build_task,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Initialization);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        source: Slot,
        when: Slot,
    }
    #[repr(C)]
    struct Writer {
        source: WriteCallback,
        when: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        source: ReadCallback,
        when: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_initialization"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_initialization"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Initialization {
        fn empty() -> Self {
            Self {
                source: <InitializationSource as Native>::empty(),
                when: <bounds::Condition as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        source: contracts::address(&self.source).cast_mut(),
                        when: contracts::address(&self.when).cast_mut(),
                    },
                    Writer {
                        source: contracts::write_value::<InitializationSource>,
                        when: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        source: contracts::slot(&mut output.source),
                        when: contracts::slot(&mut output.when),
                    },
                    Reader {
                        source: contracts::read_value::<InitializationSource>,
                        when: contracts::read_value::<bounds::Condition>,
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

crate::codec::contracts::codec!(Identity);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        constant: ReadCallback,
        value: ReadCallback,
        storage: ReadCallback,
    }
    unsafe extern "C" fn build_constant(context: Slot, destination: Slot) -> ReadStatus {
        let mut bits = <Bits as Native>::empty();
        #[repr(C)]
        struct Fields {
            bits: Slot,
        }
        #[repr(C)]
        struct Host {
            bits: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_identity_constant"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    bits: contracts::slot(&mut bits),
                },
                Host {
                    bits: contracts::read_value::<Bits>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Identity>() = Identity::Constant(bits);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_value(context: Slot, destination: Slot) -> ReadStatus {
        let mut ids = <Vec<ValueIdentity> as Native>::empty();
        #[repr(C)]
        struct Fields {
            ids: Slot,
        }
        #[repr(C)]
        struct Host {
            ids: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_identity_value"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    ids: contracts::slot(&mut ids),
                },
                Host {
                    ids: contracts::read_value::<Vec<ValueIdentity>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Identity>() = Identity::Value(ids);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_storage(context: Slot, destination: Slot) -> ReadStatus {
        let mut borrow = <view::Borrow as Native>::empty();
        #[repr(C)]
        struct Fields {
            borrow: Slot,
        }
        #[repr(C)]
        struct Host {
            borrow: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_address_identity_storage"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    borrow: contracts::slot(&mut borrow),
                },
                Host {
                    borrow: contracts::read_value::<view::Borrow>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Identity>() = Identity::Storage(borrow);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_address_identity"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Identity {
        fn empty() -> Self {
            Self::Constant(0)
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Constant(bits) => {
                    #[repr(C)]
                    struct Fields {
                        bits: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        bits: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_identity_constant"]
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
                                bits: contracts::address(bits).cast_mut(),
                            },
                            Host {
                                bits: contracts::write_value::<Bits>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Value(ids) => {
                    #[repr(C)]
                    struct Fields {
                        ids: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        ids: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_identity_value"]
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
                                ids: contracts::address(ids).cast_mut(),
                            },
                            Host {
                                ids: contracts::write_value::<Vec<ValueIdentity>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Storage(borrow) => {
                    let projected = borrow.map_sources(|source| match source {
                        view::Source::Position(origin) if origin.has_local() => {
                            view::Borrow::source(view::Source::External)
                        }
                        source => view::Borrow::source(source),
                    });
                    let borrow = &projected;
                    #[repr(C)]
                    struct Fields {
                        borrow: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        borrow: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_address_identity_storage"]
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
                                borrow: contracts::address(borrow).cast_mut(),
                            },
                            Host {
                                borrow: contracts::write_value::<view::Borrow>,
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
                        constant: build_constant,
                        value: build_value,
                        storage: build_storage,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Origin);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        root: Slot,
        layout: Slot,
        base: Slot,
        words: Slot,
        writable: Slot,
        initialized: Slot,
    }
    #[repr(C)]
    struct Writer {
        root: WriteCallback,
        layout: WriteCallback,
        base: WriteCallback,
        words: WriteCallback,
        writable: WriteCallback,
        initialized: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        root: ReadCallback,
        layout: ReadCallback,
        base: ReadCallback,
        words: ReadCallback,
        writable: ReadCallback,
        initialized: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_address_origin"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_address_origin"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    unsafe extern "C" fn read_public_root(context: Slot, output: Slot) -> ReadStatus {
        let root = unsafe { &mut *output.cast::<Root>() };
        let status = unsafe { Root::read(context, root) };
        if status != ReadStatus::Success {
            return status;
        }
        if matches!(root, Root::Local { .. } | Root::Buffer(_)) {
            return unsafe {
                contracts::fail(context, "模块公开地址合同不能包含被调函数的私有存储")
            };
        }
        ReadStatus::Success
    }
    impl Native for Origin {
        fn empty() -> Self {
            Self {
                root: <Root as Native>::empty(),
                layout: <Layout as Native>::empty(),
                base: <Layout as Native>::empty(),
                words: <Arc<[u32]> as Native>::empty(),
                writable: <bool as Native>::empty(),
                initialized: <Arc<[bool]> as Native>::empty(),
                positions: Arc::from([]),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        root: contracts::address(&self.root).cast_mut(),
                        layout: contracts::address(&self.layout).cast_mut(),
                        base: contracts::address(&self.base).cast_mut(),
                        words: contracts::address(&self.words).cast_mut(),
                        writable: contracts::address(&self.writable).cast_mut(),
                        initialized: contracts::address(&self.initialized).cast_mut(),
                    },
                    Writer {
                        root: contracts::write_value::<Root>,
                        layout: contracts::write_value::<Layout>,
                        base: contracts::write_value::<Layout>,
                        words: contracts::write_value::<Arc<[u32]>>,
                        writable: contracts::write_value::<bool>,
                        initialized: contracts::write_value::<Arc<[bool]>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        root: contracts::slot(&mut output.root),
                        layout: contracts::slot(&mut output.layout),
                        base: contracts::slot(&mut output.base),
                        words: contracts::slot(&mut output.words),
                        writable: contracts::slot(&mut output.writable),
                        initialized: contracts::slot(&mut output.initialized),
                    },
                    Reader {
                        root: read_public_root,
                        layout: contracts::read_value::<Layout>,
                        base: contracts::read_value::<Layout>,
                        words: contracts::read_value::<Arc<[u32]>>,
                        writable: contracts::read_value::<bool>,
                        initialized: contracts::read_value::<Arc<[bool]>>,
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
