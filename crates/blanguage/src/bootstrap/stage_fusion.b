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

extern "C" void private_reference(LiveReferenceKind kind,address live,u64 slot,
    cfn<void,LiveSetAction,address,address,u64> change) on cpu requires nonzero(change) {
    LiveSetAction action;
    if (kind==LiveReferenceKind::Result) { action=LiveSetAction::RemoveRegister; }
    else if (kind==LiveReferenceKind::Operand) { action=LiveSetAction::InsertRegister; }
    else { action=LiveSetAction::InsertLocal; }
    change(action,live,unsafe_address(0u64),slot);
}

extern "C" void private_loop(FusionBlock condition,u64 test,FusionBlock body,
    address after,address break_live,address continue_live,address exit_live,
    address result,address empty,LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
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

extern "C" void private_before(FusionBlock code,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    u64 index=code.count;
    while (index!=0u64) {
        index-=1u64;
        LiveNode node=host.read_node(code,index);
        if (node.kind==LiveKind::If) {
            address first=host.clone_set(live);
            private_before(node.first,first,break_live,continue_live,exit_live,empty,host);
            private_before(node.second,live,break_live,continue_live,exit_live,empty,host);
            host.change(LiveSetAction::Union,live,first,0u64);
            host.drop_set(first);
            host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),node.first_slot);
        } else if (node.kind==LiveKind::Loop) {
            private_loop(node.first,node.first_slot,node.second,live,break_live,continue_live,exit_live,live,empty,host);
        } else if (node.kind==LiveKind::Scope) {
            address after=host.clone_set(live);
            private_before(node.first,live,break_live,continue_live,after,empty,host);
            host.drop_set(after);
        } else if (node.kind==LiveKind::Exit) {
            host.change(LiveSetAction::Copy,live,exit_live,0u64);
        } else if (node.kind==LiveKind::Break) {
            host.change(LiveSetAction::Copy,live,break_live,0u64);
        } else if (node.kind==LiveKind::Continue) {
            host.change(LiveSetAction::Copy,live,continue_live,0u64);
        } else if (node.kind==LiveKind::LocalRead) {
            host.change(LiveSetAction::RemoveRegister,live,unsafe_address(0u64),node.first_slot);
            host.change(LiveSetAction::InsertLocal,live,unsafe_address(0u64),node.second_slot);
        } else if (node.kind==LiveKind::LocalWrite) {
            host.change(LiveSetAction::RemoveLocal,live,unsafe_address(0u64),node.first_slot);
            host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),node.second_slot);
        } else {
            host.references(node.instruction,live);
        }
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
DceResults dce_results(address instruction,address needed,DceHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block) requires nonzero(host.contains) requires nonzero(host.change) requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block) requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record) requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    DceResults output=DceResults(0,0);
    Results values=results(Node(instruction,N_INSTRUCTION),host.ir);
    while(true){
        Word value=next_result(values);if(value.present==0){break;}
        if(dce_result(output,needed,value.value,host.contains)==0){break;}
    }
    return output;
}
bool dce_scope_observable(Span code,address after,DceHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block) requires nonzero(host.contains) requires nonzero(host.change) requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block) requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record) requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    u64 index=0u64;
    while(index<code.count){
        address instruction=unsafe_address(address_bits(code.data)+index*u64(code.stride));
        View value=host.ir.inspect(Node(instruction,N_INSTRUCTION));u32 kind=value.tag;
        if(kind==OP_STORE_LOCAL){
            if(host.contains(after,1,value.a)!=0){return true;}
        } else if(kind==OP_STORE_LOCAL_INDEXED){
            if(dce_any_local(after,value.a,value.b,host.contains)){return true;}
        } else if(kind==OP_SCOPE){
            DceStorage storage=host.storage(instruction);
            if(dce_scope_observable(host.read_block(storage.first_code),after,host)){return true;}
        } else if(kind==OP_BRANCH){
            DceStorage storage=host.storage(instruction);
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
extern "C" bool eliminate_instruction(address instruction,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block) requires nonzero(host.contains) requires nonzero(host.change) requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block) requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record) requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    View value=host.ir.inspect(Node(instruction,N_INSTRUCTION));u32 kind=value.tag;
    // Copy actual scalar fields before requesting an exclusive physical owner.
    u64 first=value.a;u64 second=value.b;u64 index=value.c;u64 stored=value.d;u64 byte_offset=0u64;
    if(kind==OP_STORE_LOCAL_INDEXED){byte_offset=u64(unsafe_position<u32>(value.first.data));}
    if(kind==OP_TILE_COLLECTIVE){
        DceStorage storage=host.storage(instruction);host.prune_collective(storage.payload,needed);
    }
    if(kind==OP_INTEGER_DIVIDE){
        View output=host.ir.inspect(value.right);
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
            DceStorage storage=host.storage(instruction);
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
        DceStorage storage=host.storage(instruction);address after=host.clone_set(needed);
        host.eliminate_block(storage.first_code,needed,break_live,continue_live,after,context);
        bool observable=dce_scope_observable(host.read_block(storage.first_code),after,host);
        if(!observable){host.change(LiveSetAction::Copy,needed,after,0u64);}
        host.drop_set(after);if(!observable){return false;}
    } else if(kind==OP_BRANCH){
        DceStorage storage=host.storage(instruction);address yes_needed=host.clone_set(needed);
        host.eliminate_block(storage.first_code,yes_needed,break_live,continue_live,exit_live,context);
        host.eliminate_block(storage.second_code,needed,break_live,continue_live,exit_live,context);
        u64 yes_count=host.read_block(storage.first_code).count;
        u64 no_count=host.read_block(storage.second_code).count;
        if(yes_count==0u64&&no_count==0u64){host.drop_set(yes_needed);return false;}
        host.change(LiveSetAction::Union,needed,yes_needed,0u64);host.drop_set(yes_needed);
        host.change(LiveSetAction::InsertRegister,needed,unsafe_address(0u64),first);
    } else if(kind==OP_LOOP_VALUE){
        DceStorage storage=host.storage(instruction);
        Span condition=host.read_block(storage.first_code);Span body=host.read_block(storage.second_code);
        address header=host.loop_live(FusionBlock(condition.data,condition.count),first,FusionBlock(body.data,body.count),needed,break_live,continue_live,exit_live);
        address before_body=host.clone_set(header);
        host.eliminate_block(storage.second_code,before_body,needed,header,exit_live,context);
        host.change(LiveSetAction::Union,before_body,needed,0u64);
        host.change(LiveSetAction::InsertRegister,before_body,unsafe_address(0u64),first);
        host.eliminate_block(storage.first_code,before_body,break_live,continue_live,exit_live,context);
        host.change(LiveSetAction::Swap,needed,before_body,0u64);host.drop_set(before_body);host.drop_set(header);
    } else {
        host.live_before(FusionBlock(instruction,1u64),needed,break_live,continue_live,exit_live);
    }
    return true;
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
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(kind==0){host.remove_range(values,first,first+length);}
    else {host.memory_filter(values,0,first,0u64,0);}
}
extern "C" void value_call_effect(address values,u32 kind,u64 first,u64 length,u32 writes,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(writes==0){return;}
    if(kind==0){host.remove_range(values,first,first+1u64);}
    else if(kind==1 || kind==4){host.memory_filter(values,0,first,0u64,0);}
    else if(kind==2){host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);}
    else {host.remove_range(values,first,first+length);}
}
void vf_merge(inout address left,address right,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(address_bits(right)!=0u64){
        if(address_bits(left)!=0u64){host.intersect(left,right);}
        else {left=right;}
    }
}
bool vf_resume(address values,inout VfFlow flow,VfFlow step,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    vf_merge(flow.exits,step.exits,host);
    bool continuing=address_bits(step.next)!=0u64;
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Locals);
    if(continuing){host.apply(values,step.next);}
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);
    return continuing;
}
address vf_child(address values,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    address child=host.state(VfStateOp::New,unsafe_address(0u64),unsafe_address(0u64),VfMap::Aliases);
    host.state(VfStateOp::Move,child,values,VfMap::Aliases);
    host.state(VfStateOp::Copy,child,values,VfMap::Expressions);
    host.state(VfStateOp::Move,child,values,VfMap::Locals);
    host.state(VfStateOp::Move,child,values,VfMap::Memory);
    return child;
}
extern "C" VfAction value_instruction(address instruction,address values,inout VfFlow flow,address context,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    host.remap(instruction,values);
    VfNode node=host.read(instruction,context);
    address none=unsafe_address(0u64);
    VfDivision empty=VfDivision(0u64,0u64,0u64,0,0);
    if(node.kind==VfKind::If || node.kind==VfKind::Loop || node.kind==VfKind::Scope || node.kind==VfKind::Exit || node.kind==VfKind::Raise ||
        node.kind==VfKind::Publish || node.kind==VfKind::Collective || node.kind==VfKind::Barrier || node.kind==VfKind::Break || node.kind==VfKind::Continue){
        host.division(context,VfDivisionOp::Clear,0u64,0u64,0u64,empty);
    }
    if(node.kind==VfKind::Division){
        u32 retained=0;
        u32 paired=node.flags==3 ? 1 : 0;
        if((node.flags&1)!=0){
            VfExpression key=VfExpression(10,0,node.first,node.second,node.third,node.arguments,0u64);
            VfOptional old=host.expression(values,key,node.dst);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,node.dst,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Quotient,paired));
            } else {retained|=1;}
        }
        if((node.flags&2)!=0){
            VfExpression key=VfExpression(10,1,node.first,node.second,node.third,node.arguments,0u64);
            VfOptional old=host.expression(values,key,node.extra);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,node.extra,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Remainder,paired));
            } else {retained|=2;}
        }
        if(retained==0){return VfAction::Discard;}
        VfDivisionChoice choice;
        if(retained==1){choice=VfDivisionChoice::Quotient;}
        else if(retained==2){choice=VfDivisionChoice::Remainder;}
        else {choice=VfDivisionChoice::Both;}
        host.rewrite_division(node.division_result,choice,node.dst,node.extra);
        VfDivision previous=host.division(context,VfDivisionOp::Take,node.first,node.second,node.third,empty);
        if(previous.present!=0 && ((previous.mask==1 && retained==2)||(previous.mask==2 && retained==1))){
            u64 quotient=retained==1 ? node.dst : previous.quotient;
            u64 remainder=retained==2 ? node.extra : previous.remainder;
            host.pair(context,instruction,previous,node.divisor,node.first,quotient,remainder);
            return VfAction::Discard;
        }
        if(retained!=3){
            VfDivision next=VfDivision(host.position(context),node.dst,node.extra,retained,1);
            host.division(context,VfDivisionOp::Put,node.first,node.second,node.third,next);
        }
    }
    VfExpression expression=VfExpression(0,node.operation,node.first,node.second,node.third,node.arguments,0u64);
    if(node.kind==VfKind::Constant){expression.kind=1;}
    else if(node.kind==VfKind::Index){expression.kind=2;}
    else if(node.kind==VfKind::Uniform){expression.kind=3;}
    else if(node.kind==VfKind::BufferStride){expression.kind=4;}
    else if(node.kind==VfKind::Unary){expression.kind=5;}
    else if(node.kind==VfKind::Binary){expression.kind=6;}
    else if(node.kind==VfKind::Cast){expression.kind=7;}
    else if(node.kind==VfKind::Math){expression.kind=8;expression.count=node.count;}
    else if(node.kind==VfKind::Select){expression.kind=9;}
    if(expression.kind!=0){
        VfOptional old=host.expression(values,expression,node.dst);
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.dst,old.value);
            host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Value,0));
            return VfAction::Discard;
        }
    }
    bool continuing=true;
    if(node.kind==VfKind::Scope){
        address scoped=vf_child(values,host);
        VfFlow scope=host.block(node.first_code,scoped,context);
        host.state(VfStateOp::Move,values,scoped,VfMap::Aliases);
        vf_merge(scope.next,scope.exits,host);
        scope.exits=none;
        continuing=vf_resume(values,flow,scope,host);
        host.state(VfStateOp::Drop,scoped,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Raise){continuing=false;}
    else if(node.kind==VfKind::Exit){vf_merge(flow.exits,host.snapshot(values),host);continuing=false;}
    else if(node.kind==VfKind::Break || node.kind==VfKind::Continue){continuing=false;}
    else if(node.kind==VfKind::Atomic){
        if(node.flags!=0){host.aliases(values,node.payload);}
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
    } else if(node.kind==VfKind::Append || node.kind==VfKind::Barrier){host.state(VfStateOp::Clear,values,none,VfMap::Memory);}
    else if(node.kind==VfKind::LocalRead){
        VfOptional old=host.get(values,VfRegMap::Locals,node.first);
        if(old.present!=0){
            u64 value=value_root(values,old.value,host.get);
            host.put(values,VfRegMap::Aliases,node.dst,value);
            host.note(context,value,instruction,VfReuse(none,VfReuseKind::LocalRead,VfRole::Value,0));
        }
        host.put(values,VfRegMap::Locals,node.first,node.dst);
    } else if(node.kind==VfKind::LocalWrite){host.put(values,VfRegMap::Locals,node.first,node.second);}
    else if(node.kind==VfKind::LocalIndexedWrite){host.remove_range(values,node.first,node.first+node.second);}
    else if(node.kind==VfKind::IndexedWrite){host.aliases(values,node.payload);}
    else if(node.kind==VfKind::Load){
        VfMemory old=host.memory_get(values,node.first,node.second,u32(node.third));
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.dst,old.value);
            VfReuseKind kind;
            if(address_bits(old.origin)!=0u64){kind=VfReuseKind::IndexedStoreRead;}
            else {kind=VfReuseKind::IndexedRead;}
            host.note(context,old.value,instruction,VfReuse(old.origin,kind,VfRole::Value,0));
            return VfAction::Discard;
        }
        host.memory_put(values,node.first,node.second,u32(node.third),node.dst,none);
    } else if(node.kind==VfKind::Store){
        host.memory_filter(values,1,node.first,node.second,u32(node.third));
        host.memory_put(values,node.first,node.second,u32(node.third),node.extra,instruction);
    } else if(node.kind==VfKind::If){
        address yes=host.state(VfStateOp::Clone,none,values,VfMap::Aliases);
        address no=vf_child(values,host);
        VfFlow branches=host.block(node.first_code,yes,context);
        VfFlow other=host.block(node.second_code,no,context);
        vf_merge(branches.next,other.next,host);
        vf_merge(branches.exits,other.exits,host);
        host.state(VfStateOp::Move,values,yes,VfMap::Aliases);
        host.state(VfStateOp::Extend,values,no,VfMap::Aliases);
        continuing=vf_resume(values,flow,branches,host);
        host.state(VfStateOp::Drop,yes,none,VfMap::Aliases);
        host.state(VfStateOp::Drop,no,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Loop){
        host.written(values,node.first_code,node.second_code);
        address after=host.snapshot(values);
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
        address cycle=vf_child(values,host);
        VfFlow condition=host.block(node.first_code,cycle,context);
        unsafe_position<u64>(node.arguments)=value_root(cycle,unsafe_position<u64>(node.arguments),host.get);
        VfFlow loop_flow=VfFlow(none,condition.exits);
        if(address_bits(condition.next)!=0u64){
            host.drop_snapshot(condition.next);
            loop_flow.next=after;
            VfFlow body=host.block(node.second_code,cycle,context);
            if(address_bits(body.next)!=0u64){host.drop_snapshot(body.next);}
            vf_merge(loop_flow.exits,body.exits,host);
        } else {host.drop_snapshot(after);}
        host.state(VfStateOp::Move,values,cycle,VfMap::Aliases);
        continuing=vf_resume(values,flow,loop_flow,host);
        host.state(VfStateOp::Drop,cycle,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Call){host.call(values,node.payload);}
    return continuing ? VfAction::Keep : VfAction::Stop;
}
extern "C" VfFlow value_finish(address values,VfFlow flow,u32 continuing,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    address next=flow.next;
    if(continuing!=0){next=host.snapshot(values);}
    return VfFlow(next,flow.exits);
}
extern "C" void value_discard_flow(VfFlow flow,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(address_bits(flow.next)!=0u64){host.drop_snapshot(flow.next);}
    if(address_bits(flow.exits)!=0u64){host.drop_snapshot(flow.exits);}
}
