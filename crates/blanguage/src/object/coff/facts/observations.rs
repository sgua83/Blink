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
    let mut digest = hash::Digest::new();
    digest.field(unsafe { section.row.bytes.borrowed() });
    digest.field(&section.row.flags.to_le_bytes());
    digest.field(&(section.relocations.len() as u64).to_le_bytes());
    for relocation in &section.relocations {
        let symbol = facts.symbols[relocation.symbol].as_ref().unwrap();
        digest.field(&(relocation.offset as u64).to_le_bytes());
        digest.field(&relocation.kind.to_le_bytes());
        digest.field(unsafe { symbol.name.borrowed() });
        digest.field(&symbol.section.to_le_bytes());
        digest.field(&symbol.value.to_le_bytes());
    }
    digest.field(&(spans.len() as u64).to_le_bytes());
    for range in spans {
        digest.field(&(range.start as u64).to_le_bytes());
        digest.field(&(range.end as u64).to_le_bytes());
    }
    digest.finish()
}
impl<S> Layout<S> {
    pub(super) fn regions(&self, length: usize, spans: &[Span])
        -> Result<(usize, impl Iterator<Item = (&'static str, usize, usize)> + '_)>
    {
        let (kind, end, tails) = match self {
            Self::Startup | Self::Print(_) | Self::ImportThunk => ("generated_code", length, &[][..]),
            Self::Function { tails, .. } => {
                let mut main = spans.iter().filter(|span| span.start == 0);
                let end = main.next().ok_or("COFF generated function material has no matching original .pdata range")?.end;
                if main.any(|s| s.end != end) { return Err("COFF original main range is ambiguous".into()); }
                let mut start = end;
                for tail in tails {
                    if tail.end < start || tail.end > length { return Err("COFF generated tail is outside or out of order in its section".into()); }
                    start = tail.end;
                }
                if start != length { return Err("COFF generated ranges do not cover the complete section".into()); }
                ("runtime_function", end, tails.as_slice())
            }
        };
        let regions = std::iter::once((kind, 0, end)).chain(tails.iter().scan(end, |start, tail| {
            let kind = match tail.kind { RegionKind::StackProbe => "stack_probe", RegionKind::Alignment => "alignment", RegionKind::Literal => "literal" };
            let region = (kind, *start, tail.end);
            *start = tail.end;
            Some(region)
        }));
        Ok((end, regions))
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
        let code = facts.sections.iter().enumerate().filter(|(_, s)| s.row.flags & 0x20 != 0);
        if value.sections.len() != code.clone().count() { return Err("COFF observation code section count does not match".into()); }
        for (saved, (index, section)) in value.sections.iter().zip(code) {
            let ranges = section_spans(spans, index);
            if saved.section != index + 1 || saved.identity != identity(facts, index, ranges) {
                return Err("COFF observation material does not match the original code section and relocations".into());
            }
            let (end, _) = saved.layout.regions(section.row.bytes.length, ranges)?;
            if let Layout::Function { sources, references, .. } = &saved.layout {
                sources.validate(end)?;
                if let Some(references) = references {
                    crate::native::material::references(unsafe { section.row.bytes.borrowed() }, references)?;
                }
            }
        }
        Ok(value)
    }
    pub(super) fn layout(&self, index: usize) -> Option<&Layout<crate::native::SavedSources>> {
        // Reading material verified this order against the original physical code sections.
        self.sections.binary_search_by_key(&(index + 1), |section| section.section)
            .ok().map(|position| &self.sections[position].layout)
    }
}
pub(crate) fn capture(object: &Object) -> Result<Vec<u8>> {
    let layouts = object.sections.iter().enumerate().filter(|(_, section)| section.flags & 0x20 != 0);
    if layouts.clone().any(|(_, section)| section.layout.is_none()) {
        return Err("COFF producer code ranges were not retained".into());
    }
    let encoded = super::super::encode(object)?;
    let facts = Facts::read(&encoded)?;
    let spans = facts.ranges(|_, _, _, _| {})?;
    let mut writer = crate::codec::Writer(MAGIC.to_vec());
    layouts.clone().count().encode(&mut writer);
    for (index, section) in layouts {
        let layout = section.layout.as_ref().unwrap();
        let length = match &section.bytes {
            SectionData::Initialized(bytes) => bytes.len(),
            SectionData::ZeroFill(length) => *length,
        };
        let ranges = section_spans(&spans, index);
        let _ = layout.regions(length, ranges)?;
        (index + 1).encode(&mut writer);
        identity(&facts, index, ranges).encode(&mut writer);
        layout.encode(&mut writer);
    }
    Ok(writer.0)
}
