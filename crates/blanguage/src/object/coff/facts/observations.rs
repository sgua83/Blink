//! Optional layout facts transferred from the original Object, never encoded as execution data.
use super::*;
use crate::{build::hash, codec::{Decode, Encode, record, variants}, native::{Region, RegionKind}};

#[derive(Clone)]
pub(crate) enum Layout {
    /// The main end remains in the original .pdata; only emitted tails are retained here.
    Function(Vec<Region>),
    /// The original handwritten producer emitted this entire section as code.
    Generated,
}
struct SectionLayout {
    section: usize,
    identity: [u8; 32],
    layout: Layout,
}
pub(super) struct Material {
    sections: Vec<SectionLayout>,
}
variants!(Layout { 0=>Function(tails), 1=>Generated });
record!(SectionLayout { section, identity, layout });
record!(Material { sections });
const MAGIC: &[u8; 8] = b"BLCF\x01\0\0\0";

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
impl Layout {
    fn regions(&self, index: usize, length: usize, spans: &[Span]) -> Result<Vec<(&'static str, usize, usize)>> {
        match self {
            Self::Generated => Ok(vec![("generated_code", 0, length)]),
            Self::Function(tails) => {
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
            saved.layout.regions(index, section.row.bytes.length, spans)?;
        }
        Ok(value)
    }
    pub(super) fn regions(&self, index: usize, length: usize, spans: &[Span]) -> Result<Vec<(&'static str, usize, usize)>> {
        // read() has already matched every code section in its original order.
        self.sections.iter().find(|s| s.section == index + 1).unwrap().layout.regions(index, length, spans)
    }
}
pub(crate) fn capture(object: &Object) -> Result<Vec<u8>> {
    let layouts = object.sections.iter().enumerate().filter(|(_, s)| s.flags & 0x20 != 0)
        .map(|(index, section)| Ok((index, section, section.layout.as_ref().ok_or("COFF producer code ranges were not retained")?)))
        .collect::<Result<Vec<_>>>()?;
    let encoded = super::super::encode(object)?;
    let facts = Facts::read(&encoded)?;
    let (spans, _) = facts.ranges()?;
    let sections = layouts.into_iter().map(|(index, section, layout)| {
        layout.regions(index, section.bytes.len(), &spans)?;
        Ok(SectionLayout { section: index + 1, identity: identity(&facts, index, &spans), layout: layout.clone() })
    }).collect::<Result<Vec<_>>>()?;
    let mut bytes = MAGIC.to_vec(); bytes.extend(Material { sections }.bytes()); Ok(bytes)
}
