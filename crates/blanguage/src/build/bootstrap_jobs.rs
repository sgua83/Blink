use std::{
    ffi::OsString,
    io,
    os::windows::io::{AsHandle, AsRawHandle, RawHandle},
    path::Path,
    process::{Child, Command},
    sync::mpsc,
    thread,
};
#[path = "../worker_protocol.rs"]
mod worker_protocol;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn WaitForSingleObject(handle: RawHandle, milliseconds: u32) -> u32;
}

pub struct Job {
    pub name: &'static str,
    pub compiler: bool,
    pub arguments: Vec<OsString>,
    pub inputs: Vec<usize>,
}

impl Job {
    fn start(
        &self,
        seed: &Path,
        workers: usize,
        shared: Option<&jobserver::Client>,
    ) -> io::Result<Child> {
        let mut process = if self.compiler {
            let mut command = Command::new(seed.join("bin/bc.exe"));
            command.arg("--jobs").arg(workers.to_string());
            if let Some(cargo) = shared {
                cargo.configure(&mut command);
                command.env(worker_protocol::BOOTSTRAP_JOBSERVER, "1");
            } else {
                command.env_remove(worker_protocol::BOOTSTRAP_JOBSERVER);
            }
            command
        } else {
            Command::new(seed.join("bin/blink.exe"))
        };
        process.args(&self.arguments).spawn()
    }
}

fn shared_workers(seed: &Path) -> io::Result<bool> {
    let help = Command::new(seed.join("bin/bc.exe"))
        .arg("--help")
        .output()?;
    if !help.status.success() {
        return Err(io::Error::other(format!(
            "Blink种子{}读取工作许可能力失败：{}",
            seed.display(),
            help.status
        )));
    }
    Ok(String::from_utf8_lossy(&help.stdout)
        .lines()
        .any(|line| line == worker_protocol::BOOTSTRAP_JOBSERVER_HELP))
}

struct Running {
    child: Child,
    implicit: bool,
    shared: bool,
    _permits: Vec<jobserver::Acquired>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            // A cooperating child can own permits from Cargo. Let it return them.
            if !self.shared {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
    }
}

enum Event {
    Permit(io::Result<jobserver::Acquired>),
    Complete(usize, io::Result<()>),
}

pub fn run(seed: &Path, jobs: &[Job], cargo: jobserver::Client, limit: usize) -> io::Result<()> {
    let shared = (jobs.iter().any(|job| job.compiler) && shared_workers(seed)?).then_some(&cargo);
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
        let mut running: Vec<Option<Running>> = (0..jobs.len()).map(|_| None).collect();
        let mut active = 0;
        let mut completed = 0;

        while completed != jobs.len() {
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
                let wanted = if shared.is_none() && ready.iter().any(|(_, job)| job.compiler) {
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
                let mut surplus = if shared.is_some() {
                    0
                } else {
                    budget - launches
                };
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
                    let process = Running {
                        child: job.start(seed, workers, shared)?,
                        implicit,
                        shared: job.compiler && shared.is_some(),
                        _permits: owned,
                    };
                    let handle = process.child.as_handle().try_clone_to_owned()?;
                    running[index] = Some(process);
                    let send = send.clone();
                    started[index] = true;
                    active += 1;
                    thread::Builder::new().spawn_scoped(scope, move || {
                        // The scheduler retains the child so it can stop its own work on error.
                        let result = if unsafe {
                            WaitForSingleObject(handle.as_raw_handle(), u32::MAX)
                        } == 0
                        {
                            Ok(())
                        } else {
                            Err(io::Error::last_os_error())
                        };
                        let _ = send.send(Event::Complete(index, result));
                    })?;
                }
                permits.clear();
                if launches < ready.len() && !waiting {
                    // Cargo can return a permit while the current compiler is still running.
                    waiting = true;
                    waiter.request_token();
                }
            }
            match receive.recv().map_err(io::Error::other)? {
                Event::Permit(token) => {
                    waiting = false;
                    permits.push(token?);
                }
                Event::Complete(index, result) => {
                    result?;
                    let mut process = running[index].take().expect("owned bootstrap process");
                    let status = process.child.wait()?;
                    if !status.success() {
                        return Err(io::Error::other(format!(
                            "Blink种子{}生成{}失败：{status}",
                            seed.display(),
                            jobs[index].name,
                        )));
                    }
                    active -= 1;
                    implicit_free |= process.implicit;
                    finished[index] = true;
                    completed += 1;
                }
            }
        }
        Ok(())
    })
}
