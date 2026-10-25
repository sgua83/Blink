//! 原生完成点、命令资源及物理数据的使用寿命；共享分支只共享执行事实。
use super::*;
use std::cell::RefCell;

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
            self.result.set(Some(r));
            r
        };
        check(r, "vkWaitForFences")
    }
    fn is_complete(&self) -> Result<bool> {
        if let Some(r) = self.result.get() {
            check(r, "vkWaitForFences")?;
            return Ok(true);
        }
        let r = unsafe {
            (self.inner.functions.vkGetFenceStatus)(self.inner.owner.device, self.fence.get())
        };
        match r {
            0 => {
                self.result.set(Some(0));
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
}
impl QueryPool {
    pub fn take(commands: &mut Commands, count: u32) -> Rc<Self> {
        Rc::new(Self {
            inner: commands.inner.clone(),
            handle: std::mem::replace(&mut commands.queries, 0),
            count,
        })
    }
    pub fn read(&self) -> Result<GpuTimings> {
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
        let bits = self.inner.timestamp_bits;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let period = self.inner.properties.limits.timestampPeriod as f64;
        Ok(GpuTimings {
            elapsed_ms: (stamps[count - 1].wrapping_sub(stamps[0]) & mask) as f64 * period
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
        _queries: Rc<QueryPool>,
    },
    Transfer {
        commands: Commands,
        _staging: Vec<Allocation>,
    },
    Acceleration {
        commands: Commands,
        _scratch: Allocation,
        _inputs: Vec<ray::RayInput>,
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
    discarded: RefCell<Vec<GpuStorage>>,
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
        let mut all = Vec::new();
        for source in sources {
            all.extend(std::mem::take(&mut *source.dependencies.borrow_mut()));
            all.push(source);
        }
        all.sort_unstable_by_key(|s| s.point.sequence.get());
        all.dedup_by_key(|s| s.point.sequence.get());
        let mut finished = 0;
        for source in &all {
            if !source.is_complete()? {
                break;
            }
            finished += 1;
        }
        all.drain(..finished);
        Ok(all)
    }
    pub fn discard(&self, storage: GpuStorage) {
        if self.point.result.get() == Some(0) {
            drop(storage);
        } else {
            self.discarded.borrow_mut().push(storage);
        }
    }
    pub fn discard_structure(&self, storage: ray::StructureStorage) {
        if self.point.result.get() == Some(0) {
            drop(storage);
        } else {
            self.discarded_structures.borrow_mut().push(storage);
        }
    }
    fn release_known(&self) {
        let dependencies = std::mem::take(&mut *self.dependencies.borrow_mut());
        for previous in dependencies {
            // 当前 Fence 完成，包含同一队列较早提交的真实命令。
            previous.point.result.set(Some(0));
            previous.release_known();
        }
        let resources = self.resources.borrow_mut().take();
        drop(resources);
        let storage = std::mem::take(&mut *self.discarded.borrow_mut());
        drop(storage);
        let structures = std::mem::take(&mut *self.discarded_structures.borrow_mut());
        drop(structures);
        self.point.release_fence();
    }
    pub fn is_complete(&self) -> Result<bool> {
        if self.point.is_complete()? {
            self.release_known();
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn wait(&self) -> Result<()> {
        self.point.wait()?;
        self.release_known();
        Ok(())
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        self.point.finish_for_drop();
        if self.point.result.get() == Some(0) {
            self.release_known();
        }
    }
}
