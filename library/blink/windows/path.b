module blink.windows.path;
import blink.windows.error;

extern "C" address CreateFileW(address path,u32 access,u32 sharing,address security,
    u32 creation,u32 flags,address template_file) on cpu from "KERNEL32.dll";
extern "C" i32 CloseHandle(address handle) on cpu from "KERNEL32.dll";

result<bool,ExistsError> try_exists(address path) on cpu {
    address file=CreateFileW(path,0,7,address(),3,33554432,address());
    if (address_bits(file) == 18446744073709551615u64) {
        u32 code=GetLastError();
        if (code == 2 || code == 3) { return Ok(false); }
        return Err(ExistsError::Query(code));
    }
    if (CloseHandle(file) == 0) {
        return Err(ExistsError::Close(ExistsCloseError(file,GetLastError())));
    }
    return Ok(true);
}
