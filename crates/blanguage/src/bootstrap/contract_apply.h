module bootstrap.contract_apply;
import bootstrap.condition_index;

struct ContractOperationMemo { address entries; usize count; usize capacity; };

extern "C" usize apply_contract_decisions(
    address decisions,address memo,usize variable_bytes,usize left,usize right,u32 either,
    cfn<i32,address,address> compare
) on cpu requires nonzero(compare);
extern "C" usize normalize_contract_decision(
    address decisions,ContractDecision value,usize variable_bytes,cfn<i32,address,address> compare
) on cpu requires nonzero(compare);
extern "C" void drop_contract_operations(address memo) on cpu;

extern "C" usize negate_contract_decision(
    address decisions,address memo,usize variable_bytes,usize root,cfn<i32,address,address> compare
) on cpu requires nonzero(compare);
