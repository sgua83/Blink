use super::*;

#[repr(u32)]
#[derive(Clone, Copy)]
enum LoopWriteKind { Move, Position, Contents, Memory, External }

impl Write {
    fn loop_kind(&self) -> LoopWriteKind {
        match self {
            Self::Move { .. } => LoopWriteKind::Move,
            Self::Position { contents: false, .. } => LoopWriteKind::Position,
            Self::Position { contents: true, .. } => LoopWriteKind::Contents,
            Self::Memory { .. } => LoopWriteKind::Memory,
            Self::External { .. } | Self::Generator(_) => LoopWriteKind::External,
        }
    }
}

#[repr(C)]
struct LoopBytes { begin: u64, end: u64 }

#[repr(C)]
struct LoopStorageSite { root: Reg, descendants: u32, declared: u32 }

#[repr(C)]
struct LoopStoragePart { present: u32, word: u32, width: u32 }

#[repr(C)]
struct LoopStorageHost {
    context: *mut std::ffi::c_void,
    site: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> LoopStorageSite,
    start_parts: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
    next_part: unsafe extern "C" fn(*mut std::ffi::c_void) -> LoopStoragePart,
}

struct LoopStorageProjection<'a, I> {
    locations: &'a [(StorageWrite, Ty, application::DataLayout)],
    parts: I,
    current: Option<I>,
}

impl<I: Iterator<Item = (Scalar, u32)> + Clone> LoopStorageProjection<'_, I> {
    unsafe extern "C" fn site(context: *mut std::ffi::c_void, index: usize) -> LoopStorageSite {
        let projection = unsafe { &*context.cast::<Self>() };
        let (location, _, layout) = &projection.locations[index];
        LoopStorageSite {
            root: location.root,
            descendants: (location.kind == StorageWriteKind::Descendants).into(),
            declared: matches!(layout, application::DataLayout::Declared).into(),
        }
    }

    unsafe extern "C" fn start_parts(context: *mut std::ffi::c_void, index: usize) -> u32 {
        let projection = unsafe { &mut *context.cast::<Self>() };
        let (_, ty, _) = &projection.locations[index];
        projection.current = Some(projection.parts.clone());
        ir::align_up(ty.size, ty.align)
    }

    unsafe extern "C" fn next_part(context: *mut std::ffi::c_void) -> LoopStoragePart {
        let projection = unsafe { &mut *context.cast::<Self>() };
        match projection.current.as_mut().and_then(Iterator::next) {
            Some((scalar, word)) => LoopStoragePart { present: 1, word, width: scalar.size() },
            None => LoopStoragePart { present: 0, word: 0, width: 0 },
        }
    }

    fn project(&mut self, output: &mut LoopStorage, constant: bool, index: u32) {
        let count = self.locations.len();
        let host = LoopStorageHost {
            context: (self as *mut Self).cast(),
            site: Self::site,
            start_parts: Self::start_parts,
            next_part: Self::next_part,
        };
        unsafe { project_loop_storage(output, host, count, constant.into(), index) };
    }
}

#[repr(u32)]
enum LoopPosition { Missing, Local, Reference, Other }

fn position(place: Option<&Place>) -> LoopPosition {
    match place {
        None => LoopPosition::Missing,
        Some(Place::Local(..)) => LoopPosition::Local,
        Some(Place::Reference { .. }) => LoopPosition::Reference,
        Some(_) => LoopPosition::Other,
    }
}

#[repr(u32)]
#[allow(dead_code)]
enum LoopProjectionAction { Done, Memory, Position }

#[repr(C)]
#[derive(Default)]
struct LoopProjectionCursor {
    next: usize,
    unknown_storage: u32,
    external: u32,
}

#[repr(C)]
struct LoopProjectionRequest { index: usize, action: LoopProjectionAction }

#[repr(C)]
struct LoopWriteInfo { kind: LoopWriteKind, address_write: u32, iterator: u32 }

#[repr(C)]
struct LoopContentsInfo { root: Reg, found: u32, owns_resources: u32 }

#[repr(C)]
struct LoopProjectionHost {
    context: *mut std::ffi::c_void,
    write: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> LoopWriteInfo,
    contents: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> LoopContentsInfo,
}

struct LoopProjectionInput<'a, 'w> {
    builder: &'a Builder<'w>,
    writes: &'a [Write],
    root: PlaceRoot<'a>,
}

impl LoopProjectionInput<'_, '_> {
    unsafe extern "C" fn write(context: *mut std::ffi::c_void, index: usize) -> LoopWriteInfo {
        let input = unsafe { &mut *context.cast::<Self>() };
        let write = &input.writes[index];
        let (root, iterator) = match write {
            Write::Position { place, iterator, .. } => (place_root(place), iterator.is_some()),
            _ => (PlaceRoot::Other, false),
        };
        input.root = root;
        LoopWriteInfo { kind: write.loop_kind(), address_write: input.root.is_address().into(), iterator: iterator.into() }
    }

    unsafe extern "C" fn contents(context: *const std::ffi::c_void, index: usize) -> LoopContentsInfo {
        let input = unsafe { &*context.cast::<Self>() };
        let Write::Position { place, .. } = &input.writes[index] else { unreachable!() };
        match input.builder.written_application_data(place) {
            Some((data, _, ty)) => LoopContentsInfo {
                root: data.root.regs[0], found: 1, owns_resources: ty.owns_resources().into(),
            },
            None => LoopContentsInfo { root: 0, found: 0, owns_resources: 0 },
        }
    }

    fn advance(&mut self, cursor: &mut LoopProjectionCursor, storage: &mut LoopStorage) -> LoopProjectionRequest {
        let count = self.writes.len();
        let host = LoopProjectionHost {
            context: (self as *mut Self).cast(),
            write: Self::write,
            contents: Self::contents,
        };
        unsafe { advance_loop_projection(cursor, storage, count, host) }
    }
}

#[repr(C)]
#[derive(Default)]
struct LoopMemoryCursor { next: usize, origin: *const address::Origin, located: u32 }

#[repr(C)]
struct LoopMemoryOrigin { source: *const address::Origin, resource: u32, active: u32 }

#[repr(C)]
struct LoopMemoryOwner {
    slots: *const usize,
    components: *const usize,
    count: usize,
    position: LoopPosition,
    present: u32,
}

#[repr(C)]
struct LoopMemoryRequest { origin: *const address::Origin, resource: *const application::Data }

#[repr(C)]
struct LoopMemoryHost {
    context: *mut std::ffi::c_void,
    origin: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> LoopMemoryOrigin,
    resource: unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> *const application::Data,
    owner: unsafe extern "C" fn(*mut std::ffi::c_void) -> LoopMemoryOwner,
}

struct LoopMemoryInputs<'a, 'w, R, O> {
    builder: &'a Builder<'w>,
    origins: &'a [&'a address::Origin],
    positions: &'a [Position],
    resources: R,
    current_resources: Option<R>,
    owners: O,
    current_owner: Option<Place>,
    components: Vec<usize>,
}

impl<'a, R, O> LoopMemoryInputs<'a, '_, R, O>
where
    R: Iterator<Item = &'a application::Data> + Clone,
    O: Iterator<Item = Place>,
{
    unsafe extern "C" fn origin(context: *const std::ffi::c_void, index: usize) -> LoopMemoryOrigin {
        let input = unsafe { &*context.cast::<Self>() };
        let source = input.origins[index];
        let resource = matches!(source.root, address::Root::Resource { .. } | address::Root::Dataset { .. });
        LoopMemoryOrigin { source, resource: resource.into(), active: (!resource && source.resources().active()).into() }
    }

    unsafe extern "C" fn resource(context: *mut std::ffi::c_void, restart: u32) -> *const application::Data {
        let input = unsafe { &mut *context.cast::<Self>() };
        if restart != 0 {
            input.current_resources = Some(input.resources.clone());
        }
        input.current_resources.as_mut().and_then(Iterator::next).map_or(std::ptr::null(), std::ptr::from_ref)
    }

    unsafe extern "C" fn owner(context: *mut std::ffi::c_void) -> LoopMemoryOwner {
        // B consumes the slot layout and selected components before requesting another owner.
        let input = unsafe { &mut *context.cast::<Self>() };
        input.current_owner = input.owners.next();
        let Some(owner) = &input.current_owner else {
            return LoopMemoryOwner { slots: std::ptr::null(), components: std::ptr::null(), count: 0,
                position: LoopPosition::Missing, present: 0 };
        };
        let storage = owner.storage();
        input.components = input.builder.address_write_components(input.positions, owner);
        let slots = match storage {
            Place::Local(_, slots, ..) => slots.as_ptr(),
            _ => std::ptr::null(),
        };
        LoopMemoryOwner { slots, components: input.components.as_ptr(), count: input.components.len(),
            position: position(Some(storage)), present: 1 }
    }

    fn advance(&mut self, cursor: &mut LoopMemoryCursor, projection: &mut LoopProjectionCursor,
        locals: &mut LoopSlots, resolved: bool, located: bool,
    ) -> LoopMemoryRequest {
        let count = self.origins.len();
        let host = LoopMemoryHost {
            context: (self as *mut Self).cast(),
            origin: Self::origin,
            resource: Self::resource,
            owner: Self::owner,
        };
        unsafe { advance_loop_memory(cursor, projection, locals, count, host, resolved.into(), located.into()) }
    }
}

#[repr(u32)]
#[allow(dead_code)]
enum LoopInvalidation { Elements, Rebind, Select, Apply, Done }

#[repr(u32)]
#[allow(dead_code)]
enum LoopElementWrite { None, Bytes, Unknown, Contents, Position }

#[repr(C)]
struct LoopElementEffect { action: LoopElementWrite, raw: u32, deferred: u32 }

#[repr(C)]
struct LoopIndexRange { low: u32, high: u32 }

#[repr(u32)]
#[allow(dead_code)]
enum LoopRebind { Keep, CheckBorrow, Scan, Forget }

#[repr(u32)]
#[allow(dead_code)]
enum LoopComponentWrite { None, All, Memory, OutsideElement, Contents, Position }

#[derive(Default)]
struct LoopMemoryUse { raw: u32, deferred: u32 }

unsafe extern "C" {
    #[link_name = "bootstrap.source_writes::advance_loop_projection"]
    fn advance_loop_projection(cursor: &mut LoopProjectionCursor, storage: &mut LoopStorage,
        count: usize, host: LoopProjectionHost) -> LoopProjectionRequest;
    #[link_name = "bootstrap.source_writes::project_loop_storage"]
    fn project_loop_storage(output: &mut LoopStorage, host: LoopStorageHost, count: usize, constant_index: u32, index: u32);
    #[link_name = "bootstrap.source_writes::loop_array_elements"]
    fn loop_array_elements(length: u64, exact: u32, index: u32) -> LoopBytes;
    #[link_name = "bootstrap.source_writes::loop_part_matches"]
    fn loop_part_matches(source_scalar: u32, source_word: u32, target_scalar: u32, target_word: u32, offset: u32) -> u32;
    #[link_name = "bootstrap.source_writes::advance_loop_memory"]
    fn advance_loop_memory(cursor: &mut LoopMemoryCursor, projection: &mut LoopProjectionCursor,
        locals: &mut LoopSlots, origins: usize, host: LoopMemoryHost, resolved: u32, located: u32) -> LoopMemoryRequest;
    #[link_name = "bootstrap.source_writes::record_loop_resource"]
    fn record_loop_resource(cursor: &mut LoopProjectionCursor, located: u32);
    #[link_name = "bootstrap.source_writes::record_loop_position"]
    fn record_loop_position(cursor: &mut LoopProjectionCursor, position: LoopPosition, written: u32, has_binding: u32, direct: u32);
    #[link_name = "bootstrap.source_writes::loop_invalidation_next"]
    fn loop_invalidation_next(phase: LoopInvalidation) -> LoopInvalidation;
    #[link_name = "bootstrap.source_writes::loop_element_effect"]
    fn loop_element_effect(kind: LoopWriteKind, address_write: u32, resolved: u32) -> LoopElementEffect;
    #[link_name = "bootstrap.source_writes::loop_index_range"]
    fn loop_index_range(constant: u32, index: u32) -> LoopIndexRange;
    #[link_name = "bootstrap.source_writes::loop_rebind"]
    fn loop_rebind(held: u32, identity: u32, borrowed: u32, checked: u32, written: u32) -> LoopRebind;
    #[link_name = "bootstrap.source_writes::loop_scalar_header"]
    fn loop_scalar_header(conditional: u32, owned: u32, unsigned_scalar: u32, position: LoopPosition) -> u32;
    #[link_name = "bootstrap.source_writes::loop_cursor_query"]
    fn loop_cursor_query(kind: LoopWriteKind, cursor: u32, application_element: u32) -> u32;
    #[link_name = "bootstrap.source_writes::loop_component_write"]
    fn loop_component_write(kind: LoopWriteKind, cursor: u32, same_resource: u32, same_layout: u32, same_index: u32) -> LoopComponentWrite;
    #[link_name = "bootstrap.source_writes::loop_deferred_input"]
    fn loop_deferred_input(deferred: u32, view_scope: u32) -> u32;
    #[link_name = "bootstrap.source_writes::loop_metadata_contains"]
    fn loop_metadata_contains(begin: u32, size: u32, word: u32) -> u32;
    #[link_name = "bootstrap.source_writes::loop_external_proof"]
    fn loop_external_proof(raw: u32, view_scope: u32) -> u32;
    #[link_name = "bootstrap.source_writes::loop_local_input"]
    fn loop_local_input(view_scope: u32, entry: u32, direct: u32) -> u32;
}

impl Builder<'_> {
    async fn append_loop_storage(
        &self,
        effect: &mut LoopWrites,
        input: &application::Data,
        resource: &view::Borrow,
        index: Option<bounds::Operand>,
        parts: impl Iterator<Item = (Scalar, u32)> + Clone,
    ) -> Result<bool> {
        let Some(locations) = (unsafe { self.world.call(self.application_resource_storage(input, resource)).await })? else {
            return Ok(false);
        };
        let (constant, index) = match index {
            Some(bounds::Operand::Constant(Scalar::U32, value)) => (true, value as u32),
            _ => (false, 0),
        };
        LoopStorageProjection {
            locations: &locations,
            parts,
            current: None,
        }.project(&mut effect.storage, constant, index);
        Ok(true)
    }

    async fn project_loop_memory(&self, effect: &mut LoopWrites, projection: &mut LoopProjectionCursor,
        borrow: &view::Borrow,
    ) -> Result<()> {
        let mut positions = Vec::new();
        let mut origins = Vec::new();
        let resolved = Self::visit_address_writes(borrow, |origin, when| {
            if origin.writable {
                positions.extend(origin.memory_regions(&when)?.into_iter().map(|region| region.position));
                origins.push(origin.as_ref());
            }
            Some(())
        }).is_some();

        let mut cursor = LoopMemoryCursor::default();
        let mut input = LoopMemoryInputs {
            builder: self,
            origins: &origins,
            positions: &positions,
            resources: self.env.values().flat_map(Binding::application_resources),
            current_resources: None,
            owners: self.env.values().filter_map(Binding::stored_place),
            current_owner: None,
            components: Vec::new(),
        };
        let mut located = false;
        loop {
            let request = input.advance(&mut cursor, projection, &mut effect.locals, resolved, located);
            let Some(resource) = (unsafe { request.resource.as_ref() }) else { return Ok(()); };
            let origin = unsafe { &*request.origin };
            let (address::Root::Resource { borrow, index } | address::Root::Dataset { borrow, index }) = &origin.root
                else { unreachable!() };
            let parts = origin.layout.parts.iter().zip(origin.words.iter()).map(|((scalar, _), word)| (*scalar, *word));
            located = self.append_loop_storage(effect, resource, borrow, *index, parts).await?;
        }
    }

    async fn project_loop_position(
        &self, effect: &mut LoopWrites, cursor: &mut LoopProjectionCursor, place: &Expr,
        name: Option<&str>, writes: &[Write],
    ) -> Result<()> {
        if let Some((projection, input, _)) = unsafe { self.world.call(self.loop_write_projection(place,
            &async |index, _| self.loop_index_operand(index, writes).map(|index| (index, None)))).await }?
        {
            let parts = projection.value.1.iter().flatten().map(|component| projection.base.components[*component]);
            let located = self.append_loop_storage(effect, input, &projection.resource, projection.index, parts).await?;
            unsafe { record_loop_resource(cursor, located.into()) };
            return Ok(());
        }
        let binding = name.and_then(|name| self.env.get(name));
        let stored = binding.and_then(Binding::stored_place);
        let owner = match binding {
            Some(Binding::ApplicationData(data)) => data.place.as_ref(),
            _ => stored.as_ref(),
        };
        let mut projected = false;
        if let (Some(name), Some(owner @ Place::Local(_, slots, ..))) = (name, owner)
            && let Some((_, components)) = unsafe { self.world.call(self.written_projection(place, name, owner, None, writes)).await }?
        {
            effect.locals.extend(components.into_iter().flatten().map(|component| slots[component]));
            projected = true;
        }
        let storage = if matches!(stored, Some(Place::Reference { .. })) { stored.as_ref() } else { owner };
        unsafe { record_loop_position(cursor, position(storage), projected.into(), binding.is_some().into(),
            matches!(place.kind(), ExprKind::Name(_)).into()) };
        Ok(())
    }

    pub(super) async fn project_loop_writes(&self, writes: &[Write]) -> Result<LoopWrites> {
        let mut effect = LoopWrites::default();
        let mut cursor = LoopProjectionCursor::default();
        let mut input = LoopProjectionInput { builder: self, writes, root: PlaceRoot::Other };
        loop {
            let next = input.advance(&mut cursor, &mut effect.storage);
            match (next.action, writes.get(next.index)) {
                (LoopProjectionAction::Done, _) => {
                    effect.unknown_storage = cursor.unknown_storage != 0;
                    effect.external = cursor.external != 0;
                    return Ok(effect);
                }
                (LoopProjectionAction::Memory, Some(Write::Memory { borrow, .. })) => {
                    self.project_loop_memory(&mut effect, &mut cursor, borrow).await?;
                }
                (LoopProjectionAction::Position, Some(Write::Position { place, .. })) => {
                    self.project_loop_position(&mut effect, &mut cursor, place, input.root.name(), writes).await?;
                }
                _ => unreachable!(),
            }
        }
    }

    pub(super) async fn invalidate_writes(
        &mut self,
        writes: &[Write],
        borrowed_data: Option<ValueIdentity>,
        effect: &LoopWrites,
        condition: Option<&Expr>,
        cursor: Option<&str>,
    ) -> Result<()> {
        let mut phase = LoopInvalidation::Elements;
        let mut memory = LoopMemoryUse::default();
        let mut modified = BTreeMap::new();
        loop {
            match phase {
                LoopInvalidation::Elements => {
                    for write in writes {
                        self.invalidate_loop_elements(write, writes, &mut memory).await?;
                    }
                }
                LoopInvalidation::Rebind => {
                    self.invalidate_loop_rebindings(writes, borrowed_data).await?;
                }
                LoopInvalidation::Select => {
                    modified = self.loop_modified_components(writes, condition.is_some(), cursor).await?;
                }
                LoopInvalidation::Apply => {
                    self.invalidate_loop_proofs(&memory, borrowed_data, effect, &modified);
                }
                LoopInvalidation::Done => return Ok(()),
            }
            phase = unsafe { loop_invalidation_next(phase) };
        }
    }

    async fn invalidate_loop_elements(&mut self, write: &Write, writes: &[Write], usage: &mut LoopMemoryUse) -> Result<()> {
        let memory = write.memory_effect();
        let address = matches!(write, Write::Position { contents: false, place, .. } if place_root(place).is_address());
        // The address effect borrows the normalized write. It is independent
        // of Data updates, so the later proof pass uses this same classification.
        let effect = unsafe { loop_element_effect(write.loop_kind(), address.into(), memory.is_some().into()) };
        usage.raw |= effect.raw;
        usage.deferred |= effect.deferred;
        let changed = match (effect.action, write, memory) {
            (LoopElementWrite::Bytes, Write::Memory { borrow, .. }, Some(memory)) => {
                return unsafe { self.world.call(self.update_address_elements(borrow, memory)).await };
            }
            (LoopElementWrite::Unknown, _, _) => {
                // The update walks the original mutable env. Only its resource
                // operands and field words survive the shared env borrow.
                let contents = self.env.values().flat_map(Binding::application_resources)
                    .map(|data| (data.root.identity(), view::resource_borrow(data),
                        data.ty.components.iter().map(|(_, word)| *word).collect::<Vec<_>>()))
                    .collect::<Vec<_>>();
                for (source, resource, words) in contents {
                    unsafe { self.world.call(self.update_application_elements(
                        &[source], &resource, &words, application::ElementUpdate::Unknown)).await }?;
                }
                return Ok(());
            }
            (LoopElementWrite::Contents, Write::Position { place, when, .. }, _) => {
                self.written_application_data(place).map(|(data, offset, ty)| (
                    Some(data.root.identity()), view::resource_borrow(data),
                    ty.components.iter().map(|(_, word)| offset + word).collect::<Vec<_>>(), None, when.as_ref(),
                ))
            }
            (LoopElementWrite::Position, Write::Position { place, when, .. }, _) => {
                unsafe { self.world.call(self.loop_write_projection(place,
                    &async |index, _| self.loop_index_operand(index, writes).map(|index| (index, None)))).await }?
                    .map(|(projection, _, _)| (
                        projection.data.map(|data| data.root.identity()), projection.resource,
                        projection.value.1.into_iter().flatten().map(|i| projection.base.components[i].1).collect(),
                        projection.index.map(element_index_range), when.as_ref(),
                    ))
            }
            _ => None,
        };
        if let Some((source, resource, words, index, when)) = changed {
            unsafe { self.world.call(self.update_application_elements(source.as_slice(), &resource, &words,
                application::ElementUpdate::Element { proof: &bounds::Proof::default(), index, when })).await }?;
        }
        Ok(())
    }

    async fn invalidate_loop_rebindings(&mut self, writes: &[Write], borrowed_data: Option<ValueIdentity>) -> Result<()> {
        let mut resources = Vec::new();
        for (name, binding) in &self.env {
            let Binding::ApplicationData(data) = binding else { continue; };
            let held = borrowed_data == Some(data.root.identity());
            let identity = data.root.proof.known_identity(0);
            let mut action = unsafe { loop_rebind(held.into(), identity.is_some().into(), 0, 0, 0) };
            if let (LoopRebind::CheckBorrow, Some(identity)) = (&action, identity) {
                let borrowed = unsafe { self.world.call(self.application_borrowed(identity)).await }?;
                action = unsafe { loop_rebind(held.into(), 1, borrowed.into(), 1, 0) };
            }
            if let LoopRebind::Scan = action {
                let written = writes.iter().any(|write| matches!(write,
                    Write::Position { contents: false, place, .. }
                        if matches!(place.kind(), ExprKind::Name(target) if target == name)));
                action = unsafe { loop_rebind(held.into(), identity.is_some().into(), 0, 2, written.into()) };
            }
            if let LoopRebind::Forget = action {
                resources.push(name.clone());
            }
        }
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.root.proof.forget_value(&[0], data.root.ty.components.len());
                data.length.proof = data.count.map(|count| bounds::Proof::unsigned(count, count)).unwrap_or_default();
                data.length.constant = None;
                data.length.uniform = false;
            }
        }
        Ok(())
    }

    async fn loop_modified_components(
        &self,
        writes: &[Write],
        conditional: bool,
        cursor: Option<&str>,
    ) -> Result<BTreeMap<String, BTreeSet<usize>>> {
        let mut modified: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (name, binding) in &self.env {
            let Some(owner) = binding.stored_place() else { continue; };
            if owner.immutable_local() {
                continue;
            }
            let storage = owner.storage();
            if unsafe { loop_scalar_header(conditional.into(), matches!(binding, Binding::Owned(_)).into(),
                (owner.ty() == &Ty::scalar(Scalar::U32)).into(), position(Some(storage))) } != 0
            {
                continue;
            }
            let cursor_name = cursor == Some(name.as_str());
            let application_element = matches!(storage, Place::ApplicationElement { .. });
            for write in writes {
                let kind = write.loop_kind();
                let mut cursor_component = (false, false, false);
                if unsafe { loop_cursor_query(kind, cursor_name.into(), application_element.into()) } != 0
                    && let Place::ApplicationElement { identity, index_value, element_layout, .. } = storage
                    && let Write::Position { place, .. } = write
                    && let Some((written, _, _)) = unsafe { self.world.call(self.loop_write_projection(place,
                        &async |index, _| self.loop_index_operand(index, writes).map(|index| (index, None)))).await }?
                {
                    let same_resource = written.data.is_some_and(|data| data.root.identity() == *identity);
                    let same_layout = same_resource && address::Layout::of(&written.base) == *element_layout;
                    let same_index = same_layout && written.index.is_some_and(|index|
                        bounds::Condition::index_equal(index, *index_value).truth_in(&self.path) == Some(true));
                    cursor_component = (same_resource, same_layout, same_index);
                }
                let (same_resource, same_layout, same_index) = cursor_component;
                let action = unsafe { loop_component_write(kind, (cursor_name && application_element).into(),
                    same_resource.into(), same_layout.into(), same_index.into()) };
                let components = match (&action, write) {
                    (LoopComponentWrite::All, _) => vec![(0..owner.ty().components.len()).collect()],
                    (LoopComponentWrite::Memory | LoopComponentWrite::OutsideElement, _) => vec![self.memory_effect_components(
                        write.memory_effect(), &owner, matches!(action, LoopComponentWrite::OutsideElement))],
                    (LoopComponentWrite::Contents, Write::Position { place, .. }) => {
                        self.application_write_projection(place, &owner).map(|(_, components)| components).unwrap_or_default()
                    }
                    (LoopComponentWrite::Position, Write::Position { place, iterator, .. }) => {
                        let iterator = iterator.as_ref().map(|(name, index)| (name.as_str(), index));
                        unsafe { self.world.call(self.written_projection(place, name, &owner, iterator, writes)).await }?
                            .map(|(_, components)| components).unwrap_or_default()
                    }
                    _ => continue,
                };
                for component in components.into_iter().flatten() {
                    modified.entry(name.clone()).or_default().insert(component);
                }
            }
        }
        Ok(modified)
    }

    fn invalidate_loop_proofs(
        &mut self,
        memory: &LoopMemoryUse,
        borrowed_data: Option<ValueIdentity>,
        effect: &LoopWrites,
        modified: &BTreeMap<String, BTreeSet<usize>>,
    ) {
        let context = self.loop_contexts.last_mut().unwrap();
        let view_scope = u32::from(context.view_scope.is_some());
        for (name, binding) in &mut self.env {
            if unsafe { loop_deferred_input(memory.deferred, view_scope) } != 0 {
                let mut paths = Vec::new();
                if let Some(entry) = context.entry.env.get(name) {
                    view::for_each_data_path(entry, |path, data, _| {
                        let Some(value) = data.elements.known().and_then(returns::Returned::view_value) else { return; };
                        let metadata = address::resource_metadata(&value.ty, view::resource_paths(&value.ty).into_iter()
                            .filter(|fields| application::model_data(data.elements.known(), fields).is_some()).collect());
                        let components = value.ty.components.iter().enumerate().filter_map(|(component, (scalar, word))| {
                            (!metadata.iter().any(|range| unsafe { loop_metadata_contains(range.start, range.end, *word) } != 0))
                                .then_some((component, *scalar))
                        }).collect::<Vec<_>>();
                        paths.push((path.to_vec(), components));
                    });
                }
                for (path, components) in paths {
                    let Some(target) = view::data_at_mut(binding, &path).and_then(|data| data.elements.known_mut())
                        .and_then(returns::Returned::view_value_mut) else { continue; };
                    for (component, scalar) in components {
                        target.proof.forget_value(&[component], target.ty.components.len());
                        let id = context.value_input(name, Some(path.clone()), component, scalar);
                        target.proof = std::mem::take(&mut target.proof).with_component_identity(component, id);
                    }
                    target.constant = None;
                    target.uniform = false;
                }
            }
            let Some(components) = modified.get(name) else { continue; };
            let components: Vec<_> = components.iter().copied().collect();
            // Inputs with a view scope receive actual writes through their backedges.
            if unsafe { loop_external_proof(memory.raw, view_scope) } != 0
                && let Some((proof, _, storage)) = binding.proof_storage()
            {
                for (offset, field) in storage.ty().borrow_fields() {
                    if field.is_address() {
                        let component = storage.ty().components.iter().position(|(_, at)| *at == offset).unwrap();
                        if components.contains(&component) {
                            proof.mark_external_borrow(component);
                        }
                    }
                }
            }
            match binding {
                Binding::Owned(owner) => {
                    owner.uniform = false;
                    owner.proof.forget_value(&components, owner.place.ty().components.len());
                    if let Place::Local(_, slots, ..) = owner.place.storage() {
                        let entry = u32::from(context.entry.env.contains_key(name));
                        for component in components {
                            if unsafe { loop_local_input(view_scope, entry, effect.locals.contains(&slots[component]).into()) } != 0 {
                                let scalar = owner.place.ty().components[component].0;
                                let id = context.value_input(name, None, component, scalar);
                                owner.proof = std::mem::take(&mut owner.proof).with_component_identity(component, id);
                            }
                        }
                    }
                }
                Binding::OwnedValue(owner) => owner.invalidate_components(&components, borrowed_data),
                _ => {}
            }
        }
    }
}

fn element_index_range(index: bounds::Operand) -> (bounds::Operand, (u32, u32)) {
    let constant = match index {
        bounds::Operand::Constant(_, value) => Some(value as u32),
        _ => None,
    };
    let range = unsafe { loop_index_range(constant.is_some().into(), constant.unwrap_or_default()) };
    (index, (range.low, range.high))
}

pub(super) fn array_elements(length: u32, index: Option<u32>) -> std::ops::Range<usize> {
    let selected = unsafe { loop_array_elements(length.into(), index.is_some().into(), index.unwrap_or_default()) };
    selected.begin as usize..selected.end as usize
}

pub(super) fn part_matches(source: (Scalar, u32), target: (Scalar, u32), offset: u32) -> bool {
    unsafe { loop_part_matches(source.0 as u32, source.1, target.0 as u32, target.1, offset) != 0 }
}
