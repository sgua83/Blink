module blink.windows.path;
import blink.windows.error;
import blink.sequence;

const handle INVALID_HANDLE_VALUE=unsafe_handle(18446744073709551615u64);

extern "C" handle CreateFileW(address path,u32 access,u32 sharing,address security,
    u32 creation,u32 flags,handle template_file) on cpu from "KERNEL32.dll";
extern "C" i32 CloseHandle(handle handle) on cpu from "KERNEL32.dll";

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

bool lexical_separator(u16 unit,bool literal) on cpu {
    return unit == u16(92) || (!literal && unit == u16(47));
}

bool lexical_drive(u16 unit) on cpu {
    return (unit >= u16(65) && unit <= u16(90)) ||
        (unit >= u16(97) && unit <= u16(122));
}

u32 lexical_component_end(read dataset<u16> path,u32 start,bool literal) on cpu {
    u32 end=start;
    while (end < path.length) {
        if (lexical_separator(path[end],literal)) { return end; }
        end+=1;
    }
    return end;
}

struct LexicalPrefix { u32 end; bool literal; };

LexicalPrefix lexical_prefix(read dataset<u16> path,bool nt_prefix) on cpu {
    if (path.length >= 2) {
        if ((lexical_separator(path[0],false) && lexical_separator(path[1],false)) || nt_prefix) {
            if (path.length >= 4) {
                if ((nt_prefix || (path[0] == u16(92) && path[1] == u16(92))) &&
                    path[2] == u16(63) && path[3] == u16(92)) {
                    if (path.length >= 8) {
                        if (path[4] == u16(85) && path[5] == u16(78) &&
                            path[6] == u16(67) && path[7] == u16(92)) {
                            u32 server=lexical_component_end(path,8,true);
                            u32 share_start=server;
                            if (share_start < path.length) { share_start+=1; }
                            u32 share=lexical_component_end(path,share_start,true);
                            if (share > share_start) { return LexicalPrefix(share,true); }
                            return LexicalPrefix(server,true);
                        }
                    }
                    if (path.length >= 6) {
                        if (lexical_drive(path[4]) && path[5] == u16(58)) {
                            if (path.length == 6) { return LexicalPrefix(6,true); }
                            if (lexical_separator(path[6],false)) { return LexicalPrefix(6,true); }
                        }
                    }
                    return LexicalPrefix(lexical_component_end(path,4,true),true);
                }
                if (path[2] == u16(46) && lexical_separator(path[3],false)) {
                    return LexicalPrefix(lexical_component_end(path,4,false),false);
                }
            }
            u32 server=lexical_component_end(path,2,false);
            if (server > 2 && server < path.length) {
                u32 share_start=server+1;
                u32 share=lexical_component_end(path,share_start,false);
                if (share > share_start) { return LexicalPrefix(share,false); }
            }
            return LexicalPrefix(0,false);
        }
        if (lexical_drive(path[0]) && path[1] == u16(58)) {
            return LexicalPrefix(2,false);
        }
    }
    return LexicalPrefix(0,false);
}

struct ExtensionScan {
    bool found; u32 stem; u32 size; bool only_dots;
    bool has_dot; u32 dot; u32 end;
};

ExtensionScan extension_component(ExtensionScan input,bool literal) on cpu {
    ExtensionScan scan=input;
    if (scan.size > 0) {
        if (scan.only_dots && scan.size <= 2) {
            if (scan.size == 2 || literal) { scan.found=false; }
        }
        else {
            scan.found=true;
            scan.stem=scan.end;
            if (scan.has_dot) { scan.stem=scan.dot; }
        }
    }
    scan.size=0;
    scan.only_dots=true;
    scan.has_dot=false;
    return scan;
}

ExtensionScan extension_name(read dataset<u16> path,LexicalPrefix prefix) on cpu {
    u32 body=prefix.end;
    if (body < path.length) {
        if (lexical_separator(path[body],false)) { body+=1; }
    }
    ExtensionScan scan=ExtensionScan(false,0,0,true,false,0,0);
    for (index in body..path.length) {
        u16 unit=path[index];
        if (lexical_separator(unit,prefix.literal)) {
            scan=extension_component(scan,prefix.literal);
        }
        else {
            if (unit == u16(46)) {
                if (scan.size > 0) { scan.has_dot=true;scan.dot=index; }
            }
            else { scan.only_dots=false; }
            scan.size+=1;
            scan.end=index+1;
        }
    }
    return extension_component(scan,prefix.literal);
}

result<dataset<u16>,ExtensionError> with_extension(
    read dataset<u16> path,read dataset<u16> extension) on cpu {
    for (index in 0..extension.length) {
        if (lexical_separator(extension[index],false)) {
            return Err(ExtensionError::Separator(index));
        }
    }
    ExtensionScan name=extension_name(path,lexical_prefix(path,false));
    dataset<u16> output=dataset<u16>(path.length);
    for (index in 0..path.length) { output[index]=path[index]; }
    if (!name.found) { return Ok(output); }
    output=resize(output,name.stem,u16(0));
    if (extension.length == 0) { return Ok(output); }
    u32 stem=output.length;
    if (stem == 4294967295) { return Err(ExtensionError::LengthOverflow()); }
    u32 begin=stem+1;
    if (extension.length > 4294967295-begin) { return Err(ExtensionError::LengthOverflow()); }
    let full=resize(output,begin+extension.length,u16(46));
    for (index in 0..extension.length) { full[begin+index]=extension[index]; }
    return Ok(full);
}

dataset<u16> join_copy(read dataset<u16> path) on cpu {
    dataset<u16> output=dataset<u16>(path.length);
    for (index in 0..path.length) { output[index]=path[index]; }
    return output;
}

result<dataset<T>,JoinError> join_push<T>(dataset<T> data,T value) on cpu {
    u32 length=data.length;
    if (length == 4294967295) { return Err(JoinError::LengthOverflow); }
    return Ok(resize(data,length+1,value));
}

result<dataset<u16>,JoinError> join_tail(dataset<u16> data,read dataset<u16> tail) on cpu {
    u32 begin=data.length;
    if (tail.length > 4294967295-begin) { return Err(JoinError::LengthOverflow); }
    let output=resize(data,begin+tail.length,u16(0));
    for (index in 0..tail.length) { output[begin+index]=tail[index]; }
    return Ok(output);
}

const u32 JoinPrefix=0;
const u32 JoinRoot=1;
const u32 JoinCurrent=2;
const u32 JoinParent=3;
const u32 JoinNormal=4;
struct JoinComponent { bool from_base; u32 start; u32 length; u32 kind; };

result<dataset<JoinComponent>,JoinError> join_component(
    dataset<JoinComponent> parts,bool from_base,u32 start,u32 length,bool dots) on cpu {
    if (length == 0) { return Ok(parts); }
    u32 kind=JoinNormal;
    if (dots && length == 1) { kind=JoinCurrent; }
    if (dots && length == 2) { kind=JoinParent; }
    if (!from_base) {
        if (kind == JoinCurrent) { return Ok(parts); }
        if (kind == JoinParent) {
            if (parts.length > 0) {
                u32 last=parts.length-1;
                if (parts[last].kind == JoinNormal) {
                    return Ok(resize(parts,last,JoinComponent(false,0,0,JoinNormal)));
                }
            }
            return Ok(parts);
        }
    }
    return join_push(parts,JoinComponent(from_base,start,length,kind));
}

result<dataset<JoinComponent>,JoinError> join_components(
    dataset<JoinComponent> input,read dataset<u16> path,
    u32 body,bool from_base,bool literal) on cpu {
    dataset<JoinComponent> parts=input;
    u32 start=body;
    u32 length=0;
    bool dots=true;
    for (index in body..path.length) {
        u16 unit=path[index];
        if (lexical_separator(unit,literal)) {
            parts=try(join_component(parts,from_base,start,length,dots));
            start=index+1;length=0;dots=true;
        }
        else {
            length+=1;
            if (unit != u16(46)) { dots=false; }
        }
    }
    return join_component(parts,from_base,start,length,dots);
}

result<dataset<u16>,JoinError> join_component_units(
    dataset<u16> input,read dataset<u16> path,JoinComponent component) on cpu {
    dataset<u16> output=input;
    for (index in component.start..path.length) {
        if (index-component.start >= component.length) { break; }
        output=try(join_push(output,path[index]));
    }
    return Ok(output);
}

result<dataset<u16>,JoinError> join_verbatim(
    read dataset<u16> base,read dataset<u16> path,LexicalPrefix prefix) on cpu {
    dataset<JoinComponent> parts=dataset<JoinComponent>(1);
    parts[0]=JoinComponent(true,0,prefix.end,JoinPrefix);
    u32 body=prefix.end;
    if (body < base.length) {
        if (lexical_separator(base[body],false)) {
            parts=try(join_push(parts,JoinComponent(true,body,1,JoinRoot)));
            body+=1;
        }
    }
    parts=try(join_components(parts,base,body,true,true));
    u32 right_body=0;
    if (path.length > 0) {
        if (lexical_separator(path[0],false)) {
            parts=resize(parts,1,JoinComponent(true,0,prefix.end,JoinPrefix));
            parts=try(join_push(parts,JoinComponent(false,0,1,JoinRoot)));
            right_body=1;
        }
    }
    parts=try(join_components(parts,path,right_body,false,false));
    dataset<u16> output=dataset<u16>(0);
    bool separator=false;
    for (index in 0..parts.length) {
        JoinComponent component=parts[index];
        if (separator && component.kind != JoinRoot) {
            output=try(join_push(output,u16(92)));
        }
        if (component.kind == JoinRoot) {
            output=try(join_push(output,u16(92)));
            separator=false;
        }
        else {
            if (component.from_base) {
                output=try(join_component_units(output,base,component));
            }
            else { output=try(join_component_units(output,path,component)); }
            separator=true;
        }
    }
    return Ok(output);
}

result<dataset<u16>,JoinError> join(read dataset<u16> base,read dataset<u16> path) on cpu {
    LexicalPrefix right=lexical_prefix(path,false);
    if (right.end > 0) { return Ok(join_copy(path)); }
    LexicalPrefix left=lexical_prefix(base,false);
    if (left.literal && path.length > 0) { return join_verbatim(base,path,left); }
    dataset<u16> output=join_copy(base);
    bool rooted=false;
    if (path.length > 0) { rooted=lexical_separator(path[0],false); }
    if (rooted) { output=resize(output,left.end,u16(0)); }
    else {
        bool separator=false;
        if (base.length > 0) { separator=!lexical_separator(base[base.length-1],false); }
        if (left.end == 2 && base.length == 2) { separator=false; }
        if (separator) { output=try(join_push(output,u16(92))); }
    }
    return join_tail(output,path);
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
    LexicalPrefix prefix=lexical_prefix(units,nt);
    u32 body=prefix.end;
    if (body < units.length) {
        if (lexical_separator(units[body],prefix.literal)) { body+=1; }
    }
    if (body == length) {
        if (try(directory_target(path))) { return Ok(); }
        return Err(DirectoryError::NotDirectory());
    }
    dataset<JoinComponent> parts;
    match (join_components(dataset<JoinComponent>(0),units,body,prefix.literal,prefix.literal)) {
        Ok(parsed) { parts=parsed; }
        Err(error) { return Err(DirectoryError::Input(AbsoluteError::LengthOverflow())); }
    }
    if (parts.length == 0) { return directory_component(path); }
    dataset<u16> terminated;
    match (terminated_name(units)) {
        Ok(prepared) { terminated=prepared; }
        Err(error) { return Err(DirectoryError::Input(error)); }
    }
    u32 next=0;
    for (end in body..terminated.length) {
        if (next < parts.length) {
            if (end == parts[next].start+parts[next].length) {
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
