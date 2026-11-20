//! Compiler-only queries borrow every module from its original image container.
use super::{DecodeError, ProgramImage, ShaderCode, StageCode, TerminalScan, module_json};
use crate::ir::{ExecutionPlan, ExecutionStage};

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
    fn code_json(&self, plan: &ExecutionPlan, stage: &ExecutionStage) -> Result<String> {
        let finish = match &self.finish {
            None => "null".into(),
            Some((name, words)) => object(&[("name", text(name)),
                ("module", module_json(words).map_err(|error| at(error, "finish"))?)]),
        };
        let additional = self.additional_shaders(plan, stage);
        let sort = if additional.sort.is_empty() {
            "null".into()
        } else {
            array(additional.sort.iter().enumerate().map(|(index, shader)| {
                shader.code_json().map_err(|error| at(error, &format!("sort[{index}]")))
            }).collect::<Result<Vec<_>>>()?)
        };
        let single_group = additional.segmented_single_group.map_or_else(|| Ok("null".into()),
            |shader| shader.code_json().map_err(|error| at(error, "single_group")))?;
        let terminal = |name, choice: Option<TerminalScan<'_>>| match choice {
            None => Ok("null".into()),
            Some(TerminalScan::Canonical) => Ok(object(&[("canonical", "true".into())])),
            Some(TerminalScan::Alternate(shader)) => shader.code_json().map_err(|error| at(error, name)),
        };
        let [root, tail] = additional.terminal_scans;
        Ok(object(&[("kind", text("spirv_stage")),
            ("main", module_json(&self.main).map_err(|error| at(error, "main"))?),
            ("tail", optional(&self.tail, "tail")?), ("scan_add", optional(&self.scan_add, "scan_add")?),
            ("finish", finish), ("dispatch", optional(&self.dispatch, "dispatch")?),
            ("selector", optional(&self.selector, "selector")?), ("sort", sort),
            ("single_group", single_group), ("terminal_root", terminal("terminal_root", root)?),
            ("terminal_tail", terminal("terminal_tail", tail)?)]))
    }
}
impl<P> ProgramImage<P> {
    /// Reads every original stage/module; offsets and IDs are local to each module.
    pub fn code_json(&self) -> Result<String> {
        let stages = self.stages().iter().zip(self.program().stages()).enumerate().map(|(index, (stage, layout))| {
            stage.code_json(self.program(), layout).map_err(|error| at(error, &format!("stage[{index}]")))
        }).collect::<Result<Vec<_>>>()?;
        Ok(object(&[("kind", text("gpu_program_image")), ("program", text(self.program().name())),
            ("offset_basis", text("each_module_relative")), ("stages", array(stages))]))
    }
}
