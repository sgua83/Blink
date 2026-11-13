module blink.windows.path_core;
import blink.windows.error;
import blink.windows.file;
import blink.windows.system;

u16 unit(UnitSpan path,u64 index) on cpu {
    return unsafe_position<u16>(unsafe_address(address_bits(path.data)+index*2u64));
}
void put(address output,u64 index,u16 value) on cpu {
    unsafe_position<u16>(unsafe_address(address_bits(output)+index*2u64))=value;
}
void copy_units(UnitSpan path,u64 start,u64 length,address output,u64 begin) on cpu {
    u64 index=0u64;
    while (index < length) { put(output,begin+index,unit(path,start+index)); 
        index+=1u64;
    }
}
bool lexical_separator(u16 unit,bool literal) on cpu {
    return unit == u16(92u64) || (!literal && unit == u16(47u64));
}

bool lexical_drive(u16 unit) on cpu {
    return (unit >= u16(65u64) && unit <= u16(90u64)) ||
        (unit >= u16(97u64) && unit <= u16(122u64));
}

u64 lexical_component_end(UnitSpan path,u64 start,bool literal) on cpu {
    u64 end=start;
    while (end < path.length) {
        if (lexical_separator(unit(path,end),literal)) { return end; }
        end+=1u64;
    }
    return end;
}

LexicalPrefix lexical_prefix(UnitSpan path,bool nt_prefix) on cpu {
    if (path.length >= 2u64) {
        if ((lexical_separator(unit(path,0u64),false) && lexical_separator(unit(path,1u64),false)) || nt_prefix) {
            if (path.length >= 4u64) {
                if ((nt_prefix || (unit(path,0u64) == u16(92u64) && unit(path,1u64) == u16(92u64))) &&
                    unit(path,2u64) == u16(63u64) && unit(path,3u64) == u16(92u64)) {
                    if (path.length >= 8u64) {
                        if (unit(path,4u64) == u16(85u64) && unit(path,5u64) == u16(78u64) &&
                            unit(path,6u64) == u16(67u64) && unit(path,7u64) == u16(92u64)) {
                            u64 server=lexical_component_end(path,8u64,true);
                            u64 share_start=server;
                            if (share_start < path.length) { share_start+=1u64; }
                            u64 share=lexical_component_end(path,share_start,true);
                            if (share > share_start) { return LexicalPrefix(share,true); }
                            return LexicalPrefix(server,true);
                        }
                    }
                    if (path.length >= 6u64) {
                        if (lexical_drive(unit(path,4u64)) && unit(path,5u64) == u16(58u64)) {
                            if (path.length == 6u64) { return LexicalPrefix(6u64,true); }
                            if (lexical_separator(unit(path,6u64),false)) { return LexicalPrefix(6u64,true); }
                        }
                    }
                    return LexicalPrefix(lexical_component_end(path,4u64,true),true);
                }
                if (unit(path,2u64) == u16(46u64) && lexical_separator(unit(path,3u64),false)) {
                    return LexicalPrefix(lexical_component_end(path,4u64,false),false);
                }
            }
            u64 server=lexical_component_end(path,2u64,false);
            if (server > 2u64 && server < path.length) {
                u64 share_start=server+1u64;
                u64 share=lexical_component_end(path,share_start,false);
                if (share > share_start) { return LexicalPrefix(share,false); }
            }
            return LexicalPrefix(0u64,false);
        }
        if (lexical_drive(unit(path,0u64)) && unit(path,1u64) == u16(58u64)) {
            return LexicalPrefix(2u64,false);
        }
    }
    return LexicalPrefix(0u64,false);
}

ExtensionScan extension_component(ExtensionScan input,bool literal) on cpu {
    ExtensionScan scan=input;
    if (scan.size > 0u64) {
        if (scan.only_dots && scan.size <= 2u64) {
            if (scan.size == 2u64 || literal) { scan.found=false; }
        }
        else {
            scan.found=true;
            scan.start=scan.end-scan.size;
            scan.stem=scan.end;
            if (scan.has_dot) { scan.stem=scan.dot; }
        }
    }
    scan.size=0u64;
    scan.only_dots=true;
    scan.has_dot=false;
    return scan;
}

ExtensionScan extension_name(UnitSpan path,LexicalPrefix prefix) on cpu {
    u64 body=prefix.end;
    if (body < path.length) {
        if (lexical_separator(unit(path,body),false)) { body+=1u64; }
    }
    ExtensionScan scan=ExtensionScan(false,0u64,0u64,0u64,true,false,0u64,0u64);
    u64 index=body;
    while (index < path.length) {
        u16 unit=unit(path,index);
        if (lexical_separator(unit,prefix.literal)) {
            scan=extension_component(scan,prefix.literal);
        }
        else {
            if (unit == u16(46u64)) {
                if (scan.size > 0u64) { scan.has_dot=true;scan.dot=index; }
            }
            else { scan.only_dots=false; }
            scan.size+=1u64;
            scan.end=index+1u64;
        }
    
        index+=1u64;
    }
    return extension_component(scan,prefix.literal);
}


extern "C" UnitSpan file_stem(UnitSpan path) on cpu {
    ExtensionScan name=extension_name(path,lexical_prefix(path,false));
    if (!name.found) { return UnitSpan(address(),0u64); }
    address stem=unsafe_address(address_bits(path.data)+name.start*2u64);
    return UnitSpan(stem,name.stem-name.start);
}

extern "C" ExtensionPlan extension_prepare(UnitSpan path,u64 extension_length) on cpu {
    ExtensionScan name=extension_name(path,lexical_prefix(path,false));
    if (!name.found) { return ExtensionPlan(path.length,path.length,0u64); }
    if (extension_length == 0u64) { return ExtensionPlan(name.stem,name.stem,0u64); }
    return ExtensionPlan(name.stem+1u64+extension_length,name.stem,1u64);
}
extern "C" void extension_encode(UnitSpan path,UnitSpan extension,
    ExtensionPlan plan,address output) on cpu {
    copy_units(path,0u64,plan.copy,output,0u64);
    if (plan.append != 0u64) {
        put(output,plan.copy,u16(46));
        copy_units(extension,0u64,extension.length,output,plan.copy+1u64);
    }
}

const u64 JoinPrefix=0u64;
const u64 JoinRoot=1u64;
const u64 JoinCurrent=2u64;
const u64 JoinParent=3u64;
const u64 JoinNormal=4u64;

JoinComponent part(address parts,u64 index) on cpu {
    return unsafe_position<JoinComponent>(unsafe_address(address_bits(parts)+index*size_of<JoinComponent>()));
}
void set_part(address parts,u64 index,JoinComponent value) on cpu {
    unsafe_position<JoinComponent>(unsafe_address(address_bits(parts)+index*size_of<JoinComponent>()))=value;
}
u64 join_component(address parts,u64 count,bool from_base,u64 start,u64 length,bool dots) on cpu {
    if (length == 0u64) { return count; }
    u64 kind=JoinNormal;
    if (dots && length == 1u64) { kind=JoinCurrent; }
    if (dots && length == 2u64) { kind=JoinParent; }
    if (!from_base) {
        if (kind == JoinCurrent) { return count; }
        if (kind == JoinParent) {
            if (count > 0u64) {
                if (part(parts,count-1u64).kind == JoinNormal) { return count-1u64; }
            }
            return count;
        }
    }
    set_part(parts,count,JoinComponent(from_base,start,length,kind));
    return count+1u64;
}
u64 component_capacity(UnitSpan path,bool literal) on cpu {
    u64 count=0u64;
    bool inside=false;
    u64 index=0u64;
    while (index < path.length) {
        bool separator=lexical_separator(unit(path,index),literal);
        if (!separator && !inside) { count+=1u64; }
        inside=!separator;
    
        index+=1u64;
    }
    return count;
}
u64 join_components(UnitSpan path,u64 body,bool from_base,bool literal,
    address parts,u64 count) on cpu {
    u64 used=count;
    u64 start=body;
    u64 length=0u64;
    bool dots=true;
    u64 index=body;
    while (index < path.length) {
        u16 value=unit(path,index);
        if (lexical_separator(value,literal)) {
            used=join_component(parts,used,from_base,start,length,dots);
            start=index+1u64;length=0u64;dots=true;
        }
        else {
            length+=1u64;
            if (value != u16(46)) { dots=false; }
        }
    
        index+=1u64;
    }
    return join_component(parts,used,from_base,start,length,dots);
}
extern "C" u64 join_scratch(UnitSpan base,UnitSpan path) on cpu {
    LexicalPrefix right=lexical_prefix(path,false);
    if (right.end > 0u64 || path.length == 0u64) { return 0u64; }
    if (!lexical_prefix(base,false).literal) { return 0u64; }
    return component_capacity(base,true)+component_capacity(path,false)+2u64;
}
extern "C" JoinPlan join_prepare(UnitSpan base,UnitSpan path,address scratch) on cpu {
    LexicalPrefix right=lexical_prefix(path,false);
    if (right.end > 0u64) { return JoinPlan(path.length,0u64,0u64,0u64); }
    LexicalPrefix left=lexical_prefix(base,false);
    if (left.literal && path.length > 0u64) {
        set_part(scratch,0u64,JoinComponent(true,0u64,left.end,JoinPrefix));
        u64 count=1u64;
        u64 body=left.end;
        if (body < base.length) {
            if (lexical_separator(unit(base,body),false)) {
                set_part(scratch,count,JoinComponent(true,body,1u64,JoinRoot));
                count+=1u64;body+=1u64;
            }
        }
        count=join_components(base,body,true,true,scratch,count);
        u64 right_body=0u64;
        if (lexical_separator(unit(path,0u64),false)) {
            count=2u64;
            set_part(scratch,1u64,JoinComponent(false,0u64,1u64,JoinRoot));
            right_body=1u64;
        }
        count=join_components(path,right_body,false,false,scratch,count);
        u64 length=0u64;
        bool separator=false;
        u64 index=0u64;
    while (index < count) {
            JoinComponent component=part(scratch,index);
            if (separator && component.kind != JoinRoot) { length+=1u64; }
            if (component.kind == JoinRoot) { length+=1u64;separator=false; }
            else { length+=component.length;separator=true; }
        
        index+=1u64;
    }
        return JoinPlan(length,count,0u64,0u64);
    }
    u64 length=base.length;
    u64 separator=0u64;
    bool rooted=false;
    if (path.length > 0u64) { rooted=lexical_separator(unit(path,0u64),false); }
    if (rooted) { length=left.end; }
    else {
        if (base.length > 0u64) {
            if (!lexical_separator(unit(base,base.length-1u64),false)) { separator=1u64; }
        }
        if (left.end == 2u64 && base.length == 2u64) { separator=0u64; }
    }
    return JoinPlan(length+separator+path.length,0u64,length,separator);
}
extern "C" void join_encode(UnitSpan base,UnitSpan path,address scratch,
    JoinPlan plan,address output) on cpu {
    if (plan.components == 0u64) {
        copy_units(base,0u64,plan.base_length,output,0u64);
        if (plan.separator != 0u64) { put(output,plan.base_length,u16(92)); }
        copy_units(path,0u64,path.length,output,plan.base_length+plan.separator);
        return;
    }
    u64 length=0u64;
    bool separator=false;
    u64 index=0u64;
    while (index < plan.components) {
        JoinComponent component=part(scratch,index);
        if (separator && component.kind != JoinRoot) { put(output,length,u16(92));length+=1u64; }
        if (component.kind == JoinRoot) { put(output,length,u16(92));length+=1u64;separator=false; }
        else {
            UnitSpan input=path;
            if (component.from_base) { input=base; }
            copy_units(input,component.start,component.length,output,length);
            length+=component.length;separator=true;
        }
    
        index+=1u64;
    }
}
extern "C" void module_tail(address name,u64 length,u16 suffix) on cpu {
    UnitSpan input=UnitSpan(name,length);
    u64 index=0u64;
    while (index < length) {
        if (unit(input,index) == u16(46)) { put(name,index,u16(47)); }
    
        index+=1u64;
    }
    put(name,length,u16(46));put(name,length+1u64,suffix);
}

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" i32 CreateDirectoryW(address path,address security) on cpu from "KERNEL32.dll";
extern "C" i32 GetFileInformationByHandleEx(handle file,u32 kind,address data,
    u32 bytes) on cpu from "KERNEL32.dll";

const u32 ERROR_NOT_ENOUGH_MEMORY = 8;
const u32 ERROR_ALREADY_EXISTS = 183;
const u32 ERROR_FILENAME_EXCED_RANGE = 206;

bool nt_namespace(UnitSpan path) on cpu {
    return path.length >= 4u64 && unit(path,0u64) == u16(92) && unit(path,1u64) == u16(63) &&
        unit(path,2u64) == u16(63) && unit(path,3u64) == u16(92);
}

UnitSpan extended_native_path(UnitSpan full) on cpu {
    LexicalPrefix prefix = lexical_prefix(full,false);
    if (prefix.literal || prefix.end == 0u64) { return full; }
    if (prefix.end == 2u64) {
        address start = unsafe_address(address_bits(full.data) - 8u64);
        unsafe_position<array<u16,4>>(start) = array<u16,4>(u16(92),u16(92),u16(63),u16(92));
        return UnitSpan(start,full.length+4u64);
    }
    if (unit(full,2u64) == u16(46) && lexical_separator(unit(full,3u64),false)) {
        put(full.data,2u64,u16(63));
        return full;
    }
    // The original UNC slashes share the last two units of the extended prefix.
    address start = unsafe_address(address_bits(full.data) - 12u64);
    unsafe_position<array<u16,8>>(start) = array<u16,8>(
        u16(92),u16(92),u16(63),u16(92),u16(85),u16(78),u16(67),u16(92));
    return UnitSpan(start,full.length+6u64);
}

extern "C" NativePath absolute_path(address path,usize units) on cpu {
    if (lexical_prefix(UnitSpan(path,units),false).literal) {
        return NativePath(path,address(),units,0);
    }

    // Six leading units can extend a UNC prefix in the same allocation.
    u32 capacity = 32768;
    handle heap = GetProcessHeap();
    address allocation = HeapAlloc(heap,0,(usize(capacity)+6u64)*2u64);
    if (address_bits(allocation) == 0u64) {
        return NativePath(address(),address(),0u64,ERROR_NOT_ENOUGH_MEMORY);
    }
    address full = unsafe_address(address_bits(allocation)+12u64);
    u32 written = GetFullPathNameW(path,capacity,full,address());
    if (written == 0 || written >= capacity) {
        u32 status = written == 0 ? GetLastError() : ERROR_FILENAME_EXCED_RANGE;
        HeapFree(heap,0,allocation);
        return NativePath(address(),address(),0u64,status);
    }
    return NativePath(full,allocation,usize(written),0);
}

NativePath native_path(address path,usize units,bool prefer_verbatim) on cpu {
    UnitSpan input = UnitSpan(path,units);
    LexicalPrefix prefix = lexical_prefix(input,nt_namespace(input));
    bool absolute = prefix.end > 2u64 || (prefix.end == 2u64 && units > 2u64 &&
        lexical_separator(unit(input,2u64),false));
    if (prefix.literal || (prefer_verbatim && absolute && units < 247u64)) {
        return NativePath(path,address(),units,0);
    }

    NativePath resolved = absolute_path(path,units);
    if (resolved.status != 0) { return resolved; }
    if (prefer_verbatim || resolved.units >= 247u64) {
        UnitSpan extended = extended_native_path(UnitSpan(resolved.name,resolved.units));
        resolved.name = extended.data;
        resolved.units = extended.length;
    }
    return resolved;
}

extern "C" void release_native_path(NativePath path) on cpu {
    if (address_bits(path.allocation) != 0u64) { HeapFree(GetProcessHeap(),0,path.allocation); }
}

result<bool,DirectoryError> directory_target(address path) on cpu {
    handle file = CreateFileW(path,0,7,address(),3,33554432,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(DirectoryError::Open(GetLastError()));
    }
    array<u64,3> info = array<u64,3>(0u64,0u64,0u64);
    i32 queried = GetFileInformationByHandleEx(file,1,address_of(info[0]),24);
    u32 query_error = queried == 0 ? GetLastError() : 0;
    i32 closed = CloseHandle(file);
    u32 close_error = closed == 0 ? GetLastError() : 0;
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
    u32 code = GetLastError();
    if (code == ERROR_ALREADY_EXISTS) {
        if (try(directory_target(path))) { return Ok(); }
    }
    return Err(DirectoryError::Create(code));
}

result<void,DirectoryError> directory_component(address path,usize units,bool prefer_verbatim) on cpu {
    NativePath name = native_path(path,units,prefer_verbatim);
    if (name.status != 0) { return Err(DirectoryError::Input(AbsoluteError::System(name.status))); }
    result<void,DirectoryError> outcome = directory_create(name.name);
    release_native_path(name);
    return outcome;
}

result<void,DirectoryError> directory_tree(address path,usize units,bool prefer_verbatim) on cpu {
    if (units == 0u64) { return Ok(); }
    UnitSpan input = UnitSpan(path,units);
    LexicalPrefix prefix = lexical_prefix(input,nt_namespace(input));
    u64 body = prefix.end;
    if (body < units && lexical_separator(unit(input,body),prefix.literal)) { body += 1u64; }
    if (body == units) {
        if (try(directory_target(path))) { return Ok(); }
        return Err(DirectoryError::NotDirectory());
    }

    u64 parts_bytes = component_capacity(input,prefix.literal)*size_of<JoinComponent>();
    handle heap = GetProcessHeap();
    address storage = HeapAlloc(heap,0,parts_bytes+(units+1u64)*2u64);
    if (address_bits(storage) == 0u64) {
        return Err(DirectoryError::Input(AbsoluteError::System(ERROR_NOT_ENOUGH_MEMORY)));
    }
    u64 count = join_components(input,body,prefix.literal,prefix.literal,storage,0u64);
    if (count == 0u64) {
        HeapFree(heap,0,storage);
        return directory_component(path,units,prefer_verbatim);
    }
    address copy = unsafe_address(address_bits(storage)+parts_bytes);
    copy_units(input,0u64,units,copy,0u64);
    put(copy,units,u16(0));

    u64 index = 0u64;
    while (index < count) {
        JoinComponent component = part(storage,index);
        u64 end = component.start+component.length;
        u16 saved = unit(UnitSpan(copy,units+1u64),end);
        put(copy,end,u16(0));
        result<void,DirectoryError> outcome = directory_component(copy,end,prefer_verbatim);
        put(copy,end,saved);
        match (outcome) {
            Ok() { index += 1u64; }
            Err(error) { HeapFree(heap,0,storage); return Err(error); }
        }
    }
    HeapFree(heap,0,storage);
    return Ok();
}

extern "C" u32 create_output_directories(address path,usize units) on cpu {
    match (directory_tree(path,units,true)) {
        Ok() { return 0; }
        Err(error) {
            match (error) {
                Input(cause) {
                    match (cause) {
                        System(code) { return code; }
                        BufferTooSmall(required) { return ERROR_FILENAME_EXCED_RANGE; }
                        LengthOverflow() { return ERROR_FILENAME_EXCED_RANGE; }
                    }
                }
                Create(code) { return code; }
                Close(cause) { return cause.code; }
                // Existing entries that cannot be identified as directories retain the create error.
                Open(code) { return ERROR_ALREADY_EXISTS; }
                Query(code) { return ERROR_ALREADY_EXISTS; }
                QueryAndClose(cause) { return ERROR_ALREADY_EXISTS; }
                NotDirectory() { return ERROR_ALREADY_EXISTS; }
            }
        }
    }
}
