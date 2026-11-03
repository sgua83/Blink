//! Optional CPU range material. Execution codecs and runtime owners do not consume it.
use super::{Compilation, Result, hash};
use crate::{native::{ContinuationCode, NativeCode, Region, RegionKind, StageCode}, resource};
use crate::codec::{Decode, Encode, record, variants};
use std::path::Path;

const MAGIC: &[u8; 8] = b"BLCR\x01\0\0\0";
struct CodeRanges {
    identity: [u8; 32],
    tails: Vec<Region>,
}
struct Observations {
    codes: Vec<CodeRanges>,
}
record!(CodeRanges { identity, tails });
record!(Observations { codes });
variants!(RegionKind { 0=>StackProbe, 1=>Alignment, 2=>Literal });
record!(Region { end, kind });

fn codes(stage: &StageCode) -> impl Iterator<Item = &NativeCode> {
    let (first, rest): (&[NativeCode], [Option<&NativeCode>; 3]) = match stage {
        StageCode::Element { main, continuation, .. } => {
            let rest = match continuation {
                None => [None, None, None],
                Some(ContinuationCode::ReductionTail(code) | ContinuationCode::ScanAdd(code)
                    | ContinuationCode::CompactScatter(code) | ContinuationCode::SegmentedFinish(code)) => [Some(code), None, None],
                Some(ContinuationCode::Scan { prefix, apply, .. }) => [Some(prefix), Some(apply), None],
            };
            (std::slice::from_ref(main), rest)
        }
        StageCode::Sort(code) => (&code.initialize, [Some(&code.histogram), Some(&code.scatter), Some(&code.gather)]),
    };
    first.iter().chain(rest.into_iter().flatten())
}
fn codes_mut(stage: &mut StageCode) -> impl Iterator<Item = &mut NativeCode> {
    let (first, rest): (&mut [NativeCode], [Option<&mut NativeCode>; 3]) = match stage {
        StageCode::Element { main, continuation, .. } => {
            let rest = match continuation {
                None => [None, None, None],
                Some(ContinuationCode::ReductionTail(code) | ContinuationCode::ScanAdd(code)
                    | ContinuationCode::CompactScatter(code) | ContinuationCode::SegmentedFinish(code)) => [Some(code), None, None],
                Some(ContinuationCode::Scan { prefix, apply, .. }) => [Some(prefix), Some(apply), None],
            };
            (std::slice::from_mut(main), rest)
        }
        StageCode::Sort(code) => (&mut code.initialize, [Some(&mut code.histogram), Some(&mut code.scatter), Some(&mut code.gather)]),
    };
    first.iter_mut().chain(rest.into_iter().flatten())
}
fn image_codes<P>(image: &resource::Image<P>) -> impl Iterator<Item = &NativeCode> {
    let stages = match image { resource::Image::Cpu(image) => image.stages(), resource::Image::Gpu(_) => &[] };
    stages.iter().flat_map(codes)
}
fn image_codes_mut<P>(image: &mut resource::Image<P>) -> impl Iterator<Item = &mut NativeCode> {
    let stages = match image { resource::Image::Cpu(image) => image.stages_mut(), resource::Image::Gpu(_) => &mut [] };
    stages.iter_mut().flat_map(codes_mut)
}
fn capture<'a>(codes: impl Iterator<Item = &'a NativeCode>) -> Result<Vec<u8>> {
    let codes = codes.map(|code| {
        let tails = code.tail_regions.as_ref().ok_or("CPU observation ranges were not retained")?;
        ranges(code, tails)?;
        Ok(CodeRanges { identity: hash::digest(&code.bytes()), tails: tails.clone() })
    }).collect::<Result<Vec<_>>>()?;
    if codes.is_empty() { return Err("no CPU code images; GPU range observations are not supported".into()); }
    let mut bytes = MAGIC.to_vec();
    bytes.extend(Observations { codes }.bytes());
    Ok(bytes)
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
    if codes.is_empty() { return Err("no CPU code images; GPU range observations are not supported".into()); }
    if codes.len() != material.codes.len() { return Err("CPU observation code count does not match".into()); }
    for (code, saved) in codes.iter().zip(&material.codes) {
        if hash::digest(&code.bytes()) != saved.identity { return Err("CPU observation material does not match execution code".into()); }
        ranges(code, &saved.tails)?;
    }
    for (code, saved) in codes.iter_mut().zip(material.codes) {
        code.tail_regions = Some(saved.tails);
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
        let images = self.images.iter().filter(|i| i.prefix_requirements.is_empty())
            .chain(self.images.iter().filter(|i| !i.prefix_requirements.is_empty()));
        capture(images.flat_map(image_codes))
    }
    /// Restores only matching observation fields. Execution ownership and contracts are unchanged.
    pub fn restore_code_observations(&mut self, material: &[u8]) -> Result<()> {
        let (programs, templates): (Vec<_>, Vec<_>) = self.images.iter_mut().partition(|i| i.prefix_requirements.is_empty());
        restore(programs.into_iter().chain(templates).flat_map(image_codes_mut), material)
    }
}
fn image_json<P>(images: &[resource::Image<P>]) -> Result<String> {
    let values = images.iter().enumerate().map(|(index, image)| {
        Ok(match image {
            resource::Image::Cpu(image) => format!("{{\"index\":{index},\"target\":\"cpu\",\"code\":{}}}",
                image.code_json().map_err(|e| std::io::Error::other(e.to_string()))?),
            resource::Image::Gpu(_) => format!("{{\"index\":{index},\"target\":\"gpu\",\"code\":null,\"status\":\"not_supported\"}}"),
        })
    }).collect::<Result<Vec<_>>>()?;
    Ok(format!("[{}]", values.join(",")))
}
/// Reads saved CPU facts for a BLBI without source, compiler workers, devices or code execution.
/// Pairing checks equality, not authenticity; material must come from the original producer.
pub fn code_facts(path: &Path, material: &[u8]) -> Result<String> {
    let mut compilation = unsafe { super::load(path) }?;
    compilation.restore_code_observations(material)?;
    Ok(format!("{{\"kind\":\"build_cpu_code\",\"programs\":{},\"templates\":{}}}",
        image_json(&compilation.programs)?, image_json(&compilation.templates)?))
}
/// Reads an explicitly selected BLRI; no executable memory or runtime resource is created.
pub fn image_code_facts(execution: &[u8], material: &[u8]) -> Result<String> {
    let mut module = unsafe { resource::from_compiler_output(execution) }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    module.restore_code_observations(material)?;
    Ok(format!("{{\"kind\":\"execution_image_cpu_code\",\"images\":{}}}", image_json(&module.images)?))
}
