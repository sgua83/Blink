module blink.windows.time;

u64 file_time_ticks(FileTime value) on cpu {
    return (u64(value.high) << 32) | u64(value.low);
}
