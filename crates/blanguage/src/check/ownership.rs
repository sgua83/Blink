//! 独占位置保留原存储：元素函数静态投影，CPU 普通调用传递实际借用地址。
use super::*;
#[path="ownership/position.rs"]
mod position;
#[path="ownership/place.rs"]
mod place;
use position::PositionChild;

#[derive(Clone, Copy)]
pub(super) enum ApplicationIndex {
    Constant(u32),
    Value(ValueIdentity,Reg),
}
impl ApplicationIndex {
    pub(super) fn of(value: &Value) -> Self {
        if value.minimum() == value.maximum() {
            Self::Constant(value.minimum())
        } else {
            Self::Value(value.identity(),value.regs[0])
        }
    }
}
impl PartialEq for ApplicationIndex {fn eq(&self,other:&Self)->bool{self.cmp(other).is_eq()}}
impl Eq for ApplicationIndex {}
impl PartialOrd for ApplicationIndex {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for ApplicationIndex {fn cmp(&self,other:&Self)->std::cmp::Ordering{match(self,other){(Self::Constant(a),Self::Constant(b))=>a.cmp(b),(Self::Value(a,_),Self::Value(b,_))=>a.cmp(b),(Self::Constant(_),Self::Value(..))=>std::cmp::Ordering::Less,_=>std::cmp::Ordering::Greater}}}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Position {
    Initialized,
    Local(usize),
    Borrowed(Arc<view::Borrow>,address::Layout,u32),
    Initialization(Arc<address::InitializationEffect>),
    Element(usize, u32),
    ViewElement([ValueIdentity; 3], Reg, ElementStride, u32),
    TileElement(usize, Reg, u32),
    Parameter(usize, usize),
    ApplicationElement(usize, ApplicationIndex, u32),
    ApplicationInput(view::LoopInput, Option<ApplicationIndex>, u32),
    Indexed(SharedPositions, ApplicationIndex),
    Conditional(Arc<owned_presence::Presence>, PositionChild, PositionChild),
    Both(PositionChild, PositionChild),
}
impl Position {
    pub(super) fn index(positions:Vec<Self>,index:ApplicationIndex,width:usize)->Vec<Self>{position::indexed(positions,index,width)}
    pub(super) fn selected(when:owned_presence::Presence,a:Self,b:Self)->Self{position::selected(when,a,b)}
    pub(super) fn both(a:Self,b:Self)->Self{position::both(a,b)}
    pub(super) fn initialized_by(&self,initialized:&BTreeSet<Position>)->bool{position::initialized(self,initialized)}
    pub(super) fn application_word(&self,index:Option<ApplicationIndex>,word:u32)->Self{position::word(self,index,word)}
    pub(super) fn dataset_position(&self, index: bounds::Operand) -> Option<Self> { position::dataset_position(self, index) }
    pub(super) fn application_word_initialized(&self,index:Option<ApplicationIndex>,word:u32,initialized:&BTreeSet<Position>)->bool {
        position::word_initialized(self,index,word,initialized)
    }
    pub(super) fn application_prefix_initialized(&self,length:u32,word:u32,initialized:&BTreeSet<Position>)->bool{position::prefix(self,length,word,initialized)}
    pub(super) fn uses_scope(&self,scope:usize)->bool{position::scope(self,scope)}
}
pub(super) use position::{initialization_read_borrow, initialization_read_complete, merge_initialization, project_dataset_output, resolve_initialization, SharedPositions};
impl Place {
    pub(super) fn data_positions(&self)->Vec<Position>{place::data(self)}
    pub(super) fn owned_positions(&self)->Option<Vec<Position>>{place::owned(self)}
    fn has_owned_positions(&self)->bool{place::has_owned(self)}
    pub(super) fn ty(&self) -> &Ty {
        match self {
            Self::Reference { ty, .. }
            | Self::AtomicParameter(ty)
            | Self::Local(ty, _, ..)
            | Self::ReadOnlyLocal(ty, _, ..)
            | Self::Indexed { ty, .. }
            | Self::Element { ty, .. }
            | Self::TileElement { ty, .. }
            | Self::ViewElement { ty, .. } => ty,
            Self::ApplicationElement { ty, .. } => ty,
        }
    }
    pub(super) fn components_in(&self,owner:&Place)->Option<Vec<usize>>{place::components(self,owner)}
    pub(super) fn overlaps(&self,other:&Place)->bool{place::overlaps(self,other)}

    pub(super) fn storage_components(&self) -> StorageComponents<'_> {
        match self {
            Self::Local(_, slots, _) | Self::ReadOnlyLocal(_, slots, ..) => StorageComponents::Slots(slots),
            Self::Reference { words, .. } | Self::Indexed { words, .. } | Self::ViewElement { words, .. }
            | Self::TileElement { words, .. } | Self::Element { words, .. }
            | Self::ApplicationElement { words, .. } => StorageComponents::Words(words),
            Self::AtomicParameter(_) => StorageComponents::Words(&[]),
        }
    }
}

pub(super) enum StorageComponents<'a> {
    Slots(&'a [usize]),
    Words(&'a [u32]),
}

impl StorageComponents<'_> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Slots(slots) => slots.len(),
            Self::Words(words) => words.len(),
        }
    }

    pub(super) fn at(&self, index: usize) -> usize {
        match self {
            Self::Slots(slots) => slots[index],
            Self::Words(words) => words[index] as usize,
        }
    }
}

pub(super) struct StorageAccess {
    pub kind: u32,
    pub access: Access,
    pub writable: bool,
}

#[repr(C)]
struct WritableStorage { reference: u32, permitted: u32 }

#[repr(C)]
struct MutablePlaceCursor { phase: u32, component: u64 }

#[repr(C)]
struct MutablePlaceFacts { access: u32, persistent: u32, views: u32, unique: u32, components: u64 }

#[repr(u32)]
enum MutablePlaceNeed { ViewError, Owned, Writable, Unique, Duplicate, Initialize, Complete }

#[repr(C)]
struct MutablePlaceStep { cursor: MutablePlaceCursor, need: MutablePlaceNeed }

unsafe extern "C" {
    #[link_name = "bootstrap.function_body::writable_storage"]
    fn writable_storage(kind: u32, access: u32, current: u32) -> WritableStorage;
    #[link_name = "bootstrap.function_body::mutable_place_step"]
    fn mutable_place_step(cursor: MutablePlaceCursor, facts: MutablePlaceFacts) -> MutablePlaceStep;
}
impl Builder<'_> {
    pub(super) fn storage_access(&self, base: &Place) -> StorageAccess {
        let (kind, access, writable) = match base {
            Place::Reference { access, .. } => (0, *access, false),
            Place::ApplicationElement { access, .. } => (1, *access, false),
            Place::ViewElement { .. } => (3, Access::Read, false),
            Place::TileElement { tile, .. } => (4, self.tiles[*tile].access, false),
            Place::Local(..) => (5, Access::Value, true),
            Place::ReadOnlyLocal(..) => (6, Access::Read, false),
            Place::Element { binding, writable, .. } => (7, self.view_access[binding], *writable),
            Place::AtomicParameter(_) => (8, Access::Atomic, false),
            Place::Indexed { .. } => unreachable!(),
        };
        StorageAccess { kind, access, writable }
    }

    pub(super) async fn mutable_place(&mut self, expr: &Expr, access: Access) -> Result<Option<Place>> {
        let Some(place) = unsafe { self.world.call(self.place(expr)).await }? else {
            return Ok(None);
        };
        unsafe { self.world.call(self.mutable_place_value(place,access,expr.span())).await }.map(Some)
    }
    pub(super) async fn mutable_place_value(&mut self,place:Place,access:Access,span:Span)->Result<Place> {
        let components = place.storage_components();
        let mut cursor = MutablePlaceCursor { phase: 0, component: 0 };
        let mut facts = MutablePlaceFacts {
            access: access as u32,
            persistent: u32::from(matches!(place.storage(), Place::Element { .. } | Place::ApplicationElement { .. })),
            views: u32::from(place.ty().contains_views()),
            unique: 0,
            components: components.len() as u64,
        };
        let mut seen = BTreeSet::new();
        loop {
            let step = unsafe { mutable_place_step(cursor, MutablePlaceFacts { ..facts }) };
            cursor = step.cursor;
            match step.need {
                MutablePlaceNeed::ViewError => return self.fail(span, "B185", "持久dataset的view载荷不可写或独占借出；数值字段按原权限修改"),
                MutablePlaceNeed::Owned => self.require_owned_writable(&place, span)?,
                MutablePlaceNeed::Writable => { unsafe { self.world.call(self.writable_storage(&place, span, None)).await }?; }
                MutablePlaceNeed::Unique => facts.unique = u32::from(seen.insert(components.at(cursor.component as usize))),
                MutablePlaceNeed::Duplicate => return self.fail(span, "B227", "独占借用位置不能包含重复分量"),
                MutablePlaceNeed::Initialize => { unsafe { self.world.call(self.require_initialized(&place, span)).await }?; }
                MutablePlaceNeed::Complete => return Ok(place),
            }
        }
    }
    pub(super) fn require_owned_writable(&self,place:&Place,span:Span)->Result<()> {
        if self.env.values().any(|binding|matches!(binding,Binding::OwnedValue(value)if value.ownership==application::Ownership::Borrowed && value.place().is_some_and(|root|place.overlaps(&root)))) {
            return self.fail(span,"B227","read聚合的元数据与普通字段不能写入或取得独占借用");
        }
        Ok(())
    }
    pub(super) async fn writable_storage(&mut self, place: &Place, span: Span, written: Option<bounds::Proof>) -> Result<()> {
        let base = place.storage();
        let storage = self.storage_access(base);
        let decision = unsafe { writable_storage(storage.kind, storage.access as u32, u32::from(storage.writable)) };
        if decision.reference != 0 {
            let borrow = unsafe { self.world.call(self.position_origin(base, span)).await }?.normalize();
            let written = if matches!(place, Place::Indexed { .. }) { written.map(|_| bounds::Proof::default()) } else { written };
            self.check_view_requirement(view::Requirement::Address {
                borrow, layout: address::Layout::of(base.ty()), written,
                writable: true, initialized: false, external_atomic: false, span,
            })?;
        }
        if decision.permitted == 0 {
            return self.fail(
                span,
                "B227",
                "write／inout 需要当前独占元素或可变局部位置，不能借用只读或原子共享位置",
            );
        }
        Ok(())
    }
    pub(super) fn positions(&self, place: &Place) -> Vec<Position> {
        if let Some(positions) = self.env.values().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let identities = owner.initialization.as_ref()?;
            let components = place.components_in(&owner.place)?;
            Some(components.iter().map(|i| identities[*i].clone()).collect())
        }) {
            return positions;
        }
        place.owned_positions().unwrap()
    }
    pub(super) fn indexed_positions(
        &self,
        place: &Place,
        index: &Value,
        width: usize,
    ) -> Vec<Position> {
        if place.has_owned_positions() {
            Position::index(self.positions(place), ApplicationIndex::of(index), width)
        } else {
            Vec::new()
        }
    }
    pub(super) fn position_initialized_in(
        &self,
        position: &Position,
        initialized: &BTreeSet<Position>,
        environment: &BTreeMap<String, Binding>,
        assuming: Option<&owned_presence::Presence>,
    ) -> bool {
        let position=match assuming {
            Some(assuming)=>std::borrow::Cow::Owned(self.resolve_owned_initialization(position,Some(assuming))),
            None=>std::borrow::Cow::Borrowed(position),
        };
        let initialized=match assuming {
            Some(assuming)=>std::borrow::Cow::Owned(initialized.iter().map(|position|self.resolve_owned_initialization(position,Some(assuming))).collect()),
            None=>std::borrow::Cow::Borrowed(initialized),
        };
        let path=match assuming {
            Some(assuming)=>std::borrow::Cow::Owned(assuming.condition().assume(&self.path,true)),
            None=>std::borrow::Cow::Borrowed(&self.path),
        };
        position.initialized_by(&initialized) || address::initialized_by_effects(&position,&initialized,&path) || match position.as_ref() {
            Position::Conditional(when,a,b)=>[(&**a,when.as_ref().clone()),(&**b,when.as_ref().clone().not())].into_iter().all(|(position,when)|{
                let assuming=assuming.cloned().unwrap_or(owned_presence::Presence::All).and(when);
                self.position_initialized_in(position,&initialized,environment,Some(&assuming))
            }),
            Position::Both(a,b)=>self.position_initialized_in(a,&initialized,environment,assuming)&&self.position_initialized_in(b,&initialized,environment,assuming),
            Position::Indexed(choices,_)=>choices.iter().all(|choice|self.position_initialized_in(choice,&initialized,environment,assuming)),
            Position::ApplicationElement(scope,_,word)=>environment.values().flat_map(Binding::application_resources).any(|data|
                data.initialization.as_ref().is_some_and(|position|self.resolve_owned_initialization(position,assuming)==Position::Parameter(*scope,0))&&self.application_field_initialized_in(data,*word,&initialized)),
            _=>false,
        }
    }
    pub(super) fn is_initialized(&self, place: &Place) -> bool {
        self.positions_initialized(&self.positions(place))
    }
    pub(super) fn positions_initialized(&self, positions: &[Position]) -> bool {
        if positions.iter().all(|position| self.position_initialized_in(position, &self.initialized, &self.env,None)) {
            return true;
        }
        let initialized=self.initialized.iter().map(|position|self.resolve_owned_initialization(position,None)).collect();
        positions.iter().all(|position|self.position_initialized_in(&self.resolve_owned_initialization(position,None),&initialized,&self.env,None))
    }
    pub(super) fn incomplete_positions(&self) -> bool {
        self.env.values().any(|binding| match binding {
            Binding::Owned(owner) => {
                !self.is_initialized(&owner.place)
            }
            Binding::OwnedValue(owner) => {
                owner
                        .place()
                        .is_some_and(|place| !self.is_initialized(&place))
            }
            _ => false,
        }) || self.view_access.iter().any(|(binding, access)| {
            *access == Access::Write
                && self.program.buffers[*binding]
                    .ty
                    .components
                    .iter()
                    .any(|(_, offset)| {
                        !self
                            .initialized
                            .contains(&Position::Element(*binding, *offset))
                    })
        })
    }
    pub(super) fn defer_initialization(&mut self,place:&Place,code:&'static str,span:Span)->Result<bool> {
        self.defer_position_initialization(self.positions(place),code,span)
    }
    pub(super) fn defer_position_initialization(&mut self,positions:Vec<Position>,code:&'static str,span:Span)->Result<bool> {
        let read=address::InitializationRead{
            positions:positions.iter().map(|position|self.resolve_owned_initialization(position,None)).collect(),
            initialized:self.initialized.iter().map(|position|self.resolve_owned_initialization(position,None)).collect(),
            dataset_output: None,
        };
        let borrow = read.borrow();
        if borrow.inputs.is_empty() && !address::has_pending_initialization(&read.initialized) {
            return Ok(false);
        }
        self.check_view_requirement(view::Requirement::Initialized{borrow,read,code,span})?;
        Ok(true)
    }
    pub(super) async fn require_initialized(&mut self, place: &Place, span: Span) -> Result<()> {
        if !self.is_initialized(place) {
            if self.defer_initialization(place,"B142",span)?{return Ok(());}
            if let Some((_,borrow))=unsafe { self.world.call(self.reference_memory(place,span)).await }? {
                return self.check_view_requirement(view::Requirement::Address {borrow,layout:address::Layout::of(place.ty()),written:None,writable:false,initialized:true,external_atomic:false,span});
            }
            return self.fail(
                span,
                "B142",
                "读取或 inout 借用了尚未初始化的位置；先完整写入所需分量",
            );
        }
        Ok(())
    }
    pub(super) fn initialize(&mut self, place: &Place) {
        self.initialized.extend(self.positions(place));
    }
    // Ownership establishes the stored handle only for this destructor read.
    pub(super) async fn read_cleanup_storage<T>(&mut self,place:&Place,read:impl std::ops::AsyncFnOnce(&mut Self)->Result<T>)->Result<T> {
        let temporary=self.positions(place).into_iter()
            .filter(|position|self.initialized.insert(position.clone())).collect::<Vec<_>>();
        let result=read(self).await;
        for position in temporary {
            self.initialized.remove(&position);
        }
        result
    }
    pub(super) fn join_initialization(
        &mut self,
        choice:&owned_presence::Presence,
        yes:BTreeSet<Position>,
        no:BTreeSet<Position>,
    ) {
        use owned_presence::Presence;
        self.initialized=match choice {
            Presence::All=>yes,
            Presence::None=>no,
            choice=>{
                merge_initialization(&mut [yes, no], &[choice.clone(), choice.clone().not()], |set| set)
            }
        };
    }
    pub(super) fn borrow_owned(&mut self, place: Place) -> OwnedBinding {
        for binding in self.env.values() {
            if let Binding::OwnedValue(owner) = binding
                && let Some(root) = owner.place()
                && let Some(components) = place.components_in(&root)
            {
                return OwnedBinding {
                    proof: owner
                        .value
                        .proof
                        .project(&components, owner.value.ty.components.len()),
                    uniform: owner.value.uniform,
                    initialization: None,
                    place,
                };
            }
        }
        let projected = self.env.values().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            Some((owner, place.components_in(&owner.place)?))
        });
        if let Some((owner, components)) = projected {
            OwnedBinding {
                proof: owner
                    .proof
                    .project(&components, owner.place.ty().components.len()),
                uniform: owner.uniform,
                initialization: owner
                    .initialization
                    .as_ref()
                    .map(|ids| components.iter().map(|i| ids[*i].clone()).collect()),
                place,
            }
        } else {
            OwnedBinding {
                proof: self.projected_owned_proof(&place, None).unwrap_or_default(),
                place,
                uniform: false,
                initialization: None,
            }
        }
    }
}
