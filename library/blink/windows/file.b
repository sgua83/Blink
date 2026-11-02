module blink.windows.file;
import blink.windows.error;

result<dataset<u8>,ReadError> read_bytes(handle file) on cpu {
    dataset<u8> data=dataset<u8>(0);
    while (true) {
        dataset<u8> buffer=dataset<u8>(65536);
        u32 received=0;
        i32 status=ReadFile(file,address_of(buffer[0]),buffer.length,received,address());
        if (status == 0) { return Err(ReadError::System(GetLastError())); }
        if (received == 0) { return Ok(data); }
        let chunk=resize(buffer,received,u8(0));
        match (extend(data,chunk)) {
            Ok(next) { data=next; }
            Err(failure) { return Err(ReadError::Sequence(failure.reason)); }
        }
    }
}

result<dataset<u8>,SourceError> read_binary(address path) on cpu {
    handle file=CreateFileW(path,2147483648,7,address(),3,128,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(SourceError::Open(GetLastError()));
    }
    result<dataset<u8>,ReadError> contents=read_bytes(file);
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    match (contents) {
        Ok(data) {
            if (closed == 0) { return Err(SourceError::Close(CloseError(file,close_error))); }
            return Ok(data);
        }
        Err(error) {
            if (closed == 0) {
                return Err(SourceError::ReadAndClose(ReadCloseError(error,CloseError(file,close_error))));
            }
            return Err(SourceError::Read(error));
        }
    }
}
result<dataset<u8>,SourceError> read_utf8(address path) on cpu {
    match (read_binary(path)) {
        Ok(data) {
            match (scalar_length_utf8(data)) {
                Ok(count) { return Ok(data); }
                Err(error) { return Err(SourceError::Decode(error)); }
            }
        }
        Err(error) { return Err(error); }
    }
}

result<u32,WriteError> write_bytes(handle file,read dataset<u8> data) on cpu {
    u32 completed=0;
    while (completed<data.length) {
        u32 written=0;
        i32 status=WriteFile(file,address_of(data[completed]),data.length-completed,
            written,address());
        if (status == 0) {
            return Err(WriteError::System(WriteSystemError(completed,GetLastError())));
        }
        if (written == 0) { return Err(WriteError::NoProgress(completed)); }
        completed+=written;
    }
    return Ok(completed);
}

result<u32,OutputError> write_file(address path,read dataset<u8> data,u32 creation) on cpu {
    handle file=CreateFileW(path,1073741824,7,address(),creation,128,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(OutputError::Open(GetLastError()));
    }
    result<u32,WriteError> outcome=write_bytes(file,data);
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    match (outcome) {
        Ok(completed) {
            if (closed == 0) { return Err(OutputError::Close(CloseError(file,close_error))); }
            return Ok(completed);
        }
        Err(error) {
            if (closed == 0) {
                return Err(OutputError::WriteAndClose(WriteCloseError(error,CloseError(file,close_error))));
            }
            return Err(OutputError::Write(error));
        }
    }
}

result<u32,OutputError> write_binary(address path,read dataset<u8> data) on cpu {
    return write_file(path,data,2);
}

result<u32,OutputError> create_new_binary(address path,read dataset<u8> data) on cpu {
    return write_file(path,data,1);
}

result<void,FileChangeError> change_file(address path,i32 information_class,
    address information,u32 bytes,u32 flags) on cpu {
    handle file=CreateFileW(path,65536,7,address(),3,flags,handle());
    if (file == INVALID_HANDLE_VALUE) {
        return Err(FileChangeError::Open(GetLastError()));
    }
    i32 applied=SetFileInformationByHandle(file,information_class,information,bytes);
    u32 apply_error=0;
    if (applied == 0) { apply_error=GetLastError(); }
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    if (applied == 0) {
        if (closed == 0) {
            return Err(FileChangeError::ApplyAndClose(FileApplyCloseError(
                apply_error,CloseError(file,close_error))));
        }
        return Err(FileChangeError::Apply(apply_error));
    }
    if (closed == 0) { return Err(FileChangeError::Close(CloseError(file,close_error))); }
    return Ok();
}

result<void,RenameError> rename(address source,read dataset<u16> target) on cpu {
    u32 words=target.length/4+3;
    if (target.length%4>=2) { words+=1; }
    if (words>4294967295/8) { return Err(RenameError::LengthOverflow()); }
    dataset<u64> buffer=dataset<u64>(words);
    for (word in 0..buffer.length) {
        u64 value=u64(0);
        if (word == 0) { value=u64(3); }
        if (word == 2) { value=u64(target.length)*u64(2); }
        if (word>=2) {
            for (lane in 0..4) {
                u32 field=word*4+lane;
                if (field>=10) {
                    u32 index=field-10;
                    if (index<target.length) {
                        u16 unit=target[index];
                        if (unit == u16(0)) { return Err(RenameError::Nul(index)); }
                        value|=u64(unit)<<u64(lane*16);
                    }
                }
            }
        }
        buffer[word]=value;
    }
    match (change_file(source,i32(22),address_of(buffer[0]),words*8,35651584)) {
        Ok() { return Ok(); }
        Err(error) { return Err(RenameError::Change(error)); }
    }
}

struct DispositionInfo { u32 flags; };

result<void,FileChangeError> remove_file(address path) on cpu {
    DispositionInfo info=DispositionInfo(19);
    return change_file(path,i32(21),address_of(info),u32(size_of<DispositionInfo>()),2097152);
}
