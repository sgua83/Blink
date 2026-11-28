module bootstrap.contract_apply;
import bootstrap.condition_index;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" never abort() on cpu from "ucrtbase.dll";

struct ApplyMemoEntry { usize left; usize right; usize answer_plus_one; u32 either; };

address apply_record(address base,usize index,usize stride) on cpu {
    return unsafe_address(address_bits(base)+index*stride);
}

usize apply_hash(usize word) on cpu {
    usize hash=word^(word>>33u64);
    hash*=0xff51afd7ed558ccdu64;
    return hash^(hash>>33u64);
}

address apply_memo_slot(ContractOperationMemo memo,u32 either,usize left,usize right) on cpu {
    usize mask=memo.capacity-1u64;
    usize index=apply_hash(left^apply_hash(right)^u64(either))&mask;
    while(true){
        address slot=apply_record(memo.entries,index,size_of<ApplyMemoEntry>());
        let entry=unsafe_position<ApplyMemoEntry>(slot);
        if(entry.answer_plus_one==0u64||(entry.left==left&&entry.right==right&&entry.either==either)){
            return slot;
        }
        index=(index+1u64)&mask;
    }
}

usize apply_capacity(usize capacity,usize stride) on cpu requires nonzero(stride) {
    if(capacity>9223372036854775807u64/stride/2u64){abort();}
    return capacity==0u64?8u64:capacity*2u64;
}

void grow_apply_memo(inout ContractOperationMemo memo) on cpu {
    let capacity=apply_capacity(memo.capacity,size_of<ApplyMemoEntry>());
    let heap=GetProcessHeap();
    let entries=HeapAlloc(heap,8,capacity*size_of<ApplyMemoEntry>());
    if(entries==address()){abort();}
    let grown=ContractOperationMemo(entries,memo.count,capacity);
    usize index=0u64;
    while(index<memo.capacity){
        let item=unsafe_position<ApplyMemoEntry>(apply_record(memo.entries,index,size_of<ApplyMemoEntry>()));
        if(item.answer_plus_one!=0u64){
            unsafe_position<ApplyMemoEntry>(apply_memo_slot(grown,item.either,item.left,item.right))=item;
        }
        index+=1u64;
    }
    if(memo.entries!=address()){HeapFree(heap,0,memo.entries);}
    memo=grown;
}

struct ApplyBuffer { address data; usize count; usize capacity; };

address append_apply_record(inout ApplyBuffer buffer,usize stride) on cpu requires nonzero(stride) {
    if(buffer.count==buffer.capacity){
        let capacity=apply_capacity(buffer.capacity,stride);
        let heap=GetProcessHeap();
        address data=buffer.data==address()?HeapAlloc(heap,0,capacity*stride):
            HeapReAlloc(heap,0,buffer.data,capacity*stride);
        if(data==address()){abort();}
        buffer.data=data;
        buffer.capacity=capacity;
    }
    let slot=apply_record(buffer.data,buffer.count,stride);
    buffer.count+=1u64;
    return slot;
}

void free_apply_buffer(ApplyBuffer buffer) on cpu {
    if(buffer.data!=address()){HeapFree(GetProcessHeap(),0,buffer.data);}
}

struct ApplyWork { usize left; usize right; u32 either; u32 ready; };
struct ApplyPair { u64 low; u64 high; usize left; usize right; };

ApplyWork apply_work(usize left,usize right,u32 either) on cpu {
    return left<=right?ApplyWork(left,right,either,0):ApplyWork(right,left,either,0);
}

ApplyPair apply_pair(ApplyBuffer pairs,usize index) on cpu {
    return unsafe_position<ApplyPair>(apply_record(pairs.data,index,size_of<ApplyPair>()));
}

ContractDecisionCell read_decision_cell(ContractDecision decision,usize index) on cpu {
    return unsafe_position<ContractDecisionCell>(
        apply_record(decision.cells,index,size_of<ContractDecisionCell>()));
}

void append_apply_pair(inout ApplyBuffer output,ContractDecisionCell end,usize left,usize right) on cpu {
    let slot=append_apply_record(output,size_of<ApplyPair>());
    unsafe_position<ApplyPair>(slot)=ApplyPair(end.low,end.high,left,right);
}

void push_apply_work(inout ApplyBuffer output,ApplyWork work) on cpu {
    let slot=append_apply_record(output,size_of<ApplyWork>());
    unsafe_position<ApplyWork>(slot)=work;
}

usize apply_answer(ContractOperationMemo memo,ApplyWork work) on cpu {
    if(memo.capacity==0u64){return 0u64;}
    let address_slot=apply_memo_slot(memo,work.either,work.left,work.right);
    return unsafe_position<ApplyMemoEntry>(address_slot).answer_plus_one;
}

void remember_apply_answer(inout ContractOperationMemo memo,ApplyWork work,usize result) on cpu {
    if(memo.count==memo.capacity/2u64){grow_apply_memo(memo);}
    let slot=apply_memo_slot(memo,work.either,work.left,work.right);
    if(unsafe_position<ApplyMemoEntry>(slot).answer_plus_one==0u64){memo.count+=1u64;}
    unsafe_position<ApplyMemoEntry>(slot)=ApplyMemoEntry(work.left,work.right,result+1u64,work.either);
}

bool earlier_end(ContractDecisionCell a,ContractDecisionCell b) on cpu {
    return a.high<b.high||(a.high==b.high&&a.low<b.low);
}

address split_apply_operands(
    address decisions,ApplyWork work,inout ApplyBuffer pairs,cfn<i32,address,address> compare
) on cpu requires nonzero(compare) {
    let left=contract_decision_at(decisions,work.left);
    let right=contract_decision_at(decisions,work.right);
    let order=compare(left.variable,right.variable);
    pairs.count=0u64;
    if(order!=i32(0)){
        let split=order<i32(0)?left:right;
        usize index=0u64;
        while(index<split.count){
            let cell=read_decision_cell(split,index);
            append_apply_pair(pairs,cell,order<i32(0)?cell.child:work.left,order<i32(0)?work.right:cell.child);
            index+=1u64;
        }
        return split.variable;
    }

    usize a=0u64;
    usize b=0u64;
    while(a<left.count&&b<right.count){
        let x=read_decision_cell(left,a);
        let y=read_decision_cell(right,b);
        let end=earlier_end(x,y)?x:y;
        append_apply_pair(pairs,end,x.child,y.child);
        if(x.low==end.low&&x.high==end.high){a+=1u64;}
        if(y.low==end.low&&y.high==end.high){b+=1u64;}
    }
    return left.variable;
}

usize evaluate_apply(
    address decisions,inout ContractOperationMemo memo,usize variable_bytes,ApplyWork root,
    cfn<i32,address,address> compare
) on cpu requires nonzero(compare) {
    let cached=apply_answer(memo,root);
    if(cached!=0u64){return cached-1u64;}

    ApplyBuffer pending=ApplyBuffer(address(),0u64,0u64);
    ApplyBuffer pairs=ApplyBuffer(address(),0u64,0u64);
    ApplyBuffer cells=ApplyBuffer(address(),0u64,0u64);
    push_apply_work(pending,root);
    while(pending.count!=0u64){
        pending.count-=1u64;
        ApplyWork work=unsafe_position<ApplyWork>(apply_record(pending.data,pending.count,size_of<ApplyWork>()));
        if(apply_answer(memo,work)!=0u64){continue;}

        usize value=0u64;
        bool simple=true;
        if(work.left==work.right){value=work.left;}
        else if(work.either!=0){
            if(work.left==1u64||work.right==1u64){value=1u64;}
            else if(work.left==0u64){value=work.right;}
            else{simple=false;}
        }else if(work.left==0u64){value=0u64;}
        else if(work.left==1u64){value=work.right;}
        else{simple=false;}

        if(simple){
            remember_apply_answer(memo,work,value);
            continue;
        }
        let variable=split_apply_operands(decisions,work,pairs,compare);
        if(work.ready==0){
            work.ready=1;
            push_apply_work(pending,work);
            usize index=pairs.count;
            while(index!=0u64){
                index-=1u64;
                let pair=apply_pair(pairs,index);
                push_apply_work(pending,apply_work(pair.left,pair.right,work.either));
            }
        }else{
            cells.count=0u64;
            usize index=0u64;
            while(index<pairs.count){
                let pair=apply_pair(pairs,index);
                let child=apply_answer(memo,apply_work(pair.left,pair.right,work.either))-1u64;
                let slot=append_apply_record(cells,size_of<ContractDecisionCell>());
                unsafe_position<ContractDecisionCell>(slot)=ContractDecisionCell(pair.low,pair.high,child);
                index+=1u64;
            }
            value=normalize_contract_decision(decisions,ContractDecision(variable,cells.data,cells.count),variable_bytes,compare);
            remember_apply_answer(memo,work,value);
        }
    }
    free_apply_buffer(cells);
    free_apply_buffer(pairs);
    free_apply_buffer(pending);
    return apply_answer(memo,root)-1u64;
}

extern "C" usize apply_contract_decisions(
    address decisions,address memo,usize variable_bytes,usize left,usize right,u32 either,
    cfn<i32,address,address> compare
) on cpu requires nonzero(compare) {
    return evaluate_apply(decisions,unsafe_position<ContractOperationMemo>(memo),variable_bytes,
        apply_work(left,right,either),compare);
}

extern "C" usize normalize_contract_decision(
    address decisions,ContractDecision value,usize variable_bytes,cfn<i32,address,address> compare
) on cpu requires nonzero(compare) {
    usize merged=0u64;
    usize index=0u64;
    while(index<value.count){
        let next=read_decision_cell(value,index);
        if(merged!=0u64){
            let last=apply_record(value.cells,merged-1u64,size_of<ContractDecisionCell>());
            if(unsafe_position<ContractDecisionCell>(last).child==next.child){
                unsafe_position<ContractDecisionCell>(last)=next;
                index+=1u64;
                continue;
            }
        }
        unsafe_position<ContractDecisionCell>(apply_record(value.cells,merged,size_of<ContractDecisionCell>()))=next;
        merged+=1u64;
        index+=1u64;
    }
    if(merged==1u64){return read_decision_cell(value,0u64).child;}
    return intern_contract_decision(decisions,ContractDecision(value.variable,value.cells,merged),variable_bytes,compare);
}

extern "C" void drop_contract_operations(address memo) on cpu {
    let entries=unsafe_position<ContractOperationMemo>(memo).entries;
    if(entries!=address()){HeapFree(GetProcessHeap(),0,entries);}
    unsafe_position<ContractOperationMemo>(memo)=ContractOperationMemo(address(),0u64,0u64);
}

struct NegateWork { usize node; u32 ready; };

void push_negate_work(inout ApplyBuffer pending,usize node,u32 ready) on cpu {
    let slot=append_apply_record(pending,size_of<NegateWork>());
    unsafe_position<NegateWork>(slot)=NegateWork(node,ready);
}

extern "C" usize negate_contract_decision(
    address decisions,address memo,usize variable_bytes,usize root,cfn<i32,address,address> compare
) on cpu requires nonzero(compare) {
    let cached=contract_memo_negative(memo,root);
    if(cached!=18446744073709551615u64){return cached;}

    ApplyBuffer pending=ApplyBuffer(address(),0u64,0u64);
    ApplyBuffer cells=ApplyBuffer(address(),0u64,0u64);
    push_negate_work(pending,root,0);
    while(pending.count!=0u64){
        pending.count-=1u64;
        let work=unsafe_position<NegateWork>(apply_record(pending.data,pending.count,size_of<NegateWork>()));
        if(contract_memo_negative(memo,work.node)!=18446744073709551615u64){continue;}
        let decision=contract_decision_at(decisions,work.node);
        if(work.ready==0){
            push_negate_work(pending,work.node,1);
            usize index=decision.count;
            while(index!=0u64){
                index-=1u64;
                push_negate_work(pending,read_decision_cell(decision,index).child,0);
            }
        }else{
            cells.count=0u64;
            usize index=0u64;
            while(index<decision.count){
                let cell=read_decision_cell(decision,index);
                let child=contract_memo_negative(memo,cell.child);
                let slot=append_apply_record(cells,size_of<ContractDecisionCell>());
                unsafe_position<ContractDecisionCell>(slot)=ContractDecisionCell(cell.low,cell.high,child);
                index+=1u64;
            }
            let inverse=normalize_contract_decision(decisions,
                ContractDecision(decision.variable,cells.data,cells.count),variable_bytes,compare);
            remember_contract_negative(memo,work.node,inverse);
            remember_contract_negative(memo,inverse,work.node);
        }
    }
    free_apply_buffer(cells);
    free_apply_buffer(pending);
    return contract_memo_negative(memo,root);
}
