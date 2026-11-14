module bootstrap.coloring;
import bootstrap.names;

const u32 UNSEEN = 0;
const u32 IMMEDIATE = 1;
const u32 GPR = 2;
const u32 XMM = 3;
const u32 STACK = 4;
const u32 VARYING = 5;

address place_at(address places, u64 index) on cpu {
    return unsafe_address(address_bits(places) + index * size_of<ColorPlace>());
}

address order_at(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<array<u64,3>>());
}
address spill_mark(address rows, u32 slot) on cpu {
    return unsafe_address(address_bits(rows) + u64(slot) * size_of<u64>());
}
extern "C" bool priority_before(address left, address right, address context) on cpu {
    array<u64,3> a = unsafe_position<array<u64,3>>(left);
    array<u64,3> b = unsafe_position<array<u64,3>>(right);
    if (a[0] != b[0]) { return a[0] > b[0]; }
    if (a[1] != b[1]) { return a[1] > b[1]; }
    return a[2] < b[2];
}
// After coloring, every order row is dead. Reuse that memory first for the
// maximum width of each spill color, then for its final stack offset.
u64 layout_spills(address places, u64 count, address scratch, u64 spills, u64 base) on cpu {
    u64 end = base;
    u64 slot = 0u64;
    while (slot < spills) {
        unsafe_position<u64>(order_at(scratch, slot)) = 0u64;
        slot += 1u64;
    }
    u64 id = 0u64;
    while (id < count) {
        ColorPlace place = unsafe_position<ColorPlace>(place_at(places, id));
        if (place.kind == STACK) {
            address row = order_at(scratch, place.value);
            u64 width = u64(place.width);
            if (unsafe_position<u64>(row) < width) {
                unsafe_position<u64>(row) = width;
            }
        }
        id += 1u64;
    }
    slot = 0u64;
    while (slot < spills) {
        address row = order_at(scratch, slot);
        u64 width = unsafe_position<u64>(row);
        if (width != 0u64) {
            end = ((end + width - 1u64) / width) * width;
        }
        unsafe_position<u64>(row) = end;
        end += width;
        slot += 1u64;
    }
    id = 0u64;
    while (id < count) {
        address row = place_at(places, id);
        if (unsafe_position<ColorPlace>(row).kind == STACK) {
            slot = unsafe_position<ColorPlace>(row).value;
            unsafe_position<ColorPlace>(row).value = unsafe_position<u64>(order_at(scratch, slot));
        }
        id += 1u64;
    }
    return end;
}

extern "C" ColorLayout color(address graph, u64 count, address places, address order, u32 mode, u64 base,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next) {
    u64 pending = 0u64;
    u64 id = 0u64;
    while (id < count) {
        if (unsafe_position<ColorPlace>(place_at(places, id)).kind != IMMEDIATE) {
            ColorNode facts = node(graph, id);
            unsafe_position<array<u64,3>>(order_at(order, pending)) = array<u64,3>(facts.weight, facts.degree, id);
            pending += 1u64;
        }
        id += 1u64;
    }
    sort_rows<array<u64,3>>(order, pending, address(), c_function(priority_before));
    u32 spills = 0;
    u32 saved_gpr = 0;
    u32 saved_xmm = 0;
    u64 at = 0u64;
    while (at < pending) {
        id = unsafe_position<array<u64,3>>(order_at(order, at))[2];
        ColorNode facts = node(graph, id);
        address destination = place_at(places, id);
        u32 bank = GPR;
        u32 allowed = mode == 1 ? 0x3e0 : 0xfe0;
        if (facts.across_call != 0u64) { allowed &= 0xe0; }
        if (unsafe_position<ColorPlace>(destination).floating != 0) {
            bank = XMM;
            allowed = facts.across_call != 0u64 ? 0xffc0 : 0xfffc;
        }
        if (mode == 2) { bank = XMM; allowed = 0xfff0; }

        u32 used = 0;
        u64 visit = at + 1u64;
        u64 neighbor = next(graph, id, false, count);
        while (neighbor != count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank) { used |= 1 << u32(place.value); }
            if (place.kind == STACK) {
                unsafe_position<u64>(spill_mark(order, u32(place.value))) = visit;
            }
            neighbor = next(graph, id, false, neighbor);
        }
        u32 available = allowed & ~used;
        u32 chosen = 16;
        neighbor = next(graph, id, true, count);
        while (neighbor != count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank && (available & (1 << u32(place.value))) != 0) {
                chosen = u32(place.value);
                break;
            }
            neighbor = next(graph, id, true, neighbor);
        }
        if (chosen == 16) {
            u32 candidate = 0;
            while (candidate < 16) {
                u32 reg = candidate;
                if (bank == GPR) {
                    if (candidate >= 7) { break; }
                    reg = candidate < 4 ? candidate + 8 : (candidate < 6 ? candidate + 2 : 5);
                }
                if ((available & (1 << reg)) != 0) { chosen = reg; break; }
                candidate += 1;
            }
        }
        if (chosen != 16) {
            unsafe_position<ColorPlace>(destination).value = u64(chosen);
            unsafe_position<ColorPlace>(destination).kind = bank;
            // Addressable fields receive their final stack positions after coloring.
            if (unsafe_position<ColorPlace>(destination).width != 0) {
                if (bank == GPR) { saved_gpr |= (1 << chosen) & 0xe0; }
                else { saved_xmm |= (1 << chosen) & 0xffc0; }
            }
        } else {
            u32 slot = 0;
            while (slot < spills && unsafe_position<u64>(spill_mark(order, slot)) == visit) {
                slot += 1;
            }
            if (slot == spills) {
                // At most one color per processed node. The current order row
                // has been read, so the marks still fit in the consumed rows.
                unsafe_position<u64>(spill_mark(order, slot)) = 0u64;
                spills += 1;
            }
            unsafe_position<ColorPlace>(destination).value = u64(slot);
            unsafe_position<ColorPlace>(destination).kind = STACK;
        }
        at += 1u64;
    }
    u64 end = base;
    if (spills != 0) { end = layout_spills(places, count, order, u64(spills), base); }
    return ColorLayout(end, saved_gpr, saved_xmm);
}

extern "C" u64 place_private(address places, u64 count, u64 data_end, u64 bytes, u64 alignment)
    on cpu requires nonzero(alignment) {
    u64 base = ((data_end + alignment - 1u64) / alignment) * alignment;
    u64 index = 0u64;
    while (index < count) {
        address at = place_at(places, index);
        unsafe_position<ColorPlace>(at).value += base;
        unsafe_position<ColorPlace>(at).kind = STACK;
        index += 1u64;
    }
    return base + bytes;
}

extern "C" void record_definition(address place, u32 literal, u64 bits) on cpu {
    u32 kind = unsafe_position<ColorPlace>(place).kind;
    if (literal != 0 && (kind == UNSEEN
        || (kind == IMMEDIATE && unsafe_position<ColorPlace>(place).value == bits))) {
        unsafe_position<ColorPlace>(place).kind = IMMEDIATE;
        unsafe_position<ColorPlace>(place).value = bits;
    } else {
        unsafe_position<ColorPlace>(place).kind = VARYING;
    }
}

u64 graph_saturating_add(u64 left,u64 right) on cpu {
    u64 maximum=18446744073709551615u64;
    return left>maximum-right ? maximum : left+right;
}
u64 graph_nested_weight(u64 weight) on cpu {
    u64 maximum=18446744073709551615u64;
    return weight>maximum/8u64 ? maximum : weight*8u64;
}
u64 graph_slot_id(address graph,GraphSpace space,u64 index,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    return space==GraphSpace::Register ? index : core.counter(graph,GraphCounter::Registers)+index;
}
void graph_edge(address graph,u64 first,u64 second,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    if(first!=second && core.addressable(graph,first)==0 && core.addressable(graph,second)==0){
        core.relation(graph,GraphRelation::Edge,first,second);
        core.relation(graph,GraphRelation::Edge,second,first);
    }
}
void graph_weight(address graph,u64 id,u64 amount,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    core.set_weight(graph,id,graph_saturating_add(core.weight(graph,id),amount));
}
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    u64 id=graph_slot_id(graph,space,index,core);
    if(event.kind==0){
        if(event.skip_present==0 || id!=event.skip){graph_edge(graph,event.anchor,id,core);}
    } else if(event.kind==1){
        if(core.addressable(graph,id)==0){core.visit_set(live,ordinal,graph,GraphEvent(id,0u64,0,0));}
    } else {core.across(graph,id);}
}
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,1,0));
}
extern "C" void graph_inputs(address graph,address inputs,u64 count,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    u64 index=0u64;
    while(index<count){
        u64 first=unsafe_position<u64>(unsafe_address(address_bits(inputs)+index*8u64));
        u64 before=0u64;
        while(before<index){
            u64 second=unsafe_position<u64>(unsafe_address(address_bits(inputs)+before*8u64));
            graph_edge(graph,first,second,core);
            before+=1u64;
        }
        index+=1u64;
    }
}
extern "C" void graph_definition(address graph,address live,GraphSpace space,u64 index,GraphDefinition definition,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    if(definition.pass!=0){core.slot(live,GraphSlotAction::Remove,space,index);return;}
    if(space==GraphSpace::Register){record_definition(core.location(graph,index),definition.literal,definition.bits);}
    u64 id=graph_slot_id(graph,space,index,core);
    if(core.addressable(graph,id)==0){
        GraphEvent event=GraphEvent(id,definition.transfer_src,0,0);
        if(definition.transfer!=0 && id==definition.transfer_dst){event.skip_present=1;}
        core.visit_set(live,18446744073709551615u64,graph,event);
    }
    graph_weight(graph,id,definition.weight,core);
}
extern "C" void graph_read(address graph,address live,GraphSpace space,u64 index,u64 weight,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    u64 id=graph_slot_id(graph,space,index,core);
    graph_weight(graph,id,weight,core);
    core.slot(live,GraphSlotAction::Insert,space,index);
}
extern "C" void graph_block(address graph,GraphBlock code,GraphScalars registers,address live,
    address break_live,address continue_live,address exit_live,u64 weight,GraphHost host) on cpu 
    requires nonzero(host.core.addressable) requires nonzero(host.core.relation) requires nonzero(host.core.weight) requires nonzero(host.core.set_weight)
    requires nonzero(host.core.across) requires nonzero(host.core.counter) requires nonzero(host.core.set_outgoing) requires nonzero(host.core.location)
    requires nonzero(host.core.visit_set) requires nonzero(host.core.slot) requires nonzero(host.read) requires nonzero(host.clone_set)
    requires nonzero(host.drop_set) requires nonzero(host.change) requires nonzero(host.loop_live) requires nonzero(host.definitions)
    requires nonzero(host.reads) {
    address none=unsafe_address(0u64);
    u64 remaining=code.count;
    while(remaining!=0u64){
        remaining-=1u64;
        GraphNode node=host.read(code,remaining,registers);
        if(node.kind==GraphKind::If){
            address yes=host.clone_set(live);
            graph_block(graph,node.first,registers,yes,break_live,continue_live,exit_live,weight,host);
            graph_block(graph,node.second,registers,live,break_live,continue_live,exit_live,weight,host);
            host.change(GraphSetAction::DrainUnion,live,yes);
            host.drop_set(yes);
            host.core.slot(live,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,weight,host.core);
        } else if(node.kind==GraphKind::Loop){
            address header=host.loop_live(node.first,node.a,node.second,live,break_live,continue_live,exit_live);
            u64 nested=graph_nested_weight(weight);
            address before=host.clone_set(header);
            graph_block(graph,node.second,registers,before,live,header,exit_live,nested,host);
            host.change(GraphSetAction::DrainUnion,before,live);
            host.core.slot(before,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,nested,host.core);
            graph_block(graph,node.first,registers,before,break_live,continue_live,exit_live,nested,host);
            host.change(GraphSetAction::Swap,live,before);
            host.drop_set(before);
            host.drop_set(header);
        } else if(node.kind==GraphKind::Scope){
            address before=host.clone_set(live);
            graph_block(graph,node.first,registers,before,break_live,continue_live,live,weight,host);
            host.change(GraphSetAction::Swap,live,before);
            host.drop_set(before);
        } else if(node.kind==GraphKind::Raise){
            host.change(GraphSetAction::Clear,live,none);
            host.core.slot(live,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,weight,host.core);
        } else if(node.kind==GraphKind::Exit){host.change(GraphSetAction::Copy,live,exit_live);}
        else if(node.kind==GraphKind::Break){host.change(GraphSetAction::Copy,live,break_live);}
        else if(node.kind==GraphKind::Continue){host.change(GraphSetAction::Copy,live,continue_live);}
        else {
            u32 transfer=0;
            u64 destination=node.dst;
            u64 source=node.a;
            if(node.kind==GraphKind::LocalLoad){transfer=1;source+=host.core.counter(graph,GraphCounter::Registers);}
            else if(node.kind==GraphKind::LocalStore){transfer=1;destination+=host.core.counter(graph,GraphCounter::Registers);}
            else if(node.kind==GraphKind::Cast && (node.flags&1)!=0){transfer=1;}
            if(transfer!=0 && (host.core.addressable(graph,destination)!=0 || host.core.addressable(graph,source)!=0)){transfer=0;}
            u32 preferred=transfer;
            u64 preferred_destination=destination;
            u64 preferred_source=source;
            if(preferred==0){
                if(node.kind==GraphKind::Unary || node.kind==GraphKind::Select){preferred=1;preferred_destination=node.dst;preferred_source=node.a;}
                else if(node.kind==GraphKind::Binary){
                    preferred=1;preferred_destination=node.dst;preferred_source=node.a;
                    if((node.flags&4)!=0){
                        bool reverse=(node.flags&2)!=0 ? (node.operation==3 || node.operation==5) : (node.operation==2 || node.operation==5);
                        if(reverse){preferred_source=node.b;}
                    }
                } else if(node.kind==GraphKind::Math && (node.operation==0 || node.operation==9 || node.operation==10 || node.operation==11)){
                    preferred=1;preferred_destination=node.dst;preferred_source=unsafe_position<u64>(node.arguments);
                }
            }
            if(preferred!=0){
                host.core.relation(graph,GraphRelation::Affinity,preferred_destination,preferred_source);
                host.core.relation(graph,GraphRelation::Affinity,preferred_source,preferred_destination);
            }
            u32 literal=node.kind==GraphKind::Constant ? 1 : 0;
            GraphDefinition definitions=GraphDefinition(weight,destination,source,node.bits,transfer,literal,0);
            host.definitions(node.instruction,graph,live,definitions);
            definitions.pass=1;
            host.definitions(node.instruction,graph,live,definitions);
            bool calls=node.kind==GraphKind::Print || node.kind==GraphKind::Call ||
                (node.kind==GraphKind::Math && node.operation>=1 && node.operation<=8);
            if(calls){
                u64 outgoing=node.kind==GraphKind::Call ? node.outgoing : 32u64;
                if(outgoing>host.core.counter(graph,GraphCounter::Outgoing)){host.core.set_outgoing(graph,outgoing);}
                host.core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,2,0));
            }
            host.reads(node.instruction,graph,live,weight);
        }
    }
}
