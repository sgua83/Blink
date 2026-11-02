//! Stack information follows the same sections and relocations as native code.
use super::{Kind, Object, Result, Target};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) struct Frame {
    pub name: String,
    pub coff: Option<(usize, u32)>,
    pub local: u32,
    pub calls: Vec<Call>,
}
pub(crate) enum Call {
    Reference { offset: usize, tail: bool },
    Indirect,
    Unreported,
}

pub(crate) fn compute(name: &str, stages: &[crate::native::StageCode]) -> Vec<Frame> {
    use crate::native::{ContinuationCode, StageCode};
    let mut frames = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        let mut record = |entry: &str, code: &crate::native::NativeCode| frames.push(Frame {
            name: format!("CPU {name} stage {index}{entry}"), coff: None, local: code.stack_bytes,
            calls: vec![Call::Unreported],
        });
        match stage {
            StageCode::Element { main, continuation, .. } => {
                record("", main);
                match continuation {
                    Some(ContinuationCode::ReductionTail(code)) => record(" reduction tail", code),
                    Some(ContinuationCode::ScanAdd(code)) => record(" scan add", code),
                    Some(ContinuationCode::CompactScatter(code)) => record(" compact scatter", code),
                    Some(ContinuationCode::SegmentedFinish(code)) => record(" segmented finish", code),
                    Some(ContinuationCode::Scan { prefix, apply, .. }) => {
                        record(" scan prefix", prefix);
                        record(" scan apply", apply);
                    }
                    None => {}
                }
            }
            StageCode::Sort(sort) => {
                for (entry, code) in sort.initialize.iter().enumerate() {
                    record(&format!(" sort initialize {entry}"), code);
                }
                for (entry, code) in [(" sort histogram", &sort.histogram),
                    (" sort scatter", &sort.scatter), (" sort gather", &sort.gather)] {
                    record(entry, code);
                }
            }
        }
    }
    frames
}

// Validate only the new references crossing the compiled-object boundary.
pub(super) fn validate(object: &Object) -> Result<()> {
    let mut positions = BTreeSet::new();
    for frame in &object.stack {
        if let Some((index, start)) = frame.coff {
            let section = object.sections.get(index).ok_or("栈信息引用不存在的节")?;
            if start as usize >= section.bytes.len() || !positions.insert((index, start)) {
                return Err("栈信息的函数位置无效或重复".into());
            }
        }
        for call in &frame.calls {
            if let Call::Reference { offset, .. } = call {
                let (index, _) = frame.coff.ok_or("没有COFF位置的入口不能引用COFF调用")?;
                let section = &object.sections[index];
                if !section.relocations.iter().any(|relocation| relocation.offset == *offset) {
                    return Err("栈调用信息缺少对应的原生重定位".into());
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn report(object: &Object, capacity: Option<u64>, reserve: Option<u64>) -> String {
    use std::fmt::Write;
    let positions: BTreeMap<_, _> = object.stack.iter().enumerate()
        .filter_map(|(index, frame)| frame.coff.map(|position| (position, index))).collect();
    let symbols: BTreeMap<_, _> = object.symbols.iter()
        .map(|symbol| (symbol.name.as_str(), (symbol.section, symbol.value))).collect();
    let count = object.stack.len();
    let mut edges = vec![Vec::new(); count];
    let mut parents = vec![Vec::new(); count];
    let mut unknown = vec![BTreeSet::<String>::new(); count];
    let mut callees = vec![BTreeSet::<String>::new(); count];
    let mut prefix = object.stack.iter().map(|frame| u128::from(frame.local) + 8).collect::<Vec<_>>();
    for (index, frame) in object.stack.iter().enumerate() {
        for call in &frame.calls {
            let (target, tail) = match call {
                Call::Unreported => {
                    unknown[index].insert("compute call graph not reported".into());
                    continue;
                }
                Call::Indirect => {
                    unknown[index].insert("indirect target".into());
                    prefix[index] = prefix[index].max(u128::from(frame.local) + 16);
                    continue;
                }
                Call::Reference { offset, tail } => {
                    (&object.sections[frame.coff.unwrap().0].relocations.iter()
                        .find(|relocation| relocation.offset == *offset).unwrap().target, *tail)
                }
            };
            let position = match target {
                Target::Section(section, start) => Some((*section, *start)),
                Target::Symbol(name) => {
                    if let Some(position) = symbols.get(name.as_str()) { Some(*position) } else {
                        unknown[index].insert(format!("unresolved module {name}"));
                        callees[index].insert(name.clone());
                        None
                    }
                }
                Target::Import(import) => {
                    let name = format!("{}!{}", import.dll, import.name);
                    unknown[index].insert(format!("external {name}"));
                    callees[index].insert(name);
                    None
                }
            };
            if let Some(position) = position {
                if let Some(&child) = positions.get(&position) {
                    edges[index].push((child, tail));
                    parents[child].push((index, tail));
                    callees[index].insert(object.stack[child].name.clone());
                } else {
                    unknown[index].insert(format!("unreported native body section {}+{}", position.0, position.1));
                }
            }
            if position.is_none() || position.is_some_and(|position| !positions.contains_key(&position)) {
                prefix[index] = prefix[index].max(u128::from(frame.local) + if tail { 8 } else { 16 });
            }
        }
    }
    // Leaves close once. Nodes left over have a cycle in their reachable graph;
    // a cycle says nothing about a particular input's recursion depth.
    let mut pending = edges.iter().map(Vec::len).collect::<Vec<_>>();
    let mut ready = pending.iter().enumerate().filter_map(|(index, &n)| (n == 0).then_some(index)).collect::<VecDeque<_>>();
    let mut closed = vec![false; count];
    while let Some(child) = ready.pop_front() {
        closed[child] = true;
        for &(parent, tail) in &parents[child] {
            let bytes = u128::from(object.stack[parent].local) + 8 + prefix[child] - if tail { 8 } else { 0 };
            prefix[parent] = prefix[parent].max(bytes);
            let reasons = unknown[child].iter().cloned().collect::<Vec<_>>();
            unknown[parent].extend(reasons);
            pending[parent] -= 1;
            if pending[parent] == 0 { ready.push_back(parent); }
        }
    }
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
    if object.archive.as_ref().and_then(|archive| archive.resources.as_ref()).is_some() || matches!(object.kind, Kind::Library { .. }) {
        text.push_str("Compute and resource execution belongs to its runtime/thread owner; its native/Rust stack is not established by this report.\n");
    }
    text.push_str("\nFunction\tLocal\tKnown path\tUpper\tCapacity comparison\tUnknown / callees\n");
    let mut order = (0..count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| object.stack[a].name.cmp(&object.stack[b].name).then(a.cmp(&b)));
    for index in order {
        let frame = &object.stack[index];
        let path = if closed[index] { prefix[index].to_string() } else { "unknown".into() };
        let upper = if closed[index] && unknown[index].is_empty() { Some(prefix[index]) } else { None };
        let comparison = match capacity {
            None => "not supplied",
            Some(bytes) if u128::from(frame.local) + 8 > u128::from(bytes) => "local alone exceeds if entered",
            Some(bytes) if upper.is_some_and(|upper| upper <= u128::from(bytes)) => "closed upper fits supplied budget",
            Some(_) if upper.is_some() => "upper exceeds; actual path not established",
            Some(_) => "unknown",
        };
        let mut reasons = unknown[index].iter().cloned().collect::<Vec<_>>();
        if !closed[index] { reasons.push("recursion reachable; finite bound not established".into()); }
        if !callees[index].is_empty() { reasons.push(format!("calls: {}", callees[index].iter().cloned().collect::<Vec<_>>().join(", "))); }
        writeln!(text, "{}\t{}\t{}\t{}\t{}\t{}", frame.name, frame.local, path,
            upper.map_or_else(|| "unknown".into(), |bytes| bytes.to_string()), comparison, reasons.join("; ")).unwrap();
    }
    text
}
