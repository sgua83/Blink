use super::SourceRoute;
use crate::{ast::Result, build::Declaration, compiler::{Worker, diagnostic}};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    sync::{Arc, Mutex, OnceLock, TryLockError},
};

#[derive(Default)]
pub(crate) struct EntryChecks {
    jobs: Mutex<BTreeMap<(String, u32, bool), Arc<Check>>>,
}

pub(super) type SharedInputs = Arc<BTreeSet<Declaration>>;

#[derive(Default)]
struct Check {
    producer: Mutex<()>,
    result: OnceLock<Result<SharedInputs>>,
}

impl EntryChecks {
    pub(super) fn run(
        &self,
        name: String,
        route: SourceRoute,
        cpu_signature: bool,
        worker: &Worker,
        source: &Path,
        check: impl FnOnce() -> Result<BTreeSet<Declaration>>,
    ) -> Result<SharedInputs> {
        let job = Arc::clone(self.jobs.lock().unwrap()
            .entry((name, route as u32, cpu_signature)).or_default());
        if let Some(result) = job.result.get() {
            return result.clone();
        }
        let interrupted = || io::Error::other("普通实例检查的生产工作已中止");
        match job.producer.try_lock() {
            Ok(_producer) => job.result.get_or_init(|| check().map(Arc::new)).clone(),
            Err(TryLockError::WouldBlock) => worker.wait_for_work(|| {
                let _completed = job.producer.lock().map_err(|_| interrupted())?;
                Ok::<_, io::Error>(job.result.get().unwrap().clone())
            }).and_then(|result| result).map_err(|error| diagnostic(source, error))?,
            Err(TryLockError::Poisoned(_)) => Err(diagnostic(source, interrupted())),
        }
    }
}
