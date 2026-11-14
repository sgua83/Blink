module blink.windows.context;

const u32 CONTEXT_AMD64=0x00100000;
const u32 CONTEXT_CONTROL=0x00100001;
const u32 CONTEXT_INTEGER=0x00100002;
const u32 CONTEXT_SEGMENTS=0x00100004;
const u32 CONTEXT_FLOATING_POINT=0x00100008;
const u32 CONTEXT_DEBUG_REGISTERS=0x00100010;
const u32 CONTEXT_FULL=0x0010000b;
const u32 CONTEXT_ALL=0x0010001f;
const u32 CONTEXT_XSTATE=0x00100040;
const u32 CONTEXT_EXCEPTION_REQUEST=0x40000000;

const u32 XSTATE_LEGACY_FLOATING_POINT=0;
const u32 XSTATE_LEGACY_SSE=1;
const u32 XSTATE_AVX=2;

// Four little-endian u32 words are storage bits, not a B lane-to-register map.
struct Native128 { uint4 words; };
struct XmmSaveArea32 {
    u16 control_word;
    u16 status_word;
    u8 tag_word;
    u8 reserved1;
    u16 error_opcode;
    u32 error_offset;
    u16 error_selector;
    u16 reserved2;
    u32 data_offset;
    u16 data_selector;
    u16 reserved3;
    u32 mx_csr;
    u32 mx_csr_mask;
    array<Native128,8> float_registers;
    array<Native128,16> xmm_registers;
    array<u8,96> reserved4;
};
struct Context64 {
    u64 p1_home;
    u64 p2_home;
    u64 p3_home;
    u64 p4_home;
    u64 p5_home;
    u64 p6_home;
    u32 context_flags;
    u32 mx_csr;
    u16 seg_cs;
    u16 seg_ds;
    u16 seg_es;
    u16 seg_fs;
    u16 seg_gs;
    u16 seg_ss;
    u32 eflags;
    u64 dr0;
    u64 dr1;
    u64 dr2;
    u64 dr3;
    u64 dr6;
    u64 dr7;
    u64 rax;
    u64 rcx;
    u64 rdx;
    u64 rbx;
    u64 rsp;
    u64 rbp;
    u64 rsi;
    u64 rdi;
    u64 r8;
    u64 r9;
    u64 r10;
    u64 r11;
    u64 r12;
    u64 r13;
    u64 r14;
    u64 r15;
    u64 rip;
    XmmSaveArea32 floating;
    array<Native128,26> vector_registers;
    u64 vector_control;
    u64 debug_control;
    u64 last_branch_to_rip;
    u64 last_branch_from_rip;
    u64 last_exception_to_rip;
    u64 last_exception_from_rip;
};

extern "C" i32 InitializeContext(address buffer,u32 flags,inout address context,
    inout u32 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 GetThreadContext(handle thread,address context) on cpu from "KERNEL32.dll";
extern "C" i32 SetThreadContext(handle thread,address context) on cpu from "KERNEL32.dll";
extern "C" u64 GetEnabledXStateFeatures() on cpu from "KERNEL32.dll";
extern "C" i32 SetXStateFeaturesMask(address context,u64 features) on cpu from "KERNEL32.dll";
extern "C" i32 GetXStateFeaturesMask(address context,write u64 features) on cpu
    ensures initialized(features,returned != 0) from "KERNEL32.dll";
extern "C" address LocateXStateFeature(address context,u32 feature,address bytes) on cpu
    from "KERNEL32.dll";
extern "C" i32 CopyContext(address destination,u32 flags,address source) on cpu
    from "KERNEL32.dll";

enum ContextError : u32 {
    Size(u32)=1;
    Initialize(u32)=2;
    Features(u32)=3;
    Before(u32)=4;
    Capture(u32)=5;
    After(u32)=6;
    Availability(u32)=7;
};
struct ContextSnapshot {
    dataset<u8> bytes;
    u32 context_offset;
    u64 present_features;
    i64 begin_ticks;
    i64 end_ticks;
};

result<ContextSnapshot,ContextError> capture_thread(handle thread,u32 flags,u64 features) on cpu;
