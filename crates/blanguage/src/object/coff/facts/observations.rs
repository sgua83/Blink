//! Optional layout facts transferred from the original Object, never encoded as execution data.
use super::*;
mod abi;
mod source;
mod operations;
use crate::{build::hash, codec::{Decode, Encode, record, variants}, native::{Region, RegionKind, TailReference}};

pub(crate) enum Layout<A = crate::native::FunctionAbi, O = crate::native::OperationRange> {
    /// The original .pdata owns the main end; ABI facts and tails belong to this section.
    Function { tails: Vec<Region>, references: Option<Vec<TailReference>>, abi: Option<A>, operations: Option<Vec<O>> },
    /// Each original handwritten producer owns this entire code section.
    Startup,
    Print(crate::ir::Scalar),
    ImportThunk,
}
struct SectionLayout {
    section: usize,
    identity: [u8; 32],
    layout: Layout<abi::FunctionAbi, operations::OperationRange>,
}
pub(super) struct Material {
    sections: Vec<SectionLayout>,
}
variants!(@<A,O> Layout<A,O> { 0=>Function{tails,references,abi,operations}, 1=>Startup, 2=>Print(scalar), 3=>ImportThunk });
record!(TailReference { target, rel32_fields });
record!(SectionLayout { section, identity, layout });
record!(Material { sections });
const MAGIC: &[u8; 8] = b"BLCF\x05\0\0\0";

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
impl<A,O> Layout<A,O> {
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
impl Layout<abi::FunctionAbi, operations::OperationRange> {
    pub(super) fn operation_ranges(&self) -> String {
        match self {
            Self::Function { operations, .. } => operations.as_deref().map_or_else(|| "null".into(), operations::ranges_json),
            Self::Startup | Self::Print(_) | Self::ImportThunk => "null".into(),
        }
    }
    pub(super) fn function_abi(&self) -> String {
        match self {
            Self::Function { abi, .. } => abi.as_ref().map_or_else(|| "null".into(), abi::FunctionAbi::json),
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
            if let Layout::Function { abi, operations, .. } = &saved.layout {
                if let Some(abi) = abi { abi.validate(regions[0].2)?; }
                if let Some(operations) = operations {
                    if operations.iter().any(|range| range.start > range.end || range.end > regions[0].2) {
                        return Err("COFF operation range is outside its original main function".into());
                    }
                }
            }
            if let Layout::Function { references: Some(references), .. } = &saved.layout {
                let code = unsafe { section.row.bytes.borrowed() };
                for reference in references {
                    if reference.target > code.len() { return Err("COFF tail reference target is outside its code section".into()); }
                    for &field in &reference.rel32_fields {
                        let value = code.get(field..).and_then(|bytes| bytes.get(..4))
                            .ok_or("COFF tail reference field is outside its code section")?;
                        let displacement = i32::from_le_bytes(value.try_into()?);
                        if field as i128 + 4 + i128::from(displacement) != reference.target as i128 {
                            return Err("COFF tail reference does not match its original rel32 field".into());
                        }
                    }
                }
            }
        }
        Ok(value)
    }
    pub(super) fn layout(&self, index: usize) -> Option<&Layout<abi::FunctionAbi, operations::OperationRange>> {
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
