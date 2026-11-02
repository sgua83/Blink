module blink.windows.path;
import blink.windows.error;
import blink.sequence;

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

extern "C" u32 GetFullPathNameW(address path,u32 capacity,address buffer,
    address file_part) on cpu from "KERNEL32.dll";

bool verbatim(address path) on cpu {
    usize start=address_bits(path);
    if (unsafe_position<u16>(path) == u16(92)) {
        if (unsafe_position<u16>(unsafe_address(start+2)) == u16(92)) {
            if (unsafe_position<u16>(unsafe_address(start+4)) == u16(63)) {
                if (unsafe_position<u16>(unsafe_address(start+6)) == u16(92)) {
                    return true;
                }
            }
        }
    }
    return false;
}

result<dataset<u16>,AbsoluteError> copy_verbatim(address path) on cpu {
    dataset<u16> output=dataset<u16>(0);
    usize cursor=address_bits(path);
    while (true) {
        u16 unit=unsafe_position<u16>(unsafe_address(cursor));
        if (unit == u16(0)) { return Ok(output); }
        match (push(output,unit)) {
            Ok(next) { output=next; }
            Err(failure) { return Err(AbsoluteError::LengthOverflow()); }
        }
        cursor+=2;
    }
}

result<dataset<u16>,AbsoluteError> absolute(address path) on cpu {
    if (verbatim(path)) { return copy_verbatim(path); }
    u32 capacity=GetFullPathNameW(path,0,address(),address());
    if (capacity == 0) { return Err(AbsoluteError::System(GetLastError())); }
    dataset<u16> output=dataset<u16>(capacity);
    u32 written=GetFullPathNameW(path,capacity,address_of(output[0]),address());
    if (written == 0) { return Err(AbsoluteError::System(GetLastError())); }
    if (written >= capacity) { return Err(AbsoluteError::BufferTooSmall(written)); }
    return Ok(resize(output,written,u16(0)));
}
