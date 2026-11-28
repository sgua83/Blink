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
pub(super) enum AddressSourceKind { End, Position, Inactive, Other }

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
#[repr(u32)]
enum ResourceIdentityKind { Unknown, Input, Data, Element, Projection }

#[repr(C)]
struct ResourceIdentityView {
    kind: ResourceIdentityKind,
    key: *const std::ffi::c_void,
    parent: *const std::ffi::c_void,
}

#[repr(C)]
struct ResourceIdentityHost {
    inspect: unsafe extern "C" fn(*const std::ffi::c_void) -> ResourceIdentityView,
    equal: unsafe extern "C" fn(ResourceIdentityKind, *const std::ffi::c_void, *const std::ffi::c_void) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::same_position_resource"]
    fn b_same_position_resource(left: *const std::ffi::c_void, right: *const std::ffi::c_void, host: ResourceIdentityHost) -> u32;
}

unsafe extern "C" fn inspect_resource_identity(borrow: *const std::ffi::c_void) -> ResourceIdentityView {
    use ResourceIdentityKind as Kind;
    use std::ptr::{from_ref, null};
    let borrow = unsafe { &*borrow.cast::<view::Borrow>() };
    let unknown = ResourceIdentityView { kind: Kind::Unknown, key: null(), parent: null() };
    if !borrow.calls.is_empty() || !borrow.reads.is_empty() { return unknown; }

    if borrow.sources.is_empty() {
        let mut inputs = borrow.inputs.iter();
        return match (inputs.next(), inputs.next()) {
            (Some(input), None) => ResourceIdentityView { kind: Kind::Input, key: from_ref(input).cast(), parent: null() },
            _ => unknown,
        };
    }
    if !borrow.inputs.is_empty() { return unknown; }
    let mut sources = borrow.sources.iter();
    let (Some(source), None) = (sources.next(), sources.next()) else { return unknown; };
    let (kind, key, parent) = match source {
        view::Source::Data(id) => (Kind::Data, from_ref(id).cast(), null()),
        view::Source::Element { parent, .. } => (Kind::Element, from_ref(source).cast(), from_ref(&**parent).cast()),
        view::Source::Projection { parent, .. } => (Kind::Projection, from_ref(source).cast(), from_ref(&**parent).cast()),
        _ => return unknown,
    };
    ResourceIdentityView { kind, key, parent }
}

unsafe extern "C" fn equal_resource_identity(
    kind: ResourceIdentityKind, left: *const std::ffi::c_void, right: *const std::ffi::c_void,
) -> u32 {
    use ResourceIdentityKind as Kind;
    let equal = match kind {
        Kind::Input => unsafe { *left.cast::<view::LoopInput>() == *right.cast::<view::LoopInput>() },
        Kind::Data => unsafe { *left.cast::<ValueIdentity>() == *right.cast::<ValueIdentity>() },
        Kind::Element | Kind::Projection => {
            let (left, right) = unsafe { (&*left.cast::<view::Source>(), &*right.cast::<view::Source>()) };
            match (left, right) {
                (view::Source::Element { fields, index, .. }, view::Source::Element { fields: other, index: other_index, .. }) =>
                    fields == other && index == other_index,
                (view::Source::Projection { fields, .. }, view::Source::Projection { fields: other, .. }) => fields == other,
                _ => unreachable!(),
            }
        }
        Kind::Unknown => unreachable!(),
    };
    u32::from(equal)
}

fn same_position_resource(left: &view::Borrow, right: &view::Borrow) -> bool {
    let host = ResourceIdentityHost { inspect: inspect_resource_identity, equal: equal_resource_identity };
    // B borrows each root and existing projection only through this query.
    unsafe { b_same_position_resource(std::ptr::from_ref(left).cast(), std::ptr::from_ref(right).cast(), host) != 0 }
}
#[repr(u32)]
enum RegionWriteKind { Unrelated, Position, Resource, Selected }

#[repr(C)]
struct RegionWriteFacts { kind: RegionWriteKind, word: u32, target: u32, offset: u32, size: u32 }

#[repr(u32)]
enum WritePositionKind { None, Ranges, All, Composite }

#[repr(C)]
struct WritePositionView { children: usize, kind: WritePositionKind }

#[repr(C)]
struct WriteRegions { data: *const std::ffi::c_void, count: usize, present: u32 }

#[repr(C)]
struct WriteProjectionHost {
    context: *const std::ffi::c_void,
    distinct_element: unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> u32,
    position: unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> WritePositionView,
    child: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> *const std::ffi::c_void,
    regions: unsafe extern "C" fn(*const std::ffi::c_void) -> WriteRegions,
    region: unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void, usize) -> RegionWriteFacts,
}

struct WriteProjection<'a, 'r> {
    origin: &'a Origin,
    when: &'a bounds::Clause,
    index: Option<bounds::Operand>,
    regions: &'a dyn Fn() -> Option<&'r [MemoryRegion]>,
}

impl WriteProjection<'_, '_> {
    unsafe extern "C" fn distinct_element(context: *const std::ffi::c_void, position: *const std::ffi::c_void) -> u32 {
        let input = unsafe { &*context.cast::<Self>() };
        let position = unsafe { &*position.cast::<Position>() };
        (MemoryEffect::same_element(input.origin, input.when, position, input.index) == Some(false)).into()
    }

    unsafe extern "C" fn position(context: *const std::ffi::c_void, position: *const std::ffi::c_void) -> WritePositionView {
        let input = unsafe { &*context.cast::<Self>() };
        let position = unsafe { &*position.cast::<Position>() };
        let (kind, children) = match position {
            Position::Initialized => (WritePositionKind::None, 0),
            Position::Local(_) => (WritePositionKind::Ranges, 0),
            Position::Element(binding, _) => {
                let aliases = input.origin.aliases_when(
                    &|alias| matches!(alias, MemoryAlias::Data(actual) if actual == binding), || true);
                let kind = if aliases.is_empty() { WritePositionKind::None } else { WritePositionKind::All };
                (kind, 0)
            }
            Position::Borrowed(borrow, _, _) if matches!(&input.origin.root,
                Root::Resource { borrow: source, .. } | Root::Dataset { borrow: source, .. }
                    if same_position_resource(source, borrow))
                || !borrow.pending() && !borrow.unresolved() && !external_memory(borrow)
                    && !borrow.sources.iter().any(|source| matches!(source, view::Source::Parameter(..))) =>
                (WritePositionKind::Ranges, 0),
            Position::Both(..) | Position::Conditional(..) => (WritePositionKind::Composite, 2),
            Position::Indexed(positions, _) => (WritePositionKind::Composite, positions.len()),
            _ => (WritePositionKind::All, 0),
        };
        WritePositionView { children, kind }
    }

    unsafe extern "C" fn child(position: *const std::ffi::c_void, index: usize) -> *const std::ffi::c_void {
        let position = unsafe { &*position.cast::<Position>() };
        let child = match position {
            Position::Both(a, b) | Position::Conditional(_, a, b) => [a.as_ref(), b.as_ref()][index],
            Position::Indexed(positions, _) => &positions[index],
            _ => unreachable!(),
        };
        std::ptr::from_ref(child).cast()
    }

    unsafe extern "C" fn regions(context: *const std::ffi::c_void) -> WriteRegions {
        let input = unsafe { &*context.cast::<Self>() };
        match (input.regions)() {
            Some(regions) => WriteRegions { data: regions.as_ptr().cast(), count: regions.len(), present: 1 },
            None => WriteRegions { data: std::ptr::null(), count: 0, present: 0 },
        }
    }

    unsafe extern "C" fn region(data: *const std::ffi::c_void, position: *const std::ffi::c_void, index: usize) -> RegionWriteFacts {
        let region = unsafe { &*data.cast::<MemoryRegion>().add(index) };
        region.write_facts(unsafe { &*position.cast::<Position>() })
    }

    fn bytes(&self, position: &Position, width: u32) -> u8 {
        let host = WriteProjectionHost { context: std::ptr::from_ref(self).cast(),
            distinct_element: Self::distinct_element, position: Self::position, child: Self::child,
            regions: Self::regions, region: Self::region };
        unsafe { project_address_write_bytes(std::ptr::from_ref(position).cast(), width, host) }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::project_address_write_bytes"]
    fn project_address_write_bytes(position: *const std::ffi::c_void, width: u32, host: WriteProjectionHost) -> u8;
    #[link_name = "bootstrap.call_arguments::region_written_bytes"]
    fn region_written_bytes(facts: RegionWriteFacts, width: u32) -> u8;
}

impl MemoryRegion {
    fn write_facts(&self, position: &Position) -> RegionWriteFacts {
        let (kind, word, target) = match (&self.position, position) {
            (Position::Borrowed(other, base, word), Position::Borrowed(borrow, layout, target)) if base == layout => {
                let aliases = same_position_resource(other, borrow)
                    || borrow.sources.entries().any(|(source, when)| other.overlaps_source(source, &when));
                let kind = if aliases { RegionWriteKind::Resource } else { RegionWriteKind::Unrelated };
                (kind, *word, *target)
            }
            (written, position) if written == position => (RegionWriteKind::Position, 0, 0),
            (written, position) if owned_presence::Presence::selected(written, position) != owned_presence::Presence::None =>
                (RegionWriteKind::Selected, 0, 0),
            _ => (RegionWriteKind::Unrelated, 0, 0),
        };
        RegionWriteFacts { kind, word, target, offset: self.offset, size: self.size }
    }

    pub(super) fn written_bytes(&self, position: &Position, scalar: Scalar) -> u8 {
        unsafe { region_written_bytes(self.write_facts(position), scalar.size()) }
    }
}
#[repr(u32)]
enum OriginSourceKind { End, Position, Inactive, External, Other }

#[repr(C)]
struct OriginSource { kind: OriginSourceKind, layout_matches: u32 }

#[repr(u32)]
enum OriginSourceAction { Direct, Project, Inactive, External, Retain, Pending }

#[repr(C)]
struct OriginNormalizationHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> OriginSource,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, OriginSourceAction),
}

#[repr(C)]
struct OriginWord { word: u32, initialized: u32 }

#[repr(u32)]
enum OriginProjectionResult { Missing, ReadOnly, Writable }

#[repr(C)]
struct OriginProjectionHost {
    context: *mut std::ffi::c_void,
    selected: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> OriginWord,
    parent_offset: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u32,
    parent: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> OriginWord,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, usize, u32, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::normalize_address_origin"]
    fn b_normalize_address_origin(derived: u32, pending: u32, host: OriginNormalizationHost);
    #[link_name = "bootstrap.call_arguments::project_address_origin"]
    fn b_project_address_origin(selected: usize, parent: usize, selected_writable: u32,
        parent_writable: u32, host: OriginProjectionHost) -> OriginProjectionResult;
}

struct OriginProjection<'a> {
    parent: &'a Origin,
    selected: &'a Origin,
    words: Vec<u32>,
    positions: Vec<Position>,
    initialized: Vec<bool>,
}

impl OriginProjection<'_> {
    unsafe extern "C" fn selected(context: *const std::ffi::c_void, slot: usize) -> OriginWord {
        let selected = unsafe { &*context.cast::<Self>() }.selected;
        OriginWord { word: selected.words[slot], initialized: selected.initialized[slot].into() }
    }

    unsafe extern "C" fn parent_offset(context: *const std::ffi::c_void, slot: usize) -> u32 {
        unsafe { &*context.cast::<Self>() }.parent.layout.parts[slot].1
    }

    unsafe extern "C" fn parent(context: *const std::ffi::c_void, slot: usize) -> OriginWord {
        let parent = unsafe { &*context.cast::<Self>() }.parent;
        OriginWord { word: parent.words[slot], initialized: parent.initialized[slot].into() }
    }

    unsafe extern "C" fn emit(context: *mut std::ffi::c_void, source: usize, word: u32, initialized: u32) {
        let projection = unsafe { &mut *context.cast::<Self>() };
        projection.words.push(word);
        if !projection.parent.positions.is_empty() {
            projection.positions.push(projection.parent.positions[source].clone());
        }
        projection.initialized.push(initialized != 0);
    }

    fn materialize(mut self) -> Origin {
        let (selected, parent) = (self.selected, self.parent);
        let host = OriginProjectionHost {
            context: std::ptr::from_mut(&mut self).cast(), selected: Self::selected,
            parent_offset: Self::parent_offset, parent: Self::parent, emit: Self::emit,
        };
        let result = unsafe { b_project_address_origin(selected.words.len(), parent.layout.parts.len(),
            selected.writable.into(), parent.writable.into(), host) };
        let writable = match result {
            OriginProjectionResult::Missing => panic!("origin projection must be within the parent layout"),
            OriginProjectionResult::ReadOnly => false,
            OriginProjectionResult::Writable => true,
        };
        Origin { root: self.parent.root.clone(), layout: self.selected.layout.clone(), base: self.parent.base.clone(),
            words: self.words.into(), positions: self.positions.into(), writable, initialized: self.initialized.into() }
    }
}

struct OriginNormalization<'a, I> {
    origin: &'a Origin,
    borrow: Option<&'a view::Borrow>,
    entries: I,
    current: Option<(&'a view::Source, bounds::Clause)>,
    result: view::Borrow,
}

impl<'a, I: Iterator<Item = (&'a view::Source, bounds::Clause)>> OriginNormalization<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> OriginSource {
        let normalization = unsafe { &mut *context.cast::<Self>() };
        normalization.current = normalization.entries.next();
        let (kind, matches) = match normalization.current.as_ref().map(|(source, _)| *source) {
            None => (OriginSourceKind::End, false),
            Some(view::Source::Position(parent)) => (OriginSourceKind::Position, parent.layout == normalization.origin.base),
            Some(view::Source::Inactive) => (OriginSourceKind::Inactive, false),
            Some(view::Source::External) => (OriginSourceKind::External, false),
            Some(_) => (OriginSourceKind::Other, false),
        };
        OriginSource { kind, layout_matches: matches.into() }
    }

    unsafe extern "C" fn emit(context: *mut std::ffi::c_void, action: OriginSourceAction) {
        let normalization = unsafe { &mut *context.cast::<Self>() };
        let origin = normalization.origin;
        match action {
            OriginSourceAction::Direct => {
                normalization.result.sources.insert(view::Source::Position(Arc::new(origin.clone())));
                return;
            }
            OriginSourceAction::Pending => {
                let borrow = normalization.borrow.unwrap();
                let mut value = origin.clone();
                value.root = Root::Derived(view::Borrow {
                    inputs: borrow.inputs.clone(), calls: borrow.calls.clone(), reads: borrow.reads.clone(),
                    ..view::Borrow::default()
                });
                normalization.result.sources.insert(view::Source::Position(Arc::new(value)));
                return;
            }
            _ => {}
        }
        let (source, when) = normalization.current.take().unwrap();
        let source = match action {
            OriginSourceAction::Project => {
                let view::Source::Position(parent) = source else { unreachable!() };
                view::Source::Position(Arc::new(parent.project(origin)))
            }
            OriginSourceAction::Inactive => view::Source::Inactive,
            OriginSourceAction::External => {
                let mut value = origin.clone();
                value.root = Root::External(Identity::Value(Vec::new()));
                value.base = value.layout.clone();
                value.words = value.layout.parts.iter().map(|(_, at)| *at).collect();
                value.initialized = vec![true; value.layout.parts.len()].into();
                view::Source::Position(Arc::new(value))
            }
            OriginSourceAction::Retain => {
                let mut value = origin.clone();
                value.root = Root::Derived(view::Borrow::source(source.clone()));
                view::Source::Position(Arc::new(value))
            }
            _ => unreachable!(),
        };
        let when = bounds::Clause::both(&bounds::Clause::default(), &when);
        normalization.result.sources.union_when(source, &when);
    }

    fn normalize(mut self) -> view::Borrow {
        let derived = self.borrow.is_some();
        let pending = self.borrow.is_some_and(|borrow| {
            !borrow.inputs.is_empty() || !borrow.calls.is_empty() || !borrow.reads.is_empty()
        });
        let host = OriginNormalizationHost { context: std::ptr::from_mut(&mut self).cast(),
            next: Self::next, emit: Self::emit };
        unsafe { b_normalize_address_origin(derived.into(), pending.into(), host) };
        self.result
    }
}

#[repr(u32)]
enum AddressPositionRoot { Other, Derived, Dataset }

#[repr(u32)]
enum AddressPositionValue { Stored, Initialized }

#[repr(C)]
struct AddressPositionHost {
    context: *mut std::ffi::c_void,
    offset: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u32,
    initialized: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u32,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, usize, AddressPositionValue),
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressSourceEntry,
    resolve: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    same: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
    accept: unsafe extern "C" fn(*mut std::ffi::c_void),
    pending: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::address_position_at"]
    fn b_address_position_at(word: u32, parts: usize, stored: u32, root: AddressPositionRoot,
        host: AddressPositionHost) -> u32;
}

struct PositionSources<'a, I> {
    entries: I,
    parent: Option<&'a Origin>,
}

impl<'a, I: Iterator<Item = (&'a view::Source, bounds::Clause)>> PositionSources<'a, I> {
    fn next(&mut self) -> AddressSourceEntry {
        let Some((source, when)) = self.entries.next() else {
            return AddressSourceEntry { kind: AddressSourceKind::End, possible: 0 };
        };
        self.parent = match source { view::Source::Position(origin) => Some(origin), _ => None };
        let kind = if self.parent.is_some() { AddressSourceKind::Position } else { AddressSourceKind::Other };
        AddressSourceEntry { kind, possible: when.possible().into() }
    }
}

struct PositionAt<'a, I> {
    origin: &'a Origin,
    borrow: Option<&'a view::Borrow>,
    index: Option<bounds::Operand>,
    sources: PositionSources<'a, I>,
    current: Option<Position>,
    known: Option<Position>,
}

impl<'a, I: Iterator<Item = (&'a view::Source, bounds::Clause)>> PositionAt<'a, I> {
    unsafe extern "C" fn offset(context: *const std::ffi::c_void, slot: usize) -> u32 {
        unsafe { &*context.cast::<Self>() }.origin.layout.parts[slot].1
    }

    unsafe extern "C" fn initialized(context: *const std::ffi::c_void, slot: usize) -> u32 {
        unsafe { &*context.cast::<Self>() }.origin.initialized[slot].into()
    }

    unsafe extern "C" fn emit(context: *mut std::ffi::c_void, slot: usize, value: AddressPositionValue) {
        let query = unsafe { &mut *context.cast::<Self>() };
        query.known = Some(match value {
            AddressPositionValue::Stored => query.origin.positions[slot].clone(),
            AddressPositionValue::Initialized => Position::Initialized,
        });
    }

    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressSourceEntry {
        unsafe { &mut *context.cast::<Self>() }.sources.next()
    }

    unsafe extern "C" fn resolve(context: *mut std::ffi::c_void, slot: usize) -> u32 {
        let query = unsafe { &mut *context.cast::<Self>() };
        let parent = query.sources.parent.unwrap();
        if parent.layout != query.origin.base { return 0; }
        let Some(position) = parent.position_at(query.origin.words[slot]) else { return 0; };
        query.current = match query.index {
            Some(index) => position.dataset_position(index),
            None => Some(position),
        };
        query.current.is_some().into()
    }

    unsafe extern "C" fn same(context: *const std::ffi::c_void) -> u32 {
        let query = unsafe { &*context.cast::<Self>() };
        (query.known == query.current).into()
    }

    unsafe extern "C" fn accept(context: *mut std::ffi::c_void) {
        let query = unsafe { &mut *context.cast::<Self>() };
        query.known = query.current.take();
    }

    unsafe extern "C" fn pending(context: *const std::ffi::c_void) -> u32 {
        let borrow = unsafe { &*context.cast::<Self>() }.borrow.unwrap();
        (!borrow.inputs.is_empty() || !borrow.calls.is_empty() || !borrow.reads.is_empty()).into()
    }

    fn get(mut self, word: u32, root: AddressPositionRoot) -> Option<Position> {
        let parts = self.origin.layout.parts.len();
        let stored = !self.origin.positions.is_empty();
        let host = AddressPositionHost {
            context: std::ptr::from_mut(&mut self).cast(), offset: Self::offset, initialized: Self::initialized,
            emit: Self::emit, next: Self::next, resolve: Self::resolve,
            same: Self::same, accept: Self::accept, pending: Self::pending,
        };
        if unsafe { b_address_position_at(word, parts, stored.into(), root, host) } == 0 { return None; }
        self.known
    }
}

#[repr(u32)]
enum AddressLayoutKind { End, Trusted, Parent, Relation, Unknown }

#[repr(C)]
struct AddressLayoutEntry { kind: AddressLayoutKind, fits: u32 }

#[repr(C)]
struct AddressLayoutHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressLayoutEntry,
    validate: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::address_layout_valid"]
    fn b_address_layout_valid(host: AddressLayoutHost) -> u32;
}

struct AddressLayout<'a, I> {
    base: &'a Layout,
    sources: I,
    current: Option<&'a view::Source>,
}

impl<'a, I: Iterator<Item = &'a view::Source>> AddressLayout<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressLayoutEntry {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.current = input.sources.next();
        use AddressLayoutKind as Kind;
        let (kind, fits) = match input.current {
            None => (Kind::End, false),
            Some(view::Source::Position(origin)) => (Kind::Parent, origin.layout == *input.base),
            Some(view::Source::Relation(..)) => (Kind::Relation, false),
            Some(view::Source::External | view::Source::Parameter(..) | view::Source::Contents { .. }
                 | view::Source::Inactive) => (Kind::Trusted, false),
            Some(_) => (Kind::Unknown, false),
        };
        AddressLayoutEntry { kind, fits: fits.into() }
    }

    unsafe extern "C" fn validate(context: *const std::ffi::c_void) -> u32 {
        let input = unsafe { &*context.cast::<Self>() };
        match input.current.unwrap() {
            view::Source::Position(origin) => origin.layout_valid(),
            view::Source::Relation(graph, root) => graph.symbolic(*root),
            _ => unreachable!(),
        }.into()
    }

    fn valid(mut self) -> bool {
        let host = AddressLayoutHost { context: std::ptr::from_mut(&mut self).cast(),
            next: Self::next, validate: Self::validate };
        unsafe { b_address_layout_valid(host) != 0 }
    }
}

#[repr(u32)]
enum AddressOriginRoot { External, Derived, Dataset, Stored, Parameter, Resource }

impl Root {
    fn kind(&self) -> AddressOriginRoot {
        use AddressOriginRoot as Kind;
        match self {
            Self::External(_) => Kind::External,
            Self::Derived(_) => Kind::Derived,
            Self::Dataset { .. } => Kind::Dataset,
            Self::Parameter(_) => Kind::Parameter,
            Self::Resource { .. } => Kind::Resource,
            Self::Local { .. } | Self::Buffer(_) => Kind::Stored,
        }
    }
}

#[repr(u32)]
enum AddressOriginQuery { Storage, Symbolic }

#[repr(u32)]
enum AddressOriginSource { End, Position, Symbolic, Relation, Other }

#[repr(C)]
struct AddressOriginHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressOriginSource,
    follow: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
    symbolic_relation: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
    possible: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::query_address_origin"]
    fn b_query_address_origin(root: AddressOriginRoot, query: AddressOriginQuery, host: AddressOriginHost) -> u32;
}

struct AddressOriginSources<'a, I> {
    sources: I,
    path: Option<&'a bounds::Clause>,
    current: Option<(&'a view::Source, &'a bounds::Clause)>,
}

impl<'a, I: Iterator<Item = (&'a view::Source, &'a bounds::Clause)>> AddressOriginSources<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressOriginSource {
        let source = unsafe { &mut *context.cast::<Self>() };
        source.current = source.sources.next();
        use AddressOriginSource as Kind;
        match source.current.map(|(entry, _)| entry) {
            None => Kind::End,
            Some(view::Source::Position(_)) => Kind::Position,
            Some(view::Source::Relation(..)) => Kind::Relation,
            Some(view::Source::Parameter(ValueIdentity::Register(_) | ValueIdentity::ParameterContent(..)
                | ValueIdentity::DatasetContent(..), _) | view::Source::Contents { .. }) => Kind::Symbolic,
            Some(_) => Kind::Other,
        }
    }

    unsafe extern "C" fn follow(context: *const std::ffi::c_void) -> u32 {
        let source = unsafe { &*context.cast::<Self>() };
        let (view::Source::Position(parent), edge) = source.current.unwrap() else { unreachable!() };
        match source.path {
            Some(path) => parent.has_blink_storage(&bounds::Clause::both(path, edge)),
            None => parent.symbolic_input(),
        }.into()
    }

    unsafe extern "C" fn symbolic_relation(context: *const std::ffi::c_void) -> u32 {
        let (view::Source::Relation(graph, root), _) = unsafe { &*context.cast::<Self>() }.current.unwrap()
            else { unreachable!() };
        graph.symbolic(*root).into()
    }

    unsafe extern "C" fn possible(context: *const std::ffi::c_void) -> u32 {
        unsafe { &*context.cast::<Self>() }.path.unwrap().possible().into()
    }

    fn query(mut self, root: AddressOriginRoot) -> bool {
        let query = if self.path.is_some() { AddressOriginQuery::Storage } else { AddressOriginQuery::Symbolic };
        let host = AddressOriginHost { context: std::ptr::from_mut(&mut self).cast(), next: Self::next,
            follow: Self::follow, symbolic_relation: Self::symbolic_relation, possible: Self::possible };
        unsafe { b_query_address_origin(root, query, host) != 0 }
    }
}

#[repr(u32)]
enum AddressOriginBindingMode { Substitute, Outputs, Call }

#[repr(C)]
struct AddressOriginBindingHost {
    context: *mut std::ffi::c_void,
    parameter: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    unresolved_parameter: unsafe extern "C" fn(*mut std::ffi::c_void),
    bind_borrow: unsafe extern "C" fn(*mut std::ffi::c_void),
    external: unsafe extern "C" fn(*mut std::ffi::c_void),
    clear_positions: unsafe extern "C" fn(*mut std::ffi::c_void),
    index: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    unresolved_index: unsafe extern "C" fn(*mut std::ffi::c_void),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::bind_address_origin"]
    fn b_bind_address_origin(root: AddressOriginRoot, mode: AddressOriginBindingMode, host: AddressOriginBindingHost);
}

#[derive(Copy, Clone)]
enum AddressOriginBindings<'a> {
    Substitute(&'a BTreeMap<ValueIdentity, bounds::Proof>),
    Outputs(&'a bounds::OutputBindings),
    Call(&'a BTreeMap<ValueIdentity, bounds::Proof>),
}

impl AddressOriginBindings<'_> {
    fn mode(self) -> AddressOriginBindingMode {
        match self {
            Self::Substitute(_) => AddressOriginBindingMode::Substitute,
            Self::Outputs(_) => AddressOriginBindingMode::Outputs,
            Self::Call(_) => AddressOriginBindingMode::Call,
        }
    }
}

struct AddressOriginBinding<'a> { value: Origin, bindings: AddressOriginBindings<'a> }

impl AddressOriginBinding<'_> {
    unsafe extern "C" fn parameter(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        let Root::Parameter(parameter) = &input.value.root else { unreachable!() };
        let (AddressOriginBindings::Substitute(values) | AddressOriginBindings::Call(values)) = input.bindings
            else { unreachable!() };
        let Some(proof) = values.get(&ValueIdentity::ParameterStorage(*parameter)) else { return 0 };
        input.value.root = Root::Derived(proof.view_borrow(0));
        1
    }

    unsafe extern "C" fn unresolved_parameter(context: *mut std::ffi::c_void) {
        unsafe { &mut *context.cast::<Self>() }.value.root = Root::Derived(view::Borrow::source(view::Source::Unresolved));
    }

    fn borrow_mut(&mut self) -> &mut view::Borrow {
        match &mut self.value.root {
            Root::Resource { borrow, .. } | Root::Derived(borrow) | Root::Dataset { borrow, .. } => borrow,
            _ => unreachable!(),
        }
    }

    unsafe extern "C" fn bind_borrow(context: *mut std::ffi::c_void) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let bindings = input.bindings;
        let borrow = input.borrow_mut();
        *borrow = match bindings {
            AddressOriginBindings::Substitute(values) => borrow.substitute_values(values),
            AddressOriginBindings::Outputs(values) => borrow.output_bound(values),
            AddressOriginBindings::Call(values) => borrow.at_call(values),
        };
    }

    unsafe extern "C" fn external(context: *mut std::ffi::c_void) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let Root::External(identity) = &mut input.value.root else { unreachable!() };
        *identity = match input.bindings {
            AddressOriginBindings::Substitute(values) => identity.substitute_values(values),
            AddressOriginBindings::Outputs(values) => identity.output_bound(values),
            AddressOriginBindings::Call(values) => identity.at_call(values),
        };
    }

    unsafe extern "C" fn clear_positions(context: *mut std::ffi::c_void) {
        unsafe { &mut *context.cast::<Self>() }.value.positions = Arc::from([]);
    }

    unsafe extern "C" fn index(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (Root::Resource { index: Some(index), .. } | Root::Dataset { index: Some(index), .. }) = &mut input.value.root
            else { return 1 };
        let resolved = match input.bindings {
            AddressOriginBindings::Substitute(values) => {
                let bounds::Operand::Value(id) = index else { return 1 };
                if !values.contains_key(id) { return 1; }
                index.at_call(values)
            }
            AddressOriginBindings::Outputs(values) => {
                if let bounds::Operand::Value(id) = index && let Some(bound) = values.get(id) { *id = *bound; }
                return 1;
            }
            AddressOriginBindings::Call(values) => index.at_call(values),
        };
        let Some(resolved) = resolved else { return 0 };
        *index = resolved;
        1
    }

    unsafe extern "C" fn unresolved_index(context: *mut std::ffi::c_void) {
        *unsafe { &mut *context.cast::<Self>() }.borrow_mut() = view::Borrow::source(view::Source::Unresolved);
    }

    fn bind(mut self) -> Origin {
        let root = self.value.root.kind();
        let mode = self.bindings.mode();
        let host = AddressOriginBindingHost { context: std::ptr::from_mut(&mut self).cast(), parameter: Self::parameter,
            unresolved_parameter: Self::unresolved_parameter, bind_borrow: Self::bind_borrow, external: Self::external,
            clear_positions: Self::clear_positions, index: Self::index, unresolved_index: Self::unresolved_index };
        unsafe { b_bind_address_origin(root, mode, host) };
        self.value
    }
}

impl Origin {
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        AddressOriginBinding { value: self.clone(), bindings: AddressOriginBindings::Substitute(values) }.bind()
    }

    fn query_address_origin(&self, path: Option<&bounds::Clause>) -> bool {
        let borrow = match &self.root {
            Root::Derived(borrow) | Root::Dataset { borrow, .. } => Some(borrow),
            _ => None,
        };
        let sources = borrow.into_iter().flat_map(|borrow| borrow.sources.borrowed_entries());
        AddressOriginSources { sources, path, current: None }.query(self.root.kind())
    }

    pub fn symbolic_input(&self) -> bool {
        self.query_address_origin(None)
    }
    pub(super) fn has_blink_storage(&self, when: &bounds::Clause) -> bool {
        self.query_address_origin(Some(when))
    }
    pub fn layout_valid(&self) -> bool {
        let (Root::Derived(borrow) | Root::Dataset { borrow, .. }) = &self.root else { return true };
        AddressLayout { base: &self.base, sources: borrow.sources.iter(), current: None }.valid()
    }
    pub fn position_at(&self,word:u32)->Option<Position> {
        let (kind, borrow, index) = match &self.root {
            Root::Derived(borrow) => (AddressPositionRoot::Derived, Some(borrow), None),
            Root::Dataset { borrow, index } => (AddressPositionRoot::Dataset, Some(borrow), *index),
            _ => (AddressPositionRoot::Other, None, None),
        };
        let sources = PositionSources { entries: borrow.into_iter().flat_map(|borrow| borrow.sources.entries()), parent: None };
        PositionAt { origin: self, borrow, index, sources, current: None, known: None }.get(word, kind)
    }
    fn project(&self, selected: &Self) -> Self {
        let count = selected.words.len();
        let positions = if self.positions.is_empty() { Vec::new() } else { Vec::with_capacity(count) };
        OriginProjection { parent: self, selected, words: Vec::with_capacity(count), positions,
            initialized: Vec::with_capacity(count) }.materialize()
    }

    pub fn normalize(&self) -> view::Borrow {
        let borrow = match &self.root { Root::Derived(borrow) => Some(borrow), _ => None };
        OriginNormalization { origin: self, borrow, entries: borrow.into_iter().flat_map(|borrow| borrow.sources.entries()),
            current: None, result: view::Borrow::default() }.normalize()
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
        project_memory_aliases(MemoryAliasOrigin::of(self), needed, possible)
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
        AddressOriginBinding { value: self.clone(), bindings: AddressOriginBindings::Outputs(bindings) }.bind()
    }
    pub fn bound_at_call(&self, bindings: &BTreeMap<ValueIdentity,bounds::Proof>) -> Self {
        AddressOriginBinding { value: self.clone(), bindings: AddressOriginBindings::Call(bindings) }.bind()
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

#[repr(u32)]
enum AddressIdentityKind { Constant, Values, Storage }

#[repr(u32)]
enum AddressIdentityResult { Original, Bound, Values }

#[repr(C)]
struct AddressIdentitySources { present: u32, count: usize }

#[repr(C)]
struct AddressIdentityHost {
    context: *mut std::ffi::c_void,
    address_at: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    sources: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> AddressIdentitySources,
    append_source: unsafe extern "C" fn(*mut std::ffi::c_void, usize),
    append_original: unsafe extern "C" fn(*mut std::ffi::c_void, usize),
    bind_storage: unsafe extern "C" fn(*mut std::ffi::c_void),
}

#[repr(C)]
struct AddressStorageHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressSourceKind,
    local: unsafe extern "C" fn(*const std::ffi::c_void) -> u32,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::bind_address_identity"]
    fn b_bind_address_identity(kind: AddressIdentityKind, count: usize, mode: AddressOriginBindingMode,
        host: AddressIdentityHost) -> AddressIdentityResult;
    #[link_name = "bootstrap.call_arguments::bind_external_address_storage"]
    fn b_bind_external_address_storage(host: AddressStorageHost);
}

struct AddressStorage<'a, I> {
    sources: I,
    current: Option<(&'a view::Source, &'a bounds::Clause)>,
    output: view::Borrow,
}

impl<'a, I: Iterator<Item = (&'a view::Source, &'a bounds::Clause)>> AddressStorage<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressSourceKind {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.current = input.sources.next();
        match input.current {
            None => AddressSourceKind::End,
            Some((view::Source::Position(_), _)) => AddressSourceKind::Position,
            Some(_) => AddressSourceKind::Other,
        }
    }

    unsafe extern "C" fn local(context: *const std::ffi::c_void) -> u32 {
        let (view::Source::Position(origin), _) = unsafe { &*context.cast::<Self>() }.current.unwrap()
            else { unreachable!() };
        origin.has_local().into()
    }

    unsafe extern "C" fn emit(context: *mut std::ffi::c_void, external: u32) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (source, when) = input.current.take().unwrap();
        let source = if external != 0 { view::Source::External } else { source.clone() };
        let when = bounds::Clause::both(&bounds::Clause::default(), when);
        input.output.sources.union_when(source, &when);
    }

    fn project(mut self) -> view::Borrow {
        let host = AddressStorageHost { context: std::ptr::from_mut(&mut self).cast(),
            next: Self::next, local: Self::local, emit: Self::emit };
        unsafe { b_bind_external_address_storage(host) };
        self.output
    }
}

struct AddressIdentityBinding<'a> {
    identity: &'a Identity,
    bindings: AddressOriginBindings<'a>,
    ids: &'a [ValueIdentity],
    current: &'a [ValueIdentity],
    output: Option<Identity>,
    result_ids: Vec<ValueIdentity>,
}

impl AddressIdentityBinding<'_> {
    unsafe extern "C" fn address_at(context: *mut std::ffi::c_void, index: usize) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (AddressOriginBindings::Call(values) | AddressOriginBindings::Substitute(values)) = input.bindings
            else { unreachable!() };
        let Some(identity) = values.get(&input.ids[index]).and_then(|proof| proof.addresses.first()).and_then(Option::as_ref)
            else { return 0 };
        input.output = Some(identity.clone());
        1
    }

    unsafe extern "C" fn sources(context: *mut std::ffi::c_void, index: usize) -> AddressIdentitySources {
        let input = unsafe { &mut *context.cast::<Self>() };
        let id = &input.ids[index];
        let (present, current) = match input.bindings {
            AddressOriginBindings::Call(values) | AddressOriginBindings::Substitute(values) => {
                let proof = values.get(id);
                (proof.is_some(), proof.and_then(|proof| proof.origins.first()).map_or(&[][..], |row| row.as_slice()))
            }
            AddressOriginBindings::Outputs(values) => {
                let value = values.get(id);
                (value.is_some(), value.map_or(&[][..], std::slice::from_ref))
            }
        };
        input.current = current;
        AddressIdentitySources { present: present.into(), count: current.len() }
    }

    unsafe extern "C" fn append_source(context: *mut std::ffi::c_void, index: usize) {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.result_ids.push(input.current[index]);
    }

    unsafe extern "C" fn append_original(context: *mut std::ffi::c_void, index: usize) {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.result_ids.push(input.ids[index]);
    }

    unsafe extern "C" fn bind_storage(context: *mut std::ffi::c_void) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let Identity::Storage(borrow) = input.identity else { unreachable!() };
        let result = match input.bindings {
            AddressOriginBindings::Substitute(values) => borrow.substitute_values(values),
            AddressOriginBindings::Outputs(values) => borrow.output_bound(values),
            AddressOriginBindings::Call(values) => {
                let output = view::Borrow { inputs: borrow.inputs.clone(), calls: borrow.calls.clone(),
                    reads: borrow.reads.clone(), ..view::Borrow::default() };
                AddressStorage { sources: borrow.sources.borrowed_entries(), current: None, output }.project().at_call(values)
            }
        };
        input.output = Some(Identity::Storage(result));
    }

    fn bind(mut self, kind: AddressIdentityKind) -> Identity {
        let count = self.ids.len();
        let mode = self.bindings.mode();
        let host = AddressIdentityHost { context: std::ptr::from_mut(&mut self).cast(), address_at: Self::address_at,
            sources: Self::sources, append_source: Self::append_source, append_original: Self::append_original,
            bind_storage: Self::bind_storage };
        match unsafe { b_bind_address_identity(kind, count, mode, host) } {
            AddressIdentityResult::Original => self.identity.clone(),
            AddressIdentityResult::Bound => self.output.unwrap(),
            AddressIdentityResult::Values => Identity::Value(self.result_ids),
        }
    }
}

impl Identity {
    fn bind_address_identity(&self, bindings: AddressOriginBindings<'_>) -> Self {
        let (kind, ids) = match self {
            Self::Constant(_) => (AddressIdentityKind::Constant, &[][..]),
            Self::Value(ids) => (AddressIdentityKind::Values, ids.as_slice()),
            Self::Storage(_) => (AddressIdentityKind::Storage, &[][..]),
        };
        AddressIdentityBinding { identity: self, bindings, ids, current: &[], output: None, result_ids: Vec::new() }.bind(kind)
    }

    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        self.bind_address_identity(AddressOriginBindings::Substitute(values))
    }

    pub(super) fn output_bound(&self,bindings:&bounds::OutputBindings)->Self {
        self.bind_address_identity(AddressOriginBindings::Outputs(bindings))
    }
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        self.bind_address_identity(AddressOriginBindings::Call(bindings))
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
type KnownMemoryEffect = (Arc<Origin>, bounds::Clause, Access, Option<bounds::Proof>);

#[repr(C)]
struct NativeMemoryRecords<T> {
    records: *mut std::ffi::c_void,
    root: usize,
    count: usize,
    capacity: usize,
    contents: std::marker::PhantomData<T>,
}

impl<T> Default for NativeMemoryRecords<T> {
    fn default() -> Self {
        Self { records: std::ptr::null_mut(), root: 0, count: 0, capacity: 0, contents: std::marker::PhantomData }
    }
}

unsafe impl<T: Send> Send for NativeMemoryRecords<T> {}
unsafe impl<T: Sync> Sync for NativeMemoryRecords<T> {}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::push_memory_effect_record"]
    fn push_memory_effect_record(records: *mut std::ffi::c_void, width: usize, record: *const std::ffi::c_void);
    #[link_name = "bootstrap.call_arguments::append_memory_effect_records"]
    fn append_memory_effect_records(records: *mut std::ffi::c_void, source: *mut std::ffi::c_void, width: usize);
    #[link_name = "bootstrap.call_arguments::drop_memory_effect_records"]
    fn drop_memory_effect_records(records: *mut std::ffi::c_void, release: unsafe extern "C" fn(*mut std::ffi::c_void, usize));
}

impl<T> NativeMemoryRecords<T> {
    fn push(&mut self, record: T) {
        // The B process heap holds nonempty records aligned to the Windows CPU ABI.
        const { assert!(std::mem::size_of::<T>() != 0 && std::mem::align_of::<T>() <= 16); }
        let record = std::mem::ManuallyDrop::new(record);
        unsafe { push_memory_effect_record(std::ptr::from_mut(self).cast(), std::mem::size_of::<T>(), std::ptr::from_ref(&*record).cast()) };
    }

    fn append(&mut self, mut source: Self) {
        unsafe { append_memory_effect_records(std::ptr::from_mut(self).cast(), std::ptr::from_mut(&mut source).cast(), std::mem::size_of::<T>()) };
    }
}

impl<T> std::ops::Deref for NativeMemoryRecords<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        if self.count == 0 { return &[]; }
        // B publishes the count only after transferring each complete initialized record.
        unsafe { std::slice::from_raw_parts(self.records.cast(), self.count) }
    }
}

impl<T> Drop for NativeMemoryRecords<T> {
    fn drop(&mut self) {
        unsafe extern "C" fn release<T>(records: *mut std::ffi::c_void, count: usize) {
            let records = std::ptr::slice_from_raw_parts_mut(records.cast::<T>(), count);
            unsafe { std::ptr::drop_in_place(records) };
        }
        if self.capacity != 0 {
            unsafe { drop_memory_effect_records(std::ptr::from_mut(self).cast(), release::<T>) };
        }
    }
}

#[derive(Default)]
pub(super) struct MemoryEffect {
    external: Option<Access>,
    known: NativeMemoryRecords<KnownMemoryEffect>,
}

#[repr(C)]
struct MemoryEffectCoverage {
    count: usize,
    width: u32,
    request: Access,
    external: Access,
    external_present: u32,
    outside_element: u32,
}

#[repr(C)]
struct MemoryEffectCoverageHost {
    context: *const std::ffi::c_void,
    access: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> Access,
    same_element: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u32,
    project: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u8,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::memory_effect_bytes"]
    fn b_memory_effect_bytes(effect: MemoryEffectCoverage, host: MemoryEffectCoverageHost) -> u8;
}

struct EffectCoverage<'a> {
    effect: &'a MemoryEffect,
    position: &'a Position,
    scalar: Scalar,
    index: Option<bounds::Operand>,
}

impl EffectCoverage<'_> {
    unsafe extern "C" fn access(context: *const std::ffi::c_void, at: usize) -> Access {
        let query = unsafe { &*context.cast::<Self>() };
        query.effect.known[at].2
    }

    unsafe extern "C" fn same_element(context: *const std::ffi::c_void, at: usize) -> u32 {
        let query = unsafe { &*context.cast::<Self>() };
        let (origin, when, _, _) = &query.effect.known[at];
        u32::from(MemoryEffect::same_element(origin, when, query.position, query.index) == Some(true))
    }

    unsafe extern "C" fn project(context: *const std::ffi::c_void, at: usize) -> u8 {
        let query = unsafe { &*context.cast::<Self>() };
        let (origin, when, _, _) = &query.effect.known[at];
        MemoryEffect::origin_written_bytes(origin, when, query.position, query.scalar, query.index)
    }

    fn bytes(&self, request: Access, outside_element: bool) -> u8 {
        let effect = MemoryEffectCoverage {
            count: self.effect.known.len(),
            width: self.scalar.size(),
            request,
            external: self.effect.external.unwrap_or(Access::Value),
            external_present: u32::from(self.effect.external.is_some()),
            outside_element: u32::from(outside_element),
        };
        let host = MemoryEffectCoverageHost {
            context: std::ptr::from_ref(self).cast(),
            access: Self::access,
            same_element: Self::same_element,
            project: Self::project,
        };
        // B borrows this query only for the call and indexes the original entries by count.
        unsafe { b_memory_effect_bytes(effect, host) }
    }
}

#[repr(C)]
struct WrittenValueFacts { count: usize, mask: u64, bias: u64, width: u32, integer: u32, external: Access }

#[repr(C)]
struct WrittenValueSource { bytes: u8, proof: u32 }

#[repr(C)]
struct WrittenValueRegion { bytes: u8, offset: u32, size: u32 }

#[repr(C)]
struct WrittenValueBits { changed: u64, kept: u64, ordered: u64, exact: u32, partial_integer: u32 }

#[repr(C)]
struct WrittenValueHost {
    context: *mut std::ffi::c_void,
    access: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> Access,
    project: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> WrittenValueSource,
    regions: unsafe extern "C" fn(*mut std::ffi::c_void) -> usize,
    region: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> WrittenValueRegion,
    retain: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    merge: unsafe extern "C" fn(*mut std::ffi::c_void, WrittenValueBits),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::written_memory_value"]
    fn b_written_memory_value(facts: WrittenValueFacts, host: WrittenValueHost) -> u32;
}

struct WrittenValue<'a> {
    coverage: EffectCoverage<'a>,
    previous: &'a bounds::Proof,
    source: usize,
    regions: Option<Option<Vec<MemoryRegion>>>,
    incoming: Option<bounds::Proof>,
    combined: Option<(bounds::Proof, bounds::Proof)>,
}

impl WrittenValue<'_> {
    unsafe extern "C" fn access(context: *mut std::ffi::c_void, at: usize) -> Access {
        let query = unsafe { &*context.cast::<Self>() };
        query.coverage.effect.known[at].2
    }

    unsafe extern "C" fn project(context: *mut std::ffi::c_void, at: usize) -> WrittenValueSource {
        let query = unsafe { &mut *context.cast::<Self>() };
        query.source = at;
        let (origin, when, _, written) = &query.coverage.effect.known[at];
        let regions = std::cell::OnceCell::new();
        let bytes = MemoryEffect::project_written_bytes(origin, when, query.coverage.position,
            query.coverage.scalar, query.coverage.index, &|| regions.get_or_init(|| origin.memory_regions(when)).as_deref());
        query.regions = regions.into_inner();
        WrittenValueSource { bytes, proof: u32::from(written.is_some()) }
    }

    unsafe extern "C" fn regions(context: *mut std::ffi::c_void) -> usize {
        let query = unsafe { &mut *context.cast::<Self>() };
        let (origin, when, _, _) = &query.coverage.effect.known[query.source];
        query.regions.get_or_insert_with(|| origin.memory_regions(when)).as_ref().map_or(0, Vec::len)
    }

    unsafe extern "C" fn region(context: *mut std::ffi::c_void, at: usize) -> WrittenValueRegion {
        let query = unsafe { &*context.cast::<Self>() };
        let region = &query.regions.as_ref().unwrap().as_ref().unwrap()[at];
        let bytes = region.written_bytes(query.coverage.position, query.coverage.scalar);
        WrittenValueRegion { bytes, offset: region.offset, size: region.size }
    }

    unsafe extern "C" fn retain(context: *mut std::ffi::c_void, at: usize) -> u32 {
        let query = unsafe { &mut *context.cast::<Self>() };
        let region = &query.regions.as_ref().unwrap().as_ref().unwrap()[at];
        let (origin, _, _, written) = &query.coverage.effect.known[query.source];
        let Some(component) = origin.layout.parts.iter().position(|part| *part == (query.coverage.scalar, region.word)) else {
            return 0;
        };
        let proof = written.as_ref().unwrap().project_numeric_at_call(&[component], origin.layout.parts.len(), &BTreeMap::new());
        if let Some(incoming) = &mut query.incoming {
            incoming.join(&proof, 1, None);
        } else {
            query.incoming = Some(proof);
        }
        1
    }

    unsafe extern "C" fn merge(context: *mut std::ffi::c_void, bits: WrittenValueBits) {
        let query = unsafe { &mut *context.cast::<Self>() };
        let (old_numeric, result) = query.combined.get_or_insert_with(|| {
            let old_numeric = query.previous.numeric_at_call(&BTreeMap::new());
            let result = old_numeric.clone();
            (old_numeric, result)
        });
        let incoming = query.incoming.take();
        let proof = if bits.exact != 0 {
            incoming.unwrap()
        } else if bits.partial_integer != 0 {
            old_numeric.written_integer_bits(0, query.coverage.scalar, bits.changed, bits.kept, bits.ordered)
        } else {
            bounds::Proof::default()
        };
        result.join(&proof, 1, None);
    }

    fn value(mut self) -> bounds::Proof {
        let scalar = self.coverage.scalar;
        let facts = WrittenValueFacts {
            count: self.coverage.effect.known.len(),
            mask: scalar.bit_mask(),
            bias: scalar.order_bias(),
            width: scalar.size(),
            integer: u32::from(scalar.is_integer()),
            external: self.coverage.effect.external.unwrap_or(Access::Value),
        };
        let host = WrittenValueHost {
            context: std::ptr::from_mut(&mut self).cast(),
            access: Self::access,
            project: Self::project,
            regions: Self::regions,
            region: Self::region,
            retain: Self::retain,
            merge: Self::merge,
        };
        // The callbacks borrow the original sources and keep only the current source's temporary proof.
        if unsafe { b_written_memory_value(facts, host) } != 0 {
            return bounds::Proof::default();
        }
        self.combined.map_or_else(|| self.previous.clone(), |(_, result)| result)
    }
}

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

#[repr(C)]
struct OptionalMemoryAccess { access: Access, present: u32 }

impl From<Option<Access>> for OptionalMemoryAccess {
    fn from(value: Option<Access>) -> Self {
        Self { access: value.unwrap_or(Access::Value), present: u32::from(value.is_some()) }
    }
}

impl OptionalMemoryAccess {
    fn option(self) -> Option<Access> { (self.present != 0).then_some(self.access) }
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::merge_external_memory_access"]
    fn b_merge_external_memory_access(left: OptionalMemoryAccess, right: OptionalMemoryAccess) -> OptionalMemoryAccess;
}

fn merged_external_access(left: Option<Access>, right: Option<Access>) -> Option<Access> {
    unsafe { b_merge_external_memory_access(left.into(), right.into()) }.option()
}
#[repr(u32)]
enum EffectSourceKind { End, Position, External, Other }

#[repr(C)]
struct EffectSourceView { kind: EffectSourceKind, writable: u32 }

impl EffectSourceView {
    fn of(source: Option<&view::Source>) -> Self {
        let (kind, writable) = match source {
            None => (EffectSourceKind::End, false),
            Some(view::Source::Position(origin)) => (EffectSourceKind::Position, origin.writable),
            Some(view::Source::External) => (EffectSourceKind::External, false),
            _ => (EffectSourceKind::Other, false),
        };
        Self { kind, writable: u32::from(writable) }
    }
}

#[repr(C)]
struct EffectSourceHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> EffectSourceView,
    possible: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    external: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    record: unsafe extern "C" fn(*mut std::ffi::c_void, Access, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::collect_memory_effect_sources"]
    fn b_collect_memory_effect_sources(request: Access, host: EffectSourceHost) -> u32;
}

struct EffectSources<'a, 'b, I, P, W> {
    sources: I,
    path: std::cell::LazyCell<bounds::Clause, P>,
    written: std::cell::LazyCell<Option<&'b bounds::Proof>, W>,
    current: Option<(&'a view::Source, bounds::Clause)>,
    records: NativeMemoryRecords<KnownMemoryEffect>,
}

impl<'a, 'b, I, P, W> EffectSources<'a, 'b, I, P, W>
where
    I: Iterator<Item = (&'a view::Source, bounds::Clause)>,
    P: FnOnce() -> bounds::Clause,
    W: FnOnce() -> Option<&'b bounds::Proof>,
{
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> EffectSourceView {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.current = input.sources.next();
        EffectSourceView::of(input.current.as_ref().map(|(source, _)| *source))
    }

    unsafe extern "C" fn possible(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (_, when) = input.current.as_mut().unwrap();
        *when = bounds::Clause::both(when, &input.path);
        u32::from(when.possible())
    }

    unsafe extern "C" fn external(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &*context.cast::<Self>() };
        let (source, when) = input.current.as_ref().unwrap();
        u32::from(source_has_external_memory(source, || Some(when)))
    }

    unsafe extern "C" fn record(context: *mut std::ffi::c_void, access: Access, retain_value: u32) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (view::Source::Position(origin), when) = input.current.take().unwrap() else { unreachable!() };
        let written = if retain_value != 0 { input.written.cloned() } else { None };
        input.records.push((origin.clone(), when, access, written));
    }

    fn collect(mut self, access: Access) -> MemoryEffect {
        let host = EffectSourceHost {
            context: std::ptr::from_mut(&mut self).cast(),
            next: Self::next,
            possible: Self::possible,
            external: Self::external,
            record: Self::record,
        };
        // B advances this borrowed source iterator and publishes only retained records.
        let external = unsafe { b_collect_memory_effect_sources(access, host) };
        MemoryEffect { external: (external != 0).then_some(access), known: self.records }
    }
}

#[repr(C)]
struct HeldPosition { access: Access, whole_dataset: u32 }

#[repr(C)]
struct RegionAccess { bytes: u8, offset: u32, size: u32 }

#[repr(C)]
struct AccessConflictHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> EffectSourceKind,
    possible: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    held_count: unsafe extern "C" fn(*mut std::ffi::c_void) -> usize,
    held: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> HeldPosition,
    descendant: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    region_count: unsafe extern "C" fn(*mut std::ffi::c_void) -> usize,
    region: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> RegionAccess,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::memory_access_conflicts"]
    fn b_memory_access_conflicts(access: Access, host: AccessConflictHost) -> u32;
}

struct AccessConflictPosition {
    effect: MemoryEffect,
    owners: view::Borrow,
    index: Option<bounds::Operand>,
    regions: Option<Vec<MemoryRegion>>,
}

struct AccessConflict<'a, I> {
    held: &'a view::Borrow,
    accessed: I,
    path: &'a bounds::Clause,
    access: Access,
    current: Option<(&'a Origin, bounds::Clause)>,
    position: Option<AccessConflictPosition>,
}

impl<'a, I> AccessConflict<'a, I>
where
    I: Iterator<Item = (&'a view::Source, bounds::Clause)>,
{
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> EffectSourceKind {
        let query = unsafe { &mut *context.cast::<Self>() };
        query.position = None;
        query.current = None;
        match query.accessed.next() {
            Some((view::Source::Position(origin), when)) => {
                query.current = Some((origin, when));
                EffectSourceKind::Position
            }
            Some(_) => EffectSourceKind::Other,
            None => EffectSourceKind::End,
        }
    }

    unsafe extern "C" fn possible(context: *mut std::ffi::c_void) -> u32 {
        let query = unsafe { &mut *context.cast::<Self>() };
        let (_, when) = query.current.as_mut().unwrap();
        *when = bounds::Clause::both(query.path, when);
        u32::from(when.possible())
    }

    unsafe extern "C" fn held_count(context: *mut std::ffi::c_void) -> usize {
        let query = unsafe { &mut *context.cast::<Self>() };
        let (origin, when) = query.current.as_ref().unwrap();
        let index = match &origin.root {
            Root::Resource { index, .. } | Root::Dataset { index, .. } => *index,
            _ => None,
        };
        let effect = MemoryEffect::from_sources(query.held.sources.entries(), || when.clone(), Access::Write, || None);
        let count = effect.known.len();
        query.position = Some(AccessConflictPosition { effect, owners: origin.owners(), index, regions: None });
        count
    }

    unsafe extern "C" fn held(context: *mut std::ffi::c_void, at: usize) -> HeldPosition {
        let query = unsafe { &*context.cast::<Self>() };
        let (held, _, access, _) = &query.position.as_ref().unwrap().effect.known[at];
        HeldPosition { access: *access, whole_dataset: u32::from(matches!(held.root, Root::Dataset { index: None, .. })) }
    }

    unsafe extern "C" fn descendant(context: *mut std::ffi::c_void, at: usize) -> u32 {
        let query = unsafe { &*context.cast::<Self>() };
        let position = query.position.as_ref().unwrap();
        let (held, active, _, _) = &position.effect.known[at];
        let Root::Dataset { borrow, .. } = &held.root else { unreachable!() };
        let held = borrow.resources();
        u32::from(position.owners.sources.entries().any(|(owner, path)|
            held.overlaps_source(owner, &bounds::Clause::both(active, &path))))
    }

    unsafe extern "C" fn region_count(context: *mut std::ffi::c_void) -> usize {
        let query = unsafe { &mut *context.cast::<Self>() };
        let (origin, when) = query.current.as_ref().unwrap();
        let regions = origin.memory_regions(when);
        let count = regions.as_ref().map_or(0, Vec::len);
        query.position.as_mut().unwrap().regions = regions;
        count
    }

    unsafe extern "C" fn region(context: *mut std::ffi::c_void, at: usize) -> RegionAccess {
        let query = unsafe { &*context.cast::<Self>() };
        let position = query.position.as_ref().unwrap();
        let region = &position.regions.as_ref().unwrap()[at];
        let bytes = position.effect.accessed_bytes(&region.position, Scalar::U64, position.index, query.access, false);
        RegionAccess { bytes, offset: region.offset, size: region.size }
    }

    fn conflicts(mut self) -> bool {
        let host = AccessConflictHost {
            context: std::ptr::from_mut(&mut self).cast(),
            next: Self::next,
            possible: Self::possible,
            held_count: Self::held_count,
            held: Self::held,
            descendant: Self::descendant,
            region_count: Self::region_count,
            region: Self::region,
        };
        // The B query borrows these entries only until the current conflict check ends.
        unsafe { b_memory_access_conflicts(self.access, host) != 0 }
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
        EffectSources {
            sources,
            path: std::cell::LazyCell::new(path),
            written: std::cell::LazyCell::new(written),
            current: None,
            records: NativeMemoryRecords::default(),
        }.collect(access)
    }

    pub fn union(&mut self,other:Self) {
        self.known.append(other.known);
        self.external = merged_external_access(self.external, other.external);
    }
    fn same_element(origin:&Origin,when:&bounds::Clause,position:&Position,index:Option<bounds::Operand>)->Option<bool> {
        let (Some(index),Position::Borrowed(borrow,layout,_))=(index,position)else{return None;};
        let(Root::Resource{borrow:source,index:Some(written)}|Root::Dataset{borrow:source,index:Some(written)})=&origin.root else{return None;};
        if origin.base!=*layout||!same_position_resource(source,borrow){return None;}
        bounds::Condition::index_equal(*written,index).truth_in(when)
    }
    // The loop owner uses this query only for a forward cursor's preceding iterations.
    pub(super) fn outside_element_bytes(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        self.accessed_bytes(position, scalar, index, Access::Read, true)
    }
    fn origin_written_bytes(origin:&Origin,when:&bounds::Clause,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        let regions = std::cell::LazyCell::new(|| origin.memory_regions(when));
        Self::project_written_bytes(origin, when, position, scalar, index, &|| regions.as_deref())
    }

    fn project_written_bytes<'a>(
        origin: &Origin, when: &bounds::Clause, position: &Position, scalar: Scalar,
        index: Option<bounds::Operand>, regions: &impl Fn() -> Option<&'a [MemoryRegion]>,
    ) -> u8 {
        WriteProjection { origin, when, index, regions }.bytes(position, scalar.size())
    }
    pub(super) fn access_conflicts(
        held: &view::Borrow, accessed: &view::Borrow, access: Access, path: &bounds::Clause,
    ) -> bool {
        AccessConflict { held, accessed: accessed.sources.entries(), path, access, current: None, position: None }
            .conflicts()
    }
    // The content producer has already selected this position on a feasible write path.
    pub(super) fn position_may_write(origin: &Origin, when: &bounds::Clause, position: &Position) -> bool {
        position_has_external_memory(origin, || Some(when))
            || origin.writable && Self::origin_written_bytes(origin, when, position, Scalar::U64, None) != 0
    }

    pub(super) fn written_bytes(&self,position:&Position,scalar:Scalar,index:Option<bounds::Operand>)->u8 {
        self.accessed_bytes(position, scalar, index, Access::Read, false)
    }
    fn accessed_bytes(
        &self, position: &Position, scalar: Scalar, index: Option<bounds::Operand>, request: Access,
        outside_element: bool,
    ) -> u8 {
        EffectCoverage { effect: self, position, scalar, index }.bytes(request, outside_element)
    }
    pub(super) fn has_external_write(&self) -> bool {
        self.external.is_some_and(Access::writes)
    }

    // A possible write joins its value with the old value; it is never a must-write guarantee.
    pub(super) fn written_value(
        &self, position: &Position, scalar: Scalar, index: Option<bounds::Operand>, previous: &bounds::Proof,
    ) -> bounds::Proof {
        WrittenValue {
            coverage: EffectCoverage { effect: self, position, scalar, index },
            previous,
            source: 0,
            regions: None,
            incoming: None,
            combined: None,
        }.value()
    }
}

/// The IR only consumes the possible aliases and their access modes.
#[derive(Default)]
pub(super) struct MemoryAccesses {
    known: NativeMemoryRecords<MemoryIrRow>,
    external: Option<Access>,
}
#[repr(C)]
struct IrMemorySourceHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> EffectSourceView,
    external: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    origin: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, Access),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::collect_ir_memory_sources"]
    fn b_collect_ir_memory_sources(known: *mut std::ffi::c_void, external: OptionalMemoryAccess, request: Access,
        host: IrMemorySourceHost) -> OptionalMemoryAccess;
    #[link_name = "bootstrap.call_arguments::collect_closed_ir_memory"]
    fn b_collect_closed_ir_memory(known: *mut std::ffi::c_void, count: usize, external: OptionalMemoryAccess,
        incoming: OptionalMemoryAccess, context: *const std::ffi::c_void,
        origin: unsafe extern "C" fn(*const std::ffi::c_void, *mut std::ffi::c_void, usize)) -> OptionalMemoryAccess;
    #[link_name = "bootstrap.call_arguments::append_ir_memory_aliases"]
    fn b_append_ir_memory_aliases(known: *mut std::ffi::c_void, aliases: *const std::ffi::c_void, count: usize,
        access: Access, row: unsafe extern "C" fn(*const std::ffi::c_void, usize, Access) -> MemoryIrRow);
}

unsafe fn ir_memory_origin(known: *mut std::ffi::c_void, origin: &Origin, access: Access, possible: impl FnOnce() -> bool) {
    unsafe extern "C" fn row(aliases: *const std::ffi::c_void, at: usize, access: Access) -> MemoryIrRow {
        let alias = unsafe { *aliases.cast::<MemoryAlias>().add(at) };
        MemoryIrRow::new(alias, access)
    }

    // The temporary source aliases borrow the current record set only while projecting.
    let aliases = {
        let storage = unsafe { &*known.cast::<NativeMemoryRecords<MemoryIrRow>>() };
        origin.aliases_when(&|alias| unsafe {
            b_contains_ir_memory_record(storage.as_ptr(), storage.len(), MemoryIrRow::new(*alias, access)) == 0
        }, possible)
    };
    unsafe { b_append_ir_memory_aliases(known, aliases.as_ptr().cast(), aliases.len(), access, row) };
}

struct IrMemorySources<'a, I, P> {
    sources: I,
    path: std::cell::LazyCell<bounds::Clause, P>,
    current: Option<(&'a view::Source, bounds::Clause)>,
    active: std::cell::OnceCell<Option<bounds::Clause>>,
}

impl<'a, I, P> IrMemorySources<'a, I, P>
where
    I: Iterator<Item = (&'a view::Source, bounds::Clause)>,
    P: FnOnce() -> bounds::Clause,
{
    fn active(&self) -> Option<&bounds::Clause> {
        self.active.get_or_init(|| {
            let (_, when) = self.current.as_ref().unwrap();
            let when = bounds::Clause::both(when, &self.path);
            when.possible().then_some(when)
        }).as_ref()
    }

    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> EffectSourceView {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.active.take();
        input.current = input.sources.next();
        EffectSourceView::of(input.current.as_ref().map(|(source, _)| *source))
    }

    unsafe extern "C" fn external(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &*context.cast::<Self>() };
        let (source, _) = input.current.as_ref().unwrap();
        u32::from(source_has_external_memory(source, || input.active()))
    }

    unsafe extern "C" fn origin(context: *mut std::ffi::c_void, known: *mut std::ffi::c_void, access: Access) {
        let input = unsafe { &*context.cast::<Self>() };
        let (view::Source::Position(origin), _) = input.current.as_ref().unwrap() else { unreachable!() };
        unsafe { ir_memory_origin(known, origin, access, || input.active().is_some()) };
    }

    fn collect(mut self, output: &mut MemoryAccesses, request: Access) {
        let host = IrMemorySourceHost { context: std::ptr::from_mut(&mut self).cast(), next: Self::next,
            external: Self::external, origin: Self::origin };
        output.external = unsafe { b_collect_ir_memory_sources(std::ptr::from_mut(&mut output.known).cast(),
            output.external.into(), request, host) }.option();
    }
}

impl MemoryAccesses {
    pub(super) fn record(&mut self, effect: &MemoryEffect) {
        unsafe extern "C" fn origin(context: *const std::ffi::c_void, known: *mut std::ffi::c_void, index: usize) {
            let effect = unsafe { &*context.cast::<MemoryEffect>() };
            let (origin, _, access, _) = &effect.known[index];
            unsafe { ir_memory_origin(known, origin, *access, || true) };
        }
        self.external = unsafe { b_collect_closed_ir_memory(std::ptr::from_mut(&mut self.known).cast(),
            effect.known.len(), self.external.into(), effect.external.into(), std::ptr::from_ref(effect).cast(), origin) }.option();
    }

    pub(super) fn extend_sources<'a>(
        &mut self,
        sources: impl Iterator<Item = (&'a view::Source, bounds::Clause)>,
        path: impl FnOnce() -> bounds::Clause,
        access: Access,
    ) {
        IrMemorySources { sources, path: std::cell::LazyCell::new(path), current: None, active: std::cell::OnceCell::new() }
            .collect(self, access);
    }

    fn call_effects(&self, locals: &BTreeMap<usize, Ty>) -> Vec<MemoryAccess> {
        let mut output = Vec::new();
        IrMemoryRows { source: self, locals: locals.iter() }.materialize(
            MemoryIrOutput::Call, std::ptr::from_mut(&mut output).cast(), IrMemoryRows::effect,
        );
        output
    }

    fn pointer_aliases(&self, locals: &BTreeMap<usize, Ty>) -> Vec<MemoryAlias> {
        let mut output = Vec::new();
        IrMemoryRows { source: self, locals: locals.iter() }.materialize(
            MemoryIrOutput::Pointer, std::ptr::from_mut(&mut output).cast(), IrMemoryRows::alias,
        );
        output
    }
}

#[repr(u32)]
enum MemoryIrRowKind { Local, Data }

#[repr(u32)]
enum MemoryIrOutput { Call, Pointer }

#[repr(C)]
struct MemoryIrRow { kind: MemoryIrRowKind, access: Access, base: usize, length: usize }

impl MemoryIrRow {
    fn new(alias: MemoryAlias, access: Access) -> Self {
        match alias {
            MemoryAlias::Local { base, len } => Self { kind: MemoryIrRowKind::Local, access, base, length: len },
            MemoryAlias::Data(base) => Self { kind: MemoryIrRowKind::Data, access, base, length: 0 },
        }
    }

    fn alias(&self) -> MemoryAlias {
        match self.kind {
            MemoryIrRowKind::Local => MemoryAlias::Local { base: self.base, len: self.length },
            MemoryIrRowKind::Data => MemoryAlias::Data(self.base),
        }
    }
}

#[repr(u32)]
enum MemoryAliasRoot { Other, Local, Data, Borrow }

#[repr(C)]
struct MemoryAliasOrigin { kind: MemoryAliasRoot, base: usize, length: usize, borrow: *const std::ffi::c_void }

impl MemoryAliasOrigin {
    fn of(origin: &Origin) -> Self {
        let empty = std::ptr::null();
        match &origin.root {
            Root::Local { base, len } => Self { kind: MemoryAliasRoot::Local, base: *base, length: *len, borrow: empty },
            Root::Buffer(binding) => Self { kind: MemoryAliasRoot::Data, base: *binding, length: 0, borrow: empty },
            Root::Derived(borrow) | Root::Dataset { borrow, .. } | Root::External(Identity::Storage(borrow)) => Self::borrow(borrow),
            _ => Self { kind: MemoryAliasRoot::Other, base: 0, length: 0, borrow: empty },
        }
    }

    fn borrow(borrow: &view::Borrow) -> Self {
        Self { kind: MemoryAliasRoot::Borrow, base: 0, length: 0, borrow: std::ptr::from_ref(borrow).cast() }
    }
}

#[repr(C)]
pub(super) struct MemoryAliasCollector {
    keys: *mut std::ffi::c_void,
    parent: *const std::ffi::c_void,
    context: *const std::ffi::c_void,
    needed: unsafe extern "C" fn(*const std::ffi::c_void, MemoryIrRow) -> u32,
}

#[repr(C)]
struct MemoryAliasOriginHost {
    context: *mut std::ffi::c_void,
    possible: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    borrow: unsafe extern "C" fn(*const std::ffi::c_void, MemoryAliasCollector),
}

#[repr(C)]
struct MemoryAliasBorrowHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> EffectSourceKind,
    position: unsafe extern "C" fn(*mut std::ffi::c_void, MemoryAliasCollector),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::collect_memory_origin_aliases"]
    fn b_collect_memory_origin_aliases(origin: MemoryAliasOrigin, collector: MemoryAliasCollector, host: MemoryAliasOriginHost);
    #[link_name = "bootstrap.call_arguments::collect_memory_borrow_aliases"]
    fn b_collect_memory_borrow_aliases(collector: MemoryAliasCollector, host: MemoryAliasBorrowHost);
    #[link_name = "bootstrap.call_arguments::project_memory_aliases"]
    fn b_project_memory_aliases(origin: MemoryAliasOrigin, host: MemoryAliasOriginHost, context: *const std::ffi::c_void,
        needed: unsafe extern "C" fn(*const std::ffi::c_void, MemoryIrRow) -> u32, output: *mut std::ffi::c_void,
        emit: unsafe extern "C" fn(*mut std::ffi::c_void, MemoryIrRow));
}

struct AliasPossible<F>(Option<F>);

impl<F: FnOnce() -> bool> AliasPossible<F> {
    unsafe extern "C" fn possible(context: *mut std::ffi::c_void) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        u32::from(input.0.take().unwrap()())
    }

    unsafe extern "C" fn borrow(borrow: *const std::ffi::c_void, collector: MemoryAliasCollector) {
        let borrow = unsafe { &*borrow.cast::<view::Borrow>() };
        borrow.collect_memory_aliases(collector);
    }

    fn host(&mut self) -> MemoryAliasOriginHost {
        MemoryAliasOriginHost { context: std::ptr::from_mut(self).cast(), possible: Self::possible, borrow: Self::borrow }
    }
}

fn project_memory_aliases(origin: MemoryAliasOrigin, needed: &dyn Fn(&MemoryAlias) -> bool, possible: impl FnOnce() -> bool) -> Vec<MemoryAlias> {
    unsafe extern "C" fn is_needed(context: *const std::ffi::c_void, alias: MemoryIrRow) -> u32 {
        let needed = unsafe { &*context.cast::<&dyn Fn(&MemoryAlias) -> bool>() };
        u32::from(needed(&alias.alias()))
    }

    let mut possible = AliasPossible(Some(possible));
    let mut aliases = Vec::new();
    // B borrows each source and predicate only for this synchronous traversal.
    unsafe { b_project_memory_aliases(origin, possible.host(), std::ptr::from_ref(&needed).cast(), is_needed,
        std::ptr::from_mut(&mut aliases).cast(), IrMemoryRows::alias) };
    aliases
}

pub(super) fn memory_borrow_aliases(borrow: &view::Borrow, needed: &dyn Fn(&MemoryAlias) -> bool) -> Vec<MemoryAlias> {
    project_memory_aliases(MemoryAliasOrigin::borrow(borrow), needed, || true)
}

struct BorrowMemoryAliases<'a, I> { sources: I, selected: Option<(&'a Origin, &'a bounds::Clause)> }

impl<'a, I: Iterator<Item = (&'a view::Source, &'a bounds::Clause)>> BorrowMemoryAliases<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> EffectSourceKind {
        let input = unsafe { &mut *context.cast::<Self>() };
        match input.sources.next() {
            Some((view::Source::Position(origin), when)) => {
                input.selected = Some((origin, when));
                EffectSourceKind::Position
            }
            Some(_) => EffectSourceKind::Other,
            None => EffectSourceKind::End,
        }
    }

    unsafe extern "C" fn position(context: *mut std::ffi::c_void, collector: MemoryAliasCollector) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (origin, when) = input.selected.take().unwrap();
        let mut possible = AliasPossible(Some(|| when.possible()));
        unsafe { b_collect_memory_origin_aliases(MemoryAliasOrigin::of(origin), collector, possible.host()) };
    }

    fn collect(&mut self, collector: MemoryAliasCollector) {
        let host = MemoryAliasBorrowHost { context: std::ptr::from_mut(self).cast(), next: Self::next, position: Self::position };
        unsafe { b_collect_memory_borrow_aliases(collector, host) };
    }
}

pub(super) fn collect_borrow_aliases<'a>(sources: impl Iterator<Item = (&'a view::Source, &'a bounds::Clause)>, collector: MemoryAliasCollector) {
    BorrowMemoryAliases { sources, selected: None }.collect(collector);
}

#[repr(C)]
struct MemoryIrLocal { end: usize, length: usize }

#[repr(C)]
struct MemoryIrRows { known: *const MemoryIrRow, known_count: usize, locals: usize, external: Access, external_present: u32 }

#[repr(C)]
struct MemoryIrRowsHost {
    context: *mut std::ffi::c_void,
    local: unsafe extern "C" fn(*mut std::ffi::c_void) -> MemoryIrLocal,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, MemoryIrRow),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::contains_ir_memory_record"]
    fn b_contains_ir_memory_record(known: *const MemoryIrRow, count: usize, sought: MemoryIrRow) -> u32;
    #[link_name = "bootstrap.call_arguments::materialize_ir_memory"]
    fn b_materialize_ir_memory(rows: MemoryIrRows, format: MemoryIrOutput, output: *mut std::ffi::c_void, host: MemoryIrRowsHost);
}

struct IrMemoryRows<'a> {
    source: &'a MemoryAccesses,
    locals: std::collections::btree_map::Iter<'a, usize, Ty>,
}

impl IrMemoryRows<'_> {
    unsafe extern "C" fn local(context: *mut std::ffi::c_void) -> MemoryIrLocal {
        let input = unsafe { &mut *context.cast::<Self>() };
        let (&end, ty) = input.locals.next().unwrap();
        MemoryIrLocal { end, length: ty.components.len() }
    }

    unsafe extern "C" fn effect(output: *mut std::ffi::c_void, row: MemoryIrRow) {
        unsafe { &mut *output.cast::<Vec<MemoryAccess>>() }.push(MemoryAccess { alias: row.alias(), access: row.access });
    }

    unsafe extern "C" fn alias(output: *mut std::ffi::c_void, row: MemoryIrRow) {
        unsafe { &mut *output.cast::<Vec<MemoryAlias>>() }.push(row.alias());
    }

    fn materialize(mut self, format: MemoryIrOutput, output: *mut std::ffi::c_void, emit: unsafe extern "C" fn(*mut std::ffi::c_void, MemoryIrRow)) {
        let rows = MemoryIrRows {
            known: self.source.known.as_ptr(),
            known_count: self.source.known.len(),
            locals: self.locals.len(),
            external: self.source.external.unwrap_or(Access::Value),
            external_present: u32::from(self.source.external.is_some()),
        };
        let host = MemoryIrRowsHost { context: std::ptr::from_mut(&mut self).cast(), local: Self::local, emit };
        // The selected emitter writes the actual IR field's element type through this call only.
        unsafe { b_materialize_ir_memory(rows, format, output, host) }
    }
}

#[repr(u32)]
enum MemoryIrOwner { Instruction, Argument }

#[repr(C)]
struct MemoryIrHost {
    ir: dataflow::Host,
    call: unsafe extern "C" fn(*const std::ffi::c_void, *mut std::ffi::c_void) -> dataflow::Span,
    pointer: unsafe extern "C" fn(*const std::ffi::c_void, *mut std::ffi::c_void, MemoryIrOwner, Reg),
    child: unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> dataflow::Span,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::publish_ir_memory"]
    fn b_publish_ir_memory(code: dataflow::Span, context: *const std::ffi::c_void, host: MemoryIrHost);
}

struct MemoryPublication<'a> {
    memory: &'a BTreeMap<MemoryTarget, MemoryAccesses>,
    locals: &'a BTreeMap<usize, Ty>,
}

impl MemoryPublication<'_> {
    unsafe extern "C" fn call(context: *const std::ffi::c_void, owner: *mut std::ffi::c_void) -> dataflow::Span {
        let input = unsafe { &*context.cast::<Self>() };
        let instruction = unsafe { &mut *owner.cast::<Inst>() };
        let InstKind::Call(call) = &mut instruction.kind else { unreachable!() };
        call.memory = call.memory_use.and_then(|usage| input.memory.get(&MemoryTarget::Call(usage)))
            .map(|effect| effect.call_effects(input.locals)).unwrap_or_default();
        dataflow::Span::mutable(&mut call.arguments)
    }

    unsafe extern "C" fn pointer(
        context: *const std::ffi::c_void, owner: *mut std::ffi::c_void, kind: MemoryIrOwner, register: Reg,
    ) {
        fn reference(value: &mut CallReference) -> &mut IndexedSource {
            let CallStorage::Indexed(source) = &mut value.storage else { unreachable!() };
            source
        }

        let input = unsafe { &*context.cast::<Self>() };
        let source = match kind {
            MemoryIrOwner::Instruction => {
                let instruction = unsafe { &mut *owner.cast::<Inst>() };
                match &mut instruction.kind {
                    InstKind::LoadIndexed { source, .. } | InstKind::StoreIndexed { source, .. } => source,
                    InstKind::Atomic(value) => &mut value.target,
                    InstKind::PositionAddress { address, .. } => reference(address),
                    _ => unreachable!(),
                }
            }
            MemoryIrOwner::Argument => {
                let argument = unsafe { &mut *owner.cast::<CallArgument>() };
                let CallArgument::Reference { address, .. } = argument else { unreachable!() };
                reference(address)
            }
        };
        let IndexedSource::Pointer { aliases, .. } = source else { unreachable!() };
        *aliases = input.memory.get(&MemoryTarget::Value(register))
            .map(|effect| effect.pointer_aliases(input.locals)).unwrap_or_default();
    }

    unsafe extern "C" fn child(owner: *mut std::ffi::c_void, index: u32) -> dataflow::Span {
        let instruction = unsafe { &mut *owner.cast::<Inst>() };
        let body = match &mut instruction.kind {
            InstKind::Scope(body) | InstKind::Cleanup { body, .. } => body,
            InstKind::If { yes, no, .. } | InstKind::Loop { condition: yes, body: no, .. } =>
                if index == 0 { yes } else { no },
            _ => unreachable!(),
        };
        dataflow::Span::mutable(body)
    }
}

pub(super) fn apply_memory(code: &mut [Inst], memory: &BTreeMap<MemoryTarget, MemoryAccesses>, locals: &BTreeMap<usize, Ty>) {
    let input = MemoryPublication { memory, locals };
    let host = MemoryIrHost { ir: dataflow::HOST, call: MemoryPublication::call, pointer: MemoryPublication::pointer,
        child: MemoryPublication::child };
    // Mutable destinations come from the original spans. The IR inspector only lends read-only views.
    unsafe { b_publish_ir_memory(dataflow::Span::mutable(code), std::ptr::from_ref(&input).cast(), host) }
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

#[repr(u32)]
enum BorrowedPositionMatch { Unavailable, Equal, Different }

#[repr(C)]
struct BorrowedPositionHost {
    context: *mut std::ffi::c_void,
    next: unsafe extern "C" fn(*mut std::ffi::c_void) -> AddressSourceEntry,
    remember: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    compare: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> BorrowedPositionMatch,
    forget: unsafe extern "C" fn(*mut std::ffi::c_void, usize),
    publish: unsafe extern "C" fn(*mut std::ffi::c_void, usize, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_arguments::borrowed_address_positions"]
    fn b_borrowed_address_positions(fields: usize, host: BorrowedPositionHost);
}

struct BorrowedPositions<'a, I> {
    borrow: &'a view::Borrow,
    layout: &'a Layout,
    words: &'a [u32],
    sources: PositionSources<'a, I>,
    known: Vec<Option<Position>>,
    output: Vec<Position>,
}

impl<'a, I: Iterator<Item = (&'a view::Source, bounds::Clause)>> BorrowedPositions<'a, I> {
    unsafe extern "C" fn next(context: *mut std::ffi::c_void) -> AddressSourceEntry {
        unsafe { &mut *context.cast::<Self>() }.sources.next()
    }

    unsafe extern "C" fn remember(context: *mut std::ffi::c_void, field: usize) -> u32 {
        let input = unsafe { &mut *context.cast::<Self>() };
        input.known[field] = input.sources.parent.unwrap().position_at(input.words[field]);
        input.known[field].is_some().into()
    }

    unsafe extern "C" fn compare(context: *mut std::ffi::c_void, field: usize) -> BorrowedPositionMatch {
        let input = unsafe { &*context.cast::<Self>() };
        let Some(known) = input.known[field].as_ref() else { return BorrowedPositionMatch::Unavailable; };
        let candidate = input.sources.parent.unwrap().position_at(input.words[field]);
        if candidate.as_ref() == Some(known) { BorrowedPositionMatch::Equal } else { BorrowedPositionMatch::Different }
    }

    unsafe extern "C" fn forget(context: *mut std::ffi::c_void, field: usize) {
        unsafe { &mut *context.cast::<Self>() }.known[field] = None;
    }

    unsafe extern "C" fn publish(context: *mut std::ffi::c_void, field: usize, retain: u32) {
        let input = unsafe { &mut *context.cast::<Self>() };
        let value = if retain != 0 { input.known[field].take() } else { None };
        input.output.push(value.unwrap_or_else(||
            Position::Borrowed(Arc::new(input.borrow.clone()), input.layout.clone(), input.words[field])));
    }

    fn collect(mut self) -> Vec<Position> {
        let fields = self.words.len();
        let host = BorrowedPositionHost { context: std::ptr::from_mut(&mut self).cast(), next: Self::next,
            remember: Self::remember, compare: Self::compare, forget: Self::forget, publish: Self::publish };
        unsafe { b_borrowed_address_positions(fields, host) };
        self.output
    }
}

pub(super) fn borrowed_positions(borrow: &view::Borrow, layout: &Layout, words: &[u32]) -> Vec<Position> {
    let sources = PositionSources { entries: borrow.sources.entries(), parent: None };
    BorrowedPositions { borrow, layout, words, sources, known: vec![None; words.len()],
        output: Vec::with_capacity(words.len()) }.collect()
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
