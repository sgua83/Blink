//! 本阶段真实工作借用编译会话的空闲许可，汇合后没有后台消费者。
use std::{fmt, io, sync::Mutex, thread};
use crate::compiler::Worker;
pub(crate) enum Error<E> { Resource(io::Error), Work(E) }
impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Resource(error) => error.fmt(f), Self::Work(error) => error.fmt(f) }
    }
}
pub(crate) fn try_map<T, C, R, E>(
    current: &Worker, jobs: Vec<T>,
    context: impl Fn(&Worker) -> C + Sync,
    compile: impl Fn(&mut C, T) -> Result<R, E> + Sync,
) -> Result<Vec<R>, Error<E>>
where T: Send, R: Send, E: Send {
    let count = jobs.len();
    if count == 0 { return Ok(Vec::new()); }
    let queue = Mutex::new((jobs.into_iter().enumerate(), count));
    let worker = |current: &Worker| {
        let mut context = context(current);
        let mut output = Vec::new();
        loop {
            let (next, end) = {
                let mut queue = queue.lock().unwrap();
                (queue.0.next(), queue.1)
            };
            let Some((index, job)) = next else { break; };
            if index >= end { break; }
            let result = compile(&mut context, job);
            if result.is_err() {
                let mut queue = queue.lock().unwrap();
                queue.1 = queue.1.min(index);
            }
            output.push((index, result));
        }
        output
    };
    let mut output = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 1..count {
            let Some(helper) = current.compiler().try_worker() else { break; };
            let queue = &queue;
            let worker = &worker;
            let executing = helper.clone();
            let handle = thread::Builder::new().spawn_scoped(scope, move || {
                if let Err(error) = executing.bind() {
                    queue.lock().unwrap().1 = 0;
                    return Err(error);
                }
                Ok(worker(&executing))
            }).map_err(|error| {
                queue.lock().unwrap().1 = 0;
                Error::Resource(error)
            })?;
            handles.push((handle, helper));
        }
        let mut output = worker(current);
        for (handle, _permit) in handles {
            output.extend(handle.join().unwrap_or_else(|error| std::panic::resume_unwind(error))
                .map_err(Error::Resource)?);
        }
        Ok::<_, Error<E>>(output)
    })?;
    output.sort_unstable_by_key(|(index, _)| *index);
    output.into_iter().map(|(_, result)| result).collect::<Result<_, _>>().map_err(Error::Work)
}
