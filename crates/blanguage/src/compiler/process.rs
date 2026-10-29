//! 原对象编译者创建并持有子工作；CPU选择在其第一条指令前交接。
use super::{Processor, Worker};
use std::{ffi::OsString, io, path::Path, process::Output};
impl Worker {
    pub(crate) fn cargo_output(&self, arguments: &[OsString], directory: &Path, working_directory: &Path) -> io::Result<Output> {
        cargo_output(arguments, directory, working_directory, self.processor())
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
    use std::{
        ffi::{OsStr, c_void}, fs::File, mem::{size_of, zeroed},
        os::windows::{ffi::OsStrExt, io::{AsRawHandle, FromRawHandle, OwnedHandle},
            process::ExitStatusExt},
        process::ExitStatus, ptr::{null, null_mut},
    };
    type Handle = *mut c_void;
    #[repr(C)]
    struct Affinity { mask: usize, group: u16, reserved: [u16; 3] }
    #[repr(C)]
    struct Startup {
        size: u32, reserved: *mut u16, desktop: *mut u16, title: *mut u16,
        x: u32, y: u32, width: u32, height: u32, x_chars: u32, y_chars: u32,
        fill: u32, flags: u32, show: u16, reserved_size: u16, reserved_bytes: *mut u8,
        input: Handle, output: Handle, error: Handle,
    }
    #[repr(C)]
    struct StartupEx { startup: Startup, attributes: *mut c_void }
    #[repr(C)]
    struct Process { process: Handle, thread: Handle, id: u32, thread_id: u32 }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
        fn InitializeProcThreadAttributeList(list: *mut c_void, count: u32, flags: u32,
            bytes: *mut usize) -> i32;
        fn UpdateProcThreadAttribute(list: *mut c_void, flags: u32, attribute: usize,
            value: *mut c_void, bytes: usize, previous: *mut c_void, size: *mut usize) -> i32;
        fn DeleteProcThreadAttributeList(list: *mut c_void);
        fn CreateProcessW(application: *const u16, command: *mut u16,
            process_security: *const c_void, thread_security: *const c_void, inherit: i32,
            flags: u32, environment: *const c_void, directory: *const u16,
            startup: *const StartupEx, process: *mut Process) -> i32;
        fn CreateJobObjectW(security: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, information: *const c_void, bytes: u32) -> i32;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn GetExitCodeProcess(process: Handle, exit: *mut u32) -> i32;
    }
    fn win32(success: i32) -> io::Result<()> {
        if success == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }
    struct Attributes { storage: Vec<usize> }
    impl Attributes {
        fn new(count: u32) -> io::Result<Self> {
            let mut bytes = 0;
            // Win32的空缓冲区查询以ERROR_INSUFFICIENT_BUFFER交付真实所需大小。
            unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes); }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(122) { return Err(error); }
            let mut storage = vec![0; bytes.div_ceil(size_of::<usize>())];
            win32(unsafe { InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(),
                count, 0, &mut bytes) })?;
            Ok(Self { storage })
        }
        fn pointer(&mut self) -> *mut c_void { self.storage.as_mut_ptr().cast() }
        fn set<T>(&mut self, attribute: usize, value: &T) -> io::Result<()> {
            win32(unsafe { UpdateProcThreadAttribute(self.pointer(), 0, attribute,
                (value as *const T).cast_mut().cast(), size_of::<T>(), null_mut(), null_mut()) })
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) { unsafe { DeleteProcThreadAttributeList(self.pointer()); } }
    }
    fn argument(command: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
        if !command.is_empty() { command.push(32); }
        command.push(34);
        let mut slashes = 0;
        for value in argument.encode_wide() {
            match value {
                0 => return Err(io::Error::new(io::ErrorKind::InvalidInput, "进程参数包含NUL")),
                92 => slashes += 1,
                34 => {
                    command.extend(std::iter::repeat_n(92, slashes * 2 + 1));
                    command.push(34); slashes = 0;
                }
                value => {
                    command.extend(std::iter::repeat_n(92, slashes));
                    command.push(value); slashes = 0;
                }
            }
        }
        command.extend(std::iter::repeat_n(92, slashes * 2));
        command.push(34);
        Ok(())
    }
    pub(super) fn output(arguments: &[OsString], directory: &Path, working_directory: &Path, processor: Option<Processor>) -> io::Result<Output> {
        let stdout_path = directory.join("cargo.stdout");
        let stderr_path = directory.join("cargo.stderr");
        let input = File::open("NUL")?;
        let output = File::create(&stdout_path)?;
        let error = File::create(&stderr_path)?;
        let handles = [input.as_raw_handle(), output.as_raw_handle(), error.as_raw_handle()];
        for handle in handles { win32(unsafe { SetHandleInformation(handle, 1, 1) })?; }
        let job = processor.map(|processor| {
            let handle = unsafe { CreateJobObjectW(null(), null()) };
            if handle.is_null() { return Err(io::Error::last_os_error()); }
            let job = unsafe { OwnedHandle::from_raw_handle(handle) };
            let affinity = Affinity { mask: processor.mask()?, group: processor.group, reserved: [0; 3] };
            win32(unsafe { SetInformationJobObject(job.as_raw_handle(), 14,
                (&affinity as *const Affinity).cast(), size_of::<Affinity>() as u32) })?;
            Ok::<_, io::Error>(job)
        }).transpose()?;
        let jobs = job.as_ref().map(|job| [job.as_raw_handle()]);
        let mut attributes = Attributes::new(1 + u32::from(jobs.is_some()))?;
        attributes.set(0x20002, &handles)?;
        if let Some(jobs) = &jobs { attributes.set(0x2000d, jobs)?; }
        let mut startup: StartupEx = unsafe { zeroed() };
        startup.startup.size = size_of::<StartupEx>() as u32;
        startup.startup.flags = 0x100;
        [startup.startup.input, startup.startup.output, startup.startup.error] = handles;
        startup.attributes = attributes.pointer();
        let mut command = Vec::new();
        argument(&mut command, OsStr::new("cargo.exe"))?;
        for value in arguments { argument(&mut command, value)?; }
        command.push(0);
        let working_directory: Vec<u16> = working_directory.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut process: Process = unsafe { zeroed() };
        let flags = 0x80000 | 0x08000000;
        win32(unsafe { CreateProcessW(null(), command.as_mut_ptr(), null(), null(), 1, flags,
            null(), working_directory.as_ptr(), &startup, &mut process) })?;
        let process_handle = unsafe { OwnedHandle::from_raw_handle(process.process) };
        let thread_handle = unsafe { OwnedHandle::from_raw_handle(process.thread) };
        drop(attributes);
        drop((input, output, error));
        drop(thread_handle);
        if unsafe { WaitForSingleObject(process_handle.as_raw_handle(), u32::MAX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut code = 0;
        win32(unsafe { GetExitCodeProcess(process_handle.as_raw_handle(), &mut code) })?;
        Ok(Output { status: ExitStatus::from_raw(code),
            stdout: std::fs::read(stdout_path)?, stderr: std::fs::read(stderr_path)? })
    }
}
