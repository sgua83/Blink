module blink.windows.process_core;
import blink.windows.process;

struct CommandLineSpan { address data; u32 length; };
struct CommandLineSizes { u32 units; u32 arguments; };

// The returned OS span is read-only, system-managed and not a Frame resource.
// Calls below are synchronous. Keep the input unchanged until writing ends.
extern "C" CommandLineSpan blink_process_command_line() on cpu;
extern "C" CommandLineSizes blink_process_arguments_size(CommandLineSpan line) on cpu;
// sizes must come from the same unchanged input. units holds sizes.units u16
// values; ranges holds sizes.arguments process::ArgumentRange rows. Outputs
// do not overlap the input or each other. All rows are written before return;
// empty units are not accessed. No pointer is retained or released here.
extern "C" void blink_process_arguments_write(CommandLineSpan line,CommandLineSizes sizes,
    address units,address ranges) on cpu;
