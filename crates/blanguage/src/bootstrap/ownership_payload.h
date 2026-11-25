module bootstrap.ownership_payload;
struct Info {u64 count;u32 flags;};
struct Item {address node;u32 kind;};
struct Field {address ty;u32 offset;u32 owns;};
struct Indices {address values;u64 count;};
struct Count {u32 known;u32 value;};
enum PayloadState:u32 { Moved=0; Owned=1; Borrowed=2; Conditional=3; };
struct Host {
    cfn<Info,address> inspect;
    cfn<address,address> value;
    cfn<address,address> ownership;
    cfn<Item,address,u64> child;
    cfn<Field,address,u64> field;
    cfn<u32,address,u64> field_owned;
    cfn<u64,address> aggregate_count;
    cfn<u64,address> component_count;
    cfn<u32,address,u64> component_word;
    cfn<u64,address,u32> component_index;
    cfn<u64,Indices,u64> index_at;
    address state_context;
    cfn<PayloadState,address,address> state;
    cfn<u32,address,address> state_equal;
    cfn<void,address> state_set_owned;
    cfn<void,address> state_set_borrowed;
    cfn<void,address> state_set_moved;
    cfn<void,address,address,address> proof_join_stored;
    cfn<void,address,address,address> proof_join_common;
    cfn<u32,address> uniform;
    cfn<void,address,u32> set_uniform;
    cfn<u32,address,address> constant_equal;
    cfn<void,address> clear_constant;
    cfn<void,address> proof_borrow;
    cfn<void,address> proof_clear;
    cfn<void,address,Indices> proof_assign;
    cfn<void,address,u32> length_constant;
    cfn<u32,address> view_empty;
    cfn<void,address> install_source;
    cfn<void,address,address,u64,u32,address,address,u32> join_child;
    cfn<void,address,address,address> join_ownership;
    cfn<void,address,address,u64> move_child;
    cfn<address,address> data_root;
    cfn<address,address> data_length;
    cfn<address,address> data_ownership;
    cfn<Count,address> data_count;
    cfn<void,address> data_read;
    cfn<void,address> data_clear_residency;
    cfn<u32,address,address> data_identity;
    cfn<u32,address,address,address,address> ownership_loop;
    cfn<address,address,address,address> loop_path;
    cfn<u32,address> path_possible;
    cfn<address> fresh_indices;
    cfn<void,address,u64> push_index;
    cfn<Indices,address> indices_view;
    cfn<void,address> drop_indices;
};

extern "C" void visit_values(Item item,address context,cfn<void,address,address> visit,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.value) requires nonzero(host.child)
 requires nonzero(host.data_length) requires nonzero(visit);

extern "C" u32 available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state)
    requires nonzero(host.data_ownership);
extern "C" u32 all_available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state)
    requires nonzero(host.data_ownership);
extern "C" void borrow(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_set_borrowed)
    requires nonzero(host.view_empty)
    requires nonzero(host.install_source)
    requires nonzero(host.data_root)
    requires nonzero(host.data_ownership)
    requires nonzero(host.data_read);
extern "C" u32 incomplete(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state)
    requires nonzero(host.data_ownership);
enum InitialWrite:u32 { Ready=0; IncompleteShape=1; Unavailable=2; };
extern "C" InitialWrite initial_write(Item item,u32 shape,Host host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.ownership) requires nonzero(host.child)
    requires nonzero(host.field_owned) requires nonzero(host.state) requires nonzero(host.data_ownership);
extern "C" u32 initial_root_incomplete(Item item,Host host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.ownership) requires nonzero(host.child)
    requires nonzero(host.field_owned) requires nonzero(host.state) requires nonzero(host.data_ownership);
struct LoopCursor {address before;address after;address path;u64 count;u64 index;};
struct LoopNeed {u32 kind;u32 value;address before;address after;};
// kind=0 is the actual final value; kind=3/4 asks for the original Data/Owned child.
extern "C" LoopNeed loop_start(address state,address before,address after,address context,address path,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.ownership_loop)
    requires nonzero(host.loop_path)
    requires nonzero(host.path_possible);
extern "C" LoopNeed loop_step(address state,u32 compatible,Host host) on cpu
    requires nonzero(host.child);
extern "C" void invalidate(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.child)
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear)
    requires nonzero(host.length_constant)
    requires nonzero(host.data_root)
    requires nonzero(host.data_length)
    requires nonzero(host.data_count);
extern "C" void invalidate_components(address value,Indices components,address borrowed,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.child)
    requires nonzero(host.field)
    requires nonzero(host.aggregate_count)
    requires nonzero(host.component_count)
    requires nonzero(host.component_word)
    requires nonzero(host.component_index)
    requires nonzero(host.index_at)
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear)
    requires nonzero(host.proof_assign)
    requires nonzero(host.length_constant)
    requires nonzero(host.data_root)
    requires nonzero(host.data_length)
    requires nonzero(host.data_count)
    requires nonzero(host.data_identity)
    requires nonzero(host.fresh_indices)
    requires nonzero(host.push_index)
    requires nonzero(host.indices_view)
    requires nonzero(host.drop_indices);
extern "C" void mark_moved(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_set_owned)
    requires nonzero(host.state_set_moved)
    requires nonzero(host.data_ownership)
    requires nonzero(host.data_clear_residency);
extern "C" void handles(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state)
    requires nonzero(visit);
struct DataPathHost {
    cfn<u64,Indices,u64> index_at;
    cfn<Item,address,u64> child;
    cfn<u32,Item> moved;
};
extern "C" address data_at(address value, Indices path, u32 live, DataPathHost host) on cpu
    requires nonzero(host.index_at)
    requires nonzero(host.child)
    requires nonzero(host.moved);

extern "C" void resources(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state)
    requires nonzero(visit);

struct Alternative { address owner; address choice; };
struct SelectionHost {
    cfn<Alternative,address,u64> alternative;
    cfn<u32,address> mode;
    cfn<u32,address,address> equal;
    cfn<void,address,Alternative> accumulate;
    cfn<void,Item,address,address,u32> commit;
    cfn<void,Item> reset_residency;
    cfn<void,Item,Alternative> join_residency;
    cfn<void,address,Alternative> push_proof;
    cfn<void,Item,address> finish_proofs;
    cfn<Info,address> inspect;
    cfn<Item,address,u64> mutable_child;
    cfn<void,Item,address,u64> select_child;
};

struct LivePathHost {
    cfn<Info,address> inspect;
    cfn<Item,address,u64> child;
    cfn<u32,Item> moved;
    cfn<void,address,u64> push;
    cfn<void,address> pop;
    cfn<u32,address,address,u32> visit;
};

extern "C" u32 live_paths(address value, u32 independent, address context, LivePathHost host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.moved)
    requires nonzero(host.push)
    requires nonzero(host.pop)
    requires nonzero(host.visit);

extern "C" void select_presence(Item target,address inputs,u64 count,SelectionHost host) on cpu
    requires nonzero(host.alternative)
    requires nonzero(host.mode)
    requires nonzero(host.equal)
    requires nonzero(host.accumulate)
    requires nonzero(host.commit)
    requires nonzero(host.reset_residency)
    requires nonzero(host.join_residency)
    requires nonzero(host.push_proof)
    requires nonzero(host.finish_proofs)
    requires nonzero(host.inspect)
    requires nonzero(host.mutable_child)
    requires nonzero(host.select_child);

struct LookupNode { Item item; address place; address payload; address ty; u32 projected; };
struct LookupHost {
    cfn<u32,address,address> matches_place;
    cfn<void,address,LookupNode> capture;
    cfn<Info,address> inspect;
    cfn<Field,address,u64> field;
    cfn<void,address,LookupNode,u64,Field> visit_child;
};

extern "C" void collect_owned_places(address context,LookupNode node,LookupHost host) on cpu
    requires nonzero(host.matches_place)
    requires nonzero(host.capture)
    requires nonzero(host.inspect)
    requires nonzero(host.field)
    requires nonzero(host.visit_child);

struct PayloadPair { Item left; Item right; };
enum DataInitial:u32 {Left=0;Right=1;Selected=2;};
enum DataJoin:u32 {Left=0;Right=1;Selected=2;Common=3;};
struct DataPresenceHost {
    cfn<u32,address> moved;
    cfn<void,address,address,address,u32> ownership;
    cfn<void,address,address,DataInitial,address,u32> initialization;
    cfn<void,address,address,DataJoin,address,u32> residency;
};
struct ReturnedHost {
    address context;Host values;DataPresenceHost data;
    cfn<void,address,address,address> save;
    cfn<void,address,address,address> restore;
    cfn<u32,address> maximum;
    cfn<void,address,address,DataJoin,address,address> elements;
    cfn<void,address,address,address> prefixes;
};
extern "C" void join_returned(Item left,Item right,u32 stored,address choice,address presence,u32 merge_metadata,ReturnedHost host) on cpu
    requires nonzero(host.values.inspect) requires nonzero(host.values.value)
    requires nonzero(host.values.ownership) requires nonzero(host.values.child)
    requires nonzero(host.values.state_equal) requires nonzero(host.values.state_set_moved)
    requires nonzero(host.values.proof_join_stored) requires nonzero(host.values.proof_join_common)
    requires nonzero(host.values.uniform) requires nonzero(host.values.set_uniform)
    requires nonzero(host.values.constant_equal) requires nonzero(host.values.clear_constant)
    requires nonzero(host.values.join_child) requires nonzero(host.values.join_ownership) requires nonzero(host.values.move_child)
    requires nonzero(host.values.data_root) requires nonzero(host.values.data_length) requires nonzero(host.values.data_ownership)
    requires nonzero(host.data.moved) requires nonzero(host.data.ownership)
    requires nonzero(host.data.initialization) requires nonzero(host.data.residency)
    requires nonzero(host.save) requires nonzero(host.restore) requires nonzero(host.maximum)
    requires nonzero(host.elements) requires nonzero(host.prefixes);
struct RetainedHost {
    cfn<Info,address> inspect;
    cfn<Item,address,u64> child;
    cfn<void,PayloadPair,address,u32> ownership;
    cfn<PayloadPair,PayloadPair> dataset;
    DataPresenceHost data;
};

extern "C" void retain_presence(PayloadPair pair,address presence,u32 take_right,RetainedHost host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.ownership)
    requires nonzero(host.dataset)
    requires nonzero(host.data.moved) requires nonzero(host.data.ownership)
    requires nonzero(host.data.initialization) requires nonzero(host.data.residency);

struct PlaceChoiceHost {
    cfn<u32,address,u64> condition;
    cfn<void,address,u64,u32> merge;
};
extern "C" void join_place_candidates(address context,u64 count,PlaceChoiceHost host) on cpu
    requires nonzero(host.condition)
    requires nonzero(host.merge);
