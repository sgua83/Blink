module bootstrap.frame_layout;

struct SavedRegister {
    u32 offset;
    u16 register;
    u16 bank;
};

// The caller owns one output row per used register bit. GPRs precede XMMs.
extern "C" u64 saved_registers(u64 data_end, u32 gpr_mask, u32 xmm_mask, address saves) on cpu;

enum TileFrameBank:u32 { Shared=0; Private=1; };
struct TileFrameSlot { address key; u64 value; };
struct TileFrameHost {
    cfn<TileFrameSlot,address,TileFrameBank> next;
    cfn<void,address,TileFrameBank,address,u32> place;
};

// Each borrowed key identifies a shared word slot or a private lane width.
// The host writes each byte position into the original frame.
extern "C" u64 tile_frame(u64 data_end, u64 width, bool steps,
    address context, TileFrameHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.place);

// Code and unwind rows both belong to the original emitter. Offsets use
// the image start, including the ordinary function's register-home writes.
struct FrameOutput { address code; address operations; address saved; u64 count; };
struct FramePrologue { u64 push_end; u64 allocate_end; u64 end; u64 probe_field; u64 operations; };
extern "C" FramePrologue function_frame_prologue(u32 frame, bool scratch, u64 start, FrameOutput output) on cpu;

// Committed prefix length, probe displacement and initialized unwind rows.
struct FrameEntry { u64 length; u64 probe_field; u64 operations; u8 prolog; };

struct RangeFrameHost {
    cfn<FrameOutput,address,u64,u64> reserve;
    cfn<void,address,FrameEntry> publish;
};
// Saved registers are borrowed from the original frame through the epilogue.
struct RangePrologue { u64 end; address saved; u64 count; u32 stack_bytes; };
extern "C" RangePrologue range_prologue(u32 frame, u64 count, address context, RangeFrameHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.publish);

// The caller lends 20 + 9 * entry.count bytes at end. Exit rel32 fields point to
// restoration after the success status write. The result is the final image end.
extern "C" u64 range_epilogue(u32 frame, RangePrologue entry, address image, u64 end, u64 exits) on cpu;

// The caller lends 10 + 9 * count bytes at end. Offsets refer to the
// actual restored ranges and return in the original image.
struct FunctionEpilogue { u64 saved_end; u64 frame_end; u64 scratch_end; u64 end; };
extern "C" FunctionEpilogue function_epilogue(u32 frame, u32 scratch, address saved, u64 count,
    address image, u64 end, u64 exits) on cpu;
