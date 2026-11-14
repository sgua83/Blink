module blink.windows.process;
import blink.windows.error;

extern "C" address GetCommandLineW() on cpu from "KERNEL32.dll";

dataset<u16> command_line() on cpu {
    usize text=address_bits(GetCommandLineW());
    u32 length=0;
    while (unsafe_position<u16>(unsafe_address(text+usize(length)*2)) != u16(0)) {
        length+=1;
    }
    dataset<u16> output=dataset<u16>(length);
    for (index in 0..length) {
        output[index]=unsafe_position<u16>(unsafe_address(text+usize(index)*2));
    }
    return output;
}

struct CommandLineState {
    u32 next;
    bool quoted;
    u32 slashes;
    bool quote_pending;
    u16 unit;
    bool ended;
};
struct CommandLineSizes { u32 units; u32 arguments; };

CommandLineState command_line_unit(read dataset<u16> line,CommandLineState state,bool first) on cpu {
    CommandLineState current=state;
    if (current.slashes != 0) {
        current.slashes-=1; current.unit=u16(92); return current;
    }
    if (current.quote_pending) {
        current.quote_pending=false; current.unit=u16(34); return current;
    }
    while (current.next<line.length) {
        u16 unit=line[current.next];
        current.next+=1;
        if (!current.quoted && (unit == u16(32) || unit == u16(9))) {
            current.ended=true; return current;
        }
        if (unit == u16(34)) {
            if (!first && current.quoted) {
                if (current.next<line.length) {
                    if (line[current.next] == u16(34)) {
                        current.next+=1; current.unit=u16(34); return current;
                    }
                }
            }
            current.quoted=!current.quoted;
        } else if (!first && unit == u16(92)) {
            u32 count=1;
            while (current.next<line.length) {
                if (line[current.next] != u16(92)) { break; }
                current.next+=1; count+=1;
            }
            bool quote=false;
            if (current.next<line.length) { quote=line[current.next] == u16(34); }
            if (quote) {
                u32 pairs=count/2;
                if (pairs != 0) {
                    current.slashes=pairs-1;
                    current.quote_pending=count%2 != 0;
                    if (current.quote_pending) { current.next+=1; }
                    current.unit=u16(92); return current;
                }
                current.next+=1; current.unit=u16(34); return current;
            }
            current.slashes=count-1; current.unit=u16(92); return current;
        } else {
            current.unit=unit; return current;
        }
    }
    current.ended=true;
    return current;
}

CommandLineSizes command_line_sizes(read dataset<u16> line) on cpu {
    u32 cursor=0;
    u32 units=0;
    u32 arguments=0;
    bool first=true;
    while (first || cursor<line.length) {
        if (!first) {
            while (cursor<line.length) {
                u16 unit=line[cursor];
                if (unit != u16(32) && unit != u16(9)) { break; }
                cursor+=1;
            }
            if (cursor == line.length) { break; }
        }
        CommandLineState state=CommandLineState(cursor,false,0,false,u16(0),false);
        while (true) {
            state=command_line_unit(line,state,first);
            if (state.ended) { break; }
            units+=1;
        }
        cursor=state.next;
        arguments+=1;
        first=false;
    }
    return CommandLineSizes(units,arguments);
}

Arguments parse_command_line(read dataset<u16> line) on cpu {
    CommandLineSizes sizes=command_line_sizes(line);
    dataset<u16> units=dataset<u16>(sizes.units);
    dataset<ArgumentRange> ranges=dataset<ArgumentRange>(sizes.arguments);
    u32 cursor=0;
    u32 written=0;
    for (range in ranges) {
        bool first=ordinal(range) == 0;
        if (!first) {
            while (cursor<line.length) {
                u16 unit=line[cursor];
                if (unit != u16(32) && unit != u16(9)) { break; }
                cursor+=1;
            }
        }
        CommandLineState state=CommandLineState(cursor,false,0,false,u16(0),false);
        u32 start=written;
        for (index in written..units.length) {
            state=command_line_unit(line,state,first);
            if (state.ended) { break; }
            units[index]=state.unit;
            written=index+1;
        }
        cursor=state.next;
        range=ArgumentRange(start,written-start);
    }
    return Arguments(units,ranges);
}

Arguments arguments() on cpu {
    dataset<u16> line=command_line();
    return parse_command_line(line);
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
