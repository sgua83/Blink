//! 调用方持有的本机编译资源；所有活动工作借用同一份执行许可。
use std::{
    io,
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex},
    thread,
};
use crate::parallel::Error;
#[cfg(feature = "compiler")]
mod process;

/// Windows处理器组中的一个逻辑处理器；不是跨机器地址。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Processor {
    pub group: u16,
    pub number: u8,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowSolvePhase<'a> {
    Started,
    Finished { succeeded: bool },
    Storage { function: &'a str, storage: BorrowSolveStorage },
}

/// Initialized element count and reserved element capacity of the original
/// container. Tree containers do not expose a capacity.
#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisStorage {
    pub len: usize,
    pub capacity: Option<usize>,
    pub element_bytes: usize,
}

#[cfg(feature = "compiler")]
impl AnalysisStorage {
    pub(crate) fn vector<T>(values: &Vec<T>) -> Self {
        Self { len: values.len(), capacity: Some(values.capacity()), element_bytes: std::mem::size_of::<T>() }
    }

    pub(crate) fn entries<K, V>(len: usize, capacity: Option<usize>) -> Self {
        Self { len, capacity, element_bytes: std::mem::size_of::<(K, V)>() }
    }
}

/// A read of storage at its existing producer, before that producer releases it.
#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowSolveStorage {
    Relation {
        root: usize,
        nodes: AnalysisStorage,
        frames: AnalysisStorage,
        resolved: AnalysisStorage,
        previous_inputs: AnalysisStorage,
        next_inputs: AnalysisStorage,
    },
    ReadFrame { memory: usize, tables: ConditionStorage },
}

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionStorage {
    pub decisions: AnalysisStorage,
    pub unique: AnalysisStorage,
    pub operations: AnalysisStorage,
    pub negatives: AnalysisStorage,
    pub imported: AnalysisStorage,
    pub emitted: AnalysisStorage,
}

/// A function participating in this invocation of the ordinary borrow solver.
#[cfg(feature = "compiler")]
pub struct BorrowSolveFunction<'a> {
    pub name: &'a str,
    pub source: &'a crate::ast::Source,
    pub span: crate::ast::Span,
}

/// The sources are borrowed from the original compilation for this callback.
#[cfg(feature = "compiler")]
pub struct BorrowSolveEvent<'a> {
    pub phase: BorrowSolvePhase<'a>,
    pub functions: &'a [BorrowSolveFunction<'a>],
}

#[cfg(feature = "compiler")]
pub(crate) type BorrowSolveObserver = dyn for<'a> Fn(BorrowSolveEvent<'a>) + Send + Sync;

#[derive(Clone)]
pub struct Compiler {
    resources: Arc<Resources>,
    max_parameter_bytes: u32,
    #[cfg(feature = "compiler")]
    pub(crate) borrow_solve_observer: Option<Arc<BorrowSolveObserver>>,
}
struct Resources {
    workers: usize,
    processors: Option<Vec<Processor>>,
    available: Mutex<Vec<usize>>,
    released: Condvar,
    #[cfg(feature = "compiler")]
    bootstrap_jobserver: Option<jobserver::Client>,
    #[cfg(all(feature = "installed", feature = "compiler"))]
    sdk_input: Mutex<Option<crate::object::InstalledSdk>>,
}
enum CpuLease {
    Local(usize),
    #[cfg(feature = "compiler")]
    Shared { _permit: jobserver::Acquired },
}
struct Permit {
    compiler: Compiler,
    lease: Mutex<Option<CpuLease>>,
}
impl Permit {
    fn release(&self, lease: CpuLease) {
        match lease {
            CpuLease::Local(slot) => {
                let resources = &self.compiler.resources;
                resources.available.lock().unwrap().push(slot);
                resources.released.notify_one();
            }
            #[cfg(feature = "compiler")]
            CpuLease::Shared { .. } => drop(lease),
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.get_mut().unwrap().take() {
            self.release(lease);
        }
    }
}
/// 一份正在执行的工作。共享引用不产生新的执行许可。
#[derive(Clone)]
pub(crate) struct Worker {
    permit: Arc<Permit>,
}
impl Compiler {
    /// 普通本机调用的资源估计。需要组合多个调用时，共享同一Compiler或其clone。
    pub fn available() -> io::Result<Self> {
        Ok(Self::with_workers(thread::available_parallelism()?))
    }
    pub fn with_workers(workers: NonZeroUsize) -> Self {
        Self::resources(workers.get(), None)
    }
    /// 每个活动工作绑定到所选处理器之一；OS拒绝真实绑定时直接报错。
    pub fn on_processors(processors: Vec<Processor>) -> io::Result<Self> {
        if processors.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "编译CPU集合不能为空"));
        }
        let unique: std::collections::BTreeSet<_> = processors.iter().copied().collect();
        if unique.len() != processors.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "编译CPU集合包含重复处理器"));
        }
        for processor in &processors {
            processor.mask()?;
        }
        Ok(Self::resources(processors.len(), Some(processors)))
    }
    fn resources(workers: usize, processors: Option<Vec<Processor>>) -> Self {
        Self { max_parameter_bytes: 128, resources: Arc::new(Resources {
            workers, processors,
            available: Mutex::new((0..workers).rev().collect()),
            released: Condvar::new(),
            #[cfg(feature = "compiler")]
            bootstrap_jobserver: None,
            #[cfg(all(feature = "installed", feature = "compiler"))]
            sdk_input: Mutex::new(None),
        }),
            #[cfg(feature = "compiler")]
            borrow_solve_observer: None,
        }
    }
    pub fn workers(&self) -> usize { self.resources.workers }
    #[cfg(feature = "compiler")]
    #[doc(hidden)]
    pub fn bootstrap_worker() -> io::Result<Self> {
        // The launcher retains the one permit for this process's initial work.
        let jobserver = unsafe { jobserver::Client::from_env() }
            .ok_or_else(|| io::Error::other("B引导编译缺少继承的Cargo工作许可"))?;
        let mut compiler = Self::with_workers(NonZeroUsize::MIN);
        Arc::get_mut(&mut compiler.resources).unwrap().bootstrap_jobserver = Some(jobserver);
        Ok(compiler)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn bootstrap_jobserver(&self) -> Option<&jobserver::Client> {
        self.resources.bootstrap_jobserver.as_ref()
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn shared_worker(&self, permit: jobserver::Acquired) -> Worker {
        self.with_lease(CpuLease::Shared { _permit: permit })
    }
    /// 本次新生成参数布局的字节上限；不会查询或改变执行设备。
    pub fn with_max_parameter_bytes(mut self, bytes: u32) -> Self {
        self.max_parameter_bytes = bytes;
        self
    }
    pub fn max_parameter_bytes(&self) -> u32 { self.max_parameter_bytes }
    #[cfg(all(feature = "installed", feature = "compiler"))]
    pub(crate) fn with_sdk_input<T>(&self, use_root: impl FnOnce(&std::path::Path) -> T) -> io::Result<T> {
        let mut input = self.resources.sdk_input.lock().unwrap();
        let sdk = match &mut *input {
            Some(sdk) => sdk,
            empty => empty.insert(crate::object::InstalledSdk::open()?),
        };
        Ok(use_root(&sdk.root))
    }
    fn with_lease(&self, lease: CpuLease) -> Worker {
        Worker { permit: Arc::new(Permit {
            compiler: self.clone(),
            lease: Mutex::new(Some(lease)),
        }) }
    }
    fn worker(&self, slot: usize) -> Worker {
        self.with_lease(CpuLease::Local(slot))
    }
    pub(crate) fn try_worker(&self) -> Option<Worker> {
        self.resources.available.lock().unwrap().pop().map(|slot| self.worker(slot))
    }
    fn acquire_slot(&self) -> usize {
        let mut available = self.resources.available.lock().unwrap();
        loop {
            if let Some(slot) = available.pop() { return slot; }
            available = self.resources.released.wait(available).unwrap();
        }
    }
    fn acquire(&self) -> Worker {
        self.worker(self.acquire_slot())
    }
    pub(crate) fn execute<T: Send, E: Send>(
        &self, compile: impl FnOnce(&Worker) -> Result<T, E> + Send,
    ) -> Result<T, Error<E>> {
        let worker = self.acquire();
        if self.resources.processors.is_none() {
            return compile(&worker).map_err(Error::Work);
        }
        thread::scope(|scope| {
            let current = &worker;
            let handle = thread::Builder::new().spawn_scoped(scope, move || {
                current.bind().map_err(Error::Resource)?;
                compile(current).map_err(Error::Work)
            }).map_err(Error::Resource)?;
            handle.join().unwrap_or_else(|error| std::panic::resume_unwind(error))
        })
    }
    pub(crate) fn ast<T: Send>(
        &self, path: &std::path::Path,
        compile: impl FnOnce(&Worker) -> crate::ast::Result<T> + Send,
    ) -> crate::ast::Result<T> {
        self.execute(compile).map_err(|error| match error {
            Error::Work(error) => error,
            Error::Resource(error) => diagnostic(path, error),
        })
    }
}
pub(crate) fn diagnostic(path: &std::path::Path, error: io::Error) -> crate::ast::Diagnostic {
    crate::ast::Diagnostic {
        source: crate::ast::Source { path: path.into(), text: "".into() },
        span: crate::ast::Span::default(), code: "B020",
        message: format!("无法创建本机编译工作：{error}"),
    }
}
impl Worker {
    pub(crate) fn compiler(&self) -> &Compiler { &self.permit.compiler }
    fn processor(&self) -> Option<Processor> {
        let lease = self.permit.lease.lock().unwrap();
        let slot = match lease.as_ref()? {
            CpuLease::Local(slot) => *slot,
            #[cfg(feature = "compiler")]
            CpuLease::Shared { .. } => return None,
        };
        self.compiler().resources.processors.as_ref().map(|processors| processors[slot])
    }
    pub(crate) fn bind(&self) -> io::Result<()> {
        if let Some(processor) = self.processor() {
            bind(processor)?;
        }
        Ok(())
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn wait_for_work<T>(&self, wait: impl FnOnce() -> T) -> io::Result<T> {
        let lease = self.permit.lease.lock().unwrap().take().unwrap();
        let shared = matches!(lease, CpuLease::Shared { .. });
        self.permit.release(lease);
        let result = wait();
        let lease = if shared {
            let permit = self.compiler().bootstrap_jobserver().unwrap().acquire()?;
            CpuLease::Shared { _permit: permit }
        } else {
            CpuLease::Local(self.compiler().acquire_slot())
        };
        *self.permit.lease.lock().unwrap() = Some(lease);
        self.bind()?;
        Ok(result)
    }
}
impl Processor {
    fn mask(self) -> io::Result<usize> {
        1usize.checked_shl(self.number.into()).ok_or_else(||
            io::Error::new(io::ErrorKind::InvalidInput, "处理器编号超出Windows组掩码宽度"))
    }
}
#[cfg(windows)]
fn bind(processor: Processor) -> io::Result<()> {
    use std::{ffi::c_void, ptr::null_mut};
    #[repr(C)]
    struct Affinity { mask: usize, group: u16, reserved: [u16; 3] }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadGroupAffinity(thread: *mut c_void, affinity: *const Affinity,
            previous: *mut Affinity) -> i32;
    }
    let affinity = Affinity { mask: processor.mask()?, group: processor.group, reserved: [0; 3] };
    if unsafe { SetThreadGroupAffinity(GetCurrentThread(), &affinity, null_mut()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(not(windows))]
fn bind(_: Processor) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "显式编译CPU选择仅实现Windows处理器组"))
}
