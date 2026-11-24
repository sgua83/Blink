//! 显式传输闭合资源图；一次操作内保留共享，不解析源平台地址或维护跨操作缓存。
use super::*;
use crate::cpu::CpuDataset;
use std::{collections::BTreeMap, ops::Range};

impl Device {
    /// Accept the CPU source before obtaining a device for the upload.
    pub(crate) fn upload_cpu<'a>(
        data: &CpuDataset,
        device: impl FnOnce() -> Result<&'a Self>,
    ) -> Result<GpuDataset> {
        Upload::accept(data)?;
        let mut upload = Upload::new(device()?);
        let root = match data {
            CpuDataset::Owned(data) => UploadRoot::Owned(upload.record_borrowed(data)?),
            CpuDataset::Read(source) => UploadRoot::Read(upload.record_read(source)?),
        };
        Ok(upload.submit()?.root(root))
    }

    /// Stage original host fields into the same physical pages used by dataset uploads.
    pub(crate) fn upload_fields<'a>(
        &self,
        bindings: impl IntoIterator<Item = (&'a blanguage::ir::Buffer, u32)>,
        mut fill: impl FnMut(usize, usize, &mut [u8]),
    ) -> Result<DomainBindings<GpuDataset>> {
        let mut upload = Upload::new(self);
        let mut roots = Vec::new();
        for (index, (binding, count)) in bindings.into_iter().enumerate() {
            let target = GpuDataset::new(self.inner.clone(), binding.ty.clone(), binding.stride, count)?;
            self.stage_pages(&target, &mut upload.commands, &mut upload.staging, |source, range| {
                let pointer = source.mapping()?;
                // The newly mapped staging page is exclusive until the transfer is submitted.
                // Clear untouched fields and the last partial copy word before exposing bytes.
                let words = unsafe {
                    pointer.write_bytes(0, source.size as usize);
                    std::slice::from_raw_parts_mut(pointer.cast::<u32>(), source.size as usize / 4)
                };
                let bytes = unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), range.len()) };
                fill(index, range.start, bytes);
                crate::views::require_unbound_views(&binding.ty, binding.stride, words)
            })?;
            roots.push(UploadRoot::Owned(StagedDataset { data: target, views: Views::default() }));
        }
        let uploaded = upload.submit()?;
        Ok(roots.into_iter().map(|root| uploaded.root(root)).collect::<Vec<_>>().into())
    }

    pub(super) fn upload_payload(&self, target: &mut GpuDataset, words: &[u32]) -> Result<()> {
        let mut commands = None;
        let mut staging = Vec::new();
        self.stage_words(
            target,
            words,
            |start, mapped| target.views.encode(start, mapped, |source| source.0.view_header()),
            &mut commands,
            &mut staging,
        )?;
        if let Some(commands) = commands {
            target.last_use = Some(DatasetUse::Binding(self.submit_dataset_transfer(commands, staging, false)?));
        }
        Ok(())
    }

    fn stage_words(
        &self,
        target: &GpuDataset,
        words: &[u32],
        encode_views: impl Fn(usize, &mut [u32]),
        commands: &mut Option<Commands>,
        staging: &mut Vec<Allocation>,
    ) -> Result<()> {
        if words.is_empty() {
            return Ok(());
        }
        let bytes = unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), target.bytes() as usize) };
        self.stage_pages(target, commands, staging, |source, range| {
            let start = range.start;
            source.write_with(&bytes[range], |mapped| encode_views(start / 4, mapped))
        })
    }

    fn stage_pages(
        &self,
        target: &GpuDataset,
        commands: &mut Option<Commands>,
        staging: &mut Vec<Allocation>,
        mut write: impl FnMut(&Allocation, Range<usize>) -> Result<()>,
    ) -> Result<()> {
        if target.pages.is_empty() {
            return Ok(());
        }
        let commands = match commands {
            Some(commands) => commands,
            slot @ None => slot.insert(Commands::new(self.inner.clone())?),
        };
        let bytes_per_page = target.page_records() as usize * target.stride as usize;
        for (index, page) in target.pages.iter().enumerate() {
            let start = index * bytes_per_page;
            let range = start..(start + bytes_per_page).min(target.bytes() as usize);
            self.stage_page(page, range.len() as u64, |source| write(source, range), commands, staging)?;
        }
        Ok(())
    }

    // The writer initializes the mapped source, including padding to the four-byte copy size.
    pub(super) fn stage_page(
        &self,
        target: &Allocation,
        bytes: u64,
        write: impl FnOnce(&Allocation) -> Result<()>,
        commands: &mut Commands,
        staging: &mut Vec<Allocation>,
    ) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }
        let source = Allocation::new(self.inner.clone(), bytes, 1, 6, 4)?;
        write(&source)?;
        let copy = VkBufferCopy {
            srcOffset: 0,
            dstOffset: 0,
            size: bytes.div_ceil(4) * 4,
        };
        unsafe {
            (self.inner.functions.vkCmdCopyBuffer)(
                commands.command,
                source.buffer,
                target.buffer,
                1,
                &copy,
            );
        }
        staging.push(source);
        Ok(())
    }
    pub(super) fn submit_dataset_transfer(
        &self,
        commands: Commands,
        staging: Vec<Allocation>,
        covers_view_sources: bool,
    ) -> Result<Rc<Execution>> {
        Execution::submit(
            ExecutionResources::DatasetTransfer {
                commands,
                _staging: staging,
                covers_view_sources,
            },
            Dependencies::Empty,
        )
    }
}

/// Record a CPU resource graph before publishing any of its GPU bindings.
pub(super) struct Upload<'a> {
    device: &'a Device,
    commands: Option<Commands>,
    staging: Vec<Allocation>,
    identities: BTreeMap<usize, usize>,
    shared: Vec<StagedDataset>,
}
pub(super) enum UploadRoot {
    Owned(StagedDataset),
    Read(usize),
}
pub(super) struct StagedDataset {
    data: GpuDataset,
    views: Views<usize>,
}
pub(super) struct Uploaded {
    shared: Vec<ReadDataset>,
    execution: Option<Rc<Execution>>,
}

impl<'a> Upload<'a> {
    pub(super) fn new(device: &'a Device) -> Self {
        Self {
            device,
            commands: None,
            staging: Vec::new(),
            identities: BTreeMap::new(),
            shared: Vec::new(),
        }
    }

    pub(super) fn accept(data: &CpuDataset) -> Result<()> {
        if let CpuDataset::Owned(data) = data {
            data.require_views()?;
        }
        Ok(())
    }

    // The upload entry point has accepted this CPU binding.
    pub(super) fn record_root(&mut self, data: CpuDataset) -> Result<UploadRoot> {
        match data {
            CpuDataset::Owned(data) => self.record(
                data.stride, data.count, &data.words, &data.views.sources,
                || (data.ty, data.views.bindings),
            ).map(UploadRoot::Owned),
            CpuDataset::Read(source) => self.record_read(&source).map(UploadRoot::Read),
        }
    }

    fn record_read(&mut self, source: &crate::ReadDataset) -> Result<usize> {
        let identity = source.identity();
        if let Some(&index) = self.identities.get(&identity) {
            return Ok(index);
        }
        let target = self.record_borrowed(source)?;
        let index = self.shared.len();
        self.shared.push(target);
        self.identities.insert(identity, index);
        Ok(index)
    }

    fn record_borrowed(&mut self, source: &Dataset) -> Result<StagedDataset> {
        self.record(
            source.stride, source.count, &source.words, &source.views.sources,
            || (source.ty.clone(), source.views.bindings.clone()),
        )
    }

    fn record(
        &mut self,
        stride: u32,
        count: u32,
        words: &[u32],
        view_sources: &[crate::ReadDataset],
        metadata: impl FnOnce() -> (Ty, Vec<(usize, usize)>),
    ) -> Result<StagedDataset> {
        let sources = view_sources.iter()
            .map(|source| self.record_read(source))
            .collect::<Result<Vec<_>>>()?;
        let (ty, bindings) = metadata();
        let views = Views { sources, bindings };
        let data = GpuDataset::new(self.device.inner.clone(), ty, stride, count)?;
        self.device.stage_words(
            &data,
            words,
            |start, mapped| views.encode(start, mapped, |&index| self.shared[index].data.view_header()),
            &mut self.commands,
            &mut self.staging,
        )?;
        Ok(StagedDataset { data, views })
    }

    pub(super) fn submit(self) -> Result<Uploaded> {
        let execution = self.commands.map(|commands| {
            self.device.submit_dataset_transfer(commands, self.staging, true)
        }).transpose()?;
        let mut shared = Vec::with_capacity(self.shared.len());
        // A source is recorded before any record that refers to it.
        for staged in self.shared {
            shared.push(staged.publish(&shared, execution.as_ref()).freeze());
        }
        Ok(Uploaded { shared, execution })
    }
}
impl StagedDataset {
    fn publish(mut self, shared: &[ReadDataset], execution: Option<&Rc<Execution>>) -> GpuDataset {
        self.data.owned_storage().expect("staged upload owns its destination").views = Views {
            sources: self.views.sources.into_iter().map(|index| shared[index].clone()).collect(),
            bindings: self.views.bindings,
        };
        if !self.data.pages.is_empty() {
            self.data.last_use = execution.cloned().map(DatasetUse::Binding);
        }
        self.data
    }
}
impl Uploaded {
    pub(super) fn root(&self, root: UploadRoot) -> GpuDataset {
        match root {
            UploadRoot::Owned(staged) => staged.publish(&self.shared, self.execution.as_ref()),
            UploadRoot::Read(index) => self.shared[index].binding(),
        }
    }
}

/// One synchronous download owns the source memo and reuses its transfer until the last root.
pub(super) struct Readback<'a> {
    device: &'a Device,
    sources: BTreeMap<usize, crate::ReadDataset>,
    completion: Option<Completion>,
    commands: Option<Commands>,
    staging: Option<Allocation>,
}
struct ReadbackPayload {
    stride: u32,
    count: u32,
    words: Vec<u32>,
    sources: Vec<crate::ReadDataset>,
}

/// 已提交的设备到主机传输。完成后仍持有领取 CPU 结果所需的暂存。
pub struct GpuReadback {
    // End the native work before freeing the mapped destinations on abandonment.
    executions: Dependencies,
    sources: Vec<PendingDataset>,
    roots: Vec<ReadbackRoot>,
    domains: bindings::DomainRelations,
}

impl GpuReadback {
    /// 查询实际GPU执行的完成点，不等待或领取主机结果。
    pub fn is_complete(&self) -> Result<bool> {
        self.executions.observe(Execution::is_complete)
    }

    /// 等待实际GPU执行完成并领取 CPU 数据及原域关系。
    pub fn wait(self) -> Result<DomainBindings<CpuDataset>> {
        self.executions.observe(|execution| execution.wait().map(|()| true))?;
        let Self { sources, roots, domains, .. } = self;
        let mut ready = Vec::with_capacity(sources.len());
        for source in sources {
            ready.push(source.finish(&ready)?.freeze()?);
        }
        let datasets = roots
            .into_iter()
            .map(|root| match root {
                ReadbackRoot::Owned(dataset) => dataset.finish(&ready).map(Into::into),
                ReadbackRoot::Read(source) => Ok(ready[source].clone().into()),
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(DomainBindings { datasets, domains })
    }
}

enum ReadbackRoot {
    Owned(PendingDataset),
    Read(usize),
}

struct PendingDataset {
    layout: HostDatasetLayout,
    data: PendingPayload,
}

impl PendingDataset {
    fn finish(self, ready: &[crate::ReadDataset]) -> Result<Dataset> {
        let PendingPayload { stride, count, instance_input, sources, pages } = self.data;
        let byte_count = stride as usize * count as usize;
        // Each mapped page contributes its recorded logical bytes in storage order.
        let words = unsafe {
            readback_words(byte_count, instance_input, |output| {
                let mut written = 0;
                for (page, bytes) in pages {
                    page.read_with(|pointer| {
                        std::ptr::copy_nonoverlapping(pointer, output.add(written), bytes);
                    })?;
                    written += bytes;
                }
                Ok(())
            })
        }?;
        let sources = sources.into_iter().map(|source| ready[source].clone()).collect();
        Ok(ReadbackPayload { stride, count, words, sources }.finish(self.layout))
    }
}

struct PendingPayload {
    stride: u32,
    count: u32,
    instance_input: bool,
    sources: Vec<usize>,
    pages: Vec<(Allocation, usize)>,
}

/// Transient recorder for one closed GPU graph.
pub(super) struct PendingReadback<'a> {
    device: &'a Device,
    commands: Option<Commands>,
    dependencies: DependencySources,
    source_pages: Vec<Rc<Allocation>>,
    sources: Vec<PendingDataset>,
    recorded_sources: BTreeMap<usize, usize>,
}

impl<'a> PendingReadback<'a> {
    pub(super) fn submit(device: &'a Device, bindings: DomainBindings<GpuDataset>) -> Result<GpuReadback> {
        let DomainBindings { datasets, domains } = bindings;
        let mut readback = Self {
            device,
            commands: None,
            dependencies: DependencySources::default(),
            source_pages: Vec::new(),
            sources: Vec::new(),
            recorded_sources: BTreeMap::new(),
        };
        let roots = datasets
            .into_iter()
            .map(|data| match &*data.storage {
                DatasetStorage::Owned(_) => {
                    let payload = readback.record_payload(&data)?;
                    Ok(ReadbackRoot::Owned(PendingDataset {
                        layout: data.into_readback_layout(),
                        data: payload,
                    }))
                }
                DatasetStorage::Read(source) => readback.record_read(&data, source).map(ReadbackRoot::Read),
            })
            .collect::<Result<Vec<_>>>()?;
        let dependencies = readback.dependencies.prepare()?;
        let executions = match readback.commands {
            Some(commands) => {
                commands.barrier(0x1000, 0x4000, 0x1000, 0x2000);
                Dependencies::Single(Execution::submit(
                    ExecutionResources::DatasetReadback { commands, _source_pages: readback.source_pages },
                    dependencies,
                )?)
            }
            None => dependencies,
        };
        Ok(GpuReadback { executions, sources: readback.sources, roots, domains })
    }

    fn record_read(&mut self, data: &GpuDataset, source: &ReadDataset) -> Result<usize> {
        let key = Rc::as_ptr(&source.0) as usize;
        // Shared storage can be reached through bindings with distinct last uses.
        if let Some(&index) = self.recorded_sources.get(&key) {
            self.dependencies.insert_pending(data.dependency())?;
            return Ok(index);
        }
        let payload = self.record_payload(data)?;
        let index = self.sources.len();
        self.sources.push(PendingDataset {
            layout: HostDatasetLayout { ty: data.ty.clone(), bindings: data.views.bindings.clone() },
            data: payload,
        });
        self.recorded_sources.insert(key, index);
        Ok(index)
    }

    fn record_payload(&mut self, data: &GpuDataset) -> Result<PendingPayload> {
        self.dependencies.insert_pending(data.dependency())?;
        let sources = data.views.sources.iter()
            .map(|source| self.record_read(&source.0, source))
            .collect::<Result<Vec<_>>>()?;
        let mut pages = Vec::with_capacity(data.pages.len());
        if !data.pages.is_empty() {
            let commands = match &mut self.commands {
                Some(commands) => commands,
                slot @ None => slot.insert(Commands::new(self.device.inner.clone())?),
            };
            commands.barrier(data.producer.stage(), 0x1000, data.producer.access(), 0x800);
            let page_bytes = u64::from(data.page_records()) * u64::from(data.stride);
            let mut remaining = data.bytes();
            for source in &data.pages {
                let bytes = remaining.min(page_bytes);
                let staging = Allocation::new(self.device.inner.clone(), bytes, 2, 6, 4)?;
                record_readback_page(commands, source, &staging, bytes);
                self.source_pages.push(source.clone());
                pages.push((staging, bytes as usize));
                remaining -= bytes;
            }
        }
        Ok(PendingPayload { stride: data.stride, count: data.count, instance_input: data.instances.is_some(), sources, pages })
    }
}

fn record_readback_page(commands: &Commands, source: &Allocation, target: &Allocation, bytes: u64) {
    let copy = VkBufferCopy { srcOffset: 0, dstOffset: 0, size: bytes.div_ceil(4) * 4 };
    unsafe {
        (commands.inner.functions.vkCmdCopyBuffer)(commands.command, source.buffer, target.buffer, 1, &copy);
    }
}

// A successful writer must initialize exactly byte_count bytes at the provided pointer.
unsafe fn readback_words(
    byte_count: usize,
    instance_input: bool,
    write: impl FnOnce(*mut u8) -> Result<()>,
) -> Result<Vec<u32>> {
    let word_count = byte_count.div_ceil(4);
    let mut words = Vec::<u32>::new();
    words.try_reserve_exact(word_count)?;
    let output = words.spare_capacity_mut().as_mut_ptr().cast::<u8>();
    write(output)?;
    // A successful write initializes all logical bytes. Only the final word can need padding.
    unsafe {
        output.add(byte_count).write_bytes(0, word_count * 4 - byte_count);
        words.set_len(word_count);
    }
    // Native instance metadata is not portable. The host receives the declared matrix.
    if instance_input {
        for record in words.chunks_exact_mut(16) {
            record[12..16].fill(0);
        }
    }
    Ok(words)
}
impl ReadbackPayload {
    fn finish(self, layout: HostDatasetLayout) -> Dataset {
        let mut target = Dataset {
            ty: layout.ty,
            stride: self.stride,
            count: self.count,
            words: self.words,
            views: Views::default(),
        };
        target.attach_views(Views {
            sources: self.sources,
            bindings: layout.bindings,
        });
        target
    }
}
impl<'a> Readback<'a> {
    pub(super) fn new(device: &'a Device) -> Self {
        Self {
            device,
            sources: BTreeMap::new(),
            completion: None,
            commands: None,
            staging: None,
        }
    }

    pub(super) fn download_cpu(&mut self, data: GpuDataset) -> Result<CpuDataset> {
        match &*data.storage {
            DatasetStorage::Owned(_) => {
                let payload = self.read_graph(&data)?;
                Ok(payload.finish(data.into_readback_layout()).into())
            }
            DatasetStorage::Read(source) => {
                let key = Rc::as_ptr(&source.0) as usize;
                self.download_read(&data, key).map(Into::into)
            }
        }
    }
    fn download_read(
        &mut self,
        source: &GpuDataset,
        key: usize,
    ) -> Result<crate::ReadDataset> {
        if let Some(target) = self.sources.get(&key) {
            // Bindings of the same frozen storage can have different last uses.
            Execution::borrow_synchronous(source.dependency())?;
            return Ok(target.clone());
        }
        let target = self.download_graph(source)?.freeze()?;
        self.sources.insert(key, target.clone());
        Ok(target)
    }

    pub(super) fn download_graph(&mut self, data: &GpuDataset) -> Result<Dataset> {
        let payload = self.read_graph(data)?;
        Ok(payload.finish(HostDatasetLayout {
            ty: data.ty.clone(),
            bindings: data.views.bindings.clone(),
        }))
    }

    fn read_graph(&mut self, data: &GpuDataset) -> Result<ReadbackPayload> {
        let previous = Execution::borrow_synchronous(data.dependency())?;
        let mut sources = Vec::with_capacity(data.views.sources.len());
        for source in &data.views.sources {
            let key = Rc::as_ptr(&source.0) as usize;
            sources.push(self.download_read(&source.0, key)?);
        }
        let words = self.read_payload(data, previous)?;
        Ok(ReadbackPayload {
            stride: data.stride,
            count: data.count,
            words,
            sources,
        })
    }

    fn read_payload(&mut self, data: &GpuDataset, mut previous: Option<&Execution>) -> Result<Vec<u32>> {
        let Some(first) = data.pages.first() else {
            return Ok(Vec::new());
        };
        let byte_count = data.bytes() as usize;
        // The allocated pages cover all logical bytes, in storage order.
        unsafe {
            readback_words(byte_count, data.instances.is_some(), |output| {
                if self.staging.as_ref().is_none_or(|staging| staging.size < first.size) {
                    // A previous copy has finished and its bytes have reached the CPU result.
                    drop(self.staging.take());
                    self.staging = Some(Allocation::new(self.device.inner.clone(), first.size, 2, 6, 4)?);
                }
                let staging = self.staging.as_ref().unwrap();
                let page_bytes = data.page_records() as usize * data.stride as usize;
                let mut written = 0;
                let commands = match &mut self.commands {
                    Some(commands) => commands,
                    slot @ None => slot.insert(Commands::with_pool_flags(self.device.inner.clone(), 3)?),
                };
                for page in &data.pages {
                    let bytes = (byte_count - written).min(page_bytes);
                    if self.completion.is_some() {
                        commands.begin()?;
                    }
                    commands.barrier(data.producer.stage(), 0x1000, data.producer.access(), 0x800);
                    record_readback_page(commands, page, staging, bytes as u64);
                    commands.barrier(0x1000, 0x4000, 0x1000, 0x2000);
                    let point = match &mut self.completion {
                        Some(point) => {
                            commands.resubmit(point)?;
                            point
                        }
                        slot @ None => slot.insert(commands.submit_with(&[], &[], &[])?),
                    };
                    Execution::wait_synchronous(point, previous.take())?;
                    staging.read_with(|pointer| {
                        std::ptr::copy_nonoverlapping(pointer, output.add(written), bytes);
                    })?;
                    written += bytes;
                }
                Ok(())
            })
        }
    }
}
