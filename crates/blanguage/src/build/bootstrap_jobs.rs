use std::{ffi::OsString, io, path::Path, process::Command, sync::mpsc, thread};

pub struct Job {
    pub name: &'static str,
    pub compiler: bool,
    pub arguments: Vec<OsString>,
    pub inputs: Vec<usize>,
}

impl Job {
    fn execute(&self, seed: &Path, workers: usize) -> io::Result<()> {
        let mut process = if self.compiler {
            let mut command = Command::new(seed.join("bin/bc.exe"));
            command.arg("--jobs").arg(workers.to_string());
            command
        } else {
            Command::new(seed.join("bin/blink.exe"))
        };
        let status = process.args(&self.arguments).status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "Blink种子{}生成{}失败：{status}",
                seed.display(),
                self.name,
            )));
        }
        Ok(())
    }
}

enum Event {
    Permit(io::Result<jobserver::Acquired>),
    Complete {
        index: usize,
        implicit: bool,
        result: io::Result<()>,
    },
}

pub fn run(seed: &Path, jobs: &[Job], cargo: jobserver::Client, limit: usize) -> io::Result<()> {
    thread::scope(|scope| {
        let (send, receive) = mpsc::channel();
        let permit_send = send.clone();
        let waiter = cargo.clone().into_helper_thread(move |token| {
            let _ = permit_send.send(Event::Permit(token));
        })?;
        let mut waiting = false;
        let mut implicit_free = true;
        let mut permits = Vec::new();
        let mut started = vec![false; jobs.len()];
        let mut finished = vec![false; jobs.len()];
        let mut active = 0;
        let mut completed = 0;
        let mut failure = None;

        while completed != jobs.len() && (failure.is_none() || active != 0) {
            if failure.is_none() {
                let ready: Vec<_> = jobs
                    .iter()
                    .enumerate()
                    .filter(|(index, job)| {
                        !started[*index] && job.inputs.iter().all(|input| finished[*input])
                    })
                    .collect();
                if ready.is_empty() {
                    permits.clear();
                    if active == 0 {
                        return Err(io::Error::other("B引导构建依赖不能继续"));
                    }
                } else {
                    let wanted = if ready.iter().any(|(_, job)| job.compiler) {
                        limit
                    } else {
                        ready.len()
                    };
                    while permits.len() + usize::from(implicit_free) < wanted {
                        let Some(permit) = cargo.try_acquire()? else {
                            break;
                        };
                        permits.push(permit);
                    }

                    let budget = permits.len() + usize::from(implicit_free);
                    let launches = budget.min(ready.len());
                    let compilers = ready[..launches]
                        .iter()
                        .filter(|(_, job)| job.compiler)
                        .count();
                    let mut surplus = budget - launches;
                    let mut remaining_compilers = compilers;
                    for &(index, job) in &ready[..launches] {
                        let mut workers = 1;
                        if job.compiler {
                            let extra = surplus.div_ceil(remaining_compilers);
                            surplus -= extra;
                            remaining_compilers -= 1;
                            workers += extra;
                        }
                        let implicit = implicit_free;
                        implicit_free = false;
                        let owned =
                            permits.split_off(permits.len() - (workers - usize::from(implicit)));
                        let send = send.clone();
                        started[index] = true;
                        active += 1;
                        scope.spawn(move || {
                            let result = job.execute(seed, workers);
                            drop(owned);
                            let _ = send.send(Event::Complete {
                                index,
                                implicit,
                                result,
                            });
                        });
                    }
                    permits.clear();
                    if launches < ready.len() && !waiting {
                        // Cargo can return a permit while the current compiler is still running.
                        waiting = true;
                        waiter.request_token();
                    }
                }
            }
            match receive.recv().map_err(io::Error::other)? {
                Event::Permit(token) => {
                    waiting = false;
                    match token {
                        Ok(token) if failure.is_none() => permits.push(token),
                        Ok(_) => {}
                        Err(error) => {
                            failure.get_or_insert(error);
                        }
                    }
                }
                Event::Complete {
                    index,
                    implicit,
                    result,
                } => {
                    active -= 1;
                    implicit_free |= implicit;
                    match result {
                        Ok(()) => {
                            finished[index] = true;
                            completed += 1;
                        }
                        Err(error) => {
                            failure.get_or_insert(error);
                        }
                    }
                }
            }
        }
        failure.map_or(Ok(()), Err)
    })
}
