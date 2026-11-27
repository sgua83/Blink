module bootstrap.stage_fusion;
import bootstrap.ir_dataflow;
import bootstrap.names;

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

struct TilePlacement { i64 begin; i64 end; u64 first; u64 count; u64 width; u32 bytes; };
struct TileBuffer { address data; u64 count; u64 capacity; };
struct TilePlan { TileBuffer rows; TileBuffer members; };

bool tile_key_less(TileKey a, TileKey b) on cpu {
    if (a.storage != b.storage) { return a.storage < b.storage; }
    if (a.owner != b.owner) { return a.owner < b.owner; }
    return a.byte < b.byte;
}

address tile_piece<T>(address data, u64 index) on cpu {
    return unsafe_address(address_bits(data) + index * size_of<T>());
}

void tile_push<T>(inout TileBuffer buffer, TileScratch bank, T value, TileLayoutHost host) on cpu
    requires nonzero(host.reserve) {
    if (buffer.count == buffer.capacity) {
        u64 capacity = max(8u64, buffer.capacity * 2u64);
        buffer.data = host.reserve(host.context, bank, capacity * (size_of<T>() / 8u64));
        buffer.capacity = capacity;
    }
    unsafe_position<T>(tile_piece<T>(buffer.data, buffer.count)) = value;
    buffer.count += 1u64;
}

void tile_describe_cells(Span tiles, TileLayoutHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.describe) {
    u64 index = 0u64;
    while (index < tiles.count) {
        View tile = host.ir.inspect(Node(span_item_at(tiles, index), N_TILE));
        TileKey origin = tile_key_for_view(tile, index, 0);
        u64 component = 0u64;
        while (component < tile.first.count) {
            View field = host.ir.inspect(Node(span_item_at(tile.first, component), N_COMPONENT));
            u32 low = (origin.byte + u32(field.a)) & 0xfffffffc;
            u32 bytes = field.b == 8u64 ? 8 : 4;
            host.describe(host.context, TileKey(origin.storage, origin.owner, low), TileCell(low, bytes));
            if (bytes == 8) {
                host.describe(host.context, TileKey(origin.storage, origin.owner, low + 4), TileCell(low, bytes));
            }
            component += 1u64;
        }
        index += 1u64;
    }
}

u64 tile_join_uses(address uses, u64 count, TileLayoutHost host) on cpu
    requires nonzero(host.cell) {
    // Normalizing to a low word preserves the order of aligned input keys.
    u64 kept = 0u64;
    u64 index = 0u64;
    while (index < count) {
        TileUse item = unsafe_position<TileUse>(tile_piece<TileUse>(uses, index));
        item.key.byte = host.cell(host.context, item.key).byte;
        if (kept != 0u64) {
            address previous = tile_piece<TileUse>(uses, kept - 1u64);
            if (!tile_key_less(unsafe_position<TileUse>(previous).key, item.key)) {
                unsafe_position<TileUse>(previous).end =
                    max(unsafe_position<TileUse>(previous).end, item.end);
                index += 1u64;
                continue;
            }
        }
        unsafe_position<TileUse>(tile_piece<TileUse>(uses, kept)) = item;
        kept += 1u64;
        index += 1u64;
    }
    return kept;
}

i64 tile_take_use(address uses, u64 count, TileKey key) on cpu {
    u64 low = 0u64;
    u64 high = count;
    while (low < high) {
        u64 middle = low + (high - low) / 2u64;
        TileUse item = unsafe_position<TileUse>(tile_piece<TileUse>(uses, middle));
        if (tile_key_less(item.key, key)) { low = middle + 1u64; }
        else { high = middle; }
    }
    if (low == count) { return -i64(1); }
    address at = tile_piece<TileUse>(uses, low);
    TileUse item = unsafe_position<TileUse>(at);
    if (tile_key_less(key, item.key)) { return -i64(1); }
    unsafe_position<TileUse>(at).end = -i64(1);
    return item.end;
}

i64 tile_begin(TileKey key, TileLayoutHost host) on cpu requires nonzero(host.birth) {
    return key.storage == 0u64 ? -i64(1) : host.birth(host.context, key.owner);
}

void tile_make_row(inout TilePlan plan, u64 first, i64 end, bool wide, TileLayoutHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.cell) requires nonzero(host.birth) {
    TileKey low = unsafe_position<TileKey>(tile_piece<TileKey>(plan.members.data, first));
    TileKey high = unsafe_position<TileKey>(tile_piece<TileKey>(plan.members.data, plan.members.count - 1u64));
    u32 bytes = wide ? host.cell(host.context, low).bytes : 4;
    u32 shift = bytes == 4 ? 2 : 3;
    u64 width = u64((high.byte - low.byte) >> shift) + 1u64;
    TilePlacement row = TilePlacement(tile_begin(low, host), end, first, plan.members.count - first, width, bytes);
    tile_push<TilePlacement>(plan.rows, TileScratch::Rows, row, host);
}

void tile_collect_group(inout TilePlan plan, TileGroup group, u32 bytes, address uses, u64 count,
    bool wide, TileLayoutHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.cell) requires nonzero(host.birth) {
    u64 first = plan.members.count;
    i64 end = -i64(1);
    u32 word = group.first;
    while (word < group.end) {
        TileKey key = TileKey(group.storage, group.owner, word * 4);
        TileCell cell = wide ? host.cell(host.context, key) : TileCell(key.byte, 4);
        if (cell.bytes == bytes) {
            key.byte = cell.byte;
            // A high word can only repeat the immediately preceding low word.
            if (plan.members.count == first || unsafe_position<TileKey>(
                    tile_piece<TileKey>(plan.members.data, plan.members.count - 1u64)).byte != key.byte) {
                tile_push<TileKey>(plan.members, TileScratch::Members, key, host);
                end = max(end, tile_take_use(uses, count, key));
            }
        }
        word += 1;
    }
    if (plan.members.count != first) { tile_make_row(plan, first, end, wide, host); }
}

extern "C" bool tile_row_before(address left, address right, address context) on cpu {
    TilePlan plan = unsafe_position<TilePlan>(context);
    TilePlacement a = unsafe_position<TilePlacement>(
        tile_piece<TilePlacement>(plan.rows.data, unsafe_position<u64>(left)));
    TilePlacement b = unsafe_position<TilePlacement>(
        tile_piece<TilePlacement>(plan.rows.data, unsafe_position<u64>(right)));
    address members = plan.members.data;
    if (a.begin != b.begin) { return a.begin < b.begin; }
    u64 index = 0u64;
    while (index < min(a.count, b.count)) {
        TileKey x = unsafe_position<TileKey>(tile_piece<TileKey>(members, a.first + index));
        TileKey y = unsafe_position<TileKey>(tile_piece<TileKey>(members, b.first + index));
        if (tile_key_less(x, y)) { return true; }
        if (tile_key_less(y, x)) { return false; }
        index += 1u64;
    }
    if (a.count != b.count) { return a.count < b.count; }
    return a.end < b.end;
}

void tile_publish_word(TileKey key, u32 bytes, u64 slot, bool wide, TileLayoutHost host) on cpu
    requires nonzero(host.cell) requires nonzero(host.publish) {
    if (!wide) {
        host.publish(host.context, key, bytes, slot, 0);
    } else {
        TileCell low = host.cell(host.context, key);
        if (low.bytes != 0 && low.byte == key.byte) {
            host.publish(host.context, key, bytes, slot, 0);
        }
        TileKey next = TileKey(key.storage, key.owner, key.byte + 4);
        TileCell cell = host.cell(host.context, next);
        if (cell.bytes != 0 && cell.byte == key.byte) {
            host.publish(host.context, next, bytes, slot, 4);
        }
    }
}

TileExtents tile_place_rows(TilePlan plan, bool wide, bool publish, TileLayoutHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.cell) requires nonzero(host.publish) {
    TileExtents capacity = TileExtents(0u64, 0u64);
    u64 index = 0u64;
    while (index < plan.rows.count) {
        TilePlacement row = unsafe_position<TilePlacement>(tile_piece<TilePlacement>(plan.rows.data, index));
        if (row.bytes == 4) { capacity.narrow += row.width; }
        else { capacity.wide += row.width; }
        index += 1u64;
    }
    // The same storage holds the row permutation and the narrow plane's end positions.
    address order = host.reserve(host.context, TileScratch::Narrow, plan.rows.count + capacity.narrow);
    address narrow = tile_piece<i64>(order, plan.rows.count);
    address large = host.reserve(host.context, TileScratch::Wide, capacity.wide);
    index = 0u64;
    while (index < plan.rows.count) {
        unsafe_position<u64>(tile_piece<u64>(order, index)) = index;
        index += 1u64;
    }
    sort_rows<u64>(order, plan.rows.count, address_of(plan), c_function(tile_row_before));
    TileExtents placed = TileExtents(0u64, 0u64);
    index = 0u64;
    while (index < plan.rows.count) {
        u64 ordinal = unsafe_position<u64>(tile_piece<u64>(order, index));
        TilePlacement row = unsafe_position<TilePlacement>(tile_piece<TilePlacement>(plan.rows.data, ordinal));
        address plane = row.bytes == 4 ? narrow : large;
        u64 used = row.bytes == 4 ? placed.narrow : placed.wide;
        u64 slot = 0u64;
        u64 next = 0u64;
        while (next < used && next < slot + row.width) {
            if (unsafe_position<i64>(tile_piece<i64>(plane, next)) >= row.begin) { slot = next + 1u64; }
            next += 1u64;
        }
        next = slot;
        while (next < slot + row.width) {
            unsafe_position<i64>(tile_piece<i64>(plane, next)) = row.end;
            next += 1u64;
        }
        if (row.bytes == 4) { placed.narrow = max(used, next); }
        else { placed.wide = max(used, next); }
        if (publish) {
            TileKey first = unsafe_position<TileKey>(tile_piece<TileKey>(plan.members.data, row.first));
            u32 shift = row.bytes == 4 ? 2 : 3;
            u64 member = 0u64;
            while (member < row.count) {
                TileKey key = unsafe_position<TileKey>(tile_piece<TileKey>(plan.members.data, row.first + member));
                tile_publish_word(key, row.bytes, slot + u64((key.byte - first.byte) >> shift), wide, host);
                member += 1u64;
            }
        }
        index += 1u64;
    }
    return placed;
}

extern "C" TileExtents place_tile(TileLayoutInput input, TileLayoutHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.reserve) requires nonzero(host.describe)
    requires nonzero(host.cell) requires nonzero(host.birth) requires nonzero(host.publish) {
    bool wide = input.wide != 0;
    u64 count = input.use_count;
    if (wide) {
        tile_describe_cells(input.tiles, host);
        count = tile_join_uses(input.uses, count, host);
    }
    TileBuffer empty = TileBuffer(address(), 0u64, 0u64);
    TilePlan plan = TilePlan(empty, empty);
    u64 index = 0u64;
    while (index < input.groups.count) {
        TileGroup group = unsafe_position<TileGroup>(span_item_at(input.groups, index));
        tile_collect_group(plan, group, 4, input.uses, count, wide, host);
        if (wide) { tile_collect_group(plan, group, 8, input.uses, count, true, host); }
        index += 1u64;
    }
    index = 0u64;
    while (index < count) {
        TileUse item = unsafe_position<TileUse>(tile_piece<TileUse>(input.uses, index));
        if (item.end >= i64(0)) {
            u64 first = plan.members.count;
            tile_push<TileKey>(plan.members, TileScratch::Members, item.key, host);
            tile_make_row(plan, first, item.end, wide, host);
        }
        index += 1u64;
    }
    return tile_place_rows(plan, wide, input.publish != 0, host);
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

extern "C" address private_loop(Span condition,u64 test,Span body,
    address after,address break_live,address continue_live,address exit_live,
    address empty,LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    // Pointwise gen/kill transfers close the backedge from an empty header.
    address need=host.clone_set(empty);
    private_before(body,need,after,empty,exit_live,empty,host);
    host.change(LiveSetAction::Union,need,after,0u64);
    host.insert_register(need,test);
    private_before(condition,need,break_live,continue_live,exit_live,empty,host);
    return need;
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
        address header=private_loop(value.first,value.a,value.second,live,break_live,continue_live,exit_live,empty,host);
        host.change(LiveSetAction::Swap,live,header,0u64);
        host.drop_set(header);
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

void private_before(Span code,address live,address break_live,
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

extern "C" void private_range_live(Span code, address live, address empty, LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    address header = host.clone_set(live);
    private_before(code,header,live,live,empty,empty,host);
    host.change(LiveSetAction::DrainUnion,live,header,0u64);
    host.drop_set(header);
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
        address header=private_loop(value.first,first,value.second,needed,break_live,continue_live,exit_live,empty,host.live);
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
    if(map==VfRegMap::Aliases){return address_of(unsafe_position<VfRegisters>(values).aliases);}
    return address_of(unsafe_position<VfRegisters>(values).locals);
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

extern "C" void value_release_registers(address values,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfRegisters registers=unsafe_position<VfRegisters>(values);
    vf_release(registers.aliases,resize);
    vf_release(registers.locals,resize);
}

void vf_merge_aliases(address target,address source,cfn<address,address,u64,u64> resize) on cpu
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

extern "C" void value_rebind_roots(address registers,RewriteSpan roots) on cpu {
    if(unsafe_position<VfRegisters>(registers).aliases.count==0u64){return;}
    u64 index=0u64;
    while(index<roots.count){
        address field=unsafe_address(address_bits(roots.data)+index*roots.stride);
        unsafe_position<usize>(field)=value_root(registers,unsafe_position<usize>(field));
        index+=1u64;
    }
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

struct VfValues { VfRegisters registers; VfRows memory; };

address vf_registers(address values) on cpu {
    return address_of(unsafe_position<VfValues>(values).registers);
}

address vf_memory_rows(address values) on cpu {
    return address_of(unsafe_position<VfValues>(values).memory);
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

void vf_copy_memory(address target,address source,VfMemoryHost memory) on cpu
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
    if(alias.tag==ALIAS_LOCAL){vf_remove_range(vf_registers(values),alias.a,alias.a+alias.b);}
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
                vf_remove_range(vf_registers(values),storage.a,storage.a+storage.b);
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
    address local=vf_rows_at(vf_registers(values),VfRegMap::Locals);
    bool continuing=step.next.reached!=0;
    vf_release(unsafe_position<VfRows>(local),host.resize_rows);
    unsafe_position<VfRows>(local)=step.next.rows;
    vf_clear_memory(vf_memory_rows(values),host.memory);
    return continuing;
}

VfValues vf_empty_values() on cpu {
    return VfValues(VfRegisters(vf_empty_rows(),vf_empty_rows()),vf_empty_rows());
}

VfValues vf_fork_values(address values,VfHost host) on cpu
    requires nonzero(host.resize_rows) requires nonzero(host.memory.resize) requires nonzero(host.memory.copy_origin) {
    VfRegisters original=unsafe_position<VfRegisters>(vf_registers(values));
    VfRows aliases=vf_copy_rows(original.aliases,host.resize_rows);
    VfRows locals=vf_copy_rows(original.locals,host.resize_rows);
    VfValues child=VfValues(VfRegisters(aliases,locals),vf_empty_rows());
    vf_copy_memory(vf_memory_rows(address_of(child)),vf_memory_rows(values),host.memory);
    return child;
}

VfValues vf_take_values(address values) on cpu {
    VfValues child=unsafe_position<VfValues>(values);
    unsafe_position<VfValues>(values)=vf_empty_values();
    return child;
}

void vf_release_values(address values,VfHost host) on cpu
    requires nonzero(host.resize_rows) requires nonzero(host.memory.resize) requires nonzero(host.memory.drop_origin) {
    value_release_registers(vf_registers(values),host.resize_rows);
    value_release_memory(vf_memory_rows(values),host.memory);
}

struct VfExpressionEntry {
    VfExpression key;
    u64 arguments;
    u64 hash;
    u64 next;
    u64 result;
};
struct VfExpressionMark { u64 entries; u64 arguments; };

address vf_word(u64 base,u64 index) on cpu {
    return unsafe_address(base+index*size_of<u64>());
}

address vf_expression_entry(VfRows entries,u64 index) on cpu {
    return unsafe_address(entries.data+index*size_of<VfExpressionEntry>());
}

u64 vf_hash_word(u64 hash,u64 word) on cpu {
    u64 mixed=(hash ^ word)*1099511628211u64;
    return mixed ^ (mixed >> 32u64);
}

u64 vf_expression_hash(VfExpression key,u64 arguments) on cpu {
    u64 hash=vf_hash_word(14695981039346656037u64,(u64(key.kind)<<32u64)|u64(key.operation));
    hash=vf_hash_word(hash,key.first);
    hash=vf_hash_word(hash,key.second);
    hash=vf_hash_word(hash,key.third);
    hash=vf_hash_word(hash,key.count);
    u64 index=0u64;
    while(index<key.count){
        hash=vf_hash_word(hash,unsafe_position<u64>(vf_word(arguments,index)));
        index+=1u64;
    }
    return hash;
}

bool vf_same_expression(VfExpression left,VfExpression right,u64 stored,u64 input) on cpu {
    if(left.kind!=right.kind || left.operation!=right.operation || left.first!=right.first ||
        left.second!=right.second || left.third!=right.third || left.count!=right.count){return false;}
    u64 index=0u64;
    while(index<left.count){
        if(unsafe_position<u64>(vf_word(stored,index))!=unsafe_position<u64>(vf_word(input,index))){return false;}
        index+=1u64;
    }
    return true;
}

address vf_expression_bucket(VfRows buckets,u64 hash) on cpu {
    return vf_word(buckets.data,hash & (buckets.count-1u64));
}

void vf_grow_expressions(inout VfExpressions table,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    u64 count=table.buckets.count==0u64 ? 8u64 : table.buckets.count*2u64;
    vf_reserve(table.buckets,count,resize);
    table.buckets.count=count;
    u64 index=0u64;
    while(index<count){unsafe_position<u64>(vf_word(table.buckets.data,index))=0u64;index+=1u64;}
    index=0u64;
    // Insertion order keeps each bucket newest-first for lexical rollback.
    while(index<table.entries.count){
        address entry=vf_expression_entry(table.entries,index);
        address bucket=vf_expression_bucket(table.buckets,unsafe_position<VfExpressionEntry>(entry).hash);
        unsafe_position<VfExpressionEntry>(entry).next=unsafe_position<u64>(bucket);
        unsafe_position<u64>(bucket)=index+1u64;
        index+=1u64;
    }
}

VfOptional vf_expression(address state,VfExpression key,u64 arguments,u64 result,
    cfn<address,address,u64,u64> resize) on cpu requires nonzero(resize) {
    VfExpressions table=unsafe_position<VfExpressions>(state);
    u64 hash=vf_expression_hash(key,arguments);
    if(table.buckets.count!=0u64){
        u64 next=unsafe_position<u64>(vf_expression_bucket(table.buckets,hash));
        while(next!=0u64){
            VfExpressionEntry entry=unsafe_position<VfExpressionEntry>(vf_expression_entry(table.entries,next-1u64));
            if(entry.hash==hash && vf_same_expression(entry.key,key,address_bits(vf_word(table.arguments.data,entry.arguments)),arguments)){
                return VfOptional(entry.result,1);
            }
            next=entry.next;
        }
    }
    if(table.entries.count==table.buckets.count){vf_grow_expressions(table,resize);}
    u64 start=table.arguments.count;
    vf_reserve(table.arguments,start+key.count,resize);
    u64 index=0u64;
    while(index<key.count){
        unsafe_position<u64>(vf_word(table.arguments.data,start+index))=unsafe_position<u64>(vf_word(arguments,index));
        index+=1u64;
    }
    table.arguments.count=start+key.count;
    vf_reserve(table.entries,(table.entries.count+1u64)*(size_of<VfExpressionEntry>()/size_of<u64>()),resize);
    address bucket=vf_expression_bucket(table.buckets,hash);
    unsafe_position<VfExpressionEntry>(vf_expression_entry(table.entries,table.entries.count))=
        VfExpressionEntry(key,start,hash,unsafe_position<u64>(bucket),result);
    table.entries.count+=1u64;
    unsafe_position<u64>(bucket)=table.entries.count;
    unsafe_position<VfExpressions>(state)=table;
    return VfOptional(0u64,0);
}

VfExpressionMark vf_mark_expressions(address state) on cpu {
    VfExpressions table=unsafe_position<VfExpressions>(state);
    return VfExpressionMark(table.entries.count,table.arguments.count);
}

void vf_rewind_expressions(address state,VfExpressionMark mark) on cpu {
    VfExpressions table=unsafe_position<VfExpressions>(state);
    while(table.entries.count>mark.entries){
        table.entries.count-=1u64;
        VfExpressionEntry entry=unsafe_position<VfExpressionEntry>(vf_expression_entry(table.entries,table.entries.count));
        unsafe_position<u64>(vf_expression_bucket(table.buckets,entry.hash))=entry.next;
    }
    table.arguments.count=mark.arguments;
    unsafe_position<VfExpressions>(state)=table;
}

extern "C" void value_release_expressions(address state,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    VfExpressions table=unsafe_position<VfExpressions>(state);
    vf_release(table.entries,resize);
    vf_release(table.arguments,resize);
    vf_release(table.buckets,resize);
}

struct VfDivisionEntry { u64 source; u64 kind; u64 divisor; VfDivision value; };

address vf_division_entry(VfRows table,u64 index) on cpu {
    return unsafe_address(table.data+index*size_of<VfDivisionEntry>());
}

u64 vf_division_lower(VfRows table,u64 source,u64 kind,u64 divisor) on cpu {
    u64 start=0u64;
    u64 end=table.count;
    while(start<end){
        u64 middle=start+(end-start)/2u64;
        VfDivisionEntry entry=unsafe_position<VfDivisionEntry>(vf_division_entry(table,middle));
        bool before=entry.source<source || (entry.source==source &&
            (entry.kind<kind || (entry.kind==kind && entry.divisor<divisor)));
        if(before){start=middle+1u64;}
        else{end=middle;}
    }
    return start;
}

VfDivision vf_take_division(inout VfRows table,u64 at,u64 source,u64 kind,u64 divisor) on cpu {
    if(at<table.count){
        VfDivisionEntry entry=unsafe_position<VfDivisionEntry>(vf_division_entry(table,at));
        if(entry.source==source && entry.kind==kind && entry.divisor==divisor){
            table.count-=1u64;
            u64 index=at;
            while(index<table.count){
                unsafe_position<VfDivisionEntry>(vf_division_entry(table,index))=
                    unsafe_position<VfDivisionEntry>(vf_division_entry(table,index+1u64));
                index+=1u64;
            }
            return entry.value;
        }
    }
    return VfDivision(0u64,0u64,0u64,0);
}

void vf_insert_division(inout VfRows table,u64 at,VfDivisionEntry entry,
    cfn<address,address,u64,u64> resize) on cpu requires nonzero(resize) {
    vf_reserve(table,(table.count+1u64)*(size_of<VfDivisionEntry>()/size_of<u64>()),resize);
    u64 index=table.count;
    while(index>at){
        unsafe_position<VfDivisionEntry>(vf_division_entry(table,index))=
            unsafe_position<VfDivisionEntry>(vf_division_entry(table,index-1u64));
        index-=1u64;
    }
    unsafe_position<VfDivisionEntry>(vf_division_entry(table,at))=entry;
    table.count+=1u64;
}

struct VfOriginHead { u64 result; u32 kind; u32 pending; };
struct VfOriginBook { VfRows order; VfRows facts; };
struct VfOriginOrder { VfRows facts; u64 stride; };

address vf_origin_fact(VfRows facts,u64 ordinal,u64 stride) on cpu {
    return unsafe_address(facts.data+ordinal*stride);
}

address vf_ordered_origin(VfOriginBook book,u64 index,u64 stride) on cpu {
    u64 ordinal=unsafe_position<u64>(vf_word(book.order.data,index));
    return vf_origin_fact(book.facts,ordinal,stride);
}

void vf_note_origin(address state,u64 result,address instruction,VfReuse reuse,VfHost host) on cpu
    requires nonzero(host.scratch) requires nonzero(host.origin.resize) requires nonzero(host.origin.capture) {
    VfOriginBook book=unsafe_position<VfOriginBook>(state);
    u64 ordinal=book.order.count;
    u64 count=ordinal+1u64;
    vf_reserve(book.order,count,host.scratch);
    vf_reserve(book.facts,count,host.origin.resize);
    host.origin.capture(vf_origin_fact(book.facts,ordinal,host.origin.stride),result,instruction,reuse);
    unsafe_position<u64>(vf_word(book.order.data,ordinal))=ordinal;
    book.order.count=count;
    book.facts.count=count;
    unsafe_position<VfOriginBook>(state)=book;
}

extern "C" bool vf_origin_before(address left,address right,address context) on cpu {
    VfOriginOrder order=unsafe_position<VfOriginOrder>(context);
    u64 a=unsafe_position<u64>(left);
    u64 b=unsafe_position<u64>(right);
    u64 x=unsafe_position<VfOriginHead>(vf_origin_fact(order.facts,a,order.stride)).result;
    u64 y=unsafe_position<VfOriginHead>(vf_origin_fact(order.facts,b,order.stride)).result;
    return x<y || (x==y && a<b);
}

extern "C" void value_order_origins(address state,u64 stride) on cpu {
    VfOriginBook book=unsafe_position<VfOriginBook>(state);
    VfOriginOrder input=VfOriginOrder(book.facts,stride);
    sort_rows<u64>(unsafe_address(book.order.data),book.order.count,address_of(input),c_function(vf_origin_before));
}

u64 vf_origin_lower(VfOriginBook book,u64 result,u64 stride) on cpu {
    u64 start=0u64;
    u64 end=book.order.count;
    while(start<end){
        u64 middle=start+(end-start)/2u64;
        address fact=vf_ordered_origin(book,middle,stride);
        if(unsafe_position<VfOriginHead>(fact).result<result){start=middle+1u64;}
        else{end=middle;}
    }
    return start;
}

u32 vf_collect_origins(address state,u64 result,VfResultLocation location,u64 stride,VfOriginWriter writer) on cpu
    requires nonzero(writer.collect) {
    VfOriginBook book=unsafe_position<VfOriginBook>(state);
    u64 at=vf_origin_lower(book,result,stride);
    u32 kinds=0;
    while(at<book.order.count){
        address fact=vf_ordered_origin(book,at,stride);
        VfOriginHead head=unsafe_position<VfOriginHead>(fact);
        if(head.result!=result || head.pending==0){break;}
        kinds|=u32(1)<<head.kind;
        writer.collect(writer.sink,fact,location);
        unsafe_position<VfOriginHead>(fact).pending=0;
        book.facts.count-=1u64;
        at+=1u64;
    }
    if(kinds!=0){unsafe_position<VfOriginBook>(state).facts.count=book.facts.count;}
    return kinds;
}

bool vf_retain_origin_block(RewriteSpan code,address state,Host read,RewriteHost write,u64 stride,VfOriginWriter writer) on cpu
    requires nonzero(read.inspect) requires nonzero(write.inspect)
    requires nonzero(writer.collect) requires nonzero(writer.attach) {
    u64 index=0u64;
    while(index<code.count){
        address inst=unsafe_address(address_bits(code.data)+index*code.stride);
        View value=read.inspect(Node(inst,N_INSTRUCTION));
        OriginResults results=origin_results(value,read);
        u64 at=0u64;
        u32 kinds=0;
        while(at<results.count){
            OriginResult result=next_origin_result(results);
            kinds|=vf_collect_origins(state,result.reg,VfResultLocation(at,results.count,result.role),stride,writer);
            at+=1u64;
        }
        if(kinds!=0){
            if((kinds&1)!=0){writer.attach(writer.sink,inst,VfReuseKind::Expression);}
            if((kinds&2)!=0){writer.attach(writer.sink,inst,VfReuseKind::LocalRead);}
            if((kinds&4)!=0){writer.attach(writer.sink,inst,VfReuseKind::IndexedStoreRead);}
            if((kinds&8)!=0){writer.attach(writer.sink,inst,VfReuseKind::IndexedRead);}
            if(unsafe_position<VfOriginBook>(state).facts.count==0u64){return false;}
        }
        if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE){
            RewriteView children=write.inspect(RewriteNode(inst,N_INSTRUCTION));
            if(!vf_retain_origin_block(children.first,state,read,write,stride,writer) ||
               !vf_retain_origin_block(children.second,state,read,write,stride,writer)){return false;}
        }
        index+=1u64;
    }
    return true;
}

extern "C" void value_retain_origins(RewriteSpan code,address state,Host read,RewriteHost write,
    u64 stride,VfOriginWriter writer) on cpu requires nonzero(read.inspect) requires nonzero(write.inspect)
    requires nonzero(writer.collect) requires nonzero(writer.attach) {
    if(unsafe_position<VfOriginBook>(state).facts.count!=0u64){
        vf_retain_origin_block(code,state,read,write,stride,writer);
    }
}

extern "C" bool value_take_input_origins(address state,u64 result,address sink,VfOriginHost host,
    cfn<void,address,address> accept) on cpu requires nonzero(host.discard) requires nonzero(accept) {
    VfOriginBook book=unsafe_position<VfOriginBook>(state);
    u64 at=vf_origin_lower(book,result,host.stride);
    bool accepted=false;
    while(at<book.order.count){
        address fact=vf_ordered_origin(book,at,host.stride);
        VfOriginHead head=unsafe_position<VfOriginHead>(fact);
        if(head.result!=result || head.pending==0){break;}
        if(head.kind==u32(VfReuseKind::LocalRead)){accept(sink,fact);accepted=true;}
        else{host.discard(fact);}
        unsafe_position<VfOriginHead>(fact).pending=0;
        book.facts.count-=1u64;
        at+=1u64;
    }
    unsafe_position<VfOriginBook>(state).facts.count=book.facts.count;
    return accepted;
}

extern "C" void value_release_origins(address state,cfn<address,address,u64,u64> resize,VfOriginHost host) on cpu
    requires nonzero(resize) requires nonzero(host.resize) requires nonzero(host.discard) {
    VfOriginBook book=unsafe_position<VfOriginBook>(state);
    if(book.facts.count!=0u64){
        u64 ordinal=0u64;
        while(ordinal<book.order.count){
            address fact=vf_origin_fact(book.facts,ordinal,host.stride);
            if(unsafe_position<VfOriginHead>(fact).pending!=0){host.discard(fact);}
            ordinal+=1u64;
        }
    }
    vf_release(book.order,resize);
    vf_release(book.facts,host.resize);
}

VfExpression value_expression(View node,Span registers,RegisterHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
    u32 kind=node.tag;
    if(kind==OP_CONSTANT){return VfExpression(1,u32(node.c),node.b,0u64,0u64,0u64);}
    if(kind==OP_INDEX_VALUE){return VfExpression(2,0,0u64,0u64,0u64,0u64);}
    if(kind==OP_UNIFORM){return VfExpression(3,0,node.b,0u64,0u64,0u64);}
    if(kind==OP_BUFFER_STRIDE){return VfExpression(4,0,node.b,0u64,0u64,0u64);}
    if(kind==OP_UNARY){return VfExpression(5,u32(node.c),node.b,u64(host.scalar(registers,node.a).kind),0u64,0u64);}
    if(kind==OP_BINARY){return VfExpression(6,host.ir.inspect(node.right).tag,node.b,node.c,0u64,0u64);}
    if(kind==OP_CAST){return VfExpression(7,host.scalar(registers,node.a).kind,node.b,0u64,0u64,0u64);}
    if(kind==OP_MATH){return VfExpression(8,u32(node.b),0u64,0u64,0u64,node.first.count);}
    if(kind==OP_SELECT_VALUE){return VfExpression(9,0,node.b,node.c,node.d,0u64);}
    return VfExpression(0,0,0u64,0u64,0u64,0u64);
}
VfAction value_instruction(address instruction,address retained_at,address values,address expressions,
    inout VfRows divisions,inout VfFlow flow,address context,Span registers,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.rewrite.inspect) requires nonzero(host.resize_rows) requires nonzero(host.scratch)
    requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.origin.capture) requires nonzero(host.origin.resize)
    requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin) requires nonzero(host.memory.copy_origin)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate)
    {
    address register_state=vf_registers(values);
    if(unsafe_position<VfRegisters>(register_state).aliases.count!=0u64){
        rewrite_operands(RewriteNode(instruction,N_INSTRUCTION),host.rewrite,
            RewriteInputMap(register_state,c_function(value_root)));
    }
    View node=host.values.ir.inspect(Node(instruction,N_INSTRUCTION));
    u32 kind=node.tag;
    address none=unsafe_address(0u64);
    if(kind==OP_BRANCH || kind==OP_LOOP_VALUE || kind==OP_SCOPE || kind==OP_EXIT_VALUE || kind==OP_RAISE ||
        kind==OP_TILE_PUBLISH || kind==OP_TILE_COLLECTIVE || kind==OP_TILE_BARRIER || kind==OP_BREAK_VALUE || kind==OP_CONTINUE_VALUE){
        divisions.count=0u64;
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
            VfExpression key=VfExpression(10,0,node.a,divisor_kind,divisor.a,0u64);
            VfOptional old=vf_expression(expressions,key,0u64,quotient_result,host.scratch);
            if(old.present!=0){
                vf_put(register_state,VfRegMap::Aliases,quotient_result,old.value,host.resize_rows);
                vf_note_origin(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Quotient,paired),host);
            } else {retained|=1;}
        }
        if((mask&2)!=0){
            VfExpression key=VfExpression(10,1,node.a,divisor_kind,divisor.a,0u64);
            VfOptional old=vf_expression(expressions,key,0u64,remainder_result,host.scratch);
            if(old.present!=0){
                vf_put(register_state,VfRegMap::Aliases,remainder_result,old.value,host.resize_rows);
                vf_note_origin(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Remainder,paired),host);
            } else {retained|=2;}
        }
        if(retained==0){return VfAction::Discard;}
        VfDivisionChoice choice;
        if(retained==1){choice=VfDivisionChoice::Quotient;}
        else if(retained==2){choice=VfDivisionChoice::Remainder;}
        else {choice=VfDivisionChoice::Both;}
        InstructionStorage storage=host.storage(instruction);
        host.rewrite_division(storage.payload,choice,quotient_result,remainder_result);
        u64 division=vf_division_lower(divisions,node.a,divisor_kind,divisor.a);
        VfDivision previous=vf_take_division(divisions,division,node.a,divisor_kind,divisor.a);
        if((previous.mask==1 && retained==2)||(previous.mask==2 && retained==1)){
            u64 quotient=retained==1 ? quotient_result : previous.quotient;
            u64 remainder=retained==2 ? remainder_result : previous.remainder;
            address earlier=unsafe_address(previous.instruction);
            address quotient_from=retained==1 ? instruction : earlier;
            address remainder_from=retained==2 ? instruction : earlier;
            host.pair(earlier,quotient_from,remainder_from,storage.input,node.a,quotient,remainder);
            return VfAction::Discard;
        }
        if(retained!=3){
            VfDivision next=VfDivision(address_bits(retained_at),quotient_result,remainder_result,retained);
            vf_insert_division(divisions,division,VfDivisionEntry(node.a,divisor_kind,divisor.a,next),host.scratch);
        }
    }
    VfExpression expression=value_expression(node,registers,host.values);
    if(expression.kind!=0){
        VfOptional old=vf_expression(expressions,expression,address_bits(node.first.data),node.a,host.scratch);
        if(old.present!=0){
            vf_put(register_state,VfRegMap::Aliases,node.a,old.value,host.resize_rows);
            vf_note_origin(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Value,0),host);
            return VfAction::Discard;
        }
    }
    bool continuing=true;
    if(kind==OP_SCOPE){
        VfValues child=vf_take_values(values);
        address scoped=address_of(child);
        VfFlow scope=value_block(host.storage(instruction).first_code,scoped,expressions,context,registers,host,false);
        vf_move_rows(register_state,vf_registers(scoped),VfRegMap::Aliases,host.resize_rows);
        vf_merge(scope.next,scope.exits,host.resize_rows);
        scope.exits=vf_no_path();
        continuing=vf_resume(values,flow,scope,host);
        vf_release_values(scoped,host);
    } else if(kind==OP_RAISE){continuing=false;}
    else if(kind==OP_EXIT_VALUE){vf_merge(flow.exits,vf_snapshot(register_state,host.resize_rows),host.resize_rows);continuing=false;}
    else if(kind==OP_BREAK_VALUE || kind==OP_CONTINUE_VALUE){continuing=false;}
    else if(kind==OP_ATOMIC){
        if(host.values.ir.inspect(node.right).tag!=0){
            vf_forget_indexed(host.values.ir.inspect(node.left),values,false,host);
        }
        vf_clear_memory(vf_memory_rows(values),host.memory);
    } else if(kind==OP_APPEND || kind==OP_TILE_BARRIER){vf_clear_memory(vf_memory_rows(values),host.memory);}
    else if(kind==OP_LOAD_LOCAL){
        VfOptional old=vf_get(register_state,VfRegMap::Locals,node.b);
        if(old.present!=0){
            u64 value=value_root(register_state,old.value);
            vf_put(register_state,VfRegMap::Aliases,node.a,value,host.resize_rows);
            vf_note_origin(context,value,instruction,VfReuse(none,VfReuseKind::LocalRead,VfRole::Value,0),host);
        }
        vf_put(register_state,VfRegMap::Locals,node.b,node.a,host.resize_rows);
    } else if(kind==OP_STORE_LOCAL){vf_put(register_state,VfRegMap::Locals,node.a,node.b,host.resize_rows);}
    else if(kind==OP_STORE_LOCAL_INDEXED){vf_remove_range(register_state,node.a,node.a+node.b);}
    else if(kind==OP_STORE_INDEXED){vf_forget_indexed(host.values.ir.inspect(node.left),values,true,host);}
    else if(kind==OP_LOAD){
        VfMemory old=vf_memory_get(values,node.b,node.c,u32(node.d),host.memory);
        if(old.present!=0){
            vf_put(register_state,VfRegMap::Aliases,node.a,old.value,host.resize_rows);
            VfReuseKind reuse;
            if(old.origin!=0u64){reuse=VfReuseKind::IndexedStoreRead;}
            else {reuse=VfReuseKind::IndexedRead;}
            vf_note_origin(context,old.value,instruction,VfReuse(unsafe_address(old.origin),reuse,VfRole::Value,0),host);
            return VfAction::Discard;
        }
        vf_memory_insert(values,node.b,node.c,u32(node.d),node.a,none,host.memory);
    } else if(kind==OP_STORE){
        vf_memory_forget(values,node.a,true,node.b,u32(node.c),host.memory);
        vf_memory_insert(values,node.a,node.b,u32(node.c),node.d,instruction,host.memory);
    } else if(kind==OP_BRANCH){
        VfValues then_values=vf_fork_values(values,host);
        VfValues else_values=vf_take_values(values);
        address yes=address_of(then_values);
        address no=address_of(else_values);
        InstructionStorage storage=host.storage(instruction);
        VfFlow branches=value_block(storage.first_code,yes,expressions,context,registers,host,false);
        VfFlow other=value_block(storage.second_code,no,expressions,context,registers,host,false);
        vf_merge(branches.next,other.next,host.resize_rows);
        vf_merge(branches.exits,other.exits,host.resize_rows);
        vf_move_rows(register_state,vf_registers(yes),VfRegMap::Aliases,host.resize_rows);
        vf_merge_aliases(register_state,vf_registers(no),host.resize_rows);
        continuing=vf_resume(values,flow,branches,host);
        vf_release_values(yes,host);
        vf_release_values(no,host);
    } else if(kind==OP_LOOP_VALUE){
        visit_local_writes(node.first,host.values.ir,register_state,c_function(vf_remove_local));
        visit_local_writes(node.second,host.values.ir,register_state,c_function(vf_remove_local));
        InstructionStorage storage=host.storage(instruction);
        VfPath after=vf_snapshot(register_state,host.resize_rows);
        vf_clear_memory(vf_memory_rows(values),host.memory);
        VfValues cycle_values=vf_take_values(values);
        address cycle=address_of(cycle_values);
        VfExpressionMark mark=vf_mark_expressions(expressions);
        VfFlow condition=value_block(storage.first_code,cycle,expressions,context,registers,host,true);
        unsafe_position<u64>(storage.payload)=value_root(vf_registers(cycle),node.a);
        VfFlow loop_flow=VfFlow(vf_no_path(),condition.exits);
        if(condition.next.reached!=0){
            vf_release(condition.next.rows,host.resize_rows);
            loop_flow.next=after;
            VfFlow body=value_block(storage.second_code,cycle,expressions,context,registers,host,true);
            vf_release(body.next.rows,host.resize_rows);
            vf_merge(loop_flow.exits,body.exits,host.resize_rows);
        } else {vf_release(after.rows,host.resize_rows);}
        vf_rewind_expressions(expressions,mark);
        vf_move_rows(register_state,vf_registers(cycle),VfRegMap::Aliases,host.resize_rows);
        continuing=vf_resume(values,flow,loop_flow,host);
        vf_release_values(cycle,host);
    } else if(kind==OP_INVOKE){vf_forget_call(node.left,values,host);}
    return continuing ? VfAction::Keep : VfAction::Stop;
}
extern "C" VfFlow value_block(address block,address values,address expressions,address context,
    Span registers,VfHost host,bool keep_expressions) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.rewrite.inspect) requires nonzero(host.resize_rows) requires nonzero(host.scratch)
    requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.origin.capture) requires nonzero(host.origin.resize)
    requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin) requires nonzero(host.memory.copy_origin)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate) {
    Span code=host.instruction_span(block);
    VfFlow flow=VfFlow(vf_no_path(),vf_no_path());
    VfRows divisions=vf_empty_rows();
    VfExpressionMark mark=vf_mark_expressions(expressions);
    u64 kept=0u64;
    u64 index=0u64;
    bool continuing=true;
    // The retained prefix never moves again, including slots awaiting a division pair.
    while(index<code.count){
        address instruction=span_item_at(code,index);
        address retained_at=span_item_at(code,kept);
        VfAction action=value_instruction(instruction,retained_at,values,expressions,divisions,flow,context,registers,host);
        if(action!=VfAction::Discard){
            if(kept!=index){host.swap_instructions(instruction,retained_at);}
            kept+=1u64;
            if(action==VfAction::Stop){continuing=false;break;}
        }
        index+=1u64;
    }
    vf_release(divisions,host.scratch);
    if(!keep_expressions){vf_rewind_expressions(expressions,mark);}
    if(kept!=code.count){host.truncate(block,kept);}
    if(continuing){flow.next=vf_snapshot(vf_registers(values),host.resize_rows);}
    return flow;
}
extern "C" void value_discard_flow(VfFlow flow,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize) {
    vf_release(flow.next.rows,resize);
    vf_release(flow.exits.rows,resize);
}

address vf_reduction_part(address blocks,u64 part) on cpu {
    return unsafe_position<address>(unsafe_address(address_bits(blocks)+part*size_of<address>()));
}

RewriteSpan vf_reduction_root(address roots,u64 index) on cpu {
    return unsafe_position<RewriteSpan>(unsafe_address(address_bits(roots)+index*size_of<RewriteSpan>()));
}

extern "C" void value_reduction(VfReduction input,VfHost host,VfOriginWriter writer,VfReductionOrigins origins) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.rewrite.inspect) requires nonzero(host.resize_rows) requires nonzero(host.scratch)
    requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.origin.capture) requires nonzero(host.origin.resize) requires nonzero(host.origin.discard)
    requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin) requires nonzero(host.memory.copy_origin)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate)
    requires nonzero(writer.collect) requires nonzero(writer.attach)
    requires nonzero(origins.accept) requires nonzero(origins.finish_input) {
    VfValues entry=vf_empty_values();
    VfExpressions expressions=VfExpressions(vf_empty_rows(),vf_empty_rows(),vf_empty_rows());
    VfOriginBook pending=VfOriginBook(vf_empty_rows(),vf_empty_rows());
    address state=address_of(entry);
    address table=address_of(expressions);
    address book=address_of(pending);
    address entry_block=vf_reduction_part(input.parts,0u64);
    value_discard_flow(value_block(entry_block,state,table,book,input.registers,host,true),host.resize_rows);
    VfRows seed=unsafe_position<VfRegisters>(vf_registers(state)).aliases;
    VfRegisters aliases=VfRegisters(vf_copy_rows(seed,host.resize_rows),vf_empty_rows());
    address merged=address_of(aliases);
    u64 last=3u64;
    while(last!=0u64 && host.instruction_span(vf_reduction_part(input.parts,last)).count==0u64){last-=1u64;}
    u64 part=1u64;
    while(part<=last){
        address block=vf_reduction_part(input.parts,part);
        if(host.instruction_span(block).count!=0u64){
            bool final_part=part==last;
            VfValues working=final_part ? vf_take_values(state) : vf_fork_values(state,host);
            address active=address_of(working);
            value_discard_flow(value_block(block,active,table,book,input.registers,host,final_part),host.resize_rows);
            vf_merge_aliases(merged,vf_registers(active),host.resize_rows);
            vf_release_values(active,host);
        }
        part+=1u64;
    }
    vf_release_values(state,host);
    value_release_expressions(table,host.scratch);
    value_order_origins(book,host.origin.stride);
    part=0u64;
    while(part<4u64){
        Span code=host.instruction_span(vf_reduction_part(input.parts,part));
        value_retain_origins(RewriteSpan(code.data,code.count,code.stride),book,host.values.ir,host.rewrite,host.origin.stride,writer);
        part+=1u64;
    }
    if(origins.inputs!=address()){
        // The original input registers still identify the reads waiting for a real input.
        u64 role=0u64;
        while(role<3u64){
            RewriteSpan root=vf_reduction_root(input.roots,role==2u64 ? 0u64 : 3u64+role);
            u64 index=0u64;
            while(index<root.count){
                u64 reg=unsafe_position<u64>(unsafe_address(address_bits(root.data)+index*root.stride));
                if(value_take_input_origins(book,reg,origins.reads,host.origin,origins.accept)){
                    origins.finish_input(origins.reads,origins.inputs,u32(role),index);
                }
                index+=1u64;
            }
            role+=1u64;
        }
    }
    value_release_origins(book,host.scratch,host.origin);
    u64 root=0u64;
    while(root<6u64){
        value_rebind_roots(merged,vf_reduction_root(input.roots,root));
        root+=1u64;
    }
    value_release_registers(merged,host.resize_rows);
}
