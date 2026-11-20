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

result<dataset<u16>,CanonicalQueryError> final_path(handle file) on cpu {
    CanonicalName needed=canonical_name(file,address(),0);
    u32 capacity=needed.units;
    if (capacity == 0) { return Err(CanonicalQueryError::System(needed.status)); }
    dataset<u16> output=dataset<u16>(capacity);
    CanonicalName name=canonical_name(file,address_of(output[0]),capacity);
    if (name.units == 0) { return Err(CanonicalQueryError::System(name.status)); }
    if (name.units >= capacity) { return Err(CanonicalQueryError::BufferTooSmall(name.units)); }
    return Ok(resize(output,name.units,u16(0)));
}

result<dataset<u16>,CanonicalError> canonicalize(address path) on cpu {
    usize units=0u64;
    while (unsafe_position<u16>(unsafe_address(address_bits(path)+units*2u64)) != u16(0)) {
        units+=1u64;
    }
    CanonicalSource source=normalized_canonical_source(path,units);
    if (source.status != 0) {
        if (source.input) { return Err(CanonicalError::Input(AbsoluteError::System(source.status))); }
        return Err(CanonicalError::Open(source.status));
    }
    result<dataset<u16>,CanonicalQueryError> queried=final_path(source.file);
    u32 close_error=close_canonical_source(source.file);
    match (queried) {
        Ok(full) {
            if (close_error != 0) {
                return Err(CanonicalError::Close(CanonicalCloseError(source.file,close_error)));
            }
            return Ok(full);
        }
        Err(error) {
            if (close_error != 0) {
                return Err(CanonicalError::QueryAndClose(
                    CanonicalQueryCloseError(error,CanonicalCloseError(source.file,close_error))));
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

result<void,DirectoryError> create_dir_all(address path) on cpu {
    u64 units=0u64;
    while (unsafe_position<u16>(unsafe_address(address_bits(path)+units*2u64)) != u16(0)) {
        units+=1u64;
    }
    return directory_tree(path,units,false);
}
