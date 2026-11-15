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

// Complete compaction borrows the original IR, Copy scalar records and root
// indices. The two zeroed maps have one word per original register/local slot.
// A declaration is owned by the host iterator until B moves it whole or emits
// its selected scalar components; next releases the last original declaration.
struct CompactInput {
 Span code; Span registers; Span inputs; Span outputs;
 address register_map; address local_map; usize local_count;
};
struct CompactDecl { u32 present; usize end; usize count; };
struct CompactHost {
 Host ir; address context;
 cfn<CompactDecl,address> next;
 cfn<void,address,usize> whole;
 cfn<void,address,usize,usize> scalar;
 cfn<void,address> rewrite;
};
extern "C" usize compact_slots(CompactInput input,CompactHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.next)
 requires nonzero(host.whole) requires nonzero(host.scalar) requires nonzero(host.rewrite);


// Complete Select control borrows one original Stage. Scratch is overwritten
// before every plan; output rows live only until that branch is emitted.
// Block owners and cursors stay in Rust for their actual synchronous scope.
struct SelectionSlot { usize yes; usize no; };
struct SelectionValue { usize slot; usize destination; usize yes; usize no; };
struct SelectionInput { Span registers; address indices; address slots; usize local_count; usize domain; };
struct SelectionChildren { address first; address second; };
struct SelectionNext { address instruction; Span following; };
struct SelectionPlan { address values; usize count; };
struct SelectionHost {
 Host ir; address context;
 cfn<Span,address> ensures memory(read) block;
 cfn<SelectionChildren,address> children;
 cfn<address,address,usize> scratch;
 cfn<void,address,address,usize,SelectionPlan> rebuild;
 cfn<SelectionNext,address> next;
 cfn<void,address> keep;
 cfn<void,address,address> arm;
 cfn<void,address,usize,SelectionValue> emit;
};

extern "C" void selection_continue(address cursor,SelectionNext first,SelectionPlan plan,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit);
extern "C" void selection_move_arm(address cursor,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit);
extern "C" u32 selection_run(address code,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit);

// Complete optimization control borrows the original Program and Stage owners.
// Views expire before callbacks mutate those owners. Work stays in the original
// synchronous native Worker batch; its actual io::Error remains with the host.
struct PipelineField { usize binding; u32 word; u32 present; };
struct PipelineStage {
 Span code; Span reads; Span writes; Span cross;
 usize domain; PipelineField prefix; PipelineField selection;
 u32 operation; u32 boundary;
};
struct PipelineFusion {
 Host ir; address context;
 cfn<void,address> begin;
 cfn<address,address> next;
 cfn<address,address> previous;
 cfn<PipelineStage,address> ensures memory(read) stage;
 cfn<void,address> append;
 cfn<void,address> merge;
};
struct PipelineProgram {
 PipelineFusion fusion;
 cfn<u32,address> batch;
 cfn<usize,address> remove;
 cfn<void,address> origin;
 cfn<void,address> finish;
};
struct PipelineEmpty { u32 keep; u32 boundary; u32 carry; };
struct PipelineValues { address project; address identity; address combine; };
struct PipelineValue {
 cfn<void,address> flow;
 cfn<void,address> eliminate;
 cfn<void,address> compact;
};
struct PipelineStageHost {
 Host ir; address context; PipelineValue values;
 cfn<PipelineStage,address> ensures memory(read) stage;
 cfn<PipelineValues,address> custom;
 cfn<void,address> flow;
 cfn<void,address> eliminate;
 cfn<void,address> words;
 cfn<u32,address,u32> prune;
 cfn<u32,address> select;
 cfn<void,address> bindings;
 cfn<void,address> release_words;
 cfn<void,address> compact;
};
struct PipelineRead {
 address context;
 cfn<u32,address,u32,usize,u32> ensures memory(read) contains;
 cfn<u32,address,usize> ensures memory(read) scalar_bytes;
};
// These fields come from an exclusive root-instruction borrow. B copies only
// real word/register fields; the original Vec applies its final length.
struct PipelineRoot { u32 kind; usize tile; Span words; Span registers; usize count; };
struct PipelineRootResult { u32 keep; usize count; u32 changed; };
struct PipelineMap {
 Span code; Span tiles; PipelineField prefix; PipelineField selection;
 address flags; usize binding_count;
 address reads; address writes; address cross; usize cross_count;
};
struct PipelineAccessCounts { usize reads; usize writes; usize cross; };
extern "C" void optimize_program(PipelineProgram host) on cpu
 requires nonzero(host.fusion.ir.inspect) requires nonzero(host.fusion.begin)
 requires nonzero(host.fusion.next) requires nonzero(host.fusion.previous)
 requires nonzero(host.fusion.stage) requires nonzero(host.fusion.append) requires nonzero(host.fusion.merge)
 requires nonzero(host.batch) requires nonzero(host.remove) requires nonzero(host.origin) requires nonzero(host.finish);
extern "C" PipelineEmpty optimize_empty(PipelineStage stage,u32 carry) on cpu;
extern "C" void optimize_stage(PipelineStageHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.stage) requires nonzero(host.custom)
 requires nonzero(host.values.flow) requires nonzero(host.values.eliminate) requires nonzero(host.values.compact)
 requires nonzero(host.flow) requires nonzero(host.eliminate) requires nonzero(host.words)
 requires nonzero(host.prune) requires nonzero(host.select) requires nonzero(host.bindings)
 requires nonzero(host.release_words) requires nonzero(host.compact);
extern "C" void optimize_value(address value,PipelineValue host) on cpu
 requires nonzero(host.flow) requires nonzero(host.eliminate) requires nonzero(host.compact);
extern "C" PipelineRootResult optimize_root(PipelineRoot input,u32 preserve_scope,PipelineRead host) on cpu
 requires nonzero(host.contains) requires nonzero(host.scalar_bytes);
extern "C" PipelineAccessCounts optimize_bindings(PipelineMap input,Host ir,PipelineRead host) on cpu
 requires nonzero(ir.inspect) requires nonzero(host.contains);


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

// Only the original known set and physical IR reader are needed by a body query.
struct EffectRead {
 address known;
 cfn<EffectNode,address> ensures memory(read) inspect;
 cfn<u32,address,address> ensures memory(read) contains;
};

// A lowering cursor owns the original node, not a second IR representation.
struct LowerNode {
 u32 present; u32 kind; address source;
 address first; address second; Span body; usize slot;
 Word error; address error_slot; address target;
 u32 continuation; address payload;
};
struct LowerBranch { u32 kind; usize error; usize slot; u32 existing; usize previous; usize joined; };
struct LowerHost {
 EffectRead effects;
 cfn<LowerNode,address> next;
 cfn<u32,address,address> child;
 cfn<usize,address,u32> reg;
 cfn<void,address,address,address,u32> capture;
 cfn<void,address,usize> set_error;
 cfn<void,address,address> original;
 cfn<void,address,address,address> cleanup_body;
 cfn<void,address,address,address> cleanup_scope;
 cfn<void,address,usize> fold;
 cfn<void,address,address,usize,u64> constant;
 cfn<void,address,address,usize,usize> load;
 cfn<void,address,address,usize,usize> store;
 cfn<void,address,address,usize,usize,usize> compare;
 cfn<void,address,address,usize> raise;
 cfn<void,address,address,usize,usize,usize> join;
 cfn<u32,address,address,usize,Word> scope;
 cfn<u32,address,address,Word,LowerBranch> branches;
 cfn<void,address,address,usize,address,address> finish_if;
};
struct FoldNode {
 EffectNode structure; u32 kind; usize reg; usize slot;
 DependencyText library; DependencyText symbol; u32 value_argument;
 Span arguments; Span outputs;
};
struct FoldHost {
 address reads; DependencyText runtime;
 cfn<FoldNode,address> ensures memory(read) inspect;
 cfn<void,address,usize> insert;
 cfn<u32,address,usize> ensures memory(read) contains;
 cfn<void,address,usize,u32> replace;
};
extern "C" u32 lower_failure_block(address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare) requires nonzero(host.raise) requires nonzero(host.join) requires nonzero(host.scope) requires nonzero(host.branches) requires nonzero(host.finish_if)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains);
extern "C" u32 emit_failure_if(address block,address output,address yes,address no,Word test,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare) requires nonzero(host.raise) requires nonzero(host.join) requires nonzero(host.scope) requires nonzero(host.branches) requires nonzero(host.finish_if)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains);
extern "C" void fold_empty_status(Span code,usize slot,FoldHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.insert) requires nonzero(host.contains) requires nonzero(host.replace);

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

struct SubgroupFlags {
 u32 basic; u32 vote; u32 arithmetic; u32 ballot;
 u32 shuffle; u32 shuffle_relative; u32 extended_types;
};
struct SubgroupHost {
 Host ir;
 cfn<u32,address,usize> ensures memory(read) scalar_bytes;
};
extern "C" u32 has_subgroups(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" void subgroup_operations(Span code,Host host,address context,
 cfn<void,address,usize> emit) on cpu requires nonzero(host.inspect) requires nonzero(emit);
extern "C" SubgroupFlags subgroup_from_operations(address context,cfn<Word,address> next) on cpu
 requires nonzero(next);
extern "C" SubgroupFlags subgroup_capabilities(Span stages,SubgroupHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes);

struct RayHost {
 Host ir; address used;
 cfn<u32,address,usize> include;
 cfn<u32,address,usize> ensures memory(read) contains;
 cfn<Span,Span,usize,usize> ensures memory(read) fields;
};
extern "C" u32 ray_result_queries(Span code,Span ranges,RayHost host,address context,
 cfn<u32,address,usize,usize> visit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) requires nonzero(host.contains)
 requires nonzero(host.fields) requires nonzero(visit);

struct ExecutionBuffers { usize indirect; usize auxiliary; };
extern "C" usize indirect_buffer(Span stages,usize buffers,Host host) on cpu
 requires nonzero(host.inspect);
extern "C" ExecutionBuffers execution_buffers(Span stages,usize buffers,Host host) on cpu
 requires nonzero(host.inspect);

struct NumericRequirements { u32 float64; u32 integer8; u32 integer16; };
struct NumericHost {
 Host ir;
 cfn<void,address,address,cfn<void,address,u32>> locals;
};
extern "C" NumericRequirements numeric_requirements(Span stages,Span buffers,Span uniforms,NumericHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.locals);

extern "C" u32 atomic64_requirements(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" u32 program_atomic64_requirements(Span stages,Host host) on cpu requires nonzero(host.inspect);

extern "C" u32 maximal_reconvergence(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" u32 program_maximal_reconvergence(Span stages,Host host) on cpu requires nonzero(host.inspect);
