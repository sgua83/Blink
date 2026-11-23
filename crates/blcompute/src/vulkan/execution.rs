//! 原生完成点、命令资源及物理数据的使用寿命；共享分支只共享执行事实。
use super::*;
use std::{cell::RefCell, sync::Arc};

/// 已提交执行的原生完成与时间戳；不拥有另一份数据或帧计数。
/// 数据及AS继续通过原物理最后使用点保持到实际GPU消费结束。
pub struct GpuCompletion {
    pub(super) execution: Rc<Execution>,
    pub(super) queries: Option<Rc<QueryPool>>,
}
impl GpuCompletion {
    pub fn is_complete(&self) -> Result<bool> {
        self.execution.is_complete()
    }
    pub fn wait(self) -> Result<Option<GpuTimings>> {
        self.wait_timings()
    }

    fn wait_timings(&self) -> Result<Option<GpuTimings>> {
        self.execution.wait()?;
        self.queries.as_ref().map(|queries| queries.timings()).transpose()
    }

    pub(crate) fn wait_bound<'a>(&self, datasets: impl Iterator<Item = &'a mut GpuDataset>) -> Result<Option<GpuTimings>> {
        let timings = self.wait_timings()?;
        for dataset in datasets {
            if dataset.last_use.as_ref().is_some_and(|point| Rc::ptr_eq(point, &self.execution)) {
                dataset.last_use = None;
            }
        }
        Ok(timings)
    }

    /// 等待本次提交结束，交回显式请求的原设备时间戳与实际阶段。
    pub fn wait_timestamps(self) -> Result<Option<GpuTimestamps>> {
        self.execution.wait()?;
        self.queries
            .as_ref()
            .map(|queries| queries.timestamps(&self.execution.point))
            .transpose()
    }
}

/// 同一完成提交的设备时间戳。设备与序号只在原存活逻辑设备内关联。
pub struct GpuTimestamps {
    pub device: usize,
    pub sequence: u64,
    /// 每个设备tick对应的纳秒数。
    pub period_ns: f32,
    /// 原计算队列实际使用的低位位数。
    pub valid_bits: u32,
    program: Arc<Program>,
    stamps: Vec<u64>,
}

/// 原提交的一轮及该轮实际阶段，序号从零开始。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuStageTimestamps {
    pub step: usize,
    pub stage: usize,
    pub start_ticks: u64,
    pub end_ticks: u64,
}
impl GpuTimestamps {
    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn stages(&self) -> impl ExactSizeIterator<Item = GpuStageTimestamps> + '_ {
        let stage_count = self.program.stages().len();
        self.stamps.chunks_exact(2).enumerate().map(move |(index, pair)| {
            GpuStageTimestamps {
                step: index / stage_count,
                stage: index % stage_count,
                start_ticks: pair[0],
                end_ticks: pair[1],
            }
        })
    }
}

pub(super) struct Completion {
    pub inner: Rc<Inner>,
    pub fence: Cell<VkFence>,
    pub sequence: u64,
    result: Cell<Option<i32>>,
}
impl Completion {
    pub fn submit(inner: Rc<Inner>, next: u64, batch: Option<&VkSubmitInfo>) -> Result<Self> {
        let point = Self::new(inner, next - 1)?;
        point.inner.enqueue(next, point.fence.get(), batch)?;
        Ok(point)
    }

    fn new(inner: Rc<Inner>, sequence: u64) -> Result<Self> {
        let mut fence = 0;
        let mut info: VkFenceCreateInfo = zero();
        info.sType = 8;
        unsafe {
            check(
                (inner.functions.vkCreateFence)(inner.owner.device, &info, null(), &mut fence),
                "vkCreateFence",
            )?;
        }
        Ok(Self {
            inner,
            fence: Cell::new(fence),
            sequence,
            result: Cell::new(None),
        })
    }
    pub fn reset(&mut self, sequence: u64) -> Result<()> {
        let fence = self.fence.get();
        unsafe {
            check(
                (self.inner.functions.vkResetFences)(self.inner.owner.device, 1, &fence),
                "vkResetFences",
            )?;
        }
        self.sequence = sequence;
        self.result.set(None);
        Ok(())
    }
    fn terminal_result(&self) -> Option<i32> {
        self.result.get().or_else(|| self.inner.queue_lost().then_some(-4))
    }
    pub fn wait(&self) -> Result<()> {
        let r = if let Some(r) = self.terminal_result() {
            r
        } else {
            let fence = self.fence.get();
            let r = unsafe {
                (self.inner.functions.vkWaitForFences)(
                    self.inner.owner.device,
                    1,
                    &fence,
                    1,
                    u64::MAX,
                )
            };
            if matches!(r, 0 | -4) {
                self.result.set(Some(r));
            }
            r
        };
        check(r, "vkWaitForFences")
    }
    fn observed_complete(&self) -> Result<bool> {
        let Some(result) = self.terminal_result() else { return Ok(false); };
        check(result, "vkGetFenceStatus")?;
        Ok(true)
    }
    fn is_complete(&self) -> Result<bool> {
        if self.observed_complete()? {
            return Ok(true);
        }
        let r = unsafe {
            (self.inner.functions.vkGetFenceStatus)(self.inner.owner.device, self.fence.get())
        };
        match r {
            0 | -4 => {
                self.result.set(Some(r));
                check(r, "vkGetFenceStatus")?;
                Ok(true)
            }
            1 => Ok(false),
            _ => Err(error(format!("vkGetFenceStatus 返回 VkResult {r}"))),
        }
    }
    pub fn finish_for_drop(&self) {
        if self.terminal_result().is_some() {
            return;
        }
        if let Err(e) = self.wait() {
            eprintln!("设备提交生命周期终止：{e}");
            if self.result.get() != Some(-4) {
                std::process::abort();
            }
        }
    }
    fn release_fence(&self) {
        let fence = self.fence.replace(0);
        if fence != 0 {
            unsafe {
                (self.inner.functions.vkDestroyFence)(self.inner.owner.device, fence, null());
            }
        }
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        self.release_fence();
    }
}
pub(super) struct QueryPool {
    inner: Rc<Inner>,
    handle: u64,
    count: u32,
    program: Arc<Program>,
}
impl QueryPool {
    pub fn take(commands: &mut Commands, count: u32, program: Arc<Program>) -> Rc<Self> {
        Rc::new(Self {
            inner: commands.inner.clone(),
            handle: std::mem::replace(&mut commands.queries, 0),
            count,
            program,
        })
    }

    fn read(&self) -> Result<Vec<u64>> {
        let count = self.count as usize;
        let mut stamps = vec![0u64; count];
        unsafe {
            check(
                (self.inner.functions.vkGetQueryPoolResults)(
                    self.inner.owner.device,
                    self.handle,
                    0,
                    self.count,
                    count * 8,
                    stamps.as_mut_ptr().cast(),
                    8,
                    1,
                ),
                "vkGetQueryPoolResults",
            )?;
        }
        Ok(stamps)
    }

    fn timestamps(&self, point: &Completion) -> Result<GpuTimestamps> {
        Ok(GpuTimestamps {
            device: point.inner.owner.device as usize,
            sequence: point.sequence,
            period_ns: self.inner.properties.limits.timestampPeriod,
            valid_bits: self.inner.timestamp_bits,
            program: self.program.clone(),
            stamps: self.read()?,
        })
    }

    pub(super) fn timings(&self) -> Result<GpuTimings> {
        let stamps = self.read()?;
        let bits = self.inner.timestamp_bits;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let period = self.inner.properties.limits.timestampPeriod as f64;
        Ok(GpuTimings {
            elapsed_ms: (stamps[stamps.len() - 1].wrapping_sub(stamps[0]) & mask) as f64 * period
                / 1_000_000.0,
            stage_ms: stamps
                .chunks_exact(2)
                .map(|t| (t[1].wrapping_sub(t[0]) & mask) as f64 * period / 1_000_000.0)
                .collect(),
        })
    }
}
impl Drop for QueryPool {
    fn drop(&mut self) {
        unsafe {
            (self.inner.functions.vkDestroyQueryPool)(self.inner.owner.device, self.handle, null());
        }
    }
}
pub(super) enum ExecutionResources {
    Compute {
        commands: Commands,
        _resources: Option<ComputeResources>,
        _queries: Option<Rc<QueryPool>>,
    },
    DatasetTransfer {
        commands: Commands,
        _staging: Vec<Allocation>,
        covers_view_sources: bool,
    },
    Present { commands: Commands },
    ImageCopy {
        commands: Commands,
        _staging: Option<Allocation>,
        discarded: Option<texture::Storage>,
    },
    Acceleration {
        commands: Commands,
        _scratch: Allocation,
        _inputs: Vec<Allocation>,
        _source: Option<Rc<ray::Structure>>,
        retired: Option<Box<ray::StructureStorage>>,
    },
}
impl ExecutionResources {
    fn commands(&self) -> &Commands {
        match self {
            Self::Compute { commands, .. }
            | Self::DatasetTransfer { commands, .. }
            | Self::Present { commands }
            | Self::ImageCopy { commands, .. }
            | Self::Acceleration { commands, .. } => commands,
        }
    }
}
pub(super) enum DiscardedDataset {
    TransferTargets {
        _pages: Vec<Allocation>,
        _instance_pointers: Option<Allocation>,
    },
    Owned {
        _binding_table: Allocation,
        _pages: Vec<Allocation>,
        _sources: Vec<ReadDataset>,
        _instances: Option<ray::InstanceInput>,
    },
    Read { _source: ReadDataset },
}
impl DatasetStorage {
    pub(super) fn into_retirement(self) -> (Option<HostDatasetLayout>, DiscardedDataset) {
        match self {
            Self::Owned(GpuStorage {
                allocation, pages, ty, views: Views { sources, bindings }, instances, ..
            }) => (
                Some(HostDatasetLayout { ty, bindings }),
                DiscardedDataset::Owned {
                    _binding_table: allocation,
                    _pages: pages,
                    _sources: sources,
                    _instances: instances,
                },
            ),
            Self::Read(source) => (None, DiscardedDataset::Read { _source: source }),
        }
    }
}
impl DiscardedDataset {
    pub(super) fn retire(self, execution: Option<&Execution>) {
        let Some(execution) = execution else { return; };
        if execution.point.terminal_result().is_some() {
            return;
        }
        let storage = match self {
            Self::Owned { _pages, _instances, .. }
                if matches!(execution.resources.borrow().as_ref(), Some(ExecutionResources::DatasetTransfer { .. })) =>
            {
                // Initialization writes pages and instance pointers without following the binding table.
                Self::TransferTargets {
                    _pages,
                    _instance_pointers: _instances.and_then(|input| input.pointers),
                }
            }
            storage => storage,
        };
        execution.discarded.borrow_mut().push(storage);
    }
}

pub(super) struct Execution {
    pub point: Completion,
    resources: RefCell<Option<ExecutionResources>>,
    // Some holds the unpublished completion, even for a fence without commands.
    dependencies: RefCell<Option<Dependencies>>,
    discarded: RefCell<Vec<DiscardedDataset>>,
}

#[derive(Default)]
pub(super) enum Dependencies {
    #[default]
    Empty,
    Single(Rc<Execution>),
    Multiple(Vec<Rc<Execution>>),
}

impl Dependencies {
    fn append_to(self, completed: &mut Vec<Rc<Execution>>) {
        match self {
            Self::Empty => {}
            Self::Single(previous) => completed.push(previous),
            Self::Multiple(sources) => completed.extend(sources),
        }
    }
}

#[derive(Default)]
pub(super) enum DependencySources {
    #[default]
    Empty,
    Single(Rc<Execution>),
    Multiple(BTreeMap<u64, Rc<Execution>>),
}

impl DependencySources {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn insert(&mut self, source: &Rc<Execution>) {
        self.insert_with(source.point.sequence, || source.clone());
    }

    pub fn insert_owned(&mut self, source: Rc<Execution>) {
        self.insert_with(source.point.sequence, || source);
    }

    fn insert_with(&mut self, sequence: u64, source: impl FnOnce() -> Rc<Execution>) {
        *self = match std::mem::take(self) {
            Self::Empty => Self::Single(source()),
            Self::Single(first) if first.point.sequence == sequence => Self::Single(first),
            Self::Single(first) => {
                let mut sources = BTreeMap::new();
                sources.insert(first.point.sequence, first);
                sources.insert(sequence, source());
                Self::Multiple(sources)
            }
            Self::Multiple(mut sources) => {
                sources.entry(sequence).or_insert_with(source);
                Self::Multiple(sources)
            }
        };
    }

    pub fn prepare(self) -> Result<Dependencies> {
        let mut sources = match self {
            Self::Empty => return Ok(Dependencies::Empty),
            Self::Single(source) => {
                return Ok(if source.is_complete()? {
                    Dependencies::Empty
                } else {
                    Dependencies::Single(source)
                });
            }
            Self::Multiple(sources) => sources.into_values(),
        };
        while let Some(source) = sources.next() {
            if source.is_complete()? {
                continue;
            }
            if sources.len() == 0 {
                return Ok(Dependencies::Single(source));
            }
            let mut pending = Vec::with_capacity(1 + sources.len());
            pending.push(source);
            // A later point can already hold an error. Read its recorded result without polling.
            for later in sources {
                later.point.observed_complete()?;
                pending.push(later);
            }
            return Ok(Dependencies::Multiple(pending));
        }
        Ok(Dependencies::Empty)
    }
}

impl Execution {
    pub fn submit(
        resources: ExecutionResources,
        dependencies: Dependencies,
    ) -> Result<Rc<Self>> {
        Self::submit_with(resources, dependencies, &[], &[], &[])
    }
    pub fn submit_with(
        resources: ExecutionResources,
        dependencies: Dependencies,
        wait: &[u64],
        stages: &[u32],
        signal: &[u64],
    ) -> Result<Rc<Self>> {
        let point = resources.commands().submit_with(wait, stages, signal)?;
        Ok(Self::submitted(point, Some(resources), dependencies))
    }
    pub fn submit_fence(inner: Rc<Inner>, dependencies: Dependencies) -> Result<Rc<Self>> {
        let next = inner.next_submission()?;
        let point = Completion::submit(inner, next, None)?;
        Ok(Self::submitted(point, None, dependencies))
    }
    fn submitted(point: Completion, resources: Option<ExecutionResources>, dependencies: Dependencies) -> Rc<Self> {
        Rc::new(Self {
            point,
            resources: RefCell::new(resources),
            dependencies: RefCell::new(Some(dependencies)),
            discarded: RefCell::new(Vec::new()),
        })
    }
    pub fn prepare_one(source: Option<&Rc<Self>>) -> Result<Option<&Rc<Self>>> {
        if let Some(source) = source
            && !source.is_complete()?
        {
            return Ok(Some(source));
        }
        Ok(None)
    }
    /// The retained initial upload owns the copies for this storage and its CPU view sources.
    pub(super) fn uploads_dataset_graph(&self) -> bool {
        matches!(
            self.resources.borrow().as_ref(),
            Some(ExecutionResources::DatasetTransfer { covers_view_sources: true, .. }),
        )
    }
    pub fn discard_structure(&self, storage: ray::StructureStorage) {
        if self.point.terminal_result().is_none()
            && let Some(ExecutionResources::Acceleration { retired, .. }) = self.resources.borrow_mut().as_mut()
        {
            *retired = Some(Box::new(storage));
        }
    }
    pub(super) fn discard_image(&self, storage: texture::Storage) {
        if self.point.terminal_result().is_none()
            && let Some(ExecutionResources::ImageCopy { discarded, .. }) = self.resources.borrow_mut().as_mut()
        {
            *discarded = Some(storage);
        }
    }
    fn release_known(&self) {
        let Some(dependencies) = self.dependencies.take() else {
            return;
        };
        Self::release_dependencies(dependencies, self.point.terminal_result());
        self.release_resources();
    }
    pub fn borrow_synchronous(source: Option<&Rc<Self>>) -> Result<Option<&Self>> {
        let Some(source) = source else {
            return Ok(None);
        };
        if source.point.observed_complete()? {
            return Ok(None);
        }
        // The readback fence covers earlier work on this queue.
        Ok(Some(source))
    }
    pub fn wait_synchronous(point: &Completion, previous: Option<&Self>) -> Result<()> {
        let result = point.wait();
        point.finish_for_drop();
        if let Some(previous) = previous {
            previous.point.result.set(previous.point.result.get().or(point.result.get()));
            previous.release_known();
        }
        result
    }
    fn take_completed_dependencies(&self, result: Option<i32>) -> Option<Dependencies> {
        let dependencies = self.dependencies.take()?;
        // 保留真实原生终态；同队列成功覆盖生产依赖，设备丢失仍是错误。
        self.point.result.set(self.point.result.get().or(result));
        Some(dependencies)
    }
    fn release_dependencies(dependencies: Dependencies, result: Option<i32>) {
        let (mut completed, mut index) = match dependencies {
            Dependencies::Empty => return,
            Dependencies::Single(previous) => {
                let Some(sources) = previous.take_completed_dependencies(result) else {
                    return;
                };
                if matches!(sources, Dependencies::Empty) {
                    previous.release_resources();
                    return;
                }
                let mut completed = vec![previous];
                sources.append_to(&mut completed);
                (completed, 1)
            }
            Dependencies::Multiple(completed) => (completed, 0),
        };
        while index < completed.len() {
            let previous = &completed[index];
            let Some(sources) = previous.take_completed_dependencies(result) else {
                completed.swap_remove(index);
                continue;
            };
            sources.append_to(&mut completed);
            index += 1;
        }
        // Publish the entire closure before destructors can consult an earlier point.
        for previous in completed.into_iter().rev() {
            previous.release_resources();
        }
    }
    fn release_resources(&self) {
        drop(self.resources.take());
        let storage = std::mem::take(&mut *self.discarded.borrow_mut());
        drop(storage);
        self.point.release_fence();
    }
    pub fn is_complete(&self) -> Result<bool> {
        let result = self.point.is_complete();
        if self.point.terminal_result().is_some() {
            self.release_known();
        }
        result
    }
    pub fn wait(&self) -> Result<()> {
        let result = self.point.wait();
        if self.point.terminal_result().is_some() {
            self.release_known();
        }
        result
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        self.point.finish_for_drop();
        self.release_known();
    }
}
