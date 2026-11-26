module bootstrap.coloring;
import bootstrap.names;
import bootstrap.ir_dataflow;

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

u64 packet_order(address graph, u64 count, address places, address order, Span uses,
    cfn<ColorNode,address,u64> node, cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next) {
    u64 first_skipped = count;
    u64 id = 0u64;
    while (id < count) {
        bool index_only = id < uses.count && unsafe_position<u8>(
            unsafe_address(address_bits(uses.data) + id * u64(uses.stride))) == u8(1);
        unsafe_position<ColorPlace>(place_at(places, id)) =
            ColorPlace(0u64, UNSEEN, index_only ? u8(0) : u8(16), u8(0));
        if (index_only) {
            first_skipped = id < first_skipped ? id : first_skipped;
        } else {
            ColorNode facts = node(graph, id);
            unsafe_position<array<u64,3>>(order_at(order, id)) = array<u64,3>(facts.weight, facts.degree, id);
        }
        id += 1u64;
    }

    // The original graph is symmetric. Discount each edge from a skipped
    // register to an active neighbor in that neighbor's actual priority row.
    id = first_skipped;
    while (id < uses.count) {
        if (unsafe_position<ColorPlace>(place_at(places, id)).width == u8(0)) {
            u64 neighbor = next(graph, id, false, count);
            while (neighbor != count) {
                if (unsafe_position<ColorPlace>(place_at(places, neighbor)).width != u8(0)) {
                    address row = order_at(order, neighbor);
                    array<u64,3> priority = unsafe_position<array<u64,3>>(row);
                    priority[1] -= 1u64;
                    unsafe_position<array<u64,3>>(row) = priority;
                }
                neighbor = next(graph, id, false, neighbor);
            }
        }
        id += 1u64;
    }

    u64 pending = first_skipped;
    id = first_skipped;
    while (id < count) {
        if (unsafe_position<ColorPlace>(place_at(places, id)).width != u8(0)) {
            unsafe_position<array<u64,3>>(order_at(order, pending)) =
                unsafe_position<array<u64,3>>(order_at(order, id));
            pending += 1u64;
        }
        id += 1u64;
    }
    return pending;
}

extern "C" ColorLayout color(address graph, u64 count, address places, address order, u32 mode, u64 base, Span packet_uses,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next) {
    u64 pending = 0u64;
    u64 id = 0u64;
    if (mode == 2) {
        pending = packet_order(graph, count, places, order, packet_uses, node, next);
    } else {
        while (id < count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, id));
            // Addressable fields receive their stack positions after coloring.
            if (place.kind != IMMEDIATE && place.width != 0) {
                ColorNode facts = node(graph, id);
                unsafe_position<array<u64,3>>(order_at(order, pending)) = array<u64,3>(facts.weight, facts.degree, id);
                pending += 1u64;
            }
            id += 1u64;
        }
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
            if (bank == GPR) { saved_gpr |= (1 << chosen) & 0xe0; }
            else { saved_xmm |= (1 << chosen) & 0xffc0; }
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

extern "C" u64 place_private(address places, u64 data_end, usize registers, address context,
    cfn<PrivateDeclaration,address> next) on cpu requires nonzero(next) {
    u64 end = data_end;
    while (true) {
        PrivateDeclaration declaration = next(context);
        Span offsets = declaration.offsets;
        if (address_bits(offsets.data) == 0u64) { return end; }
        u64 alignment = declaration.alignment > 16 ? u64(declaration.alignment) : 16u64;
        u64 base = ((end + alignment - 1u64) / alignment) * alignment;
        usize first = registers + declaration.begin;
        usize index = 0u64;
        while (index < offsets.count) {
            address offset = unsafe_address(address_bits(offsets.data) + u64(index) * u64(offsets.stride));
            address place = place_at(places, u64(first + index));
            unsafe_position<ColorPlace>(place).value = base + u64(unsafe_position<u32>(offset));
            unsafe_position<ColorPlace>(place).kind = STACK;
            index += 1u64;
        }
        end = base + u64(declaration.size);
    }
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
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    return space==GraphSpace::Register ? index : core.counter(graph,GraphCounter::Registers)+index;
}
void graph_edge(address graph,u64 first,u64 second,GraphCore core) on cpu 
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    if(first!=second){
        core.relation(graph,GraphRelation::Edge,first,second);
        core.relation(graph,GraphRelation::Edge,second,first);
    }
}
void graph_weight(address graph,u64 id,u64 amount,GraphCore core) on cpu 
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    core.set_weight(graph,id,graph_saturating_add(core.weight(graph,id),amount));
}
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu 
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    u64 id=graph_slot_id(graph,space,index,core);
    if(event.kind==0){
        if((event.skip_present==0 || id!=event.skip) && unsafe_position<ColorPlace>(core.location(graph,id)).width!=0){
            graph_edge(graph,event.anchor,id,core);
        }
    } else if(event.kind==1){
        if(unsafe_position<ColorPlace>(core.location(graph,id)).width!=0){
            core.visit_set(live,ordinal,graph,GraphEvent(id,0u64,0,0));
        }
    } else {core.across(graph,id);}
}
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu 
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,1,0));
}
extern "C" void graph_inputs(address graph,address inputs,u64 count,GraphCore core) on cpu 
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
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
struct GraphDefinition { u64 weight; u64 transfer_dst; u64 transfer_src; u64 bits; u32 transfer; u32 literal; u32 pass; };
void graph_definition(address graph,address live,GraphSpace space,u64 index,GraphDefinition definition,GraphCore core) on cpu
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    if(definition.pass!=0){core.slot(live,GraphSlotAction::Remove,space,index);return;}
    if(space==GraphSpace::Register){record_definition(core.location(graph,index),definition.literal,definition.bits);}
    u64 id=graph_slot_id(graph,space,index,core);
    if(unsafe_position<ColorPlace>(core.location(graph,id)).width!=0){
        GraphEvent event=GraphEvent(id,definition.transfer_src,0,0);
        if(definition.transfer!=0 && id==definition.transfer_dst){event.skip_present=1;}
        core.visit_set(live,18446744073709551615u64,graph,event);
    }
    graph_weight(graph,id,definition.weight,core);
}
extern "C" void graph_read(address graph,address live,GraphSpace space,u64 index,u64 weight,GraphCore core) on cpu
    requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot) {
    u64 id=graph_slot_id(graph,space,index,core);
    graph_weight(graph,id,weight,core);
    core.slot(live,GraphSlotAction::Insert,space,index);
}
Node graph_node_at(Span span, usize index, u32 kind) on cpu {
    return Node(unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride)),kind);
}
bool graph_indexed_reference(Node reference, Host ir) on cpu requires nonzero(ir.inspect) {
    return ir.inspect(ir.inspect(reference).left).tag==STORAGE_INDEXED;
}
bool graph_call_uses_r15(Node call, Host ir) on cpu requires nonzero(ir.inspect) {
    Span arguments=ir.inspect(call).first;
    usize index=0u64;
    while(index<arguments.count){
        View argument=ir.inspect(graph_node_at(arguments,index,N_ARGUMENT));
        if(argument.tag==ARGUMENT_REFERENCE && graph_indexed_reference(argument.left,ir)){return true;}
        index+=1u64;
    }
    return false;
}
extern "C" u32 graph_block(address graph,Span code,Span registers,address live,
    address break_live,address continue_live,address exit_live,u64 weight,GraphHost host) on cpu 
    requires nonzero(host.core.relation) requires nonzero(host.core.weight) requires nonzero(host.core.set_weight)
    requires nonzero(host.core.across) requires nonzero(host.core.counter) requires nonzero(host.core.set_outgoing) requires nonzero(host.core.location)
    requires nonzero(host.core.visit_set) requires nonzero(host.core.slot)
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar)
    requires nonzero(host.cast_bits) requires nonzero(host.call_outgoing) requires nonzero(host.math_outgoing) requires nonzero(host.clone_set)
    requires nonzero(host.drop_set) requires nonzero(host.change) requires nonzero(host.loop_live)
    requires nonzero(host.register_reads) {
    address none=unsafe_address(0u64);
    u32 uses_r15=0;
    u64 remaining=code.count;
    while(remaining!=0u64){
        remaining-=1u64;
        Node instruction=graph_node_at(code,remaining,N_INSTRUCTION);
        View node=host.values.ir.inspect(instruction);
        if(node.tag==OP_BRANCH){
            address yes=host.clone_set(live);
            uses_r15 |= graph_block(graph,node.first,registers,yes,break_live,continue_live,exit_live,weight,host);
            uses_r15 |= graph_block(graph,node.second,registers,live,break_live,continue_live,exit_live,weight,host);
            host.change(GraphSetAction::DrainUnion,live,yes);
            host.drop_set(yes);
            host.core.slot(live,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,weight,host.core);
        } else if(node.tag==OP_LOOP_VALUE){
            address header=host.loop_live(node.first,node.a,node.second,live,break_live,continue_live,exit_live);
            u64 nested=graph_nested_weight(weight);
            address before=host.clone_set(header);
            uses_r15 |= graph_block(graph,node.second,registers,before,live,header,exit_live,nested,host);
            host.change(GraphSetAction::DrainUnion,before,live);
            host.core.slot(before,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,nested,host.core);
            uses_r15 |= graph_block(graph,node.first,registers,before,break_live,continue_live,exit_live,nested,host);
            host.change(GraphSetAction::Swap,live,before);
            host.drop_set(before);
            host.drop_set(header);
        } else if(node.tag==OP_SCOPE){
            address before=host.clone_set(live);
            uses_r15 |= graph_block(graph,node.first,registers,before,break_live,continue_live,live,weight,host);
            host.change(GraphSetAction::Swap,live,before);
            host.drop_set(before);
        } else if(node.tag==OP_RAISE){
            host.change(GraphSetAction::Clear,live,none);
            host.core.slot(live,GraphSlotAction::Insert,GraphSpace::Register,node.a);
            graph_weight(graph,node.a,weight,host.core);
        } else if(node.tag==OP_EXIT_VALUE){host.change(GraphSetAction::Copy,live,exit_live);}
        else if(node.tag==OP_BREAK_VALUE){host.change(GraphSetAction::Copy,live,break_live);}
        else if(node.tag==OP_CONTINUE_VALUE){host.change(GraphSetAction::Copy,live,continue_live);}
        else {
            u64 outgoing=0u64;
            if(node.tag==OP_INVOKE){
                outgoing=host.call_outgoing(node.left.data);
                if(graph_call_uses_r15(node.left,host.values.ir)){uses_r15=1;}
            } else if(node.tag==OP_POSITION_ADDRESS){
                if(graph_indexed_reference(node.left,host.values.ir)){uses_r15=1;}
            } else if(node.tag==OP_TEXTURE_BINDING_ADDRESS || (node.tag>=OP_TEXTURE_SIZE && node.tag<=OP_TEXTURE_FETCH) ||
                node.tag==OP_VIEW_LOAD || node.tag==OP_LOAD_INDEXED || node.tag==OP_STORE_INDEXED || node.tag==OP_ATOMIC){
                uses_r15=1;
            } else if(node.tag==OP_PRINT){outgoing=32u64;}
            else if(node.tag==OP_MATH){outgoing=host.math_outgoing(u32(node.b));}

            u32 transfer=0;
            u64 destination=node.a;
            u64 source=node.b;
            if(node.tag==OP_LOAD_LOCAL){transfer=1;source+=host.core.counter(graph,GraphCounter::Registers);}
            else if(node.tag==OP_STORE_LOCAL){transfer=1;destination+=host.core.counter(graph,GraphCounter::Registers);}
            else if(node.tag==OP_CAST && host.cast_bits(registers,node.b,node.a)!=0){transfer=1;}
            if(transfer!=0 && (unsafe_position<ColorPlace>(host.core.location(graph,destination)).width==0 ||
                unsafe_position<ColorPlace>(host.core.location(graph,source)).width==0)){transfer=0;}
            u32 preferred=transfer;
            u64 preferred_destination=destination;
            u64 preferred_source=source;
            if(preferred==0){
                if(node.tag==OP_UNARY){preferred=1;}
                else if(node.tag==OP_SELECT_VALUE){preferred=1;preferred_source=node.d;}
                else if(node.tag==OP_BINARY){
                    preferred=1;
                    if(node.left.kind==N_COMPARISON){
                        u32 comparison=host.values.ir.inspect(node.left).tag;
                        bool floating=host.values.scalar(registers,node.b).floating!=0;
                        bool reverse=floating ? (comparison==COMPARE_GREATER || comparison==COMPARE_GREATER_EQUAL) :
                            (comparison==COMPARE_LESS || comparison==COMPARE_GREATER_EQUAL);
                        if(reverse){preferred_source=node.c;}
                    }
                } else if(node.tag==OP_MATH && outgoing==0u64){
                    preferred=1;
                    preferred_source=unsafe_position<usize>(node.first.data);
                }
            }
            if(preferred!=0){
                host.core.relation(graph,GraphRelation::Affinity,preferred_destination,preferred_source);
                host.core.relation(graph,GraphRelation::Affinity,preferred_source,preferred_destination);
            }
            u32 literal=node.tag==OP_CONSTANT ? 1 : 0;
            GraphDefinition definition=GraphDefinition(weight,destination,source,u64(node.b),transfer,literal,0);
            if(node.tag==OP_STORE_LOCAL){
                graph_definition(graph,live,GraphSpace::Local,node.a,definition,host.core);
                host.core.slot(live,GraphSlotAction::Remove,GraphSpace::Local,node.a);
            }else{
                Results results=view_results(node,host.values.ir);
                for(pass in 0..2){
                    definition.pass=u32(pass);
                    Results cursor=results;
                    Word next=next_result(cursor);
                    while(next.present!=0){
                        graph_definition(graph,live,GraphSpace::Register,next.value,definition,host.core);
                        next=next_result(cursor);
                    }
                }
            }
            if(node.tag==OP_INVOKE || outgoing!=0u64){
                if(outgoing>host.core.counter(graph,GraphCounter::Outgoing)){host.core.set_outgoing(graph,outgoing);}
                host.core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,2,0));
            }
            host.register_reads(instruction.data,graph,live,weight);
            Locals local_reads=view_locals(node,0,host.values.ir);
            Word next=next_local(local_reads,host.values.ir);
            while(next.present!=0){
                graph_read(graph,live,GraphSpace::Local,next.value,weight,host.core);
                next=next_local(local_reads,host.values.ir);
            }
        }
    }
    return uses_r15;
}
