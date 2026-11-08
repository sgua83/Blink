//! Borrowed physical COFF facts, using the existing B structural reader.
use super::*;
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub enum CodeSelection<'a> {
    CodeSections,
    /// The original COFF section number, starting at one.
    Section(usize),
    /// An exact, defined function symbol; aliases do not determine a new function end.
    Function(&'a str),
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadSection {
    name: ReadBytes, bytes: ReadBytes, flags: u64,
    relocations: usize, relocation_count: usize, error: ReadError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.coff_read::section"]
    fn read_section(input: ReadBytes, headers: ReadBytes, index: usize) -> ReadSection;
}
struct SectionFacts {
    row: ReadSection,
    relocations: Vec<ReadRelocation>,
}
struct Facts<'a> {
    reader: Reader<'a>,
    table: ReadSymbols,
    symbols: Vec<Option<ReadSymbol>>,
    sections: Vec<SectionFacts>,
}
struct Span {
    section: usize, start: usize, end: usize,
    pdata_section: usize, record_offset: usize,
}
fn text(value: &str) -> String {
    let mut out = String::new(); crate::json::string(&mut out, value); out
}
fn hex(value: &[u8]) -> String {
    let mut out = String::from("\"");
    for byte in value { write!(&mut out, "{byte:02x}").unwrap(); }
    out.push('"'); out
}
fn array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}
fn object(values: &[(&str, String)]) -> String {
    format!("{{{}}}", values.iter().map(|(key, value)| format!("{}:{value}", text(key))).collect::<Vec<_>>().join(","))
}
fn position(section: usize, offset: usize) -> String {
    object(&[("section_number", (section + 1).to_string()), ("offset_bytes", offset.to_string())])
}
impl<'a> Facts<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self> {
        let reader = Reader::new(bytes)?;
        let input = ReadBytes::new(bytes);
        let table = unsafe { read_symbols(input) }; table.error.check()?;
        let mut symbols = vec![None; table.count];
        let mut index = 0;
        while index < table.count {
            let row = unsafe { read_symbol(input, table, index) }; row.error.check()?;
            if row.next > table.count { return Err("COFF auxiliary records exceed the symbol table".into()); }
            symbols[index] = Some(row); index = row.next;
        }
        let mut sections = Vec::new();
        for index in 0..reader.headers.len() / 40 {
            let row = unsafe { read_section(input, ReadBytes::new(reader.headers), index) }; row.error.check()?;
            let table = unsafe { read_relocations(input, row.relocations, row.relocation_count, row.flags) }; table.error.check()?;
            let mut relocations = Vec::new();
            for i in 0..table.count {
                let relocation = unsafe { read_relocation(input, table.offset, i) }; relocation.error.check()?;
                if symbols.get(relocation.symbol).and_then(Option::as_ref).is_none() {
                    return Err("COFF relocation does not refer to a primary symbol record".into());
                }
                relocations.push(relocation);
            }
            sections.push(SectionFacts { row, relocations });
        }
        Ok(Self { reader, table, symbols, sections })
    }
    fn symbol(&self, index: usize, row: &ReadSymbol) -> Result<String> {
        let begin = self.table.offset + index * 18;
        let end = self.table.offset + row.next * 18;
        let raw = self.reader.bytes.get(begin..end).ok_or("COFF symbol records are truncated")?;
        Ok(object(&[("symbol_index", index.to_string()), ("file_offset_bytes", begin.to_string()), ("name", text(&read_text(row.name)?)),
            ("name_bytes", hex(unsafe { row.name.borrowed() })), ("section_number", row.section.to_string()),
            ("value", row.value.to_string()), ("storage_class", row.storage.to_string()),
            ("function", (row.function != 0).to_string()), ("record_bytes", hex(&raw[..18])),
            ("auxiliary_count", row.auxiliary.to_string()), ("auxiliary_bytes", hex(&raw[18..]))]))
    }
    fn relocation(&self, row: &ReadRelocation) -> Result<String> {
        let symbol = self.symbols[row.symbol].as_ref().unwrap();
        Ok(object(&[("offset_bytes", row.offset.to_string()), ("type", row.kind.to_string()),
            ("symbol_index", row.symbol.to_string()), ("symbol_name", text(&read_text(symbol.name)?)),
            ("symbol_section_number", symbol.section.to_string()), ("symbol_value", symbol.value.to_string())]))
    }
    fn address(&self, section: &SectionFacts, at: usize) -> Result<(Option<(usize, usize)>, String)> {
        let bytes = unsafe { section.row.bytes.borrowed() };
        let addend = u32::from_le_bytes(bytes[at..at + 4].try_into()?);
        let mut relocations = section.relocations.iter().filter(|row| row.offset == at);
        let relocation = relocations.next();
        if relocations.next().is_some() { return Err("COFF .pdata field has multiple relocations".into()); }
        let (resolved, status) = match relocation {
            None => (None, "no_relocation"),
            Some(row) if row.kind != 3 => (None, "relocation_type_not_supported"),
            Some(row) => {
                let symbol = self.symbols[row.symbol].as_ref().unwrap();
                if symbol.section <= 0 { (None, "symbol_not_section_relative") }
                else {
                    let index = usize::try_from(symbol.section - 1)?;
                    let target = self.sections.get(index).ok_or("COFF .pdata target section is outside the section table")?;
                    let offset = usize::try_from(symbol.value + u64::from(addend))?;
                    if offset > target.row.bytes.length { return Err("COFF .pdata address is outside its target section".into()); }
                    (Some((index, offset)), "resolved")
                }
            }
        };
        Ok((resolved, object(&[("addend", addend.to_string()), ("status", text(status)),
            ("relocation", relocation.map(|row| self.relocation(row)).transpose()?.unwrap_or_else(|| "null".into())),
            ("position", resolved.map(|(s, o)| position(s, o)).unwrap_or_else(|| "null".into()))])))
    }
    fn ranges(&self) -> Result<(Vec<Span>, String)> {
        let mut spans = Vec::new(); let mut records = Vec::new();
        for (index, section) in self.sections.iter().enumerate() {
            if unsafe { section.row.name.borrowed() } != b".pdata" { continue; }
            let bytes = unsafe { section.row.bytes.borrowed() };
            if bytes.len() % 12 != 0 { return Err("COFF x64 .pdata contains an incomplete function record".into()); }
            for at in (0..bytes.len()).step_by(12) {
                let (begin, begin_json) = self.address(section, at)?;
                let (end, end_json) = self.address(section, at + 4)?;
                let (_, unwind_json) = self.address(section, at + 8)?;
                let status = match (begin, end) {
                    (Some((s, _)), Some((t, _))) if s != t => "cross_section_range",
                    (Some((s, start)), Some((_, end))) => {
                        if start > end { return Err("COFF .pdata function range is reversed".into()); }
                        spans.push(Span { section: s, start, end, pdata_section: index, record_offset: at }); "resolved"
                    }
                    _ => "unresolved",
                };
                records.push(object(&[("section_number", (index + 1).to_string()), ("offset_bytes", at.to_string()),
                    ("record_bytes", hex(&bytes[at..at + 12])), ("status", text(status)),
                    ("begin", begin_json), ("end", end_json), ("unwind", unwind_json)]));
            }
        }
        Ok((spans, array(records)))
    }
    fn selected(&self, selection: CodeSelection<'_>) -> Result<(Vec<usize>, String)> {
        Ok(match selection {
            CodeSelection::CodeSections => (self.sections.iter().enumerate().filter_map(|(i, s)| (s.row.flags & 0x20 != 0).then_some(i)).collect(),
                object(&[("kind", text("code_sections"))])),
            CodeSelection::Section(number) => {
                let index = number.checked_sub(1).filter(|&i| i < self.sections.len()).ok_or("COFF section number is outside the section table")?;
                (vec![index], object(&[("kind", text("section")), ("section_number", number.to_string())]))
            }
            CodeSelection::Function(name) => {
                let mut found = self.symbols.iter().enumerate().filter_map(|(i, row)| row.as_ref().map(|r| (i, r)))
                    .filter(|(_, row)| row.function != 0 && unsafe { row.name.borrowed() } == name.as_bytes());
                let (index, row) = found.next().ok_or("COFF function symbol was not found")?;
                if found.next().is_some() { return Err("COFF function symbol is ambiguous; select its physical section".into()); }
                let section = usize::try_from(row.section).ok().and_then(|s| s.checked_sub(1)).filter(|&s| s < self.sections.len())
                    .ok_or("COFF function symbol has no defined section")?;
                (vec![section], object(&[("kind", text("function")), ("name", text(name)),
                    ("symbol_index", index.to_string()), ("section_number", (section + 1).to_string()), ("value", row.value.to_string())]))
            }
        })
    }
    fn section_json(&self, index: usize, selected: bool, spans: &[Span], material: Option<&observations::Material>) -> Result<String> {
        let section = &self.sections[index]; let row = section.row;
        let mut values = vec![("section_number", (index + 1).to_string()), ("name", text(&read_text(row.name)?)),
            ("header_file_offset_bytes", (20 + index * 40).to_string()),
            ("data_file_offset_bytes", (row.bytes.bytes as usize - self.reader.bytes.as_ptr() as usize).to_string()),
            ("relocation_table_file_offset_bytes", row.relocations.to_string()),
            ("header_bytes", hex(&self.reader.headers[index * 40..(index + 1) * 40])),
            ("characteristics", row.flags.to_string()), ("byte_length", row.bytes.length.to_string()),
            ("relocation_count", section.relocations.len().to_string()), ("selected", selected.to_string())];
        if selected {
            let bytes = unsafe { row.bytes.borrowed() };
            let layout = material.and_then(|material| material.layout(index));
            let regions = if let Some(layout) = layout {
                layout.regions(index, bytes.len(), spans)?.into_iter().map(|(kind, start, end)| {
                    let instructions = if matches!(kind, "literal" | "alignment") { "null".into() }
                    else {
                        crate::native::instructions_json(&bytes[start..end], start as u64).map_err(|error| {
                            format!("COFF section {}: {}", index + 1, crate::native::DecodeError {
                                offset: start + error.offset, ip: error.ip, reason: error.reason,
                            })
                        })?
                    };
                    Ok(object(&[("kind", text(kind)), ("offset_bytes", start.to_string()),
                        ("length_bytes", (end - start).to_string()), ("bytes", hex(&bytes[start..end])), ("instructions", instructions)]))
                }).collect::<Result<Vec<_>>>()?
            } else {
            let mut regions = Vec::new(); let mut code = spans.iter().filter(|s| s.section == index).collect::<Vec<_>>();
            code.sort_by_key(|s| (s.start, s.end));
            let mut end = 0;
            for span in code {
                if span.start > end { regions.push(raw_region(bytes, end, span.start)); }
                let raw = &bytes[span.start..span.end];
                let instructions = crate::native::instructions_json(raw, span.start as u64).map_err(|error| {
                    format!("COFF section {}: {}", index + 1, crate::native::DecodeError {
                        offset: span.start + error.offset, ip: error.ip, reason: error.reason,
                    })
                })?;
                regions.push(object(&[("kind", text("runtime_function")), ("offset_bytes", span.start.to_string()),
                    ("length_bytes", raw.len().to_string()), ("bytes", hex(raw)), ("instructions", instructions),
                    ("pdata_section_number", (span.pdata_section + 1).to_string()), ("pdata_record_offset_bytes", span.record_offset.to_string())]));
                end = end.max(span.end);
            }
            if end < bytes.len() { regions.push(raw_region(bytes, end, bytes.len())); }
            regions
            };
            values.extend([("bytes", hex(bytes)), ("regions", array(regions)),
                ("relocations", array(section.relocations.iter().map(|r| self.relocation(r)).collect::<Result<Vec<_>>>()?)),
                ("generation", layout.map_or_else(|| "null".into(), |layout| layout.generation())),
                ("function_abi", layout.map_or_else(|| "null".into(), |layout| layout.function_abi())),
                ("tail_references", layout.and_then(|layout| layout.references()).map_or_else(|| "null".into(), |references|
                    array(references.iter().map(|reference| object(&[
                        ("target_offset_bytes", reference.target.to_string()),
                        ("rel32_field_offsets_bytes", array(reference.rel32_fields.iter().map(usize::to_string))),
                    ])))))]);
        }
        Ok(object(&values))
    }
}
fn raw_region(bytes: &[u8], start: usize, end: usize) -> String {
    object(&[("kind", text("unclassified")), ("offset_bytes", start.to_string()),
        ("length_bytes", (end - start).to_string()), ("bytes", hex(&bytes[start..end])), ("instructions", "null".into())])
}
/// Inspects original physical COFF records without source, compilation, loading, or execution.
/// Only resolved x64 `.pdata` ranges are decoded; other bytes remain explicitly unclassified.
/// A selected symbol identifies its containing section, not an inferred function end.
pub fn code_facts(bytes: &[u8], selection: CodeSelection<'_>) -> Result<String> {
    query(bytes, selection, None)
}
/// Reads explicitly supplied producer material. A mismatch is an error, not a partial query.
pub fn code_facts_with_observations(bytes: &[u8], selection: CodeSelection<'_>, material: &[u8]) -> Result<String> {
    query(bytes, selection, Some(material))
}
fn query(bytes: &[u8], selection: CodeSelection<'_>, material: Option<&[u8]>) -> Result<String> {
    let facts = Facts::read(bytes)?;
    let (selected, selection_json) = facts.selected(selection)?;
    let (spans, tables) = facts.ranges()?;
    let material = material.map(|bytes| observations::Material::read(&facts, &spans, bytes)).transpose()?;
    let mut values = vec![("kind", text("coff_code")), ("bitness", "64".into()), ("address_basis", text("section_relative")),
        ("header_bytes", hex(&bytes[..20])), ("symbol_table_file_offset_bytes", facts.table.offset.to_string()),
        ("string_table_file_offset_bytes", (facts.table.strings.bytes as usize - bytes.as_ptr() as usize).to_string()),
        ("string_table_bytes", hex(unsafe { facts.table.strings.borrowed() })),
        ("selection", selection_json), ("symbol_record_count", facts.table.count.to_string()),
        ("symbols", array(facts.symbols.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| facts.symbol(i, s))).collect::<Result<Vec<_>>>()?)),
        ("function_tables", tables),
        ("sections", array(facts.sections.iter().enumerate().map(|(i, _)| facts.section_json(i, selected.contains(&i), &spans, material.as_ref())).collect::<Result<Vec<_>>>()?))];
    if material.is_some() { values.push(("code_observations", text("matched"))); }
    Ok(object(&values))
}

pub(crate) mod observations;
