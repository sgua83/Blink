module blink.windows.process;
import blink.windows.error;
import blink.windows.process_core;

dataset<u16> command_line() on cpu {
    CommandLineSpan line=blink_process_command_line();
    dataset<u16> output=dataset<u16>(line.length);
    for (index in 0..line.length) {
        output[index]=unsafe_position<u16>(unsafe_address(address_bits(line.data)+usize(index)*2));
    }
    return output;
}

Arguments arguments_from_span(CommandLineSpan line) on cpu {
    CommandLineSizes sizes=blink_process_arguments_size(line);
    dataset<u16> units=dataset<u16>(sizes.units);
    dataset<ArgumentRange> ranges=dataset<ArgumentRange>(sizes.arguments);
    address unit_data=address();
    address range_data=address();
    if (units.length!=0) { unit_data=address_of(units[0]); }
    if (ranges.length!=0) { range_data=address_of(ranges[0]); }
    blink_process_arguments_write(line,sizes,unit_data,range_data);
    return Arguments(units,ranges);
}

Arguments parse_command_line(read dataset<u16> line) on cpu {
    address data=address();
    if (line.length!=0) { data=address_of(line[0]); }
    return arguments_from_span(CommandLineSpan(data,line.length));
}

Arguments arguments() on cpu {
    return arguments_from_span(blink_process_command_line());
}

const u32 PROCESS_MEMORY_COUNTERS_EX_SIZE=80;

extern "C" i32 K32GetProcessMemoryInfo(
    handle process,write ProcessMemoryCountersEx counters,u32 bytes
) on cpu
    ensures initialized(counters,returned != 0 && bytes == PROCESS_MEMORY_COUNTERS_EX_SIZE)
    from "KERNEL32.dll";

result<ProcessMemoryCountersEx,u32> process_memory_counters(handle process) on cpu {
    ProcessMemoryCountersEx counters;
    if (K32GetProcessMemoryInfo(process,counters,PROCESS_MEMORY_COUNTERS_EX_SIZE) == 0) {
        return Err(GetLastError());
    }
    return Ok(counters);
}
