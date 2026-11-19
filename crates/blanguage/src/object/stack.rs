//! Stack information follows the same sections and relocations as native code.
use super::{Kind, Object, Relocation, Symbol, Target};
mod analysis;
use analysis::{Budget, Label};
use std::fmt::Write;

pub(crate) struct Frame {
    pub name: String,
    pub coff: Option<(usize, u32)>,
    pub local: u32,
    pub calls: Vec<Call>,
}
pub(crate) enum Call {
    Reference { relocation: usize, tail: bool },
    Indirect,
    Unreported(String),
}

pub(crate) fn uses_runtime<Bytes>(object: &Object<Bytes>) -> bool {
    object.archive.as_ref().and_then(|archive| archive.resources.as_ref()).is_some()
        || matches!(object.kind, Kind::Library { .. })
        || object.sections.iter().flat_map(|section| &section.relocations).any(|relocation| {
            matches!(&relocation.target, Target::Import(import) if import.dll == crate::resource::RUNTIME_LIBRARY)
        })
}

pub(crate) struct Code<'a> {
    pub relocations: &'a dyn Fn(usize) -> &'a [Relocation],
    pub frames: &'a [Frame],
    pub symbols: &'a [Symbol],
}

fn write_amount(text: &mut String, bytes: u128, known: bool) {
    if known {
        write!(text, "{bytes}").unwrap();
    } else {
        text.push_str("unknown");
    }
    text.push('\t');
}

fn write_joined<'a>(text: &mut String, mut labels: impl Iterator<Item = &'a str>, separator: &str) {
    if let Some(first) = labels.next() {
        text.push_str(first);
        for label in labels {
            text.push_str(separator);
            text.push_str(label);
        }
    }
}

pub(crate) fn report(name: &str, code: Code<'_>, capacity: Option<u64>, reserve: Option<u64>, runtime: bool) -> String {
    let frames = code.frames;
    let analysis::Analysis { metrics, unknown, callees } = analysis::analyze(code, capacity);
    let count = frames.len();
    let mut text = format!("Blink CPU stack report: {name}\nTarget: Windows x64; unit: bytes\n");
    text.push_str("Each row starts immediately before calling the named function. Local includes saved registers and outgoing argument space, excluding its 8-byte return address. Known path includes return addresses and stops at unreported calls; it is not a bound on the rest of execution.\n");
    text.push_str("Upper is available only for a closed, acyclic reported call graph. A reachable cycle has no established finite bound; it does not assert infinite recursion for an actual input.\n");
    if let Some(bytes) = reserve {
        writeln!(text, "PE stack reserve: {bytes}; nominal image capacity, not the remaining capacity on an arbitrary caller/thread.").unwrap();
    }
    if let Some(bytes) = capacity {
        writeln!(text, "Supplied budget: {bytes}; before an independent call to each named function. The caller owns this remaining-capacity declaration.").unwrap();
    }
    if frames.is_empty() {
        text.push_str("No ordinary CPU function frames in this object. Compute/runtime/native library stack is not established.\n");
    }
    if runtime {
        text.push_str("Compute and resource execution belongs to its runtime/thread owner; its native/Rust stack is not established by this report.\n");
    }
    text.push_str("\nFunction\tLocal\tKnown path\tUpper\tCapacity comparison\tUnknown / callees\n");
    let mut order = (0..count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| frames[a].name.cmp(&frames[b].name).then(a.cmp(&b)));
    for index in order {
        let frame = &frames[index];
        let metric = &metrics[index];
        let bytes = metric.bytes();
        let comparison = match metric.capacity {
            Budget::NotSupplied => "not supplied",
            Budget::LocalExceeds => "local alone exceeds if entered",
            Budget::UpperFits => "closed upper fits supplied budget",
            Budget::UpperExceeds => "upper exceeds; actual path not established",
            Budget::Unknown => "unknown",
        };
        write!(text, "{}\t{}\t", frame.name, frame.local).unwrap();
        write_amount(&mut text, bytes, metric.closed != 0);
        write_amount(&mut text, bytes, metric.upper != 0);
        write!(text, "{comparison}\t").unwrap();

        let reasons = &unknown[index];
        write_joined(&mut text, reasons.iter().map(Label::as_str), "; ");
        if metric.closed == 0 {
            if !reasons.is_empty() { text.push_str("; "); }
            text.push_str("recursion reachable; finite bound not established");
        }
        let calls = &callees[index];
        if !calls.is_empty() {
            if !reasons.is_empty() || metric.closed == 0 { text.push_str("; "); }
            text.push_str("calls: ");
            write_joined(&mut text, calls.iter().map(Label::as_str), ", ");
        }
        text.push('\n');
    }
    text
}
