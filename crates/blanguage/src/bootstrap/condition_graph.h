module bootstrap.condition_graph;
import codec.scalar_ranges;
import bootstrap.scalar_fold;
import bootstrap.fact_key;
import bootstrap.condition_index;
import bootstrap.clause_storage;

// These descriptors borrow the original Clause query and its physical buffers.
// They are private native fields, not a stored condition or public graph format.
struct Span { address data; usize count; };
struct TraversalStack { address records; u64 length; u64 capacity; };

extern "C" void clause_postorder_start(inout TraversalStack stack,address context,cfn<address,address> next) on cpu
    requires nonzero(next);
extern "C" address clause_postorder_next(inout TraversalStack stack,address context,
    cfn<u32,address,address> completed) on cpu requires nonzero(completed);
extern "C" void traversal_stack_drop(inout TraversalStack stack) on cpu;

struct CallProjectionHost {
 cfn<address,address> next;
 cfn<void,address,address> impossible;
 cfn<void,address,address,address> facts;
 cfn<void,address,address,address,address> both;
 cfn<void,address,address,address,address> either;
};
extern "C" void project_call_graph(address context,CallProjectionHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.impossible) requires nonzero(host.facts)
 requires nonzero(host.both) requires nonzero(host.either);
struct Gate { usize left; usize right; u32 kind; usize parent_start; usize parent_count; };
struct Graph { address nodes; usize stride; address values; address parents; };
extern "C" void connect_parents(Graph graph,usize count,address storage,cfn<address,address,usize> allocate) on cpu
 requires nonzero(allocate);
struct Assignment { u32 present; usize node; u32 truth; };
struct Checkpoint { usize nodes; usize facts; usize negative; usize choice; };
struct Alternative { u32 kind; usize node; u32 truth; Checkpoint checkpoint; };
struct Host {
 cfn<void,address,usize,u32> pending_push;
 cfn<Assignment,address> pending_pop;
 cfn<void,address> pending_clear;
 cfn<void,address,usize> trail_push;
 cfn<usize,address> trail_pop;
 cfn<Span,address> ensures memory(read) trail;
 cfn<usize,address> ensures memory(read) additions;
 cfn<void,address> undo_addition;
 cfn<void,address,usize> add_leaf;
 cfn<u32,address> contradictory;
 cfn<u32,address> add_next;
 cfn<void,address,usize> begin_facts;
 cfn<address,address> next_fact;
 cfn<void,address,FactView> set_next_fact;
 FactHost facts;
 cfn<usize,address> ensures memory(read) remaining;
 cfn<void,address,usize,u32,Checkpoint> push_clause;
 cfn<void,address,address,Checkpoint> push_fact;
 cfn<Alternative,address> pop_alternative;
};
extern "C" u32 overlap(usize first,usize second,u32 right_truth,address context,Graph graph,Host host) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative);
// The preparation frame borrows the original facts and owns only query-local containers.
struct QueryNode { usize left; usize right; u32 kind; };
struct EquivalenceHost {
 cfn<QueryNode,address,usize> ensures memory(read) node;
 cfn<void,address,usize,usize,QueryNode> write;
 cfn<usize,address,QueryNode> append;
 cfn<usize,address,QueryNode> ensures memory(read) gate_get;
 cfn<void,address,QueryNode,usize> gate_put;
 cfn<usize,address> ensures memory(read) size;
 cfn<Span,address,usize> indices;
 cfn<void,address,usize> lists;
 cfn<Span,address,usize> ensures memory(read) list;
 cfn<void,address,usize,usize> list_push;
 cfn<void,address,usize> list_clear;
 cfn<void,address,usize> list_sort;
 cfn<void,address> seen_clear;
 cfn<u32,address,usize> visit;
 cfn<void,address,usize> push;
 cfn<usize,address> pop;
 cfn<void,address,usize> atoms_clear;
 cfn<void,address,usize,usize> atom_insert;
 cfn<u32,address,usize,usize> ensures memory(read) atoms_subset;
 cfn<Graph,address,address,usize> build;
};
extern "C" u32 equivalent(usize first,usize second,address plan,address context,usize count,EquivalenceHost preparation,Host host,cfn<void,address> reset) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative)
 requires nonzero(reset)
 requires nonzero(preparation.node) requires nonzero(preparation.write) requires nonzero(preparation.indices)
 requires nonzero(preparation.list_clear) requires nonzero(preparation.seen_clear) requires nonzero(preparation.visit)
 requires nonzero(preparation.push) requires nonzero(preparation.pop) requires nonzero(preparation.list_push)
 requires nonzero(preparation.list_sort) requires nonzero(preparation.list) requires nonzero(preparation.lists)
 requires nonzero(preparation.atoms_clear) requires nonzero(preparation.atom_insert) requires nonzero(preparation.atoms_subset)
 requires nonzero(preparation.append) requires nonzero(preparation.gate_get) requires nonzero(preparation.gate_put)
 requires nonzero(preparation.size) requires nonzero(preparation.build);

// The original index owns the topology. Each result slot starts with the solver's state.
enum RequiredState:u32 { Pending=0; Impossible=1; Ready=2; };
struct RequiredSlot { usize users; usize remaining; RequiredState state; RequiredState additions; };
struct RequiredGraph { ConditionNodes nodes; address slots; usize stride; usize root; };
struct RequiredHost {
 cfn<u32,address,usize,address> leaf;
 cfn<u32,address,usize,usize,usize,u32,u32> union;
 cfn<u32,address,usize,usize,usize,u32,u32> common;
 cfn<u32,address,usize> valid_additions;
 cfn<void,address,usize> discard;
};
extern "C" void required_prepare(RequiredGraph graph) on cpu;
extern "C" void required_evaluate(RequiredGraph graph,inout TraversalStack pending,address context,RequiredHost host) on cpu
 requires nonzero(host.leaf) requires nonzero(host.union) requires nonzero(host.common)
 requires nonzero(host.valid_additions) requires nonzero(host.discard);

struct CrossScalars { array<address,4> values; };
extern "C" u32 cross_conflict(address left,address right,u32 may_share) on cpu;

struct FactHost {
 address unsigned32;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" FactView negated_fact(FactView value,FactHost host) on cpu
 requires nonzero(host.compare);


struct NegatePending { u32 present; u32 ready; address source; };
struct NegateHost {
 cfn<void,address,address,u32> push;
 cfn<NegatePending,address> pop;
 cfn<u32,address,address> ensures memory(read) has_result;
 cfn<void,address,address> unconstrained;
 cfn<void,address,address> impossible;
 cfn<void,address,address> begin_facts;
 cfn<FactView,address> next_fact;
 cfn<void,address,address,FactView> either_fact;
 cfn<void,address,address,address,address> either;
 cfn<void,address,address,address,address> both;
 FactHost facts;
};
extern "C" void negate_graph(address root,address context,NegateHost host) on cpu
 requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.has_result) requires nonzero(host.unconstrained) requires nonzero(host.impossible)
 requires nonzero(host.begin_facts) requires nonzero(host.next_fact) requires nonzero(host.either_fact)
 requires nonzero(host.either) requires nonzero(host.both) requires nonzero(host.facts.compare);

struct InternalScalar { u32 kind; u32 width; u64 mask; u64 bias; };
struct InternalOperand { address identity; u64 bits; address scalar; u32 is_constant; };
struct InternalPredicate { address scalar; BinaryOperation operation; InternalOperand left; InternalOperand right; };
extern "C" u32 internal_conflict(address owner,u32 aliases,CrossScalars integers,
 cfn<InternalPredicate,address> ensures memory(read) read_predicate) on cpu
 requires nonzero(read_predicate);

// The original synchronous task-return query owns all sets and node results.
struct TaskSourceSet { u32 known; address values; };
struct TaskSourceHost {
 cfn<usize,address> ensures memory(read) count;
 cfn<address,address,usize> ensures memory(read) node;
 cfn<void,address,usize> begin;
 cfn<FactView,address> next;
 cfn<u32,address,FactView> ensures memory(read) contains;
 cfn<TaskSourceSet,address,address> ensures memory(read) lookup;
 cfn<void,address,usize> unknown;
 cfn<void,address,usize> empty;
 cfn<void,address,usize,u32> single;
 cfn<void,address,usize,address> copy;
 cfn<void,address,usize,address,address> intersection;
 cfn<void,address,usize,address,address> union;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 address identity;
 FactHost facts;
};
extern "C" void task_return_sources(address context,TaskSourceHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.node) requires nonzero(host.begin)
 requires nonzero(host.next) requires nonzero(host.contains) requires nonzero(host.lookup)
 requires nonzero(host.unknown) requires nonzero(host.empty) requires nonzero(host.single)
 requires nonzero(host.copy) requires nonzero(host.intersection) requires nonzero(host.union)
 requires nonzero(host.same_scalar) requires nonzero(host.facts.compare);

struct ComputedEqualityStep { u32 present; address result; };
struct ComputedEqualityHost {
 cfn<void,address> begin;
 cfn<ComputedEqualityStep,address> step;
 cfn<address,address> ensures memory(read) produced;
 cfn<void,address> remember;
 cfn<void,address,address,address> equate;
 cfn<u32,address> ensures memory(read) has_equalities;
 cfn<void,address> extend;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" void computed_equalities(address context,ComputedEqualityHost host) on cpu
 requires nonzero(host.begin) requires nonzero(host.step) requires nonzero(host.produced)
 requires nonzero(host.remember) requires nonzero(host.equate) requires nonzero(host.has_equalities)
 requires nonzero(host.extend) requires nonzero(host.compare);
struct AdditionalEqualityHost {
 cfn<FactView,address> next;
 cfn<address,address,address> ensures memory(read) lookup;
 cfn<address,address> ensures memory(read) expression;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<void,address> own;
 cfn<void,address> extend;
 cfn<void,address> computed;
 cfn<u32,address> ensures memory(read) unchanged;
 cfn<void,address> borrow;
};
extern "C" void additional_equalities(address context,AdditionalEqualityHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.expression)
 requires nonzero(host.compare) requires nonzero(host.own) requires nonzero(host.extend)
 requires nonzero(host.computed) requires nonzero(host.unchanged) requires nonzero(host.borrow);
struct AliasFactHost {
 cfn<FactView,address,u32> next;
 cfn<address,address,address> ensures memory(read) root;
 cfn<address,address,address> ensures memory(read) previous;
 cfn<void,address,address> group;
 cfn<address,address> member;
 cfn<void,address,FactView,address> emit;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" void alias_facts(address context,u32 inherited,AliasFactHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.previous)
 requires nonzero(host.group) requires nonzero(host.member) requires nonzero(host.emit)
 requires nonzero(host.compare);

// Original typed source-path terms remain owned by the synchronous native caller.
struct CanonicalTerm { u32 present; address value; };
enum CanonicalInsertion : u32 { Reject=0; Insert=1; RetainInsert=2; };
struct CanonicalTermHost {
 cfn<u32,address> ensures memory(read) same;
 cfn<u32,address,u32> conflict;
 cfn<FactView,address> next;
 cfn<u32,address,FactView> insert;
 cfn<void,address,FactView> remove;
 cfn<u32,address> ensures memory(read) remaining;
 FactHost facts;
};
extern "C" u32 canonical_term_overlap(address context,CanonicalTermHost host) on cpu
 requires nonzero(host.same) requires nonzero(host.conflict) requires nonzero(host.next)
 requires nonzero(host.insert) requires nonzero(host.remove) requires nonzero(host.remaining)
 requires nonzero(host.facts.compare);
struct CanonicalInsertHost {
 cfn<CanonicalTerm,address> next;
 cfn<u32,address,address> ensures memory(read) subset;
 cfn<void,address> reset;
 cfn<u32,address,address> overlap;
};
extern "C" CanonicalInsertion canonical_insert(address context,address candidate,u32 empty,CanonicalInsertHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.subset) requires nonzero(host.reset)
 requires nonzero(host.overlap);
struct CanonicalKeepHost {
 cfn<u32,address,address> ensures memory(read) subset;
 cfn<u32,address,address> overlap;
};
extern "C" u32 canonical_keep(address old,address candidate,CanonicalKeepHost host) on cpu
 requires nonzero(host.subset) requires nonzero(host.overlap);
extern "C" void canonical_normalize(address source,address output) on cpu;
struct CanonicalAcceptHost {
 cfn<u32,address> conflict;
 cfn<void,address,address> normalize;
};
extern "C" void canonical_accept(address source,address target,CanonicalAcceptHost host) on cpu
 requires nonzero(host.conflict) requires nonzero(host.normalize);
struct CanonicalEitherHost {
 cfn<CanonicalTerm,address> next;
 cfn<void,address,address> insert;
};
extern "C" void canonical_either_terms(address context,CanonicalEitherHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.insert);
struct CanonicalProductHost {
 cfn<CanonicalTerm,address> left;
 cfn<void,address> reset;
 cfn<CanonicalTerm,address> right;
 cfn<void,address,address,address> union;
};
extern "C" void canonical_product(address context,CanonicalProductHost host) on cpu
 requires nonzero(host.left) requires nonzero(host.reset) requires nonzero(host.right) requires nonzero(host.union);
struct CanonicalGraphHost {
 cfn<usize,address> ensures memory(read) count;
 cfn<address,address,usize> ensures memory(read) node;
 cfn<address,address,address> ensures memory(read) lookup;
 cfn<void,address,usize> empty;
 cfn<void,address,usize,address> leaf;
 cfn<void,address,usize,address,address> either;
 cfn<void,address,usize,address,address> both;
};
extern "C" void canonical_graph(address context,CanonicalGraphHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.node) requires nonzero(host.lookup)
 requires nonzero(host.empty) requires nonzero(host.leaf) requires nonzero(host.either) requires nonzero(host.both);
struct CanonicalConsensusHost {
 cfn<CanonicalTerm,address> left;
 cfn<void,address,address> right_begin;
 cfn<CanonicalTerm,address> right;
 cfn<void,address,address> facts_begin;
 cfn<FactView,address> fact;
 cfn<u32,address,FactView> ensures memory(read) contains;
 cfn<void,address,address,address,FactView,FactView> resolve;
 FactHost facts;
};
extern "C" void canonical_consensus(address context,CanonicalConsensusHost host) on cpu
 requires nonzero(host.left) requires nonzero(host.right_begin) requires nonzero(host.right)
 requires nonzero(host.facts_begin) requires nonzero(host.fact) requires nonzero(host.contains)
 requires nonzero(host.resolve) requires nonzero(host.facts.compare);
struct CanonicalMergeHost {
 cfn<u32,address> next;
 cfn<u32,address> insert;
};
extern "C" u32 canonical_merge_additions(address context,CanonicalMergeHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.insert);
struct CanonicalClosureHost { cfn<u32,address> round; };
extern "C" void canonical_closure(address context,CanonicalClosureHost host) on cpu
 requires nonzero(host.round);
struct CanonicalFoldHost { cfn<u32,address> next; cfn<void,address> append; };
extern "C" void canonical_fold(address context,CanonicalFoldHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.append);

// One synchronous public-contract normalizer owns every typed container.
struct ContractEnd { u64 low; u64 high; };
struct ContractCell { u32 present; ContractEnd end; usize value; };
struct ContractPair { ContractEnd end; usize left; usize right; };
struct ContractIndex { u32 present; usize value; };
struct ContractPending { u32 present; u32 ready; usize index; };
struct ContractOperation { u32 present; u32 ready; u32 either; usize left; usize right; };
struct ContractDomain { u32 kind; address identity; u32 count; u32 distance; };
struct ContractVariable { u32 kind; FactView fact; ContractEnd end; };
struct ContractImport { u32 present; u32 fold; address clause; u32 either; usize next; usize result; usize count; };
struct ContractNodeHost {
 cfn<ContractCell,address> next;
 cfn<ContractCell,address> ensures memory(read) last;
 cfn<void,address,ContractEnd> extend;
 cfn<void,address,ContractCell> push;
 cfn<usize,address> ensures memory(read) count;
 cfn<usize,address> intern;
};
struct ContractPairHost {
 cfn<i32,address> ensures memory(read) compare;
 cfn<usize,address,u32> ensures memory(read) count;
 cfn<ContractCell,address,u32,usize> ensures memory(read) cell;
 cfn<void,address,ContractEnd,usize,usize> push;
};
struct ContractOperationHost {
 cfn<ContractOperation,address> pop;
 cfn<void,address,ContractOperation> push;
 cfn<ContractIndex,address,u32,usize,usize> ensures memory(read) lookup;
 cfn<void,address,u32,usize,usize,usize> remember;
 cfn<void,address,usize,usize> prepare;
 cfn<usize,address> ensures memory(read) count;
 cfn<ContractPair,address,usize> ensures memory(read) pair;
 cfn<void,address,ContractEnd,usize> cell;
 cfn<usize,address> make;
 cfn<void,address> release;
};
struct ContractNotHost {
 cfn<ContractPending,address> pop;
 cfn<void,address,usize,u32> push;
 cfn<ContractIndex,address,usize> ensures memory(read) lookup;
 cfn<void,address,usize,usize> remember;
 cfn<void,address,usize> load;
 cfn<usize,address> ensures memory(read) count;
 cfn<ContractCell,address,usize> ensures memory(read) cell;
 cfn<void,address,ContractEnd,usize> emit;
 cfn<usize,address> make;
 cfn<void,address> release;
};
struct ContractSemanticHost {
 cfn<usize,address,FactView> atom;
 cfn<usize,address,address,address,u64> range;
 cfn<usize,address,usize,usize,u32> operation;
 cfn<usize,address,usize> invert;
 cfn<address,address,address,u32> integer;
 cfn<u32,address,address> ensures memory(read) boolean;
 cfn<address,address,address> length;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 cfn<InternalScalar,address> ensures memory(read) scalar;
 cfn<InternalPredicate,address> ensures memory(read) predicate;
 cfn<ContractDomain,address> ensures memory(read) domain;
 address unsigned32;
};
struct ContractOperandsHost {
 cfn<address,address> pop;
 cfn<void,address,address> push;
 cfn<u32,address,address> seen;
 cfn<u32,address,address> ensures memory(read) imported;
 cfn<u32,address> all_unary;
 cfn<void,address,address,u32> emit;
};
struct ContractImportHost {
 cfn<ContractImport,address> pop;
 cfn<void,address,address> visit;
 cfn<void,address,address,u32,usize> start;
 cfn<void,address,address,u32,usize,usize> resume;
 cfn<ContractIndex,address,address> ensures memory(read) lookup;
 cfn<void,address,address> begin;
 cfn<FactView,address> next;
 cfn<InternalPredicate,address> ensures memory(read) predicate;
 cfn<ContractDomain,address> ensures memory(read) domain;
 cfn<usize,address,FactView> fact;
 cfn<usize,address,usize,usize,u32> operation;
 cfn<address,address,usize> ensures memory(read) operand;
 cfn<void,address,address,usize> remember;
};
struct ContractEmitHost {
 cfn<ContractPending,address> pop;
 cfn<void,address,usize,u32> push;
 cfn<u32,address,usize> ensures memory(read) known;
 cfn<void,address,usize> load;
 cfn<ContractVariable,address> ensures memory(read) variable;
 cfn<usize,address> ensures memory(read) count;
 cfn<ContractCell,address,usize> ensures memory(read) cell;
 cfn<void,address,FactView> bound;
 cfn<void,address,ContractEnd,ContractEnd> guard;
 cfn<void,address,FactView> atom;
 cfn<void,address> negate;
 cfn<address,address> ensures memory(read) guard_clause;
 cfn<void,address,usize> child;
 cfn<void,address,usize> both;
 cfn<void,address> first;
 cfn<void,address> either;
 cfn<void,address,usize,u32> remember;
 cfn<void,address> release;
};
struct ContractUnionHost {
 cfn<address,address,u32,u32> ensures memory(read) case;
 cfn<usize,address,address> import;
 cfn<usize,address,usize,usize,u32> operation;
 cfn<void,address,u32,usize> emit;
};

extern "C" usize contract_node(address context,ContractNodeHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.last) requires nonzero(host.extend) requires nonzero(host.push) requires nonzero(host.count) requires nonzero(host.intern);

extern "C" u32 contract_pairs(address context,usize left,usize right,ContractPairHost host) on cpu
 requires nonzero(host.compare) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.push);

extern "C" usize contract_operation(address context,usize left,usize right,u32 either,ContractOperationHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.lookup) requires nonzero(host.remember) requires nonzero(host.prepare) requires nonzero(host.count) requires nonzero(host.pair) requires nonzero(host.cell) requires nonzero(host.make) requires nonzero(host.release);

extern "C" usize contract_not(address context,usize root,ContractNotHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.lookup) requires nonzero(host.remember) requires nonzero(host.load) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.emit) requires nonzero(host.make) requires nonzero(host.release);

extern "C" usize contract_fact(address context,FactView input,ContractSemanticHost host) on cpu
 requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain);

extern "C" u32 contract_unary(FactView fact,cfn<InternalPredicate,address> ensures memory(read) predicate,cfn<ContractDomain,address> ensures memory(read) domain) on cpu
 requires nonzero(predicate) requires nonzero(domain);

extern "C" u32 contract_all_unary(address context,cfn<FactView,address> next,cfn<InternalPredicate,address> ensures memory(read) predicate,cfn<ContractDomain,address> ensures memory(read) domain) on cpu
 requires nonzero(next) requires nonzero(predicate) requires nonzero(domain);

extern "C" void contract_operands(address context,u32 either,ContractOperandsHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.seen) requires nonzero(host.imported) requires nonzero(host.all_unary) requires nonzero(host.emit);

extern "C" usize contract_import(address context,address root,ContractImportHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.visit) requires nonzero(host.start) requires nonzero(host.resume) requires nonzero(host.lookup) requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.predicate) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.operation) requires nonzero(host.operand) requires nonzero(host.remember);

extern "C" void contract_emit(address context,usize root,ContractEmitHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.known) requires nonzero(host.load) requires nonzero(host.variable) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.bound) requires nonzero(host.guard) requires nonzero(host.atom) requires nonzero(host.negate) requires nonzero(host.guard_clause) requires nonzero(host.child) requires nonzero(host.both) requires nonzero(host.first) requires nonzero(host.either) requires nonzero(host.remember) requires nonzero(host.release);

extern "C" void contract_union(address context,ContractUnionHost host) on cpu
 requires nonzero(host.case) requires nonzero(host.import) requires nonzero(host.operation) requires nonzero(host.emit);

struct RelationAffine {u32 present;address source;IntegerWide scale;IntegerWide offset;};
struct RelationRange {u32 present;IntegerWide lower;IntegerWide upper;};
struct RelationExpression {u32 present;u32 operation;address scalar;InternalOperand left;InternalOperand right;address definition;};
struct RelationIntervalHost {cfn<RelationRange,address,address> ensures memory(read) bounds;};
struct RelationOperandHost {
 cfn<RelationAffine,address,address> cache;
 cfn<address,address,address> root;
 cfn<u32,address,address> enter;
 cfn<RelationRange,address,address> ensures memory(read) bounds;
 cfn<void,address,address> begin;
 cfn<FactView,address> next;
 cfn<RelationExpression,address> ensures memory(read) computed;
 cfn<RelationAffine,address,address> expression;
 cfn<void,address> end;
 cfn<void,address,address> leave;
 cfn<void,address,address,RelationAffine> remember;
 cfn<i32,address,address> ensures memory(read) compare;
};
struct RelationExpressionHost {
 cfn<RelationAffine,address,u32> operand;
 cfn<RelationRange,address,RelationAffine> interval;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 cfn<i32,address,address> ensures memory(read) compare;
};
struct RelationRetainHost {
 cfn<RelationRange,address> ensures memory(read) bounds;
 cfn<void,address,u32,u64> emit;
};
struct RelationQuotientHost {
 cfn<RelationRange,address,address> ensures memory(read) bounds;
 cfn<RelationRange,address,InternalOperand> ensures memory(read) operand_bounds;
 cfn<u32,address,address,IntegerWide,IntegerWide> retain;
};
struct RelationAtMostHost {cfn<u32,address,address,IntegerWide,IntegerWide> retain;};
struct RelationFactsHost {
 cfn<FactView,address> next;
 cfn<InternalScalar,address> ensures memory(read) scalar;
 cfn<RelationExpression,address> ensures memory(read) computed;
 cfn<RelationAffine,address,u32,InternalOperand,address> operand;
 cfn<RelationAffine,address,address,address> expression;
 cfn<u32,address,address,RelationAffine> at_most;
 cfn<u32,address,address,address,InternalOperand,InternalOperand> quotient;
 cfn<address,address,address> root;
 cfn<RelationRange,address,RelationAffine,address> interval;
 cfn<u32,address,address,address,IntegerWide,IntegerWide> retain;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<void,address> finish;
};

extern "C" RelationRange relation_interval(address context,RelationAffine value,RelationIntervalHost host) on cpu
 requires nonzero(host.bounds);

extern "C" RelationAffine relation_operand(address context,InternalOperand operand,InternalScalar info,RelationOperandHost host) on cpu
 requires nonzero(host.cache) requires nonzero(host.root) requires nonzero(host.enter) requires nonzero(host.bounds) requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.computed) requires nonzero(host.expression) requires nonzero(host.end) requires nonzero(host.leave) requires nonzero(host.remember) requires nonzero(host.compare);

extern "C" RelationAffine relation_expression(address context,RelationExpression expression,address scalar,InternalScalar info,RelationExpressionHost host) on cpu
 requires nonzero(host.operand) requires nonzero(host.interval) requires nonzero(host.same_scalar) requires nonzero(host.compare);

extern "C" u32 relation_retain(address context,IntegerWide lower,IntegerWide upper,InternalScalar info,RelationRetainHost host) on cpu
 requires nonzero(host.bounds) requires nonzero(host.emit);

extern "C" u32 relation_quotient(address context,address id,InternalOperand numerator,InternalOperand divisor,InternalScalar info,RelationQuotientHost host) on cpu
 requires nonzero(host.bounds) requires nonzero(host.operand_bounds) requires nonzero(host.retain);

extern "C" u32 relation_at_most_zero(address context,RelationAffine value,InternalScalar info,RelationAtMostHost host) on cpu
 requires nonzero(host.retain);

extern "C" u32 relation_facts(address context,RelationFactsHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.scalar) requires nonzero(host.computed) requires nonzero(host.operand) requires nonzero(host.expression) requires nonzero(host.at_most) requires nonzero(host.quotient) requires nonzero(host.root) requires nonzero(host.interval) requires nonzero(host.retain) requires nonzero(host.compare) requires nonzero(host.finish);

struct RelationSourceWork {u32 present;u32 ready;InternalOperand operand;};
struct RelationSourcePlanHost {
 cfn<RelationSourceWork,address> pop;
 cfn<void,address,InternalOperand,u32> push;
 cfn<RelationRange,address,address> ensures memory(read) bounds;
 cfn<u32,address,address> enter;
 cfn<RelationExpression,address,address> computed;
 cfn<address,address,address> root;
 cfn<address,address,address> select;
 cfn<void,address,address> emit;
 cfn<void,address> clear;
 cfn<i32,address,address> ensures memory(read) compare;
};
struct RelationSourceBelowHost {
 cfn<void,address,address,BitRange> seed;
 cfn<address,address,u64> next;
 cfn<RelationRange,address,address> ensures memory(read) bounds;
 cfn<RelationExpression,address,address> computed;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 cfn<void,address,address,u64,u64> store;
};
extern "C" u32 relation_source_plan(address context,RelationSourcePlanHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.bounds) requires nonzero(host.enter) requires nonzero(host.computed) requires nonzero(host.root) requires nonzero(host.select) requires nonzero(host.emit) requires nonzero(host.clear) requires nonzero(host.compare);
extern "C" bool relation_source_below(address context, address left, address right, address source,
    RangeInput intervals, address scalar, InternalScalar info, RelationSourceBelowHost host) on cpu
    requires nonzero(intervals.get) requires nonzero(host.seed) requires nonzero(host.next)
    requires nonzero(host.bounds) requires nonzero(host.computed) requires nonzero(host.same_scalar)
    requires nonzero(host.store);

struct IndexFields {u32 present;u32 kind;address original;address payload;address first;address second;u64 distance;};
enum IndexOutput:u32 {Original=0;Data=1;Tile=2;View=3;ApplicationData=4;Upper=5;Shifted=6;Sum=7;Offset=8;Room=9;};
struct IndexBelowHost {cfn<IndexFields,address> next;cfn<void,address,IndexOutput,IndexFields> emit;};
struct LengthRelationHost {
 cfn<IndexFields,address,u32> next;
 cfn<u32,address,u32,address> ensures memory(read) origin;
 cfn<u32,address,IndexFields> ensures memory(read) resource;
 cfn<u32,address,IndexOutput,IndexFields,u32,u32> ensures memory(read) range;
 cfn<u32,address,address,address> ensures memory(read) sum;
};
extern "C" u32 proof_length_relation(address context,u32 strict,LengthRelationHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.origin) requires nonzero(host.resource)
 requires nonzero(host.range) requires nonzero(host.sum);


struct IndexBounds {u64 lower;u64 upper;};
struct IndexConstant {u32 present;u64 bits;};
struct ArithmeticIndexHost {
 cfn<void,address,u32> select;
 cfn<IndexFields,address> next;
 cfn<void,address,IndexOutput,IndexFields> emit;
 cfn<u32,address> ensures memory(read) same;
 cfn<IndexBounds,address,u32> ensures memory(read) bounds;
 cfn<IndexConstant,address,u32> ensures memory(read) constant;
 cfn<u32,address,u32> ensures memory(read) nonzero;
 cfn<u32,address> ensures memory(read) shift_zero;
 cfn<address,address,u32> identity;
 cfn<u32,address,u32,address> ensures memory(read) room;
 cfn<void,address,u32> scratch;
 cfn<u32,address,IndexOutput,IndexFields> ensures memory(read) contains;
};
extern "C" void index_domains_below(address context,u32 strict,IndexBelowHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.emit);
extern "C" void arithmetic_index_domains(address context,u32 operation,ArithmeticIndexHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.next) requires nonzero(host.emit) requires nonzero(host.same) requires nonzero(host.bounds) requires nonzero(host.constant) requires nonzero(host.nonzero) requires nonzero(host.shift_zero) requires nonzero(host.identity) requires nonzero(host.room) requires nonzero(host.scratch) requires nonzero(host.contains);

enum ComparisonOutput:u32 {Minimum=0;Maximum=1;NonZero=2;Equal=3;NotEqual=4;};
struct ComparisonDomain {u32 present;u32 common;address original;};
struct ComparisonCaseHost {
 cfn<void,address,u32,ComparisonOutput,u32,u64> emit;
 cfn<void,address,u32> select;
 cfn<ComparisonDomain,address> next;
 cfn<void,address,u32,u32,address> domain;
 cfn<void,address,u32,u32> finish;
};
enum ComparisonConstraintOutput:u32 {Below=0;Upper=1;Minimum=2;Maximum=3;};
struct ComparisonConstraintHost {
 cfn<void,address,ComparisonConstraintOutput,u64> emit;
 cfn<IndexFields,address> next;
 cfn<void,address,IndexOutput,IndexFields> domain;
};
struct CallComparisonRanges {
 address left;u64 left_count;address right;u64 right_count;
 cfn<BitRange,address,u64> ensures memory(read) get;
};
extern "C" bool comparison_call_below(address context,u32 strict,u32 same,InternalScalar info,
 CallComparisonRanges ranges,ComparisonConstraintHost host) on cpu
 requires nonzero(ranges.get) requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.domain);
struct ComparisonIntegerInfo {
 InternalScalar scalar;u64 left_count;u64 right_count;IndexBounds left;IndexBounds right;
 u32 same;u32 left_nonzero;u32 right_nonzero;
};
struct ComparisonIntegerHost {
 ComparisonCaseHost cases;
 cfn<IndexBounds,address,u32,u64> ensures memory(read) range;
 cfn<void,address,address,u32,u32,u32> constrain;
 cfn<void,address,address,u32,u32,ComparisonIntegerInfo> ordered;
 cfn<void,address> refine;
};
struct ComparisonFloatHost {ComparisonCaseHost cases;cfn<void,address,u32,BinaryOperation> predicate;};
struct ComparisonBooleanHost {cfn<void,address,u32,u32,u32> pair;cfn<void,address,u32> finish;};
extern "C" void comparison_constraint(address context,u32 strict,InternalScalar info,IndexBounds left,IndexBounds right,ComparisonConstraintHost host) on cpu
 requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.domain);
extern "C" void comparison_integer_case(address state,address input,u32 operation,u32 truth,ComparisonIntegerInfo info,ComparisonIntegerHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.range) requires nonzero(host.constrain);
extern "C" u32 comparison_integer(address state,address input,u32 operation,ComparisonIntegerInfo info,ComparisonIntegerHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.range) requires nonzero(host.constrain) requires nonzero(host.ordered) requires nonzero(host.refine);
extern "C" u32 comparison_float(address state,u32 operation,IndexBounds finite,ComparisonFloatHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.predicate);
extern "C" u32 comparison_boolean(address state,u32 operation,ComparisonBooleanHost host) on cpu
 requires nonzero(host.pair) requires nonzero(host.finish);

struct ValueConditionInput {InternalScalar scalar;IndexConstant constant;u32 base_cases;};
enum ValueConditionOutput:u32 {BooleanFalse=0;BooleanTrue=1;Zero=2;NonZero=3;Minimum=4;Maximum=5;};
enum ValueConditionFinish:u32 {Impossible=0;Base=1;Facts=2;};
struct ValueConditionHost {
 cfn<ComparisonDomain,address> next;
 cfn<void,address,u32,address> domain;
 cfn<IndexBounds,address> ensures memory(read) bounds;
 cfn<void,address,u32,ValueConditionOutput,u64> emit;
 cfn<void,address,u32,ValueConditionFinish> finish;
};
extern "C" void value_condition(address context,ValueConditionInput input,ValueConditionHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.domain) requires nonzero(host.bounds) requires nonzero(host.emit) requires nonzero(host.finish);

struct IndexRelationOperand {u32 kind;address identity;u64 bits;};
struct IndexRelationInput {IndexRelationOperand left;IndexRelationOperand right;u32 left_nonzero;u32 right_nonzero;};
enum IndexConditionOutput:u32 {Minimum=0;Maximum=1;Equal=2;NotEqual=3;};
enum IndexConditionFinish:u32 {Impossible=0;Empty=1;Facts=2;};
struct IndexConditionCaseHost {
 cfn<void,address,u32,IndexConditionOutput,address,address,u64> emit;
 cfn<void,address,u32,IndexConditionFinish> finish;
};
struct IndexRelationEqualHost {
 IndexConditionCaseHost cases;
 cfn<void,address,address,address> predicate;
 cfn<void,address> guard;
};
struct IndexRelationBelowHost {
 IndexConditionCaseHost cases;
 cfn<IndexBounds,address,u32> ensures memory(read) bounds;
 cfn<void,address,address,u32,u32,address,address,IndexBounds,IndexBounds,u32> constrain;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" void index_condition_equal(address state,IndexRelationOperand left,IndexRelationOperand right,IndexRelationEqualHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.finish) requires nonzero(host.predicate) requires nonzero(host.guard);
extern "C" void index_condition_below(address state,address input,IndexRelationInput values,u32 original_strict,IndexRelationBelowHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.finish) requires nonzero(host.bounds) requires nonzero(host.constrain) requires nonzero(host.compare);

struct ExecutionCase {address source;u32 truth;u32 original;};
struct ExecutionJoin {ExecutionCase left;ExecutionCase right;u32 any;};
struct ExecutionHost {
 cfn<void,address,address,u32> push;
 cfn<NegatePending,address> pop;
 cfn<u32,address,address> ensures memory(read) has_result;
 cfn<void,address,address> impossible;
 cfn<void,address,address> guard;
 cfn<void,address,address,ExecutionJoin,ExecutionJoin> compose;
};
extern "C" void execution_condition(address root,address context,ExecutionHost host) on cpu
 requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.has_result) requires nonzero(host.impossible) requires nonzero(host.guard) requires nonzero(host.compose);

struct ProofFactsInput {
 InternalScalar scalar;address scalar_ref;address primary;usize origins;usize ranges;
 u32 mode;u32 nonzero;u32 constant;u64 bits;u32 numeric_only;
};
struct ProofFactDomain {IndexFields fields;u32 known_bits;};
struct ProofFactsHost {
 cfn<address,address,usize> ensures memory(read) origin;
 cfn<IndexBounds,address,usize> ensures memory(read) range;
 cfn<void,address> rewind;cfn<ProofFactDomain,address> next;
 cfn<u32,address> boolean_cases;
 cfn<void,address,FactView> emit;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" void proof_facts(address context,ProofFactsInput input,ProofFactsHost host) on cpu
 requires nonzero(host.origin) requires nonzero(host.range) requires nonzero(host.rewind) requires nonzero(host.next)
 requires nonzero(host.boolean_cases) requires nonzero(host.emit) requires nonzero(host.compare);

struct OutputAliasHost {
 cfn<ProofFactsInput,address,usize,address> input;
 cfn<void,address> finish_values;
 cfn<address,address,usize> ensures memory(read) case_identity;
 cfn<void,address,usize,FactView,FactView> join_cases;
};
extern "C" void output_aliases(address context,usize components,usize conditions,address primary,OutputAliasHost host,ProofFactsHost facts) on cpu
 requires nonzero(host.input) requires nonzero(host.finish_values)
 requires nonzero(host.case_identity) requires nonzero(host.join_cases)
 requires nonzero(facts.origin) requires nonzero(facts.range) requires nonzero(facts.rewind) requires nonzero(facts.next)
 requires nonzero(facts.boolean_cases) requires nonzero(facts.emit) requires nonzero(facts.compare);

// The original fact and input proofs stay borrowed for this synchronous call.
struct CallFactInput {address original;address identity;u32 nonzero;};
struct CallFactRange {u64 lower;u64 upper;};
struct CallFactCase {address original;u32 empty;};
struct CallFactCommon {address source;address from;address low;address extended;address to;};
struct CallFactConverted {u64 lower;u64 upper;u32 contains_zero;u32 known;u64 value;};
struct CallFactOrigin {address original;u32 element;};
struct CallFactHost {
 cfn<CallFactInput,address,address> input;
 cfn<CallFactRange,address,u64,u32> range;
 cfn<u32,address> boolean;
 cfn<CallFactCase,address,u32> select_case;
 cfn<void,address,address> include;
 cfn<void,address,address> include_cases;
 cfn<void,address> impossible;
 cfn<void,address,FactView> emit;
 cfn<void,address> commit;cfn<usize,address> count;
 cfn<void,address,CallFactInput,CallFactInput,u32,address> below;
 cfn<address,address,address> mapped_domain;
 cfn<CallFactCommon,address> ensures memory(read) common;
 cfn<CallFactConverted,address,CallFactCommon> converted;
 cfn<void,address,address> begin;
 cfn<ComparisonDomain,address> next;
 cfn<address,address,address,CallFactCommon> cast_domain;
 cfn<CallFactOrigin,CallFactInput,usize> ensures memory(read) origin;
 cfn<void,address,address,address,CallFactCommon> emit_common;
 cfn<CallFactRange,address> ensures memory(read) singleton;
 cfn<void,address,InternalPredicate,u32> emit_predicate;
 cfn<InternalPredicate,address> ensures memory(read) inspect_predicate;
 cfn<InternalScalar,address> ensures memory(read) scalar;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" void call_fact(address context,FactView fact,CallFactHost host) on cpu
 requires nonzero(host.input) requires nonzero(host.range) requires nonzero(host.boolean)
 requires nonzero(host.select_case) requires nonzero(host.include) requires nonzero(host.include_cases)
 requires nonzero(host.impossible) requires nonzero(host.emit) requires nonzero(host.commit) requires nonzero(host.count)
 requires nonzero(host.below) requires nonzero(host.mapped_domain) requires nonzero(host.common) requires nonzero(host.converted)
 requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.cast_domain) requires nonzero(host.origin)
 requires nonzero(host.emit_common) requires nonzero(host.singleton) requires nonzero(host.emit_predicate)
 requires nonzero(host.inspect_predicate) requires nonzero(host.scalar) requires nonzero(host.compare);
void call_fact_domains(address context,CallFactHost host,CallFactInput input,address target,FactView key,u32 mode,CallFactCommon common) on cpu
 requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.cast_domain) requires nonzero(host.emit);

enum OutputStatus : u32 { Pending=0; Original=1; Replaced=2; };
enum OutputBuild : u32 { Original=0; Facts=1; Both=2; Either=3; Branch=4; MappedBranch=5; };
struct OutputRenameHost {
 cfn<address,address> root;
 cfn<void,address,address,u32> push;
 cfn<NegatePending,address> pop;
 cfn<OutputStatus,address,address> ensures memory(read) status;
 cfn<void,address,address> begin_facts;
 cfn<u32,address> map_fact;
 cfn<u32,address,address> ensures memory(read) same_facts;
 cfn<void,address,address,SourceNode,OutputBuild> build;
 cfn<void,address,address> emit;
};
extern "C" void rename_output_graph(address context,OutputRenameHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.status) requires nonzero(host.begin_facts)
 requires nonzero(host.map_fact) requires nonzero(host.same_facts)
 requires nonzero(host.build) requires nonzero(host.emit);

struct AggregateProofHost {
 cfn<void,address,usize> project;
 cfn<void,address> relate;
 cfn<u32,address> required;
 cfn<void,address,usize> refine;
 cfn<void,address> join;
};
extern "C" void select_aggregate_proof(address context,usize choices,usize components,u32 publish,AggregateProofHost host) on cpu
 requires nonzero(host.project) requires nonzero(host.relate) requires nonzero(host.required)
 requires nonzero(host.refine) requires nonzero(host.join);


struct PathProofComponent { u32 existing; u32 boolean; u32 integer; };
struct PathProofHost {
 cfn<PathProofComponent,address,usize> component;
 cfn<address,address,usize> identity;
 cfn<void,address,usize> merge;
 cfn<void,address,usize> share;
 cfn<void,address,usize,address> boolean;
 cfn<void,address,usize,address> integer;
};
extern "C" void bind_proof_path(address context,usize count,PathProofHost host) on cpu
 requires nonzero(host.component) requires nonzero(host.identity) requires nonzero(host.merge)
 requires nonzero(host.share) requires nonzero(host.boolean) requires nonzero(host.integer);


enum ProofDomainKind:u32 { Other=0; FixedLength=1; ResourceLength=2; KnownBits=3; Numeric=4; };
struct ProofDomainView { ProofDomainKind kind; u64 value; };
struct ProofDomainHost {
 cfn<ProofDomainView,address> describe;
 cfn<void,address> nonzero;
 cfn<void,address,u64> bounded;
 cfn<void,address,address> retain;
};
struct ProofRefinementHost {
 ProofDomainHost domains;
 cfn<ScalarEncoding,address> scalar;
 cfn<void,address,u32> deny;
 cfn<void,address,u64,u64> range;
};
extern "C" void bind_proof_domain(address context,address domain,ProofDomainHost host) on cpu
 requires nonzero(host.describe) requires nonzero(host.nonzero) requires nonzero(host.bounded) requires nonzero(host.retain);
extern "C" void refine_proof_fact(address context,ScalarEncoding scalar,u64 zero,FactView fact,ProofRefinementHost host) on cpu
 requires nonzero(host.domains.describe) requires nonzero(host.domains.nonzero)
 requires nonzero(host.domains.bounded) requires nonzero(host.domains.retain)
 requires nonzero(host.scalar) requires nonzero(host.deny) requires nonzero(host.range);


struct ConditionJoinHost {
 cfn<u32,address,usize> present;
 cfn<void,address> empty;
 cfn<void,address,usize> common;
 cfn<void,address,usize> selected;
};
extern "C" void join_proof_conditions(address context,usize width,u32 selected,ConditionJoinHost host) on cpu
 requires nonzero(host.present) requires nonzero(host.empty) requires nonzero(host.common) requires nonzero(host.selected);

struct NumericJoinFacts { u32 left_nonzero; u32 right_nonzero; u32 bits_nonzero; };
struct NumericJoinHost {
 cfn<NumericJoinFacts,address,usize> domains;
 cfn<void,address,usize,u32> store;
};
extern "C" void join_proof_numeric(address context,usize width,NumericJoinHost host) on cpu
 requires nonzero(host.domains) requires nonzero(host.store);

struct ProofCallWidths { usize origins; usize domains; usize conditions; };
struct ProofCallDomain { address original; u32 kind; };
struct ProofCallHost {
 cfn<void,address,usize,u32,u32> begin;
 cfn<CallFactInput,address> input;
 cfn<void,address,address> merge_input;
 cfn<void,address,address> keep_origin;
 cfn<ProofCallDomain,address> domain;
 cfn<void,address,address> direct;
 cfn<void,address,address> common;
 cfn<void,address,address> derived;
 cfn<address,address> false_case;
 cfn<u32,address> possible;
 cfn<void,address> nonzero;
};
extern "C" void project_call_proof(address context,ProofCallWidths widths,ProofCallHost host) on cpu
 requires nonzero(host.begin) requires nonzero(host.input) requires nonzero(host.merge_input) requires nonzero(host.keep_origin)
 requires nonzero(host.domain) requires nonzero(host.direct) requires nonzero(host.common) requires nonzero(host.derived)
 requires nonzero(host.false_case) requires nonzero(host.possible) requires nonzero(host.nonzero);

struct CaptureProofHost {
 cfn<void,address> all;
 cfn<void,address,usize,u32> component;
};
extern "C" void capture_selected_proof(address context,address indices,usize count,
 usize width,usize registers,CaptureProofHost host) on cpu
 requires nonzero(host.all) requires nonzero(host.component);

struct ProofOutputShape { usize addresses; usize views; usize origins; usize domains; };
struct ProofOutputHost {
 cfn<void,address,usize> bind_address;
 cfn<void,address,usize> bind_view;
 cfn<void,address,usize> origins;
 cfn<void,address,usize> domains;
 cfn<void,address> conditions;
};
extern "C" void bind_output_proof(address context,ProofOutputShape shape,ProofOutputHost host) on cpu
 requires nonzero(host.bind_address) requires nonzero(host.bind_view)
 requires nonzero(host.origins) requires nonzero(host.domains) requires nonzero(host.conditions);
