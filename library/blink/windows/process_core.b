module blink.windows.process_core;

extern "C" address GetCommandLineW() on cpu from "KERNEL32.dll";

u16 command_line_at(CommandLineSpan line,u32 index) on cpu {
    return unsafe_position<u16>(unsafe_address(address_bits(line.data)+usize(index)*2));
}

extern "C" CommandLineSpan blink_process_command_line() on cpu {
    address text=GetCommandLineW();
    u32 length=0;
    while (unsafe_position<u16>(unsafe_address(address_bits(text)+usize(length)*2)) != u16(0)) {
        length+=1;
    }
    return CommandLineSpan(text,length);
}

struct CommandLineState {
    u32 next;
    bool quoted;
    u32 slashes;
    bool quote_pending;
    u16 unit;
    bool ended;
};

CommandLineState command_line_unit(CommandLineSpan line,CommandLineState state,bool first) on cpu {
    CommandLineState current=state;
    if (current.slashes != 0) {
        current.slashes-=1; current.unit=u16(92); return current;
    }
    if (current.quote_pending) {
        current.quote_pending=false; current.unit=u16(34); return current;
    }
    while (current.next<line.length) {
        u16 unit=command_line_at(line,current.next);
        current.next+=1;
        if (!current.quoted && (unit == u16(32) || unit == u16(9))) {
            current.ended=true; return current;
        }
        if (unit == u16(34)) {
            if (!first && current.quoted) {
                if (current.next<line.length) {
                    if (command_line_at(line,current.next) == u16(34)) {
                        current.next+=1; current.unit=u16(34); return current;
                    }
                }
            }
            current.quoted=!current.quoted;
        } else if (!first && unit == u16(92)) {
            u32 count=1;
            while (current.next<line.length) {
                if (command_line_at(line,current.next) != u16(92)) { break; }
                current.next+=1; count+=1;
            }
            bool quote=false;
            if (current.next<line.length) { quote=command_line_at(line,current.next) == u16(34); }
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

extern "C" CommandLineSizes blink_process_arguments_size(CommandLineSpan line) on cpu {
    u32 cursor=0;
    u32 units=0;
    u32 arguments=0;
    bool first=true;
    while (first || cursor<line.length) {
        if (!first) {
            while (cursor<line.length) {
                u16 unit=command_line_at(line,cursor);
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

extern "C" void blink_process_arguments_write(CommandLineSpan line,CommandLineSizes sizes,
    address units,address ranges) on cpu {
    u32 cursor=0;
    u32 written=0;
    for (range_index in 0..sizes.arguments) {
        bool first=range_index == 0;
        if (!first) {
            while (cursor<line.length) {
                u16 unit=command_line_at(line,cursor);
                if (unit != u16(32) && unit != u16(9)) { break; }
                cursor+=1;
            }
        }
        CommandLineState state=CommandLineState(cursor,false,0,false,u16(0),false);
        u32 start=written;
        for (index in written..sizes.units) {
            state=command_line_unit(line,state,first);
            if (state.ended) { break; }
            unsafe_position<u16>(unsafe_address(address_bits(units)+usize(index)*2))=state.unit;
            written=index+1;
        }
        cursor=state.next;
        unsafe_position<ArgumentRange>(unsafe_address(address_bits(ranges)+usize(range_index)*size_of<ArgumentRange>()))=ArgumentRange(start,written-start);
    }
}
