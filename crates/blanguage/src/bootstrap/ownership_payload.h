module bootstrap.ownership_payload;
struct Info {u64 count;u32 flags;};
struct Item {address node;u32 kind;};
struct Field {address ty;u32 offset;u32 owns;};
struct Indices {address values;u64 count;};
struct Count {u32 known;u32 value;};
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
    cfn<u32,address> state_available;
    cfn<u32,address> state_owned;
    cfn<u32,address> state_moved;
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
    cfn<void,address,address,u64,u32,address> join_child;
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
    cfn<void,address> drop_path;
    cfn<u32,address,address,address,address> data_loop;
    cfn<address> fresh_indices;
    cfn<void,address,u64> push_index;
    cfn<Indices,address> indices_view;
    cfn<void,address> drop_indices;
};

extern "C" void join(address left,address right,u32 stored,address choice,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_equal)
    requires nonzero(host.state_set_moved)
    requires nonzero(host.proof_join_stored)
    requires nonzero(host.proof_join_common)
    requires nonzero(host.uniform)
    requires nonzero(host.set_uniform)
    requires nonzero(host.constant_equal)
    requires nonzero(host.clear_constant)
    requires nonzero(host.join_child)
    requires nonzero(host.move_child);
extern "C" u32 available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state_available)
    requires nonzero(host.data_ownership);
extern "C" u32 all_available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state_available)
    requires nonzero(host.state_owned)
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
    requires nonzero(host.state_moved)
    requires nonzero(host.data_ownership);
extern "C" u32 loop_compatible(address before,address after,address context,address path,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.ownership_loop)
    requires nonzero(host.loop_path)
    requires nonzero(host.path_possible)
    requires nonzero(host.drop_path)
    requires nonzero(host.data_loop);
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
    requires nonzero(host.state_moved)
    requires nonzero(visit);
extern "C" void resources(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_moved)
    requires nonzero(visit);
