//! Optional layout facts transferred from the original Object, never encoded as execution data.
use super::*;
use crate::{build::hash, codec::{Decode, Encode, record, variants}, native::{Region, RegionKind, TailReference}};

pub(crate) enum Layout<S = crate::native::CodeSources> {
    /// The original .pdata owns the main end; ABI facts and tails belong to this section.
    Function { tails: Vec<Region>, references: Option<Vec<TailReference>>, sources: S },
    /// Each original handwritten producer owns this entire code section.
    Startup,
    Print(crate::ir::Scalar),
    ImportThunk,
}
struct SectionLayout {
    section: usize,
    identity: [u8; 32],
    layout: Layout<crate::native::SavedSources>,
}
pub(super) struct Material {
    sections: Vec<SectionLayout>,
}
variants!(@<S> Layout<S> { 0=>Function{tails,references,sources}, 1=>Startup, 2=>Print(scalar), 3=>ImportThunk });
record!(SectionLayout { section, identity, layout });
record!(Material { sections });
const MAGIC: &[u8; 8] = b"BLCF\x0a\0\0\0";

fn identity(facts: &Facts<'_>, index: usize, spans: &[Span]) -> [u8; 32] {
    let section = &facts.sections[index];
    let mut bytes = Vec::new();
    hash::field(&mut bytes, unsafe { section.row.bytes.borrowed() });
    hash::field(&mut bytes, &section.row.flags.to_le_bytes());
    hash::field(&mut bytes, &(section.relocations.len() as u64).to_le_bytes());
    for relocation in &section.relocations {
        let symbol = facts.symbols[relocation.symbol].as_ref().unwrap();
        hash::field(&mut bytes, &(relocation.offset as u64).to_le_bytes());
        hash::field(&mut bytes, &relocation.kind.to_le_bytes());
        hash::field(&mut bytes, unsafe { symbol.name.borrowed() });
        hash::field(&mut bytes, &symbol.section.to_le_bytes());
        hash::field(&mut bytes, &symbol.value.to_le_bytes());
    }
    let ranges = spans.iter().filter(|s| s.section == index).collect::<Vec<_>>();
    hash::field(&mut bytes, &(ranges.len() as u64).to_le_bytes());
    for range in ranges {
        hash::field(&mut bytes, &(range.start as u64).to_le_bytes());
        hash::field(&mut bytes, &(range.end as u64).to_le_bytes());
    }
    hash::digest(&bytes)
}
impl<S> Layout<S> {
    pub(super) fn regions(&self, index: usize, length: usize, spans: &[Span]) -> Result<Vec<(&'static str, usize, usize)>> {
        match self {
            Self::Startup | Self::Print(_) | Self::ImportThunk => Ok(vec![("generated_code", 0, length)]),
            Self::Function { tails, .. } => {
                let mut main = spans.iter().filter(|s| s.section == index && s.start == 0);
                let end = main.next().ok_or("COFF generated function material has no matching original .pdata range")?.end;
                if main.any(|s| s.end != end) { return Err("COFF original main range is ambiguous".into()); }
                let mut regions = vec![("runtime_function", 0, end)];
                let mut start = end;
                for tail in tails {
                    if tail.end < start || tail.end > length { return Err("COFF generated tail is outside or out of order in its section".into()); }
                    let kind = match tail.kind { RegionKind::StackProbe => "stack_probe", RegionKind::Alignment => "alignment", RegionKind::Literal => "literal" };
                    regions.push((kind, start, tail.end)); start = tail.end;
                }
                if start != length { return Err("COFF generated ranges do not cover the complete section".into()); }
                Ok(regions)
            }
        }
    }
    pub(super) fn references(&self) -> Option<&[TailReference]> {
        match self {
            Self::Function { references, .. } => references.as_deref(),
            Self::Startup | Self::Print(_) | Self::ImportThunk => None,
        }
    }
    pub(super) fn generation(&self) -> String {
        match self {
            Self::Function { .. } => "null".into(),
            Self::Startup => object(&[("reason", text("application_startup"))]),
            Self::Print(scalar) => object(&[("reason", text("scalar_output")), ("scalar", text(scalar.name()))]),
            Self::ImportThunk => object(&[("reason", text("import_thunk"))]),
        }
    }

}
impl Layout<crate::native::SavedSources> {
    pub(super) fn operation_ranges(&self, selected: Option<&std::ops::Range<usize>>) -> String {
        match self {
            Self::Function { sources, .. } => sources.operation_ranges_json(selected),
            Self::Startup | Self::Print(_) | Self::ImportThunk => "null".into(),
        }
    }
    pub(super) fn function_abi(&self, selected: Option<&std::ops::Range<usize>>) -> String {
        match self {
            Self::Function { sources, .. } => sources.function_abi_json(selected),
            Self::Startup | Self::Print(_) | Self::ImportThunk => "null".into(),
        }
    }
}

impl Material {
    pub(super) fn read(facts: &Facts<'_>, spans: &[Span], bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_prefix(MAGIC).ok_or("unsupported COFF code observation material")?;
        let value = Self::from_bytes(bytes)?;
        let code = facts.sections.iter().enumerate().filter(|(_, s)| s.row.flags & 0x20 != 0).collect::<Vec<_>>();
        if value.sections.len() != code.len() { return Err("COFF observation code section count does not match".into()); }
        for (saved, (index, section)) in value.sections.iter().zip(code) {
            if saved.section != index + 1 || saved.identity != identity(facts, index, spans) {
                return Err("COFF observation material does not match the original code section and relocations".into());
            }
            let regions = saved.layout.regions(index, section.row.bytes.length, spans)?;
            if let Layout::Function { sources, references, .. } = &saved.layout {
                sources.validate(regions[0].2)?;
                if let Some(references) = references {
                    crate::native::material::references(unsafe { section.row.bytes.borrowed() }, references)?;
                }
            }
        }
        Ok(value)
    }
    pub(super) fn layout(&self, index: usize) -> Option<&Layout<crate::native::SavedSources>> {
        self.sections.iter().find(|section| section.section == index + 1).map(|section| &section.layout)
    }
}
pub(crate) fn capture(object: &Object) -> Result<Vec<u8>> {
    let layouts = object.sections.iter().enumerate().filter(|(_, section)| section.flags & 0x20 != 0);
    if layouts.clone().any(|(_, section)| section.layout.is_none()) {
        return Err("COFF producer code ranges were not retained".into());
    }
    let encoded = super::super::encode(object)?;
    let facts = Facts::read(&encoded)?;
    let (spans, _) = facts.ranges()?;
    let mut writer = crate::codec::Writer(MAGIC.to_vec());
    layouts.clone().count().encode(&mut writer);
    for (index, section) in layouts {
        let layout = section.layout.as_ref().unwrap();
        layout.regions(index, section.bytes.len(), &spans)?;
        (index + 1).encode(&mut writer);
        identity(&facts, index, &spans).encode(&mut writer);
        layout.encode(&mut writer);
    }
    Ok(writer.0)
}
