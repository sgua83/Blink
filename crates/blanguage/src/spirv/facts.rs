//! Compiler-only queries borrow every module from its original image container.
use super::{DecodeError, ProgramImage, ShaderCode, StageCode, module_json};

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
    /// Reads these original words and the original sort rank role without changing either.
    pub fn code_json(&self) -> Result<String> {
        Ok(object(&[("kind", text("spirv_shader")), ("ranks", self.ranks.to_string()),
            ("module", module_json(&self.words).map_err(|error| at(error, "words"))?)]))
    }
}
impl StageCode {
    /// Optional fields and sort order describe the original container, not dispatch history.
    pub fn code_json(&self) -> Result<String> {
        let finish = match &self.finish {
            None => "null".into(),
            Some((name, words)) => object(&[("name", text(name)),
                ("module", module_json(words).map_err(|error| at(error, "finish"))?)]),
        };
        let sort = match &self.sort {
            None => "null".into(),
            Some(shaders) => array(shaders.iter().enumerate().map(|(index, shader)| {
                shader.code_json().map_err(|error| at(error, &format!("sort[{index}]")))
            }).collect::<Result<Vec<_>>>()?),
        };
        Ok(object(&[("kind", text("spirv_stage")),
            ("main", module_json(&self.main).map_err(|error| at(error, "main"))?),
            ("tail", optional(&self.tail, "tail")?), ("scan_add", optional(&self.scan_add, "scan_add")?),
            ("finish", finish), ("dispatch", optional(&self.dispatch, "dispatch")?),
            ("selector", optional(&self.selector, "selector")?), ("sort", sort)]))
    }
}
impl<P> ProgramImage<P> {
    /// Reads every original stage/module; offsets and IDs are local to each module.
    pub fn code_json(&self) -> Result<String> {
        let stages = self.stages().iter().enumerate().map(|(index, stage)| {
            stage.code_json().map_err(|error| at(error, &format!("stage[{index}]")))
        }).collect::<Result<Vec<_>>>()?;
        Ok(object(&[("kind", text("gpu_program_image")), ("program", text(self.program().name())),
            ("offset_basis", text("each_module_relative")), ("stages", array(stages))]))
    }
}
