module blink.windows.process_extended;
import blink.windows.process;

struct StartupInfoEx {
    StartupInfo startup;
    address attributes;
};

const u32 EXTENDED_STARTUPINFO_PRESENT=0x00080000;
const usize PROC_THREAD_ATTRIBUTE_HANDLE_LIST=usize(0x00020002);

extern "C" i32 InitializeProcThreadAttributeList(
    address attributes,u32 count,u32 flags,inout usize bytes
) on cpu from "KERNEL32.dll";

extern "C" i32 UpdateProcThreadAttribute(
    address attributes,u32 flags,usize attribute,address value,usize bytes,
    address previous_value,address return_size
) on cpu from "KERNEL32.dll";

extern "C" void DeleteProcThreadAttributeList(address attributes) on cpu
    from "KERNEL32.dll";

extern "C" i32 CreateProcessW(
    address application,address command_line,
    address process_attributes,address thread_attributes,
    i32 inherit_handles,u32 creation_flags,address environment,address directory,
    read StartupInfoEx startup,write ProcessInformation info
) on cpu ensures initialized(info,returned != 0)
    from "KERNEL32.dll";
