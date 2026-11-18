//! 本阶段真实工作借用编译会话的空闲许可，汇合后没有后台消费者。
use crate::compiler::Worker;
#[cfg(feature = "compiler")]
use std::sync::mpsc;
use std::{fmt, io, sync::Mutex, thread};

#[cfg(feature = "compiler")]
enum WorkEvent {
    Exhausted,
    Permit(io::Result<jobserver::Acquired>),
}

struct StopOnUnwind<F: Fn()>(F);
impl<F: Fn()> Drop for StopOnUnwind<F> {
    fn drop(&mut self) {
        if thread::panicking() {
            (self.0)();
        }
    }
}
pub(crate) enum Error<E> {
    Resource(io::Error),
    Work(E),
}
impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(f),
            Self::Work(error) => error.fmt(f),
        }
    }
}
pub(crate) fn try_map<T, C, R, E>(
    current: &Worker,
    jobs: Vec<T>,
    context: impl Fn(&Worker) -> C + Sync,
    compile: impl Fn(&mut C, T) -> Result<R, E> + Sync,
) -> Result<Vec<R>, Error<E>>
where
    T: Send,
    R: Send,
    E: Send,
{
    let count = jobs.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    let queue = Mutex::new(Some(jobs.into_iter().enumerate()));
    #[cfg(feature = "compiler")]
    let shared = current
        .compiler()
        .bootstrap_jobserver()
        .filter(|_| count > 1)
        .map(|cargo| (cargo.clone(), mpsc::channel()));
    #[cfg(feature = "compiler")]
    let notify = shared.as_ref().map(|(_, (send, _))| send.clone());
    let exhausted = || {
        #[cfg(feature = "compiler")]
        if let Some(send) = &notify {
            let _ = send.send(WorkEvent::Exhausted);
        }
    };
    let stop = || {
        let pending = queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if pending.is_some() {
            exhausted();
        }
    };
    let worker = |current: &Worker| {
        let _unwind = StopOnUnwind(&stop);
        let mut context = context(current);
        let mut output = Vec::new();
        loop {
            let next = {
                let mut queue = queue.lock().unwrap();
                let next = queue.as_mut().and_then(Iterator::next);
                if queue.as_ref().is_some_and(|jobs| jobs.len() == 0) {
                    queue.take();
                    exhausted();
                }
                next
            };
            let Some((index, job)) = next else {
                break;
            };
            let result = compile(&mut context, job);
            // All unclaimed jobs follow this failure in the original ordering.
            if result.is_err() {
                stop();
            }
            output.push((index, result));
        }
        output
    };
    let mut output = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 1..count {
            let Some(helper) = current.compiler().try_worker() else {
                break;
            };
            let worker = &worker;
            let stop = &stop;
            let handle = thread::Builder::new()
                .spawn_scoped(scope, move || {
                    if let Err(error) = helper.bind() {
                        stop();
                        return Err(error);
                    }
                    Ok(worker(&helper))
                })
                .map_err(|error| {
                    stop();
                    Error::Resource(error)
                })?;
            handles.push(handle);
        }
        #[cfg(feature = "compiler")]
        let dispatcher = shared
            .map(|(cargo, (send, receive))| {
                let worker = &worker;
                let queue = &queue;
                let stop = &stop;
                thread::Builder::new()
                    .spawn_scoped(scope, move || {
                        let _unwind = StopOnUnwind(stop);
                        let result = (|| {
                            let waiter = cargo.into_helper_thread(move |permit| {
                                let _ = send.send(WorkEvent::Permit(permit));
                            })?;
                            let mut helpers = Vec::new();
                            while queue.lock().unwrap().is_some() {
                                waiter.request_token();
                                let WorkEvent::Permit(permit) =
                                    receive.recv().map_err(io::Error::other)?
                                else {
                                    break;
                                };
                                let permit = permit?;
                                if queue.lock().unwrap().is_none() {
                                    break;
                                }
                                let executing = current.compiler().shared_worker(permit);
                                helpers.push(
                                    thread::Builder::new()
                                        .spawn_scoped(scope, move || worker(&executing))?,
                                );
                            }
                            drop(waiter);
                            drop(receive);
                            Ok::<_, io::Error>(
                                helpers
                                    .into_iter()
                                    .flat_map(|handle| {
                                        handle.join().unwrap_or_else(|error| {
                                            std::panic::resume_unwind(error)
                                        })
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })();
                        if result.is_err() {
                            stop();
                        }
                        result
                    })
                    .map_err(|error| {
                        stop();
                        Error::Resource(error)
                    })
            })
            .transpose()?;
        let mut output = worker(current);
        for handle in handles {
            output.extend(
                handle
                    .join()
                    .unwrap_or_else(|error| std::panic::resume_unwind(error))
                    .map_err(Error::Resource)?,
            );
        }
        #[cfg(feature = "compiler")]
        if let Some(dispatcher) = dispatcher {
            output.extend(
                dispatcher
                    .join()
                    .unwrap_or_else(|error| std::panic::resume_unwind(error))
                    .map_err(Error::Resource)?,
            );
        }
        Ok::<_, Error<E>>(output)
    })?;
    output.sort_unstable_by_key(|(index, _)| *index);
    output
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Result<_, _>>()
        .map_err(Error::Work)
}
