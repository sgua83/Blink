module bootstrap.contract_condition;
import bootstrap.scalar_fold;
import bootstrap.numeric_literal;

struct Bytes {address data;u64 count;};
struct Result {address value;u32 ok;};
struct TypeResult {address ty;u32 literal;u32 ok;};
// Constant nodes lend their actual native payload through left.
struct Node {u32 kind;u32 flag;Bytes text;address left;address right;};
struct Constant {address ty;address bits;u64 count;};
struct TypeInfo {u32 scalar;u32 width;u32 is_array;u32 is_variant;u64 components;u64 length;};
struct Lookup {address key;u32 found;u32 constant;u32 is_public;};
struct Variant {u32 found;u32 discriminant;};
struct Member {address ty;address indices;address owner;u64 count;u32 ok;};
struct BinaryRule {address ty;u32 kind;u32 scalar;u32 cast_left;u32 cast_right;u32 comparison;u32 operation;u32 ok;};
struct UnaryRule {u32 operation;u32 ok;};
// The original transient type/literal relation. Result nodes and types own the
// native values; original borrows the input for this synchronous traversal.
struct Term {address value;address original;address ty;u32 literal;u32 ok;};
struct Host {
    cfn<Node,address> node;
    cfn<Constant,address> constant;
    cfn<u64,address,u64> broadcast_bit;
    cfn<TypeInfo,address> type_info;
    cfn<address,address> copy_node;
    cfn<address,address> copy_type;
    cfn<void,address> drop_node;
    cfn<void,address> drop_type;
    cfn<address,u32,u64> scalar_type;
    cfn<address,u64> bits;
    cfn<void,address,u64> push_bit;
    cfn<void,address> drop_bits;
    cfn<address,address,address,address> make_constant;
    cfn<address,address,Bytes> make_member;
    cfn<address,Bytes,address> make_unary;
    cfn<address,Bytes,address,address> make_binary;
    cfn<u32,address,Bytes> parameter;
    cfn<Lookup,address,Bytes> lookup;
    cfn<Result,address,address> evaluate;
    cfn<Variant,address,Bytes> variant;
    cfn<Member,address,address,Bytes> member;
    cfn<void,address> drop_indices;
    cfn<Result,address,address> path_type;
    cfn<UnaryRule,address,Bytes,address> unary_type;
    cfn<BinaryRule,address,Bytes,address,u32,address,u32> binary_type;
    cfn<u32,address,address,address> same_type;
    cfn<u64,u32,u64,u64,u32> divide;
    cfn<void,address,u32,Bytes> fail;
};
extern "C" Result bind(address context,address value,u32 is_public,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.type_info)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_member)
    requires nonzero(host.make_unary)
    requires nonzero(host.make_binary)
    requires nonzero(host.parameter)
    requires nonzero(host.lookup)
    requires nonzero(host.evaluate)
    requires nonzero(host.variant)
    requires nonzero(host.member)
    requires nonzero(host.drop_indices)
    requires nonzero(host.fail);
extern "C" Term identity(address context,address value,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.broadcast_bit)
    requires nonzero(host.type_info)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.drop_bits)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_unary)
    requires nonzero(host.make_binary)
    requires nonzero(host.path_type)
    requires nonzero(host.unary_type)
    requires nonzero(host.binary_type)
    requires nonzero(host.same_type)
    requires nonzero(host.divide)
    requires nonzero(host.fail);
extern "C" TypeResult condition_type(address context,address value,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.path_type)
    requires nonzero(host.unary_type)
    requires nonzero(host.binary_type)
    requires nonzero(host.same_type)
    requires nonzero(host.fail);
