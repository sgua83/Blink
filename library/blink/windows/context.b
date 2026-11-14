module blink.windows.context;
import blink.windows.error;
import blink.windows.time;

result<ContextSnapshot,ContextError> capture_thread(handle thread,u32 flags,u64 features) on cpu {
    address context=address();
    u32 required=0;
    if (InitializeContext(address(),flags,context,required)==0) {
        u32 error=GetLastError();
        if (error!=122) { return Err(ContextError::Size(error)); }
    }
    dataset<u8> bytes=dataset<u8>(required);
    address buffer=address();
    if (bytes.length!=0) { buffer=address_of(bytes[0]); }
    if (InitializeContext(buffer,flags,context,required)==0) {
        return Err(ContextError::Initialize(GetLastError()));
    }
    if (SetXStateFeaturesMask(context,features)==0) {
        return Err(ContextError::Features(GetLastError()));
    }
    i64 begin;
    if (QueryPerformanceCounter(begin)==0) { return Err(ContextError::Before(GetLastError())); }
    if (GetThreadContext(thread,context)==0) { return Err(ContextError::Capture(GetLastError())); }
    i64 end;
    if (QueryPerformanceCounter(end)==0) { return Err(ContextError::After(GetLastError())); }
    u64 present;
    if (GetXStateFeaturesMask(context,present)==0) {
        return Err(ContextError::Availability(GetLastError()));
    }
    u32 offset=u32(address_bits(context)-address_bits(buffer));
    return Ok(ContextSnapshot(bytes,offset,present,begin,end));
}
