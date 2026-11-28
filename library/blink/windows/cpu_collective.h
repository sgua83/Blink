module blink.windows.cpu_collective;

import codec.builtin_type;

struct SummaryRows { address data; u64 count; u64 stride; };
struct SummaryColumn { u64 offset; u32 width; u32 signed; u32 floating; CollectiveOperation operation; };

// Only the actual scalar bytes and metadata in a summary row are initialized.
extern "C" void reduce_column(SummaryRows rows, SummaryColumn column, address output) on cpu;
extern "C" void scan_column(SummaryRows rows, SummaryColumn column, u64 head, u32 segmented) on cpu;
extern "C" void segment_counts(SummaryRows rows, u64 metadata, u32 count, address output) on cpu;
extern "C" void compact_prefix(SummaryRows rows, address output) on cpu;
