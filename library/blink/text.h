module blink.text;

dataset<u8> decimal_u64(u64 value) on cpu;
dataset<u8> decimal_i64(i64 value) on cpu;
result<dataset<u8>,u32> hexadecimal_bytes(read dataset<u8> data) on cpu;
