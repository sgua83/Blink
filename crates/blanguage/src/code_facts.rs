use super::{Argument, CompileError, parsed_text};
use blanguage::object::{CodeSelection, code_facts, code_facts_with_observations, source_facts};
use std::{ffi::OsString, fs, ops::Range, path::Path};

#[derive(PartialEq, Eq)]
#[repr(u32)]
#[expect(dead_code, reason = "B produces the artifact kind")]
enum CodeInput {
    Entry,
    Image,
    Object,
}

#[repr(u32)]
#[expect(dead_code, reason = "B produces the query kind")]
enum CodeQuery {
    Code,
    Source,
}

#[repr(u32)]
#[expect(dead_code, reason = "B produces the object selection")]
enum CodeSelector {
    Sections,
    Section,
    Function,
}

#[repr(C)]
struct CodeCommandLine {
    input: CodeInput,
    query: CodeQuery,
    error: CompileError,
    selector: CodeSelector,
    source_index: usize,
    observations_index: usize,
    selected: usize,
    range_start: usize,
    range_end: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_code_arguments"]
    fn parse_code_arguments(arguments: *const Argument, count: usize) -> CodeCommandLine;
}

pub(super) fn print(args: &[OsString], arguments: &[Argument]) -> Result<(), Box<dyn std::error::Error>> {
    // B borrows the same argument storage used by the compiler command dispatch.
    let line = unsafe { parse_code_arguments(arguments.as_ptr(), arguments.len()) };
    line.error.result(args, line.source_index)?;
    let source = Path::new(&args[line.source_index]);
    let observations = args.get(line.observations_index).map(Path::new);
    let range = match line.query {
        CodeQuery::Code => None,
        CodeQuery::Source => Some(line.range_start..line.range_end),
    };
    let facts = if line.input == CodeInput::Object {
        let selection = match line.selector {
            CodeSelector::Sections => CodeSelection::CodeSections,
            CodeSelector::Section => CodeSelection::Section(line.selected),
            // B selects Function only for an argument marked as UTF-8 text.
            CodeSelector::Function => CodeSelection::Function(unsafe { parsed_text(&args[line.selected]) }),
        };
        object(source, observations, selection, range)?
    } else {
        let material = observations.map(fs::read).transpose()?;
        match (line.input == CodeInput::Image, range) {
            (true, Some(range)) => blanguage::build::image_source_facts(&fs::read(source)?, range, material.as_deref()),
            (false, Some(range)) => blanguage::build::source_facts(source, range, material.as_deref()),
            (true, None) => blanguage::build::image_code_facts(&fs::read(source)?, material.as_deref()),
            (false, None) => blanguage::build::code_facts(source, material.as_deref()),
        }.map_err(|error| error.to_string())?
    };
    println!("{facts}");
    Ok(())
}

fn object(
    source: &Path, observations: Option<&Path>, selection: CodeSelection<'_>, range: Option<Range<usize>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(source)?;
    let material = observations.map(fs::read).transpose()?;
    Ok(match (range, material.as_deref()) {
        (Some(range), material) => source_facts(&bytes, selection, range, material)?,
        (None, Some(material)) => code_facts_with_observations(&bytes, selection, material)?,
        (None, None) => code_facts(&bytes, selection)?,
    })
}
