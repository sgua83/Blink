module bootstrap.ownership_presence;
import bootstrap.ownership_position;

struct Node { u32 kind; u32 operation; PositionIndex a; PositionIndex b; address first; address second; address value; };
struct EmitBranch { address value; u64 slot; u64 condition; u32 either; };
struct EmitHost {
    address layout;
    cfn<u64,address,u32,u32> constant;
    cfn<u64,address> input_register;
    cfn<u64,address,u32,u64,u64> compare;
    cfn<u64,address,u64> invert;
    cfn<u64,address> slot;
    cfn<void,address,u64,u64> store;
    cfn<u64,address,u64> load;
    cfn<void,address,EmitBranch> scoped_branch;
    cfn<void,address,address> swap_code;
    cfn<void,address,address,u64,u64> fixed_store;
    cfn<void,address,u64,address,address> branch;
};

struct OwnershipNode { u32 kind; address presence; };
// Opaque nodes stay in their original Rust containers. Scoped callbacks borrow
// actual Rust stack values synchronously; no address survives its owner.
struct Host {
    address layout;
    cfn<i32,address,address> compare_input;
    cfn<u32,address> constant_input;
    cfn<u32,address,PositionIndex> input_uses_index;
    cfn<void,address,address> input;
    cfn<void,address,SharedNodeKind> drop_payload;
    cfn<u32,address,address,Host,cfn<u32,address,address,address,address,Host>
    requires nonzero(argument4.compare_input)
    requires nonzero(argument4.constant_input)
    requires nonzero(argument4.input_uses_index)
    requires nonzero(argument4.drop_payload)
    requires nonzero(argument4.input)
    requires nonzero(argument4.children_scope)
    requires nonzero(argument4.clone_scope)
    requires nonzero(argument4.truth_scope)
    requires nonzero(argument4.position_equal)
    requires nonzero(argument4.condition_scope)
    requires nonzero(argument4.condition_clone_scope)
    requires nonzero(argument4.condition_truth)
    requires nonzero(argument4.condition_input)
    requires nonzero(argument4.condition_index)
    requires nonzero(argument4.condition_negate)
    requires nonzero(argument4.condition_logical)
    requires nonzero(argument4.condition_substitute)
    requires nonzero(argument4.input_offset)
    requires nonzero(argument4.condition_truth_in)>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.drop_payload)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)
    requires nonzero(argument3) children_scope;
    cfn<u32,address,address,Host,cfn<u32,address,address,Host>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.drop_payload)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)
    requires nonzero(argument3) clone_scope;
    cfn<u32,u32,address,Host,cfn<u32,address,address,Host>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.drop_payload)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)
    requires nonzero(argument3) truth_scope;
    cfn<u32,address,address> position_equal;
    cfn<u32,address,Host,cfn<u32,address,address,Host>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.drop_payload)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)>
    requires nonzero(argument1.compare_input)
    requires nonzero(argument1.constant_input)
    requires nonzero(argument1.input_uses_index)
    requires nonzero(argument1.drop_payload)
    requires nonzero(argument1.input)
    requires nonzero(argument1.children_scope)
    requires nonzero(argument1.clone_scope)
    requires nonzero(argument1.truth_scope)
    requires nonzero(argument1.position_equal)
    requires nonzero(argument1.condition_scope)
    requires nonzero(argument1.condition_clone_scope)
    requires nonzero(argument1.condition_truth)
    requires nonzero(argument1.condition_input)
    requires nonzero(argument1.condition_index)
    requires nonzero(argument1.condition_negate)
    requires nonzero(argument1.condition_logical)
    requires nonzero(argument1.condition_substitute)
    requires nonzero(argument1.input_offset)
    requires nonzero(argument1.condition_truth_in)
    requires nonzero(argument2) condition_scope;
    cfn<u32,address,address,Host,cfn<u32,address,address,Host>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.drop_payload)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)>
    requires nonzero(argument2.compare_input)
    requires nonzero(argument2.constant_input)
    requires nonzero(argument2.input_uses_index)
    requires nonzero(argument2.input)
    requires nonzero(argument2.children_scope)
    requires nonzero(argument2.clone_scope)
    requires nonzero(argument2.truth_scope)
    requires nonzero(argument2.position_equal)
    requires nonzero(argument2.condition_scope)
    requires nonzero(argument2.condition_clone_scope)
    requires nonzero(argument2.condition_truth)
    requires nonzero(argument2.condition_input)
    requires nonzero(argument2.condition_index)
    requires nonzero(argument2.condition_negate)
    requires nonzero(argument2.condition_logical)
    requires nonzero(argument2.condition_substitute)
    requires nonzero(argument2.input_offset)
    requires nonzero(argument2.condition_truth_in)
    requires nonzero(argument3) condition_clone_scope;
    cfn<void,address,u32> condition_truth;
    cfn<void,address,address> condition_input;
    cfn<void,address,u32,PositionIndex,PositionIndex> condition_index;
    cfn<void,address> condition_negate;
    cfn<void,address,address,u32> condition_logical;
    cfn<void,address,address,PositionIndex,PositionIndex> condition_substitute;
    cfn<u32,address,PositionIndex> input_offset;
    cfn<u32,address,address> condition_truth_in;
};

extern "C" void relation(address output,Selection value,Host host) on cpu
    requires nonzero(host.drop_payload)
    ;

extern "C" i32 compare(address a,address b,Host host) on cpu
    requires nonzero(host.compare_input);

extern "C" u32 uses_index(address value,PositionIndex wanted,Host host) on cpu
    requires nonzero(host.input_uses_index);

extern "C" void from_value(address output,address value,Host host) on cpu
    requires nonzero(host.constant_input)
    requires nonzero(host.input)
    requires nonzero(host.drop_payload);

extern "C" void combine(address left,address right,u32 any,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void negate(address value,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void selected(address output,address position,address wanted,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void condition(address output,address value,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void loop_condition(address output,address value,address inputs,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void clone_presence(address output,address source,Host host) on cpu
    requires nonzero(host.input)
    requires nonzero(host.drop_payload);

extern "C" u32 ownership_may_own(address value,Host host) on cpu
   ;

extern "C" u32 ownership_available(address value,Host host) on cpu
   ;

extern "C" void ownership_presence(address output,address value,Host host) on cpu
    requires nonzero(host.input)
    requires nonzero(host.drop_payload);

extern "C" void ownership_from_presence(address output,address value,Host host) on cpu
    requires nonzero(host.drop_payload)
    ;

extern "C" void ownership_either(address output,address a,address b,address choice,u32 has_choice,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" void ownership_restricted(address output,address value,address when,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" u32 ownership_compatible(address a,address b,address inputs,address path,Host host) on cpu
    requires nonzero(host.compare_input)
    requires nonzero(host.constant_input)
    requires nonzero(host.input_uses_index)
    requires nonzero(host.drop_payload)
    requires nonzero(host.input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in);

extern "C" u64 emit_presence(address context,address value,EmitHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.input_register)
    requires nonzero(host.compare)
    requires nonzero(host.invert)
    requires nonzero(host.slot)
    requires nonzero(host.load)
    requires nonzero(host.scoped_branch);

extern "C" void emit_presence_branch(address context,EmitBranch work,address right,address fixed,EmitHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.input_register)
    requires nonzero(host.compare)
    requires nonzero(host.invert)
    requires nonzero(host.slot)
    requires nonzero(host.store)
    requires nonzero(host.load)
    requires nonzero(host.scoped_branch)
    requires nonzero(host.swap_code)
    requires nonzero(host.fixed_store)
    requires nonzero(host.branch);

enum TaskSplit:u32 { Unchanged=0; Unresolved=1; Selected=2; };
struct TaskSourceBranches {
    address yes;
    u64 yes_count;
    address yes_result;
    address no;
    u64 no_count;
    address no_result;
};
struct TaskSourceHost {
    cfn<u32,address,u64> matches;
    cfn<address,address> next_control;
    cfn<TaskSplit,address,address,address> partition;
    cfn<void,address,u32> truth;
    cfn<void,address,address,address,address> merge;
};
extern "C" u32 task_source_select(address input,u64 count,address output,TaskSourceHost host) on cpu
    requires nonzero(host.matches)
    requires nonzero(host.next_control)
    requires nonzero(host.partition)
    requires nonzero(host.truth);
extern "C" TaskSplit task_source_split(u64 count,address control,TaskSourceBranches branches,address output,
    TaskSourceHost host) on cpu
    requires nonzero(host.matches)
    requires nonzero(host.next_control)
    requires nonzero(host.partition)
    requires nonzero(host.truth)
    requires nonzero(host.merge);
