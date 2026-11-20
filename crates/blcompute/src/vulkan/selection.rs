//! where 的私有索引队列与间接参数沿原命令／完成点持有。
use super::*;
pub(super) fn allocate(c: &Compiled, stage: usize, count: u32) -> Result<Scratch> {
    let mut out = compact::allocate(c, stage, count)?;
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
    let (summaries, records) = scratch.buffers.split_at(scratch.buffers.len() - 2);
    let root = records[0].descriptor();
    let indices = records[1].descriptor();
    let args = Some(scratch.arguments.as_ref().unwrap().descriptor());

    // The first scan and prefix preparation both read the original bindings.
    let first = writer.set(&[], summaries.first().map(PagedBuffer::descriptor), args, Some(root), None)?;
    let finish = writer.set(&[], Some(indices), args, Some(root), None)?;
    let layers = sources.into_iter().map(|source| {
        scan::descriptors(
            writer, (source, selection), false, count, scan::ScanRoot::Set(first),
            root, summaries, args,
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
    commands.bind_compute_set(c.layout, scan.root());
    prefix::prepare(commands, c, stage, uniforms, partition, reused);
    scan::record_dispatches(
        commands,
        c,
        stage,
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
    let continuation = prefix::continuation_slot(c, stage);
    if continuation < scan::levels(count) {
        // Scatter replaces a scan record after its last command and shader reads.
        commands.barrier(0x802, 0x800, 0x61, 0x60);
    } else {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
    }
    commands.bind_compute_set(c.layout, sets.finish);
    prefix::dispatch(
        commands,
        c,
        c.finishes[stage].unwrap(),
        args,
        uniforms,
        0,
    );
    commands.barrier(0x800, 0x802, 0x40, 0x21);
    prefix::dispatch(
        commands,
        c,
        c.pipelines[stage],
        args,
        uniforms,
        continuation,
    );
}
