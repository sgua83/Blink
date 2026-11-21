module bootstrap.ir_dataflow;
// One original instruction vocabulary, shared by the actual IR consumers.
const u32 N_INSTRUCTION=0;
const u32 OP_TEXTURE_BINDING_ADDRESS=0;const u32 OP_TEXTURE_SAMPLE=1;const u32 OP_TEXTURE_SIZE=2;
const u32 OP_TEXTURE_LEVELS=3;const u32 OP_TEXTURE_ADDRESS=4;const u32 OP_TEXTURE_FORMAT=5;
const u32 OP_TEXTURE_READ=6;const u32 OP_TEXTURE_FETCH=7;const u32 OP_LOAD_INDEXED=8;
const u32 OP_STORE_INDEXED=9;const u32 OP_ATOMIC=10;const u32 OP_APPEND=11;
const u32 OP_TILE_BARRIER=12;const u32 OP_TILE_PUBLISH=13;const u32 OP_TILE_COLLECTIVE=14;
const u32 OP_TILE_LENGTH=15;const u32 OP_TILE_LANE=16;const u32 OP_TILE_LOAD=17;
const u32 OP_SUBGROUP=18;const u32 OP_PRINT=19;const u32 OP_INVOKE=20;const u32 OP_VIEW_LOAD=21;
const u32 OP_RAY_QUERY=22;const u32 OP_CONSTANT=23;const u32 OP_FUNCTION_ADDRESS=24;
const u32 OP_POSITION_ADDRESS=25;const u32 OP_BUFFER_STRIDE=26;const u32 OP_INDEX_VALUE=27;
const u32 OP_UNIFORM=28;const u32 OP_LOAD_LOCAL=29;const u32 OP_STORE_LOCAL=30;
const u32 OP_LOAD_LOCAL_INDEXED=31;const u32 OP_STORE_LOCAL_INDEXED=32;
const u32 OP_LOAD=33;const u32 OP_STORE=34;const u32 OP_UNARY=35;const u32 OP_BINARY=36;
const u32 OP_INTEGER_DIVIDE=37;const u32 OP_CAST=38;const u32 OP_MATH=39;const u32 OP_SELECT_VALUE=40;
const u32 OP_BRANCH=41;const u32 OP_LOOP_VALUE=42;const u32 OP_BREAK_VALUE=43;
const u32 OP_CONTINUE_VALUE=44;const u32 OP_SCOPE=45;const u32 OP_EXIT_VALUE=46;
const u32 OP_RAISE=47;const u32 OP_CLEANUP=48;


struct Node { address data; u32 kind; };
struct Span { address data; usize count; usize stride; };
// StoreLocalIndexed.first borrows its original u32 byte offset; a/b/c/d
// remain the actual base/local count/index/value register fields.
struct View {
 u32 tag; usize a; usize b; usize c; usize d;
 Span first; Span second; Node left; Node right;
};
struct Host { cfn<View,Node> ensures memory(read) inspect; };

struct LoopPaths { address live; address exited; };
struct LoopPathHost {
 Host ir;
 address context;
 cfn<void,address,address> record_break;
 cfn<void,address> clear;
 cfn<void,address,address,address> union_paths;
 cfn<void,usize,Span,Span,LoopPaths,LoopPathHost> with_branch;
 cfn<void,Span,LoopPaths,LoopPathHost> with_scope;
};
extern "C" usize next_loop_edit(Span code,usize first,Host host) on cpu
 requires nonzero(host.inspect);
extern "C" u32 has_function_exit(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" void loop_exit_paths(Span code,LoopPaths paths,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope);
extern "C" void loop_path_branch(Span yes_code,Span no_code,LoopPaths parent,
 LoopPaths yes,LoopPaths no,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope);
extern "C" void loop_path_scope(Span code,LoopPaths parent,LoopPaths child,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope);
struct LoopBorrowEdge { usize level; usize input; u32 present; };
struct LoopBorrowHost {
 cfn<LoopBorrowEdge,address> inspect;
 cfn<address,address,usize> source;
 cfn<void,address,address> replace;
 cfn<void,address> retain;
};
extern "C" void project_loop_borrow(address context, usize scope, LoopBorrowHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.source)
 requires nonzero(host.replace) requires nonzero(host.retain);
struct Word { u32 present; usize value; };
struct Results { Span first; Span second; usize a; usize b; usize inline_count; usize index; };
struct Locals {
 Node source; u32 mode; u32 writing; usize phase; usize entry; usize alias;
 usize position; usize end; u32 active;
};
extern "C" Results results(Node source,Host host) on cpu requires nonzero(host.inspect);
extern "C" Word next_result(inout Results state) on cpu;
enum ResultRole:u32 { Value=0; Quotient=1; Remainder=2; };
extern "C" void result_origins(Node source,Host host,address context,
 cfn<u32,address,usize,usize,usize,ResultRole> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit);

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


// One fresh ReductionKernel, built directly from the three real value bodies.
// Rust keeps their owning Vec/Values/source records; B controls the complete
// preparation and only borrows views until the next actual owner mutation.
struct ReductionInput { Node source; u32 project; u32 identity; };
struct ReductionHeader { usize registers; usize locals; };
struct ReductionExtract { u32 eligible; u32 operation; usize inputs; };
struct ReductionHost {
 Host ir; address context;
 cfn<ReductionHeader,address> ensures memory(read) header;
 cfn<void,address,usize,Node,usize> clone;
 cfn<void,address,usize,usize,usize> remap;
 cfn<void,address,usize,usize> push_root;
 cfn<Span,address,usize> roots;
 cfn<Span,address,usize> part;
 cfn<address,address,usize> uniform;
 cfn<void,address,usize,address> extract;
 cfn<void,address> release_uniform;
 cfn<void,address> entry;
 cfn<void,address> seed;
 cfn<void,address,usize,u32> body;
 cfn<void,address,usize> retain;
 cfn<void,address,usize,usize,usize> forward;
 cfn<void,address> origin;
 cfn<usize,address,usize> ensures memory(read) alias;
 cfn<void,address,usize> invariant;
 cfn<void,address,usize,usize,usize> boundaries;
 cfn<void,address,usize> append;
 cfn<void,address> finish;
};

extern "C" ReductionExtract reduction_extract(Node source,address uniform,Host ir) on cpu requires nonzero(ir.inspect);
extern "C" void reduction_kernel(ReductionInput input,ReductionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.header) requires nonzero(host.clone) requires nonzero(host.remap) requires nonzero(host.push_root) requires nonzero(host.roots) requires nonzero(host.part) requires nonzero(host.uniform) requires nonzero(host.extract) requires nonzero(host.release_uniform) requires nonzero(host.entry) requires nonzero(host.seed) requires nonzero(host.body) requires nonzero(host.retain) requires nonzero(host.forward) requires nonzero(host.origin) requires nonzero(host.alias) requires nonzero(host.invariant) requires nonzero(host.boundaries) requires nonzero(host.append) requires nonzero(host.finish);

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

enum ResultOriginKind:u32 { Expression=0; LocalRead=1; IndexedStoreRead=2; IndexedRead=3; };
struct RetainOriginHost {
 Host read; RewriteHost write;
 cfn<u32,address,usize,usize,usize,ResultRole> collect;
 cfn<void,address,address,ResultOriginKind> attach;
 cfn<u32,address> pending;
};
extern "C" void retain_result_origins(RewriteSpan code,address context,RetainOriginHost host) on cpu
 requires nonzero(host.read.inspect) requires nonzero(host.write.inspect) requires nonzero(host.collect)
 requires nonzero(host.attach) requires nonzero(host.pending);

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
struct LowerCursor {LowerNode node;u32 flags;u32 phase;};
struct LowerNeed {u32 kind;u32 flags;address child;Word test;LowerBranch branch;};
struct FailureBranchNeed {u32 kind;Word test;LowerBranch branch;};
extern "C" LowerNeed lower_failure_start(address state,address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains);
extern "C" LowerNeed lower_failure_child(address state,address block,address output,u32 nested,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains);
extern "C" LowerNeed lower_failure_branch(address state,address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains);
extern "C" FailureBranchNeed failure_branch_prepare(address block,address yes,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.raise) requires nonzero(host.load) requires nonzero(host.constant) requires nonzero(host.compare) requires nonzero(host.join) requires nonzero(host.store);
extern "C" void failure_branch_finish(address block,address output,address yes,address no,Word test,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.constant) requires nonzero(host.compare) requires nonzero(host.store) requires nonzero(host.finish_if);
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
struct RegisterFacts { u32 kind; u32 integer; u32 floating; u32 bytes; };
struct RegisterHost {
 Host ir;
 cfn<RegisterFacts,Span,usize> ensures memory(read) scalar;
};
// The mask uses the original ScalarCode positions, including for a single requested scalar.
extern "C" u32 float_preserve(Node source,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);

// Query only the tile usage facts consumed by the original Stage caller.
extern "C" u32 has_tile_scope(Span code,u32 has_tiles,Host host) on cpu
 requires nonzero(host.inspect);
extern "C" void private_spans(Span code,Host host,address context,
                              cfn<void,address,usize,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit);
extern "C" void storage_tiles(Span code,Host host,address context,
 cfn<void,address,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit);

struct StorageHost {
 Host ir; address stage;
 cfn<u32,address,usize> ensures memory(read) scalar_bytes;
};
enum StorageEvent : u32 { Read = 0; Birth = 1; Group = 2; };
struct StorageOutput {
 address context;
 cfn<void,address,StorageEvent,usize,u32,i64> emit;
};
// Use phase zero for layout and -1 for the standalone word query.
extern "C" void storage_facts(Span code,Span tiles,StorageHost host,i64 phase,StorageOutput output) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes) requires nonzero(output.emit);

// Borrow the Stage tiles and used words for this call and deliver only matching input fragments.
extern "C" void tile_input_words(Span tiles,Host host,address used,
 cfn<u32,address,usize,u32> ensures memory(read) contains,address output,
 cfn<void,address,usize,u32,u32,u32> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(contains) requires nonzero(emit);

// The bits select the stored low word and, for a wide value, the stored high word.
extern "C" u32 tile_publish_parts(address used,usize tile,u32 word,u32 bytes,
 cfn<u32,address,usize,u32> ensures memory(read) contains) on cpu
 requires nonzero(contains);

struct SubgroupFlags {
 u32 basic; u32 vote; u32 arithmetic; u32 ballot;
 u32 shuffle; u32 shuffle_relative; u32 extended_types;
};
struct ShaderSubgroups {
 SubgroupFlags capabilities; u32 builtins; u32 maximal_reconvergence;
};
struct ProgramCodeRequirements {
 SubgroupFlags subgroup; u32 atomic64; u32 float_preserve; u32 maximal_reconvergence;
};
extern "C" u32 has_subgroups(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" u32 subgroup_operations(Span code,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);
struct ShaderCodeRequirements {
 ShaderSubgroups subgroups; u32 atomic64; u32 float_preserve;
};
extern "C" ShaderCodeRequirements shader_code_requirements(
 Node source,u32 requested,bool subgroups,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);
extern "C" SubgroupFlags subgroup_from_operations(address context,cfn<Word,address> next) on cpu
 requires nonzero(next);
extern "C" SubgroupFlags subgroup_capabilities(Span stages,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);
extern "C" ProgramCodeRequirements program_code_requirements(Span stages,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar);

struct RayQuery { Span fields; usize result; };
extern "C" u32 scan_ray_queries(Span code,Host host,bool closest_only,address context,
 cfn<void,address,Span,usize> emit) on cpu requires nonzero(host.inspect) requires nonzero(emit);

struct RayHost {
 Host ir; address used;
 cfn<u32,address,usize> include;
 cfn<u32,address,usize> ensures memory(read) contains;
 cfn<Span,Span,usize,usize> ensures memory(read) fields;
};
extern "C" u32 ray_result_queries(Span code,Span queries,Span ranges,RayHost host,address context,
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

extern "C" u32 maximal_reconvergence(Span code,Host host) on cpu requires nonzero(host.inspect);
extern "C" u32 program_maximal_reconvergence(Span stages,Host host) on cpu requires nonzero(host.inspect);

// The original program's buffers and type storage are borrowed synchronously.
extern "C" u32 shader_pages_fit(Span buffers,u32 bytes,Host host) on cpu requires nonzero(host.inspect);

// CPU ordering borrows the real block and reuses its pending instruction storage.
enum ReadQueue:u32 { Immediate=0; Value=1; Shared=2; Local=3; };
enum ReadWorkKind:u32 { Done=0; Expand=1; Emit=2; };
enum ReadClear:u32 { All=0; Shared=1; Local=2; };
struct ReadWork { usize id; ReadWorkKind kind; };
struct ReadPrivate { usize id; usize first; usize end; bool pending; };
struct ReadHost {
 Host ir;
 cfn<u64,u32> math_outgoing;
 cfn<Node,address> next;
 cfn<void,address,u32> child;
 cfn<void,address,address,ReadQueue,usize,usize> store;
 cfn<Node,address,usize> queued;
 cfn<u32,address,usize> enqueue_operand;
 cfn<u32,address,usize> demand_operand;
 cfn<usize,address> work_len;
 cfn<void,address,ReadWork> push;
 cfn<void,address,usize> reverse;
 cfn<ReadWork,address> pop;
 cfn<void,address,usize> emit;
 cfn<usize,address,ReadClear> count;
 cfn<usize,address,usize> read_at;
 cfn<ReadPrivate,address,usize> private_at;
 cfn<void,address,usize,usize> retain;
 cfn<void,address,ReadClear,usize> clear;
};
extern "C" void read_demand(address pending,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.push)
 requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand);
extern "C" void place_reads(address input,address pending,Span writes,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.math_outgoing)
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.store)
 requires nonzero(host.queued) requires nonzero(host.enqueue_operand) requires nonzero(host.demand_operand) requires nonzero(host.work_len)
 requires nonzero(host.push) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.count) requires nonzero(host.read_at)
 requires nonzero(host.private_at) requires nonzero(host.retain) requires nonzero(host.clear);

enum CpuRangePass:u32 { Reduce=0; ReduceTail=1; Scan=2; ScanPrefix=3; ScanAdd=4;
 SegmentReduce=5; SegmentPrefix=6; SegmentFinish=7; };
enum CpuRangeType:u32 { U32=0; Bool=1; Value=2; };
enum CpuRangeBlock:u32 { Body=0; Yes=1; No=2; Project=3; Combine=4; };
enum CpuRangeOp:u32 { Index=0; Constant=1; Load=2; Store=3; LocalLoad=4; LocalStore=5;
 Add=6; Subtract=7; Equal=8; Or=9; Uniform=10; Break=11; };

struct CpuRangePlan {
 u32 project; u32 identity; u32 apply; u32 finish; u32 packed; u32 prefix; u32 publish;
 usize summary;
};
struct CpuRangeInput {
 CpuRangePass pass; u32 has_head; u32 head_word; u32 counted;
 usize source; usize target; usize temporary; usize count_binding; u32 count_word;
 u32 source_word; u32 target_word; u32 payload_bytes; usize fields;
 Span initial; Span left; Span source_registers; Span projected; Span result;
};
struct CpuRangeBody {
 usize initialization_end; usize loop_end; usize carry; usize count_slot; usize length_slot;
 u32 history; u32 reset_on_head; u32 break_on_head;
};
struct CpuRangeHost {
 cfn<usize,address,CpuRangeType,usize> reg;
 cfn<usize,address,CpuRangeType> local;
 cfn<u32,address,usize,u32> ensures memory(read) offset;
 cfn<void,address,CpuRangeBlock,CpuRangeOp,usize,usize,usize,u32> put;
 cfn<void,address,CpuRangeBlock> move_block;
 cfn<void,address,usize,CpuRangeBlock> branch;
 cfn<usize,address> ensures memory(read) size;
};
extern "C" CpuRangePlan cpu_range_plan(CpuRangePass pass,usize words,u32 has_head,u32 counted) on cpu;
extern "C" CpuRangeBody cpu_range_body(CpuRangeInput input,CpuRangePlan plan,address context,CpuRangeHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.local) requires nonzero(host.offset) requires nonzero(host.put)
 requires nonzero(host.move_block) requires nonzero(host.branch) requires nonzero(host.size);

