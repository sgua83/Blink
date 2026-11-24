//! Project a module's linked code groups into independently extractable native members.
use super::*;

pub(super) fn needs_groups(symbols: &[ObjectSymbol]) -> bool {
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

pub(super) struct Groups {
    members: Vec<Vec<usize>>,
    positions: Vec<(usize, usize)>,
    owners: Vec<Option<usize>>,
    relocations: Vec<Vec<BindingRelocation>>,
}

impl Groups {
    pub(super) fn new(raw: &[ObjectSection], definitions: &[SectionDefinition]) -> Result<Option<Self>> {
        let mut owners = Vec::new();
        let mut physical = Vec::new();
        let rows = ObjectRows::new(raw);
        for (index, row) in raw.iter().enumerate().filter(|(_, row)| row.mapped != 0) {
            let owner = if row.flags & 0x1000 != 0 {
                let association = unsafe { object_association(rows, definitions.as_ptr(), index) };
                association.error.check()?;
                (association.present != 0).then_some(association.owner)
            } else {
                None
            };
            owners.push(owner);
            physical.push(index);
        }
        let roots = group_roots(&owners)?;
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut member_of_root = vec![usize::MAX; physical.len()];
        let mut positions = Vec::with_capacity(physical.len());
        for (index, root) in roots.into_iter().enumerate() {
            let member = &mut member_of_root[root];
            if *member == usize::MAX {
                *member = members.len();
                members.push(Vec::new());
            }
            positions.push((*member, members[*member].len()));
            members[*member].push(physical[index]);
        }
        if members.len() < 2 {
            return Ok(None);
        }
        let relocations = (0..physical.len()).map(|_| Vec::new()).collect();
        Ok(Some(Self { members, positions, owners, relocations }))
    }

    pub(super) fn relocation(&mut self, section: usize, offset: usize, kind: u16, target: ObjectTarget) {
        let target_kind = match target.kind {
            ObjectTargetKind::Section => BindingTarget::Section,
            ObjectTargetKind::Symbol => BindingTarget::Symbol,
            ObjectTargetKind::Import => BindingTarget::Import,
        };
        self.relocations[section].push(BindingRelocation {
            offset: offset as u64, kind, target: target_kind,
            value: target.value as u32, section: target.section as u64,
            name: binding_bytes(target.name), dll: binding_bytes(target.dll),
        });
    }

    fn combine_closed_groups(&mut self, raw: &[ObjectSection]) -> Option<usize> {
        let mut needs_resolution = vec![false; self.members.len()];
        let mut dependents = vec![Vec::new(); self.members.len()];
        let mut pending = Vec::new();
        for (section, relocations) in self.relocations.iter().enumerate() {
            let member = self.positions[section].0;
            for relocation in relocations {
                let external = match relocation.target {
                    BindingTarget::Section => {
                        let destination = self.positions[relocation.section as usize].0;
                        if destination != member {
                            dependents[destination].push(member);
                        }
                        false
                    }
                    BindingTarget::Symbol => true,
                    BindingTarget::Import => {
                        let dll = unsafe { std::slice::from_raw_parts(relocation.dll.bytes, relocation.dll.length) };
                        dll == crate::resource::RUNTIME_LIBRARY.as_bytes()
                    }
                };
                if external && !std::mem::replace(&mut needs_resolution[member], true) {
                    pending.push(member);
                }
            }
        }
        // A member needs independent extraction if any referenced group does.
        while let Some(member) = pending.pop() {
            for &dependent in &dependents[member] {
                if !std::mem::replace(&mut needs_resolution[dependent], true) {
                    pending.push(dependent);
                }
            }
        }
        let mut closed = needs_resolution.iter().enumerate()
            .filter_map(|(member, needs_resolution)| (!needs_resolution).then_some(member));
        let first = closed.next()?;
        let second = closed.next()?;
        for member in std::iter::once(second).chain(closed) {
            let group = std::mem::take(&mut self.members[member]);
            self.members[first].extend(group);
        }
        self.members[first].sort_unstable();
        for (index, &section) in self.members[first].iter().enumerate() {
            self.positions[raw[section].mapped - 1] = (first, index);
        }
        Some(first)
    }

    pub(super) fn encode(
        mut self, bytes: &[u8], raw: &[ObjectSection], symbols: &[ObjectSymbol],
        build: Option<&HostBuildSection>,
    ) -> Result<Vec<(Vec<u8>, Vec<String>)>> {
        let combined = self.combine_closed_groups(raw);
        let identity = member_identity(bytes, raw, build);
        let mut anchors = BTreeMap::new();
        for (source, relocations) in self.relocations.iter_mut().enumerate() {
            for relocation in relocations {
                if !matches!(relocation.target, BindingTarget::Section) {
                    continue;
                }
                let target = relocation.section as usize;
                let (member, local) = self.positions[target];
                if relocation.value == 0 && self.positions[source].0 == member {
                    relocation.section = local as u64;
                } else {
                    let value = relocation.value;
                    let name = anchors.entry((target, value)).or_insert_with(||
                        format!("__blink_host_section${identity}${target}${value}")).as_str();
                    relocation.target = BindingTarget::Symbol;
                    relocation.name = BindingBytes::new(name.as_bytes());
                }
            }
        }
        let mut definitions: Vec<Vec<BindingDefinition>> = self.members.iter().map(|_| Vec::new()).collect();
        let mut names: Vec<Vec<String>> = self.members.iter().map(|_| Vec::new()).collect();
        for symbol in symbols.iter().filter(|symbol| symbol.external != 0 && symbol.section > 0) {
            let index = object_index(raw, symbol.section as usize - 1)
                .ok_or("导出符号引用不可装载的节")?;
            let (member, section) = self.positions[index];
            let name = unsafe { std::str::from_utf8_unchecked(symbol.name.borrowed()) };
            definitions[member].push(BindingDefinition {
                name: BindingBytes::new(name.as_bytes()), section: section as u64,
                value: u64::from(symbol.value), function: u64::from(symbol.is_function()),
            });
            names[member].push(name.to_owned());
        }
        for (&(index, value), name) in &anchors {
            let (member, section) = self.positions[index];
            definitions[member].push(BindingDefinition {
                name: BindingBytes::new(name.as_bytes()), section: section as u64, value: u64::from(value), function: 0,
            });
            names[member].push(name.clone());
        }

        let mut output = Vec::with_capacity(self.members.len());
        for (member, physical) in self.members.iter().enumerate() {
            if physical.is_empty() {
                continue;
            }
            let mut associated = vec![Vec::new(); physical.len()];
            for (local, &index) in physical.iter().enumerate() {
                let section = raw[index].mapped - 1;
                if let Some(owner) = self.owners[section] {
                    associated[self.positions[owner].1].push(local);
                }
            }
            let bindings = physical.iter().zip(&associated).map(|(&index, children)| {
                let row = &raw[index];
                let references = &self.relocations[row.mapped - 1];
                BindingSection {
                    name: binding_bytes(row.name), flags: row.flags,
                    relocations: references.as_ptr(), relocation_count: references.len(),
                    associated: children.as_ptr(), associated_count: children.len(), owner: 0, symbol: 0,
                }
            }).collect();
            let mut payloads: Vec<_> = physical.iter().map(|&index| {
                let row = &raw[index];
                (row_name(row), row_payload(row), row.flags as u32)
            }).collect();
            if member == 0 {
                payloads.extend(raw.iter().filter(|row| row.mapped == 0 && row_name(row) != BUILD)
                    .map(|row| (row_name(row), row_payload(row), row.flags as u32)));
            }
            // Retain the original COMDAT namespace of each independent member.
            let group_name = if combined == Some(member) {
                format!("{identity}$closed${member}")
            } else {
                format!("{identity}${member}")
            };
            let code = encode_bound(&group_name, payloads, bindings, &definitions[member], None)?;
            output.push((code, std::mem::take(&mut names[member])));
        }
        Ok(output)
    }
}

fn binding_bytes(bytes: ReadBytes) -> BindingBytes {
    BindingBytes { bytes: bytes.bytes, length: bytes.length }
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

fn group_roots(owners: &[Option<usize>]) -> Result<Vec<usize>> {
    let visiting = owners.len();
    let mut roots = vec![usize::MAX; visiting];
    let mut path = Vec::new();
    for section in 0..visiting {
        let mut current = section;
        while roots[current] == usize::MAX {
            match owners[current] {
                Some(owner) => {
                    roots[current] = visiting;
                    path.push(current);
                    current = owner;
                }
                None => roots[current] = current,
            }
        }
        let root = roots[current];
        if root == visiting {
            return Err("COFF关联节形成循环".into());
        }
        for section in path.drain(..) {
            roots[section] = root;
        }
    }
    Ok(roots)
}

fn member_identity(bytes: &[u8], raw: &[ObjectSection], build: Option<&HostBuildSection>) -> String {
    let mut identity = crate::build::hash::Digest::new();
    let data = 20 + raw.len() * 40;
    for header in bytes[20..data].chunks_exact(40).filter(|header| &header[..8] != BUILD.as_bytes()) {
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
