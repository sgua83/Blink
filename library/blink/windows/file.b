module blink.windows.file;
import blink.windows.error;

result<dataset<u8>,ReadError> read_bytes(address file) on cpu {
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

result<dataset<u8>,SourceError> read_utf8(address path) on cpu {
    address file=CreateFileW(path,2147483648,7,address(),3,128,address());
    if (address_bits(file) == 18446744073709551615u64) {
        return Err(SourceError::Open(GetLastError()));
    }
    result<dataset<u8>,ReadError> contents=read_bytes(file);
    i32 closed=CloseHandle(file);
    u32 close_error=0;
    if (closed == 0) { close_error=GetLastError(); }
    match (contents) {
        Ok(data) {
            if (closed == 0) { return Err(SourceError::Close(CloseError(file,close_error))); }
            match (scalar_length_utf8(data)) {
                Ok(count) { return Ok(data); }
                Err(error) { return Err(SourceError::Decode(error)); }
            }
        }
        Err(error) {
            if (closed == 0) {
                return Err(SourceError::ReadAndClose(ReadCloseError(error,CloseError(file,close_error))));
            }
            return Err(SourceError::Read(error));
        }
    }
}