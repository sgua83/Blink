module blink.windows.path;
import blink.windows.system;
import blink.windows.error;
import blink.windows.path_core;
import blink.sequence;

const handle INVALID_HANDLE_VALUE=unsafe_handle(18446744073709551615u64);

extern "C" handle CreateFileW(address path,u32 access,u32 sharing,address security,
    u32 creation,u32 flags,handle template_file) on cpu from "KERNEL32.dll";

result<bool,ExistsError> try_exists(address path) on cpu {
    handle file=CreateFileW(path,0,7,address(),3,33554432,handle());
    if (file == INVALID_HANDLE_VALUE) {
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

extern "C" u32 GetFinalPathNameByHandleW(handle file,address output,u32 capacity,
    u32 flags) on cpu from "KERNEL32.dll";

result<dataset<u16>,CanonicalQueryError> final_path(handle file) on cpu {
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

result<dataset<u16>,AbsoluteError> terminated_name(dataset<u16> units) on cpu {
    match (push(units,u16(0))) {
        Ok(terminated) { return Ok(terminated); }
        Err(failure) { return Err(AbsoluteError::LengthOverflow()); }
    }
}

result<dataset<u16>,AbsoluteError> native_name(address path,bool prefer_verbatim) on cpu {
    match (absolute(path)) {
        Ok(full) {
            // Legacy directory paths reserve 12 units of MAX_PATH, including NUL.
            if (!prefer_verbatim && full.length < 247) { return terminated_name(full); }
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
            if (!prefix) { return terminated_name(full); }
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
            return terminated_name(output);
        }
        Err(error) { return Err(error); }
    }
}

result<handle,CanonicalError> canonical_open_path(address path) on cpu {
    handle file=CreateFileW(path,0,7,address(),3,33554432,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(CanonicalError::Open(GetLastError()));
    }
    return Ok(file);
}

result<handle,CanonicalError> canonical_open(address path) on cpu {
    if (verbatim(path) || canonical_nt_prefix(path) || unsafe_position<u16>(path) == u16(0)) {
        return canonical_open_path(path);
    }
    match (native_name(path,true)) {
        Ok(name) { return canonical_open_path(address_of(name[0])); }
        Err(error) { return Err(CanonicalError::Input(error)); }
    }
}
result<dataset<u16>,CanonicalError> canonicalize(address path) on cpu {
    handle file;
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

blink.windows.path_core::UnitSpan unit_span(read dataset<u16> path) on cpu {
    address data=address();
    if (path.length > 0) { data=address_of(path[0]); }
    return blink.windows.path_core::UnitSpan(data,u64(path.length));
}

result<dataset<u16>,ExtensionError> with_extension(
    read dataset<u16> path,read dataset<u16> extension) on cpu {
    for (index in 0..extension.length) {
        if (blink.windows.path_core::lexical_separator(extension[index],false)) {
            return Err(ExtensionError::Separator(index));
        }
    }
    blink.windows.path_core::UnitSpan input=unit_span(path);
    blink.windows.path_core::UnitSpan tail=unit_span(extension);
    blink.windows.path_core::ExtensionPlan plan=blink.windows.path_core::extension_prepare(input,u64(extension.length));
    if (plan.length > 4294967295u64) { return Err(ExtensionError::LengthOverflow()); }
    dataset<u16> output=dataset<u16>(u32(plan.length));
    address data=address();
    if (output.length > 0) { data=address_of(output[0]); }
    blink.windows.path_core::extension_encode(input,tail,plan,data);
    return Ok(output);
}

result<dataset<u16>,JoinError> join(read dataset<u16> base,read dataset<u16> path) on cpu {
    blink.windows.path_core::UnitSpan left=unit_span(base);
    blink.windows.path_core::UnitSpan right=unit_span(path);
    u64 capacity=blink.windows.path_core::join_scratch(left,right);
    if (capacity > 4294967295u64) { return Err(JoinError::LengthOverflow); }
    dataset<blink.windows.path_core::JoinComponent> scratch=dataset<blink.windows.path_core::JoinComponent>(u32(capacity));
    address parts=address();
    if (scratch.length > 0) { parts=address_of(scratch[0]); }
    blink.windows.path_core::JoinPlan plan=blink.windows.path_core::join_prepare(left,right,parts);
    if (plan.length > 4294967295u64) { return Err(JoinError::LengthOverflow); }
    dataset<u16> output=dataset<u16>(u32(plan.length));
    address data=address();
    if (output.length > 0) { data=address_of(output[0]); }
    blink.windows.path_core::join_encode(left,right,parts,plan,data);
    return Ok(output);
}

extern "C" i32 CreateDirectoryW(address path,address security) on cpu from "KERNEL32.dll";
extern "C" i32 GetFileInformationByHandleEx(handle file,u32 kind,address data,
    u32 bytes) on cpu from "KERNEL32.dll";

result<bool,DirectoryError> directory_target(address path) on cpu {
    handle file=CreateFileW(path,0,7,address(),3,33554432,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(DirectoryError::Open(GetLastError()));
    }
    dataset<u64> info=dataset<u64>(3);
    i32 queried=GetFileInformationByHandleEx(file,1,address_of(info[0]),24);
    u32 query_error=0;
    if (queried == 0) { query_error=GetLastError(); }
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    if (queried == 0) {
        if (closed == 0) {
            return Err(DirectoryError::QueryAndClose(
                DirectoryQueryCloseError(query_error,DirectoryCloseError(file,close_error))));
        }
        return Err(DirectoryError::Query(query_error));
    }
    if (closed == 0) { return Err(DirectoryError::Close(DirectoryCloseError(file,close_error))); }
    return Ok(((info[2] >> 40) & 255u64) != 0u64);
}

result<void,DirectoryError> directory_create(address path) on cpu {
    if (CreateDirectoryW(path,address()) != 0) { return Ok(); }
    u32 code=GetLastError();
    if (code == 183) {
        if (try(directory_target(path))) { return Ok(); }
    }
    return Err(DirectoryError::Create(code));
}

result<void,DirectoryError> directory_component(address path) on cpu {
    if (verbatim(path) || canonical_nt_prefix(path)) { return directory_create(path); }
    match (native_name(path,false)) {
        Ok(name) { return directory_create(address_of(name[0])); }
        Err(error) { return Err(DirectoryError::Input(error)); }
    }
}

result<void,DirectoryError> create_dir_all(address path) on cpu {
    if (unsafe_position<u16>(path) == u16(0)) { return Ok(); }
    dataset<u16> units;
    bool nt=canonical_nt_prefix(path);
    match (copy_verbatim(path)) {
        Ok(raw) { units=raw; }
        Err(error) { return Err(DirectoryError::Input(error)); }
    }
    u32 length=units.length;
    blink.windows.path_core::UnitSpan input=unit_span(units);
    blink.windows.path_core::LexicalPrefix prefix=blink.windows.path_core::lexical_prefix(input,nt);
    u32 body=u32(prefix.end);
    if (body < units.length) {
        if (blink.windows.path_core::lexical_separator(units[body],prefix.literal)) { body+=1; }
    }
    if (body == length) {
        if (try(directory_target(path))) { return Ok(); }
        return Err(DirectoryError::NotDirectory());
    }
    u64 capacity=blink.windows.path_core::component_capacity(input,prefix.literal);
    dataset<blink.windows.path_core::JoinComponent> parts=dataset<blink.windows.path_core::JoinComponent>(u32(capacity));
    address components=address();
    if (parts.length > 0) { components=address_of(parts[0]); }
    u64 count=blink.windows.path_core::join_components(input,u64(body),prefix.literal,prefix.literal,components,0u64);
    if (count == 0u64) { return directory_component(path); }
    parts=resize(parts,u32(count),blink.windows.path_core::JoinComponent(false,0u64,0u64,0u64));
    dataset<u16> terminated;
    match (terminated_name(units)) {
        Ok(prepared) { terminated=prepared; }
        Err(error) { return Err(DirectoryError::Input(error)); }
    }
    u32 next=0;
    for (end in body..terminated.length) {
        if (next < parts.length) {
            if (u64(end) == parts[next].start+parts[next].length) {
                u16 saved=terminated[end];
                terminated[end]=u16(0);
                try(directory_component(address_of(terminated[0])));
                terminated[end]=saved;
                next+=1;
            }
        }
    }
    return Ok();
}
