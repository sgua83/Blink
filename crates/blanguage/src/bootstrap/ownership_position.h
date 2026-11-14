module bootstrap.ownership_position;
struct PositionIndex { u32 kind; u32 value; address identity; u64 reg; };
struct PositionNode { u32 kind; u64 owner; u64 member; u64 word; PositionIndex index; address condition; u64 count; };
struct PositionItem { address node; u32 present; };
// Every address is borrowed for this synchronous model operation or belongs to
// the one actual output container. No node directory or second tree is stored.
struct Host {
    cfn<PositionNode,address> inspect;
    cfn<address,address,u64> child;
    cfn<u32,address,address> equal;
    cfn<u32,address,address> identity_equal;
    cfn<u32,address> presence_kind;
    cfn<u32,address,address> presence_equal;
    cfn<u32,address,PositionIndex> presence_uses_index;
    cfn<u32,address,address> contains;
    cfn<u32,address,u64,u64> contains_parameter;
    cfn<u32,address,u64,PositionIndex,u32> contains_element;
    cfn<u32,address,u64,u64,PositionIndex,u32> contains_input;
    cfn<address,address,address> iterate;
    cfn<PositionItem,address> next;
    cfn<void,address> drop_iterator;
    cfn<address> fresh_vector;
    cfn<void,address,address> push_vector;
    cfn<u64,address> vector_count;
    cfn<address,address,u64> vector_at;
    cfn<address,PositionNode,address,address,address> make_position;
    cfn<address,address> clone_position;
    cfn<address> fresh_set;
    cfn<void,address,address> insert_set;
};

extern "C" u32 select(address when,address a,address b,Host host) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_kind);
extern "C" u32 both(address a,address b,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.equal);
extern "C" address index(address values,PositionIndex selected,u64 width,Host host) on cpu
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.vector_count)
    requires nonzero(host.vector_at)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position);
extern "C" address word(address value,PositionIndex selected,u32 field,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position);
extern "C" u32 ready(address value,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator);
extern "C" u32 prefix_ready(address value,u32 length,u32 field,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator);
extern "C" u32 uses_scope(address value,u64 owner,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child);
extern "C" u32 depends_index(address value,PositionIndex selected,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_uses_index);
extern "C" address intersect(address a,address b,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_set)
    requires nonzero(host.insert_set);
