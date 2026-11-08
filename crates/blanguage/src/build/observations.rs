//! Optional CPU range material. Execution codecs and runtime owners do not consume it.
use super::{Compilation, Result, hash};
use crate::{native::{CodeSources, NativeCode, Region, RegionKind, SavedSources, TailReference}, resource};
use crate::codec::{Decode, Encode, Writer, record, variants};
use std::path::Path;

const MAGIC: &[u8; 8] = b"BLCR\x02\0\0\0";
struct CodeRanges {
    identity: [u8; 32],
    tails: Vec<Region>,
    references: Option<Vec<TailReference>>,
    sources: SavedSources,
}
struct Observations {
    codes: Vec<CodeRanges>,
}
record!(CodeRanges { identity, tails, references, sources });
record!(Observations { codes });
variants!(RegionKind { 0=>StackProbe, 1=>Alignment, 2=>Literal });
record!(Region { end, kind });

fn image_codes<P>(image: &resource::Image<P>) -> impl Iterator<Item = &NativeCode> {
    let cpu = match image { resource::Image::Cpu(image) => Some(image), resource::Image::Gpu(_) => None };
    cpu.into_iter().flat_map(crate::native::ProgramImage::code)
}
fn image_codes_mut<P>(image: &mut resource::Image<P>) -> impl Iterator<Item = &mut NativeCode> {
    let cpu = match image { resource::Image::Cpu(image) => Some(image), resource::Image::Gpu(_) => None };
    cpu.into_iter().flat_map(crate::native::ProgramImage::code_mut)
}
fn capture<'a>(codes: impl Iterator<Item = &'a NativeCode>) -> Result<Vec<u8>> {
    let codes = codes.collect::<Vec<_>>();
    if codes.is_empty() { return Err("no CPU code images for CPU range observation material".into()); }
    let mut writer = Writer(MAGIC.to_vec());
    codes.len().encode(&mut writer);
    for code in codes {
        let tails = code.tail_regions.as_ref().ok_or("CPU observation ranges were not retained")?;
        ranges(code, tails)?;
        hash::digest(&code.bytes()).encode(&mut writer);
        tails.encode(&mut writer); code.tail_references.encode(&mut writer); code.sources.encode(&mut writer);
    }
    Ok(writer.0)
}

fn ranges(code: &NativeCode, tails: &[Region]) -> Result<()> {
    let mut end = code.function_end;
    if end > code.bytes.len() { return Err("CPU main function is outside its code image".into()); }
    for part in tails {
        if part.end < end || part.end > code.bytes.len() {
            return Err("CPU observation range is outside or out of order in its code image".into());
        }
        end = part.end;
    }
    if end != code.bytes.len() { return Err("CPU observation ranges do not cover the complete code image".into()); }
    Ok(())
}
fn restore<'a>(codes: impl Iterator<Item = &'a mut NativeCode>, bytes: &[u8]) -> Result<()> {
    let bytes = bytes.strip_prefix(MAGIC).ok_or("unsupported CPU range observation material")?;
    let material = Observations::from_bytes(bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut codes = codes.collect::<Vec<_>>();
    if codes.is_empty() { return Err("no CPU code images for CPU range observation material".into()); }
    if codes.len() != material.codes.len() { return Err("CPU observation code count does not match".into()); }
    for (code, saved) in codes.iter().zip(&material.codes) {
        if hash::digest(&code.bytes()) != saved.identity { return Err("CPU observation material does not match execution code".into()); }
        ranges(code, &saved.tails)?;
        saved.sources.validate(code.function_end).map_err(|error| std::io::Error::other(error.to_string()))?;
        if let Some(references) = &saved.references {
            crate::native::material::references(&code.bytes, references).map_err(|error| std::io::Error::other(error.to_string()))?;
        }
    }
    for (code, saved) in codes.iter_mut().zip(material.codes) {
        code.tail_regions = Some(saved.tails);
        code.tail_references = saved.references;
        code.sources = CodeSources::Restored(Box::new(saved.sources));
    }
    Ok(())
}
impl Compilation {
    /// Captures original CPU range facts. A cache-only result without facts returns an error.
    /// The caller owns persistence; these bytes contain no execution code or execution plan.
    pub fn code_observations(&self) -> Result<Vec<u8>> {
        capture(self.programs.iter().flat_map(image_codes).chain(self.templates.iter().flat_map(image_codes)))
    }
    /// Attaches matching facts to the original code images without changing execution bytes.
    /// All entries are checked before any observation field is changed.
    pub fn restore_code_observations(&mut self, material: &[u8]) -> Result<()> {
        restore(self.programs.iter_mut().flat_map(image_codes_mut).chain(self.templates.iter_mut().flat_map(image_codes_mut)), material)
    }
}
impl resource::Module {
    /// Captures original CPU facts; deserialized execution-only images have none to export.
    pub fn code_observations(&self) -> Result<Vec<u8>> {
        // The original build result partitions unconditional images before conditional templates.
        let images = self.images.iter().filter(|i| !crate::ir::ProgramInterface::requires_bindings(*i))
            .chain(self.images.iter().filter(|i| crate::ir::ProgramInterface::requires_bindings(*i)));
        capture(images.flat_map(image_codes))
    }
    /// Restores only matching observation fields. Execution ownership and contracts are unchanged.
    pub fn restore_code_observations(&mut self, material: &[u8]) -> Result<()> {
        let (programs, templates): (Vec<_>, Vec<_>) = self.images.iter_mut().partition(|i| !crate::ir::ProgramInterface::requires_bindings(*i));
        restore(programs.into_iter().chain(templates).flat_map(image_codes_mut), material)
    }
}
fn image_json<P>(images: &[resource::Image<P>]) -> Result<String> {
    let values = images.iter().enumerate().map(|(index, image)| {
        Ok(match image {
            resource::Image::Cpu(image) => format!("{{\"index\":{index},\"target\":\"cpu\",\"code\":{}}}",
                image.code_json().map_err(|e| std::io::Error::other(e.to_string()))?),
            resource::Image::Gpu(image) => format!("{{\"index\":{index},\"target\":\"gpu\",\"code\":{}}}",
                image.code_json()?),
        })
    }).collect::<Result<Vec<_>>>()?;
    Ok(format!("[{}]", values.join(",")))
}
/// Reads saved CPU ranges and original GPU modules without sources, workers or execution.
/// Pairing checks equality, not authenticity; material must come from the original producer.
pub fn code_facts(path: &Path, material: Option<&[u8]>) -> Result<String> {
    let mut compilation = unsafe { super::load(path) }?;
    if let Some(material) = material { compilation.restore_code_observations(material)?; }
    Ok(format!("{{\"kind\":\"build_code\",\"programs\":{},\"templates\":{}}}",
        image_json(&compilation.programs)?, image_json(&compilation.templates)?))
}
/// Reads an explicitly selected BLRI; no executable memory or runtime resource is created.
pub fn image_code_facts(execution: &[u8], material: Option<&[u8]>) -> Result<String> {
    let mut module = unsafe { resource::from_compiler_output(execution) }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    if let Some(material) = material { module.restore_code_observations(material)?; }
    Ok(format!("{{\"kind\":\"execution_image_code\",\"images\":{}}}", image_json(&module.images)?))
}
