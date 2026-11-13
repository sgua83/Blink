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
        self.execution.wait()?;
        self.queries.as_ref().map(|queries| queries.timings()).transpose()
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
    pub submitted: Cell<bool>,
    pub sequence: Cell<u64>,
    result: Cell<Option<i32>>,
}
impl Completion {
    pub fn new(inner: Rc<Inner>) -> Result<Rc<Self>> {
        let mut fence = 0;
        let mut info: VkFenceCreateInfo = zero();
        info.sType = 8;
        unsafe {
            check(
                (inner.functions.vkCreateFence)(inner.owner.device, &info, null(), &mut fence),
                "vkCreateFence",
            )?;
        }
        Ok(Rc::new(Self {
            inner,
            fence: Cell::new(fence),
            submitted: Cell::new(false),
            sequence: Cell::new(0),
            result: Cell::new(None),
        }))
    }
    pub fn wait(&self) -> Result<()> {
        let r = if let Some(r) = self.result.get() {
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
    fn is_complete(&self) -> Result<bool> {
        if let Some(r) = self.result.get() {
            check(r, "vkGetFenceStatus")?;
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
        if self.submitted.get() {
            if let Err(e) = self.wait() {
                eprintln!("设备提交生命周期终止：{e}");
                if self.result.get() != Some(-4) {
                    std::process::abort();
                }
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
        self.finish_for_drop();
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
            sequence: point.sequence.get(),
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
        _compiled: Rc<Compiled>,
        _accelerations: Vec<Acceleration>,
        _scratch: Vec<Option<Scratch>>,
        _binding_parameters: Option<Allocation>,
        _queries: Option<Rc<QueryPool>>,
    },
    Transfer {
        commands: Commands,
        _staging: Vec<Allocation>,
    },
    Acceleration {
        commands: Commands,
        _scratch: Allocation,
        _inputs: Vec<ray::RayInput>,
        _source: Option<Rc<ray::Structure>>,
    },
}
impl ExecutionResources {
    fn commands(&self) -> &Commands {
        match self {
            Self::Compute { commands, .. }
            | Self::Transfer { commands, .. }
            | Self::Acceleration { commands, .. } => commands,
        }
    }
}
pub(super) struct Execution {
    pub point: Rc<Completion>,
    resources: RefCell<Option<ExecutionResources>>,
    dependencies: RefCell<Vec<Rc<Self>>>,
    discarded: RefCell<Vec<DatasetStorage>>,
    discarded_structures: RefCell<Vec<ray::StructureStorage>>,
}
impl Execution {
    pub fn new(resources: ExecutionResources, dependencies: Vec<Rc<Self>>) -> Rc<Self> {
        Rc::new(Self {
            point: resources.commands().completion.clone(),
            resources: RefCell::new(Some(resources)),
            dependencies: RefCell::new(dependencies),
            discarded: RefCell::new(Vec::new()),
            discarded_structures: RefCell::new(Vec::new()),
        })
    }
    pub fn prepare(mut sources: Vec<Rc<Self>>) -> Result<Vec<Rc<Self>>> {
        sources.sort_unstable_by_key(|s| s.point.sequence.get());
        sources.dedup_by_key(|s| s.point.sequence.get());
        let mut finished = 0;
        for source in &sources {
            if !source.is_complete()? {
                break;
            }
            finished += 1;
        }
        sources.drain(..finished);
        Ok(sources)
    }
    pub fn discard(&self, storage: DatasetStorage) {
        if matches!(self.point.result.get(), Some(0 | -4)) {
            drop(storage);
        } else {
            self.discarded.borrow_mut().push(storage);
        }
    }
    pub fn discard_structure(&self, storage: ray::StructureStorage) {
        if matches!(self.point.result.get(), Some(0 | -4)) {
            drop(storage);
        } else {
            self.discarded_structures.borrow_mut().push(storage);
        }
    }
    fn release_known(&self) {
        // 保留真实原生终态；同队列成功覆盖生产依赖，设备丢失仍是错误。
        let result = self.point.result.get();
        let mut completed = std::mem::take(&mut *self.dependencies.borrow_mut());
        let mut index = 0;
        while index < completed.len() {
            let dependencies = {
                let previous = &completed[index];
                previous
                    .point
                    .result
                    .set(previous.point.result.get().or(result));
                std::mem::take(&mut *previous.dependencies.borrow_mut())
            };
            completed.extend(dependencies);
            index += 1;
        }
        for previous in completed.into_iter().rev() {
            previous.release_resources();
        }
        self.release_resources();
    }
    fn release_resources(&self) {
        let resources = self.resources.borrow_mut().take();
        drop(resources);
        let storage = std::mem::take(&mut *self.discarded.borrow_mut());
        drop(storage);
        let structures = std::mem::take(&mut *self.discarded_structures.borrow_mut());
        drop(structures);
        self.point.release_fence();
    }
    pub fn is_complete(&self) -> Result<bool> {
        let result = self.point.is_complete();
        if matches!(self.point.result.get(), Some(0 | -4)) {
            self.release_known();
        }
        result
    }
    pub fn wait(&self) -> Result<()> {
        let result = self.point.wait();
        if matches!(self.point.result.get(), Some(0 | -4)) {
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
