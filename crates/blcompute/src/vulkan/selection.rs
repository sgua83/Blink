//! where 的私有索引队列与间接参数沿原命令／完成点持有。
use super::*;
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Scratch> {
    let mut out = compact::allocate(c, count)?;
    out.buffers.push(PagedBuffer::words(c.inner.clone(), count, 1)?);
    Ok(out)
}
pub(super) struct Descriptors {
    root: scan::Descriptors,
    finish: u64,
    layers: BTreeMap<usize, scan::Descriptors>,
}

pub(super) fn descriptors(
    writer: &mut DescriptorWriter<'_>,
    scratch: &Scratch,
    count: u32,
    sources: BTreeSet<usize>,
) -> Result<Descriptors> {
    let selection = writer.compiled.program.selection_buffer();
    let root = scratch.buffers[scratch.buffers.len() - 2].descriptor();
    let indices = scratch.buffers.last().unwrap().descriptor();
    let args = Some(scratch.arguments.as_ref().unwrap().descriptor());

    // The first scan and prefix preparation both read the original bindings.
    let first = writer.set(&[], Some(scratch.buffers[0].descriptor()), args, Some(root), None)?;
    let finish = writer.set(&[], Some(indices), args, Some(root), None)?;
    let layers = sources.into_iter().map(|source| {
        scan::descriptors(
            writer, (source, selection), false, count, scan::ScanRoot::Set(first),
            root, &scratch.buffers, args,
        ).map(|sets| (source, sets))
    }).collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Descriptors { root: scan::Descriptors::with_root(first), finish, layers })
}

pub(super) fn record(
    commands: &Commands,
    sets: &Descriptors,
    c: &Compiled,
    stage: usize,
    count: u32,
    args: &PagedBuffer,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) {
    let scan = if sets.layers.is_empty() {
        &sets.root
    } else {
        let source = c.program.stages()[stage].selection.unwrap().binding;
        &sets.layers[&source]
    };
    prefix::prepare(commands, c, stage, sets.root.root(), uniforms, partition, reused);
    scan::record_dispatches(
        commands,
        c,
        [
            c.selectors[stage].unwrap(),
            c.tails[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
        ],
        count,
        scan,
        Some(args),
        uniforms,
        partition,
        0,
    );
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    prefix::dispatch(
        commands,
        c,
        c.finishes[stage].unwrap(),
        sets.finish,
        args,
        uniforms,
        0,
    );
    commands.barrier(0x800, 0x802, 0x40, 0x21);
    prefix::dispatch(
        commands,
        c,
        c.pipelines[stage],
        sets.finish,
        args,
        uniforms,
        blanguage::spirv::PREFIX_LEVELS,
    );
}
