module blink.windows.process;

struct ArgumentRange { u32 start; u32 length; };
struct Arguments { dataset<u16> units; dataset<ArgumentRange> ranges; };

dataset<u16> command_line() on cpu;
Arguments parse_command_line(read dataset<u16> line) on cpu;
Arguments arguments() on cpu;
