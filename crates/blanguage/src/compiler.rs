//! 调用方持有的本机编译资源；所有活动工作借用同一份执行许可。
use std::{
    io,
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex},
    thread,
};
use crate::parallel::Error;
mod process;

/// Windows处理器组中的一个逻辑处理器；不是跨机器地址。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Processor {
    pub group: u16,
    pub number: u8,
}
#[derive(Clone)]
pub struct Compiler {
    resources: Arc<Resources>,
}
struct Resources {
    workers: usize,
    processors: Option<Vec<Processor>>,
    available: Mutex<Vec<usize>>,
    released: Condvar,
}
struct Permit {
    compiler: Compiler,
    slot: usize,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let resources = &self.compiler.resources;
        resources.available.lock().unwrap().push(self.slot);
        resources.released.notify_one();
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
        Self { resources: Arc::new(Resources {
            workers, processors,
            available: Mutex::new((0..workers).rev().collect()),
            released: Condvar::new(),
        }) }
    }
    pub fn workers(&self) -> usize { self.resources.workers }
    fn worker(&self, slot: usize) -> Worker {
        Worker { permit: Arc::new(Permit { compiler: self.clone(), slot }) }
    }
    pub(crate) fn try_worker(&self) -> Option<Worker> {
        self.resources.available.lock().unwrap().pop().map(|slot| self.worker(slot))
    }
    fn acquire(&self) -> Worker {
        let mut available = self.resources.available.lock().unwrap();
        loop {
            if let Some(slot) = available.pop() { return self.worker(slot); }
            available = self.resources.released.wait(available).unwrap();
        }
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
        self.compiler().resources.processors.as_ref().map(|processors| processors[self.permit.slot])
    }
    pub(crate) fn bind(&self) -> io::Result<()> {
        if let Some(processor) = self.processor() {
            bind(processor)?;
        }
        Ok(())
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
