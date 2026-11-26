//! Project a module's linked code groups into independently extractable native members.
use super::*;

pub(super) fn needs_groups(symbols: &[ObjectSymbol<'_>]) -> bool {
    symbols.iter().filter(|symbol| symbol.external != 0 && symbol.section <= 0).any(|symbol| {
        match symbol.role {
            ObjectSymbolRole::Import => {
                let payload = unsafe { symbol.name.borrowed() };
                &payload[..symbol.value as usize] == crate::resource::RUNTIME_LIBRARY.as_bytes()
            }
            ObjectSymbolRole::Data | ObjectSymbolRole::Function | ObjectSymbolRole::InvalidImport => true,
        }
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HostSection {
    owner: usize,
    member: usize,
    local: usize,
}

#[repr(C)]
struct HostGroupResult {
    count: usize,
    error: ObjectReadError,
}

#[repr(C)]
struct HostGroupSpan { rows: *const std::ffi::c_void, count: usize }

impl HostGroupSpan {
    fn borrowed<T>(rows: &[T]) -> Self { Self { rows: rows.as_ptr().cast(), count: rows.len() } }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct HostGroupState { start: usize, end: usize, seen: usize, required: usize }

#[repr(C)]
struct HostGroupLinks {
    sections: ObjectRows,
    positions: *const HostSection,
    states: *mut HostGroupState,
    count: usize,
    runtime: BindingBytes,
}

struct HostGroupStorage<'a> {
    members: &'a [Vec<usize>],
    relocations: &'a [Vec<BindingRelocation>],
    edges: Vec<usize>,
}

#[repr(C)]
struct HostGroupSources {
    context: *mut std::ffi::c_void,
    members: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> HostGroupSpan,
    relocations: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> HostGroupSpan,
    reserve: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> *mut usize,
}

unsafe extern "C" fn host_member_sections(storage: *const std::ffi::c_void, group: usize) -> HostGroupSpan {
    let storage = unsafe { &*storage.cast::<HostGroupStorage<'_>>() };
    HostGroupSpan::borrowed(&storage.members[group])
}

unsafe extern "C" fn host_section_relocations(storage: *const std::ffi::c_void, section: usize) -> HostGroupSpan {
    let storage = unsafe { &*storage.cast::<HostGroupStorage<'_>>() };
    HostGroupSpan::borrowed(&storage.relocations[section])
}

unsafe extern "C" fn reserve_host_group_edges(storage: *mut std::ffi::c_void, count: usize) -> *mut usize {
    let storage = unsafe { &mut *storage.cast::<HostGroupStorage<'_>>() };
    let edges = &mut storage.edges;
    edges.reserve_exact(count);
    edges.as_mut_ptr()
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HostAnchorName {
    identity: BindingBytes,
    section: usize,
    value: u32,
}

struct HostAnchors {
    identity: BindingBytes,
    names: BTreeMap<(usize, u32), String>,
}

unsafe extern "C" fn host_anchor_name(
    context: *mut std::ffi::c_void, section: usize, value: u32,
) -> BindingBytes {
    let anchors = unsafe { &mut *context.cast::<HostAnchors>() };
    let name = anchors.names.entry((section, value)).or_insert_with(|| {
        let fields = HostAnchorName { identity: anchors.identity, section, value };
        let length = unsafe { host_anchor_name_size(fields) };
        let mut bytes = Box::<[u8]>::new_uninit_slice(length);
        unsafe { write_host_anchor_name(fields, bytes.as_mut_ptr().cast()) };
        // The writer fills this slice with the ASCII identity and decimal fields.
        unsafe { String::from_utf8_unchecked(bytes.assume_init().into_vec()) }
    });
    BindingBytes::new(name.as_bytes())
}

unsafe extern "C" {
    #[link_name = "bootstrap.bindings::host_anchor_name_size"]
    fn host_anchor_name_size(name: HostAnchorName) -> usize;
    #[link_name = "bootstrap.bindings::write_host_anchor_name"]
    fn write_host_anchor_name(name: HostAnchorName, output: *mut u8);
    #[link_name = "bootstrap.object_read::rebind_host_relocations"]
    fn rebind_host_relocations(positions: *const HostSection, member: usize,
        records: *mut BindingRelocation, count: usize, context: *mut std::ffi::c_void,
        anchor: unsafe extern "C" fn(*mut std::ffi::c_void, usize, u32) -> BindingBytes);
    #[link_name = "bootstrap.object_read::host_group_resolution"]
    fn host_group_resolution(input: HostGroupLinks, source: HostGroupSources) -> usize;
    #[link_name = "bootstrap.object_read::host_groups"]
    fn host_groups(rows: ObjectRows, definitions: *const SectionDefinition,
        output: *mut HostSection, count: usize) -> HostGroupResult;
}

pub(super) struct Groups {
    members: Vec<Vec<usize>>,
    positions: Vec<HostSection>,
    relocations: Vec<Vec<BindingRelocation>>,
}

impl Groups {
    pub(super) fn new(
        raw: &[ObjectSection], definitions: &[SectionDefinition], needs_body: bool,
    ) -> Result<Option<Self>> {
        let count = raw.iter().rev().find(|row| row.mapped != 0).map_or(0, |row| row.mapped);
        let mut positions = Vec::<HostSection>::with_capacity(count);
        let plan = unsafe { host_groups(ObjectRows::new(raw), definitions.as_ptr(), positions.as_mut_ptr(), count) };
        plan.error.check()?;
        if plan.count < 2 && !needs_body {
            return Ok(None);
        }
        // The B reader writes each mapped slot before returning a complete plan.
        unsafe { positions.set_len(count) };
        let mut members: Vec<Vec<usize>> = (0..plan.count).map(|_| Vec::new()).collect();
        for (index, row) in raw.iter().enumerate().filter(|(_, row)| row.mapped != 0) {
            let position = &mut positions[row.mapped - 1];
            let member = &mut members[position.member];
            position.local = member.len();
            member.push(index);
        }
        let relocations = (0..count).map(|_| Vec::new()).collect();
        Ok(Some(Self { members, positions, relocations }))
    }

    pub(super) fn relocation(&mut self, section: usize, offset: usize, kind: u16, target: ObjectTarget<'_>) {
        let target_kind = match target.kind {
            ObjectTargetKind::Section => BindingTarget::Section,
            ObjectTargetKind::Symbol => BindingTarget::Symbol,
            ObjectTargetKind::Import => BindingTarget::Import,
        };
        self.relocations[section].push(BindingRelocation {
            offset: offset as u64, kind, target: target_kind,
            value: target.value as u32, section: target.section as u64,
            name: target.name.as_binding(), dll: target.dll.as_binding(),
        });
    }

    fn combine_closed_groups(&mut self, raw: &[ObjectSection]) -> Option<usize> {
        let count = self.members.len();
        let mut states = vec![HostGroupState::default(); count];
        let first = {
            let mut storage = HostGroupStorage {
                members: &self.members, relocations: &self.relocations, edges: Vec::new(),
            };
            let input = HostGroupLinks {
                sections: ObjectRows::new(raw), positions: self.positions.as_ptr(), states: states.as_mut_ptr(),
                count, runtime: BindingBytes::new(crate::resource::RUNTIME_LIBRARY.as_bytes()),
            };
            let source = HostGroupSources {
                context: std::ptr::from_mut(&mut storage).cast(),
                members: host_member_sections, relocations: host_section_relocations,
                reserve: reserve_host_group_edges,
            };
            unsafe { host_group_resolution(input, source) }
        };
        if first == count {
            return None;
        }
        for (member, state) in states.iter().enumerate().skip(first + 1) {
            if state.required == 0 {
                let group = std::mem::take(&mut self.members[member]);
                self.members[first].extend(group);
            }
        }
        self.members[first].sort_unstable();
        for (index, &section) in self.members[first].iter().enumerate() {
            let position = &mut self.positions[raw[section].mapped - 1];
            position.member = first;
            position.local = index;
        }
        Some(first)
    }

    pub(super) fn encode_into<'a>(
        mut self, bytes: &[u8], raw: &[ObjectSection], symbols: &[ObjectSymbol<'a>],
        build: Option<&HostBuildSection>, name: &str, output: &mut Vec<HostMember<'a>>,
    ) -> Result<()> {
        let build_index = build.map(|section| section.index);
        let combined = self.combine_closed_groups(raw);
        let identity = member_identity(bytes, raw, build);
        let mut anchors = HostAnchors { identity: BindingBytes::new(identity.as_bytes()), names: BTreeMap::new() };
        for (source, relocations) in self.relocations.iter_mut().enumerate() {
            unsafe {
                rebind_host_relocations(self.positions.as_ptr(), self.positions[source].member,
                    relocations.as_mut_ptr(), relocations.len(), std::ptr::from_mut(&mut anchors).cast(),
                    host_anchor_name);
            }
        }
        let mut definitions: Vec<Vec<BindingDefinition>> = self.members.iter().map(|_| Vec::new()).collect();
        let mut names: Vec<Vec<Cow<'a, str>>> = self.members.iter().map(|_| Vec::new()).collect();
        for symbol in symbols.iter().filter(|symbol| symbol.external != 0 && symbol.section > 0) {
            let index = object_index(raw, symbol.section as usize - 1)
                .ok_or("导出符号引用不可装载的节")?;
            let HostSection { member, local: section, .. } = self.positions[index];
            let name = unsafe { symbol.validated_name() };
            definitions[member].push(BindingDefinition {
                name: BindingBytes::new(name.as_bytes()), section: section as u64,
                value: u64::from(symbol.value), function: u64::from(symbol.is_function()),
            });
            names[member].push(Cow::Borrowed(name));
        }
        for ((index, value), name) in anchors.names {
            let HostSection { member, local: section, .. } = self.positions[index];
            definitions[member].push(BindingDefinition {
                name: BindingBytes::new(name.as_bytes()), section: section as u64, value: u64::from(value), function: 0,
            });
            // The same buffer stays live for the relocation and definition views.
            names[member].push(Cow::Owned(name));
        }

        let first = output.len();
        for (member, physical) in self.members.iter().enumerate() {
            if physical.is_empty() {
                continue;
            }
            let sections = physical.iter().map(|&index| {
                let row = &raw[index];
                let section = row.mapped - 1;
                let mut view = CoffSectionView::extra(row_name(row), row_payload(row), row.flags as u32)
                    .with_relocations(&self.relocations[section], &[]);
                view.owner = self.positions[section].owner.checked_sub(1)
                    .map_or(0, |owner| self.positions[owner].local as u64 + 1);
                view
            });
            let extra_rows = if member == 0 { raw } else { &[] };
            let extra = extra_rows.iter().enumerate()
                .filter(|(index, row)| row.mapped == 0 && Some(*index) != build_index)
                .map(|(_, row)| row);
            let extra_count = extra.clone().count();
            let sections = sections.chain(extra.map(|row|
                CoffSectionView::extra(row_name(row), row_payload(row), row.flags as u32)));
            // Retain the original COMDAT namespace of each independent member.
            let group_name = if combined == Some(member) {
                format!("{identity}$closed${member}")
            } else {
                format!("{identity}${member}")
            };
            let declarations = &definitions[member];
            let code = with_sections(
                &group_name, physical.len(), extra_count, sections, declarations.len(),
                declarations.iter().copied(), None, None, owned_output,
            )?;
            let number = output.len() - first + 1;
            // Later groups can still borrow the anchor names from these members.
            output.push(HostMember {
                name: Cow::Owned(format!("{name}.{number}.obj")),
                bytes: Cow::Owned(code),
                build: None,
                names: std::mem::take(&mut names[member]),
            });
        }
        Ok(())
    }
}

fn row_name(row: &ObjectSection) -> &str {
    unsafe { std::str::from_utf8_unchecked(row.name.borrowed()) }
}

fn row_payload(row: &ObjectSection) -> CoffPayload<'_> {
    if row.zero_fill == 0 {
        CoffPayload::Borrowed(unsafe { row.bytes.borrowed() })
    } else {
        CoffPayload::ZeroFill(row.zero_fill)
    }
}

fn member_identity(bytes: &[u8], raw: &[ObjectSection], build: Option<&HostBuildSection>) -> String {
    let mut identity = crate::build::hash::Digest::new();
    let data = 20 + raw.len() * 40;
    // The physical header contributed to the existing identity when its name used the string table.
    let short_build = build.map(|section| section.index).filter(|&index| {
        let start = 20 + index * 40;
        &bytes[start..start + 8] == BUILD.as_bytes()
    });
    for (index, header) in bytes[20..data].chunks_exact(40).enumerate() {
        if Some(index) == short_build {
            continue;
        }
        identity.field(&header[..20]);
        for pointer in header[20..32].chunks_exact(4) {
            let mut offset = u32::from_le_bytes(pointer.try_into().unwrap()) as usize;
            if let Some(build) = build && offset >= build.record.end {
                offset -= build.record.len();
            }
            identity.field(&offset.saturating_sub(data).to_le_bytes());
        }
        identity.field(&header[32..]);
    }
    if let Some(build) = build {
        identity.update(&bytes[data..build.record.start]);
        identity.update(&bytes[build.record.end..]);
    } else {
        identity.update(&bytes[data..]);
    }
    crate::build::hash::hex(&identity.finish())
}
