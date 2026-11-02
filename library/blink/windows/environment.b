module blink.windows.environment;
import blink.windows.error;

result<dataset<u16>,VariableError> variable(address name) on cpu {
    u32 capacity=GetEnvironmentVariableW(name,address(),0);
    if (capacity == 0) {
        return Err(VariableError::System(GetLastError()));
    }
    dataset<u16> output=dataset<u16>(capacity);
    SetLastError(0);
    u32 written=GetEnvironmentVariableW(name,address_of(output[0]),capacity);
    if (written == 0) {
        u32 code=GetLastError();
        if (code != 0) { return Err(VariableError::System(code)); }
    }
    if (written >= capacity) {
        return Err(VariableError::BufferTooSmall(written));
    }
    return Ok(resize(output,written,u16(0)));
}
