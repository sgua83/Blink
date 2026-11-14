module blink.text_core;

u8 hexadecimal_digit(u32 value) on cpu {
    if (value<10) { return u8(48+value); }
    return u8(87+value);
}

extern "C" void write_hexadecimal(address input,u64 count,address output) on cpu {
    u64 index=0u64;
    while (index<count) {
        u32 value=u32(unsafe_position<u8>(unsafe_address(address_bits(input)+index)));
        unsafe_position<u8>(unsafe_address(address_bits(output)+index*2u64))=hexadecimal_digit(value>>4);
        unsafe_position<u8>(unsafe_address(address_bits(output)+index*2u64+1u64))=hexadecimal_digit(value&15);
        index+=1u64;
    }
}
