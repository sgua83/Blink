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

extern "C" u32 GetFinalPathNameByHandleW(address file,address output,u32 capacity,
    u32 flags) on cpu from "KERNEL32.dll";

result<dataset<u16>,CanonicalQueryError> final_path(address file) on cpu {
    u32 capacity=GetFinalPathNameByHandleW(file,address(),0,0);
    if (capacity == 0) { return Err(CanonicalQueryError::System(GetLastError())); }
    dataset<u16> output=dataset<u16>(capacity);
    u32 written=GetFinalPathNameByHandleW(file,address_of(output[0]),capacity,0);
    if (written == 0) { return Err(CanonicalQueryError::System(GetLastError())); }
    if (written >= capacity) { return Err(CanonicalQueryError::BufferTooSmall(written)); }
    return Ok(resize(output,written,u16(0)));
}


bool canonical_nt_prefix(address path) on cpu {
    usize start=address_bits(path);
    if (unsafe_position<u16>(path) == u16(92)) {
        if (unsafe_position<u16>(unsafe_address(start+2)) == u16(63)) {
            if (unsafe_position<u16>(unsafe_address(start+4)) == u16(63)) {
                if (unsafe_position<u16>(unsafe_address(start+6)) == u16(92)) {
                    return true;
                }
            }
        }
    }
    return false;
}

result<dataset<u16>,AbsoluteError> canonical_terminated(dataset<u16> units) on cpu {
    match (push(units,u16(0))) {
        Ok(terminated) { return Ok(terminated); }
        Err(failure) { return Err(AbsoluteError::LengthOverflow()); }
    }
}

result<dataset<u16>,AbsoluteError> canonical_native_name(address path) on cpu {
    match (absolute(path)) {
        Ok(full) {
            bool prefix=false;
            bool unc=false;
            u32 skip=0;
            if (full.length >= 3) {
                if (full[1] == u16(58) && full[2] == u16(92)) { prefix=true; }
            }
            if (full.length >= 2) {
                if (full[0] == u16(92) && full[1] == u16(92)) {
                    prefix=true;unc=true;skip=2;
                    if (full.length >= 4) {
                        if (full[2] == u16(63) && full[3] == u16(92)) { prefix=false; }
                        if (full[2] == u16(46) && full[3] == u16(92)) { unc=false;skip=4; }
                    }
                }
            }
            if (!prefix) { return canonical_terminated(full); }
            dataset<u16> output=dataset<u16>(4);
            output[0]=u16(92);output[1]=u16(92);output[2]=u16(63);output[3]=u16(92);
            if (unc) {
                output=resize(output,8,u16(0));
                output[4]=u16(85);output[5]=u16(78);output[6]=u16(67);output[7]=u16(92);
            }
            for (index in skip..full.length) {
                match (push(output,full[index])) {
                    Ok(next) { output=next; }
                    Err(failure) { return Err(AbsoluteError::LengthOverflow()); }
                }
            }
            return canonical_terminated(output);
        }
        Err(error) { return Err(error); }
    }
}

result<address,CanonicalError> canonical_open_path(address path) on cpu {
    address file=CreateFileW(path,0,7,address(),3,33554432,address());
    if (address_bits(file) == 18446744073709551615u64) {
        return Err(CanonicalError::Open(GetLastError()));
    }
    return Ok(file);
}

result<address,CanonicalError> canonical_open(address path) on cpu {
    if (verbatim(path) || canonical_nt_prefix(path) || unsafe_position<u16>(path) == u16(0)) {
        return canonical_open_path(path);
    }
    match (canonical_native_name(path)) {
        Ok(name) { return canonical_open_path(address_of(name[0])); }
        Err(error) { return Err(CanonicalError::Input(error)); }
    }
}
result<dataset<u16>,CanonicalError> canonicalize(address path) on cpu {
    address file;
    match (canonical_open(path)) {
        Ok(opened) { file=opened; }
        Err(error) { return Err(error); }
    }
    result<dataset<u16>,CanonicalQueryError> queried=final_path(file);
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    match (queried) {
        Ok(full) {
            if (closed == 0) {
                return Err(CanonicalError::Close(CanonicalCloseError(file,close_error)));
            }
            return Ok(full);
        }
        Err(error) {
            if (closed == 0) {
                return Err(CanonicalError::QueryAndClose(
                    CanonicalQueryCloseError(error,CanonicalCloseError(file,close_error))));
            }
            return Err(CanonicalError::Query(error));
        }
    }
}
