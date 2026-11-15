module bootstrap.ir_dataflow;
struct Node { address data; u32 kind; };
struct Span { address data; usize count; usize stride; };
struct View {
 u32 tag; usize a; usize b; usize c; usize d;
 Span first; Span second; Node left; Node right;
};
struct Host { cfn<View,Node> ensures memory(read) inspect; };
struct Word { u32 present; usize value; };
struct Results { Span first; Span second; usize a; usize b; usize inline_count; usize index; };
struct Locals {
 Node source; u32 mode; u32 writing; usize phase; usize entry; usize alias;
 usize position; usize end; u32 active;
};
extern "C" Results results(Node source,Host host) on cpu requires nonzero(host.inspect);
extern "C" Word next_result(inout Results state) on cpu;
extern "C" u32 operands(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit);
extern "C" Locals locals(Node source,u32 writing,Host host) on cpu requires nonzero(host.inspect);
extern "C" Word next_local(inout Locals state,Host host) on cpu requires nonzero(host.inspect);

// Exclusive, synchronous projections of the same original IR storage.
struct RewriteNode { address data; u32 kind; };
struct RewriteSpan { address data; usize count; usize stride; };
struct RewriteView {
 u32 tag; address a; address b; address c; address d;
 RewriteSpan first; RewriteSpan second; RewriteSpan third;
 RewriteNode left; RewriteNode right; usize length;
};
struct RewriteHost { cfn<RewriteView,RewriteNode> ensures memory(read) inspect; };
struct RewriteMap {
 address context;
 cfn<usize,address,usize> ensures memory(read) reg;
 cfn<usize,address,usize> ensures memory(read) slot;
};
struct RewriteInputMap { address context; cfn<usize,address,usize> ensures memory(read) reg; };
extern "C" void rewrite_code(RewriteSpan code,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot);
extern "C" void rewrite_operands(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg);

// Synchronous flag sampling and call propagation over the original IR.
struct EffectSeed { u32 present; u32 enabled; address name; };
struct EffectFunction { u32 present; address flag; Span body; };
struct EffectNode { Span first; Span second; address target; u32 error; };
struct EffectSeedHost {
 address iterator; address known;
 cfn<EffectSeed,address> next;
 cfn<void,address,address> remember;
};
struct EffectHost {
 address iterator; address known;
 cfn<EffectFunction,address> next;
 cfn<EffectNode,address> ensures memory(read) inspect;
 cfn<u32,address,address> ensures memory(read) contains;
};
extern "C" void seed_effect_flags(EffectSeedHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.remember);
extern "C" u32 mark_effects(EffectHost host,u32 explicit_errors) on cpu
 requires nonzero(host.next) requires nonzero(host.inspect) requires nonzero(host.contains);
extern "C" void propagate_effects(address context,cfn<u32,address> round) on cpu
 requires nonzero(round);

// Direct native dependencies and their single-owner transitive selection.
struct DependencyText { address data; usize length; };
struct DependencyOwner { u32 present; DependencyText name; };
struct DependencyNode { Span first; Span second; address target; };
struct DependencyStage { Span body; Span project; Span identity; Span combine; };
struct DependencyUnit { Span body; DependencyText name; };
struct DependencyWork { u32 present; Span body; };
struct DependencyHost {
 address output;
 cfn<DependencyNode,address> ensures memory(read) inspect;
 cfn<void,address,address> add;
};
struct DependencySeedHost {
 address iterator; address units; address pending; DependencyOwner owner;
 cfn<address,address> next;
 cfn<DependencyUnit,address,address> ensures memory(read) lookup;
 cfn<void,address,Span> push;
};
struct DependencyClosureHost {
 address units; address required; address pending; DependencyOwner owner;
 cfn<DependencyNode,address> ensures memory(read) inspect;
 cfn<DependencyUnit,address,address> ensures memory(read) lookup;
 cfn<u32,address,address> insert;
 cfn<void,address,Span> push;
 cfn<DependencyWork,address> pop;
};
extern "C" void collect_dependencies(Span code,DependencyHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.add);
extern "C" void collect_program_dependencies(Span stages,
 cfn<DependencyStage,address> ensures memory(read) inspect,DependencyHost host) on cpu
 requires nonzero(inspect) requires nonzero(host.inspect) requires nonzero(host.add);
extern "C" void seed_dependencies(DependencySeedHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.push);
extern "C" void expand_dependencies(DependencyClosureHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.lookup) requires nonzero(host.insert)
 requires nonzero(host.push) requires nonzero(host.pop);

// Frame inference consumes the same lowered original IR and flag fields.
struct FrameMeta { u32 present; u32 enabled; u32 has_body; address name; };
struct FrameImportHost {
 address iterator; address output;
 cfn<FrameMeta,address> next;
 cfn<void,address,address> remember;
};
struct FrameFunction { u32 present; address flag; Span code; Span roots; Span outputs; };
struct FrameSourceHost {
 address iterator; Host data; address imported;
 cfn<FrameFunction,address> next;
 cfn<EffectNode,address> ensures memory(read) inspect;
 cfn<Span,address> ensures memory(read) arguments;
 cfn<u32,address,address> ensures memory(read) contains;
};
extern "C" void seed_frame_imports(FrameImportHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.remember);
extern "C" void mark_frame_sources(FrameSourceHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.data.inspect) requires nonzero(host.inspect)
 requires nonzero(host.arguments) requires nonzero(host.contains);

// Optimized floating requirements borrow each original body's own registers.
struct FloatCode { Span code; address registers; };
struct FloatInput {
 FloatCode body; FloatCode identity; FloatCode combine; FloatCode project;
 Word collective;
};
struct FloatScalar { u32 kind; u32 integer; u32 floating; };
struct FloatHost {
 Host ir;
 cfn<FloatScalar,address,usize> ensures memory(read) scalar;
};
extern "C" u32 float_preserve(FloatInput input,u32 scalar,FloatHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);

// Query only the tile usage facts consumed by the original Stage caller.
extern "C" u32 has_tile_scope(Span code,u32 has_tiles,Host host) on cpu
 requires nonzero(host.inspect);
extern "C" void storage_tiles(Span code,Host host,address context,
 cfn<void,address,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit);

struct StorageHost {
 Host ir; address stage;
 cfn<Span,address,usize> ensures memory(read) components;
 cfn<u32,address,usize> ensures memory(read) scalar_bytes;
};
extern "C" void storage_words(Span code,StorageHost host,
 address context,cfn<void,address,usize,u32> emit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.components)
 requires nonzero(host.scalar_bytes) requires nonzero(emit);
