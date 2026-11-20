//! Compiler-only queries borrow every module from its original image container.
use super::{AdditionalShaders, DecodeError, ProgramImage, ShaderCode, StageCode, module_json};
use crate::ir::Operation;

type Result<T> = std::result::Result<T, DecodeError>;
fn text(value: &str) -> String { let mut out = String::new(); crate::json::string(&mut out, value); out }
fn array(values: impl IntoIterator<Item = String>) -> String { format!("[{}]", values.into_iter().collect::<Vec<_>>().join(",")) }
fn object(values: &[(&str, String)]) -> String {
    format!("{{{}}}", values.iter().map(|(key, value)| format!("{}:{value}", text(key))).collect::<Vec<_>>().join(","))
}
fn at(mut error: DecodeError, location: &str) -> DecodeError {
    error.reason = format!("{location}: {}", error.reason); error
}
fn optional(words: &Option<Vec<u32>>, field: &str) -> Result<String> {
    words.as_deref().map_or_else(|| Ok("null".into()), |words| module_json(words).map_err(|error| at(error, field)))
}
impl ShaderCode {
    /// Reads these original words and the optional subgroup rank requirement.
    pub fn code_json(&self) -> Result<String> {
        Ok(object(&[("kind", text("spirv_shader")), ("ranks", self.ranks.to_string()),
            ("module", module_json(&self.words).map_err(|error| at(error, "words"))?)]))
    }
}
impl StageCode {
    fn code_json<R>(&self, operation: &Operation<R>) -> Result<String> {
        let finish = match &self.finish {
            None => "null".into(),
            Some((name, words)) => object(&[("name", text(name)),
                ("module", module_json(words).map_err(|error| at(error, "finish"))?)]),
        };
        let additional = self.additional_shaders(operation);
        let (sort, single_group) = match additional {
            Some(AdditionalShaders::Sort(shaders)) => (array(shaders.iter().enumerate().map(|(index, shader)| {
                shader.code_json().map_err(|error| at(error, &format!("sort[{index}]")))
            }).collect::<Result<Vec<_>>>()?), "null".into()),
            Some(AdditionalShaders::SegmentedSingleGroup(shader)) =>
                ("null".into(), shader.code_json().map_err(|error| at(error, "single_group"))?),
            None => ("null".into(), "null".into()),
        };
        Ok(object(&[("kind", text("spirv_stage")),
            ("main", module_json(&self.main).map_err(|error| at(error, "main"))?),
            ("tail", optional(&self.tail, "tail")?), ("scan_add", optional(&self.scan_add, "scan_add")?),
            ("finish", finish), ("dispatch", optional(&self.dispatch, "dispatch")?),
            ("selector", optional(&self.selector, "selector")?), ("sort", sort),
            ("single_group", single_group)]))
    }
}
impl<P> ProgramImage<P> {
    /// Reads every original stage/module; offsets and IDs are local to each module.
    pub fn code_json(&self) -> Result<String> {
        let stages = self.stages().iter().zip(self.program().stages()).enumerate().map(|(index, (stage, layout))| {
            stage.code_json(&layout.operation).map_err(|error| at(error, &format!("stage[{index}]")))
        }).collect::<Result<Vec<_>>>()?;
        Ok(object(&[("kind", text("gpu_program_image")), ("program", text(self.program().name())),
            ("offset_basis", text("each_module_relative")), ("stages", array(stages))]))
    }
}
