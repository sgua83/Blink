//! 显式传输闭合资源图；一次操作内保留共享，不解析源平台地址或维护跨操作缓存。
use super::*;
use crate::cpu::CpuDataset;
use std::collections::BTreeMap;

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

    pub(super) fn upload_payload(&self, target: &mut GpuDataset, words: &[u32]) -> Result<()> {
        if words.is_empty() {
            return Ok(());
        }
        let mut commands = Commands::new(self.inner.clone())?;
        let mut staging = Vec::new();
        self.stage_payload(
            target,
            words,
            |start, mapped| target.views.encode(start, mapped, |source| source.0.view_header()),
            &mut commands,
            &mut staging,
        )?;
        target.last_use = Some(self.submit_dataset_transfer(commands, staging, false)?);
        Ok(())
    }
    fn stage_payload(
        &self,
        target: &GpuDataset,
        words: &[u32],
        encode_views: impl Fn(usize, &mut [u32]),
        commands: &mut Commands,
        staging: &mut Vec<Allocation>,
    ) -> Result<()> {
        let bytes = unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), target.bytes() as usize) };
        for (index, page) in target.pages.iter().enumerate() {
            let bytes_per_page = target.page_records() as usize * target.stride as usize;
            let start = index * bytes_per_page;
            let bytes = &bytes[start..(start + bytes_per_page).min(bytes.len())];
            self.stage_page(
                page,
                bytes.len() as u64,
                |source| {
                    source.write_with(bytes, |mapped| {
                        encode_views(start / 4, mapped);
                    })
                },
                commands,
                staging,
            )?;
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
        if !words.is_empty() {
            let commands = match &mut self.commands {
                Some(commands) => commands,
                slot @ None => slot.insert(Commands::new(self.device.inner.clone())?),
            };
            self.device.stage_payload(
                &data,
                words,
                |start, mapped| views.encode(start, mapped, |&index| self.shared[index].data.view_header()),
                commands,
                &mut self.staging,
            )?;
        }
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
            self.data.last_use = execution.cloned();
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
        let mut words = self.read_payload(data, previous)?;
        // 原生实例尾部不是可移植数据；主机只接收 B 声明的矩阵。
        if data.instances.is_some() {
            for record in words.chunks_exact_mut(16) {
                record[12..16].fill(0);
            }
        }
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
        let word_count = byte_count.div_ceil(4);
        let mut words = Vec::<u32>::new();
        words.try_reserve_exact(word_count)?;
        let output = words.spare_capacity_mut().as_mut_ptr().cast::<u8>();
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
            let copy = VkBufferCopy {
                srcOffset: 0,
                dstOffset: 0,
                size: (bytes.div_ceil(4) * 4) as u64,
            };
            unsafe {
                (self.device.inner.functions.vkCmdCopyBuffer)(
                    commands.command,
                    page.buffer,
                    staging.buffer,
                    1,
                    &copy,
                );
            }
            commands.barrier(0x1000, 0x4000, 0x1000, 0x2000);
            let point = match &mut self.completion {
                Some(point) => {
                    commands.resubmit(point)?;
                    point
                }
                slot @ None => slot.insert(commands.submit_with(&[], &[], &[])?),
            };
            Execution::wait_synchronous(point, previous.take())?;
            staging.read_with(|pointer| unsafe {
                std::ptr::copy_nonoverlapping(pointer, output.add(written), bytes);
            })?;
            written += bytes;
        }
        // The allocated pages cover all logical bytes. Initialize only the final
        // word's padding before exposing the fully written CPU storage.
        unsafe {
            output.add(byte_count).write_bytes(0, word_count * 4 - byte_count);
            words.set_len(word_count);
        }
        Ok(words)
    }
}
