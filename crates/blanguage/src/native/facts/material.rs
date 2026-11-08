//! Source-free encoding belongs to the original CPU facts, shared by BLCR and COFF.
mod source;
mod abi;
mod operations;
use crate::{codec::{Encode, Writer, record}, native::{CodeSources, TailReference}};
type Result<T> = crate::codec::Result<T>;

/// Decoded source facts contain identity and coordinates, never an invented Source or IR.
#[derive(Clone, Debug)]
pub struct SavedSources {
    function_abi: Option<abi::FunctionAbi>,
    operation_ranges: Option<Vec<operations::OperationRange>>,
}
record!(SavedSources { function_abi, operation_ranges });
record!(TailReference { target, rel32_fields });
impl Encode for CodeSources {
    fn encode(&self, writer: &mut Writer) {
        match self {
            Self::Emitted { function_abi, operation_ranges } => {
                function_abi.encode(writer); operation_ranges.encode(writer);
            }
            Self::Restored(saved) => saved.encode(writer),
            Self::Unavailable => { false.encode(writer); false.encode(writer); }
        }
    }
}
impl SavedSources {
    pub(crate) fn function_abi_json(&self, selected: Option<&std::ops::Range<usize>>) -> String {
        self.function_abi.as_ref().map_or_else(|| "null".into(), |abi| abi.json(selected))
    }
    pub(crate) fn operation_ranges_json(&self, selected: Option<&std::ops::Range<usize>>) -> String {
        self.operation_ranges.as_deref().map_or_else(|| "null".into(), |ranges| operations::ranges_json(ranges, selected))
    }
    pub(crate) fn validate(&self, main_end: usize) -> Result<()> {
        if let Some(abi) = &self.function_abi { abi.validate(main_end)?; }
        if let Some(operations) = &self.operation_ranges {
            if operations.iter().any(|range| range.start > range.end || range.end > main_end) {
                return Err("CPU operation range is outside its original main function".into());
            }
        }
        Ok(())
    }
}
/// Checks the actual saved fields against their original, already patched bytes.
pub(crate) fn references(code: &[u8], references: &[TailReference]) -> Result<()> {
    for reference in references {
        if reference.target > code.len() { return Err("CPU tail reference target is outside its code image".into()); }
        for &field in &reference.rel32_fields {
            let value = code.get(field..).and_then(|bytes| bytes.get(..4))
                .ok_or("CPU tail reference field is outside its code image")?;
            let displacement = i32::from_le_bytes(value.try_into()?);
            if field as i128 + 4 + i128::from(displacement) != reference.target as i128 {
                return Err("CPU tail reference does not match its original rel32 field".into());
            }
        }
    }
    Ok(())
}
