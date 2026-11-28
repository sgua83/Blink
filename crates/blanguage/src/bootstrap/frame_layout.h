module bootstrap.frame_layout;
import bootstrap.ir_dataflow;
import bootstrap.stage_fusion;

struct ColorLayout { u64 end; u32 gpr; u32 xmm; };
struct SavedLayout { u64 gpr_base; u64 xmm_base; u32 gpr; u32 xmm; };
struct FrameSaveLayout { SavedLayout saved; u64 count; u64 end; };
// Both the prologue and epilogue consume the same masks and aligned starts.
extern "C" FrameSaveLayout frame_save_layout(ColorLayout scalar, ColorLayout packet) on cpu;

// The ordered shared rows become frame byte positions. Private rows receive
// their byte offsets in the same pass, with one plane per 32-bit part.
extern "C" u64 tile_frame(u64 data_end, u64 width, bool steps, Span shared, Span private_slots) on cpu;
u32 tile_private_offset(Span placed, TilePrivateKey key) on cpu;

// Code and unwind rows both belong to the original emitter. Offsets use
// the image start, including the ordinary function's register-home writes.
struct FrameOutput { address code; address operations; SavedLayout saved; u64 count; };
struct FramePrologue { u64 push_end; u64 allocate_end; u64 end; u64 probe_field; u64 operations; };
extern "C" FramePrologue function_frame_prologue(u32 frame, bool scratch, u64 start, FrameOutput output) on cpu;

// Committed prefix length, probe displacement and initialized unwind rows.
struct FrameEntry { u64 length; u64 probe_field; u64 operations; u8 prolog; };

struct RangeFrameHost {
    cfn<FrameOutput,address,u64,u64> reserve;
    cfn<void,address,FrameEntry> publish;
};
struct RangePrologue { u64 end; SavedLayout saved; u64 count; u32 stack_bytes; };
extern "C" RangePrologue range_prologue(u32 frame, u64 count, address context, RangeFrameHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.publish);

// The caller lends 20 + 9 * entry.count bytes at end. Exit rel32 fields point to
// restoration after the success status write. The result is the final image end.
extern "C" u64 range_epilogue(u32 frame, RangePrologue entry, address image, u64 end, u64 exits) on cpu;

// The caller lends 10 + 9 * count bytes at end. Offsets refer to the
// actual restored ranges and return in the original image.
struct FunctionEpilogue { u64 saved_end; u64 frame_end; u64 scratch_end; u64 end; };
extern "C" FunctionEpilogue function_epilogue(u32 frame, u32 scratch, SavedLayout saved,
    address image, u64 end, u64 exits) on cpu;
