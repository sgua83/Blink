module bootstrap.condition_graph;
import bootstrap.scalar_fold;

// These descriptors borrow the original Clause query and its physical buffers.
// They are private native fields, not a stored condition or public graph format.
struct Span { address data; usize count; };
struct SourceNode { u32 kind; address left; address right; address no; u32 truth; u32 empty; address condition; };
struct Gate { usize left; usize right; u32 kind; Span parents; };
struct Graph { address nodes; usize stride; address values; };
struct Assignment { u32 present; usize node; u32 truth; };
struct Checkpoint { usize nodes; usize facts; usize negative; };
struct Alternative { u32 kind; usize node; u32 truth; Checkpoint checkpoint; };
struct Host {
 cfn<usize,address,address> index;
 cfn<usize,address> ensures memory(read) node_count;
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
 cfn<void,address,usize,u32,usize,usize> define;
 cfn<void,address,usize,usize> parent_push;
 cfn<Graph,address> prepare;
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
 cfn<FactView,address> ensures memory(read) inspect_fact;
 cfn<void,address,FactView> set_next_fact;
 FactHost facts;
 cfn<usize,address> ensures memory(read) remaining;
 cfn<void,address,usize,u32,Checkpoint> push_clause;
 cfn<void,address,address,Checkpoint> push_fact;
 cfn<Alternative,address> pop_alternative;
};
extern "C" u32 overlap(address left,address right,u32 right_truth,address context,Host host) on cpu
 requires nonzero(host.index) requires nonzero(host.node_count) requires nonzero(host.inspect)
 requires nonzero(host.define) requires nonzero(host.parent_push) requires nonzero(host.prepare)
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.inspect_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative);
extern "C" u32 equivalent(address left,address right,address context,Host host,cfn<void,address> reset) on cpu
 requires nonzero(host.index) requires nonzero(host.node_count) requires nonzero(host.inspect)
 requires nonzero(host.define) requires nonzero(host.parent_push) requires nonzero(host.prepare)
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.inspect_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative)
 requires nonzero(reset);

// Original required-facts query nodes; the result stays in its owning container.
struct RequiredNode { usize left; usize right; u32 kind; usize users; usize remaining; };
struct RequiredGraph { address nodes; usize stride; usize count; };
struct RequiredPending { u32 present; usize index; };
struct RequiredBuildHost {
 cfn<usize,address,address> index;
 cfn<usize,address> ensures memory(read) count;
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
 cfn<void,address,usize,u32,usize,usize> define;
};
struct RequiredHost {
 cfn<void,address,usize> reset;
 cfn<u32,address,usize> ensures memory(read) status;
 cfn<void,address,usize> push;
 cfn<RequiredPending,address> pop;
 cfn<usize,address> ensures memory(read) pending_count;
 cfn<void,address,usize> impossible;
 cfn<void,address,usize> leaf;
 cfn<void,address,usize,u32,u32> union;
 cfn<void,address,usize,u32,u32> common;
 cfn<u32,address,usize> ensures memory(read) valid_additions;
 cfn<u32,address,usize,u32> release;
};
extern "C" void required_build(address root,address context,RequiredBuildHost host) on cpu
 requires nonzero(host.index) requires nonzero(host.count)
 requires nonzero(host.inspect) requires nonzero(host.define);
extern "C" void required_evaluate(RequiredGraph graph,address context,RequiredHost host) on cpu
 requires nonzero(host.reset) requires nonzero(host.status) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.pending_count) requires nonzero(host.impossible)
 requires nonzero(host.leaf) requires nonzero(host.union) requires nonzero(host.common)
 requires nonzero(host.valid_additions) requires nonzero(host.release);

enum FactKind : u32 { Equal=1;Index=2;Minimum=3;Maximum=4;NonZero=5;Boolean=6;Below=7;Outside=8;Zero=9;NotEqual=10;Predicate=11; };
struct FactView { u32 present; FactKind kind; address first; address second; address payload; address scalar; u64 value; u64 mask; u32 flag; u32 fixed; };
struct CrossScalars { array<address,4> values; };
struct CrossHost {
 cfn<usize,address,u32> ensures memory(read) count;
 cfn<void,address,u32> begin;
 cfn<FactView,address> next;
 cfn<u32,address,u32,FactView> ensures memory(read) contains;
 cfn<u32,address,u32,FactView,FactView,u32,u32> ensures memory(read) range;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" u32 cross_conflict(address context,CrossScalars scalars,CrossHost host,u32 may_share) on cpu
 requires nonzero(host.count) requires nonzero(host.begin) requires nonzero(host.next)
 requires nonzero(host.contains) requires nonzero(host.range) requires nonzero(host.compare);

struct FactHost {
 address unsigned32;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" FactView negated_fact(FactView value,FactHost host) on cpu
 requires nonzero(host.compare);


struct NegatePending { u32 present; u32 ready; address source; };
struct NegateHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
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
 requires nonzero(host.inspect) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.has_result) requires nonzero(host.unconstrained) requires nonzero(host.impossible)
 requires nonzero(host.begin_facts) requires nonzero(host.next_fact) requires nonzero(host.either_fact)
 requires nonzero(host.either) requires nonzero(host.both) requires nonzero(host.facts.compare);

struct InternalScalar { u32 kind; u32 width; u64 mask; u64 bias; };
struct InternalOperand { address identity; u64 bits; address scalar; u32 is_constant; };
struct InternalPredicate { address scalar; BinaryOperation operation; InternalOperand left; InternalOperand right; };
struct InternalUpper { address identity; u32 distance; u32 present; };
struct InternalHost {
 cfn<FactView,address> next;
 cfn<FactView,address,FactView,u32> ensures memory(read) select;
 cfn<u32,address,FactView> ensures memory(read) contains;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 cfn<InternalScalar,address> ensures memory(read) scalar;
 cfn<InternalPredicate,address> ensures memory(read) predicate;
 cfn<InternalUpper,address> ensures memory(read) upper;
};
extern "C" u32 internal_conflict(address context,CrossScalars integers,InternalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.select) requires nonzero(host.contains)
 requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar)
 requires nonzero(host.predicate) requires nonzero(host.upper);

struct CommonRoot { u32 present; u32 matched; address identity; address left; address right; };
struct CommonKnown { address scalar; u32 present; };
struct CommonBits { u64 mask; u64 value; u32 present; };
struct CommonLimit { FactView value; FactView next; };
struct CommonHost {
 cfn<void,address> intersection;
 cfn<CommonRoot,address> root_next;
 cfn<address,address,address,address,address> representative;
 cfn<u32,address,FactView> ensures memory(read) contains;
 cfn<void,address,FactView> emit;
 cfn<FactView,address> union_next;
 cfn<CommonKnown,address> ensures memory(read) known;
 cfn<CommonBits,address,u32,address,address> ensures memory(read) known_bits;
 cfn<u32,address,address,address,u64,u64> ensures memory(read) contains_known;
 cfn<void,address,address,address,u64,u64> emit_known;
 cfn<FactView,address,u32> side_next;
 cfn<u32,address,FactView,FactView> ensures memory(read) range;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<i32,address,address> ensures memory(read) scalar_compare;
 cfn<InternalScalar,address> ensures memory(read) scalar;
};
extern "C" void common_facts(address context,CommonHost host) on cpu
 requires nonzero(host.intersection) requires nonzero(host.root_next) requires nonzero(host.representative)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.union_next)
 requires nonzero(host.known) requires nonzero(host.known_bits) requires nonzero(host.contains_known)
 requires nonzero(host.emit_known) requires nonzero(host.side_next) requires nonzero(host.range)
 requires nonzero(host.compare) requires nonzero(host.scalar_compare) requires nonzero(host.scalar);

// The original synchronous task-return query owns all sets and node results.
struct TaskSourceSet { u32 known; address values; };
struct TaskSourceHost {
 cfn<usize,address> ensures memory(read) count;
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
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
 requires nonzero(host.count) requires nonzero(host.inspect) requires nonzero(host.begin)
 requires nonzero(host.next) requires nonzero(host.contains) requires nonzero(host.lookup)
 requires nonzero(host.unknown) requires nonzero(host.empty) requires nonzero(host.single)
 requires nonzero(host.copy) requires nonzero(host.intersection) requires nonzero(host.union)
 requires nonzero(host.same_scalar) requires nonzero(host.facts.compare);

// Synchronous typed Clause outputs and the original container primitives.
struct ClauseLeafHost {
 cfn<u32,address> conflict;
 cfn<void,address> impossible;
 cfn<void,address,address> make;
};
struct ClauseBothHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<u32,address,address> ensures memory(read) same;
 cfn<void,address,address,address> unite;
 cfn<void,address> impossible;
 cfn<void,address,address> copy;
 cfn<void,address,address,address> make;
};
struct ClauseEitherHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<u32,address,address> ensures memory(read) same;
 cfn<u32,address,address> ensures memory(read) same_condition;
 cfn<u32,address,address> ensures memory(read) subset;
 cfn<void,address,address> copy;
 cfn<void,address,address,address> make;
 cfn<void,address,address,address,address> factor;
};
struct ClauseBranchHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<address,address,u32> ensures memory(read) case;
 cfn<void,address> impossible;
 cfn<void,address,address,address,u32> make;
};
extern "C" void clause_facts(address facts,address output,ClauseLeafHost host) on cpu
 requires nonzero(host.conflict) requires nonzero(host.impossible) requires nonzero(host.make);
extern "C" void clause_both(address left,address right,address output,ClauseBothHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.same) requires nonzero(host.unite)
 requires nonzero(host.impossible) requires nonzero(host.copy) requires nonzero(host.make);
extern "C" void clause_either(address left,address right,address output,ClauseEitherHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.same) requires nonzero(host.same_condition)
 requires nonzero(host.subset) requires nonzero(host.copy) requires nonzero(host.make) requires nonzero(host.factor);
extern "C" void clause_branch(address parent,address condition,u32 truth,address output,ClauseBranchHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.case) requires nonzero(host.impossible) requires nonzero(host.make);

struct EqualityLookup { u32 present; address root; };
struct EqualityIdentityHost {
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<void,address,address> assign;
};
struct EqualityHost {
 cfn<FactView,address> next;
 cfn<EqualityLookup,address,u32,address> lookup;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<void,address,address,address> merge;
 cfn<void,address,address,address> insert;
};
extern "C" void equality_merge_value(address current,address merged,address root,EqualityIdentityHost host) on cpu
 requires nonzero(host.compare) requires nonzero(host.assign);
extern "C" u32 equality_extend(address context,EqualityHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.compare)
 requires nonzero(host.merge) requires nonzero(host.insert);
struct ComputedEqualityStep { u32 present; address result; };
struct ComputedEqualityHost {
 cfn<FactView,address> next;
 cfn<address,address> ensures memory(read) expression;
 cfn<void,address,address,address> collect;
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
 requires nonzero(host.next) requires nonzero(host.expression) requires nonzero(host.collect)
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
 cfn<address,address,FactView,address> key;
 cfn<u32,address,u32,address> ensures memory(read) contains;
 cfn<void,address,address> emit;
 cfn<u32,address> ensures memory(read) has_inferred;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" u32 alias_facts(address context,usize members,u32 inherited,AliasFactHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.previous)
 requires nonzero(host.group) requires nonzero(host.member) requires nonzero(host.key)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.has_inferred)
 requires nonzero(host.compare);

// Original typed source-path terms remain owned by the synchronous native caller.
struct CanonicalTerm { u32 present; address value; };
enum CanonicalInsertion : u32 { Reject=0; Insert=1; RejectPrepared=2; RetainInsert=3; };
struct CanonicalInsertHost {
 cfn<CanonicalTerm,address> next;
 cfn<u32,address,address> ensures memory(read) subset;
 cfn<void,address> prepare;
 cfn<void,address> reset;
 cfn<u32,address,address> forward;
};
extern "C" CanonicalInsertion canonical_insert(address context,address candidate,u32 empty,CanonicalInsertHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.subset) requires nonzero(host.prepare)
 requires nonzero(host.reset) requires nonzero(host.forward);
struct CanonicalKeepHost {
 cfn<u32,address,address> ensures memory(read) subset;
 cfn<u32,address,address> reverse;
};
extern "C" u32 canonical_keep(address old,address candidate,address incoming,CanonicalKeepHost host) on cpu
 requires nonzero(host.subset) requires nonzero(host.reverse);
struct CanonicalNormalHost {
 cfn<FactView,address> next;
 cfn<void,address,u32> reset;
 cfn<FactView,address,u32> bound;
 cfn<u32,address,FactView> ensures memory(read) contains;
 cfn<void,address,FactView> emit;
 cfn<i32,address,address> ensures memory(read) compare;
 cfn<InternalScalar,address> ensures memory(read) scalar;
 cfn<InternalUpper,address> ensures memory(read) upper;
};
extern "C" void canonical_normalize(address context,CanonicalNormalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reset) requires nonzero(host.bound)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.compare)
 requires nonzero(host.scalar) requires nonzero(host.upper);
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
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
 cfn<address,address,address> ensures memory(read) lookup;
 cfn<void,address,usize> empty;
 cfn<void,address,usize,address> leaf;
 cfn<void,address,usize,address,address> either;
 cfn<void,address,usize,address,address> both;
};
extern "C" void canonical_graph(address context,CanonicalGraphHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.inspect) requires nonzero(host.lookup)
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
struct ContractPriorityHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<u32,address> all_unary;
};
struct ContractOperandsHost {
 cfn<address,address> pop;
 cfn<void,address,address> push;
 cfn<u32,address,address> seen;
 cfn<u32,address,address> ensures memory(read) imported;
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<void,address,address> emit;
};
struct ContractImportHost {
 cfn<ContractImport,address> pop;
 cfn<void,address,address> visit;
 cfn<void,address,address,u32,usize> start;
 cfn<void,address,address,u32,usize,usize> resume;
 cfn<ContractIndex,address,address> ensures memory(read) lookup;
 cfn<SourceNode,address> ensures memory(read) inspect;
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
 cfn<void,address> guard;
 cfn<void,address,FactView> atom;
 cfn<void,address> negate;
 cfn<SourceNode,address> ensures memory(read) guard_node;
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

extern "C" u32 contract_priority(address source,ContractPriorityHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.all_unary);

extern "C" void contract_operands(address context,u32 either,ContractOperandsHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.seen) requires nonzero(host.imported) requires nonzero(host.inspect) requires nonzero(host.emit);

extern "C" usize contract_import(address context,address root,ContractImportHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.visit) requires nonzero(host.start) requires nonzero(host.resume) requires nonzero(host.lookup) requires nonzero(host.inspect) requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.predicate) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.operation) requires nonzero(host.operand) requires nonzero(host.remember);

extern "C" void contract_emit(address context,usize root,ContractEmitHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.known) requires nonzero(host.load) requires nonzero(host.variable) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.bound) requires nonzero(host.guard) requires nonzero(host.atom) requires nonzero(host.negate) requires nonzero(host.guard_node) requires nonzero(host.child) requires nonzero(host.both) requires nonzero(host.first) requires nonzero(host.either) requires nonzero(host.remember) requires nonzero(host.release);

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
 cfn<void,address,address,u64,u64> seed;
 cfn<address,address> next;
 cfn<RelationRange,address,address> ensures memory(read) bounds;
 cfn<RelationExpression,address,address> computed;
 cfn<u32,address,address> ensures memory(read) same_scalar;
 cfn<RelationRange,address,u32,RelationRange,RelationRange,u64,u64> arithmetic;
 cfn<void,address,address,u64,u64> store;
};
extern "C" u32 relation_source_plan(address context,RelationSourcePlanHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.bounds) requires nonzero(host.enter) requires nonzero(host.computed) requires nonzero(host.root) requires nonzero(host.select) requires nonzero(host.emit) requires nonzero(host.clear) requires nonzero(host.compare);
extern "C" u32 relation_source_below(address context,address left,address right,address source,u64 lower,u64 upper,address scalar,InternalScalar info,RelationSourceBelowHost host) on cpu
 requires nonzero(host.seed) requires nonzero(host.next) requires nonzero(host.bounds) requires nonzero(host.computed) requires nonzero(host.same_scalar) requires nonzero(host.arithmetic) requires nonzero(host.store);

struct IndexFields {u32 present;u32 kind;address original;address payload;address first;address second;u64 distance;};
enum IndexOutput:u32 {Original=0;Data=1;Tile=2;View=3;ApplicationData=4;Upper=5;Shifted=6;Sum=7;Offset=8;Room=9;};
struct IndexBelowHost {cfn<IndexFields,address> next;cfn<void,address,IndexOutput,IndexFields> emit;};
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

enum NumericBoundSide:u32 {Lower=0;Upper=1;};
struct NumericBoundPrevious {u32 present;u64 value;};
struct NumericBoundsHost {
 cfn<FactView,address> next;
 cfn<address,address,address> ensures memory(read) root;
 cfn<NumericBoundPrevious,address,NumericBoundSide,address,address> ensures memory(read) lookup;
 cfn<void,address,NumericBoundSide,address,address,u64> emit;
};
extern "C" void numeric_bounds(address context,NumericBoundsHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.lookup) requires nonzero(host.emit);

struct ExecutionCase {address source;u32 truth;u32 original;};
struct ExecutionJoin {ExecutionCase left;ExecutionCase right;u32 any;};
struct ExecutionHost {
 cfn<SourceNode,address> ensures memory(read) inspect;
 cfn<void,address,address,u32> push;
 cfn<NegatePending,address> pop;
 cfn<u32,address,address> ensures memory(read) has_result;
 cfn<void,address,address> impossible;
 cfn<void,address,address> guard;
 cfn<void,address,address,ExecutionJoin,ExecutionJoin> compose;
};
extern "C" void execution_condition(address root,address context,ExecutionHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.push) requires nonzero(host.pop)
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
