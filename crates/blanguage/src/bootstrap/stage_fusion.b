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

address tile_placement_at(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<TilePlacement>());
}

address tile_member_at(address members, u64 index) on cpu {
    return unsafe_address(address_bits(members) + index * size_of<TileMember>());
}

extern "C" TileExtents place_tile(address rows, u64 count, address members, address context,
    cfn<TilePlanes,address,u64,u64> reserve) on cpu requires nonzero(reserve) {
    u64 narrow_capacity = 0u64;
    u64 wide_capacity = 0u64;
    u64 index = 0u64;
    while (index < count) {
        address row = tile_placement_at(rows, index);
        TilePlacement item = unsafe_position<TilePlacement>(row);
        let first = unsafe_position<TileMember>(tile_member_at(members, item.first)).byte;
        let last = unsafe_position<TileMember>(tile_member_at(members, item.first + item.count - 1u64)).byte;
        let shift = item.plane == 0u64 ? 2u64 : 3u64;
        item.width = ((last - first) >> shift) + 1u64;
        unsafe_position<TilePlacement>(row) = item;
        if (item.plane == 0u64) {
            narrow_capacity += item.width;
        } else {
            wide_capacity += item.width;
        }
        index += 1u64;
    }
    let planes = reserve(context, narrow_capacity, wide_capacity);
    u64 narrow_used = 0u64;
    u64 wide_used = 0u64;
    index = 0u64;
    while (index < count) {
        TilePlacement item = unsafe_position<TilePlacement>(tile_placement_at(rows, index));
        address plane = planes.narrow;
        u64 used = narrow_used;
        if (item.plane != 0u64) {
            plane = planes.wide;
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
        let first = unsafe_position<TileMember>(tile_member_at(members, item.first)).byte;
        let shift = item.plane == 0u64 ? 2u64 : 3u64;
        u64 member = 0u64;
        while (member < item.count) {
            address row = tile_member_at(members, item.first + member);
            TileMember word = unsafe_position<TileMember>(row);
            word.slot = slot + ((word.byte - first) >> shift);
            unsafe_position<TileMember>(row) = word;
            member += 1u64;
        }
        index += 1u64;
    }
    return TileExtents(narrow_used, wide_used);
}

bool overwrites_stage_guarantee(address writes, StageGuarantee guarantee, GuaranteeHost host) on cpu
    requires nonzero(host.seek) requires nonzero(host.next) {
    WriteEnd write = host.seek(writes, guarantee.binding, guarantee.end);
    while (write.present != 0) {
        if (u64(write.end) > guarantee.start) { return true; }
        write = host.next(writes);
    }
    return false;
}

u64 retain_guarantee_kind(address writes, GuaranteeKind kind, u64 count, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots)
    requires nonzero(host.seek) requires nonzero(host.next) {
    u64 retained = 0u64;
    u64 index = 0u64;
    while (index < count) {
        let guarantee = host.field(host.context, kind, index);
        if (!overwrites_stage_guarantee(writes, guarantee, host)) {
            if (index != retained) { host.swap_slots(host.context, kind, index, retained); }
            retained += 1u64;
        }
        index += 1u64;
    }
    return retained;
}

extern "C" GuaranteeCounts retain_stage_guarantees(address writes, GuaranteeCounts counts, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots)
    requires nonzero(host.seek) requires nonzero(host.next) {
    let prefix = retain_guarantee_kind(writes, GuaranteeKind::Prefix, counts.prefix, host);
    let element = retain_guarantee_kind(writes, GuaranteeKind::Element, counts.element, host);
    return GuaranteeCounts(prefix, element);
}

address local_value_at(address values, u64 index) on cpu {
    return unsafe_address(address_bits(values) + index * size_of<LocalValue>());
}

u64 intersect_local_values(address left, u64 left_count,
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

void private_kill(View value,address live,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.change) {
    Results values=view_results(value,host.ir);
    Word reg=next_result(values);
    while(reg.present!=0){
        host.change(LiveSetAction::RemoveRegister,live,unsafe_address(0u64),reg.value);
        reg=next_result(values);
    }
}

void private_reads(View value,address live,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register) requires nonzero(host.change) {
    visit_instruction(value,host.ir,live,host.insert_register);
    Locals reads=view_locals(value,0,host.ir);
    Word slot=next_local(reads,host.ir);
    while(slot.present!=0){
        host.change(LiveSetAction::InsertLocal,live,unsafe_address(0u64),slot.value);
        slot=next_local(reads,host.ir);
    }
}

extern "C" void private_loop(Span condition,u64 test,Span body,
    address after,address break_live,address continue_live,address exit_live,
    address result,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    // Pointwise gen/kill transfers close the backedge from an empty header.
    // after may be result: it remains borrowed until the final swap.
    address need=host.clone_set(empty);
    private_before(body,need,after,empty,exit_live,empty,host);
    host.change(LiveSetAction::Union,need,after,0u64);
    host.insert_register(need,test);
    private_before(condition,need,break_live,continue_live,exit_live,empty,host);
    host.change(LiveSetAction::Swap,result,need,0u64);
    host.drop_set(need);
}

void private_instruction(View value,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    if (value.tag==OP_BRANCH) {
        address first=host.clone_set(live);
        private_before(value.first,first,break_live,continue_live,exit_live,empty,host);
        private_before(value.second,live,break_live,continue_live,exit_live,empty,host);
        host.change(LiveSetAction::Union,live,first,0u64);
        host.drop_set(first);
        host.insert_register(live,value.a);
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
        host.insert_register(live,value.b);
    } else {
        private_kill(value,live,host);
        private_reads(value,live,host);
    }
}

extern "C" void private_before(Span code,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    u64 index=code.count;
    while (index!=0u64) {
        index-=1u64;
        address instruction=span_item_at(code,index);
        View value=host.ir.inspect(Node(instruction,N_INSTRUCTION));
        private_instruction(value,live,break_live,continue_live,exit_live,empty,host);
    }
}

extern "C" void private_tile_phases(Span code, address live, address saved, address empty,
    LiveHost host, address context, cfn<void,address,u64,u64,address,address> emit) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change)
    requires nonzero(emit) {
    u64 index = code.count;
    u64 end = code.count;
    while (index != 0u64) {
        index -= 1u64;
        View value = host.ir.inspect(Node(span_item_at(code,index),N_INSTRUCTION));
        if (tile_phase_step(value.tag)) {
            u64 begin = index + 1u64;
            if (end == code.count) {
                Span tail = Span(span_item_at(code,begin), code.count - begin, code.stride);
                private_before(tail,live,empty,empty,empty,empty,host);
            }
            emit(context,begin,end,live,saved);
            // The step produces its own results after saving the other live slots.
            private_kill(value,live,host);
            host.change(LiveSetAction::Copy,saved,live,0u64);
            private_reads(value,live,host);
            end = index;
        } else if (end != code.count) {
            private_instruction(value,live,empty,empty,empty,empty,host);
        }
    }
    if (end != code.count) {
        emit(context,0u64,end,live,saved);
    }
}

u32 dce_result(inout DceResults results,address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    results.present=1;
    results.needed=contains(needed,0,reg);
    return results.needed==0 ? 1 : 0;
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
DceResults dce_results(View instruction,address needed,DceHost host) on cpu requires nonzero(host.live.ir.inspect) requires nonzero(host.contains) {
    DceResults output=DceResults(0,0);
    Results values=view_results(instruction,host.live.ir);
    while(true){
        Word value=next_result(values);if(value.present==0){break;}
        if(dce_result(output,needed,value.value,host.contains)==0){break;}
    }
    return output;
}
address span_item_at(Span code,u64 index) on cpu {
    return unsafe_address(address_bits(code.data)+index*u64(code.stride));
}

bool dce_scope_observable(Span code,address after,DceHost host) on cpu requires nonzero(host.live.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block) requires nonzero(host.contains) {
    u64 index=0u64;
    while(index<code.count){
        address instruction=span_item_at(code,index);
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
            DceResults live=dce_results(value,after,host);
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
    requires nonzero(host.live.insert_register) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    View value=host.live.ir.inspect(Node(instruction,N_INSTRUCTION));u32 kind=value.tag;
    // Copy actual scalar fields before requesting an exclusive physical owner.
    u64 first=value.a;u64 second=value.b;u64 index=value.c;u64 stored=value.d;u64 byte_offset=0u64;
    if(kind==OP_STORE_LOCAL_INDEXED){byte_offset=u64(unsafe_position<u32>(value.first.data));}
    if(kind==OP_TILE_COLLECTIVE){
        InstructionStorage storage=host.storage(instruction);host.prune_collective(storage.payload,needed);
        value=host.live.ir.inspect(Node(instruction,N_INSTRUCTION));
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
            value=host.live.ir.inspect(Node(instruction,N_INSTRUCTION));
        }
    }
    bool exempt=kind==OP_ATOMIC||kind==OP_APPEND||kind==OP_PRINT||kind==OP_INVOKE;
    if(!exempt){
        DceResults live=dce_results(value,needed,host);
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
        host.live.insert_register(needed,first);
    } else if(kind==OP_LOOP_VALUE){
        address header=host.live.clone_set(empty);
        private_loop(value.first,first,value.second,needed,break_live,continue_live,exit_live,header,empty,host.live);
        InstructionStorage storage=host.storage(instruction);
        address before_body=host.live.clone_set(header);
        eliminate_code(storage.second_code,before_body,needed,header,exit_live,empty,context,host);
        host.live.change(LiveSetAction::Union,before_body,needed,0u64);
        host.live.insert_register(before_body,first);
        eliminate_code(storage.first_code,before_body,break_live,continue_live,exit_live,empty,context,host);
        host.live.change(LiveSetAction::Swap,needed,before_body,0u64);host.live.drop_set(before_body);host.live.drop_set(header);
    } else {
        private_instruction(value,needed,break_live,continue_live,exit_live,empty,host.live);
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
    requires nonzero(host.live.insert_register) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    Span code=host.read_block(block);
    u64 read=code.count;
    u64 write=read;
    // The retained suffix is already ordered. No move touches an unread instruction.
    while(read!=0u64){
        read-=1u64;
        address instruction=span_item_at(code,read);
        if(eliminate_instruction(instruction,needed,break_live,continue_live,exit_live,empty,context,host)){
            write-=1u64;
            if(write!=read){
                host.swap_instructions(instruction,span_item_at(code,write));
            }
        }
    }
    if(write!=0u64){host.discard_prefix(block,write);}
}


VfRows vf_empty_rows() on cpu {
    return VfRows(0u64,0u64,0u64);
}

address vf_rows_at(address values,VfRegMap map) on cpu {
    return map==VfRegMap::Aliases ? values : unsafe_address(address_bits(values)+size_of<VfRows>());
}

address vf_row(VfRows rows,u64 index) on cpu {
    return unsafe_address(rows.data+index*size_of<LocalValue>());
}

u64 vf_lower(VfRows rows,u64 slot) on cpu {
    u64 start=0u64;
    u64 end=rows.count;
    while(start<end){
        u64 middle=start+(end-start)/2u64;
        if(unsafe_position<LocalValue>(vf_row(rows,middle)).slot<slot){start=middle+1u64;}
        else{end=middle;}
    }
    return start;
}

void vf_reserve(inout VfRows rows,u64 required,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    if(required>rows.capacity){
        u64 capacity=rows.capacity==0u64 ? 4u64 : rows.capacity*2u64;
        if(capacity<required){capacity=required;}
        rows.data=address_bits(resize(unsafe_address(rows.data),rows.capacity,capacity));
        rows.capacity=capacity;
    }
}

void vf_release(VfRows rows,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    if(rows.capacity!=0u64){resize(unsafe_address(rows.data),rows.capacity,0u64);}
}

void vf_move_rows(address target,address source,VfRegMap map,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    address dest=vf_rows_at(target,map);
    address origin=vf_rows_at(source,map);
    vf_release(unsafe_position<VfRows>(dest),resize);
    unsafe_position<VfRows>(dest)=unsafe_position<VfRows>(origin);
    unsafe_position<VfRows>(origin)=vf_empty_rows();
}

VfRows vf_copy_rows(VfRows original,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfRows copy=vf_empty_rows();
    vf_reserve(copy,original.count,resize);
    u64 index=0u64;
    while(index<original.count){
        unsafe_position<LocalValue>(vf_row(copy,index))=unsafe_position<LocalValue>(vf_row(original,index));
        index+=1u64;
    }
    copy.count=original.count;
    return copy;
}

extern "C" void value_copy_registers(address target,address source,u32 locals,
    cfn<address,address,u64,u64> resize) on cpu requires nonzero(resize) {
    VfRegisters original=unsafe_position<VfRegisters>(source);
    VfRows aliases=vf_copy_rows(original.aliases,resize);
    VfRows local=locals!=0 ? vf_copy_rows(original.locals,resize) : vf_empty_rows();
    unsafe_position<VfRegisters>(target)=VfRegisters(aliases,local);
}

extern "C" void value_release_registers(address values,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfRegisters registers=unsafe_position<VfRegisters>(values);
    vf_release(registers.aliases,resize);
    vf_release(registers.locals,resize);
}

extern "C" void value_merge_aliases(address target,address source,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfRows left=unsafe_position<VfRegisters>(target).aliases;
    VfRows right=unsafe_position<VfRegisters>(source).aliases;
    if(right.count==0u64){return;}
    u64 size=left.count+right.count;
    vf_reserve(left,size,resize);
    u64 a=left.count;
    u64 b=right.count;
    u64 at=size;
    // Retain RHS bindings on duplicate keys, as for the original extend.
    while(b!=0u64){
        LocalValue right_row=unsafe_position<LocalValue>(vf_row(right,b-1u64));
        while(a!=0u64 && unsafe_position<LocalValue>(vf_row(left,a-1u64)).slot>right_row.slot){
            a-=1u64;
            at-=1u64;
            unsafe_position<LocalValue>(vf_row(left,at))=unsafe_position<LocalValue>(vf_row(left,a));
        }
        if(a!=0u64 && unsafe_position<LocalValue>(vf_row(left,a-1u64)).slot==right_row.slot){a-=1u64;}
        b-=1u64;
        at-=1u64;
        unsafe_position<LocalValue>(vf_row(left,at))=right_row;
    }
    u64 size_out=a+size-at;
    while(at<size){
        unsafe_position<LocalValue>(vf_row(left,a))=unsafe_position<LocalValue>(vf_row(left,at));
        a+=1u64;
        at+=1u64;
    }
    left.count=size_out;
    unsafe_position<VfRegisters>(target).aliases=left;
}

VfOptional vf_get(address values,VfRegMap map,u64 slot) on cpu {
    VfRows rows=unsafe_position<VfRows>(vf_rows_at(values,map));
    u64 index=vf_lower(rows,slot);
    if(index<rows.count){
        LocalValue row=unsafe_position<LocalValue>(vf_row(rows,index));
        if(row.slot==slot){return VfOptional(row.value,1);}
    }
    return VfOptional(0u64,0);
}

void vf_put(address values,VfRegMap map,u64 slot,u64 value,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    address state=vf_rows_at(values,map);
    VfRows rows=unsafe_position<VfRows>(state);
    u64 index=vf_lower(rows,slot);
    if(index==rows.count || unsafe_position<LocalValue>(vf_row(rows,index)).slot!=slot){
        vf_reserve(rows,rows.count+1u64,resize);
        u64 move=rows.count;
        while(move>index){
            unsafe_position<LocalValue>(vf_row(rows,move))=unsafe_position<LocalValue>(vf_row(rows,move-1u64));
            move-=1u64;
        }
        rows.count+=1u64;
        unsafe_position<VfRows>(state)=rows;
    }
    unsafe_position<LocalValue>(vf_row(rows,index))=LocalValue(slot,value);
}

void vf_remove_range(address values,u64 start,u64 end) on cpu {
    VfRows rows=unsafe_position<VfRegisters>(values).locals;
    u64 write=vf_lower(rows,start);
    u64 read=vf_lower(rows,end);
    if(write==read){return;}
    while(read<rows.count){
        unsafe_position<LocalValue>(vf_row(rows,write))=unsafe_position<LocalValue>(vf_row(rows,read));
        write+=1u64;
        read+=1u64;
    }
    unsafe_position<VfRegisters>(values).locals.count=write;
}

extern "C" void vf_remove_local(address values,usize slot) on cpu {
    vf_remove_range(values,slot,slot+1u64);
}

extern "C" u64 value_root(address values,u64 reg) on cpu {
    u64 result=reg;
    VfOptional next=vf_get(values,VfRegMap::Aliases,result);
    while(next.present!=0){result=next.value;next=vf_get(values,VfRegMap::Aliases,result);}
    return result;
}

VfPath vf_no_path() on cpu { return VfPath(vf_empty_rows(),0); }

VfPath vf_snapshot(address values,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfRows original=unsafe_position<VfRegisters>(values).locals;
    VfRows rows=vf_empty_rows();
    vf_reserve(rows,original.count,resize);
    u64 index=0u64;
    while(index<original.count){
        LocalValue row=unsafe_position<LocalValue>(vf_row(original,index));
        unsafe_position<LocalValue>(vf_row(rows,index))=LocalValue(row.slot,value_root(values,row.value));
        index+=1u64;
    }
    rows.count=original.count;
    return VfPath(rows,1);
}


struct VfMemoryEntry { u64 binding; u64 index; u32 word; u32 has_origin; u64 value; };

address vf_memory_rows(address values) on cpu {
    return unsafe_address(address_bits(values)+size_of<VfRegisters>());
}

address vf_memory_row(VfRows rows,u64 index,u64 stride) on cpu {
    return unsafe_address(rows.data+index*stride);
}

u64 vf_memory_lower(VfRows rows,u64 binding,u64 index,u32 word,u64 stride) on cpu {
    u64 start=0u64;
    u64 end=rows.count;
    while(start<end){
        u64 middle=start+(end-start)/2u64;
        VfMemoryEntry row=unsafe_position<VfMemoryEntry>(vf_memory_row(rows,middle,stride));
        bool before=row.binding<binding || (row.binding==binding &&
            (row.index<index || (row.index==index && row.word<word)));
        if(before){start=middle+1u64;}
        else{end=middle;}
    }
    return start;
}

void vf_clear_memory(address state,VfMemoryHost memory) on cpu
    requires nonzero(memory.drop_origin) {
    VfRows rows=unsafe_position<VfRows>(state);
    u64 index=0u64;
    while(index<rows.count){
        address row=vf_memory_row(rows,index,memory.stride);
        if(unsafe_position<VfMemoryEntry>(row).has_origin!=0){memory.drop_origin(row);}
        index+=1u64;
    }
    unsafe_position<VfRows>(state).count=0u64;
}

extern "C" void value_release_memory(address state,VfMemoryHost memory) on cpu
    requires nonzero(memory.resize) requires nonzero(memory.drop_origin) {
    vf_clear_memory(state,memory);
    vf_release(unsafe_position<VfRows>(state),memory.resize);
}

extern "C" void value_copy_memory(address target,address source,VfMemoryHost memory) on cpu
    requires nonzero(memory.resize) requires nonzero(memory.copy_origin) {
    VfRows original=unsafe_position<VfRows>(source);
    VfRows rows=vf_empty_rows();
    vf_reserve(rows,original.count,memory.resize);
    u64 index=0u64;
    while(index<original.count){
        address src=vf_memory_row(original,index,memory.stride);
        address dst=vf_memory_row(rows,index,memory.stride);
        VfMemoryEntry entry=unsafe_position<VfMemoryEntry>(src);
        unsafe_position<VfMemoryEntry>(dst)=entry;
        if(entry.has_origin!=0){memory.copy_origin(dst,src);}
        index+=1u64;
    }
    rows.count=original.count;
    unsafe_position<VfRows>(target)=rows;
}

VfMemory vf_memory_get(address values,u64 binding,u64 index,u32 word,VfMemoryHost memory) on cpu {
    VfRows rows=unsafe_position<VfRows>(vf_memory_rows(values));
    u64 at=vf_memory_lower(rows,binding,index,word,memory.stride);
    if(at<rows.count){
        address element=vf_memory_row(rows,at,memory.stride);
        VfMemoryEntry row=unsafe_position<VfMemoryEntry>(element);
        if(row.binding==binding && row.index==index && row.word==word){
            u64 origin=row.has_origin!=0 ? address_bits(element)+memory.origin_offset : 0u64;
            return VfMemory(row.value,origin,1);
        }
    }
    return VfMemory(0u64,0u64,0);
}

void vf_memory_insert(address values,u64 binding,u64 index,u32 word,u64 value,address instruction,VfMemoryHost memory) on cpu
    requires nonzero(memory.resize) requires nonzero(memory.move_rows) requires nonzero(memory.capture_origin) {
    address state=vf_memory_rows(values);
    VfRows rows=unsafe_position<VfRows>(state);
    u64 at=vf_memory_lower(rows,binding,index,word,memory.stride);
    vf_reserve(rows,rows.count+1u64,memory.resize);
    if(at<rows.count){
        memory.move_rows(vf_memory_row(rows,at+1u64,memory.stride),vf_memory_row(rows,at,memory.stride),rows.count-at);
    }
    address dst=vf_memory_row(rows,at,memory.stride);
    bool written=address_bits(instruction)!=0u64;
    unsafe_position<VfMemoryEntry>(dst)=VfMemoryEntry(binding,index,word,u32(written),value);
    if(written){memory.capture_origin(dst,instruction);}
    rows.count+=1u64;
    unsafe_position<VfRows>(state)=rows;
}

void vf_memory_forget(address values,u64 binding,bool store,u64 index,u32 word,VfMemoryHost memory) on cpu
    requires nonzero(memory.move_rows) requires nonzero(memory.drop_origin) {
    address state=vf_memory_rows(values);
    VfRows rows=unsafe_position<VfRows>(state);
    u64 read=vf_memory_lower(rows,binding,0u64,0,memory.stride);
    u64 write=read;
    // The contiguous binding range is the only range a write can invalidate.
    while(read<rows.count){
        address old=vf_memory_row(rows,read,memory.stride);
        VfMemoryEntry entry=unsafe_position<VfMemoryEntry>(old);
        if(entry.binding!=binding){break;}
        if(store && entry.index==index && entry.word!=word){
            if(write!=read){memory.move_rows(vf_memory_row(rows,write,memory.stride),old,1u64);}
            write+=1u64;
        }else if(entry.has_origin!=0){memory.drop_origin(old);}
        read+=1u64;
    }
    if(write!=read){
        if(read<rows.count){
            memory.move_rows(vf_memory_row(rows,write,memory.stride),vf_memory_row(rows,read,memory.stride),rows.count-read);
        }
        unsafe_position<VfRows>(state).count=rows.count-(read-write);
    }
}

void vf_forget_alias(View alias,address values,bool memory,VfHost host) on cpu
    requires nonzero(host.memory.move_rows) requires nonzero(host.memory.drop_origin) {
    if(alias.tag==ALIAS_LOCAL){vf_remove_range(values,alias.a,alias.a+alias.b);}
    else if(memory){vf_memory_forget(values,alias.a,false,0u64,0,host.memory);}
}
void vf_forget_indexed(View indexed,address values,bool memory,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect)
    requires nonzero(host.memory.move_rows) requires nonzero(host.memory.drop_origin) {
    if(indexed.tag==INDEXED_DATA && memory){
        vf_memory_forget(values,indexed.a,false,0u64,0,host.memory);
    }else if(indexed.tag==INDEXED_POINTER){
        usize index=0u64;
        while(index<indexed.second.count){
            View alias=host.values.ir.inspect(Node(span_item_at(indexed.second,index),N_MEMORY_ALIAS));
            vf_forget_alias(alias,values,memory,host);
            index+=1u64;
        }
    }
}
void vf_forget_call(Node source,address values,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect)
    requires nonzero(host.memory.move_rows) requires nonzero(host.memory.drop_origin) {
    Host ir=host.values.ir;
    View call=ir.inspect(source);
    usize index=0u64;
    while(index<call.first.count){
        View argument=ir.inspect(Node(span_item_at(call.first,index),N_ARGUMENT));
        if(argument.tag==ARGUMENT_REFERENCE && accessed(argument.a,1)){
            View reference=ir.inspect(argument.left);
            View storage=ir.inspect(reference.left);
            if(storage.tag==STORAGE_INDEXED){
                View indexed=ir.inspect(storage.left);
                if(indexed.tag==INDEXED_POINTER){vf_forget_indexed(indexed,values,true,host);}
                else{vf_clear_memory(vf_memory_rows(values),host.memory);}
            }else{
                vf_remove_range(values,storage.a,storage.a+storage.b);
            }
        }
        index+=1u64;
    }
    index=0u64;
    while(index<call.second.count){
        View effect=ir.inspect(Node(span_item_at(call.second,index),N_MEMORY_EFFECT));
        if(accessed(effect.a,1)){vf_forget_alias(ir.inspect(effect.left),values,true,host);}
        index+=1u64;
    }
}
void vf_merge(inout VfPath left,VfPath right,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    if(right.reached!=0){
        if(left.reached==0){left=right;}
        else{
            left.rows.count=intersect_local_values(unsafe_address(left.rows.data),left.rows.count,unsafe_address(right.rows.data),right.rows.count);
            vf_release(right.rows,resize);
        }
    }
}

bool vf_resume(address values,inout VfFlow flow,VfFlow step,VfHost host) on cpu
    requires nonzero(host.resize_rows) requires nonzero(host.memory.drop_origin) {
    vf_merge(flow.exits,step.exits,host.resize_rows);
    address local=vf_rows_at(values,VfRegMap::Locals);
    bool continuing=step.next.reached!=0;
    vf_release(unsafe_position<VfRows>(local),host.resize_rows);
    unsafe_position<VfRows>(local)=step.next.rows;
    vf_clear_memory(vf_memory_rows(values),host.memory);
    return continuing;
}

address vf_child(address values,VfHost host) on cpu
    requires nonzero(host.resize_rows) requires nonzero(host.state) {
    address none=unsafe_address(0u64);
    address child=host.state(VfStateOp::New,none,none);
    vf_move_rows(child,values,VfRegMap::Aliases,host.resize_rows);
    host.state(VfStateOp::CopyExpressions,child,values);
    vf_move_rows(child,values,VfRegMap::Locals,host.resize_rows);
    address memory=vf_memory_rows(values);
    unsafe_position<VfRows>(vf_memory_rows(child))=unsafe_position<VfRows>(memory);
    unsafe_position<VfRows>(memory)=vf_empty_rows();
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
    requires nonzero(host.resize_rows) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.note) requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin)
    {
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
                vf_put(values,VfRegMap::Aliases,quotient_result,old.value,host.resize_rows);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Quotient,paired));
            } else {retained|=1;}
        }
        if((mask&2)!=0){
            VfExpression key=VfExpression(10,1,node.a,divisor_kind,divisor.a,none,0u64);
            VfOptional old=host.expression(values,key,remainder_result);
            if(old.present!=0){
                vf_put(values,VfRegMap::Aliases,remainder_result,old.value,host.resize_rows);
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
            vf_put(values,VfRegMap::Aliases,node.a,old.value,host.resize_rows);
            host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Value,0));
            return VfAction::Discard;
        }
    }
    bool continuing=true;
    if(kind==OP_SCOPE){
        address scoped=vf_child(values,host);
        VfFlow scope=host.block(host.storage(instruction).first_code,scoped,context);
        vf_move_rows(values,scoped,VfRegMap::Aliases,host.resize_rows);
        vf_merge(scope.next,scope.exits,host.resize_rows);
        scope.exits=vf_no_path();
        continuing=vf_resume(values,flow,scope,host);
        host.state(VfStateOp::Drop,scoped,none);
    } else if(kind==OP_RAISE){continuing=false;}
    else if(kind==OP_EXIT_VALUE){vf_merge(flow.exits,vf_snapshot(values,host.resize_rows),host.resize_rows);continuing=false;}
    else if(kind==OP_BREAK_VALUE || kind==OP_CONTINUE_VALUE){continuing=false;}
    else if(kind==OP_ATOMIC){
        if(host.values.ir.inspect(node.right).tag!=0){
            vf_forget_indexed(host.values.ir.inspect(node.left),values,false,host);
        }
        vf_clear_memory(vf_memory_rows(values),host.memory);
    } else if(kind==OP_APPEND || kind==OP_TILE_BARRIER){vf_clear_memory(vf_memory_rows(values),host.memory);}
    else if(kind==OP_LOAD_LOCAL){
        VfOptional old=vf_get(values,VfRegMap::Locals,node.b);
        if(old.present!=0){
            u64 value=value_root(values,old.value);
            vf_put(values,VfRegMap::Aliases,node.a,value,host.resize_rows);
            host.note(context,value,instruction,VfReuse(none,VfReuseKind::LocalRead,VfRole::Value,0));
        }
        vf_put(values,VfRegMap::Locals,node.b,node.a,host.resize_rows);
    } else if(kind==OP_STORE_LOCAL){vf_put(values,VfRegMap::Locals,node.a,node.b,host.resize_rows);}
    else if(kind==OP_STORE_LOCAL_INDEXED){vf_remove_range(values,node.a,node.a+node.b);}
    else if(kind==OP_STORE_INDEXED){vf_forget_indexed(host.values.ir.inspect(node.left),values,true,host);}
    else if(kind==OP_LOAD){
        VfMemory old=vf_memory_get(values,node.b,node.c,u32(node.d),host.memory);
        if(old.present!=0){
            vf_put(values,VfRegMap::Aliases,node.a,old.value,host.resize_rows);
            VfReuseKind reuse;
            if(old.origin!=0u64){reuse=VfReuseKind::IndexedStoreRead;}
            else {reuse=VfReuseKind::IndexedRead;}
            host.note(context,old.value,instruction,VfReuse(unsafe_address(old.origin),reuse,VfRole::Value,0));
            return VfAction::Discard;
        }
        vf_memory_insert(values,node.b,node.c,u32(node.d),node.a,none,host.memory);
    } else if(kind==OP_STORE){
        vf_memory_forget(values,node.a,true,node.b,u32(node.c),host.memory);
        vf_memory_insert(values,node.a,node.b,u32(node.c),node.d,instruction,host.memory);
    } else if(kind==OP_BRANCH){
        address yes=host.state(VfStateOp::Clone,none,values);
        address no=vf_child(values,host);
        InstructionStorage storage=host.storage(instruction);
        VfFlow branches=host.block(storage.first_code,yes,context);
        VfFlow other=host.block(storage.second_code,no,context);
        vf_merge(branches.next,other.next,host.resize_rows);
        vf_merge(branches.exits,other.exits,host.resize_rows);
        vf_move_rows(values,yes,VfRegMap::Aliases,host.resize_rows);
        value_merge_aliases(values,no,host.resize_rows);
        continuing=vf_resume(values,flow,branches,host);
        host.state(VfStateOp::Drop,yes,none);
        host.state(VfStateOp::Drop,no,none);
    } else if(kind==OP_LOOP_VALUE){
        visit_local_writes(node.first,host.values.ir,values,c_function(vf_remove_local));
        visit_local_writes(node.second,host.values.ir,values,c_function(vf_remove_local));
        InstructionStorage storage=host.storage(instruction);
        VfPath after=vf_snapshot(values,host.resize_rows);
        vf_clear_memory(vf_memory_rows(values),host.memory);
        address cycle=vf_child(values,host);
        VfFlow condition=host.block(storage.first_code,cycle,context);
        unsafe_position<u64>(storage.payload)=value_root(cycle,node.a);
        VfFlow loop_flow=VfFlow(vf_no_path(),condition.exits);
        if(condition.next.reached!=0){
            vf_release(condition.next.rows,host.resize_rows);
            loop_flow.next=after;
            VfFlow body=host.block(storage.second_code,cycle,context);
            vf_release(body.next.rows,host.resize_rows);
            vf_merge(loop_flow.exits,body.exits,host.resize_rows);
        } else {vf_release(after.rows,host.resize_rows);}
        vf_move_rows(values,cycle,VfRegMap::Aliases,host.resize_rows);
        continuing=vf_resume(values,flow,loop_flow,host);
        host.state(VfStateOp::Drop,cycle,none);
    } else if(kind==OP_INVOKE){vf_forget_call(node.left,values,host);}
    return continuing ? VfAction::Keep : VfAction::Stop;
}
extern "C" VfFlow value_block(address block,address values,address context,Span registers,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.resize_rows) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.note) requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate) {
    Span code=host.instruction_span(block);
    VfFlow flow=VfFlow(vf_no_path(),vf_no_path());
    u64 kept=0u64;
    u64 index=0u64;
    bool continuing=true;
    // The retained prefix never moves again, including slots awaiting a division pair.
    while(index<code.count){
        address instruction=span_item_at(code,index);
        address retained_at=span_item_at(code,kept);
        VfAction action=value_instruction(instruction,retained_at,values,flow,context,registers,host);
        if(action!=VfAction::Discard){
            if(kept!=index){host.swap_instructions(instruction,retained_at);}
            kept+=1u64;
            if(action==VfAction::Stop){continuing=false;break;}
        }
        index+=1u64;
    }
    if(kept!=code.count){host.truncate(block,kept);}
    if(continuing){flow.next=vf_snapshot(values,host.resize_rows);}
    return flow;
}
extern "C" void value_discard_flow(VfFlow flow,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    vf_release(flow.next.rows,resize);
    vf_release(flow.exits.rows,resize);
}
