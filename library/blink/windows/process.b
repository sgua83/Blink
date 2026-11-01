module blink.windows.process;

extern "C" address GetCommandLineW() on cpu from "KERNEL32.dll";

dataset<u16> command_line() on cpu {
    usize cursor=address_bits(GetCommandLineW());
    dataset<u16> output=dataset<u16>(0);
    while (true) {
        u16 unit=unsafe_position<u16>(unsafe_address(cursor));
        if (unit == u16(0)) { break; }
        u32 size=output.length;
        output=resize(output,size+1,unit);
        cursor+=2;
    }
    return output;
}
