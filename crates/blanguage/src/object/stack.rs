//! Stack information follows the same sections and relocations as native code.
use super::{Kind, Object, Target};
mod analysis;
use analysis::Budget;

pub(crate) struct Frame {
    pub name: String,
    pub coff: Option<(usize, u32)>,
    pub local: u32,
    pub calls: Vec<Call>,
}
pub(crate) enum Call {
    Reference { offset: usize, tail: bool },
    Indirect,
    Unreported(String),
}

pub(crate) fn uses_runtime(object: &Object) -> bool {
    object.archive.as_ref().and_then(|archive| archive.resources.as_ref()).is_some()
        || matches!(object.kind, Kind::Library { .. })
        || object.sections.iter().flat_map(|section| &section.relocations).any(|relocation| {
            matches!(&relocation.target, Target::Import(import) if import.dll == crate::resource::RUNTIME_LIBRARY)
        })
}

pub(crate) fn report(object: &Object, capacity: Option<u64>, reserve: Option<u64>, runtime: bool) -> String {
    use std::fmt::Write;
    let analysis::Analysis { metrics, unknown, callees } = analysis::analyze(object, capacity);
    let count = object.stack.len();
    let mut text = format!("Blink CPU stack report: {}\nTarget: Windows x64; unit: bytes\n", object.name);
    text.push_str("Each row starts immediately before calling the named function. Local includes saved registers and outgoing argument space, excluding its 8-byte return address. Known path includes return addresses and stops at unreported calls; it is not a bound on the rest of execution.\n");
    text.push_str("Upper is available only for a closed, acyclic reported call graph. A reachable cycle has no established finite bound; it does not assert infinite recursion for an actual input.\n");
    if let Some(bytes) = reserve {
        writeln!(text, "PE stack reserve: {bytes}; nominal image capacity, not the remaining capacity on an arbitrary caller/thread.").unwrap();
    }
    if let Some(bytes) = capacity {
        writeln!(text, "Supplied budget: {bytes}; before an independent call to each named function. The caller owns this remaining-capacity declaration.").unwrap();
    }
    if object.stack.is_empty() {
        text.push_str("No ordinary CPU function frames in this object. Compute/runtime/native library stack is not established.\n");
    }
    if runtime {
        text.push_str("Compute and resource execution belongs to its runtime/thread owner; its native/Rust stack is not established by this report.\n");
    }
    text.push_str("\nFunction\tLocal\tKnown path\tUpper\tCapacity comparison\tUnknown / callees\n");
    let mut order = (0..count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| object.stack[a].name.cmp(&object.stack[b].name).then(a.cmp(&b)));
    for index in order {
        let frame = &object.stack[index];
        let metric = &metrics[index];
        let bytes = metric.bytes();
        let path = if metric.closed != 0 { bytes.to_string() } else { "unknown".into() };
        let upper = if metric.upper != 0 { Some(bytes) } else { None };
        let comparison = match metric.capacity {
            Budget::NotSupplied => "not supplied",
            Budget::LocalExceeds => "local alone exceeds if entered",
            Budget::UpperFits => "closed upper fits supplied budget",
            Budget::UpperExceeds => "upper exceeds; actual path not established",
            Budget::Unknown => "unknown",
        };
        let mut reasons = unknown[index].iter().cloned().collect::<Vec<_>>();
        if metric.closed == 0 { reasons.push("recursion reachable; finite bound not established".into()); }
        if !callees[index].is_empty() { reasons.push(format!("calls: {}", callees[index].iter().cloned().collect::<Vec<_>>().join(", "))); }
        writeln!(text, "{}\t{}\t{}\t{}\t{}\t{}", frame.name, frame.local, path,
            upper.map_or_else(|| "unknown".into(), |bytes| bytes.to_string()), comparison, reasons.join("; ")).unwrap();
    }
    text
}
