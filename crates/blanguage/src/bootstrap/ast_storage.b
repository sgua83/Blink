module bootstrap.ast_storage;
import bootstrap.names;
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" u32 GetLastError() on cpu ensures memory(read) from "KERNEL32.dll";
struct Buffer {address data;u64 length;u64 capacity;};
struct Store {handle heap;address source;u64 source_bytes;Buffer nodes;Buffer text;};
StorageError success() on cpu {return StorageError(0,0);}
address at(address bytes,u64 offset) on cpu {return unsafe_address(address_bits(bytes)+offset);}
StorageError reserve(handle heap,inout Buffer buffer,u64 required) on cpu {
    if(required<=buffer.capacity){return success();}
    u64 capacity=required|255u64;
    capacity=capacity|(capacity>>1u64);capacity=capacity|(capacity>>2u64);
    capacity=capacity|(capacity>>4u64);capacity=capacity|(capacity>>8u64);
    capacity=capacity|(capacity>>16u64);capacity=capacity|(capacity>>32u64);
    address bytes=unsafe_address(0u64);
    if(buffer.capacity==0u64){bytes=HeapAlloc(heap,0,capacity);}
    else{bytes=HeapReAlloc(heap,0,buffer.data,capacity);}
    if(address_bits(bytes)==0u64){return StorageError(2,0);}
    buffer.data=bytes;buffer.capacity=capacity;return success();
}
StorageIndex prepare(handle heap,inout Buffer buffer,u64 size,u64 alignment) on cpu requires nonzero(alignment) {
    u64 remainder=buffer.length%alignment;u64 padding=0u64;
    if(remainder!=0u64){padding=alignment-remainder;}
    u64 maximum=18446744073709551615u64;
    if(buffer.length>maximum-padding){return StorageIndex(0u64,StorageError(1,0));}
    u64 offset=buffer.length+padding;
    if(size>maximum-offset){return StorageIndex(0u64,StorageError(1,0));}
    StorageError error=reserve(heap,buffer,offset+size);
    return StorageIndex(offset,error);
}
StorageIndex append<T>(address storage,read T value) on cpu {
    StorageIndex slot=prepare(unsafe_position<Store>(storage).heap,
        unsafe_position<Store>(storage).nodes,size_of<T>(),align_of<T>());
    if(slot.error.kind!=0){return slot;}
    unsafe_position<T>(at(unsafe_position<Store>(storage).nodes.data,slot.offset))=value;
    unsafe_position<Store>(storage).nodes.length=slot.offset+size_of<T>();
    return slot;
}
StorageSlice append_array<T>(address storage,address values,u64 count) on cpu {
    if(count==0u64){return StorageSlice(Slice(unsafe_position<Store>(storage).nodes.length,0u64),success());}
    u64 stride=size_of<array<T,2>>()/2u64;
    if(count>18446744073709551615u64/stride){return StorageSlice(Slice(0u64,0u64),StorageError(1,0));}
    StorageIndex slot=prepare(unsafe_position<Store>(storage).heap,
        unsafe_position<Store>(storage).nodes,count*stride,align_of<T>());
    if(slot.error.kind!=0){return StorageSlice(Slice(0u64,0u64),slot.error);}
    u64 i=0u64;
    while(i<count){
        unsafe_position<T>(at(unsafe_position<Store>(storage).nodes.data,slot.offset+i*stride))
            =unsafe_position<T>(at(values,i*stride));
        i+=1u64;
    }
    unsafe_position<Store>(storage).nodes.length=slot.offset+count*stride;
    return StorageSlice(Slice(slot.offset,count),success());
}
extern "C" StorageCreate create(address source,u64 length) on cpu {
    handle heap=GetProcessHeap();address memory=HeapAlloc(heap,0,size_of<Store>());
    if(address_bits(memory)==0u64){return StorageCreate(memory,StorageError(2,0));}
    Buffer empty=Buffer(unsafe_address(0u64),0u64,0u64);
    unsafe_position<Store>(memory)=Store(heap,source,length,empty,empty);
    return StorageCreate(memory,success());
}
extern "C" StorageError destroy(address storage) on cpu {
    Store store=unsafe_position<Store>(storage);StorageError error=success();
    if(store.nodes.capacity!=0u64){
        if(HeapFree(store.heap,0,store.nodes.data)==0){error=StorageError(3,GetLastError());}
    }
    if(store.text.capacity!=0u64){
        if(HeapFree(store.heap,0,store.text.data)==0){if(error.kind==0){error=StorageError(3,GetLastError());}}
    }
    if(HeapFree(store.heap,0,storage)==0){if(error.kind==0){error=StorageError(3,GetLastError());}}
    return error;
}
extern "C" StorageView view(address storage) on cpu {
    Store store=unsafe_position<Store>(storage);
    return StorageView(store.nodes.data,store.nodes.length,store.text.data,store.text.length,store.source,store.source_bytes);
}
extern "C" StorageSlice indices(address storage,address values,u64 count) on cpu {return append_array<u64>(storage,values,count);}
extern "C" bool choice_before(address left, address right, address context) on cpu {
    return unsafe_position<u32>(left) < unsafe_position<u32>(right);
}

extern "C" StorageSlice choices(address storage, address values, u64 count) on cpu {
    if (count < 2u64) { return append_array<u32>(storage, values, count); }

    sort_rows<u32>(values, count, address(), c_function(choice_before));
    u64 unique = 1u64;
    u32 previous = unsafe_position<u32>(values);
    u64 index = 1u64;
    while (index < count) {
        u32 value = unsafe_position<u32>(at(values, index * size_of<u32>()));
        if (value != previous) {
            unsafe_position<u32>(at(values, unique * size_of<u32>())) = value;
            unique += 1u64;
            previous = value;
        }
        index += 1u64;
    }
    return append_array<u32>(storage, values, unique);
}
extern "C" StorageSlice texts(address storage,address values,u64 count) on cpu {return append_array<Text>(storage,values,count);}
extern "C" StorageText text(address storage,address bytes,u64 count) on cpu {
    StorageIndex slot=prepare(unsafe_position<Store>(storage).heap,unsafe_position<Store>(storage).text,count,1u64);
    if(slot.error.kind!=0){return StorageText(Text(1,Span(0u64,0u64)),slot.error);}
    u64 i=0u64;
    while(i<count){
        unsafe_position<u8>(at(unsafe_position<Store>(storage).text.data,slot.offset+i))=unsafe_position<u8>(at(bytes,i));
        i+=1u64;
    }
    unsafe_position<Store>(storage).text.length=slot.offset+count;
    return StorageText(Text(1,Span(slot.offset,slot.offset+count)),success());
}
extern "C" StorageIndex put_type(address storage,read TypeNode value) on cpu {return append<TypeNode>(storage,value);}
extern "C" StorageIndex put_shape(address storage,read ShapeNode value) on cpu {return append<ShapeNode>(storage,value);}
extern "C" StorageIndex put_argument(address storage,read ArgumentNode value) on cpu {return append<ArgumentNode>(storage,value);}
extern "C" StorageIndex put_condition(address storage,read ConditionNode value) on cpu {return append<ConditionNode>(storage,value);}
extern "C" StorageIndex put_predicate(address storage,read PredicateNode value) on cpu {return append<PredicateNode>(storage,value);}
extern "C" StorageIndex put_interval(address storage,read IntervalNode value) on cpu {return append<IntervalNode>(storage,value);}
extern "C" StorageIndex put_contract(address storage,read ContractNode value) on cpu {return append<ContractNode>(storage,value);}
extern "C" StorageIndex put_type_family(address storage,read TypeFamilyNode value) on cpu {return append<TypeFamilyNode>(storage,value);}
extern "C" StorageIndex put_type_parameter(address storage,read TypeParameterNode value) on cpu {return append<TypeParameterNode>(storage,value);}
extern "C" StorageIndex put_value_parameter(address storage,read ValueParameterNode value) on cpu {return append<ValueParameterNode>(storage,value);}
extern "C" StorageIndex put_field(address storage,read FieldNode value) on cpu {return append<FieldNode>(storage,value);}
extern "C" StorageIndex put_variant(address storage,read VariantNode value) on cpu {return append<VariantNode>(storage,value);}
extern "C" StorageIndex put_record(address storage,read RecordNode value) on cpu {return append<RecordNode>(storage,value);}
extern "C" StorageIndex put_param(address storage,read ParamNode value) on cpu {return append<ParamNode>(storage,value);}
extern "C" StorageIndex put_external(address storage,read ExternalNode value) on cpu {return append<ExternalNode>(storage,value);}
extern "C" StorageIndex put_function(address storage,read FunctionNode value) on cpu {return append<FunctionNode>(storage,value);}
extern "C" StorageIndex put_type_set(address storage,read TypeSetNode value) on cpu {return append<TypeSetNode>(storage,value);}
extern "C" StorageIndex put_module(address storage,read ModuleNode value) on cpu {return append<ModuleNode>(storage,value);}
extern "C" StorageIndex put_expr(address storage,read ExprNode value) on cpu {return append<ExprNode>(storage,value);}
extern "C" StorageIndex put_stmt(address storage,read StmtNode value) on cpu {return append<StmtNode>(storage,value);}
extern "C" StorageIndex put_parallel(address storage,read ParallelNode value) on cpu {return append<ParallelNode>(storage,value);}
extern "C" StorageIndex put_arm(address storage,read ArmNode value) on cpu {return append<ArmNode>(storage,value);}

StorageCondition not_condition() on cpu {return StorageCondition(ConditionRef(0u64),0,success());}
StorageCondition put_condition_result(address storage,ConditionNode value) on cpu {
    StorageIndex result=append<ConditionNode>(storage,value);
    if(result.error.kind!=0){return StorageCondition(ConditionRef(0u64),0,result.error);}
    return StorageCondition(ConditionRef(result.offset),1,success());
}
extern "C" StorageCondition condition_input(address storage,read ExprNode node) on cpu {
    match(node.kind){
        ExprKind::Name(value){return put_condition_result(storage,ConditionNode::Name(value));}
        ExprKind::Number(value){return put_condition_result(storage,ConditionNode::Number(value));}
        ExprKind::StaticU32(value){return put_condition_result(storage,ConditionNode::StaticU32(value));}
        ExprKind::Bool(value){return put_condition_result(storage,ConditionNode::Bool(value));}
        ExprKind::Member(value){
            StorageCondition base=condition(storage,value.base);
            if(base.present==0){return base;}
            return put_condition_result(storage,ConditionNode::Member(ConditionMember(base.value,value.field)));
        }
        ExprKind::Unary(value){
            StorageCondition inner=condition(storage,value.value);
            if(inner.present==0){return inner;}
            return put_condition_result(storage,ConditionNode::Unary(ConditionUnary(value.operation,inner.value)));
        }
        ExprKind::Binary(value){
            StorageCondition left=condition(storage,value.left);
            if(left.present==0){return left;}
            StorageCondition right=condition(storage,value.right);
            if(right.present==0){return right;}
            return put_condition_result(storage,ConditionNode::Binary(ConditionBinary(value.operation,left.value,right.value)));
        }
        ExprKind::Index(value){return not_condition();}
        ExprKind::Call(value){return not_condition();}
        ExprKind::Invoke(value){return not_condition();}
        ExprKind::Construct(value){return not_condition();}
        ExprKind::Specialize(value){return not_condition();}
        ExprKind::FunctionRef(value){return not_condition();}
        ExprKind::Cooperate(value){return not_condition();}
        ExprKind::Target(value){return not_condition();}
        ExprKind::Select(value){return not_condition();}
    }
}

extern "C" StorageCondition condition(address storage,ExprRef expression) on cpu {
    ExprNode node=unsafe_position<ExprNode>(at(unsafe_position<Store>(storage).nodes.data,expression.offset));
    return condition_input(storage,node);
}
