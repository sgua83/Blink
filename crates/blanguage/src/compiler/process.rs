//! 原对象编译者创建并持有子工作；CPU选择在其第一条指令前交接。
use super::{Processor, Worker};
use std::{ffi::OsString, io, path::Path, process::Output};
impl Worker {
    pub(crate) fn cargo_output(&self, mut arguments: Vec<OsString>, directory: &Path, working_directory: &Path) -> io::Result<Output> {
        let processor = self.processor();
        let compiler = self.compiler();
        let mut helpers = Vec::new();
        if processor.is_none() && compiler.bootstrap_jobserver().is_none() {
            helpers.extend(std::iter::from_fn(|| compiler.try_worker()));
        }
        arguments.insert(1, format!("--jobs={}", 1 + helpers.len()).into());
        let output = cargo_output(&arguments, directory, working_directory, processor);
        drop(helpers);
        output
    }
}
#[cfg(not(windows))]
fn cargo_output(arguments: &[OsString], _: &Path, working_directory: &Path, _: Option<Processor>) -> io::Result<Output> {
    std::process::Command::new("cargo").args(arguments).current_dir(working_directory).output()
}
#[cfg(windows)]
fn cargo_output(arguments: &[OsString], directory: &Path, working_directory: &Path, processor: Option<Processor>) -> io::Result<Output> {
    windows::output(arguments, directory, working_directory, processor)
}
#[cfg(windows)]
mod windows {
    use super::*;
    use std::{fs::File,ffi::c_void,os::windows::{ffi::OsStrExt,io::AsRawHandle,process::ExitStatusExt},process::ExitStatus};
    #[repr(C)]
    struct HostProcessExit { exit_code:u32,status:u32 }
    unsafe extern "C" {
        #[link_name="bootstrap.host_sdk::run_compiler_process"]
        fn run_compiler_process(directory:*const u16,arguments:*const u16,argument_units:usize,
            stdio:*const *mut c_void,mask:usize,group:u16)->HostProcessExit;
    }
    pub(super) fn output(arguments:&[OsString],directory:&Path,working_directory:&Path,processor:Option<Processor>)->io::Result<Output> {
        let stdout_path=directory.join("cargo.stdout");let stderr_path=directory.join("cargo.stderr");
        let input=File::open("NUL")?;let output=File::create(&stdout_path)?;let error=File::create(&stderr_path)?;
        let handles=[input.as_raw_handle(),output.as_raw_handle(),error.as_raw_handle()];
        let mask=processor.map(Processor::mask).transpose()?.unwrap_or(0);let group=processor.map_or(0,|p|p.group);
        let mut units=Vec::new();
        for argument in arguments {
            for unit in argument.encode_wide() {
                if unit==0 {return Err(io::Error::new(io::ErrorKind::InvalidInput,"进程参数包含NUL"));}
                units.push(unit);
            }
            units.push(0);
        }
        let working_directory:Vec<u16>=working_directory.as_os_str().encode_wide().chain(Some(0)).collect();
        let result=unsafe{run_compiler_process(working_directory.as_ptr(),units.as_ptr(),units.len(),handles.as_ptr(),mask,group)};
        drop((input,output,error));
        if result.status!=0{return Err(io::Error::from_raw_os_error(result.status as i32));}
        Ok(Output{status:ExitStatus::from_raw(result.exit_code),stdout:std::fs::read(stdout_path)?,stderr:std::fs::read(stderr_path)?})
    }
}
