module bootstrap.stage_fusion;
import bootstrap.ir_dataflow;

u64 binding_at(address data, u64 index) on cpu {
    return unsafe_position<u64>(unsafe_address(address_bits(data) + index * 8u64));
}

extern "C" u64 merge_bindings(address output, u64 left_count, BindingSpan right) on cpu {
    u64 left_at = left_count;
    u64 right_at = right.count;
    u64 write_at = left_count + right.count;
    while (left_at != 0u64 && right_at != 0u64) {
        u64 left = binding_at(output, left_at - 1u64);
        u64 next = binding_at(right.data, right_at - 1u64);
        u64 binding = left;
        if (left >= next) { left_at -= 1u64; }
        if (left <= next) { right_at -= 1u64; binding = next; }
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) = binding;
    }
    while (right_at != 0u64) {
        right_at -= 1u64;
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) =
            binding_at(right.data, right_at);
    }
    while (left_at != 0u64 && write_at != left_at) {
        left_at -= 1u64;
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) =
            binding_at(output, left_at);
    }
    return write_at - left_at;
}

address tile_end(address plane, u64 slot) on cpu {
    return unsafe_address(address_bits(plane) + slot * size_of<i64>());
}

extern "C" void place_tile(address rows, u64 count, address narrow, address wide) on cpu {
    u64 narrow_used = 0u64;
    u64 wide_used = 0u64;
    u64 index = 0u64;
    while (index < count) {
        address row = unsafe_address(address_bits(rows) + index * size_of<TilePlacement>());
        TilePlacement item = unsafe_position<TilePlacement>(row);
        address plane = narrow;
        u64 used = narrow_used;
        if (item.plane != 0u64) {
            plane = wide;
            used = wide_used;
        }
        u64 slot = 0u64;
        u64 next = 0u64;
        while (next < used && next < slot + item.width) {
            if (unsafe_position<i64>(tile_end(plane, next)) >= item.begin) {
                slot = next + 1u64;
            }
            next += 1u64;
        }
        next = slot;
        while (next < slot + item.width) {
            unsafe_position<i64>(tile_end(plane, next)) = item.end;
            next += 1u64;
        }
        used = max(used, next);
        if (item.plane == 0u64) {
            narrow_used = used;
        } else {
            wide_used = used;
        }
        item.slot = slot;
        unsafe_position<TilePlacement>(row) = item;
        index += 1u64;
    }
}

address stage_write_at(address writes, u64 index) on cpu {
    return unsafe_address(address_bits(writes) + index * size_of<StageWrite>());
}

u64 coalesce_stage_writes(address writes, u64 count) on cpu {
    u64 used = 0u64;
    u64 index = 0u64;
    while (index < count) {
        StageWrite next = unsafe_position<StageWrite>(stage_write_at(writes, index));
        bool merged = false;
        if (used != 0u64) {
            address tail = stage_write_at(writes, used - 1u64);
            StageWrite prior = unsafe_position<StageWrite>(tail);
            if (prior.binding == next.binding && next.start <= prior.end) {
                prior.end = max(prior.end, next.end);
                unsafe_position<StageWrite>(tail) = prior;
                merged = true;
            }
        }
        if (!merged) {
            unsafe_position<StageWrite>(stage_write_at(writes, used)) = next;
            used += 1u64;
        }
        index += 1u64;
    }
    return used;
}

bool overwrites_stage_guarantee(address writes, u64 count, StageGuarantee guarantee) on cpu {
    u64 begin = 0u64;
    u64 end = count;
    while (begin < end) {
        u64 middle = begin + (end - begin) / 2u64;
        StageWrite write = unsafe_position<StageWrite>(stage_write_at(writes, middle));
        if (write.binding < guarantee.binding
            || (write.binding == guarantee.binding && u64(write.end) <= guarantee.start)) {
            begin = middle + 1u64;
        } else {
            end = middle;
        }
    }
    if (begin == count) { return false; }
    StageWrite write = unsafe_position<StageWrite>(stage_write_at(writes, begin));
    return write.binding == guarantee.binding && u64(write.start) < guarantee.end;
}

u64 retain_guarantee_kind(address writes, u64 write_count, GuaranteeKind kind,
    u64 count, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots) {
    u64 retained = 0u64;
    u64 index = 0u64;

    while (index < count) {
        let guarantee = host.field(host.context, kind, index);
        if (!overwrites_stage_guarantee(writes, write_count, guarantee)) {
            if (index != retained) { host.swap_slots(host.context, kind, index, retained); }
            retained += 1u64;
        }
        index += 1u64;
    }
    return retained;
}

extern "C" GuaranteeCounts retain_stage_guarantees(address writes, u64 write_count,
    GuaranteeCounts counts, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots) {
    let count = coalesce_stage_writes(writes, write_count);
    let prefix = retain_guarantee_kind(writes, count, GuaranteeKind::Prefix, counts.prefix, host);
    let element = retain_guarantee_kind(writes, count, GuaranteeKind::Element, counts.element, host);
    return GuaranteeCounts(prefix, element);
}

address local_value_at(address values, u64 index) on cpu {
    return unsafe_address(address_bits(values) + index * size_of<LocalValue>());
}

extern "C" u64 intersect_local_values(address left, u64 left_count,
    address right, u64 right_count) on cpu {
    u64 kept = 0u64;
    u64 a = 0u64;
    u64 b = 0u64;
    while (a < left_count && b < right_count) {
        LocalValue lhs = unsafe_position<LocalValue>(local_value_at(left, a));
        LocalValue rhs = unsafe_position<LocalValue>(local_value_at(right, b));
        if (lhs.slot == rhs.slot && lhs.value == rhs.value) {
            unsafe_position<LocalValue>(local_value_at(left, kept)) = lhs;
            kept += 1u64;
        }
        if (lhs.slot <= rhs.slot) { a += 1u64; }
        if (rhs.slot <= lhs.slot) { b += 1u64; }
    }
    return kept;
}

extern "C" void private_phase_step(address instruction,address live,address saved,LiveHost host) on cpu
    requires nonzero(host.kill) requires nonzero(host.read) requires nonzero(host.change) {
    host.kill(instruction,live);
    host.change(LiveSetAction::Copy,saved,live,0u64);
    host.read(instruction,live);
}

extern "C" void private_loop(Span condition,u64 test,Span body,
    address after,address break_live,address continue_live,address exit_live,
    address result,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.kill) requires nonzero(host.read)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    // Pointwise gen/kill transfers close the backedge from an empty header.
    // after may be result: it remains borrowed until the final swap.
    address need=host.clone_set(empty);
    private_before(body,need,after,empty,exit_live,empty,host);
    host.change(LiveSetAction::Union,need,after,0u64);
    host.change(LiveSetAction::InsertRegister,need,unsafe_address(0u64),test);
    private_before(condition,need,break_live,continue_live,exit_live,empty,host);
    host.change(LiveSetAction::Swap,result,need,0u64);
    host.drop_set(need);
}

void private_instruction(address instruction,View value,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.kill) requires nonzero(host.read)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    if (value.tag==OP_BRANCH) {
        address first=host.clone_set(live);
        private_before(value.first,first,break_live,continue_live,exit_live,empty,host);
        private_before(value.second,live,break_live,continue_live,exit_live,empty,host);
        host.change(LiveSetAction::Union,live,first,0u64);
        host.drop_set(first);
        host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),value.a);
    } else if (value.tag==OP_LOOP_VALUE) {
        private_loop(value.first,value.a,value.second,live,break_live,continue_live,exit_live,live,empty,host);
    } else if (value.tag==OP_SCOPE) {
        address after=host.clone_set(live);
        private_before(value.first,live,break_live,continue_live,after,empty,host);
        host.drop_set(after);
    } else if (value.tag==OP_EXIT_VALUE) {
        host.change(LiveSetAction::Copy,live,exit_live,0u64);
    } else if (value.tag==OP_BREAK_VALUE) {
        host.change(LiveSetAction::Copy,live,break_live,0u64);
    } else if (value.tag==OP_CONTINUE_VALUE) {
        host.change(LiveSetAction::Copy,live,continue_live,0u64);
    } else if (value.tag==OP_LOAD_LOCAL) {
        host.change(LiveSetAction::RemoveRegister,live,unsafe_address(0u64),value.a);
        host.change(LiveSetAction::InsertLocal,live,unsafe_address(0u64),value.b);
    } else if (value.tag==OP_STORE_LOCAL) {
        host.change(LiveSetAction::RemoveLocal,live,unsafe_address(0u64),value.a);
        host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),value.b);
    } else {
        host.kill(instruction,live);
        host.read(instruction,live);
    }
}

extern "C" void private_before(Span code,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.kill) requires nonzero(host.read)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    u64 index=code.count;
    while (index!=0u64) {
        index-=1u64;
        address instruction=block_instruction_at(code,index);
        View value=host.ir.inspect(Node(instruction,N_INSTRUCTION));
        private_instruction(instruction,value,live,break_live,continue_live,exit_live,empty,host);
    }
}

u32 dce_result(inout DceResults results,address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    results.present=1;
    results.needed=contains(needed,0,reg);
    return results.needed==0 ? 1 : 0;
}
extern "C" u32 dce_result_needed(address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    return contains(needed,0,reg);
}
bool dce_any_local(address needed,u64 first,u64 length,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    u64 index=0u64;
    while(index<length){
        if(contains(needed,1,first+index)!=0){return true;}
        index+=1u64;
    }
    return false;
}
DceResults dce_results(address instruction,address needed,DceHost host) on cpu requires nonzero(host.live.ir.inspect) requires nonzero(host.contains) {
    DceResults output=DceResults(0,0);
    Results values=results(Node(instruction,N_INSTRUCTION),host.live.ir);
    while(true){
        Word value=next_result(values);if(value.present==0){break;}
        if(dce_result(output,needed,value.value,host.contains)==0){break;}
    }
    return output;
}
address block_instruction_at(Span code,u64 index) on cpu {
    return unsafe_address(address_bits(code.data)+index*u64(code.stride));
}

bool dce_scope_observable(Span code,address after,DceHost host) on cpu requires nonzero(host.live.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block) requires nonzero(host.contains) {
    u64 index=0u64;
    while(index<code.count){
        address instruction=block_instruction_at(code,index);
        View value=host.live.ir.inspect(Node(instruction,N_INSTRUCTION));u32 kind=value.tag;
        if(kind==OP_STORE_LOCAL){
            if(host.contains(after,1,value.a)!=0){return true;}
        } else if(kind==OP_STORE_LOCAL_INDEXED){
            if(dce_any_local(after,value.a,value.b,host.contains)){return true;}
        } else if(kind==OP_SCOPE){
            InstructionStorage storage=host.storage(instruction);
            if(dce_scope_observable(host.read_block(storage.first_code),after,host)){return true;}
        } else if(kind==OP_BRANCH){
            InstructionStorage storage=host.storage(instruction);
            if(dce_scope_observable(host.read_block(storage.first_code),after,host)
                ||dce_scope_observable(host.read_block(storage.second_code),after,host)){return true;}
        } else if(kind==OP_LOOP_VALUE||kind==OP_STORE_INDEXED||kind==OP_STORE
            ||kind==OP_ATOMIC||kind==OP_APPEND||kind==OP_PRINT||kind==OP_INVOKE
            ||kind==OP_TILE_PUBLISH||kind==OP_TILE_COLLECTIVE||kind==OP_TILE_BARRIER){return true;}
        else {
            DceResults live=dce_results(instruction,after,host);
            if(live.needed!=0){return true;}
        }
        index+=1u64;
    }
    return false;
}
bool eliminate_instruction(address instruction,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu
    requires nonzero(host.live.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block)
    requires nonzero(host.contains) requires nonzero(host.live.change)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set)
    requires nonzero(host.swap_instructions) requires nonzero(host.discard_prefix)
    requires nonzero(host.live.kill) requires nonzero(host.live.read) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    View value=host.live.ir.inspect(Node(instruction,N_INSTRUCTION));u32 kind=value.tag;
    // Copy actual scalar fields before requesting an exclusive physical owner.
    u64 first=value.a;u64 second=value.b;u64 index=value.c;u64 stored=value.d;u64 byte_offset=0u64;
    if(kind==OP_STORE_LOCAL_INDEXED){byte_offset=u64(unsafe_position<u32>(value.first.data));}
    if(kind==OP_TILE_COLLECTIVE){
        InstructionStorage storage=host.storage(instruction);host.prune_collective(storage.payload,needed);
    }
    if(kind==OP_INTEGER_DIVIDE){
        View output=host.live.ir.inspect(value.right);
        u64 quotient=output.a;u64 remainder=output.tag==2?output.b:output.a;
        u32 original=output.tag==0?1:(output.tag==1?2:3);u32 retained=0;
        if((original&1)!=0&&host.contains(needed,0,quotient)!=0){retained|=1;}
        if((original&2)!=0&&host.contains(needed,0,remainder)!=0){retained|=2;}
        if(retained==0){
            host.record(context,instruction,DceRemoval(DceRemovalKind::Division,0u64,0u64,0u64,0u64));
            return false;
        }
        if(retained!=original){
            DceDivisionChoice choice;
            if(retained==1){choice=DceDivisionChoice::Quotient;}
            else if(retained==2){choice=DceDivisionChoice::Remainder;}
            else{choice=DceDivisionChoice::Both;}
            InstructionStorage storage=host.storage(instruction);
            host.prune_division(instruction,storage.payload,choice,quotient,remainder);
        }
    }
    bool exempt=kind==OP_ATOMIC||kind==OP_APPEND||kind==OP_PRINT||kind==OP_INVOKE;
    if(!exempt){
        DceResults live=dce_results(instruction,needed,host);
        if(live.present!=0&&live.needed==0){
            host.record(context,instruction,DceRemoval(DceRemovalKind::Values,0u64,0u64,0u64,0u64));
            return false;
        }
    }
    if(kind==OP_STORE_LOCAL&&host.contains(needed,1,first)==0){
        host.record(context,instruction,DceRemoval(DceRemovalKind::DirectWrite,second,0u64,0u64,0u64));return false;
    }
    if(kind==OP_STORE_LOCAL_INDEXED&&!dce_any_local(needed,first,second,host.contains)){
        host.record(context,instruction,DceRemoval(DceRemovalKind::IndexedWrite,stored,index,second,byte_offset));return false;
    }
    if(kind==OP_SCOPE){
        InstructionStorage storage=host.storage(instruction);address after=host.live.clone_set(needed);
        eliminate_code(storage.first_code,needed,break_live,continue_live,after,empty,context,host);
        bool observable=dce_scope_observable(host.read_block(storage.first_code),after,host);
        if(!observable){host.live.change(LiveSetAction::Copy,needed,after,0u64);}
        host.live.drop_set(after);if(!observable){return false;}
    } else if(kind==OP_BRANCH){
        InstructionStorage storage=host.storage(instruction);address yes_needed=host.live.clone_set(needed);
        eliminate_code(storage.first_code,yes_needed,break_live,continue_live,exit_live,empty,context,host);
        eliminate_code(storage.second_code,needed,break_live,continue_live,exit_live,empty,context,host);
        u64 yes_count=host.read_block(storage.first_code).count;
        u64 no_count=host.read_block(storage.second_code).count;
        if(yes_count==0u64&&no_count==0u64){host.live.drop_set(yes_needed);return false;}
        host.live.change(LiveSetAction::Union,needed,yes_needed,0u64);host.live.drop_set(yes_needed);
        host.live.change(LiveSetAction::InsertRegister,needed,unsafe_address(0u64),first);
    } else if(kind==OP_LOOP_VALUE){
        address header=host.live.clone_set(empty);
        private_loop(value.first,first,value.second,needed,break_live,continue_live,exit_live,header,empty,host.live);
        InstructionStorage storage=host.storage(instruction);
        address before_body=host.live.clone_set(header);
        eliminate_code(storage.second_code,before_body,needed,header,exit_live,empty,context,host);
        host.live.change(LiveSetAction::Union,before_body,needed,0u64);
        host.live.change(LiveSetAction::InsertRegister,before_body,unsafe_address(0u64),first);
        eliminate_code(storage.first_code,before_body,break_live,continue_live,exit_live,empty,context,host);
        host.live.change(LiveSetAction::Swap,needed,before_body,0u64);host.live.drop_set(before_body);host.live.drop_set(header);
    } else {
        private_instruction(instruction,value,needed,break_live,continue_live,exit_live,empty,host.live);
    }
    return true;
}

extern "C" void eliminate_code(address block,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu
    requires nonzero(host.live.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block)
    requires nonzero(host.contains) requires nonzero(host.live.change)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set)
    requires nonzero(host.swap_instructions) requires nonzero(host.discard_prefix)
    requires nonzero(host.live.kill) requires nonzero(host.live.read) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    Span code=host.read_block(block);
    u64 read=code.count;
    u64 write=read;
    // The retained suffix is already ordered. No move touches an unread instruction.
    while(read!=0u64){
        read-=1u64;
        address instruction=block_instruction_at(code,read);
        if(eliminate_instruction(instruction,needed,break_live,continue_live,exit_live,empty,context,host)){
            write-=1u64;
            if(write!=read){
                host.swap_instructions(instruction,block_instruction_at(code,write));
            }
        }
    }
    if(write!=0u64){host.discard_prefix(block,write);}
}

extern "C" u64 value_root(address values,u64 reg,cfn<VfOptional,address,VfRegMap,u64> get) on cpu requires nonzero(get) {
    u64 result=reg;
    VfOptional next=get(values,VfRegMap::Aliases,result);
    while(next.present!=0){result=next.value;next=get(values,VfRegMap::Aliases,result);}
    return result;
}
extern "C" u32 value_memory_keeps(u32 mode,u64 binding,u64 index,u32 word,u64 target,u64 at,u32 field) on cpu {
    if(mode==0){return binding!=target ? 1 : 0;}
    return binding!=target || (index==at && word!=field) ? 1 : 0;
}
extern "C" void value_alias(address values,u32 kind,u64 first,u64 length,VfHost host) on cpu 
    requires nonzero(host.remove_range) requires nonzero(host.memory_filter) {
    if(kind==0){host.remove_range(values,first,first+length);}
    else {host.memory_filter(values,0,first,0u64,0);}
}
extern "C" void value_call_effect(address values,u32 kind,u64 first,u64 length,u32 writes,VfHost host) on cpu 
    requires nonzero(host.state) requires nonzero(host.remove_range) requires nonzero(host.memory_filter) {
    if(writes==0){return;}
    if(kind==0){host.remove_range(values,first,first+1u64);}
    else if(kind==1 || kind==4){host.memory_filter(values,0,first,0u64,0);}
    else if(kind==2){host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);}
    else {host.remove_range(values,first,first+length);}
}
void vf_merge(inout address left,address right,VfHost host) on cpu 
    requires nonzero(host.intersect) {
    if(address_bits(right)!=0u64){
        if(address_bits(left)!=0u64){host.intersect(left,right);}
        else {left=right;}
    }
}
bool vf_resume(address values,inout VfFlow flow,VfFlow step,VfHost host) on cpu 
    requires nonzero(host.intersect) requires nonzero(host.state) requires nonzero(host.apply) {
    vf_merge(flow.exits,step.exits,host);
    bool continuing=address_bits(step.next)!=0u64;
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Locals);
    if(continuing){host.apply(values,step.next);}
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);
    return continuing;
}
address vf_child(address values,VfHost host) on cpu 
    requires nonzero(host.state) {
    address child=host.state(VfStateOp::New,unsafe_address(0u64),unsafe_address(0u64),VfMap::Aliases);
    host.state(VfStateOp::Move,child,values,VfMap::Aliases);
    host.state(VfStateOp::Copy,child,values,VfMap::Expressions);
    host.state(VfStateOp::Move,child,values,VfMap::Locals);
    host.state(VfStateOp::Move,child,values,VfMap::Memory);
    return child;
}
VfExpression value_expression(View node,Span registers,RegisterHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
    address none=unsafe_address(0u64);
    u32 kind=node.tag;
    if(kind==OP_CONSTANT){return VfExpression(1,u32(node.c),node.b,0u64,0u64,none,0u64);}
    if(kind==OP_INDEX_VALUE){return VfExpression(2,0,0u64,0u64,0u64,none,0u64);}
    if(kind==OP_UNIFORM){return VfExpression(3,0,node.b,0u64,0u64,none,0u64);}
    if(kind==OP_BUFFER_STRIDE){return VfExpression(4,0,node.b,0u64,0u64,none,0u64);}
    if(kind==OP_UNARY){return VfExpression(5,u32(node.c),node.b,u64(host.scalar(registers,node.a).kind),0u64,none,0u64);}
    if(kind==OP_BINARY){return VfExpression(6,host.ir.inspect(node.right).tag,node.b,node.c,0u64,none,0u64);}
    if(kind==OP_CAST){return VfExpression(7,host.scalar(registers,node.a).kind,node.b,0u64,0u64,none,0u64);}
    if(kind==OP_MATH){return VfExpression(8,u32(node.b),0u64,0u64,0u64,node.first.data,node.first.count);}
    if(kind==OP_SELECT_VALUE){return VfExpression(9,0,node.b,node.c,node.d,none,0u64);}
    return VfExpression(0,0,0u64,0u64,0u64,none,0u64);
}
VfAction value_instruction(address instruction,address retained_at,address values,inout VfFlow flow,address context,Span registers,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    host.remap(instruction,values);
    View node=host.values.ir.inspect(Node(instruction,N_INSTRUCTION));
    u32 kind=node.tag;
    address none=unsafe_address(0u64);
    VfDivision empty=VfDivision(none,0u64,0u64,0,0);
    if(kind==OP_BRANCH || kind==OP_LOOP_VALUE || kind==OP_SCOPE || kind==OP_EXIT_VALUE || kind==OP_RAISE ||
        kind==OP_TILE_PUBLISH || kind==OP_TILE_COLLECTIVE || kind==OP_TILE_BARRIER || kind==OP_BREAK_VALUE || kind==OP_CONTINUE_VALUE){
        host.division(context,VfDivisionOp::Clear,0u64,0u64,0u64,empty);
    }
    if(kind==OP_INTEGER_DIVIDE){
        View result=host.values.ir.inspect(node.right);
        View divisor=host.values.ir.inspect(node.left);
        u32 mask=result.tag==2 ? 3 : result.tag+1;
        u64 quotient_result=result.tag==1 ? 0u64 : result.a;
        u64 remainder_result=result.tag==2 ? result.b : (result.tag==1 ? result.a : 0u64);
        u64 divisor_kind=u64(divisor.tag)+1u64;
        u32 retained=0;
        u32 paired=mask==3 ? 1 : 0;
        if((mask&1)!=0){
            VfExpression key=VfExpression(10,0,node.a,divisor_kind,divisor.a,none,0u64);
            VfOptional old=host.expression(values,key,quotient_result);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,quotient_result,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Quotient,paired));
            } else {retained|=1;}
        }
        if((mask&2)!=0){
            VfExpression key=VfExpression(10,1,node.a,divisor_kind,divisor.a,none,0u64);
            VfOptional old=host.expression(values,key,remainder_result);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,remainder_result,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Remainder,paired));
            } else {retained|=2;}
        }
        if(retained==0){return VfAction::Discard;}
        VfDivisionChoice choice;
        if(retained==1){choice=VfDivisionChoice::Quotient;}
        else if(retained==2){choice=VfDivisionChoice::Remainder;}
        else {choice=VfDivisionChoice::Both;}
        InstructionStorage storage=host.storage(instruction);
        host.rewrite_division(storage.payload,choice,quotient_result,remainder_result);
        VfDivision previous=host.division(context,VfDivisionOp::Take,node.a,divisor_kind,divisor.a,empty);
        if(previous.present!=0 && ((previous.mask==1 && retained==2)||(previous.mask==2 && retained==1))){
            u64 quotient=retained==1 ? quotient_result : previous.quotient;
            u64 remainder=retained==2 ? remainder_result : previous.remainder;
            address quotient_from=retained==1 ? instruction : previous.instruction;
            address remainder_from=retained==2 ? instruction : previous.instruction;
            host.pair(previous.instruction,quotient_from,remainder_from,storage.input,node.a,quotient,remainder);
            return VfAction::Discard;
        }
        if(retained!=3){
            VfDivision next=VfDivision(retained_at,quotient_result,remainder_result,retained,1);
            host.division(context,VfDivisionOp::Put,node.a,divisor_kind,divisor.a,next);
        }
    }
    VfExpression expression=value_expression(node,registers,host.values);
    if(expression.kind!=0){
        VfOptional old=host.expression(values,expression,node.a);
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.a,old.value);
            host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Value,0));
            return VfAction::Discard;
        }
    }
    bool continuing=true;
    if(kind==OP_SCOPE){
        address scoped=vf_child(values,host);
        VfFlow scope=host.block(host.storage(instruction).first_code,scoped,context);
        host.state(VfStateOp::Move,values,scoped,VfMap::Aliases);
        vf_merge(scope.next,scope.exits,host);
        scope.exits=none;
        continuing=vf_resume(values,flow,scope,host);
        host.state(VfStateOp::Drop,scoped,none,VfMap::Aliases);
    } else if(kind==OP_RAISE){continuing=false;}
    else if(kind==OP_EXIT_VALUE){vf_merge(flow.exits,host.snapshot(values),host);continuing=false;}
    else if(kind==OP_BREAK_VALUE || kind==OP_CONTINUE_VALUE){continuing=false;}
    else if(kind==OP_ATOMIC){
        if(host.values.ir.inspect(node.right).tag!=0){host.aliases(values,node.left.data);}
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
    } else if(kind==OP_APPEND || kind==OP_TILE_BARRIER){host.state(VfStateOp::Clear,values,none,VfMap::Memory);}
    else if(kind==OP_LOAD_LOCAL){
        VfOptional old=host.get(values,VfRegMap::Locals,node.b);
        if(old.present!=0){
            u64 value=value_root(values,old.value,host.get);
            host.put(values,VfRegMap::Aliases,node.a,value);
            host.note(context,value,instruction,VfReuse(none,VfReuseKind::LocalRead,VfRole::Value,0));
        }
        host.put(values,VfRegMap::Locals,node.b,node.a);
    } else if(kind==OP_STORE_LOCAL){host.put(values,VfRegMap::Locals,node.a,node.b);}
    else if(kind==OP_STORE_LOCAL_INDEXED){host.remove_range(values,node.a,node.a+node.b);}
    else if(kind==OP_STORE_INDEXED){host.aliases(values,node.left.data);}
    else if(kind==OP_LOAD){
        VfMemory old=host.memory_get(values,node.b,node.c,u32(node.d));
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.a,old.value);
            VfReuseKind reuse;
            if(address_bits(old.origin)!=0u64){reuse=VfReuseKind::IndexedStoreRead;}
            else {reuse=VfReuseKind::IndexedRead;}
            host.note(context,old.value,instruction,VfReuse(old.origin,reuse,VfRole::Value,0));
            return VfAction::Discard;
        }
        host.memory_put(values,node.b,node.c,u32(node.d),node.a,none);
    } else if(kind==OP_STORE){
        host.memory_filter(values,1,node.a,node.b,u32(node.c));
        host.memory_put(values,node.a,node.b,u32(node.c),node.d,instruction);
    } else if(kind==OP_BRANCH){
        address yes=host.state(VfStateOp::Clone,none,values,VfMap::Aliases);
        address no=vf_child(values,host);
        InstructionStorage storage=host.storage(instruction);
        VfFlow branches=host.block(storage.first_code,yes,context);
        VfFlow other=host.block(storage.second_code,no,context);
        vf_merge(branches.next,other.next,host);
        vf_merge(branches.exits,other.exits,host);
        host.state(VfStateOp::Move,values,yes,VfMap::Aliases);
        host.state(VfStateOp::Extend,values,no,VfMap::Aliases);
        continuing=vf_resume(values,flow,branches,host);
        host.state(VfStateOp::Drop,yes,none,VfMap::Aliases);
        host.state(VfStateOp::Drop,no,none,VfMap::Aliases);
    } else if(kind==OP_LOOP_VALUE){
        InstructionStorage storage=host.storage(instruction);
        host.written(values,storage.first_code,storage.second_code);
        address after=host.snapshot(values);
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
        address cycle=vf_child(values,host);
        VfFlow condition=host.block(storage.first_code,cycle,context);
        unsafe_position<u64>(storage.payload)=value_root(cycle,node.a,host.get);
        VfFlow loop_flow=VfFlow(none,condition.exits);
        if(address_bits(condition.next)!=0u64){
            host.drop_snapshot(condition.next);
            loop_flow.next=after;
            VfFlow body=host.block(storage.second_code,cycle,context);
            if(address_bits(body.next)!=0u64){host.drop_snapshot(body.next);}
            vf_merge(loop_flow.exits,body.exits,host);
        } else {host.drop_snapshot(after);}
        host.state(VfStateOp::Move,values,cycle,VfMap::Aliases);
        continuing=vf_resume(values,flow,loop_flow,host);
        host.state(VfStateOp::Drop,cycle,none,VfMap::Aliases);
    } else if(kind==OP_INVOKE){host.call(values,node.left.data);}
    return continuing ? VfAction::Keep : VfAction::Stop;
}
extern "C" VfFlow value_block(address block,address values,address context,Span registers,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate) {
    Span code=host.instruction_span(block);
    address none=unsafe_address(0u64);
    VfFlow flow=VfFlow(none,none);
    u64 kept=0u64;
    u64 index=0u64;
    bool continuing=true;
    // The retained prefix never moves again, including slots awaiting a division pair.
    while(index<code.count){
        address instruction=block_instruction_at(code,index);
        address retained_at=block_instruction_at(code,kept);
        VfAction action=value_instruction(instruction,retained_at,values,flow,context,registers,host);
        if(action!=VfAction::Discard){
            if(kept!=index){host.swap_instructions(instruction,retained_at);}
            kept+=1u64;
            if(action==VfAction::Stop){continuing=false;break;}
        }
        index+=1u64;
    }
    if(kept!=code.count){host.truncate(block,kept);}
    if(continuing){flow.next=host.snapshot(values);}
    return flow;
}
extern "C" void value_discard_flow(VfFlow flow,VfHost host) on cpu 
    requires nonzero(host.drop_snapshot) {
    if(address_bits(flow.next)!=0u64){host.drop_snapshot(flow.next);}
    if(address_bits(flow.exits)!=0u64){host.drop_snapshot(flow.exits);}
}
